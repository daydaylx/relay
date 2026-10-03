//! Safety-property tests for the change engine, run against a simulated NixOS machine.
//!
//! The simulator implements [`Runner`]: it answers Nix, systemd and privileged activation
//! commands against a fixture filesystem, so `run/current-system`, the system profile, the
//! booted system and `/etc/relay/managed.nix` behave like the real thing. It also enforces
//! NixOS' own switch-inhibitor rule, so a Relay bug that tried to bypass it would fail here.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use super::{ApplyOutcome, Confirmation, Engine};
use crate::change::{Change, Risk, Value, render_state};
use crate::exec::{Invocation, Outcome, ProcessRunner, Runner};
use crate::fsutil::testutil::TempDir;
use crate::health::HealthPolicy;
use crate::host::{Host, compare_systems, fixture};
use crate::intent::parse_intent;
use crate::journal::ChangeState;
use crate::nix::NixAdapter;
use crate::sha256::sha256_hex;
use crate::source::SourceTree;
use crate::state::StateDir;

const BASE: &str = "/nix/store/00000000000000000000000000000000-nixos-system-host";
const SECRET_STDERR: &str = "error: while evaluating 'x': password = hunter2-SECRET";

// ------------------------------------------------------------------ simulator

#[derive(Default)]
struct SimState {
    /// Every invocation: (program, arguments, privileged).
    calls: Vec<(String, Vec<String>, bool)>,
    /// Labels that fail: eval, build, dry-activate, test, switch, boot, set-profile, eval-live.
    fail: BTreeSet<&'static str>,
    /// Labels that fail only the first time they are invoked.
    fail_once: BTreeSet<&'static str>,
    /// `(label, nth)`: panic before/after the nth privileged or Nix call with this label.
    crash_before: Vec<(&'static str, usize)>,
    crash_after: Vec<(&'static str, usize)>,
    counts: BTreeMap<&'static str, usize>,
    /// Properties of systems built from now on.
    next_kernel: Option<String>,
    next_inhibitors: Option<String>,
    imports_managed: bool,
    /// Any running system other than BASE has a failed unit.
    unhealthy_when_changed: bool,
    preexisting_failure: bool,
    active_units: BTreeSet<String>,
    profile_generation: u32,
    /// Overwrite this file when `test` is invoked (a concurrent foreign edit).
    edit_on_test: Option<(PathBuf, String)>,
    /// Make the live re-evaluation return a different derivation.
    reeval_returns_other_drv: bool,
    /// Which system a source tree (first 32 hex of its hash) was built into.
    tree_systems: BTreeMap<String, String>,
}

struct Sim {
    host: Host,
    state: Mutex<SimState>,
}

impl Sim {
    fn lock(&self) -> MutexGuard<'_, SimState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn privileged_log(&self) -> Vec<String> {
        self.lock()
            .calls
            .iter()
            .filter(|(_, _, privileged)| *privileged)
            .map(|(program, args, _)| {
                if program == "nix-env" {
                    format!("set-profile {}", args.last().unwrap())
                } else {
                    format!(
                        "{} {}",
                        args[0],
                        program.trim_end_matches("/bin/switch-to-configuration")
                    )
                }
            })
            .collect()
    }

    fn actions(&self) -> Vec<String> {
        self.privileged_log()
            .into_iter()
            .map(|entry| entry.split(' ').next().unwrap().to_owned())
            .collect()
    }

    fn clear_calls(&self) {
        self.lock().calls.clear();
    }

    fn fail(&self, label: &'static str) {
        self.lock().fail.insert(label);
    }

    fn fail_once(&self, label: &'static str) {
        self.lock().fail_once.insert(label);
    }

    fn crash_before(&self, label: &'static str, nth: usize) {
        self.lock().crash_before.push((label, nth));
    }

    fn crash_after(&self, label: &'static str, nth: usize) {
        self.lock().crash_after.push((label, nth));
    }

    /// A simulated reboot: boot whatever the system profile points at.
    fn reboot(&self) {
        let profile = self.host.profile_system().expect("profile");
        self.activate(&profile);
        fixture::point(&self.host, "run/booted-system", &profile);
    }

    fn activate(&self, system: &str) {
        let stamp = self
            .host
            .root()
            .join(system.trim_start_matches('/'))
            .join("etc-relay-managed.nix");
        let target = self.host.root().join("etc/relay/managed.nix");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(stamp, target).unwrap();
        fixture::point(&self.host, "run/current-system", system);
    }

    fn failed(&self, label: &'static str) -> bool {
        let mut state = self.lock();
        state.fail.contains(label) || state.fail_once.remove(label)
    }

    fn crash_check(&self, before: bool, label: &'static str, count: usize) {
        let hit = {
            let state = self.lock();
            let list = if before {
                &state.crash_before
            } else {
                &state.crash_after
            };
            list.iter().any(|(l, nth)| *l == label && *nth == count)
        };
        if hit {
            panic!(
                "simulated crash {} {label}",
                if before { "before" } else { "after" }
            );
        }
    }

    fn step(&self, label: &'static str) -> usize {
        let mut state = self.lock();
        let count = state.counts.entry(label).or_insert(0);
        *count += 1;
        *count
    }

    fn ok(stdout: &str) -> Result<Outcome, String> {
        Ok(Outcome {
            code: Some(0),
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        })
    }

    fn failure() -> Result<Outcome, String> {
        Ok(Outcome {
            code: Some(1),
            stdout: Vec::new(),
            stderr: SECRET_STDERR.as_bytes().to_vec(),
        })
    }

    fn tree_hash(&self, reference: &str) -> String {
        let path = reference
            .split('#')
            .next()
            .unwrap()
            .trim_start_matches("path:");
        SourceTree::scan(std::path::Path::new(path), &ProcessRunner::default())
            .unwrap()
            .hash()
            .unwrap()
    }

    fn eval(&self, args: &[String]) -> Result<Outcome, String> {
        let reference = args
            .iter()
            .find(|arg| arg.contains("#nixosConfigurations."))
            .unwrap();
        let isolated = reference.starts_with("path:");
        if reference.ends_with(".config.environment.etc") {
            return Self::ok(if self.lock().imports_managed {
                "true"
            } else {
                "false"
            });
        }
        if reference.ends_with(".config.system.build.toplevel.outPath") {
            // What the source would build to: the system it was built into, or a stranger.
            let hash = self.tree_hash(reference);
            let key = hash[..32].to_owned();
            let system = self.lock().tree_systems.get(&key).cloned();
            return Self::ok(
                &system.unwrap_or_else(|| format!("/nix/store/{key}-nixos-system-host")),
            );
        }
        let label = if isolated { "eval" } else { "eval-live" };
        let count = self.step(label);
        self.crash_check(true, label, count);
        if self.failed(label) {
            return Self::failure();
        }
        let hash = self.tree_hash(reference);
        let hash = if !isolated && self.lock().reeval_returns_other_drv {
            sha256_hex(hash.as_bytes())
        } else {
            hash
        };
        Self::ok(&format!("/nix/store/{}-nixos-system-host.drv", &hash[..32]))
    }

    fn build(&self, args: &[String]) -> Result<Outcome, String> {
        if self.failed("build") {
            return Self::failure();
        }
        let drv = args.last().unwrap().trim_end_matches("^out");
        let name = drv.trim_end_matches(".drv");
        // The derivation name encodes the content hash of the evaluated source.
        let hash = name
            .trim_start_matches("/nix/store/")
            .split('-')
            .next()
            .unwrap();
        let system = format!("/nix/store/{hash}-nixos-system-host");
        self.lock()
            .tree_systems
            .insert(hash.to_owned(), system.clone());
        // Find the candidate directory with this identity to embed its managed module.
        let (kernel, inhibitors) = {
            let state = self.lock();
            (
                state
                    .next_kernel
                    .clone()
                    .unwrap_or_else(|| "/nix/store/kernel-1".into()),
                state.next_inhibitors.clone().unwrap_or_else(|| "{}".into()),
            )
        };
        fixture::make_system(&self.host, &system, &kernel, &inhibitors);
        let managed = self.managed_for_hash(hash);
        fs::write(
            self.host
                .root()
                .join(system.trim_start_matches('/'))
                .join("etc-relay-managed.nix"),
            managed,
        )
        .unwrap();
        Self::ok(&system)
    }

    /// The managed module of the candidate tree whose content hash starts with `hash`.
    fn managed_for_hash(&self, hash: &str) -> String {
        let candidates = self.host.root().join("state/candidates");
        for entry in fs::read_dir(&candidates).into_iter().flatten().flatten() {
            let source = entry.path().join("src");
            if source.exists() && self.tree_hash(&source.to_string_lossy()).starts_with(hash) {
                return fs::read_to_string(source.join("relay/managed.nix")).unwrap();
            }
        }
        panic!("no candidate with identity {hash}");
    }

    fn systemctl(&self, args: &[String]) -> Result<Outcome, String> {
        match args[0].as_str() {
            "is-system-running" => Ok(Outcome {
                code: Some(0),
                stdout: b"running".to_vec(),
                stderr: Vec::new(),
            }),
            "list-units" => {
                let state = self.lock();
                let changed = self.host.current_system().as_deref() != Some(BASE);
                let mut units =
                    vec![r#"{"unit":"ok.service","active":"active","sub":"running"}"#.to_owned()];
                if state.preexisting_failure {
                    units.push(
                        r#"{"unit":"old.service","active":"failed","sub":"failed"}"#.to_owned(),
                    );
                }
                if state.unhealthy_when_changed && changed {
                    units.push(
                        r#"{"unit":"broken.service","active":"failed","sub":"failed"}"#.to_owned(),
                    );
                }
                Self::ok(&format!("[{}]", units.join(",")))
            }
            "is-active" => {
                let unit = args.last().unwrap();
                let active = self.lock().active_units.contains(unit);
                Ok(Outcome {
                    code: Some(i32::from(!active)),
                    ..Outcome::default()
                })
            }
            other => panic!("unexpected systemctl {other}"),
        }
    }

    fn set_profile(&self, args: &[String]) -> Result<Outcome, String> {
        let count = self.step("set-profile");
        self.crash_check(true, "set-profile", count);
        if self.failed("set-profile") {
            return Self::failure();
        }
        let system = args.last().unwrap();
        let generation = {
            let mut state = self.lock();
            state.profile_generation += 1;
            state.profile_generation
        };
        fixture::point(
            &self.host,
            &format!("nix/var/nix/profiles/system-{generation}-link"),
            system,
        );
        fixture::point(
            &self.host,
            "nix/var/nix/profiles/system",
            &format!("system-{generation}-link"),
        );
        self.crash_check(false, "set-profile", count);
        Self::ok("")
    }

    fn switch_to_configuration(&self, system: &str, action: &str) -> Result<Outcome, String> {
        let label: &'static str = match action {
            "dry-activate" => "dry-activate",
            "test" => "test",
            "switch" => "switch",
            "boot" => "boot",
            other => panic!("unexpected action {other}"),
        };
        let count = self.step(label);
        self.crash_check(true, label, count);
        if label == "test" {
            let edit = self.lock().edit_on_test.clone();
            if let Some((path, content)) = edit {
                fs::write(path, content).unwrap();
            }
        }
        if self.failed(label) {
            return Self::failure();
        }
        // NixOS' pre-switch check: inhibitors block `test` and `switch`, never `boot`.
        if matches!(label, "test" | "switch") {
            let current = self.host.current_system().unwrap();
            let evidence = compare_systems(&self.host, &current, system).unwrap();
            if !evidence.inhibitors.is_empty() {
                return Ok(Outcome {
                    code: Some(1),
                    stdout: b"Switching into this system is not recommended.".to_vec(),
                    stderr: Vec::new(),
                });
            }
        }
        match label {
            "test" | "switch" => self.activate(system),
            _ => {}
        }
        self.crash_check(false, label, count);
        Self::ok("would restart ok.service")
    }
}

impl Runner for Sim {
    fn run(&self, invocation: &Invocation) -> Result<Outcome, String> {
        assert!(
            invocation
                .environment()
                .iter()
                .all(|(key, _)| key != "NIXOS_NO_CHECK"),
            "switch inhibitors must never be bypassed"
        );
        self.lock().calls.push((
            invocation.program().to_owned(),
            invocation.arguments().to_vec(),
            invocation.is_privileged(),
        ));
        let args = invocation.arguments();
        match invocation.program() {
            "nix" => {
                let verb = args
                    .iter()
                    .find(|arg| !arg.starts_with("--") && *arg != "nix-command flakes")
                    .unwrap();
                match verb.as_str() {
                    "eval" => self.eval(args),
                    "build" => self.build(args),
                    "store" => Self::ok("vlc: ∅ → 3.0.21, +61.5 MiB"),
                    other => panic!("unexpected nix {other}"),
                }
            }
            "systemctl" => self.systemctl(args),
            "nix-env" => self.set_profile(args),
            program if program.ends_with("/bin/switch-to-configuration") => {
                assert!(invocation.is_privileged());
                self.switch_to_configuration(
                    program.trim_end_matches("/bin/switch-to-configuration"),
                    &args[0],
                )
            }
            other => panic!("unexpected program {other}"),
        }
    }
}

// --------------------------------------------------------------- environment

struct Env {
    _dir: TempDir,
    flake: PathBuf,
    sim: Arc<Sim>,
    host: Host,
    state_dir: PathBuf,
}

impl Env {
    fn new() -> Self {
        let dir = TempDir::new("engine");
        let root = dir.path().join("host");
        let flake = dir.path().join("flake");
        let state_dir = root.join("state");
        fs::create_dir_all(flake.join("relay")).unwrap();
        fs::write(flake.join("flake.nix"), "{ outputs = _: {}; }\n").unwrap();
        fs::write(flake.join("configuration.nix"), "{ }\n").unwrap();
        fs::write(flake.join("flake.lock"), "{}\n").unwrap();
        let initial = render_state(&Default::default());
        fs::write(flake.join("relay/managed.nix"), &initial).unwrap();

        let host = Host::new(&root);
        fixture::make_system(&host, BASE, "/nix/store/kernel-1", "{}");
        fs::write(
            root.join(BASE.trim_start_matches('/'))
                .join("etc-relay-managed.nix"),
            &initial,
        )
        .unwrap();
        fixture::point(&host, "nix/var/nix/profiles/system-1-link", BASE);
        fixture::point(&host, "nix/var/nix/profiles/system", "system-1-link");
        fixture::point(&host, "run/booted-system", BASE);
        let sim = Arc::new(Sim {
            host: host.clone(),
            state: Mutex::new(SimState {
                imports_managed: true,
                profile_generation: 1,
                ..SimState::default()
            }),
        });
        sim.activate(BASE);
        let env = Self {
            flake: flake.canonicalize().unwrap(),
            _dir: dir,
            sim,
            host,
            state_dir,
        };
        // Everything above is setup, not behaviour under test.
        let initial = env.live_hash();
        env.sim
            .lock()
            .tree_systems
            .insert(initial[..32].to_owned(), BASE.to_owned());
        env.sim.clear_calls();
        env
    }

