use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::json::{Json, Parser, json_value};
use crate::json_string;

const INDEX_SCHEMA_VERSION: u64 = 1;

/// A search hit as plain data (the JSON form is only for display).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexEntry {
    pub name: String,
    /// NixOS option type description (`None` for package entries).
    pub type_name: Option<String>,
    pub description: String,
    pub read_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchIndex {
    pub kind: String,
    pub host: String,
    pub nixpkgs_revision: String,
    pub lockfile_hash: String,
    pub config_identity: String,
    pub target_system: String,
    entries: Vec<BTreeMap<String, Json>>,
}

impl SearchIndex {
    pub fn read(path: &Path, expected_kind: &str) -> Result<Self, String> {
        let contents = fs::read_to_string(path)
            .map_err(|error| format!("could not read index {}: {error}", path.display()))?;
        Self::from_json(&contents, expected_kind)
    }

    pub fn from_json(contents: &str, expected_kind: &str) -> Result<Self, String> {
        let value = Parser::new(contents).parse()?;
        let root = object(&value, "index root")?;
        let schema_version = number(required(root, "schemaVersion")?, "schemaVersion")?;
        if schema_version != INDEX_SCHEMA_VERSION {
            return Err(format!(
                "unsupported index schema version {schema_version}; expected {INDEX_SCHEMA_VERSION}"
            ));
        }
        let kind = string(required(root, "kind")?, "kind")?.to_owned();
        if kind != expected_kind {
            return Err(format!(
                "index kind is '{kind}', but this command requires '{expected_kind}'"
            ));
        }
        let host = nonempty_string(root, "host")?;
        let nixpkgs_revision = nonempty_string(root, "nixpkgsRevision")?;
        let lockfile_hash = nonempty_string(root, "lockfileHash")?;
        let config_identity = nonempty_string(root, "configIdentity")?;
        let target_system = nonempty_string(root, "targetSystem")?;
        let entries = array(required(root, "entries")?, "entries")?
            .iter()
            .map(|entry| {
                let entry = object(entry, "entry")?;
                let name = string(required(entry, "name")?, "entry.name")?;
                if name.trim().is_empty() {
                    return Err("entry.name must not be empty".to_owned());
                }
                if expected_kind == "option" {
                    for field in [
                        "type",
                        "default",
                        "description",
                        "example",
                        "declarations",
                        "readOnly",
                        "relatedPackages",
                    ] {
                        if !entry.contains_key(field) {
                            return Err(format!(
                                "option entry is missing required field '{field}'"
                            ));
                        }
                    }
                    string(required(entry, "type")?, "entry.type")?;
                    string(required(entry, "description")?, "entry.description")?;
                    array(required(entry, "declarations")?, "entry.declarations")?;
                    boolean(required(entry, "readOnly")?, "entry.readOnly")?;
                    array(required(entry, "relatedPackages")?, "entry.relatedPackages")?;
                } else {
                    string(required(entry, "description")?, "entry.description")?;
                }
                Ok(entry.clone())
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self {
            kind,
            host,
            nixpkgs_revision,
            lockfile_hash,
            config_identity,
            target_system,
            entries,
        })
    }

    pub fn search(&self, query: &str) -> Vec<String> {
        let query = query.to_lowercase();
        self.entries
            .iter()
            .filter(|entry| {
                entry
                    .get("name")
                    .and_then(|value| string(value, "entry.name").ok())
                    .is_some_and(|name| name.to_lowercase().contains(&query))
            })
            .map(|entry| json_value(&Json::Object(entry.clone())))
            .collect()
    }

    /// The entry with exactly this name.
    pub fn entry(&self, name: &str) -> Option<IndexEntry> {
        self.entries
            .iter()
            .find(|entry| entry.get("name").and_then(Json::as_str) == Some(name))
            .map(entry_summary)
    }

    /// Entries whose name contains `query` (case-insensitive), in index order.
    pub fn find(&self, query: &str) -> Vec<IndexEntry> {
        let query = query.to_lowercase();
        self.entries
            .iter()
            .filter(|entry| {
                entry
                    .get("name")
                    .and_then(Json::as_str)
                    .is_some_and(|name| name.to_lowercase().contains(&query))
            })
            .map(entry_summary)
            .collect()
    }

    pub fn validate_host(&self, expected_host: &str) -> Result<(), String> {
        if self.host != expected_host {
            return Err(format!(
                "index host '{}' does not match running host '{}'",
                self.host, expected_host
            ));
        }
        Ok(())
    }

    pub fn validate_identity(&self, current: &Self) -> Result<(), String> {
        if self.kind != current.kind {
            return Err("index kind does not match current source identity".into());
        }
        for (label, stored, now) in [
            ("host", &self.host, &current.host),
            (
                "nixpkgs revision",
                &self.nixpkgs_revision,
                &current.nixpkgs_revision,
            ),
            (
                "flake.lock hash",
                &self.lockfile_hash,
                &current.lockfile_hash,
            ),
            (
                "configuration identity",
                &self.config_identity,
                &current.config_identity,
            ),
            ("target system", &self.target_system, &current.target_system),
        ] {
            if stored != now {
                return Err(format!("index is stale: {label} has changed"));
            }
        }
        Ok(())
    }

    pub fn metadata_json(&self) -> String {
        format!(
            "{{\"kind\":{},\"host\":{},\"nixpkgsRevision\":{},\"lockfileHash\":{},\"configIdentity\":{},\"targetSystem\":{}}}",
            json_string(&self.kind),
            json_string(&self.host),
            json_string(&self.nixpkgs_revision),
            json_string(&self.lockfile_hash),
            json_string(&self.config_identity),
            json_string(&self.target_system),
        )
    }
}

fn entry_summary(entry: &BTreeMap<String, Json>) -> IndexEntry {
    let text = |key: &str| entry.get(key).and_then(Json::as_str).map(str::to_owned);
    IndexEntry {
        name: text("name").unwrap_or_default(),
        type_name: text("type"),
        description: text("description").unwrap_or_default(),
        read_only: entry.get("readOnly") == Some(&Json::Bool(true)),
    }
}

fn required<'a>(object: &'a BTreeMap<String, Json>, key: &str) -> Result<&'a Json, String> {
    object
        .get(key)
        .ok_or_else(|| format!("index is missing required field '{key}'"))
}

