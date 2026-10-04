//! Versioned, typed request envelope used by local frontends such as `relay-agent`.

use crate::health::validate_unit_name;
use crate::json::{Json, json_value};
use crate::parse_intent;

pub const PROTOCOL_VERSION: u64 = 1;
pub const MAX_REQUEST_BYTES: usize = 70 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Status,
    Health,
    Units,
    Generations,
    Diagnose,
    SearchOption,
    SearchPackage,
    Observe,
    Plan,
    Show,
    Discard,
    Apply,
    UndoPreview,
    Undo,
    RecoverPreview,
    Recover,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Health => "health",
            Self::Units => "units",
            Self::Generations => "generations",
            Self::Diagnose => "diagnose",
            Self::SearchOption => "search_option",
            Self::SearchPackage => "search_package",
            Self::Observe => "observe",
            Self::Plan => "plan",
            Self::Show => "show",
            Self::Discard => "discard",
            Self::Apply => "apply",
            Self::UndoPreview => "undo_preview",
            Self::Undo => "undo",
            Self::RecoverPreview => "recover_preview",
            Self::Recover => "recover",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub id: String,
    pub action: Action,
    pub root: Option<String>,
    pub flake: Option<String>,
    pub host: Option<String>,
    pub state_dir: Option<String>,
    pub change_id: Option<String>,
    pub intent_json: Option<String>,
    pub confirmed: bool,
    pub abort_pending: bool,
    pub no_desktop_check: bool,
    pub observe_seconds: Option<u64>,
    pub expect_active: Vec<String>,
    pub expected_ids: Vec<String>,
    pub unit_filter: String,
    pub unit_limit: usize,
    pub diagnostic_topic: Option<String>,
    pub diagnostic_unit: Option<String>,
    pub query: Option<String>,
    pub observe_program: Option<String>,
    pub observe_args: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestError {
    pub id: Option<String>,
    pub code: &'static str,
    pub message: &'static str,
}

impl RequestError {
    fn invalid(id: Option<String>, message: &'static str) -> Self {
        Self {
            id,
            code: "invalid_request",
            message,
        }
    }
}

