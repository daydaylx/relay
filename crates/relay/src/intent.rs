//! Typed intents: the only form in which a frontend (CLI flags, a script, or an optional AI
//! provider) can ask Relay for a change. Anything that does not match the strict schema is
//! rejected before it can reach the renderer; the model's output is never authoritative.
//!
//! ```json
//! {"schema":1,"changes":[
//!   {"op":"set_option","option":"hardware.bluetooth.enable","value":true},
//!   {"op":"add_package","package":"vlc"},
//!   {"op":"remove_package","package":"vlc"}]}
//! ```
//!
//! A frontend that speaks natural language (see `ai.rs`) may also *propose* an action instead of
//! changes. Proposals are only ever shown to a person; nothing here executes anything.
//!
//! ```json
//! {"schema":1,"action":"undo"}
//! {"schema":1,"action":"unsupported","reason":"I cannot change the bootloader."}
//! ```

use std::collections::BTreeMap;

use crate::change::{Change, Value, validate};
use crate::json::Json;
use crate::json_string;

const SCHEMA_VERSION: u64 = 1;
const MAX_INTENT_BYTES: usize = 64 * 1024;
const MAX_CHANGES: usize = 32;

/// What a frontend asks for. `Changes` go through the full plan/apply pipeline; the rest are
/// requests that a person must carry out (or confirm) explicitly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Proposal {
    Changes(Vec<Change>),
    Action(Action),
    /// The proposer declined; `reason` is short, printable and shown as quoted text.
    Unsupported(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Undo,
    Recover,
    Status,
    History,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Undo => "undo",
            Self::Recover => "recover",
            Self::Status => "status",
            Self::History => "history",
        }
    }
}

const MAX_REASON_CHARS: usize = 200;

/// Parse and validate an intent document that must contain changes. Protected resources are
/// rejected here as well.
pub fn parse_intent(text: &str) -> Result<Vec<Change>, String> {
    match parse_proposal(text)? {
        Proposal::Changes(changes) => Ok(changes),
        _ => Err("intent must contain changes".into()),
    }
}

/// Parse a strict proposal: exactly one of `changes` or `action`.
pub fn parse_proposal(text: &str) -> Result<Proposal, String> {
    if text.len() > MAX_INTENT_BYTES {
        return Err("intent is larger than 64 KiB".into());
    }
    let document = Json::parse(text).map_err(|_| "intent is not valid JSON".to_owned())?;
    let root = document
        .as_object()
        .ok_or_else(|| "intent must be a JSON object".to_owned())?;
    only_keys(root, &["schema", "changes", "action", "reason"], "intent")?;
    let schema = root
        .get("schema")
        .and_then(Json::as_u64)
        .ok_or_else(|| "intent.schema must be a non-negative integer".to_owned())?;
    if schema != SCHEMA_VERSION {
        return Err(format!(
            "unsupported intent schema {schema}; expected {SCHEMA_VERSION}"
        ));
    }
    match (root.get("changes"), root.get("action")) {
        (Some(changes), None) => {
            if root.contains_key("reason") {
                return Err("intent.reason is only allowed with action \"unsupported\"".into());
            }
            changes_from_json(changes).map(Proposal::Changes)
        }
        (None, Some(action)) => {
            let action = action
                .as_str()
                .ok_or_else(|| "intent.action must be a string".to_owned())?;
            let reason = root.get("reason");
            match action {
                "unsupported" => {
                    let reason = reason
                        .and_then(Json::as_str)
                        .ok_or_else(|| "action \"unsupported\" needs a reason".to_owned())?;
                    if reason.is_empty()
                        || reason.chars().count() > MAX_REASON_CHARS
                        || reason.chars().any(char::is_control)
                    {
                        return Err(
                            "reason must be 1-200 characters without control characters".into()
                        );
                    }
                    Ok(Proposal::Unsupported(reason.to_owned()))
                }
                other => {
                    if reason.is_some() {
                        return Err(
                            "intent.reason is only allowed with action \"unsupported\"".into()
                        );
                    }
                    let action = match other {
                        "undo" => Action::Undo,
                        "recover" => Action::Recover,
                        "status" => Action::Status,
                        "history" => Action::History,
                        _ => return Err("unknown action".into()),
                    };
                    Ok(Proposal::Action(action))
                }
            }
        }
        (Some(_), Some(_)) => {
            Err("intent must contain either changes or an action, not both".into())
        }
        (None, None) => Err("intent needs changes or an action".into()),
    }
}