fn object<'a>(value: &'a Json, label: &str) -> Result<&'a BTreeMap<String, Json>, String> {
    match value {
        Json::Object(object) => Ok(object),
        _ => Err(format!("{label} must be a JSON object")),
    }
}

fn array<'a>(value: &'a Json, label: &str) -> Result<&'a [Json], String> {
    match value {
        Json::Array(array) => Ok(array),
        _ => Err(format!("{label} must be a JSON array")),
    }
}

fn string<'a>(value: &'a Json, label: &str) -> Result<&'a str, String> {
    match value {
        Json::String(value) => Ok(value),
        _ => Err(format!("{label} must be a string")),
    }
}

fn boolean(value: &Json, label: &str) -> Result<bool, String> {
    match value {
        Json::Bool(value) => Ok(*value),
        _ => Err(format!("{label} must be a boolean")),
    }
}

fn number(value: &Json, label: &str) -> Result<u64, String> {
    match value {
        Json::Number(value) => value
            .parse()
            .map_err(|_| format!("{label} must be a non-negative integer")),
        _ => Err(format!("{label} must be a number")),
    }
}

fn nonempty_string(object: &BTreeMap<String, Json>, key: &str) -> Result<String, String> {
    let value = string(required(object, key)?, key)?;
    if value.trim().is_empty() {
        return Err(format!("{key} must not be empty"));
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::SearchIndex;
    use crate::json::{Json, Parser};

    struct TempIndex(PathBuf);

    impl TempIndex {
        fn new(contents: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir()
                .join(format!("relay-index-{}-{nonce}.json", std::process::id()));
            fs::write(&path, contents).unwrap();
            Self(path)
        }
    }

    impl Drop for TempIndex {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    const OPTION_INDEX: &str = r#"{
        "schemaVersion":1,"kind":"option","host":"test-host",
        "nixpkgsRevision":"rev","lockfileHash":"sha256:lock",
        "configIdentity":"sha256:config","targetSystem":"x86_64-linux",
        "entries":[{"name":"services.bluetooth.enable","type":"boolean",
        "default":false,"description":"Enable Bluetooth","example":true,
        "declarations":["nixos/modules/services/hardware/bluetooth.nix"],
        "readOnly":false,"relatedPackages":[]}]
    }"#;

    #[test]
    fn reads_versioned_option_index_and_searches_case_insensitively() {
        let fixture = TempIndex::new(OPTION_INDEX);
        let index = SearchIndex::read(&fixture.0, "option").unwrap();
        let matches = index.search("BLUETOOTH");
        assert_eq!(matches.len(), 1);
        assert!(matches[0].contains("services.bluetooth.enable"));
        assert_eq!(index.host, "test-host");
        assert!(index.validate_host("test-host").is_ok());
        assert!(index.validate_host("other-host").is_err());
    }

    #[test]
    fn rejects_cache_when_any_identity_field_changes() {
        let index = SearchIndex::from_json(OPTION_INDEX, "option").unwrap();
        let current = SearchIndex::from_json(OPTION_INDEX, "option").unwrap();
        assert!(index.validate_identity(&current).is_ok());
        let changed = OPTION_INDEX.replace("sha256:config", "sha256:other");
        let current = SearchIndex::from_json(&changed, "option").unwrap();
        assert!(
            index
                .validate_identity(&current)
                .unwrap_err()
                .contains("configuration identity")
        );
    }

    #[test]
    fn structured_lookup_returns_typed_entries_for_validation() {
        let index = SearchIndex::from_json(OPTION_INDEX, "option").unwrap();
        let entry = index.entry("services.bluetooth.enable").unwrap();
        assert_eq!(entry.type_name.as_deref(), Some("boolean"));
        assert!(!entry.read_only);
        assert_eq!(index.find("BLUE").len(), 1);
        assert!(index.entry("services.bluetooth").is_none());
        assert!(index.find("nothing-like-this").is_empty());
    }

    #[test]
    fn searches_package_index_by_name() {
        let fixture = TempIndex::new(
            r#"{"schemaVersion":1,"kind":"package","host":"h","nixpkgsRevision":"r","lockfileHash":"l","configIdentity":"c","targetSystem":"x86_64-linux","entries":[{"name":"vlc","description":"Media player"}]}"#,
        );
        let index = SearchIndex::read(&fixture.0, "package").unwrap();
        let matches = index.search("vl");
        assert_eq!(matches.len(), 1);
        assert!(matches[0].contains("Media player"));
    }

    #[test]
    fn parses_escaped_non_bmp_unicode_and_rejects_invalid_numbers() {
        let parsed = Parser::new(r#""\uD83D\uDE80""#).parse().unwrap();
        assert_eq!(parsed, Json::String("🚀".to_owned()));
        for invalid in ["01", "1.", "1e", "+1"] {
            assert!(Parser::new(invalid).parse().is_err(), "accepted {invalid}");
        }
    }

    #[test]
    fn rejects_wrong_kind_and_duplicate_json_keys() {
        let fixture = TempIndex::new(OPTION_INDEX);
        assert!(
            SearchIndex::read(&fixture.0, "package")
                .unwrap_err()
                .contains("requires 'package'")
        );
        let duplicate = TempIndex::new(r#"{"schemaVersion":1,"schemaVersion":1}"#);
        assert!(
            SearchIndex::read(&duplicate.0, "option")
                .unwrap_err()
                .contains("duplicate JSON key")
        );
    }

    #[test]
    fn rejects_missing_option_schema_fields() {
        let fixture = TempIndex::new(
            r#"{"schemaVersion":1,"kind":"option","host":"h","nixpkgsRevision":"r","lockfileHash":"l","configIdentity":"c","targetSystem":"s","entries":[{"name":"x"}]}"#,
        );
        assert!(
            SearchIndex::read(&fixture.0, "option")
                .unwrap_err()
                .contains("missing required field 'type'")
        );
    }
}
