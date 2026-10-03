use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeState {
    Planned,
    Built,
    SourceApplied,
    TestActivated,
    Verified,
    Switched,
    RebootPending,
    RollbackStarted,
    RolledBack,
    Failed,
}

impl ChangeState {
    /// States in which the live source, runtime or boot configuration may differ from the
    /// recorded baseline. Such a change must be committed or rolled back (`relay recover`)
    /// before any new mutation starts.
    pub fn is_in_flight(self) -> bool {
        matches!(
            self,
            Self::SourceApplied
                | Self::TestActivated
                | Self::Verified
                | Self::RebootPending
                | Self::RollbackStarted
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Built => "built",
            Self::SourceApplied => "source-applied",
            Self::TestActivated => "test-activated",
            Self::Verified => "verified",
            Self::Switched => "switched",
            Self::RebootPending => "reboot-pending",
            Self::RollbackStarted => "rollback-started",
            Self::RolledBack => "rolled-back",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "planned" => Ok(Self::Planned),
            "built" => Ok(Self::Built),
            "source-applied" => Ok(Self::SourceApplied),
            "test-activated" => Ok(Self::TestActivated),
            "verified" => Ok(Self::Verified),
            "switched" => Ok(Self::Switched),
            "reboot-pending" => Ok(Self::RebootPending),
            "rollback-started" => Ok(Self::RollbackStarted),
            "rolled-back" => Ok(Self::RolledBack),
            "failed" => Ok(Self::Failed),
            _ => Err(format!("unknown journal state '{value}'")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalEntry {
    pub id: String,
    pub state: ChangeState,
    pub timestamp: u64,
    pub source_hash: String,
    pub candidate_hash: Option<String>,
    pub previous_system_path: Option<String>,
    pub candidate_system_path: Option<String>,
    /// Short machine-readable reason code (never evaluated values or command output).
    pub detail: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Journal {
    path: PathBuf,
}

impl Journal {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn entries(&self) -> Result<BTreeMap<String, JournalEntry>, String> {
        let contents = match fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(BTreeMap::new());
            }
            Err(error) => return Err(format!("could not read journal: {error}")),
        };
        let mut entries = BTreeMap::new();
        let complete_contents = contents
            .strip_suffix('\n')
            .map(|_| contents.as_str())
            .unwrap_or_else(|| {
                contents
                    .rsplit_once('\n')
                    .map_or("", |(complete, _)| complete)
            });
        for (line_number, line) in complete_contents.lines().enumerate() {
            if line.is_empty() {
                continue;
            }
            let entry = decode_event(line)
                .map_err(|error| format!("invalid journal line {}: {error}", line_number + 1))?;
            if let Some(previous) = entries.get(&entry.id) {
                let previous: &JournalEntry = previous;
                if !transition_allowed(previous.state, entry.state) {
                    return Err(format!(
                        "invalid state transition for journal entry '{}'",
                        entry.id
                    ));
                }
            } else if entry.state != ChangeState::Planned {
                return Err(format!(
                    "journal entry '{}' does not start in planned state",
                    entry.id
                ));
            }
            entries.insert(entry.id.clone(), entry);
        }
        Ok(entries)
    }

    pub fn begin(
        &self,
        id: &str,
        source_hash: &str,
        previous_system_path: &str,
    ) -> Result<JournalEntry, String> {
        validate_id(id)?;
        validate_hash(source_hash)?;
        validate_store_path(previous_system_path)?;
        let entries = self.entries()?;
        if entries.contains_key(id) {
            return Err(format!("journal entry '{id}' already exists"));
        }
        let entry = JournalEntry {
            id: id.to_owned(),
            state: ChangeState::Planned,
            timestamp: now_epoch_seconds()?,
            source_hash: source_hash.to_owned(),
            candidate_hash: None,
            previous_system_path: Some(previous_system_path.to_owned()),
            candidate_system_path: None,
            detail: None,
        };
        self.append(&entry)?;
        Ok(entry)
    }

    pub fn transition(
        &self,
        id: &str,
        state: ChangeState,
        candidate_hash: Option<&str>,
        candidate_system_path: Option<&str>,
        detail: Option<&str>,
    ) -> Result<JournalEntry, String> {
        let mut entries = self.entries()?;
        let previous = entries
            .get(id)
            .ok_or_else(|| format!("unknown journal entry '{id}'"))?;
        if let Some(detail) = detail {
            validate_detail(detail)?;
        }
        if !transition_allowed(previous.state, state) {
            return Err(format!(
                "transition {} -> {} is not allowed",
                previous.state.as_str(),
                state.as_str()
            ));
        }
        if let Some(hash) = candidate_hash {
            validate_hash(hash)?;
        }
        if let Some(path) = candidate_system_path {
            validate_store_path(path)?;
        }
        if state == ChangeState::Built
            && (candidate_hash.is_none() || candidate_system_path.is_none())
        {
            return Err("built state requires candidate hash and system store path".into());
        }
        if state == ChangeState::TestActivated && previous.candidate_system_path.is_none() {
            return Err("test activation requires a recorded candidate system path".into());
        }
        let entry = JournalEntry {
            id: previous.id.clone(),
            state,
            timestamp: now_epoch_seconds()?,
            source_hash: previous.source_hash.clone(),
            candidate_hash: candidate_hash
                .map(str::to_owned)
                .or_else(|| previous.candidate_hash.clone()),
            previous_system_path: previous.previous_system_path.clone(),
            candidate_system_path: candidate_system_path
                .map(str::to_owned)
                .or_else(|| previous.candidate_system_path.clone()),
            detail: detail.map(str::to_owned),
        };
        self.append(&entry)?;
        entries.insert(id.to_owned(), entry.clone());
        Ok(entry)
    }

    /// The most recent entry per change id, ordered by id (ids sort chronologically).
    pub fn latest_first(&self) -> Result<Vec<JournalEntry>, String> {
        let mut entries = self.entries()?.into_values().collect::<Vec<_>>();
        entries.reverse();
        Ok(entries)
    }

    fn append(&self, entry: &JournalEntry) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("could not create journal directory: {error}"))?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&self.path)
            .map_err(|error| format!("could not open journal for append: {error}"))?;
        writeln!(file, "{}", encode_event(entry))
            .map_err(|error| format!("could not append journal: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("could not sync journal: {error}"))
    }
}

fn transition_allowed(from: ChangeState, to: ChangeState) -> bool {
    use ChangeState::*;
    matches!(
        (from, to),
        (Planned, Built | Failed)
            | (Built, SourceApplied | Failed)
            | (
                SourceApplied,
                TestActivated | RebootPending | RollbackStarted | Failed
            )
            | (TestActivated, Verified | RollbackStarted | Failed)
            | (Verified, Switched | RollbackStarted | Failed)
            | (Switched, RollbackStarted | Failed)
            | (RebootPending, Verified | RollbackStarted | Failed)
            | (RollbackStarted, RolledBack | Failed)
    )
}

pub(crate) fn validate_change_id(value: &str) -> Result<(), String> {
    validate_id(value)
}

fn validate_id(value: &str) -> Result<(), String> {
    if value.is_empty()
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        Err("journal id must contain only ASCII letters, digits, '-' or '_'".into())
    } else {
        Ok(())
    }
}

fn validate_detail(value: &str) -> Result<(), String> {
    if value.len() > 200 || !value.chars().all(|c| c.is_ascii_graphic() || c == ' ') {
        Err("journal detail must be at most 200 printable ASCII characters".into())
    } else {
        Ok(())
    }
}

fn validate_hash(value: &str) -> Result<(), String> {
    if value.len() != 64 || !value.bytes().all(|c| c.is_ascii_hexdigit()) {
        Err("journal source/candidate hash must be a 64-character hexadecimal SHA-256".into())
    } else {
        Ok(())
    }
}

fn validate_store_path(value: &str) -> Result<(), String> {
    if !value.starts_with("/nix/store/") || value.contains('\n') || value.contains('\t') {
        Err("journal system path must be a single-line Nix store path".into())
    } else {
        Ok(())
    }
}

fn now_epoch_seconds() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| "system clock is before Unix epoch".into())
}