    fn engine(&self) -> Engine {
        Engine::new(
            StateDir::new(&self.state_dir),
            self.host.clone(),
            NixAdapter::with_runner("nix", self.sim.clone()),
            self.sim.clone(),
            HealthPolicy {
                settle_timeout: Duration::ZERO,
                poll_interval: Duration::ZERO,
                observe: Duration::ZERO,
            },
        )
    }

    fn managed(&self) -> String {
        fs::read_to_string(self.flake.join("relay/managed.nix")).unwrap()
    }

    fn live_hash(&self) -> String {
        SourceTree::scan(&self.flake, &ProcessRunner::default())
            .unwrap()
            .hash()
            .unwrap()
    }

    fn current(&self) -> String {
        self.host.current_system().unwrap()
    }

    fn profile(&self) -> String {
        self.host.profile_system().unwrap()
    }

    fn states(&self) -> BTreeMap<String, ChangeState> {
        self.engine()
            .state()
            .journal()
            .entries()
            .unwrap()
            .into_iter()
            .map(|(id, entry)| (id, entry.state))
            .collect()
    }

    fn entry(&self, id: &str) -> crate::journal::JournalEntry {
        self.engine()
            .state()
            .journal()
            .entries()
            .unwrap()
            .remove(id)
            .unwrap()
    }