pub fn parse_request(input: &str) -> Result<Request, RequestError> {
    if input.len() > MAX_REQUEST_BYTES {
        return Err(RequestError::invalid(
            None,
            "request exceeds the size limit",
        ));
    }
    let value =
        Json::parse(input).map_err(|_| RequestError::invalid(None, "request is not valid JSON"))?;
    let object = value
        .as_object()
        .ok_or_else(|| RequestError::invalid(None, "request must be an object"))?;
    let id = object
        .get("id")
        .and_then(Json::as_str)
        .filter(|id| {
            !id.is_empty()
                && id.len() <= 128
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._:-".contains(c))
        })
        .map(str::to_owned);
    let Some(id) = id else {
        return Err(RequestError::invalid(None, "request id is invalid"));
    };
    if object
        .keys()
        .any(|key| !["schema_version", "id", "action", "params"].contains(&key.as_str()))
    {
        return Err(RequestError::invalid(
            Some(id),
            "request contains an unknown field",
        ));
    }
    let version = object
        .get("schema_version")
        .and_then(Json::as_u64)
        .ok_or_else(|| {
            RequestError::invalid(Some(id.clone()), "schema_version must be an integer")
        })?;
    if version != PROTOCOL_VERSION {
        return Err(RequestError {
            id: Some(id),
            code: "unsupported_version",
            message: "unsupported protocol version",
        });
    }
    let action_text = object
        .get("action")
        .and_then(Json::as_str)
        .ok_or_else(|| RequestError::invalid(Some(id.clone()), "action must be a string"))?;
    let action = match action_text {
        "status" => Action::Status,
        "health" => Action::Health,
        "units" => Action::Units,
        "generations" => Action::Generations,
        "diagnose" => Action::Diagnose,
        "search_option" => Action::SearchOption,
        "search_package" => Action::SearchPackage,
        "observe" => Action::Observe,
        "plan" => Action::Plan,
        "show" => Action::Show,
        "discard" => Action::Discard,
        "apply" => Action::Apply,
        "undo_preview" => Action::UndoPreview,
        "undo" => Action::Undo,
        "recover_preview" => Action::RecoverPreview,
        "recover" => Action::Recover,
        _ => {
            return Err(RequestError {
                id: Some(id),
                code: "unsupported_action",
                message: "action is not available in this protocol version",
            });
        }
    };
    let params = object
        .get("params")
        .and_then(Json::as_object)
        .ok_or_else(|| RequestError::invalid(Some(id.clone()), "params must be an object"))?;
    let permitted: &[&str] = match action {
        Action::Status => &["root", "flake", "state_dir"],
        Action::Health => &[],
        Action::Units => &["filter", "limit"],
        Action::Generations => &["root"],
        Action::Diagnose => &["root", "topic", "filter", "unit", "limit"],
        Action::SearchOption | Action::SearchPackage => &["flake", "host", "query", "limit"],
        Action::Observe => &["program", "args"],
        Action::Plan => &["root", "flake", "host", "state_dir", "intent"],
        Action::Show => &["root", "state_dir", "change_id"],
        Action::Discard => &["root", "state_dir", "change_id"],
        Action::Apply => &[
            "root",
            "state_dir",
            "change_id",
            "confirmed",
            "expect_active",
            "observe_seconds",
            "no_desktop_check",
        ],
        Action::Undo => &[
            "root",
            "state_dir",
            "confirmed",
            "change_id",
            "observe_seconds",
        ],
        Action::UndoPreview => &["root", "state_dir"],
        Action::RecoverPreview => &["root", "state_dir"],
        Action::Recover => &[
            "root",
            "state_dir",
            "confirmed",
            "abort_pending",
            "observe_seconds",
            "expected_ids",
        ],
    };
    if params.keys().any(|key| !permitted.contains(&key.as_str())) {
        return Err(RequestError::invalid(
            Some(id),
            "params contain a field not allowed for this action",
        ));
    }
    let read_path = |key: &str| -> Result<Option<String>, RequestError> {
        let Some(value) = params.get(key) else {
            return Ok(None);
        };
        let Some(value) = value.as_str() else {
            return Err(RequestError::invalid(
                Some(id.clone()),
                "path parameters must be strings",
            ));
        };
        if value.is_empty() || value.len() > 4096 || value.chars().any(char::is_control) {
            return Err(RequestError::invalid(
                Some(id.clone()),
                "path parameter is invalid",
            ));
        }
        Ok(Some(value.to_owned()))
    };
    let read_name = |key: &str, max: usize| -> Result<Option<String>, RequestError> {
        let Some(value) = params.get(key) else {
            return Ok(None);
        };
        let Some(value) = value.as_str() else {
            return Err(RequestError::invalid(
                Some(id.clone()),
                "text parameters must be strings",
            ));
        };
        if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
            return Err(RequestError::invalid(
                Some(id.clone()),
                "text parameter is invalid",
            ));
        }
        Ok(Some(value.to_owned()))
    };
    let root = read_path("root")?;
    let flake = read_path("flake")?;
    let state_dir = read_path("state_dir")?;
    let host = read_name("host", 128)?;
    let change_id = read_name("change_id", 128)?;
    let unit_filter = read_name("filter", 128)?.unwrap_or_default();
    let unit_limit = match params.get("limit") {
        None => 50,
        Some(value) => match value.as_u64() {
            Some(limit) if (1..=100).contains(&limit) => limit as usize,
            _ => {
                return Err(RequestError::invalid(
                    Some(id.clone()),
                    "limit must be an integer from 1 to 100",
                ));
            }
        },
    };
    if action == Action::Diagnose && unit_limit > 50 {
        return Err(RequestError::invalid(
            Some(id.clone()),
            "diagnostic limit must not exceed 50",
        ));
    }
    let diagnostic_topic = read_name("topic", 32)?;
    if diagnostic_topic.as_deref().is_some_and(|topic| {
        !matches!(
            topic,
            "network" | "bluetooth" | "hardware" | "processes" | "journal" | "desktop" | "package"
        )
    }) {
        return Err(RequestError::invalid(
            Some(id.clone()),
            "diagnostic topic is unsupported",
        ));
    }
    let diagnostic_unit = read_name("unit", 128)?;
    let query = read_name("query", 128)?;
    let observe_program = read_name("program", 128)?;
    if observe_program.as_deref().is_some_and(|program| {
        !program
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
    }) {
        return Err(RequestError::invalid(
            Some(id.clone()),
            "diagnostic program name is invalid",
        ));
    }
    let observe_args = match params.get("args") {
        None => Vec::new(),
        Some(value) => {
            let Some(values) = value.as_array() else {
                return Err(RequestError::invalid(
                    Some(id.clone()),
                    "args must be an array of strings",
                ));
            };
            if values.len() > crate::sandbox::MAX_ARGUMENTS {
                return Err(RequestError::invalid(
                    Some(id.clone()),
                    "too many diagnostic arguments",
                ));
            }
            let mut args = Vec::with_capacity(values.len());
            let mut total = 0usize;
            for value in values {
                let Some(arg) = value.as_str() else {
                    return Err(RequestError::invalid(
                        Some(id.clone()),
                        "diagnostic arguments must be strings",
                    ));
                };
                if arg.len() > crate::sandbox::MAX_ARGUMENT_BYTES || arg.contains('\0') {
                    return Err(RequestError::invalid(
                        Some(id.clone()),
                        "diagnostic argument is invalid or too large",
                    ));
                }
                total = total.saturating_add(arg.len());
                args.push(arg.to_owned());
            }
            if total > 16 * 1024 {
                return Err(RequestError::invalid(
                    Some(id.clone()),
                    "diagnostic arguments exceed the total size limit",
                ));
            }
            args
        }
    };
    if action == Action::Diagnose {
        if diagnostic_topic.as_deref() == Some("package")
            && (unit_filter.is_empty() || unit_filter.starts_with('.'))
        {
            return Err(RequestError::invalid(
                Some(id.clone()),
                "package diagnostics require a safe executable name",
            ));
        }
        if let Some(unit) = diagnostic_unit.as_deref() {
            validate_unit_name(unit).map_err(|_| {
                RequestError::invalid(Some(id.clone()), "diagnostic unit is invalid")
            })?;
        }
        let max_filter_len = if diagnostic_topic.as_deref() == Some("package") {
            128
        } else {
            64
        };
        if unit_filter.len() > max_filter_len
            || !unit_filter
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_.-+".contains(c))
        {
            return Err(RequestError::invalid(
                Some(id.clone()),
                "diagnostic filter is invalid",
            ));
        }
        if diagnostic_topic
            .as_deref()
            .is_some_and(|topic| !matches!(topic, "processes" | "package"))
            && !unit_filter.is_empty()
        {
            return Err(RequestError::invalid(
                Some(id.clone()),
                "filter is only valid for process diagnostics",
            ));
        }
        if diagnostic_topic.as_deref() != Some("journal") && diagnostic_unit.is_some() {
            return Err(RequestError::invalid(
                Some(id.clone()),
                "unit is only valid for journal diagnostics",
            ));
        }
    }
    let intent_json = match params.get("intent") {
        None => None,
        Some(value) => {
            let encoded = json_value(value);
            parse_intent(&encoded).map_err(|_| {
                RequestError::invalid(
                    Some(id.clone()),
                    "intent is invalid or contains a protected change",
                )
            })?;
            Some(encoded)
        }
    };
    let read_bool = |key: &str, default: bool| -> Result<bool, RequestError> {
        match params.get(key) {
            None => Ok(default),
            Some(value) => value.as_bool().ok_or_else(|| {
                RequestError::invalid(Some(id.clone()), "boolean parameters must be booleans")
            }),
        }
    };
    let confirmed = read_bool("confirmed", false)?;
    if matches!(action, Action::Apply | Action::Undo | Action::Recover) && !confirmed {
        return Err(RequestError {
            id: Some(id),
            code: "confirmation_required",
            message: "a direct user confirmation is required",
        });
    }
    let abort_pending = read_bool("abort_pending", false)?;
    let no_desktop_check = read_bool("no_desktop_check", false)?;
    let observe_seconds = match params.get("observe_seconds") {
        None => None,
        Some(value) => match value.as_u64() {
            Some(seconds) if seconds <= 600 => Some(seconds),
            _ => {
                return Err(RequestError::invalid(
                    Some(id.clone()),
                    "observe_seconds must be an integer from 0 to 600",
                ));
            }
        },
    };
    let expected_ids = match params.get("expected_ids") {
        None => Vec::new(),
        Some(value) => {
            let Some(values) = value.as_array() else {
                return Err(RequestError::invalid(
                    Some(id.clone()),
                    "expected_ids must be an array",
                ));
            };
            if values.len() > 32 {
                return Err(RequestError::invalid(
                    Some(id.clone()),
                    "too many expected ids",
                ));
            }
            let mut ids = Vec::with_capacity(values.len());
            for value in values {
                let Some(change_id) = value.as_str() else {
                    return Err(RequestError::invalid(
                        Some(id.clone()),
                        "expected ids must be strings",
                    ));
                };
                if change_id.is_empty()
                    || change_id.len() > 128
                    || !change_id
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "._:-".contains(c))
                {
                    return Err(RequestError::invalid(
                        Some(id.clone()),
                        "expected id is invalid",
                    ));
                }
                ids.push(change_id.to_owned());
            }
            ids.sort();
            ids.dedup();
            ids
        }
    };
    let expect_active = match params.get("expect_active") {
        None => Vec::new(),
        Some(value) => {
            let Some(values) = value.as_array() else {
                return Err(RequestError::invalid(
                    Some(id.clone()),
                    "expect_active must be an array of unit names",
                ));
            };
            if values.len() > 32 {
                return Err(RequestError::invalid(
                    Some(id.clone()),
                    "too many expected units",
                ));
            }
            let mut units = Vec::with_capacity(values.len());
            for value in values {
                let Some(unit) = value.as_str() else {
                    return Err(RequestError::invalid(
                        Some(id.clone()),
                        "expected unit names must be strings",
                    ));
                };
                validate_unit_name(unit).map_err(|_| {
                    RequestError::invalid(Some(id.clone()), "expected unit name is invalid")
                })?;
                units.push(unit.to_owned());
            }
            units
        }
    };
    let required = match action {
        Action::Plan => flake.is_some() && host.is_some() && intent_json.is_some(),
        Action::Show | Action::Discard | Action::Apply | Action::Undo => change_id.is_some(),
        Action::Recover => params.contains_key("expected_ids") && !expected_ids.is_empty(),
        Action::UndoPreview
        | Action::RecoverPreview
        | Action::Status
        | Action::Health
        | Action::Units
        | Action::Generations => true,
        Action::Diagnose => diagnostic_topic.is_some(),
        Action::SearchOption | Action::SearchPackage => {
            flake.is_some() && host.is_some() && query.is_some()
        }
        Action::Observe => observe_program.is_some(),
    };
    if !required {
        return Err(RequestError::invalid(
            Some(id),
            "required action parameters are missing",
        ));
    }
    Ok(Request {
        id,
        action,
        root,
        flake,
        host,
        state_dir,
        change_id,
        intent_json,
        confirmed,
        abort_pending,
        no_desktop_check,
        observe_seconds,
        expect_active,
        expected_ids,
        unit_filter,
        unit_limit,
        diagnostic_topic,
        diagnostic_unit,
        query,
        observe_program,
        observe_args,
    })
}

