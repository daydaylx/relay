use std::io::{self, BufRead, Write};
use std::process::{Command, Stdio};

use relay::protocol::{
    Action, MAX_REQUEST_BYTES, error_response, is_json, parse_request, success_response,
};

const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

pub fn run_stdio() -> Result<(), String> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    let mut line = Vec::new();
    loop {
        match read_line_limited(&mut input, &mut line, MAX_REQUEST_BYTES) {
            Ok(false) => return Ok(()),
            Ok(true) => {}
            Err(error) => return Err(format!("protocol input failed: {error}")),
        }
        let response = if line.len() > MAX_REQUEST_BYTES {
            error_response(None, "invalid_request", "request exceeds the size limit")
        } else {
            match std::str::from_utf8(&line) {
                Ok(text) => match parse_request(text) {
                    Ok(request) => dispatch(&request),
                    Err(error) => error_response(error.id.as_deref(), error.code, error.message),
                },
                Err(_) => error_response(None, "invalid_request", "request is not UTF-8 JSON"),
            }
        };
        output
            .write_all(response.as_bytes())
            .map_err(|error| format!("protocol output failed: {error}"))?;
        output
            .write_all(b"\n")
            .map_err(|error| format!("protocol output failed: {error}"))?;
        output
            .flush()
            .map_err(|error| format!("protocol output failed: {error}"))?;
    }
}

fn read_line_limited<R: BufRead>(
    reader: &mut R,
    line: &mut Vec<u8>,
    limit: usize,
) -> io::Result<bool> {
    line.clear();
    let mut oversized = false;
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            if oversized {
                line.resize(limit + 1, 0);
            }
            return Ok(!line.is_empty() || oversized);
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let count = newline.map_or(available.len(), |index| index + 1);
        if !oversized {
            if line.len().saturating_add(count) > limit {
                oversized = true;
                line.clear();
            } else {
                line.extend_from_slice(&available[..count]);
            }
        }
        let ended = newline.is_some();
        reader.consume(count);
        if ended {
            if oversized {
                line.resize(limit + 1, 0);
            }
            return Ok(true);
        }
    }
}

fn dispatch(request: &relay::protocol::Request) -> String {
    if relay::execution::authorize(request).is_err() {
        return error_response(
            Some(&request.id),
            "confirmation_required",
            "a direct user confirmation is required by the Relay execution gateway",
        );
    }
    if matches!(request.action, Action::SearchOption | Action::SearchPackage) {
        return search_nix(request);
    }
    match run_core(request) {
        Ok(output)
            if request.action == Action::Show
                && output.status.success()
                && output.stdout.len() <= MAX_RESPONSE_BYTES =>
        {
            let data = String::from_utf8_lossy(&output.stdout);
            let wrapped = format!("{{\"review\":{}}}", relay::json_string(data.trim()));
            success_response(&request.id, &wrapped)
        }
        Ok(output)
            if output.stdout.len() <= MAX_RESPONSE_BYTES
                && is_json(std::str::from_utf8(&output.stdout).unwrap_or("")) =>
        {
            let data = String::from_utf8_lossy(&output.stdout);
            if output.status.success()
                || matches!(
                    request.action,
                    Action::Apply | Action::Undo | Action::Recover
                )
            {
                success_response(&request.id, data.trim())
            } else {
                error_response(
                    Some(&request.id),
                    "core_error",
                    "Relay Core rejected or could not complete the request",
                )
            }
        }
        Ok(_) => error_response(
            Some(&request.id),
            "invalid_core_response",
            "Relay Core returned an invalid response",
        ),
        Err(()) => error_response(
            Some(&request.id),
            "core_error",
            "Relay Core rejected or could not complete the request",
        ),
    }
}