pub(crate) fn changes_from_json(value: &Json) -> Result<Vec<Change>, String> {
    let list = value
        .as_array()
        .ok_or_else(|| "changes must be a JSON array".to_owned())?;
    if list.is_empty() {
        return Err("changes must not be empty".into());
    }
    if list.len() > MAX_CHANGES {
        return Err(format!(
            "at most {MAX_CHANGES} changes are allowed per intent"
        ));
    }
    list.iter()
        .enumerate()
        .map(|(index, item)| {
            let change =
                change_from_json(item).map_err(|error| format!("change {index}: {error}"))?;
            validate(&change).map_err(|error| format!("change {index}: {error}"))?;
            Ok(change)
        })
        .collect()
}

fn change_from_json(item: &Json) -> Result<Change, String> {
    let object = item
        .as_object()
        .ok_or_else(|| "must be an object".to_owned())?;
    let operation = object
        .get("op")
        .and_then(Json::as_str)
        .ok_or_else(|| "op must be a string".to_owned())?;
    match operation {
        "set_option" => {
            only_keys(object, &["op", "option", "value"], "set_option")?;
            Ok(Change::SetOption {
                name: string_field(object, "option")?,
                value: value_from_json(
                    object
                        .get("value")
                        .ok_or_else(|| "value is required".to_owned())?,
                )?,
            })
        }
        "add_package" => {
            only_keys(object, &["op", "package"], "add_package")?;
            Ok(Change::AddPackage {
                name: string_field(object, "package")?,
            })
        }
        "remove_package" => {
            only_keys(object, &["op", "package"], "remove_package")?;
            Ok(Change::RemovePackage {
                name: string_field(object, "package")?,
            })
        }
        _ => Err("unknown op (expected set_option, add_package or remove_package)".into()),
    }
}

fn value_from_json(value: &Json) -> Result<Value, String> {
    match value {
        Json::Bool(value) => Ok(Value::Bool(*value)),
        Json::Number(_) => value
            .as_i64()
            .map(Value::Integer)
            .ok_or_else(|| "value must be a 64-bit integer".to_owned()),
        Json::String(value) => Ok(Value::String(value.clone())),
        Json::Array(items) => items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "list values must be strings".to_owned())
            })
            .collect::<Result<_, _>>()
            .map(Value::StringList),
        _ => Err("value must be a boolean, integer, string or list of strings".into()),
    }
}

fn string_field(object: &BTreeMap<String, Json>, key: &str) -> Result<String, String> {
    object
        .get(key)
        .and_then(Json::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("{key} must be a string"))
}

fn only_keys(object: &BTreeMap<String, Json>, allowed: &[&str], label: &str) -> Result<(), String> {
    match object.keys().find(|key| !allowed.contains(&key.as_str())) {
        Some(_) => Err(format!("{label} contains an unknown field")),
        None => Ok(()),
    }
}

