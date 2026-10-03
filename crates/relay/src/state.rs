//! Relay's own persistent state: the change journal, one directory per change with the evidence
//! needed to recover it, isolated candidate trees, and a single-writer lock.
//!
//! ```text
//! <state>/journal                  append-only state transitions (no configuration values)
//! <state>/lock                     pid of the process currently mutating
//! <state>/changes/<id>/plan.json   the plan record
//! <state>/changes/<id>/managed.before.nix, managed.after.nix, *.log
//! <state>/candidates/<id>/src      isolated candidate source tree
//! ```

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::change::{Change, Risk};
use crate::fsutil::{create_private_dir, truncate_log, write_private};
use crate::intent::{changes_from_json, changes_to_json};
use crate::journal::Journal;
use crate::json::Json;
use crate::json_string;

const PLAN_SCHEMA: u64 = 1;
const MAX_LOG_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanRecord {
    pub id: String,
    pub created: u64,
    pub flake: PathBuf,
    pub host: String,
    pub changes: Vec<Change>,
    pub risk: Risk,
    /// Tree identity of the live source when the candidate was built (drift baseline).
    pub source_hash: String,
    pub managed_before_hash: String,
    pub managed_after_hash: String,
    /// The running system the plan was made against (runtime baseline and rollback target).
    pub base_system_path: String,
    pub candidate_drv: String,
    pub candidate_system_path: String,
    pub candidate_hash: String,
    pub reboot_components: Vec<String>,
    pub inhibitors: Vec<String>,
    /// Tree identity right after the managed file was written; set when the change is applied.
    pub source_hash_after: Option<String>,
    /// Health baseline captured at apply time.
    pub baseline_system_state: Option<String>,
    pub baseline_unhealthy: Vec<String>,
    /// Desktop baseline (enabled monitors and compositor config errors); `None` when no compositor
    /// was reachable at apply time.
    pub baseline_monitors: Option<u64>,
    pub baseline_config_errors: Vec<String>,
}

impl PlanRecord {
    pub fn to_json(&self) -> String {
        let strings = |values: &[String]| {
            format!(
                "[{}]",
                values
                    .iter()
                    .map(|value| json_string(value))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        let optional = |value: &Option<String>| {
            value
                .as_deref()
                .map_or_else(|| "null".to_owned(), json_string)
        };
        format!(
            concat!(
                "{{\"schema\":{},\"id\":{},\"created\":{},\"flake\":{},\"host\":{},",
                "\"changes\":{},\"risk\":{},\"source_hash\":{},\"managed_before_hash\":{},",
                "\"managed_after_hash\":{},\"base_system_path\":{},\"candidate_drv\":{},",
                "\"candidate_system_path\":{},\"candidate_hash\":{},\"reboot_components\":{},",
                "\"inhibitors\":{},\"source_hash_after\":{},\"baseline_system_state\":{},",
                "\"baseline_unhealthy\":{},\"baseline_monitors\":{},\"baseline_config_errors\":{}}}\n"
            ),
            PLAN_SCHEMA,
            json_string(&self.id),
            self.created,
            json_string(&self.flake.to_string_lossy()),
            json_string(&self.host),
            changes_to_json(&self.changes),
            json_string(self.risk.as_str()),
            json_string(&self.source_hash),
            json_string(&self.managed_before_hash),
            json_string(&self.managed_after_hash),
            json_string(&self.base_system_path),
            json_string(&self.candidate_drv),
            json_string(&self.candidate_system_path),
            json_string(&self.candidate_hash),
            strings(&self.reboot_components),
            strings(&self.inhibitors),
            optional(&self.source_hash_after),
            optional(&self.baseline_system_state),
            strings(&self.baseline_unhealthy),
            self.baseline_monitors
                .map_or_else(|| "null".to_owned(), |monitors| monitors.to_string()),
            strings(&self.baseline_config_errors),
        )
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        let root = Json::parse(text)?;
        let text_field = |key: &str| -> Result<String, String> {
            root.get(key)
                .and_then(Json::as_str)
                .map(str::to_owned)
                .ok_or_else(|| format!("plan record field '{key}' is missing or not a string"))
        };
        let optional = |key: &str| -> Result<Option<String>, String> {
            match root.get(key) {
                None | Some(Json::Null) => Ok(None),
                Some(Json::String(value)) => Ok(Some(value.clone())),
                Some(_) => Err(format!(
                    "plan record field '{key}' must be a string or null"
                )),
            }
        };
        let strings = |key: &str| -> Result<Vec<String>, String> {
            root.get(key)
                .and_then(Json::as_array)
                .ok_or_else(|| format!("plan record field '{key}' must be an array"))?
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| format!("plan record field '{key}' must contain strings"))
                })
                .collect()
        };
        if root.get("schema").and_then(Json::as_u64) != Some(PLAN_SCHEMA) {
            return Err("unsupported plan record schema".into());
        }
        Ok(Self {
            id: text_field("id")?,
            created: root
                .get("created")
                .and_then(Json::as_u64)
                .ok_or_else(|| "plan record field 'created' is invalid".to_owned())?,
            flake: PathBuf::from(text_field("flake")?),
            host: text_field("host")?,
            changes: changes_from_json(
                root.get("changes")
                    .ok_or_else(|| "plan record has no changes".to_owned())?,
            )?,
            risk: Risk::parse(&text_field("risk")?)
                .ok_or_else(|| "plan record has an unknown risk".to_owned())?,
            source_hash: text_field("source_hash")?,
            managed_before_hash: text_field("managed_before_hash")?,
            managed_after_hash: text_field("managed_after_hash")?,
            base_system_path: text_field("base_system_path")?,
            candidate_drv: text_field("candidate_drv")?,
            candidate_system_path: text_field("candidate_system_path")?,
            candidate_hash: text_field("candidate_hash")?,
            reboot_components: strings("reboot_components")?,
            inhibitors: strings("inhibitors")?,
            source_hash_after: optional("source_hash_after")?,
            baseline_system_state: optional("baseline_system_state")?,
            baseline_unhealthy: strings("baseline_unhealthy")?,
            baseline_monitors: root.get("baseline_monitors").and_then(Json::as_u64),
            baseline_config_errors: if root.get("baseline_config_errors").is_some() {
                strings("baseline_config_errors")?
            } else {
                Vec::new()
            },
        })
    }
}