    fn plan(&self, changes: &[Change]) -> Result<super::PlanOutcome, String> {
        self.engine().plan(&self.flake, "host", changes)
    }

    fn apply(&self, id: &str) -> Result<super::ApplyReport, String> {
        self.engine().apply(id, Confirmation::granted(), &[])
    }

    fn plan_and_apply(&self, changes: &[Change]) -> super::ApplyReport {
        let plan = self.plan(changes).unwrap();
        self.apply(&plan.record.id).unwrap()
    }

    fn candidate_exists(&self, id: &str) -> bool {
        self.engine().state().candidate_dir(id).exists()
    }
}

fn bluetooth() -> Vec<Change> {
    vec![Change::SetOption {
        name: "hardware.bluetooth.enable".into(),
        value: Value::Bool(true),
    }]
}

fn vlc(add: bool) -> Vec<Change> {
    vec![if add {
        Change::AddPackage { name: "vlc".into() }
    } else {
        Change::RemovePackage { name: "vlc".into() }
    }]
}

fn rolled_back_reason(report: &super::ApplyReport) -> &str {
    match &report.outcome {
        ApplyOutcome::RolledBack { reason } => reason,
        other => panic!("expected a rollback, got {other:?}"),
    }
}

// ----------------------------------------------------------- init and planning

#[test]
fn init_creates_only_the_managed_module_and_is_idempotent() {
    let env = Env::new();
    fs::remove_file(env.flake.join("relay/managed.nix")).unwrap();
    fs::remove_dir(env.flake.join("relay")).unwrap();
    let before = env.live_hash();
    let report = env.engine().init(&env.flake).unwrap();
    assert!(report.created);
    assert_eq!(env.managed(), render_state(&Default::default()));
    assert_ne!(before, env.live_hash());
    let again = env.engine().init(&env.flake).unwrap();
    assert!(!again.created);
    // Only relay/managed.nix was added; nothing else in the source changed.
    let files = SourceTree::scan(&env.flake, &ProcessRunner::default()).unwrap();
    for other in ["flake.nix", "configuration.nix", "flake.lock"] {
        assert!(files.contains(other));
    }
    // A hand-written file is never overwritten or adopted.
    fs::write(env.flake.join("relay/managed.nix"), "{ hand = 1; }\n").unwrap();
    assert!(env.engine().init(&env.flake).is_err());
    assert_eq!(env.managed(), "{ hand = 1; }\n");
}

#[test]
fn plan_builds_an_isolated_candidate_and_leaves_the_live_system_untouched() {
    let env = Env::new();
    let live_before = env.live_hash();
    let managed_before = env.managed();

    let plan = env.plan(&bluetooth()).unwrap();

    // Candidate isolation: source, runtime, profile and boot state are exactly as before.
    assert_eq!(env.live_hash(), live_before);
    assert_eq!(env.managed(), managed_before);
    assert_eq!(env.current(), BASE);
    assert_eq!(env.profile(), BASE);
    assert!(
        env.sim.privileged_log().is_empty(),
        "planning must not activate anything"
    );
    // The candidate is a separate tree that carries the change.
    let candidate = env.engine().state().candidate_source(&plan.record.id);
    let candidate_managed = fs::read_to_string(candidate.join("relay/managed.nix")).unwrap();
    assert!(candidate_managed.contains("hardware.bluetooth.enable = true;"));
    assert!(!candidate.starts_with(&env.flake));
    // The plan is complete: risk, candidate store path, closure diff, explicit managed diff.
    assert_eq!(plan.record.risk, Risk::LiveSwitchable);
    assert!(plan.applicable);
    assert_ne!(plan.record.candidate_system_path, BASE);
    assert!(plan.closure_diff.unwrap().contains("vlc"));
    assert_eq!(plan.managed_diff, ["+   hardware.bluetooth.enable = true;"]);
    assert_eq!(plan.record.base_system_path, BASE);
    assert_eq!(env.states()[&plan.record.id], ChangeState::Built);
    // Nothing privileged and nothing outside nix queries happened.
    assert!(
        env.sim
            .lock()
            .calls
            .iter()
            .all(|(program, _, privileged)| program == "nix" && !privileged)
    );
}

#[test]
fn failed_evaluation_fails_the_plan_without_touching_anything_and_keeps_stderr_private() {
    let env = Env::new();
    env.sim.fail("eval");
    let live_before = env.live_hash();
    let error = env.plan(&bluetooth()).unwrap_err();
    assert!(error.contains("eval.log"));
    assert!(
        !error.contains("hunter2"),
        "evaluated values must not reach normal output"
    );
    let (id, state) = env.states().into_iter().next().unwrap();
    assert_eq!(state, ChangeState::Failed);
    assert_eq!(env.entry(&id).detail.as_deref(), Some("evaluation-failed"));
    assert!(!env.candidate_exists(&id));
    let log = fs::read_to_string(env.engine().state().change_dir(&id).join("eval.log")).unwrap();
    assert!(
        log.contains("hunter2"),
        "diagnostics live in the private log"
    );
    assert_eq!(env.live_hash(), live_before);
    assert!(env.sim.privileged_log().is_empty());
}

#[test]
fn failed_build_fails_the_plan_and_removes_the_candidate() {
    let env = Env::new();
    env.sim.fail("build");
    let error = env.plan(&vlc(true)).unwrap_err();
    assert!(error.contains("build.log") && !error.contains("hunter2"));
    let (id, state) = env.states().into_iter().next().unwrap();
    assert_eq!(state, ChangeState::Failed);
    assert_eq!(env.entry(&id).detail.as_deref(), Some("build-failed"));
    assert!(!env.candidate_exists(&id));
    assert_eq!(env.current(), BASE);
}

#[test]
fn a_host_that_does_not_import_the_managed_module_is_refused() {
    let env = Env::new();
    env.sim.lock().imports_managed = false;
    let error = env.plan(&bluetooth()).unwrap_err();
    assert!(error.contains("does not import"));
    let (id, _) = env.states().into_iter().next().unwrap();
    assert_eq!(
        env.entry(&id).detail.as_deref(),
        Some("managed-module-not-imported")
    );
}

#[test]
fn protected_resources_are_rejected_before_anything_is_created() {
    let env = Env::new();
    for name in [
        "system.stateVersion",
        "boot.loader.systemd-boot.enable",
        "nix.settings.trusted-users",
        "services.openssh.enable",
        "users.users.root.hashedPassword",
    ] {
        let error = env
            .plan(&[Change::SetOption {
                name: name.into(),
                value: Value::Bool(true),
            }])
            .unwrap_err();
        assert!(error.contains("protected"), "{name}: {error}");
    }
    assert!(
        env.states().is_empty(),
        "rejected intents leave no journal entries"
    );
    assert!(
        env.sim.lock().calls.is_empty(),
        "rejection happens before any Nix call"
    );
}

#[test]
fn changes_without_effect_and_unsupported_removals_are_refused() {
    let env = Env::new();
    assert!(
        env.plan(&vlc(false))
            .unwrap_err()
            .contains("managed baseline")
    );
    let applied = env.plan_and_apply(&vlc(true));
    assert_eq!(applied.outcome, ApplyOutcome::Switched);
    assert!(env.plan(&vlc(true)).unwrap_err().contains("no effect"));
}

#[test]
fn planning_refuses_when_runtime_source_or_managed_module_disagree() {
    let env = Env::new();
    // Hand-edited managed module.
    fs::write(
        env.flake.join("relay/managed.nix"),
        format!("{}# edit\n", env.managed()),
    )
    .unwrap();
    assert!(
        env.plan(&bluetooth())
            .unwrap_err()
            .contains("canonical form")
    );
    // Source differs from what the running system was built from.
    fs::write(
        env.flake.join("relay/managed.nix"),
        render_state(&{
            let mut state = crate::change::ManagedState::default();
            state.apply(&vlc(true)[0]).unwrap();
            state
        }),
    )
    .unwrap();
    assert!(
        env.plan(&bluetooth())
            .unwrap_err()
            .contains("different relay/managed.nix")
    );
    // The running system does not publish the module at all.
    fs::remove_file(env.host.root().join("etc/relay/managed.nix")).unwrap();
    assert!(
        env.plan(&bluetooth())
            .unwrap_err()
            .contains("/etc/relay/managed.nix")
    );
    assert!(env.states().is_empty());
}

#[test]
fn the_state_directory_must_not_live_inside_the_source() {
    let env = Env::new();
    let inside = env.flake.join(".relay-state");
    let engine = Engine::new(
        StateDir::new(&inside),
        env.host.clone(),
        NixAdapter::with_runner("nix", env.sim.clone()),
        env.sim.clone(),
        HealthPolicy::default(),
    );
    assert!(
        engine
            .plan(&env.flake, "host", &bluetooth())
            .unwrap_err()
            .contains("state directory")
    );
}

#[test]
fn plans_for_stateful_migrations_are_explained_but_never_applied() {
    let env = Env::new();
    let plan = env
        .plan(&[Change::SetOption {
            name: "services.postgresql.enable".into(),
            value: Value::Bool(true),
        }])
        .unwrap();
    assert_eq!(plan.record.risk, Risk::MigrationRequired);
    assert!(!plan.applicable);
    let error = env.apply(&plan.record.id).unwrap_err();
    assert!(error.contains("never activated automatically"));
    assert_eq!(env.states()[&plan.record.id], ChangeState::Built);
    assert!(env.sim.privileged_log().is_empty());
    assert!(
        env.engine()
            .explain(&plan.record.id)
            .unwrap()
            .contains("never activates it automatically")
    );
}

// ------------------------------------------------------ apply: the happy paths

#[test]
fn bluetooth_option_goes_through_test_health_and_switch() {
    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    let id = plan.record.id.clone();
    let report = env.apply(&id).unwrap();

    assert_eq!(report.outcome, ApplyOutcome::Switched);
    // dry-activate is a gate, then temporary activation, then the profile and the switch.
    assert_eq!(
        env.sim.actions(),
        ["dry-activate", "test", "set-profile", "switch"]
    );
    assert!(env.managed().contains("hardware.bluetooth.enable = true;"));
    assert_eq!(env.current(), plan.record.candidate_system_path);
    assert_eq!(env.profile(), plan.record.candidate_system_path);
    assert_eq!(env.states()[&id], ChangeState::Switched);
    assert!(
        !env.candidate_exists(&id),
        "the candidate is cleaned up after commit"
    );
    // The runtime publishes exactly the source that was applied.
    let runtime = fs::read_to_string(env.host.root().join("etc/relay/managed.nix")).unwrap();
    assert_eq!(runtime, env.managed());
    // Every privileged call used the exact planned candidate path.
    for entry in env.sim.privileged_log() {
        assert!(
            entry.ends_with(&plan.record.candidate_system_path) || entry.ends_with(BASE),
            "{entry}"
        );
    }
}

#[test]
fn vlc_can_be_added_removed_and_the_last_change_undone() {
    let env = Env::new();
    let added = env.plan_and_apply(&vlc(true));
    assert_eq!(added.outcome, ApplyOutcome::Switched);
    assert!(env.managed().contains("    vlc\n"));
    let system_with_vlc = env.current();

    let removed = env.plan_and_apply(&vlc(false));
    assert_eq!(removed.outcome, ApplyOutcome::Switched);
    assert!(!env.managed().contains("vlc"));

    // "Undo the last change" brings VLC back, source and runtime together.
    let undone = env.engine().undo(Confirmation::granted()).unwrap();
    assert_eq!(
        undone.outcome,
        ApplyOutcome::RolledBack {
            reason: "undo".into()
        }
    );
    assert!(env.managed().contains("    vlc\n"));
    assert_eq!(env.current(), system_with_vlc);
    assert_eq!(env.profile(), system_with_vlc);
    assert_eq!(env.states()[&removed.id], ChangeState::RolledBack);
    assert_eq!(env.states()[&added.id], ChangeState::Switched);

    // The next undo reverts the addition; after that there is nothing left to undo.
    env.engine().undo(Confirmation::granted()).unwrap();
    assert_eq!(env.managed(), render_state(&Default::default()));
    assert_eq!(
        (env.current(), env.profile()),
        (BASE.to_owned(), BASE.to_owned())
    );
    // After both changes were undone there is nothing left to undo.
    assert!(
        env.engine()
            .undo(Confirmation::granted())
            .unwrap_err()
            .contains("no applied")
    );
}

#[test]
fn undo_with_nothing_applied_is_an_error_and_refuses_when_the_source_moved_on() {
    let env = Env::new();
    assert!(
        env.engine()
            .undo(Confirmation::granted())
            .unwrap_err()
            .contains("no applied")
    );
    env.plan_and_apply(&bluetooth());
    fs::write(env.flake.join("configuration.nix"), "{ changed = true; }\n").unwrap();
    let error = env.engine().undo(Confirmation::granted()).unwrap_err();
    assert!(error.contains("source changed"));
    assert!(
        env.managed().contains("bluetooth"),
        "a refused undo changes nothing"
    );
    assert!(
        env.sim
            .actions()
            .iter()
            .filter(|a| *a == "set-profile")
            .count()
            == 1
    );
}

#[test]
fn undo_refuses_when_the_running_system_changed_underneath() {
    let env = Env::new();
    env.plan_and_apply(&bluetooth());
    fixture::point(&env.host, "run/current-system", BASE);
    let error = env.engine().undo(Confirmation::granted()).unwrap_err();
    assert!(error.contains("running system no longer matches"));
}

#[test]
fn intents_from_any_frontend_run_the_same_pipeline_without_any_ai_provider() {
    // AI unavailable is the normal case: the whole workflow is driven by typed intents.
    let env = Env::new();
    let changes = parse_intent(
        r#"{"schema":1,"changes":[{"op":"set_option","option":"hardware.bluetooth.enable","value":true},
                                  {"op":"add_package","package":"vlc"}]}"#,
    )
    .unwrap();
    let report = env.plan_and_apply(&changes);
    assert_eq!(report.outcome, ApplyOutcome::Switched);
    assert!(env.managed().contains("vlc") && env.managed().contains("bluetooth"));
    assert!(parse_intent("please enable bluetooth, thanks").is_err());
}