/// Serialise changes in the intent schema's `changes` array form (used by plan records).
pub(crate) fn changes_to_json(changes: &[Change]) -> String {
    let items = changes
        .iter()
        .map(|change| match change {
            Change::SetOption { name, value } => format!(
                "{{\"op\":\"set_option\",\"option\":{},\"value\":{}}}",
                json_string(name),
                value_to_json(value)
            ),
            Change::AddPackage { name } => {
                format!(
                    "{{\"op\":\"add_package\",\"package\":{}}}",
                    json_string(name)
                )
            }
            Change::RemovePackage { name } => {
                format!(
                    "{{\"op\":\"remove_package\",\"package\":{}}}",
                    json_string(name)
                )
            }
        })
        .collect::<Vec<_>>();
    format!("[{}]", items.join(","))
}

fn value_to_json(value: &Value) -> String {
    match value {
        Value::Bool(value) => value.to_string(),
        Value::Integer(value) => value.to_string(),
        Value::String(value) => json_string(value),
        Value::StringList(values) => format!(
            "[{}]",
            values
                .iter()
                .map(|value| json_string(value))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{changes_from_json, changes_to_json, parse_intent};
    use crate::change::{Change, Value};
    use crate::json::Json;

    fn intent(changes: &str) -> String {
        format!("{{\"schema\":1,\"changes\":{changes}}}")
    }

    #[test]
    fn parses_the_documented_mvp_intents() {
        let parsed = parse_intent(&intent(
            r#"[{"op":"set_option","option":"hardware.bluetooth.enable","value":true},
                {"op":"add_package","package":"vlc"},
                {"op":"remove_package","package":"htop"},
                {"op":"set_option","option":"services.x.port","value":-3},
                {"op":"set_option","option":"services.x.list","value":["a","b"]}]"#,
        ))
        .unwrap();
        assert_eq!(parsed.len(), 5);
        assert_eq!(
            parsed[0],
            Change::SetOption {
                name: "hardware.bluetooth.enable".into(),
                value: Value::Bool(true)
            }
        );
        assert_eq!(
            parsed[3],
            Change::SetOption {
                name: "services.x.port".into(),
                value: Value::Integer(-3)
            }
        );
    }

    #[test]
    fn malformed_model_intents_are_rejected_before_rendering() {
        for (label, text) in [
            ("not json", "enable bluetooth please".to_owned()),
            (
                "truncated",
                r#"{"schema":1,"changes":[{"op":"add_pa"#.to_owned(),
            ),
            ("wrong root", "[]".to_owned()),
            (
                "missing schema",
                r#"{"changes":[{"op":"add_package","package":"vlc"}]}"#.to_owned(),
            ),
            (
                "future schema",
                r#"{"schema":2,"changes":[{"op":"add_package","package":"vlc"}]}"#.to_owned(),
            ),
            ("missing changes", r#"{"schema":1}"#.to_owned()),
            ("empty changes", intent("[]")),
            ("unknown op", intent(r#"[{"op":"run_shell","cmd":"id"}]"#)),
            (
                "extra field",
                intent(r#"[{"op":"add_package","package":"vlc","raw":"x"}]"#),
            ),
            (
                "extra root field",
                r#"{"schema":1,"changes":[{"op":"add_package","package":"vlc"}],"nix":"{}"}"#
                    .to_owned(),
            ),
            (
                "float value",
                intent(r#"[{"op":"set_option","option":"a.b","value":1.5}]"#),
            ),
            (
                "huge integer",
                intent(r#"[{"op":"set_option","option":"a.b","value":99999999999999999999}]"#),
            ),
            (
                "null value",
                intent(r#"[{"op":"set_option","option":"a.b","value":null}]"#),
            ),
            (
                "object value",
                intent(r#"[{"op":"set_option","option":"a.b","value":{"x":1}}]"#),
            ),
            (
                "mixed list",
                intent(r#"[{"op":"set_option","option":"a.b","value":["a",1]}]"#),
            ),
            (
                "nix injection in option",
                intent(r#"[{"op":"set_option","option":"a.b; import /tmp/x","value":true}]"#),
            ),
            (
                "nix injection in package",
                intent(r#"[{"op":"add_package","package":"x; builtins.abort"}]"#),
            ),
            (
                "non-string package",
                intent(r#"[{"op":"add_package","package":7}]"#),
            ),
        ] {
            assert!(parse_intent(&text).is_err(), "accepted {label}");
        }
    }

    #[test]
    fn protected_resources_cannot_be_smuggled_in_through_an_intent() {
        for option in [
            "system.stateVersion",
            "boot.loader.grub.enable",
            "users.users.root.hashedPassword",
        ] {
            let error = parse_intent(&intent(&format!(
                r#"[{{"op":"set_option","option":"{option}","value":true}}]"#
            )))
            .unwrap_err();
            assert!(error.contains("protected"), "{option}: {error}");
        }
    }

    #[test]
    fn proposals_are_either_changes_or_one_known_action_never_both() {
        use super::{Action, Proposal, parse_proposal};
        assert_eq!(
            parse_proposal(r#"{"schema":1,"action":"undo"}"#).unwrap(),
            Proposal::Action(Action::Undo)
        );
        for (text, action) in [
            ("recover", Action::Recover),
            ("status", Action::Status),
            ("history", Action::History),
        ] {
            assert_eq!(
                parse_proposal(&format!(r#"{{"schema":1,"action":"{text}"}}"#)).unwrap(),
                Proposal::Action(action)
            );
        }
        assert_eq!(
            parse_proposal(
                r#"{"schema":1,"action":"unsupported","reason":"I cannot change the bootloader."}"#
            )
            .unwrap(),
            Proposal::Unsupported("I cannot change the bootloader.".into())
        );
        for bad in [
            r#"{"schema":1,"action":"reboot"}"#,
            r#"{"schema":1,"action":"apply"}"#,
            r#"{"schema":1,"action":"unsupported"}"#,
            r#"{"schema":1,"action":"unsupported","reason":""}"#,
            r#"{"schema":1,"action":"undo","reason":"because"}"#,
            r#"{"schema":1,"action":"unsupported","reason":"line\nbreak"}"#,
            r#"{"schema":1,"action":7}"#,
            r#"{"schema":1,"action":"undo","changes":[{"op":"add_package","package":"vlc"}]}"#,
            r#"{"schema":1,"changes":[{"op":"add_package","package":"vlc"}],"reason":"x"}"#,
            r#"{"schema":1}"#,
        ] {
            assert!(parse_proposal(bad).is_err(), "accepted {bad}");
        }
        let long = format!(
            r#"{{"schema":1,"action":"unsupported","reason":"{}"}}"#,
            "x".repeat(201)
        );
        assert!(parse_proposal(&long).is_err());
        // A document without changes is not an intent for `plan`.
        assert!(parse_intent(r#"{"schema":1,"action":"undo"}"#).is_err());
    }

    #[test]
    fn size_and_count_limits_apply() {
        let many = format!(
            "[{}]",
            vec![r#"{"op":"add_package","package":"vlc"}"#; 33].join(",")
        );
        assert!(parse_intent(&intent(&many)).is_err());
        let big = format!(
            r#"{{"schema":1,"changes":[{{"op":"set_option","option":"a.b","value":"{}"}}]}}"#,
            "x".repeat(70 * 1024)
        );
        assert!(parse_intent(&big).is_err());
    }

    #[test]
    fn plan_records_round_trip_through_the_same_schema() {
        let changes = vec![
            Change::AddPackage { name: "vlc".into() },
            Change::SetOption {
                name: "services.x.msg".into(),
                value: Value::String("quote\" and \\ and \n newline".into()),
            },
            Change::SetOption {
                name: "services.x.list".into(),
                value: Value::StringList(vec!["a".into(), "b".into()]),
            },
            Change::RemovePackage {
                name: "htop".into(),
            },
        ];
        let json = changes_to_json(&changes);
        assert_eq!(
            changes_from_json(&Json::parse(&json).unwrap()).unwrap(),
            changes
        );
    }
}