#[derive(Clone, Debug)]
pub struct StateDir {
    root: PathBuf,
}

impl StateDir {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// `RELAY_STATE_DIR`, else `$XDG_STATE_HOME/relay`, else `$HOME/.local/state/relay`.
    pub fn default_location() -> Result<PathBuf, String> {
        let from_env = |key: &str| std::env::var_os(key).filter(|value| !value.is_empty());
        if let Some(path) = from_env("RELAY_STATE_DIR") {
            return Ok(PathBuf::from(path));
        }
        if let Some(path) = from_env("XDG_STATE_HOME") {
            return Ok(PathBuf::from(path).join("relay"));
        }
        from_env("HOME")
            .map(|home| PathBuf::from(home).join(".local/state/relay"))
            .ok_or_else(|| "cannot determine a state directory; pass --state-dir".to_owned())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn ensure(&self) -> Result<(), String> {
        create_private_dir(&self.root)?;
        create_private_dir(&self.root.join("changes"))?;
        create_private_dir(&self.root.join("candidates"))
    }

    pub fn journal(&self) -> Journal {
        Journal::new(self.root.join("journal"))
    }

    pub fn change_dir(&self, id: &str) -> PathBuf {
        self.root.join("changes").join(id)
    }

    pub fn candidate_dir(&self, id: &str) -> PathBuf {
        self.root.join("candidates").join(id)
    }

    pub fn candidate_source(&self, id: &str) -> PathBuf {
        self.candidate_dir(id).join("src")
    }

    pub fn save_plan(&self, record: &PlanRecord) -> Result<(), String> {
        create_private_dir(&self.change_dir(&record.id))?;
        write_private(
            &self.change_dir(&record.id).join("plan.json"),
            record.to_json().as_bytes(),
        )
    }

    pub fn load_plan(&self, id: &str) -> Result<PlanRecord, String> {
        crate::journal::validate_change_id(id)?;
        let text = fs::read_to_string(self.change_dir(id).join("plan.json"))
            .map_err(|error| format!("could not read the plan record of '{id}': {error}"))?;
        PlanRecord::from_json(&text)
    }

    pub fn save_file(&self, id: &str, name: &str, contents: &[u8]) -> Result<(), String> {
        create_private_dir(&self.change_dir(id))?;
        write_private(&self.change_dir(id).join(name), contents)
    }

    pub fn load_file(&self, id: &str, name: &str) -> Result<Vec<u8>, String> {
        fs::read(self.change_dir(id).join(name))
            .map_err(|error| format!("could not read {name} of change '{id}': {error}"))
    }

    /// Store command output privately; it may contain evaluated values and never goes to stdout.
    pub fn save_log(&self, id: &str, name: &str, log: &[u8]) {
        // A diagnostic log must never mask the failure it describes.
        let _ = self.save_file(id, name, &truncate_log(log, MAX_LOG_BYTES));
    }

    pub fn remove_candidate(&self, id: &str) {
        let _ = fs::remove_dir_all(self.candidate_dir(id));
    }

    /// Take the single-writer lock. A lock whose process no longer exists is stale and replaced.
    pub fn lock(&self) -> Result<StateLock, String> {
        self.ensure()?;
        let path = self.root.join("lock");
        for _ in 0..2 {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    writeln!(file, "{}", std::process::id())
                        .map_err(|error| format!("could not write lock file: {error}"))?;
                    return Ok(StateLock { path });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    let holder = fs::read_to_string(&path)
                        .ok()
                        .and_then(|text| text.trim().parse::<u32>().ok());
                    match holder {
                        Some(pid) if process_exists(pid) && pid != std::process::id() => {
                            return Err(format!(
                                "another relay process (pid {pid}) is changing the system"
                            ));
                        }
                        _ => {
                            fs::remove_file(&path).map_err(|error| {
                                format!("could not remove stale lock file: {error}")
                            })?;
                        }
                    }
                }
                Err(error) => return Err(format!("could not create lock file: {error}")),
            }
        }
        Err("could not acquire the relay lock".into())
    }
}

