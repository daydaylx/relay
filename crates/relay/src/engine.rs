//! The change engine: plan (isolated candidate, evaluate, build, diff, risk), apply (drift check,
//! source write, test, health, switch/boot), undo, and crash recovery.
//!
//! Safety properties this module enforces and the tests demonstrate:
//!
//! * planning never touches the live source, the running system or the boot configuration;
//! * every state transition is journaled *before* the action it announces, and recovery works
//!   from live evidence (file hashes, system links) rather than from the journal's optimism;
//! * the only privileged operations are typed adapter calls on an exact, unchanged store path;
//! * switch inhibitors and NixOS' own checks are never bypassed;
//! * a failed activation or health check restores both the source file and the runtime.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::change::{Change, Risk, classify, parse_managed, render_state};
use crate::exec::Runner;
use crate::fsutil::{create_private_dir, write_atomic};
use crate::health::{HealthPolicy, HealthSnapshot, SystemAdapter, validate_unit_name};
use crate::host::{Host, compare_systems};
use crate::hypr::{DesktopProbe, DesktopSnapshot, desktop_problems};
use crate::journal::{ChangeState, JournalEntry};
use crate::nix::{Activation, FlakeSource, NixAdapter, NixError};
use crate::sha256::sha256_hex;
use crate::source::{MANAGED_RELATIVE_PATH, SourceTree};
use crate::state::{PlanRecord, StateDir};

static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Proof that a person (or an explicit `--yes`) approved applying a specific plan.
#[derive(Clone, Copy, Debug)]
pub struct Confirmation(());