fn search_nix(request: &relay::protocol::Request) -> String {
    let Some(flake) = request.flake.as_deref() else {
        return error_response(
            Some(&request.id),
            "invalid_request",
            "search flake is required",
        );
    };
    let Some(host) = request.host.as_deref() else {
        return error_response(
            Some(&request.id),
            "invalid_request",
            "search host is required",
        );
    };
    let Some(query) = request.query.as_deref() else {
        return error_response(
            Some(&request.id),
            "invalid_request",
            "search query is required",
        );
    };
    let kind = if request.action == Action::SearchOption {
        relay::IndexKind::Options
    } else {
        relay::IndexKind::Packages
    };
    let result = relay::NixAdapter::default()
        .generate_index_json(std::path::Path::new(flake), host, kind)
        .map_err(|_| ())
        .and_then(|json| relay::SearchIndex::from_json(&json, kind.as_str()).map_err(|_| ()))
        .map(|index| {
            let entries = index
                .find(query)
                .into_iter()
                .take(request.unit_limit.min(20))
                .map(|entry| {
                    format!(
                        "{{\"name\":{},\"type\":{},\"description\":{},\"read_only\":{}}}",
                        relay::json_string(&entry.name),
                        entry
                            .type_name
                            .as_deref()
                            .map(relay::json_string)
                            .unwrap_or_else(|| "null".to_owned()),
                        relay::json_string(
                            &entry.description.chars().take(1200).collect::<String>()
                        ),
                        entry.read_only,
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{{\"kind\":{},\"results\":[{}]}}",
                relay::json_string(kind.as_str()),
                entries
            )
        });
    match result {
        Ok(data) if data.len() <= MAX_RESPONSE_BYTES => success_response(&request.id, &data),
        Ok(_) => error_response(
            Some(&request.id),
            "output_limit",
            "Nix search result exceeds the size limit",
        ),
        Err(()) => error_response(
            Some(&request.id),
            "search_failed",
            "Nix option/package search failed or returned an invalid index",
        ),
    }
}

fn run_core(request: &relay::protocol::Request) -> Result<std::process::Output, ()> {
    let executable = std::env::current_exe().map_err(|_| ())?;
    let mut command = Command::new(executable);
    match request.action {
        Action::Status => {
            command.arg("status");
            push_option(&mut command, "--root", request.root.as_deref());
            push_option(&mut command, "--flake", request.flake.as_deref());
            push_option(&mut command, "--state-dir", request.state_dir.as_deref());
        }
        Action::Health => {
            command.arg("health");
        }
        Action::Units => {
            command
                .arg("units")
                .args(["--limit", &request.unit_limit.to_string()]);
            if !request.unit_filter.is_empty() {
                command.args(["--filter", &request.unit_filter]);
            }
        }
        Action::Generations => {
            command.arg("generations");
            push_option(&mut command, "--root", request.root.as_deref());
        }
        Action::Diagnose => {
            command
                .arg("diagnose")
                .args(["--topic", request.diagnostic_topic.as_deref().ok_or(())?]);
            push_option(&mut command, "--root", request.root.as_deref());
            if !request.unit_filter.is_empty() {
                command.args(["--filter", &request.unit_filter]);
            }
            push_option(&mut command, "--unit", request.diagnostic_unit.as_deref());
            command.args(["--limit", &request.unit_limit.to_string()]);
        }
        Action::SearchOption | Action::SearchPackage => return Err(()),
        Action::Plan => {
            command.arg("plan");
            push_option(&mut command, "--root", request.root.as_deref());
            push_option(&mut command, "--flake", request.flake.as_deref());
            push_option(&mut command, "--host", request.host.as_deref());
            push_option(&mut command, "--state-dir", request.state_dir.as_deref());
            command.args(["--intent", "-"]).stdin(Stdio::piped());
        }
        Action::Show => {
            command.arg("show");
            if let Some(id) = request.change_id.as_deref() {
                command.arg(id);
            }
            push_option(&mut command, "--root", request.root.as_deref());
            push_option(&mut command, "--state-dir", request.state_dir.as_deref());
        }
        Action::Discard => {
            command.arg("discard");
            if let Some(id) = request.change_id.as_deref() {
                command.arg(id);
            }
            push_option(&mut command, "--root", request.root.as_deref());
            push_option(&mut command, "--state-dir", request.state_dir.as_deref());
        }
        Action::UndoPreview => {
            command.arg("undo-preview");
            push_option(&mut command, "--root", request.root.as_deref());
            push_option(&mut command, "--state-dir", request.state_dir.as_deref());
        }
        Action::RecoverPreview => {
            command.arg("recover-preview");
            push_option(&mut command, "--root", request.root.as_deref());
            push_option(&mut command, "--state-dir", request.state_dir.as_deref());
        }
        Action::Apply => {
            command.arg("apply");
            if let Some(id) = request.change_id.as_deref() {
                command.arg(id);
            }
            command.arg("--yes");
            push_option(&mut command, "--root", request.root.as_deref());
            push_option(&mut command, "--state-dir", request.state_dir.as_deref());
            for unit in &request.expect_active {
                command.args(["--expect-active", unit]);
            }
            if let Some(seconds) = request.observe_seconds {
                command.args(["--observe", &seconds.to_string()]);
            }
            if request.no_desktop_check {
                command.arg("--no-desktop-check");
            }
        }
        Action::Undo => {
            command.arg("undo").arg("--yes");
            push_option(
                &mut command,
                "--expect-change-id",
                request.change_id.as_deref(),
            );
            push_option(&mut command, "--root", request.root.as_deref());
            push_option(&mut command, "--state-dir", request.state_dir.as_deref());
            if let Some(seconds) = request.observe_seconds {
                command.args(["--observe", &seconds.to_string()]);
            }
        }
        Action::Recover => {
            command.arg("recover");
            push_option(&mut command, "--root", request.root.as_deref());
            push_option(&mut command, "--state-dir", request.state_dir.as_deref());
            if request.abort_pending {
                command.arg("--abort-pending");
            }
            for id in &request.expected_ids {
                command.args(["--expect-pending", id]);
            }
            if let Some(seconds) = request.observe_seconds {
                command.args(["--observe", &seconds.to_string()]);
            }
        }
    }
    command.stdout(Stdio::piped()).stderr(Stdio::null());
    let output = if request.action == Action::Plan {
        let mut child = command.spawn().map_err(|_| ())?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(request.intent_json.as_deref().ok_or(())?.as_bytes())
                .map_err(|_| ())?;
        }
        child.wait_with_output().map_err(|_| ())?
    } else {
        command.output().map_err(|_| ())?
    };
    if output.stdout.len() > MAX_RESPONSE_BYTES {
        return Err(());
    }
    Ok(output)
}

fn push_option(command: &mut Command, name: &str, value: Option<&str>) {
    if let Some(value) = value {
        command.arg(name).arg(value);
    }
}

#[cfg(test)]
mod tests {
    use super::{dispatch, read_line_limited};
    use relay::protocol::{Action, Request};
    use std::io::Cursor;

    fn request(action: Action, confirmed: bool) -> Request {
        Request {
            id: "gateway-test".into(),
            action,
            root: None,
            flake: None,
            host: None,
            state_dir: None,
            change_id: None,
            intent_json: None,
            confirmed,
            abort_pending: false,
            no_desktop_check: false,
            observe_seconds: None,
            expect_active: Vec::new(),
            expected_ids: Vec::new(),
            unit_filter: String::new(),
            unit_limit: 20,
            diagnostic_topic: None,
            diagnostic_unit: None,
            query: None,
        }
    }

    #[test]
    fn bounded_line_reader_drains_oversized_records_and_continues() {
        let mut input = Cursor::new(b"12345\nok\n".to_vec());
        let mut line = Vec::new();
        assert!(read_line_limited(&mut input, &mut line, 4).unwrap());
        assert_eq!(line.len(), 5);
        assert!(read_line_limited(&mut input, &mut line, 4).unwrap());
        assert_eq!(line, b"ok\n");
        assert!(!read_line_limited(&mut input, &mut line, 4).unwrap());
    }

    #[test]
    fn execution_gateway_denies_unconfirmed_managed_action_before_spawning_core() {
        let response = dispatch(&request(Action::Apply, false));
        assert!(response.contains("\"code\":\"confirmation_required\""));
        assert!(response.contains("execution gateway"));
    }
}