fn encode_event(entry: &JournalEntry) -> String {
    let timestamp = entry.timestamp.to_string();
    let fields = [
        entry.id.as_str(),
        entry.state.as_str(),
        timestamp.as_str(),
        entry.source_hash.as_str(),
        entry.candidate_hash.as_deref().unwrap_or(""),
        entry.previous_system_path.as_deref().unwrap_or(""),
        entry.candidate_system_path.as_deref().unwrap_or(""),
        entry.detail.as_deref().unwrap_or(""),
    ];
    fields
        .iter()
        .map(|field| hex_encode(field.as_bytes()))
        .collect::<Vec<_>>()
        .join("\t")
}

fn decode_event(line: &str) -> Result<JournalEntry, String> {
    let values = line
        .split('\t')
        .map(hex_decode)
        .collect::<Result<Vec<_>, _>>()?;
    if values.len() != 8 {
        return Err("expected 8 tab-separated fields".into());
    }
    let text = values
        .iter()
        .map(|value| String::from_utf8(value.clone()).map_err(|_| "field is not UTF-8".to_owned()))
        .collect::<Result<Vec<_>, _>>()?;
    let entry = JournalEntry {
        id: text[0].clone(),
        state: ChangeState::parse(&text[1])?,
        timestamp: text[2]
            .parse()
            .map_err(|_| "invalid timestamp".to_owned())?,
        source_hash: text[3].clone(),
        candidate_hash: nonempty(&text[4]),
        previous_system_path: nonempty(&text[5]),
        candidate_system_path: nonempty(&text[6]),
        detail: nonempty(&text[7]),
    };
    validate_id(&entry.id)?;
    if let Some(detail) = entry.detail.as_deref() {
        validate_detail(detail)?;
    }
    validate_hash(&entry.source_hash)?;
    if let Some(hash) = entry.candidate_hash.as_deref() {
        validate_hash(hash)?;
    }
    if let Some(path) = entry.previous_system_path.as_deref() {
        validate_store_path(path)?;
    }
    if let Some(path) = entry.candidate_system_path.as_deref() {
        validate_store_path(path)?;
    }
    Ok(entry)
}