fn process_exists(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

#[derive(Debug)]
pub struct StateLock {
    path: PathBuf,
}

impl Drop for StateLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::{PlanRecord, StateDir};
    use crate::change::{Change, Risk, Value};
    use crate::fsutil::testutil::TempDir;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    pub(crate) fn sample_record(id: &str) -> PlanRecord {
        PlanRecord {
            id: id.to_owned(),
            created: 1_700_000_000,
            flake: PathBuf::from("/home/u/flake"),
            host: "host".into(),
            changes: vec![
                Change::AddPackage { name: "vlc".into() },
                Change::SetOption {
                    name: "services.x.msg".into(),
                    value: Value::String("q\"uote".into()),
                },
            ],
            risk: Risk::RebootRequired,
            source_hash: "a".repeat(64),
            managed_before_hash: "b".repeat(64),
            managed_after_hash: "c".repeat(64),
            base_system_path: "/nix/store/aaaa-system-old".into(),
            candidate_drv: "/nix/store/bbbb-system-new.drv".into(),
            candidate_system_path: "/nix/store/bbbb-system-new".into(),
            candidate_hash: "d".repeat(64),
            reboot_components: vec!["kernel".into()],
            inhibitors: vec!["systemd: 1 -> 2".into()],
            source_hash_after: Some("e".repeat(64)),
            baseline_system_state: Some("degraded".into()),
            baseline_unhealthy: vec!["old.service".into()],
            baseline_monitors: Some(2),
            baseline_config_errors: vec!["old config error".into()],
        }
    }

    #[test]
    fn plan_records_round_trip() {
        let record = sample_record("chg-1");
        assert_eq!(PlanRecord::from_json(&record.to_json()).unwrap(), record);
        let mut minimal = record;
        minimal.source_hash_after = None;
        minimal.baseline_system_state = None;
        minimal.baseline_unhealthy.clear();
        minimal.baseline_monitors = None;
        minimal.baseline_config_errors.clear();
        assert_eq!(PlanRecord::from_json(&minimal.to_json()).unwrap(), minimal);
    }

    #[test]
    fn corrupted_or_foreign_plan_records_are_rejected() {
        let good = sample_record("chg-1").to_json();
        assert!(PlanRecord::from_json("{}").is_err());
        assert!(PlanRecord::from_json(&good.replace("\"schema\":1", "\"schema\":9")).is_err());
        assert!(PlanRecord::from_json(&good.replace("REBOOT_REQUIRED", "WHATEVER")).is_err());
        // A tampered record cannot smuggle in a protected change.
        assert!(
            PlanRecord::from_json(&good.replace("services.x.msg", "system.stateVersion")).is_err()
        );
    }

    #[test]
    fn state_files_are_private_and_plans_load_back() {
        let dir = TempDir::new("state");
        let state = StateDir::new(dir.path().join("relay"));
        state.ensure().unwrap();
        state.save_plan(&sample_record("chg-1")).unwrap();
        state.save_log("chg-1", "eval.log", b"value 'hunter2'");
        assert_eq!(state.load_plan("chg-1").unwrap(), sample_record("chg-1"));
        assert!(state.load_plan("../etc").is_err());
        for path in [
            state.root().to_path_buf(),
            state.change_dir("chg-1"),
            state.change_dir("chg-1").join("plan.json"),
            state.change_dir("chg-1").join("eval.log"),
        ] {
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o077;
            assert_eq!(
                mode,
                0,
                "{} must not be accessible to others",
                path.display()
            );
        }
    }

    #[test]
    fn the_lock_excludes_a_second_writer_and_recovers_from_stale_locks() {
        let dir = TempDir::new("state");
        let state = StateDir::new(dir.path().join("relay"));
        // A live holder other than ourselves: pid 1 always exists.
        state.ensure().unwrap();
        fs::write(state.root().join("lock"), "1\n").unwrap();
        assert!(state.lock().unwrap_err().contains("another relay process"));
        // A dead holder is replaced.
        fs::write(state.root().join("lock"), "4294967294\n").unwrap();
        let guard = state.lock().unwrap();
        assert!(
            fs::read_to_string(state.root().join("lock"))
                .unwrap()
                .trim()
                == std::process::id().to_string()
        );
        drop(guard);
        assert!(!state.root().join("lock").exists());
    }
}
