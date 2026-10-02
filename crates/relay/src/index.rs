use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::json_string;

const INDEX_SCHEMA_VERSION: u64 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Json {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
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
        let value = Parser::new(&contents).parse()?;
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

fn json_value(value: &Json) -> String {
    match value {
        Json::Null => "null".to_owned(),
        Json::Bool(value) => value.to_string(),
        Json::Number(value) => value.clone(),
        Json::String(value) => json_string(value),
        Json::Array(values) => format!(
            "[{}]",
            values.iter().map(json_value).collect::<Vec<_>>().join(",")
        ),
        Json::Object(values) => format!(
            "{{{}}}",
            values
                .iter()
                .map(|(key, value)| format!("{}:{}", json_string(key), json_value(value)))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

struct Parser<'a> {
    input: &'a str,
    offset: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, offset: 0 }
    }

    fn parse(mut self) -> Result<Json, String> {
        let value = self.value()?;
        self.whitespace();
        if self.offset != self.input.len() {
            return Err("unexpected trailing data in JSON index".to_owned());
        }
        Ok(value)
    }

    fn value(&mut self) -> Result<Json, String> {
        self.whitespace();
        match self.peek() {
            Some(b'{') => self.object_value(),
            Some(b'[') => self.array_value(),
            Some(b'"') => self.string_value().map(Json::String),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'n') => self.literal("null", Json::Null),
            Some(b'-' | b'0'..=b'9') => self.number_value(),
            _ => Err(self.error("expected a JSON value")),
        }
    }

    fn object_value(&mut self) -> Result<Json, String> {
        self.expect(b'{')?;
        let mut values = BTreeMap::new();
        self.whitespace();
        if self.consume(b'}') {
            return Ok(Json::Object(values));
        }
        loop {
            self.whitespace();
            let key = self.string_value()?;
            self.whitespace();
            self.expect(b':')?;
            let value = self.value()?;
            if values.insert(key.clone(), value).is_some() {
                return Err(self.error(&format!("duplicate JSON key '{key}'")));
            }
            self.whitespace();
            if self.consume(b'}') {
                break;
            }
            self.expect(b',')?;
        }
        Ok(Json::Object(values))
    }

    fn array_value(&mut self) -> Result<Json, String> {
        self.expect(b'[')?;
        let mut values = Vec::new();
        self.whitespace();
        if self.consume(b']') {
            return Ok(Json::Array(values));
        }
        loop {
            values.push(self.value()?);
            self.whitespace();
            if self.consume(b']') {
                break;
            }
            self.expect(b',')?;
        }
        Ok(Json::Array(values))
    }

    fn string_value(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut result = String::new();
        loop {
            let Some(byte) = self.peek() else {
                return Err(self.error("unterminated JSON string"));
            };
            match byte {
                b'"' => {
                    self.offset += 1;
                    return Ok(result);
                }
                b'\\' => {
                    self.offset += 1;
                    let escaped = self.peek().ok_or_else(|| self.error("incomplete escape"))?;
                    self.offset += 1;
                    match escaped {
                        b'"' => result.push('"'),
                        b'\\' => result.push('\\'),
                        b'/' => result.push('/'),
                        b'b' => result.push('\u{0008}'),
                        b'f' => result.push('\u{000c}'),
                        b'n' => result.push('\n'),
                        b'r' => result.push('\r'),
                        b't' => result.push('\t'),
                        b'u' => result.push(self.unicode_escape()?),
                        _ => return Err(self.error("invalid JSON escape")),
                    }
                }
                0x00..=0x1f => return Err(self.error("control character in JSON string")),
                _ => {
                    let character = self.input[self.offset..]
                        .chars()
                        .next()
                        .ok_or_else(|| self.error("invalid UTF-8"))?;
                    result.push(character);
                    self.offset += character.len_utf8();
                }
            }
        }
    }

    fn unicode_escape(&mut self) -> Result<char, String> {
        let high = self.unicode_unit()?;
        let codepoint = match high {
            0xd800..=0xdbff => {
                if self.input.get(self.offset..self.offset + 2) != Some("\\u") {
                    return Err(self.error("high surrogate without low surrogate"));
                }
                self.offset += 2;
                let low = self.unicode_unit()?;
                if !(0xdc00..=0xdfff).contains(&low) {
                    return Err(self.error("invalid low surrogate"));
                }
                0x10000 + ((u32::from(high) - 0xd800) << 10) + (u32::from(low) - 0xdc00)
            }
            0xdc00..=0xdfff => return Err(self.error("unexpected low surrogate")),
            _ => u32::from(high),
        };
        char::from_u32(codepoint).ok_or_else(|| self.error("invalid unicode codepoint"))
    }

    fn unicode_unit(&mut self) -> Result<u16, String> {
        let end = self.offset + 4;
        let digits = self
            .input
            .get(self.offset..end)
            .ok_or_else(|| self.error("incomplete unicode escape"))?;
        let unit =
            u16::from_str_radix(digits, 16).map_err(|_| self.error("invalid unicode escape"))?;
        self.offset = end;
        Ok(unit)
    }

    fn number_value(&mut self) -> Result<Json, String> {
        let start = self.offset;
        self.consume(b'-');
        match self.peek() {
            Some(b'0') => {
                self.offset += 1;
                if matches!(self.peek(), Some(b'0'..=b'9')) {
                    return Err(self.error("leading zero in JSON number"));
                }
            }
            Some(b'1'..=b'9') => self.digits(),
            _ => return Err(self.error("invalid JSON number")),
        }
        if self.consume(b'.') {
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.error("fraction requires digits"));
            }
            self.digits();
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.offset += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.offset += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.error("exponent requires digits"));
            }
            self.digits();
        }
        Ok(Json::Number(self.input[start..self.offset].to_owned()))
    }

    fn digits(&mut self) {
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.offset += 1;
        }
    }

    fn literal(&mut self, literal: &str, value: Json) -> Result<Json, String> {
        if self.input[self.offset..].starts_with(literal) {
            self.offset += literal.len();
            Ok(value)
        } else {
            Err(self.error("invalid JSON literal"))
        }
    }

    fn whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.offset += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), String> {
        if self.consume(byte) {
            Ok(())
        } else {
            Err(self.error(&format!("expected '{}'", char::from(byte))))
        }
    }

    fn consume(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.offset += 1;
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.as_bytes().get(self.offset).copied()
    }

    fn error(&self, message: &str) -> String {
        format!("invalid JSON index at byte {}: {message}", self.offset)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{Parser, SearchIndex};

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
        assert_eq!(parsed, super::Json::String("🚀".to_owned()));
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