fn nonempty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

fn hex_encode(value: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(value.len() * 2);
    for byte in value {
        result.push(char::from(HEX[usize::from(byte >> 4)]));
        result.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    result
}

fn hex_decode(value: &str) -> Result<Vec<u8>, String> {
    if value.len() % 2 != 0 {
        return Err("hex field has odd length".into());
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |value: u8| match value {
                b'0'..=b'9' => Ok(value - b'0'),
                b'a'..=b'f' => Ok(value - b'a' + 10),
                b'A'..=b'F' => Ok(value - b'A' + 10),
                _ => Err("invalid hex field".to_owned()),
            };
            Ok((digit(pair[0])? << 4) | digit(pair[1])?)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{ChangeState, Journal};
    use crate::fsutil::testutil::TempDir;
    use std::fs;
    use std::io::Write;

    const OLD: &str = "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-old-system";
    const NEW: &str = "/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-new-system";

    fn begin(journal: &Journal, id: &str) {
        journal.begin(id, &"a".repeat(64), OLD).unwrap();
    }

    fn built(journal: &Journal, id: &str) {
        journal
            .transition(
                id,
                ChangeState::Built,
                Some(&"b".repeat(64)),
                Some(NEW),
                None,
            )
            .unwrap();
    }

    #[test]
    fn journal_replays_durable_state_transitions_without_configuration_values() {
        let dir = TempDir::new("journal");
        let path = dir.path().join("journal");
        let journal = Journal::new(&path);
        begin(&journal, "change_1");
        built(&journal, "change_1");
        for state in [ChangeState::SourceApplied, ChangeState::TestActivated] {
            journal
                .transition("change_1", state, None, None, None)
                .unwrap();
        }
        let entries = journal.entries().unwrap();
        assert_eq!(entries["change_1"].state, ChangeState::TestActivated);
        assert_eq!(
            entries["change_1"].previous_system_path.as_deref(),
            Some(OLD)
        );
        assert_eq!(
            entries["change_1"].candidate_system_path.as_deref(),
            Some(NEW)
        );
        let raw = fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("password"));
    }

    #[test]
    fn the_journal_file_is_private_to_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new("journal");
        let path = dir.path().join("journal");
        begin(&Journal::new(&path), "change_7");
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o077, 0);
    }

    #[test]
    fn journal_refuses_skipping_safety_gates() {
        let dir = TempDir::new("journal");
        let journal = Journal::new(dir.path().join("journal"));
        begin(&journal, "change_2");
        for state in [
            ChangeState::Switched,
            ChangeState::TestActivated,
            ChangeState::SourceApplied,
        ] {
            assert!(
                journal
                    .transition("change_2", state, None, None, None)
                    .is_err(),
                "{state:?} must not follow planned"
            );
        }
        built(&journal, "change_2");
        // A switch needs a successful test activation and health verification first.
        for state in [
            ChangeState::TestActivated,
            ChangeState::Verified,
            ChangeState::Switched,
            ChangeState::RebootPending,
        ] {
            assert!(
                journal
                    .transition("change_2", state, None, None, None)
                    .is_err(),
                "{state:?} must not follow built"
            );
        }
    }

    #[test]
    fn terminal_states_cannot_be_reopened_and_rollback_is_recorded() {
        let dir = TempDir::new("journal");
        let journal = Journal::new(dir.path().join("journal"));
        begin(&journal, "change_4");
        built(&journal, "change_4");
        for (state, detail) in [
            (ChangeState::SourceApplied, None),
            (ChangeState::RollbackStarted, Some("health-check-failed")),
            (ChangeState::RolledBack, Some("health-check-failed")),
        ] {
            journal
                .transition("change_4", state, None, None, detail)
                .unwrap();
        }
        let entry = &journal.entries().unwrap()["change_4"];
        assert_eq!(entry.detail.as_deref(), Some("health-check-failed"));
        assert!(!entry.state.is_in_flight());
        assert!(
            journal
                .transition("change_4", ChangeState::SourceApplied, None, None, None)
                .is_err()
        );
        assert!(
            journal
                .transition("change_4", ChangeState::Failed, None, None, None)
                .is_err()
        );
    }

    #[test]
    fn in_flight_states_are_exactly_those_that_may_have_mutated_the_system() {
        use ChangeState::*;
        for state in [
            SourceApplied,
            TestActivated,
            Verified,
            RebootPending,
            RollbackStarted,
        ] {
            assert!(state.is_in_flight(), "{state:?}");
        }
        for state in [Planned, Built, Switched, RolledBack, Failed] {
            assert!(!state.is_in_flight(), "{state:?}");
        }
    }

    #[test]
    fn details_must_be_short_printable_codes() {
        let dir = TempDir::new("journal");
        let journal = Journal::new(dir.path().join("journal"));
        begin(&journal, "change_5");
        assert!(
            journal
                .transition(
                    "change_5",
                    ChangeState::Failed,
                    None,
                    None,
                    Some("multi\nline")
                )
                .is_err()
        );
        assert!(
            journal
                .transition(
                    "change_5",
                    ChangeState::Failed,
                    None,
                    None,
                    Some(&"x".repeat(201))
                )
                .is_err()
        );
        journal
            .transition(
                "change_5",
                ChangeState::Failed,
                None,
                None,
                Some("build-failed"),
            )
            .unwrap();
    }

    #[test]
    fn journal_ignores_only_a_torn_final_append_after_restart() {
        let dir = TempDir::new("journal");
        let path = dir.path().join("journal");
        let journal = Journal::new(&path);
        begin(&journal, "change_3");
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"partial-event")
            .unwrap();
        assert_eq!(
            journal.entries().unwrap()["change_3"].state,
            ChangeState::Planned
        );
    }

    #[test]
    fn corrupted_complete_lines_are_an_error_not_silently_skipped() {
        let dir = TempDir::new("journal");
        let path = dir.path().join("journal");
        let journal = Journal::new(&path);
        begin(&journal, "change_6");
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"zz\n")
            .unwrap();
        assert!(journal.entries().is_err());
    }

    #[test]
    fn latest_first_lists_newest_change_ids_first() {
        let dir = TempDir::new("journal");
        let journal = Journal::new(dir.path().join("journal"));
        begin(&journal, "chg-001");
        begin(&journal, "chg-002");
        let ids = journal
            .latest_first()
            .unwrap()
            .into_iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>();
        assert_eq!(ids, ["chg-002", "chg-001"]);
    }
}