// ------------------------------------------------------------ drift and gates

#[test]
fn source_drift_between_plan_and_apply_aborts_before_any_mutation() {
    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    fs::write(
        env.flake.join("configuration.nix"),
        "{ boot.something = 1; }\n",
    )
    .unwrap();
    let managed_before = env.managed();
    let error = env.apply(&plan.record.id).unwrap_err();
    assert!(error.contains("source changed"));
    assert_eq!(env.managed(), managed_before);
    assert!(
        env.sim.privileged_log().is_empty(),
        "not even the preview ran"
    );
    assert_eq!(env.entry(&plan.record.id).state, ChangeState::Failed);
    assert_eq!(
        env.entry(&plan.record.id).detail.as_deref(),
        Some("source-drift")
    );
    assert!(!env.candidate_exists(&plan.record.id));
}

#[test]
fn runtime_drift_and_pending_reboots_block_apply() {
    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    // Someone activated a different system in the meantime.
    let other = "/nix/store/11111111111111111111111111111111-nixos-system-host";
    fixture::make_system(&env.host, other, "/nix/store/kernel-1", "{}");
    fixture::point(&env.host, "run/current-system", other);
    assert!(
        env.apply(&plan.record.id)
            .unwrap_err()
            .contains("running system changed")
    );
    assert!(env.sim.privileged_log().is_empty());

    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    // The profile moved without a reboot (an earlier `nixos-rebuild boot`).
    fixture::point(
        &env.host,
        "nix/var/nix/profiles/system-9-link",
        other_path(&env),
    );
    fixture::point(&env.host, "nix/var/nix/profiles/system", "system-9-link");
    assert!(env.apply(&plan.record.id).unwrap_err().contains("profile"));
    assert!(env.sim.privileged_log().is_empty());
}

fn other_path(env: &Env) -> &'static str {
    let other = "/nix/store/22222222222222222222222222222222-nixos-system-host";
    fixture::make_system(&env.host, other, "/nix/store/kernel-1", "{}");
    other
}