impl Confirmation {
    /// Only frontends call this, after showing the plan summary to the user.
    pub fn granted() -> Self {
        Self(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplyOutcome {
    /// Tested, verified healthy and made the active generation.
    Switched,
    /// Prepared as the next boot generation; reboot, then run `relay recover`.
    RebootPending,
    /// Failed or unhealthy; source and runtime were restored.
    RolledBack { reason: String },
    /// Rollback itself could not be completed; manual attention needed.
    RollbackIncomplete { reason: String },
}

impl ApplyOutcome {
    pub fn succeeded(&self) -> bool {
        matches!(self, Self::Switched | Self::RebootPending)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplyReport {
    pub id: String,
    pub outcome: ApplyOutcome,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct InitReport {
    pub path: PathBuf,
    pub created: bool,
    pub tracked_by_git: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct PlanOutcome {
    pub record: PlanRecord,
    pub managed_diff: Vec<String>,
    pub closure_diff: Option<String>,
    /// `false` for MIGRATION_REQUIRED: planned and explained, never activated automatically.
    pub applicable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoverEntry {
    pub id: String,
    pub summary: String,
    pub outcome: Option<ApplyOutcome>,
}

pub struct Engine {
    state: StateDir,
    host: Host,
    nix: NixAdapter,
    system: SystemAdapter,
    runner: Arc<dyn Runner>,
    health: HealthPolicy,
    desktop: Option<Arc<dyn DesktopProbe>>,
    reporter: Arc<dyn Fn(&str) + Send + Sync>,
}

/// A refusal before anything was mutated, with the journal reason code to record.
struct Refusal {
    detail: &'static str,
    message: String,
}

impl Refusal {
    fn new(detail: &'static str, message: impl Into<String>) -> Self {
        Self {
            detail,
            message: message.into(),
        }
    }
}

impl Engine {
    pub fn new(
        state: StateDir,
        host: Host,
        nix: NixAdapter,
        runner: Arc<dyn Runner>,
        health: HealthPolicy,
    ) -> Self {
        Self {
            state,
            host,
            nix,
            system: SystemAdapter::new(Arc::clone(&runner)),
            runner,
            health,
            desktop: None,
            reporter: Arc::new(|_| {}),
        }
    }

    /// Join the desktop (compositor) into the safety loop: a live change that loses monitors,
    /// kills the compositor or introduces configuration errors is rolled back.
    pub fn with_desktop(mut self, probe: Arc<dyn DesktopProbe>) -> Self {
        self.desktop = Some(probe);
        self
    }

    /// Receive one line per workflow step (the CLI prints these to stderr).
    pub fn with_reporter(mut self, reporter: Arc<dyn Fn(&str) + Send + Sync>) -> Self {
        self.reporter = reporter;
        self
    }

    pub fn state(&self) -> &StateDir {
        &self.state
    }

    pub fn host(&self) -> &Host {
        &self.host
    }

    pub fn system(&self) -> &SystemAdapter {
        &self.system
    }

    fn say(&self, message: &str) {
        (self.reporter)(message);
    }

    // ---------------------------------------------------------------- init

    /// Create `relay/managed.nix` (the only file Relay owns). Importing it from the host
    /// configuration stays a manual, one-time step because Relay never edits other files.
    pub fn init(&self, flake: &Path) -> Result<InitReport, String> {
        let tree = SourceTree::scan(flake, &*self.runner)?;
        let path = tree.root().join(MANAGED_RELATIVE_PATH);
        let tracked_by_git = tree
            .is_git_checkout()
            .then(|| tree.contains(MANAGED_RELATIVE_PATH));
        if fs::symlink_metadata(&path).is_ok() {
            let bytes = read_managed(tree.root())?;
            parse_managed(&String::from_utf8_lossy(&bytes))?;
            return Ok(InitReport {
                path,
                created: false,
                tracked_by_git,
            });
        }
        let directory = tree.root().join("relay");
        match fs::symlink_metadata(&directory) {
            Ok(metadata) if !metadata.is_dir() => {
                return Err("'relay' exists in the flake but is not a directory".into());
            }
            Ok(_) => {}
            Err(_) => fs::create_dir(&directory)
                .map_err(|error| format!("could not create the relay directory: {error}"))?,
        }
        write_atomic(&path, render_state(&Default::default()).as_bytes(), 0o644)?;
        Ok(InitReport {
            path,
            created: true,
            tracked_by_git: tree.is_git_checkout().then_some(false),
        })
    }

    // ---------------------------------------------------------------- plan

    /// Build an isolated candidate for `changes` and evaluate everything that can be known
    /// without activating it. The live source, running system and boot entries stay untouched.
    pub fn plan(
        &self,
        flake: &Path,
        host: &str,
        changes: &[Change],
    ) -> Result<PlanOutcome, String> {
        let name_risk = classify(changes)?;
        crate::nix::validate_host(host)?;
        let _lock = self.state.lock()?;
        self.refuse_if_in_flight()?;
        let base_system = self.host.current_system().ok_or_else(|| {
            "cannot determine the running system (/run/current-system)".to_owned()
        })?;

        self.say("scanning the configuration source");
        let tree = SourceTree::scan(flake, &*self.runner)?;
        self.ensure_state_outside(&tree)?;
        if !tree.contains(MANAGED_RELATIVE_PATH) {
            return Err(if tree.is_git_checkout() {
                format!(
                    "{MANAGED_RELATIVE_PATH} is missing or not tracked by git (run `relay init`, then `git add` it)"
                )
            } else {
                format!("{MANAGED_RELATIVE_PATH} does not exist (run `relay init`)")
            });
        }
        let before = read_managed(tree.root())?;
        let before_text = String::from_utf8(before.clone())
            .map_err(|_| "managed.nix is not valid UTF-8".to_owned())?;
        let mut managed = parse_managed(&before_text)?;
        self.check_runtime_matches(&before)?;
        self.say("checking that the whole source is applied to the running system");
        self.check_source_is_applied(flake, host, &base_system)?;
        for change in changes {
            managed.apply(change)?;
        }
        let after_text = render_state(&managed);
        if after_text == before_text {
            return Err(
                "no effect: the managed configuration already contains these changes".into(),
            );
        }
        let source_hash = tree.hash()?;

        let id = new_change_id();
        let journal = self.state.journal();
        journal.begin(&id, &source_hash, &base_system)?;
        let result = self.build_candidate(
            &id,
            &tree,
            host,
            changes,
            name_risk,
            &base_system,
            &before,
            after_text.as_bytes(),
            source_hash,
        );
        match result {
            Ok(outcome) => Ok(outcome),
            Err(refusal) => {
                let _ =
                    journal.transition(&id, ChangeState::Failed, None, None, Some(refusal.detail));
                self.state.remove_candidate(&id);
                Err(refusal.message)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn build_candidate(
        &self,
        id: &str,
        tree: &SourceTree,
        host: &str,
        changes: &[Change],
        name_risk: Risk,
        base_system: &str,
        before: &[u8],
        after: &[u8],
        source_hash: String,
    ) -> Result<PlanOutcome, Refusal> {
        let internal = |message: String| Refusal::new("internal-error", message);
        let candidate_source = self.state.candidate_source(id);
        self.say("creating an isolated candidate copy");
        create_private_dir(&self.state.candidate_dir(id)).map_err(internal)?;
        tree.copy_to(&candidate_source).map_err(internal)?;
        write_atomic(&candidate_source.join(MANAGED_RELATIVE_PATH), after, 0o644)
            .map_err(internal)?;
        let source = FlakeSource::Isolated(&candidate_source);

        self.say("evaluating the candidate");
        match self.nix.imports_managed_module(source, host) {
            Ok(true) => {}
            Ok(false) => {
                return Err(Refusal::new(
                    "managed-module-not-imported",
                    format!(
                        "the host configuration does not import ./{MANAGED_RELATIVE_PATH}; add it to the host's modules (Relay never edits other files)"
                    ),
                ));
            }
            Err(error) => return Err(self.eval_failure(id, error)),
        }
        let candidate_drv = self
            .nix
            .evaluate_toplevel_in(source, host)
            .map_err(|error| self.eval_failure(id, error))?;

        self.say("building the candidate (no activation)");
        let candidate_system = self.nix.build(&candidate_drv).map_err(|error| {
            self.state.save_log(id, "build.log", &error.log);
            Refusal::new(
                "build-failed",
                format!(
                    "{error}; details: {}/build.log",
                    self.state.change_dir(id).display()
                ),
            )
        })?;
        if candidate_system == base_system {
            return Err(Refusal::new(
                "no-effect",
                "the candidate is identical to the running system",
            ));
        }
        if !self.host.is_activatable_system(&candidate_system) {
            return Err(Refusal::new(
                "candidate-not-activatable",
                "the built system has no switch-to-configuration (is system.switch.enable off?)",
            ));
        }

        let evidence = compare_systems(&self.host, base_system, &candidate_system)
            .map_err(|error| Refusal::new("inhibitor-check-failed", error))?;
        let risk = if evidence.requires_reboot() {
            name_risk.max(Risk::RebootRequired)
        } else {
            name_risk
        };

        self.say("comparing closures");
        let closure_diff = self.nix.diff_closures(base_system, &candidate_system).ok();

        let candidate_hash = SourceTree::scan(&candidate_source, &*self.runner)
            .and_then(|tree| tree.hash())
            .map_err(internal)?;
        let record = PlanRecord {
            id: id.to_owned(),
            created: epoch_seconds(),
            flake: tree.root().to_path_buf(),
            host: host.to_owned(),
            changes: changes.to_vec(),
            risk,
            source_hash,
            managed_before_hash: sha256_hex(before),
            managed_after_hash: sha256_hex(after),
            base_system_path: base_system.to_owned(),
            candidate_drv,
            candidate_system_path: candidate_system.clone(),
            candidate_hash: candidate_hash.clone(),
            reboot_components: evidence.reboot_components,
            inhibitors: evidence.inhibitors,
            source_hash_after: None,
            baseline_system_state: None,
            baseline_unhealthy: Vec::new(),
            baseline_monitors: None,
            baseline_config_errors: Vec::new(),
        };
        let managed_diff = line_diff(
            &String::from_utf8_lossy(before),
            &String::from_utf8_lossy(after),
        );
        self.state.save_plan(&record).map_err(internal)?;
        self.state
            .save_file(id, "managed.before.nix", before)
            .map_err(internal)?;
        self.state
            .save_file(id, "managed.after.nix", after)
            .map_err(internal)?;
        if let Some(diff) = &closure_diff {
            self.state
                .save_file(id, "closure-diff.txt", diff.as_bytes())
                .map_err(internal)?;
        }
        self.state
            .journal()
            .transition(
                id,
                ChangeState::Built,
                Some(&candidate_hash),
                Some(&candidate_system),
                None,
            )
            .map_err(internal)?;
        Ok(PlanOutcome {
            applicable: matches!(risk, Risk::LiveSwitchable | Risk::RebootRequired),
            record,
            managed_diff,
            closure_diff,
        })
    }

    fn eval_failure(&self, id: &str, error: NixError) -> Refusal {
        self.state.save_log(id, "eval.log", &error.log);
        Refusal::new(
            "evaluation-failed",
            format!(
                "{error}; details are in {}/eval.log",
                self.state.change_dir(id).display()
            ),
        )
    }

    /// Run `switch-to-configuration dry-activate` for a planned candidate. Preview only.
    pub fn preview(&self, id: &str) -> Result<String, String> {
        let _lock = self.state.lock()?;
        let entry = self.entry(id)?;
        if entry.state != ChangeState::Built {
            return Err(format!(
                "change '{id}' is {}; only built plans can be previewed",
                entry.state.as_str()
            ));
        }
        let record = self.state.load_plan(id)?;
        let outcome = self
            .nix
            .activate(&record.candidate_system_path, Activation::DryActivate)
            .map_err(|error| {
                self.state.save_log(id, "preview.log", &error.log);
                format!(
                    "{error}; details are in {}/preview.log",
                    self.state.change_dir(id).display()
                )
            })?;
        let mut text = outcome.stdout.clone();
        text.extend_from_slice(&outcome.stderr);
        self.state.save_file(id, "preview.txt", &text)?;
        Ok(String::from_utf8_lossy(&text).into_owned())
    }

    /// Abandon a plan that was never applied.
    pub fn discard(&self, id: &str) -> Result<(), String> {
        let _lock = self.state.lock()?;
        let entry = self.entry(id)?;
        if !matches!(entry.state, ChangeState::Planned | ChangeState::Built) {
            return Err(format!(
                "change '{id}' is {} and cannot be discarded",
                entry.state.as_str()
            ));
        }
        self.state
            .journal()
            .transition(id, ChangeState::Failed, None, None, Some("discarded"))?;
        self.state.remove_candidate(id);
        Ok(())
    }

    // --------------------------------------------------------------- apply

    pub fn apply(
        &self,
        id: &str,
        _confirmation: Confirmation,
        expect_active: &[String],
    ) -> Result<ApplyReport, String> {
        for unit in expect_active {
            validate_unit_name(unit)?;
        }
        let _lock = self.state.lock()?;
        let journal = self.state.journal();
        let entries = journal.entries()?;
        let entry = entries
            .get(id)
            .ok_or_else(|| format!("unknown change '{id}'"))?;
        if entry.state != ChangeState::Built {
            return Err(format!(
                "change '{id}' is {} and cannot be applied",
                entry.state.as_str()
            ));
        }
        if let Some(other) = entries
            .values()
            .find(|other| other.id != id && other.state.is_in_flight())
        {
            return Err(unresolved_message(other));
        }
        let mut record = self.state.load_plan(id)?;
        if !matches!(record.risk, Risk::LiveSwitchable | Risk::RebootRequired) {
            return Err(format!(
                "{} changes are planned and explained but never activated automatically",
                record.risk.as_str()
            ));
        }

        self.say("checking source, runtime and candidate against the plan");
        let baseline = match self.preflight(&record) {
            Ok(baseline) => baseline,
            Err(refusal) => {
                let _ =
                    journal.transition(id, ChangeState::Failed, None, None, Some(refusal.detail));
                self.state.remove_candidate(id);
                return Err(refusal.message);
            }
        };
        self.say("previewing the activation (dry-activate)");
        match self
            .nix
            .activate(&record.candidate_system_path, Activation::DryActivate)
        {
            Ok(outcome) => {
                let mut text = outcome.stdout;
                text.extend_from_slice(&outcome.stderr);
                self.state.save_log(id, "preview.txt", &text);
            }
            Err(error) => {
                self.state.save_log(id, "preview.log", &error.log);
                let _ = journal.transition(
                    id,
                    ChangeState::Failed,
                    None,
                    None,
                    Some("dry-activate-failed"),
                );
                self.state.remove_candidate(id);
                return Err(format!("dry-activate failed, nothing was changed; {error}"));
            }
        }
        record.baseline_system_state = Some(baseline.system_state.clone());
        record.baseline_unhealthy = baseline.unhealthy_units.iter().cloned().collect();
        let mut notes = Vec::new();
        if let Some(probe) = &self.desktop {
            match probe.snapshot() {
                Ok(snapshot) => {
                    record.baseline_monitors = Some(snapshot.monitors);
                    record.baseline_config_errors = snapshot.config_errors;
                }
                Err(error) => notes.push(format!("desktop check skipped: {error}")),
            }
        }
        self.state.save_plan(&record)?;

        // From here on the live system may change; every step is journaled before it happens.
        let after = self.state.load_file(id, "managed.after.nix")?;
        journal.transition(id, ChangeState::SourceApplied, None, None, None)?;
        self.say("writing relay/managed.nix");
        if let Err(error) = write_managed(&record.flake, &after) {
            return Ok(self.roll_back(&record, "source-write-failed", Some(error.into_bytes())));
        }
        match SourceTree::scan(&record.flake, &*self.runner).and_then(|tree| tree.hash()) {
            Ok(hash) => {
                record.source_hash_after = Some(hash);
                if let Err(error) = self.state.save_plan(&record) {
                    return Ok(self.roll_back(
                        &record,
                        "state-write-failed",
                        Some(error.into_bytes()),
                    ));
                }
            }
            Err(error) => {
                return Ok(self.roll_back(&record, "source-scan-failed", Some(error.into_bytes())));
            }
        }

        self.say("re-evaluating the live source and checking the candidate identity");
        match self.nix.evaluate_toplevel(&record.flake, &record.host) {
            Ok(drv) if drv == record.candidate_drv => {}
            Ok(_) => return Ok(self.roll_back(&record, "identity-mismatch", None)),
            Err(error) => {
                return Ok(self.roll_back(&record, "re-evaluation-failed", Some(error.log)));
            }
        }

        let mut report = if record.risk == Risk::RebootRequired {
            self.activate_for_reboot(&record)?
        } else {
            self.activate_live(&record, &baseline, expect_active)?
        };
        report.notes.splice(0..0, notes);
        Ok(report)
    }

    fn activate_live(
        &self,
        record: &PlanRecord,
        baseline: &HealthSnapshot,
        expect_active: &[String],
    ) -> Result<ApplyReport, String> {
        let id = record.id.as_str();
        let journal = self.state.journal();
        let candidate = record.candidate_system_path.as_str();
        if let Err(report) = self.enter(record, ChangeState::TestActivated) {
            return Ok(report);
        }
        self.say("activating temporarily (test); this is not a rollback point");
        if let Err(error) = self.nix.activate(candidate, Activation::Test) {
            return Ok(self.roll_back(record, "test-activation-failed", Some(error.log)));
        }
        self.say("checking system health");
        match self.system.verify(baseline, expect_active, self.health) {
            Ok(report) if report.healthy => {}
            Ok(report) => {
                let log = report.problems.join("\n").into_bytes();
                return Ok(self.roll_back(record, "health-check-failed", Some(log)));
            }
            Err(error) => {
                return Ok(self.roll_back(
                    record,
                    "health-check-unavailable",
                    Some(error.into_bytes()),
                ));
            }
        }
        if let (Some(probe), Some(monitors)) = (&self.desktop, record.baseline_monitors) {
            self.say("checking the desktop");
            let baseline = DesktopSnapshot {
                monitors,
                config_errors: record.baseline_config_errors.clone(),
            };
            let problems = desktop_problems(&baseline, probe.as_ref());
            if !problems.is_empty() {
                let log = problems.join("\n").into_bytes();
                return Ok(self.roll_back(record, "desktop-check-failed", Some(log)));
            }
        }
        if let Err(report) = self.enter(record, ChangeState::Verified) {
            return Ok(report);
        }
        self.say("making the candidate the boot default and switching");
        if let Err(error) = self.nix.set_system_profile(candidate) {
            return Ok(self.roll_back(record, "profile-update-failed", Some(error.log)));
        }
        if let Err(error) = self.nix.activate(candidate, Activation::Switch) {
            return Ok(self.roll_back(record, "switch-failed", Some(error.log)));
        }
        let activated = self.host.current_system().as_deref() == Some(candidate)
            && self.host.profile_system().as_deref() == Some(candidate);
        if !activated {
            return Ok(self.roll_back(record, "post-switch-mismatch", None));
        }
        journal.transition(id, ChangeState::Switched, None, None, None)?;
        self.state.remove_candidate(id);
        Ok(ApplyReport {
            id: id.to_owned(),
            outcome: ApplyOutcome::Switched,
            notes: Vec::new(),
        })
    }

    /// Journal a state before the action it announces. If the journal cannot be written the
    /// action must not happen, so the change is rolled back instead.
    fn enter(&self, record: &PlanRecord, state: ChangeState) -> Result<(), ApplyReport> {
        self.state
            .journal()
            .transition(&record.id, state, None, None, None)
            .map(|_| ())
            .map_err(|error| self.roll_back(record, "journal-failed", Some(error.into_bytes())))
    }

    fn activate_for_reboot(&self, record: &PlanRecord) -> Result<ApplyReport, String> {
        let id = record.id.as_str();
        let candidate = record.candidate_system_path.as_str();
        if let Err(report) = self.enter(record, ChangeState::RebootPending) {
            return Ok(report);
        }
        self.say("this change needs a reboot: preparing the next boot generation");
        if let Err(error) = self.nix.set_system_profile(candidate) {
            return Ok(self.roll_back(record, "profile-update-failed", Some(error.log)));
        }
        if let Err(error) = self.nix.activate(candidate, Activation::Boot) {
            return Ok(self.roll_back(record, "boot-preparation-failed", Some(error.log)));
        }
        let mut notes =
            vec!["reboot, then run `relay recover` to verify the new generation".to_owned()];
        if !record.inhibitors.is_empty() {
            notes.push(format!(
                "switch inhibitors prevent a live switch: {}",
                record.inhibitors.join("; ")
            ));
        }
        if !record.reboot_components.is_empty() {
            notes.push(format!(
                "components that change: {}",
                record.reboot_components.join(", ")
            ));
        }
        Ok(ApplyReport {
            id: id.to_owned(),
            outcome: ApplyOutcome::RebootPending,
            notes,
        })
    }

    fn preflight(&self, record: &PlanRecord) -> Result<HealthSnapshot, Refusal> {
        let tree = SourceTree::scan(&record.flake, &*self.runner)
            .map_err(|error| Refusal::new("source-unreadable", error))?;
        let hash = tree
            .hash()
            .map_err(|error| Refusal::new("source-unreadable", error))?;
        if hash != record.source_hash {
            return Err(Refusal::new(
                "source-drift",
                "the configuration source changed since this plan was made; plan again",
            ));
        }
        if self.host.current_system().as_deref() != Some(record.base_system_path.as_str()) {
            return Err(Refusal::new(
                "runtime-drift",
                "the running system changed since this plan was made; plan again",
            ));
        }
        if self.host.profile_system().as_deref() != Some(record.base_system_path.as_str()) {
            return Err(Refusal::new(
                "profile-drift",
                "the system profile does not match the running system (a reboot or earlier change is pending)",
            ));
        }
        let live =
            read_managed(tree.root()).map_err(|error| Refusal::new("source-unreadable", error))?;
        self.check_runtime_matches(&live)
            .map_err(|error| Refusal::new("runtime-drift", error))?;
        if !self
            .host
            .is_activatable_system(&record.candidate_system_path)
        {
            return Err(Refusal::new(
                "candidate-missing",
                "the built candidate is no longer available in the store; plan again",
            ));
        }
        self.check_candidate_integrity(record)?;
        let evidence = compare_systems(
            &self.host,
            &record.base_system_path,
            &record.candidate_system_path,
        )
        .map_err(|error| Refusal::new("inhibitor-check-failed", error))?;
        if evidence.requires_reboot() && record.risk < Risk::RebootRequired {
            return Err(Refusal::new(
                "inhibitors-changed",
                "the candidate now differs in boot-relevant components or switch inhibitors; plan again",
            ));
        }
        self.system.snapshot().map_err(|error| {
            Refusal::new(
                "health-unavailable",
                format!("cannot read the system state: {error}"),
            )
        })
    }

    fn check_candidate_integrity(&self, record: &PlanRecord) -> Result<(), Refusal> {
        let source = self.state.candidate_source(&record.id);
        let broken = |message: &str| Refusal::new("candidate-tampered", message.to_owned());
        let scanned = SourceTree::scan(&source, &*self.runner)
            .and_then(|tree| tree.hash())
            .map_err(|_| broken("the candidate directory is missing; plan again"))?;
        if scanned != record.candidate_hash {
            return Err(broken(
                "the candidate directory was modified after it was built",
            ));
        }
        let candidate_managed = fs::read(source.join(MANAGED_RELATIVE_PATH))
            .map_err(|_| broken("the candidate has no managed module"))?;
        let saved = self
            .state
            .load_file(&record.id, "managed.after.nix")
            .map_err(|_| broken("the saved managed module is missing"))?;
        let expected = &record.managed_after_hash;
        if sha256_hex(&candidate_managed) != *expected || sha256_hex(&saved) != *expected {
            return Err(broken(
                "the managed module no longer matches the built candidate",
            ));
        }
        let before = self
            .state
            .load_file(&record.id, "managed.before.nix")
            .map_err(|_| broken("the saved recovery copy is missing"))?;
        if sha256_hex(&before) != record.managed_before_hash {
            return Err(broken("the saved recovery copy was modified"));
        }
        Ok(())
    }

    // ------------------------------------------------------------ recovery

    /// Restore the source file and the runtime/boot configuration for `record`, recording the
    /// journal transitions. Works from live evidence, so it is safe to run again after a crash.
    fn roll_back(
        &self,
        record: &PlanRecord,
        reason: &'static str,
        log: Option<Vec<u8>>,
    ) -> ApplyReport {
        if let Some(log) = log {
            self.state
                .save_log(&record.id, &format!("{reason}.log"), &log);
        }
        let journal = self.state.journal();
        let id = record.id.as_str();
        let current = journal
            .entries()
            .ok()
            .and_then(|entries| entries.get(id).map(|e| e.state));
        if current != Some(ChangeState::RollbackStarted) {
            // Unlogged rollback would be worse than a failed one: stop and report.
            if let Err(error) =
                journal.transition(id, ChangeState::RollbackStarted, None, None, Some(reason))
            {
                return ApplyReport {
                    id: id.to_owned(),
                    outcome: ApplyOutcome::RollbackIncomplete {
                        reason: format!("{reason}; could not journal the rollback: {error}"),
                    },
                    notes: Vec::new(),
                };
            }
        }
        self.say("rolling back: restoring the source file and the runtime");
        let mut notes = Vec::new();
        let source = self.restore_source(record);
        let runtime = self.restore_runtime(record);
        let mut problems = Vec::new();
        match source {
            Ok(note) => notes.extend(note),
            Err(error) => problems.push(format!("source: {error}")),
        }
        match runtime {
            Ok(note) => notes.extend(note),
            Err(error) => problems.push(format!("runtime: {error}")),
        }
        if problems.is_empty() {
            let _ = journal.transition(id, ChangeState::RolledBack, None, None, Some(reason));
            self.state.remove_candidate(id);
            ApplyReport {
                id: id.to_owned(),
                outcome: ApplyOutcome::RolledBack {
                    reason: reason.to_owned(),
                },
                notes,
            }
        } else {
            let _ = journal.transition(
                id,
                ChangeState::Failed,
                None,
                None,
                Some("rollback-incomplete"),
            );
            notes.extend(problems.clone());
            ApplyReport {
                id: id.to_owned(),
                outcome: ApplyOutcome::RollbackIncomplete {
                    reason: problems.join("; "),
                },
                notes,
            }
        }
    }

    /// Put `relay/managed.nix` back to the saved pre-change content, but only if it is still
    /// exactly what this change wrote; foreign edits are never overwritten.
    fn restore_source(&self, record: &PlanRecord) -> Result<Option<String>, String> {
        let live = read_managed(&record.flake).map(|bytes| sha256_hex(&bytes))?;
        if live == record.managed_before_hash {
            return Ok(None);
        }
        if live != record.managed_after_hash {
            return Err(
                "relay/managed.nix was modified during the change; it was left untouched".into(),
            );
        }
        let before = self.state.load_file(&record.id, "managed.before.nix")?;
        if sha256_hex(&before) != record.managed_before_hash {
            return Err("the saved recovery copy of managed.nix is corrupted".into());
        }
        write_managed(&record.flake, &before)?;
        let restored = read_managed(&record.flake).map(|bytes| sha256_hex(&bytes))?;
        if restored != record.managed_before_hash {
            return Err("managed.nix did not return to its previous content".into());
        }
        Ok(Some("relay/managed.nix restored".to_owned()))
    }

    /// Return the runtime and boot configuration to the previous system. Reboot-class changes
    /// are only ever rolled back through the boot configuration, never by a live switch.
    fn restore_runtime(&self, record: &PlanRecord) -> Result<Option<String>, String> {
        let previous = record.base_system_path.as_str();
        let candidate = record.candidate_system_path.as_str();
        let current = self.host.current_system();
        let profile = self.host.profile_system();
        for (what, value) in [("running system", &current), ("system profile", &profile)] {
            if value.as_deref() != Some(previous) && value.as_deref() != Some(candidate) {
                return Err(format!(
                    "the {what} is neither the previous nor the candidate system; refusing to guess"
                ));
            }
        }
        let profile_changed = profile.as_deref() != Some(previous);
        let reboot_class = record.risk >= Risk::RebootRequired;
        if profile_changed {
            self.restore_step(record, "profile", self.nix.set_system_profile(previous))?;
        }
        let mut note = None;
        if reboot_class {
            if profile_changed {
                self.restore_step(
                    record,
                    "boot",
                    self.nix.activate(previous, Activation::Boot),
                )?;
            }
            if current.as_deref() != Some(previous) {
                note = Some("reboot to finish returning to the previous generation".to_owned());
            }
        } else if profile_changed {
            let action = if current.as_deref() == Some(previous) {
                Activation::Boot
            } else {
                Activation::Switch
            };
            self.restore_step(record, action.as_str(), self.nix.activate(previous, action))?;
        } else if current.as_deref() != Some(previous) {
            self.restore_step(
                record,
                "test",
                self.nix.activate(previous, Activation::Test),
            )?;
        }
        if self.host.profile_system().as_deref() != Some(previous) {
            return Err("the system profile did not return to the previous generation".into());
        }
        if !reboot_class && self.host.current_system().as_deref() != Some(previous) {
            return Err("the running system did not return to the previous generation".into());
        }
        Ok(note)
    }

    /// One privileged step of a rollback; a failure keeps its output in a private log.
    fn restore_step(
        &self,
        record: &PlanRecord,
        label: &str,
        result: Result<crate::exec::Outcome, NixError>,
    ) -> Result<(), String> {
        result.map(|_| ()).map_err(|error| {
            let name = format!("rollback-{label}.log");
            self.state.save_log(&record.id, &name, &error.log);
            format!(
                "{error}; details are in {}/{name}",
                self.state.change_dir(&record.id).display()
            )
        })
    }

    /// The change `undo` would revert: the most recent one that is applied.
    pub fn undo_target(&self) -> Result<String, String> {
        self.state
            .journal()
            .latest_first()?
            .into_iter()
            .find(|entry| entry.state == ChangeState::Switched)
            .map(|entry| entry.id)
            .ok_or_else(|| "there is no applied Relay change to undo".to_owned())
    }

    /// Undo the most recent applied change: source and runtime together.
    pub fn undo(&self, _confirmation: Confirmation) -> Result<ApplyReport, String> {
        let _lock = self.state.lock()?;
        let journal = self.state.journal();
        let entries = journal.latest_first()?;
        if let Some(pending) = entries.iter().find(|entry| entry.state.is_in_flight()) {
            return Err(unresolved_message(pending));
        }
        let target = entries
            .iter()
            .find(|entry| entry.state == ChangeState::Switched)
            .ok_or_else(|| "there is no applied Relay change to undo".to_owned())?;
        let record = self.state.load_plan(&target.id)?;
        let expected_hash = record
            .source_hash_after
            .as_deref()
            .ok_or_else(|| "the change record has no post-apply source identity".to_owned())?;
        let tree = SourceTree::scan(&record.flake, &*self.runner)?;
        if tree.hash()? != expected_hash {
            return Err("the configuration source changed after this change; refusing to undo it automatically".into());
        }
        let candidate = record.candidate_system_path.as_str();
        let running = self.host.current_system();
        let profile = self.host.profile_system();
        if running.as_deref() != Some(candidate) || profile.as_deref() != Some(candidate) {
            return Err("the running system no longer matches this change; refusing to undo it automatically".into());
        }
        self.say(&format!("undoing change {}", record.id));
        let mut report = self.roll_back(&record, "undo", None);
        if matches!(report.outcome, ApplyOutcome::RolledBack { .. }) {
            self.say("checking system health");
            let baseline = HealthSnapshot {
                system_state: record.baseline_system_state.clone().unwrap_or_default(),
                unhealthy_units: record.baseline_unhealthy.iter().cloned().collect(),
            };
            match self.system.verify(&baseline, &[], self.health) {
                Ok(health) if health.healthy => {}
                Ok(health) => report.notes.push(format!(
                    "health check after undo: {}",
                    health.problems.join("; ")
                )),
                Err(error) => report
                    .notes
                    .push(format!("health check after undo unavailable: {error}")),
            }
        }
        Ok(report)
    }

    /// Resolve changes that were interrupted (crash, power loss) or are waiting for a reboot.
    /// Rolling back is the safe direction; a verified candidate is only ever completed forward.
    pub fn recover(&self, abort_pending_reboot: bool) -> Result<Vec<RecoverEntry>, String> {
        let _lock = self.state.lock()?;
        let journal = self.state.journal();
        let mut results = Vec::new();
        let mut entries = journal.entries()?.into_values().collect::<Vec<_>>();
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        for entry in entries {
            match entry.state {
                ChangeState::Planned => {
                    journal.transition(
                        &entry.id,
                        ChangeState::Failed,
                        None,
                        None,
                        Some("abandoned"),
                    )?;
                    self.state.remove_candidate(&entry.id);
                    results.push(RecoverEntry {
                        id: entry.id,
                        summary: "abandoned plan closed".to_owned(),
                        outcome: None,
                    });
                }
                state if state.is_in_flight() => {
                    results.push(self.recover_entry(&entry, abort_pending_reboot)?);
                }
                _ => {}
            }
        }
        Ok(results)
    }

    fn recover_entry(
        &self,
        entry: &JournalEntry,
        abort_pending_reboot: bool,
    ) -> Result<RecoverEntry, String> {
        let journal = self.state.journal();
        let record = self.state.load_plan(&entry.id)?;
        let candidate = record.candidate_system_path.as_str();
        let rolled_back = |report: ApplyReport, summary: &str| RecoverEntry {
            id: report.id.clone(),
            summary: summary.to_owned(),
            outcome: Some(report.outcome),
        };
        match entry.state {
            ChangeState::RebootPending => {
                let booted = self.host.booted_system();
                if booted.as_deref() == Some(candidate) {
                    let baseline = HealthSnapshot {
                        system_state: record.baseline_system_state.clone().unwrap_or_default(),
                        unhealthy_units: record.baseline_unhealthy.iter().cloned().collect(),
                    };
                    match self.system.verify(&baseline, &[], self.health) {
                        Ok(health) if health.healthy => {
                            journal.transition(
                                &entry.id,
                                ChangeState::Verified,
                                None,
                                None,
                                Some("booted"),
                            )?;
                            journal.transition(
                                &entry.id,
                                ChangeState::Switched,
                                None,
                                None,
                                Some("booted"),
                            )?;
                            self.state.remove_candidate(&entry.id);
                            Ok(RecoverEntry {
                                id: entry.id.clone(),
                                summary: "booted into the candidate and verified it".to_owned(),
                                outcome: Some(ApplyOutcome::Switched),
                            })
                        }
                        Ok(health) => {
                            let log = health.problems.join("\n").into_bytes();
                            let report =
                                self.roll_back(&record, "post-boot-health-failed", Some(log));
                            Ok(rolled_back(
                                report,
                                "the booted candidate is unhealthy; rolled back",
                            ))
                        }
                        Err(error) => Ok(RecoverEntry {
                            id: entry.id.clone(),
                            summary: format!("cannot verify the booted candidate yet: {error}"),
                            outcome: None,
                        }),
                    }
                } else if abort_pending_reboot {
                    let report = self.roll_back(&record, "aborted", None);
                    Ok(rolled_back(report, "pending reboot aborted"))
                } else {
                    Ok(RecoverEntry {
                        id: entry.id.clone(),
                        summary: "waiting for a reboot into the candidate (use --abort-pending to cancel)".to_owned(),
                        outcome: None,
                    })
                }
            }
            ChangeState::Verified => {
                let source_applied = read_managed(&record.flake)
                    .map(|bytes| sha256_hex(&bytes) == record.managed_after_hash)
                    .unwrap_or(false);
                if source_applied && self.host.current_system().as_deref() == Some(candidate) {
                    // The candidate was tested and found healthy before the interruption:
                    // finish the remaining steps, but only after checking health again.
                    let baseline = HealthSnapshot {
                        system_state: record.baseline_system_state.clone().unwrap_or_default(),
                        unhealthy_units: record.baseline_unhealthy.iter().cloned().collect(),
                    };
                    let healthy = self
                        .system
                        .verify(&baseline, &[], self.health)
                        .is_ok_and(|report| report.healthy);
                    if healthy && self.complete_verified(&record).is_ok() {
                        journal.transition(
                            &entry.id,
                            ChangeState::Switched,
                            None,
                            None,
                            Some("completed-after-restart"),
                        )?;
                        self.state.remove_candidate(&entry.id);
                        return Ok(RecoverEntry {
                            id: entry.id.clone(),
                            summary: "the verified change was completed after the interruption"
                                .to_owned(),
                            outcome: Some(ApplyOutcome::Switched),
                        });
                    }
                }
                let report = self.roll_back(&record, "interrupted", None);
                Ok(rolled_back(
                    report,
                    "interrupted before completion; rolled back",
                ))
            }
            _ => {
                let report = self.roll_back(&record, "interrupted", None);
                Ok(rolled_back(report, "interrupted; rolled back"))
            }
        }
    }

    /// The tail of a live apply after verification: boot default, then switch. Reboot-class
    /// changes were already prepared through the boot configuration and are never switched.
    fn complete_verified(&self, record: &PlanRecord) -> Result<(), String> {
        let candidate = record.candidate_system_path.as_str();
        if self.host.profile_system().as_deref() != Some(candidate) {
            self.nix
                .set_system_profile(candidate)
                .map_err(|e| e.to_string())?;
        }
        if record.risk < Risk::RebootRequired {
            self.nix
                .activate(candidate, Activation::Switch)
                .map_err(|e| e.to_string())?;
        }
        let done = self.host.current_system().as_deref() == Some(candidate)
            && self.host.profile_system().as_deref() == Some(candidate);
        if done {
            Ok(())
        } else {
            Err("the candidate is not active after completing the change".into())
        }
    }

    // ------------------------------------------------------------- queries

    pub fn history(&self) -> Result<Vec<JournalEntry>, String> {
        self.state.journal().latest_first()
    }

    /// A deterministic, human-readable explanation of a change and how it can be recovered.
    pub fn explain(&self, id: &str) -> Result<String, String> {
        let entry = self.entry(id)?;
        let record = self.state.load_plan(id)?;
        let mut lines = vec![
            format!("Change {id}: {}", entry.state.as_str()),
            format!("Host {} in {}", record.host, record.flake.display()),
            "Intent:".to_owned(),
        ];
        for change in &record.changes {
            lines.push(format!("  - {}", describe_change(change)));
        }
        lines.push(format!("Risk: {}", record.risk.as_str()));
        lines.push(match record.risk {
            Risk::LiveSwitchable => "  It can be tested live, health-checked and then switched.".to_owned(),
            Risk::RebootRequired => "  It takes effect on the next boot; Relay prepares the boot generation and verifies it after the reboot.".to_owned(),
            Risk::MigrationRequired => "  It may migrate stateful data, so Relay plans it but never activates it automatically.".to_owned(),
            Risk::Protected => "  It targets a protected resource and is blocked.".to_owned(),
        });
        for component in &record.reboot_components {
            lines.push(format!("  The system component '{component}' changes."));
        }
        for inhibitor in &record.inhibitors {
            lines.push(format!("  Switch inhibitor: {inhibitor} (never bypassed)"));
        }
        lines.push(format!("Previous system: {}", record.base_system_path));
        lines.push(format!(
            "Candidate system: {}",
            record.candidate_system_path
        ));
        if let Some(detail) = &entry.detail {
            lines.push(format!("Last reason code: {detail}"));
        }
        lines.push("Recovery:".to_owned());
        lines.push(
            "  Source: relay/managed.nix is restored from the saved pre-change copy.".to_owned(),
        );
        lines.push(format!(
            "  Runtime: the system returns to {}.",
            record.base_system_path
        ));
        lines.push(match entry.state {
            ChangeState::Built => format!("Next: `relay apply {id}` (or `relay discard {id}`)."),
            ChangeState::Switched => {
                "Undo with `relay undo` while it is the latest change.".to_owned()
            }
            state if state.is_in_flight() => "Resolve with `relay recover`.".to_owned(),
            _ => "No further action.".to_owned(),
        });
        Ok(lines.join("\n"))
    }

    /// What a person should read before confirming: the explanation, the exact change to
    /// `relay/managed.nix`, and the closure diff.
    pub fn review(&self, id: &str) -> Result<String, String> {
        let mut text = self.explain(id)?;
        let before = self.state.load_file(id, "managed.before.nix")?;
        let after = self.state.load_file(id, "managed.after.nix")?;
        text.push_str("\n\nChanges to relay/managed.nix:\n");
        for line in line_diff(
            &String::from_utf8_lossy(&before),
            &String::from_utf8_lossy(&after),
        ) {
            text.push_str("  ");
            text.push_str(&line);
            text.push('\n');
        }
        if let Ok(diff) = self.state.load_file(id, "closure-diff.txt") {
            text.push_str("\nClosure diff (display only):\n");
            for line in String::from_utf8_lossy(&diff).lines() {
                text.push_str("  ");
                text.push_str(line);
                text.push('\n');
            }
        }
        Ok(text)
    }

    // ------------------------------------------------------------- helpers

    fn entry(&self, id: &str) -> Result<JournalEntry, String> {
        self.state
            .journal()
            .entries()?
            .remove(id)
            .ok_or_else(|| format!("unknown change '{id}'"))
    }

    fn refuse_if_in_flight(&self) -> Result<(), String> {
        match self
            .state
            .journal()
            .entries()?
            .values()
            .find(|entry| entry.state.is_in_flight())
        {
            Some(entry) => Err(unresolved_message(entry)),
            None => Ok(()),
        }
    }

    /// Relay may only change a system whose source is fully applied: if the live source already
    /// differs from what is running (unapplied edits anywhere in the configuration), a candidate
    /// would silently activate those edits too, and `undo` could not restore a coherent state.
    fn check_source_is_applied(
        &self,
        flake: &Path,
        host: &str,
        running: &str,
    ) -> Result<(), String> {
        let evaluated = self
            .nix
            .evaluate_toplevel_output(FlakeSource::Live(flake), host)
            .map_err(|error| format!("could not evaluate the live configuration: {error}"))?;
        if evaluated == running {
            Ok(())
        } else {
            Err("the configuration source has changes that are not applied to the running system (anywhere in the configuration, not only in relay/managed.nix); a Relay change would activate them too. Apply them first (for example with nixos-rebuild switch) or revert them".into())
        }
    }

    /// The running system must have been built from the source we are about to change.
    fn check_runtime_matches(&self, live_managed: &[u8]) -> Result<(), String> {
        match self.host.runtime_managed_module() {
            Some(runtime) if runtime == live_managed => Ok(()),
            Some(_) => Err("the running system was built from a different relay/managed.nix than the source (rebuild or restore it first)".into()),
            None => Err("the running system does not publish /etc/relay/managed.nix; import ./relay/managed.nix in the host configuration and rebuild once".into()),
        }
    }

    fn ensure_state_outside(&self, tree: &SourceTree) -> Result<(), String> {
        let state = self
            .state
            .root()
            .canonicalize()
            .map_err(|error| format!("state directory is not accessible: {error}"))?;
        if state.starts_with(tree.root()) {
            return Err(
                "the Relay state directory must not be inside the configuration source".into(),
            );
        }
        Ok(())
    }
}

fn unresolved_message(entry: &JournalEntry) -> String {
    format!(
        "change {} is {} and unresolved; run `relay recover` first",
        entry.id,
        entry.state.as_str()
    )
}

/// Read `relay/managed.nix`, refusing symlinks so the write boundary cannot be redirected.
fn read_managed(flake: &Path) -> Result<Vec<u8>, String> {
    let path = managed_path(flake)?;
    fs::read(&path).map_err(|error| format!("could not read {MANAGED_RELATIVE_PATH}: {error}"))
}

fn write_managed(flake: &Path, contents: &[u8]) -> Result<(), String> {
    let path = managed_path(flake)?;
    write_atomic(&path, contents, 0o644)
}

fn managed_path(flake: &Path) -> Result<PathBuf, String> {
    let directory = flake.join("relay");
    let directory_ok = fs::symlink_metadata(&directory).is_ok_and(|metadata| metadata.is_dir());
    let path = flake.join(MANAGED_RELATIVE_PATH);
    let file_ok = fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_file());
    if !directory_ok || !file_ok {
        return Err(format!(
            "{MANAGED_RELATIVE_PATH} must be a regular file in a real directory (run `relay init`)"
        ));
    }
    Ok(path)
}

fn new_change_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let counter = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "chg-{millis:013}-{:05}-{counter:03}",
        std::process::id() % 100_000
    )
}

fn epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn line_diff(before: &str, after: &str) -> Vec<String> {
    let mut remaining: Vec<&str> = after.lines().collect();
    let mut diff = Vec::new();
    for line in before.lines() {
        match remaining.iter().position(|candidate| *candidate == line) {
            Some(index) => {
                remaining.remove(index);
            }
            None => diff.push(format!("- {line}")),
        }
    }
    diff.extend(remaining.into_iter().map(|line| format!("+ {line}")));
    diff
}

fn describe_change(change: &Change) -> String {
    use crate::change::Value;
    match change {
        Change::AddPackage { name } => format!("add package {name}"),
        Change::RemovePackage { name } => format!("remove package {name}"),
        Change::SetOption { name, value } => {
            let shown = match value {
                Value::Bool(value) => value.to_string(),
                Value::Integer(value) => value.to_string(),
                Value::String(_) => "\"…\"".to_owned(),
                Value::StringList(values) => format!("[{} item(s)]", values.len()),
            };
            format!("set {name} = {shown}")
        }
    }
}

#[cfg(test)]
mod tests;