pub fn success_response(id: &str, result_json: &str) -> String {
    format!(
        "{{\"schema_version\":{PROTOCOL_VERSION},\"id\":{},\"ok\":true,\"data\":{result_json}}}",
        crate::json_string(id)
    )
}

pub fn is_json(input: &str) -> bool {
    Json::parse(input).is_ok()
}

pub fn error_response(id: Option<&str>, code: &str, message: &str) -> String {
    let id = id
        .map(crate::json_string)
        .unwrap_or_else(|| "null".to_owned());
    format!(
        "{{\"schema_version\":{PROTOCOL_VERSION},\"id\":{id},\"ok\":false,\"error\":{{\"code\":{},\"message\":{}}}}}",
        crate::json_string(code),
        crate::json_string(message)
    )
}

#[cfg(test)]
mod tests {
    use super::{Action, error_response, parse_request, success_response};

    #[test]
    fn accepts_only_versioned_typed_read_and_plan_requests() {
        let request = parse_request(r#"{"schema_version":1,"id":"p1","action":"plan","params":{"flake":"/etc/nixos","host":"nixos","intent":{"schema":1,"changes":[{"op":"add_package","package":"vlc"}]}}}"#).unwrap();
        assert_eq!(request.action, Action::Plan);
        assert_eq!(request.host.as_deref(), Some("nixos"));
        assert!(request.intent_json.unwrap().contains("add_package"));
    }

    #[test]
    fn option_and_package_search_are_typed_bounded_read_actions() {
        let option = parse_request(r#"{"schema_version":1,"id":"o","action":"search_option","params":{"flake":"/etc/nixos","host":"nixos","query":"hardware.bluetooth","limit":5}}"#).unwrap();
        assert_eq!(option.action, Action::SearchOption);
        assert_eq!(option.query.as_deref(), Some("hardware.bluetooth"));
        assert_eq!(option.unit_limit, 5);
        assert!(!option.confirmed);

        let package = parse_request(r#"{"schema_version":1,"id":"p","action":"search_package","params":{"flake":"/etc/nixos","host":"nixos","query":"video player"}}"#).unwrap();
        assert_eq!(package.action, Action::SearchPackage);
        assert_eq!(package.unit_limit, 50);

        for input in [
            r#"{"schema_version":1,"id":"x","action":"search_option","params":{"host":"nixos","query":"bluetooth"}}"#,
            r#"{"schema_version":1,"id":"x","action":"search_package","params":{"flake":"/etc/nixos","host":"nixos","query":""}}"#,
            r#"{"schema_version":1,"id":"x","action":"search_package","params":{"flake":"/etc/nixos","host":"nixos","query":"vlc","limit":101}}"#,
        ] {
            assert_eq!(parse_request(input).unwrap_err().code, "invalid_request");
        }
    }

    #[test]
    fn observe_accepts_only_a_bounded_program_and_argument_array() {
        let request = parse_request(r#"{"schema_version":1,"id":"o","action":"observe","params":{"program":"journalctl","args":["--boot","-n","20"]}}"#).unwrap();
        assert_eq!(request.action, Action::Observe);
        assert_eq!(request.observe_program.as_deref(), Some("journalctl"));
        assert_eq!(request.observe_args, ["--boot", "-n", "20"]);
        for input in [
            r#"{"schema_version":1,"id":"o","action":"observe","params":{"program":"bash -c id"}}"#,
            r#"{"schema_version":1,"id":"o","action":"observe","params":{"program":"journalctl","args":"--boot"}}"#,
            r#"{"schema_version":1,"id":"o","action":"observe","params":{"program":"journalctl","env":{"PATH":"/tmp"}}}"#,
        ] {
            assert_eq!(parse_request(input).unwrap_err().code, "invalid_request");
        }
    }

    #[test]
    fn rejects_unknown_fields_protected_intents_versions_and_actions() {
        let cases = [
            (
                r#"{"schema_version":1,"id":"a","action":"status","params":{},"shell":"id"}"#,
                "invalid_request",
            ),
            (
                r#"{"schema_version":2,"id":"a","action":"status","params":{}}"#,
                "unsupported_version",
            ),
            (
                r#"{"schema_version":1,"id":"a","action":"exec","params":{}}"#,
                "unsupported_action",
            ),
            (
                r#"{"schema_version":1,"id":"a","action":"health","params":{"root":"/"}}"#,
                "invalid_request",
            ),
            (
                r#"{"schema_version":1,"id":"a","action":"plan","params":{"flake":"/x","host":"h","intent":{"schema":1,"changes":[{"op":"set_option","option":"system.stateVersion","value":"26.05"}]}}}"#,
                "invalid_request",
            ),
        ];
        for (input, code) in cases {
            assert_eq!(parse_request(input).unwrap_err().code, code);
        }
    }

    #[test]
    fn response_envelopes_preserve_request_id_and_raw_json_data() {
        assert_eq!(
            success_response("x", "{\"healthy\":true}"),
            r#"{"schema_version":1,"id":"x","ok":true,"data":{"healthy":true}}"#
        );
        assert!(error_response(Some("x"), "core_error", "request failed").contains("\"id\":\"x\""));
    }

    #[test]
    fn unit_queries_accept_only_bounded_filters_and_limits() {
        let valid = r#"{"schema_version":1,"id":"u","action":"units","params":{"filter":"nginx","limit":12}}"#;
        let request = parse_request(valid).unwrap();
        assert_eq!(request.unit_filter, "nginx");
        assert_eq!(request.unit_limit, 12);

        let excessive = r#"{"schema_version":1,"id":"u","action":"units","params":{"limit":101}}"#;
        assert_eq!(
            parse_request(excessive).unwrap_err().code,
            "invalid_request"
        );
        let injected =
            r#"{"schema_version":1,"id":"u","action":"units","params":{"filter":"x --no-pager"}}"#;
        assert_eq!(parse_request(injected).unwrap().unit_filter, "x --no-pager");
    }

    #[test]
    fn diagnostics_are_read_only_and_accept_only_supported_topics_and_filters() {
        let valid = r#"{"schema_version":1,"id":"d","action":"diagnose","params":{"topic":"journal","unit":"nginx.service","limit":10}}"#;
        let request = parse_request(valid).unwrap();
        assert_eq!(request.diagnostic_topic.as_deref(), Some("journal"));
        assert_eq!(request.diagnostic_unit.as_deref(), Some("nginx.service"));
        assert!(!request.confirmed);

        for input in [
            r#"{"schema_version":1,"id":"d","action":"diagnose","params":{"topic":"exec"}}"#,
            r#"{"schema_version":1,"id":"d","action":"diagnose","params":{"topic":"journal","unit":"--follow"}}"#,
            r#"{"schema_version":1,"id":"d","action":"diagnose","params":{"topic":"network","unit":"nginx.service"}}"#,
            r#"{"schema_version":1,"id":"d","action":"diagnose","params":{"topic":"journal","limit":100}}"#,
        ] {
            assert_eq!(parse_request(input).unwrap_err().code, "invalid_request");
        }
    }

    #[test]
    fn mutation_requests_require_direct_confirmation_and_typed_arguments() {
        let no_confirmation =
            r#"{"schema_version":1,"id":"a","action":"apply","params":{"change_id":"change-1"}}"#;
        assert_eq!(
            parse_request(no_confirmation).unwrap_err().code,
            "confirmation_required"
        );

        let valid = r#"{"schema_version":1,"id":"a","action":"apply","params":{"change_id":"change-1","confirmed":true,"expect_active":["bluetooth.service"]}}"#;
        let request = parse_request(valid).unwrap();
        assert!(request.confirmed);
        assert_eq!(request.expect_active, ["bluetooth.service"]);

        let malformed_unit = r#"{"schema_version":1,"id":"a","action":"apply","params":{"change_id":"change-1","confirmed":true,"expect_active":["--help"]}}"#;
        assert_eq!(
            parse_request(malformed_unit).unwrap_err().code,
            "invalid_request"
        );
        let stale_unsafe_undo =
            r#"{"schema_version":1,"id":"a","action":"undo","params":{"confirmed":true}}"#;
        assert_eq!(
            parse_request(stale_unsafe_undo).unwrap_err().code,
            "invalid_request"
        );
        let recovery = r#"{"schema_version":1,"id":"a","action":"recover","params":{"confirmed":true,"expected_ids":["plan-1"]}}"#;
        assert!(parse_request(recovery).unwrap().confirmed);
    }
}