#[test]
fn a_modified_or_missing_candidate_is_never_activated() {
    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    let candidate = env.engine().state().candidate_source(&plan.record.id);
    fs::write(candidate.join("relay/managed.nix"), "{ pkgs, ... }: { }\n").unwrap();
    let error = env.apply(&plan.record.id).unwrap_err();
    assert!(error.contains("modified after it was built"), "{error}");
    assert!(env.sim.privileged_log().is_empty());
    assert_eq!(
        env.entry(&plan.record.id).detail.as_deref(),
        Some("candidate-tampered")
    );

    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    env.engine().state().remove_candidate(&plan.record.id);
    assert!(
        env.apply(&plan.record.id)
            .unwrap_err()
            .contains("candidate directory is missing")
    );
}

#[test]
fn a_failing_dry_activate_blocks_the_change_before_it_starts() {
    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    env.sim.fail("dry-activate");
    let error = env.apply(&plan.record.id).unwrap_err();
    assert!(error.contains("nothing was changed") && !error.contains("hunter2"));
    assert_eq!(env.sim.actions(), ["dry-activate"]);
    assert_eq!(
        env.entry(&plan.record.id).detail.as_deref(),
        Some("dry-activate-failed")
    );
    assert!(!env.managed().contains("bluetooth"));
}

#[test]
fn only_a_built_plan_can_be_applied_and_only_once() {
    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    assert_eq!(
        env.apply(&plan.record.id).unwrap().outcome,
        ApplyOutcome::Switched
    );
    assert!(
        env.apply(&plan.record.id)
            .unwrap_err()
            .contains("cannot be applied")
    );
    assert!(
        env.apply("chg-does-not-exist")
            .unwrap_err()
            .contains("unknown change")
    );
    assert!(env.apply("../escape").is_err());
}

#[test]
fn discarding_a_plan_closes_it_and_removes_its_candidate() {
    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    env.engine().discard(&plan.record.id).unwrap();
    assert_eq!(env.entry(&plan.record.id).state, ChangeState::Failed);
    assert!(!env.candidate_exists(&plan.record.id));
    assert!(env.apply(&plan.record.id).is_err());
}

#[test]
fn a_second_relay_process_is_excluded_by_the_lock() {
    let env = Env::new();
    let engine = env.engine();
    let _held = engine.state().lock().unwrap();
    fs::write(env.state_dir.join("lock"), "1\n").unwrap();
    assert!(
        env.plan(&bluetooth())
            .unwrap_err()
            .contains("another relay process")
    );
}

// ------------------------------------------------------- failure and rollback

#[test]
fn failed_test_activation_restores_source_and_runtime() {
    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    env.sim.fail("test");
    let report = env.apply(&plan.record.id).unwrap();
    assert_eq!(rolled_back_reason(&report), "test-activation-failed");
    assert_eq!(env.managed(), render_state(&Default::default()));
    assert_eq!(
        (env.current(), env.profile()),
        (BASE.to_owned(), BASE.to_owned())
    );
    assert_eq!(env.states()[&plan.record.id], ChangeState::RolledBack);
    assert!(!env.actions_contain("switch"));
    assert!(
        !env.actions_contain("set-profile"),
        "the boot default was never touched"
    );
}

impl Env {
    fn actions_contain(&self, action: &str) -> bool {
        self.sim.actions().iter().any(|a| a == action)
    }
}

#[test]
fn failed_health_check_rolls_back_because_test_is_not_a_rollback() {
    let env = Env::new();
    env.sim.lock().unhealthy_when_changed = true;
    let plan = env.plan(&bluetooth()).unwrap();
    let report = env.apply(&plan.record.id).unwrap();
    assert_eq!(rolled_back_reason(&report), "health-check-failed");
    // The test activation really changed the runtime; Relay itself returned it to the previous
    // system instead of assuming that "test" can be undone by itself.
    assert_eq!(env.sim.actions(), ["dry-activate", "test", "test"]);
    assert!(env.sim.privileged_log().last().unwrap().ends_with(BASE));
    assert_eq!(
        (env.current(), env.profile()),
        (BASE.to_owned(), BASE.to_owned())
    );
    assert_eq!(env.managed(), render_state(&Default::default()));
    let log = fs::read_to_string(
        env.engine()
            .state()
            .change_dir(&plan.record.id)
            .join("health-check-failed.log"),
    )
    .unwrap();
    assert!(log.contains("broken.service"));
}

#[test]
fn preexisting_failures_do_not_cause_a_rollback_but_expected_units_do() {
    let env = Env::new();
    env.sim.lock().preexisting_failure = true;
    assert_eq!(
        env.plan_and_apply(&bluetooth()).outcome,
        ApplyOutcome::Switched
    );

    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    // The change is expected to start bluetooth.service, which never becomes active here.
    let report = env
        .engine()
        .apply(
            &plan.record.id,
            Confirmation::granted(),
            &["bluetooth.service".to_owned()],
        )
        .unwrap();
    assert_eq!(rolled_back_reason(&report), "health-check-failed");
    assert!(
        env.engine()
            .apply("x", Confirmation::granted(), &["--now.service".to_owned()])
            .is_err()
    );
}

#[test]
fn failed_switch_after_the_profile_update_restores_the_profile_too() {
    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    env.sim.fail_once("switch");
    let report = env.apply(&plan.record.id).unwrap();
    assert_eq!(rolled_back_reason(&report), "switch-failed");
    assert_eq!(
        (env.current(), env.profile()),
        (BASE.to_owned(), BASE.to_owned())
    );
    assert_eq!(env.managed(), render_state(&Default::default()));
    // set-profile candidate, then set-profile back to the previous system.
    let log = env.sim.privileged_log();
    assert!(
        log.iter()
            .filter(|entry| entry.starts_with("set-profile"))
            .count()
            == 2
    );
}

#[test]
fn a_rollback_that_cannot_complete_is_reported_honestly_and_keeps_what_it_restored() {
    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    // `switch` keeps failing, so returning to the previous system by switching fails as well.
    env.sim.fail("switch");
    let report = env.apply(&plan.record.id).unwrap();
    assert!(
        matches!(report.outcome, ApplyOutcome::RollbackIncomplete { .. }),
        "{report:?}"
    );
    assert!(!report.outcome.succeeded());
    assert_eq!(
        env.managed(),
        render_state(&Default::default()),
        "the source part was restored"
    );
    assert_eq!(env.states()[&plan.record.id], ChangeState::Failed);
    assert_eq!(
        env.entry(&plan.record.id).detail.as_deref(),
        Some("rollback-incomplete")
    );
}

#[test]
fn a_failed_profile_update_rolls_back_cleanly() {
    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    env.sim.fail("set-profile");
    let report = env.apply(&plan.record.id).unwrap();
    assert_eq!(rolled_back_reason(&report), "profile-update-failed");
    assert_eq!(
        (env.current(), env.profile()),
        (BASE.to_owned(), BASE.to_owned())
    );
}

#[test]
fn a_candidate_that_evaluates_differently_after_the_source_write_is_never_activated() {
    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    env.sim.lock().reeval_returns_other_drv = true;
    let report = env.apply(&plan.record.id).unwrap();
    assert_eq!(rolled_back_reason(&report), "identity-mismatch");
    assert_eq!(
        env.sim.actions(),
        ["dry-activate"],
        "no activation of an unverified candidate"
    );
    assert_eq!(env.managed(), render_state(&Default::default()));
}

#[test]
fn rollback_never_overwrites_a_concurrent_foreign_edit_of_the_managed_module() {
    let env = Env::new();
    env.sim.lock().unhealthy_when_changed = true;
    let plan = env.plan(&bluetooth()).unwrap();
    let foreign = "{ pkgs, ... }: { # someone else was here\n}\n";
    env.sim.lock().edit_on_test = Some((env.flake.join("relay/managed.nix"), foreign.to_owned()));
    let report = env.apply(&plan.record.id).unwrap();
    assert!(
        matches!(report.outcome, ApplyOutcome::RollbackIncomplete { .. }),
        "{report:?}"
    );
    assert_eq!(env.managed(), foreign, "foreign content is left untouched");
    assert_eq!(
        env.current(),
        BASE,
        "the runtime part of the rollback still happened"
    );
    assert_eq!(env.states()[&plan.record.id], ChangeState::Failed);
}

// --------------------------------------------- switch inhibitors and reboots

#[test]
fn a_switch_inhibitor_forces_the_boot_path_and_is_never_bypassed() {
    let env = Env::new();
    env.sim.lock().next_inhibitors = Some(r#"{"systemd":"258"}"#.into());
    // The running system carries the old value for the same inhibitor key.
    fs::write(
        env.host
            .root()
            .join(BASE.trim_start_matches('/'))
            .join("switch-inhibitors"),
        r#"{"systemd":"257"}"#,
    )
    .unwrap();
    let plan = env.plan(&bluetooth()).unwrap();
    assert_eq!(plan.record.risk, Risk::RebootRequired);
    assert_eq!(plan.record.inhibitors, ["systemd: 257 -> 258"]);

    let report = env.apply(&plan.record.id).unwrap();
    assert_eq!(report.outcome, ApplyOutcome::RebootPending);
    assert!(report.notes.iter().any(|note| note.contains("inhibitors")));
    // Never `test`, never `switch`: only the boot generation was prepared.
    assert_eq!(env.sim.actions(), ["dry-activate", "set-profile", "boot"]);
    assert_eq!(
        env.current(),
        BASE,
        "the running system is untouched until the reboot"
    );
    assert_eq!(env.profile(), plan.record.candidate_system_path);
    assert_eq!(env.states()[&plan.record.id], ChangeState::RebootPending);
    // The simulator enforces NixOS' own rule: had Relay tried a live switch, it would fail.
    let direct = env
        .sim
        .switch_to_configuration(&plan.record.candidate_system_path, "switch")
        .unwrap();
    assert_eq!(direct.code, Some(1));
}

#[test]
fn a_kernel_change_is_reboot_required_and_verified_after_the_reboot() {
    let env = Env::new();
    env.sim.lock().next_kernel = Some("/nix/store/kernel-2".into());
    let plan = env.plan(&vlc(true)).unwrap();
    assert_eq!(plan.record.risk, Risk::RebootRequired);
    assert_eq!(plan.record.reboot_components, ["kernel"]);

    let report = env.apply(&plan.record.id).unwrap();
    assert_eq!(report.outcome, ApplyOutcome::RebootPending);
    assert_eq!(env.sim.actions(), ["dry-activate", "set-profile", "boot"]);
    assert!(
        env.managed().contains("vlc"),
        "the source is applied while the reboot is pending"
    );
    // A new change cannot start while a reboot is pending.
    assert!(env.plan(&bluetooth()).unwrap_err().contains("unresolved"));
    // Before the reboot, recover leaves the pending change alone.
    let waiting = env.engine().recover(false).unwrap();
    assert!(waiting[0].summary.contains("waiting for a reboot"));
    assert_eq!(env.states()[&plan.record.id], ChangeState::RebootPending);

    env.sim.reboot();
    let recovered = env.engine().recover(false).unwrap();
    assert_eq!(recovered[0].outcome, Some(ApplyOutcome::Switched));
    assert_eq!(env.states()[&plan.record.id], ChangeState::Switched);
    assert_eq!(env.current(), plan.record.candidate_system_path);
}

#[test]
fn an_unhealthy_system_after_the_reboot_is_rolled_back_to_the_previous_generation() {
    let env = Env::new();
    env.sim.lock().next_kernel = Some("/nix/store/kernel-2".into());
    env.sim.lock().unhealthy_when_changed = true;
    let plan = env.plan(&vlc(true)).unwrap();
    env.apply(&plan.record.id).unwrap();
    env.sim.reboot();

    let recovered = env.engine().recover(false).unwrap();
    assert_eq!(
        recovered[0].outcome,
        Some(ApplyOutcome::RolledBack {
            reason: "post-boot-health-failed".into()
        })
    );
    assert_eq!(
        env.profile(),
        BASE,
        "the next boot returns to the previous generation"
    );
    assert_eq!(env.managed(), render_state(&Default::default()));
    // A kernel change is never rolled back by a live switch.
    assert!(
        !env.sim
            .actions()
            .iter()
            .skip(3)
            .any(|a| a == "switch" || a == "test")
    );
}

#[test]
fn a_pending_reboot_can_be_aborted_explicitly() {
    let env = Env::new();
    env.sim.lock().next_kernel = Some("/nix/store/kernel-2".into());
    let plan = env.plan(&vlc(true)).unwrap();
    env.apply(&plan.record.id).unwrap();
    let aborted = env.engine().recover(true).unwrap();
    assert_eq!(
        aborted[0].outcome,
        Some(ApplyOutcome::RolledBack {
            reason: "aborted".into()
        })
    );
    assert_eq!(env.profile(), BASE);
    assert_eq!(env.managed(), render_state(&Default::default()));
}

#[test]
fn a_change_with_a_failing_boot_preparation_is_rolled_back() {
    let env = Env::new();
    env.sim.lock().next_kernel = Some("/nix/store/kernel-2".into());
    let plan = env.plan(&vlc(true)).unwrap();
    env.sim.fail_once("boot");
    let report = env.apply(&plan.record.id).unwrap();
    assert_eq!(rolled_back_reason(&report), "boot-preparation-failed");
    assert_eq!(env.profile(), BASE);
    assert_eq!(env.managed(), render_state(&Default::default()));
}

// --------------------------------------------------------- crash and restart

fn crash<T>(operation: impl FnOnce() -> T) {
    let result = catch_unwind(AssertUnwindSafe(operation));
    assert!(result.is_err(), "the simulated crash did not happen");
}

#[test]
fn a_crash_during_test_activation_is_rolled_back_after_restart() {
    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    env.sim.crash_after("test", 1);
    crash(|| env.apply(&plan.record.id));
    // The "process" died with the candidate temporarily active and the source already written.
    assert_eq!(env.current(), plan.record.candidate_system_path);
    assert!(env.managed().contains("bluetooth"));
    assert_eq!(env.states()[&plan.record.id], ChangeState::TestActivated);

    // Everything is blocked until the interrupted change is resolved.
    assert!(env.plan(&vlc(true)).unwrap_err().contains("relay recover"));
    assert!(env.apply(&plan.record.id).is_err());
    assert!(
        env.engine()
            .undo(Confirmation::granted())
            .unwrap_err()
            .contains("recover")
    );

    let recovered = env.engine().recover(false).unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(
        recovered[0].outcome,
        Some(ApplyOutcome::RolledBack {
            reason: "interrupted".into()
        })
    );
    assert_eq!(
        (env.current(), env.profile()),
        (BASE.to_owned(), BASE.to_owned())
    );
    assert_eq!(env.managed(), render_state(&Default::default()));
    assert_eq!(env.states()[&plan.record.id], ChangeState::RolledBack);
    // The system is usable again.
    assert!(env.plan(&vlc(true)).is_ok());
    // Recovery is idempotent.
    assert!(
        env.engine()
            .recover(false)
            .unwrap()
            .iter()
            .all(|entry| entry.id != plan.record.id)
    );
}

#[test]
fn a_crash_after_the_journal_entry_but_before_activation_only_restores_the_source() {
    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    env.sim.crash_before("test", 1);
    crash(|| env.apply(&plan.record.id));
    assert_eq!(env.current(), BASE);
    assert_eq!(env.states()[&plan.record.id], ChangeState::TestActivated);
    env.sim.clear_calls();
    let recovered = env.engine().recover(false).unwrap();
    assert_eq!(
        recovered[0].outcome,
        Some(ApplyOutcome::RolledBack {
            reason: "interrupted".into()
        })
    );
    assert_eq!(env.managed(), render_state(&Default::default()));
    assert!(
        env.sim.privileged_log().is_empty(),
        "the runtime was already correct; nothing to activate"
    );
}

#[test]
fn a_crash_between_source_write_intent_and_the_write_is_harmless() {
    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    // Dying right after the dry-activate gate means the journal says SourceApplied only if the
    // intent was written; crash at the first re-evaluation to cover "source written, nothing else".
    env.sim.crash_before("eval-live", 1);
    crash(|| env.apply(&plan.record.id));
    assert_eq!(env.states()[&plan.record.id], ChangeState::SourceApplied);
    assert!(env.managed().contains("bluetooth"));
    let recovered = env.engine().recover(false).unwrap();
    assert_eq!(
        recovered[0].outcome,
        Some(ApplyOutcome::RolledBack {
            reason: "interrupted".into()
        })
    );
    assert_eq!(env.managed(), render_state(&Default::default()));
    assert_eq!(env.current(), BASE);
}

#[test]
fn a_crash_after_a_complete_switch_is_recognised_and_completed_not_rolled_back() {
    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    env.sim.crash_after("switch", 1);
    crash(|| env.apply(&plan.record.id));
    assert_eq!(env.states()[&plan.record.id], ChangeState::Verified);
    let recovered = env.engine().recover(false).unwrap();
    assert_eq!(recovered[0].outcome, Some(ApplyOutcome::Switched));
    assert_eq!(env.states()[&plan.record.id], ChangeState::Switched);
    assert_eq!(env.current(), plan.record.candidate_system_path);
    assert!(env.managed().contains("bluetooth"));
    // The completed change is undoable like any other.
    assert_eq!(
        env.engine().undo(Confirmation::granted()).unwrap().outcome,
        ApplyOutcome::RolledBack {
            reason: "undo".into()
        }
    );
}

#[test]
fn a_verified_change_interrupted_before_the_switch_is_completed_after_a_health_recheck() {
    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    env.sim.crash_after("set-profile", 1);
    crash(|| env.apply(&plan.record.id));
    // The profile already points at the candidate although the journal still says Verified,
    // and the bootloader has not been updated because `switch` never ran.
    assert_eq!(env.profile(), plan.record.candidate_system_path);
    assert_eq!(env.states()[&plan.record.id], ChangeState::Verified);
    let actions_before = env.sim.actions().len();
    let recovered = env.engine().recover(false).unwrap();
    assert_eq!(recovered[0].outcome, Some(ApplyOutcome::Switched));
    assert_eq!(
        env.sim.actions()[actions_before..],
        ["switch"],
        "recovery performs the missing switch"
    );
    assert_eq!(
        (env.current(), env.profile()),
        (
            plan.record.candidate_system_path.clone(),
            plan.record.candidate_system_path
        )
    );
    assert!(env.managed().contains("bluetooth"));
}

#[test]
fn a_verified_change_that_turns_unhealthy_before_completion_is_rolled_back_instead() {
    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    env.sim.crash_after("set-profile", 1);
    crash(|| env.apply(&plan.record.id));
    env.sim.lock().unhealthy_when_changed = true;
    let recovered = env.engine().recover(false).unwrap();
    assert_eq!(
        recovered[0].outcome,
        Some(ApplyOutcome::RolledBack {
            reason: "interrupted".into()
        })
    );
    assert_eq!(
        (env.current(), env.profile()),
        (BASE.to_owned(), BASE.to_owned())
    );
    assert_eq!(env.managed(), render_state(&Default::default()));
}

#[test]
fn a_crash_in_the_middle_of_a_rollback_is_resumed() {
    let env = Env::new();
    env.sim.lock().unhealthy_when_changed = true;
    let plan = env.plan(&bluetooth()).unwrap();
    // First `test` activates the candidate; the second one is the rollback's runtime restore.
    env.sim.crash_before("test", 2);
    crash(|| env.apply(&plan.record.id));
    assert_eq!(env.states()[&plan.record.id], ChangeState::RollbackStarted);
    assert_eq!(env.current(), plan.record.candidate_system_path);
    let recovered = env.engine().recover(false).unwrap();
    assert_eq!(recovered.len(), 1);
    assert!(matches!(
        recovered[0].outcome,
        Some(ApplyOutcome::RolledBack { .. })
    ));
    assert_eq!(
        (env.current(), env.profile()),
        (BASE.to_owned(), BASE.to_owned())
    );
    assert_eq!(env.managed(), render_state(&Default::default()));
    assert_eq!(env.states()[&plan.record.id], ChangeState::RolledBack);
}

#[test]
fn recovery_refuses_to_guess_when_the_runtime_is_neither_previous_nor_candidate() {
    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    env.sim.crash_after("test", 1);
    crash(|| env.apply(&plan.record.id));
    let stranger = "/nix/store/33333333333333333333333333333333-nixos-system-host";
    fixture::make_system(&env.host, stranger, "/nix/store/kernel-1", "{}");
    fixture::point(&env.host, "run/current-system", stranger);
    let recovered = env.engine().recover(false).unwrap();
    assert!(matches!(
        recovered[0].outcome,
        Some(ApplyOutcome::RollbackIncomplete { .. })
    ));
    assert_eq!(env.current(), stranger, "nothing was activated on a guess");
    assert_eq!(env.states()[&plan.record.id], ChangeState::Failed);
}

#[test]
fn abandoned_plans_are_closed_by_recovery_and_stale_locks_do_not_block_it() {
    let env = Env::new();
    // A plan that crashed while building leaves a journal entry in `planned`.
    env.sim.crash_before("eval", 1);
    crash(|| env.plan(&bluetooth()));
    let (id, state) = env.states().into_iter().next().unwrap();
    assert_eq!(state, ChangeState::Planned);
    // The crashed process's lock file names a pid that no longer exists.
    fs::write(env.state_dir.join("lock"), "4294967294\n").unwrap();
    let recovered = env.engine().recover(false).unwrap();
    assert_eq!(recovered[0].id, id);
    assert_eq!(env.states()[&id], ChangeState::Failed);
    assert!(!env.candidate_exists(&id));
    assert!(env.plan(&bluetooth()).is_ok());
}

// ---------------------------------------------------------------- explanation

#[test]
fn explain_describes_intent_risk_and_recovery_without_leaking_values() {
    let env = Env::new();
    let plan = env
        .plan(&[Change::SetOption {
            name: "services.example.message".into(),
            value: Value::String("not-a-secret-but-stays-out".into()),
        }])
        .unwrap();
    let text = env.engine().explain(&plan.record.id).unwrap();
    assert!(text.contains("set services.example.message"));
    assert!(text.contains("LIVE_SWITCHABLE"));
    assert!(text.contains(BASE));
    assert!(text.contains("relay apply"));
    assert!(!text.contains("not-a-secret-but-stays-out"));
    let history = env.engine().history().unwrap();
    assert_eq!(history[0].id, plan.record.id);
}

#[test]
fn journal_and_state_never_contain_configuration_values() {
    let env = Env::new();
    let plan = env
        .plan(&[Change::SetOption {
            name: "services.example.message".into(),
            value: Value::String("VALUE-MARKER-12345".into()),
        }])
        .unwrap();
    env.apply(&plan.record.id).unwrap();
    let journal = fs::read_to_string(env.state_dir.join("journal")).unwrap();
    assert!(!journal.contains("VALUE-MARKER"));
    // Hex-encoded fields could hide it; decode and check.
    for line in journal.lines() {
        for field in line.split('\t') {
            let bytes: Vec<u8> = (0..field.len() / 2)
                .map(|i| u8::from_str_radix(&field[2 * i..2 * i + 2], 16).unwrap())
                .collect();
            assert!(!String::from_utf8_lossy(&bytes).contains("VALUE-MARKER"));
        }
    }
}

// ------------------------------------------------------------ optional AI layer

/// A provider that answers with a fixed result.
struct FixedProvider(Result<String, String>);

impl crate::ai::Provider for FixedProvider {
    fn label(&self) -> String {
        "fixed".into()
    }

    fn complete(&self, _prompt: &crate::ai::Prompt) -> Result<String, String> {
        self.0.clone()
    }
}

#[test]
fn a_model_proposal_flows_through_the_same_pipeline_as_any_other_intent() {
    let env = Env::new();
    let provider = FixedProvider(Ok(
        r#"{"schema":1,"changes":[{"op":"set_option","option":"hardware.bluetooth.enable","value":true}]}"#.into(),
    ));
    let resolved =
        crate::ai::propose(&provider, "Aktiviere Bluetooth", "host", None, None).unwrap();
    // Proposing changed nothing at all.
    assert!(env.sim.lock().calls.is_empty());
    assert!(env.states().is_empty());
    let crate::intent::Proposal::Changes(changes) = resolved.proposal else {
        panic!("expected changes");
    };
    let report = env.plan_and_apply(&changes);
    assert_eq!(report.outcome, ApplyOutcome::Switched);
    assert!(env.managed().contains("hardware.bluetooth.enable = true;"));
}

#[test]
fn an_unavailable_ai_provider_changes_nothing_and_the_core_keeps_working() {
    let env = Env::new();
    let live = env.live_hash();
    let provider = FixedProvider(Err("connection refused".into()));
    assert!(matches!(
        crate::ai::propose(&provider, "Aktiviere Bluetooth", "host", None, None),
        Err(crate::ai::AskError::Unavailable(_))
    ));
    assert_eq!(env.live_hash(), live);
    assert!(env.states().is_empty());
    assert!(env.sim.lock().calls.is_empty());
    // The same change works through the plain CLI path without any provider.
    assert_eq!(
        env.plan_and_apply(&bluetooth()).outcome,
        ApplyOutcome::Switched
    );
}

#[test]
fn hostile_or_confused_model_output_never_reaches_planning() {
    let env = Env::new();
    for answer in [
        "Sure, I will run `nixos-rebuild switch` for you.",
        r#"{"schema":1,"changes":[{"op":"set_option","option":"system.stateVersion","value":"99.11"}]}"#,
        r#"{"schema":1,"changes":[{"op":"set_option","option":"boot.loader.grub.enable","value":false}]}"#,
        r#"{"schema":1,"changes":[{"op":"add_package","package":"x; rm -rf /"}]}"#,
        r#"{"schema":1,"changes":[{"op":"set_option","option":"users.users.root.hashedPassword","value":"abc"}]}"#,
        r#"{"schema":1,"action":"apply"}"#,
    ] {
        let provider = FixedProvider(Ok(answer.into()));
        assert!(
            matches!(
                crate::ai::propose(&provider, "do it", "host", None, None),
                Err(crate::ai::AskError::Rejected(_))
            ),
            "accepted: {answer}"
        );
    }
    assert!(
        env.states().is_empty(),
        "nothing was planned, journaled or built"
    );
    assert!(env.sim.lock().calls.is_empty());
}

// ------------------------------------------------------------------ desktop health

/// A compositor whose state follows the simulated machine: anything but the base system "breaks"
/// it in the configured way.
struct SimDesktop {
    host: Host,
    lose_monitor: bool,
    die: bool,
    new_error: Option<&'static str>,
    preexisting_error: Option<&'static str>,
    unreachable_at_baseline: bool,
}

impl SimDesktop {
    fn healthy(host: &Host) -> Self {
        Self {
            host: host.clone(),
            lose_monitor: false,
            die: false,
            new_error: None,
            preexisting_error: None,
            unreachable_at_baseline: false,
        }
    }
}

impl crate::hypr::DesktopProbe for SimDesktop {
    fn snapshot(&self) -> Result<crate::hypr::DesktopSnapshot, String> {
        let changed = self.host.current_system().as_deref() != Some(BASE);
        if self.unreachable_at_baseline && !changed {
            return Err("no compositor".into());
        }
        if self.die && changed {
            return Err("connection refused".into());
        }
        let mut config_errors = self
            .preexisting_error
            .map(str::to_owned)
            .into_iter()
            .collect::<Vec<_>>();
        if changed {
            config_errors.extend(self.new_error.map(str::to_owned));
        }
        Ok(crate::hypr::DesktopSnapshot {
            monitors: if self.lose_monitor && changed { 1 } else { 2 },
            config_errors,
        })
    }
}

impl Env {
    fn apply_with_desktop(&self, desktop: SimDesktop, id: &str) -> super::ApplyReport {
        self.engine()
            .with_desktop(Arc::new(desktop))
            .apply(id, Confirmation::granted(), &[])
            .unwrap()
    }
}

#[test]
fn a_healthy_desktop_does_not_block_a_change_and_its_baseline_is_recorded() {
    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    let report = env.apply_with_desktop(SimDesktop::healthy(&env.host), &plan.record.id);
    assert_eq!(report.outcome, ApplyOutcome::Switched);
    assert!(report.notes.is_empty());
    let record = env.engine().state().load_plan(&plan.record.id).unwrap();
    assert_eq!(record.baseline_monitors, Some(2));
}

#[test]
fn a_change_that_costs_the_desktop_a_monitor_is_rolled_back() {
    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    let desktop = SimDesktop {
        lose_monitor: true,
        ..SimDesktop::healthy(&env.host)
    };
    let report = env.apply_with_desktop(desktop, &plan.record.id);
    assert_eq!(rolled_back_reason(&report), "desktop-check-failed");
    assert_eq!(
        (env.current(), env.profile()),
        (BASE.to_owned(), BASE.to_owned())
    );
    assert_eq!(env.managed(), render_state(&Default::default()));
    let log = fs::read_to_string(
        env.engine()
            .state()
            .change_dir(&plan.record.id)
            .join("desktop-check-failed.log"),
    )
    .unwrap();
    assert!(log.contains("1 monitor(s) were lost"), "{log}");
    // The desktop gate sits between `test` and the boot-default switch: no profile change happened.
    assert!(!env.actions_contain("set-profile"));
}

#[test]
fn a_change_that_kills_the_compositor_is_rolled_back() {
    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    let desktop = SimDesktop {
        die: true,
        ..SimDesktop::healthy(&env.host)
    };
    let report = env.apply_with_desktop(desktop, &plan.record.id);
    assert_eq!(rolled_back_reason(&report), "desktop-check-failed");
    assert_eq!(env.current(), BASE);
}

#[test]
fn only_new_compositor_config_errors_count_against_a_change() {
    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    let desktop = SimDesktop {
        new_error: Some("config line 9: unknown keyword"),
        preexisting_error: Some("config line 3: old problem"),
        ..SimDesktop::healthy(&env.host)
    };
    let report = env.apply_with_desktop(desktop, &plan.record.id);
    assert_eq!(rolled_back_reason(&report), "desktop-check-failed");

    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    let tolerated = SimDesktop {
        preexisting_error: Some("config line 3: old problem"),
        ..SimDesktop::healthy(&env.host)
    };
    assert_eq!(
        env.apply_with_desktop(tolerated, &plan.record.id).outcome,
        ApplyOutcome::Switched
    );
}

#[test]
fn without_a_reachable_compositor_the_desktop_check_is_skipped_visibly_not_silently() {
    let env = Env::new();
    let plan = env.plan(&bluetooth()).unwrap();
    let desktop = SimDesktop {
        unreachable_at_baseline: true,
        ..SimDesktop::healthy(&env.host)
    };
    let report = env.apply_with_desktop(desktop, &plan.record.id);
    assert_eq!(report.outcome, ApplyOutcome::Switched);
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("desktop check skipped")),
        "{:?}",
        report.notes
    );
    assert_eq!(
        env.engine()
            .state()
            .load_plan(&plan.record.id)
            .unwrap()
            .baseline_monitors,
        None
    );
}

// ------------------------------------------------- source must be applied before planning

#[test]
fn unapplied_edits_anywhere_in_the_configuration_block_planning() {
    let env = Env::new();
    // The user is half-way through editing their desktop configuration; the running system does
    // not have these edits yet. A Relay candidate would activate them as a side effect.
    fs::write(
        env.flake.join("configuration.nix"),
        "{ desktop.wip = true; }\n",
    )
    .unwrap();
    let error = env.plan(&bluetooth()).unwrap_err();
    assert!(
        error.contains("not applied to the running system"),
        "{error}"
    );
    assert!(
        env.states().is_empty(),
        "refused before anything was journaled or built"
    );
    assert!(env.sim.privileged_log().is_empty());
    // Once the edit is gone (or has been applied by a rebuild) planning works again.
    fs::write(env.flake.join("configuration.nix"), "{ }\n").unwrap();
    assert!(env.plan(&bluetooth()).is_ok());
}

#[test]
fn after_an_apply_and_an_undo_the_source_still_matches_the_system_it_belongs_to() {
    let env = Env::new();
    assert_eq!(
        env.plan_and_apply(&vlc(true)).outcome,
        ApplyOutcome::Switched
    );
    // Source and runtime moved together, so the next plan is allowed...
    let plan = env.plan(&bluetooth()).unwrap();
    env.engine().discard(&plan.record.id).unwrap();
    // ...and so is one after an undo returned both to the earlier state.
    env.engine().undo(Confirmation::granted()).unwrap();
    assert!(env.plan(&bluetooth()).is_ok());
}
