//! Hyprland integration (target state T6): *read-only* access to the running compositor and a
//! desktop health check that joins the safety loop of `apply`.
//!
//! Relay talks to Hyprland's own IPC socket (`$XDG_RUNTIME_DIR/hypr/<signature>/.socket.sock`)
//! with a fixed allow-list of JSON queries. There is deliberately no way to send `dispatch`,
//! `keyword`, `reload`, `exec` or any other command that changes the compositor: controlling the
//! desktop is automation and needs its own recovery design. Window titles are never read into
//! Relay's summaries (they can contain private data).

use std::io::{Read, Write};
use std::os::unix::fs::FileTypeExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::json::Json;

const IO_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_RESPONSE_BYTES: u64 = 8 * 1024 * 1024;

/// The only requests Relay ever sends to the compositor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Query {
    Version,
    Monitors,
    Workspaces,
    ActiveWindow,
    ConfigErrors,
}

impl Query {
    fn request(self) -> &'static str {
        match self {
            Self::Version => "j/version",
            Self::Monitors => "j/monitors",
            Self::Workspaces => "j/workspaces",
            Self::ActiveWindow => "j/activewindow",
            Self::ConfigErrors => "j/configerrors",
        }
    }
}

/// A compositor that can answer read-only queries.
pub(crate) trait DesktopIpc: Send + Sync {
    fn query(&self, query: Query) -> Result<Json, String>;
}

#[derive(Clone, Debug)]
pub struct HyprlandIpc {
    socket: PathBuf,
}

impl HyprlandIpc {
    /// Find the running session's socket from its environment, if there is one.
    pub fn from_env(get: &dyn Fn(&str) -> Option<String>) -> Option<Self> {
        let signature = get("HYPRLAND_INSTANCE_SIGNATURE")?;
        if signature.is_empty()
            || !signature
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return None;
        }
        let mut candidates = Vec::new();
        if let Some(runtime) = get("XDG_RUNTIME_DIR").filter(|dir| !dir.is_empty()) {
            candidates.push(PathBuf::from(runtime).join("hypr").join(&signature));
        }
        candidates.push(PathBuf::from("/tmp/hypr").join(&signature));
        candidates
            .into_iter()
            .map(|directory| directory.join(".socket.sock"))
            .find(|socket| {
                std::fs::symlink_metadata(socket).is_ok_and(|meta| meta.file_type().is_socket())
            })
            .map(|socket| Self { socket })
    }

    pub fn at(socket: impl Into<PathBuf>) -> Self {
        Self {
            socket: socket.into(),
        }
    }

    pub fn socket(&self) -> &Path {
        &self.socket
    }

    /// Monitors, workspaces, window count and configuration errors (no window titles).
    pub fn summary(&self) -> Result<DesktopSummary, String> {
        summarize(self)
    }
}

impl DesktopIpc for HyprlandIpc {
    fn query(&self, query: Query) -> Result<Json, String> {
        let mut stream = UnixStream::connect(&self.socket)
            .map_err(|error| format!("could not reach the compositor: {error}"))?;
        stream
            .set_read_timeout(Some(IO_TIMEOUT))
            .and_then(|()| stream.set_write_timeout(Some(IO_TIMEOUT)))
            .map_err(|error| format!("could not configure the compositor socket: {error}"))?;
        stream
            .write_all(query.request().as_bytes())
            .map_err(|error| format!("could not query the compositor: {error}"))?;
        let mut response = Vec::new();
        stream
            .take(MAX_RESPONSE_BYTES)
            .read_to_end(&mut response)
            .map_err(|error| format!("could not read the compositor's answer: {error}"))?;
        let text = String::from_utf8(response)
            .map_err(|_| "the compositor answered with non-UTF-8 data".to_owned())?;
        parse_response(&text)
    }
}

/// Hyprland pretty-prints JSON; raw control characters inside strings (odd window titles) would
/// make a strict parser fail, so they are replaced by spaces before parsing.
fn parse_response(text: &str) -> Result<Json, String> {
    let mut cleaned = String::with_capacity(text.len());
    let (mut in_string, mut escaped) = (false, false);
    for c in text.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            } else if c.is_control() {
                cleaned.push(' ');
                continue;
            }
        } else if c == '"' {
            in_string = true;
        }
        cleaned.push(c);
    }
    Json::parse(cleaned.trim()).map_err(|_| "the compositor's answer is not valid JSON".to_owned())
}

#[derive(Clone, Debug, PartialEq)]
pub struct MonitorInfo {
    pub name: String,
    pub width: u64,
    pub height: u64,
    pub refresh_hz: f64,
    pub focused: bool,
    pub disabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceInfo {
    pub id: i64,
    pub name: String,
    pub monitor: String,
    pub windows: u64,
}

/// What `relay desktop status` reports. No window titles.
#[derive(Clone, Debug, PartialEq)]
pub struct DesktopSummary {
    pub compositor: &'static str,
    pub version: String,
    pub monitors: Vec<MonitorInfo>,
    pub workspaces: Vec<WorkspaceInfo>,
    pub window_count: u64,
    pub active_window_class: Option<String>,
    pub config_errors: Vec<String>,
}

pub(crate) fn summarize(ipc: &dyn DesktopIpc) -> Result<DesktopSummary, String> {
    let version = ipc
        .query(Query::Version)?
        .get("version")
        .and_then(Json::as_str)
        .unwrap_or("unknown")
        .to_owned();
    let monitors = monitors(ipc)?;
    let workspaces = workspaces(ipc)?;
    let window_count = workspaces.iter().map(|workspace| workspace.windows).sum();
    let active_window_class = ipc.query(Query::ActiveWindow).ok().and_then(|window| {
        window
            .get("class")
            .and_then(Json::as_str)
            .filter(|class| !class.is_empty())
            .map(|class| clip_text(class, 64))
    });
    Ok(DesktopSummary {
        compositor: "hyprland",
        version,
        monitors,
        workspaces,
        window_count,
        active_window_class,
        config_errors: config_errors(ipc)?,
    })
}

fn monitors(ipc: &dyn DesktopIpc) -> Result<Vec<MonitorInfo>, String> {
    let value = ipc.query(Query::Monitors)?;
    value
        .as_array()
        .ok_or_else(|| "the monitor list is not an array".to_owned())?
        .iter()
        .map(|monitor| {
            Ok(MonitorInfo {
                name: monitor
                    .get("name")
                    .and_then(Json::as_str)
                    .map(|name| clip_text(name, 64))
                    .ok_or_else(|| "a monitor has no name".to_owned())?,
                width: monitor.get("width").and_then(Json::as_u64).unwrap_or(0),
                height: monitor.get("height").and_then(Json::as_u64).unwrap_or(0),
                refresh_hz: monitor
                    .get("refreshRate")
                    .and_then(Json::as_f64)
                    .unwrap_or(0.0),
                focused: monitor
                    .get("focused")
                    .and_then(Json::as_bool)
                    .unwrap_or(false),
                disabled: monitor
                    .get("disabled")
                    .and_then(Json::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect()
}

fn workspaces(ipc: &dyn DesktopIpc) -> Result<Vec<WorkspaceInfo>, String> {
    let value = ipc.query(Query::Workspaces)?;
    value
        .as_array()
        .ok_or_else(|| "the workspace list is not an array".to_owned())?
        .iter()
        .map(|workspace| {
            Ok(WorkspaceInfo {
                id: workspace
                    .get("id")
                    .and_then(Json::as_i64)
                    .ok_or_else(|| "a workspace has no id".to_owned())?,
                name: workspace
                    .get("name")
                    .and_then(Json::as_str)
                    .map(|name| clip_text(name, 64))
                    .unwrap_or_default(),
                monitor: workspace
                    .get("monitor")
                    .and_then(Json::as_str)
                    .map(|name| clip_text(name, 64))
                    .unwrap_or_default(),
                windows: workspace.get("windows").and_then(Json::as_u64).unwrap_or(0),
            })
        })
        .collect()
}

/// Hyprland reports "no errors" as `[""]`; empty entries are dropped.
fn config_errors(ipc: &dyn DesktopIpc) -> Result<Vec<String>, String> {
    let value = ipc.query(Query::ConfigErrors)?;
    Ok(value
        .as_array()
        .ok_or_else(|| "the config error list is not an array".to_owned())?
        .iter()
        .filter_map(Json::as_str)
        .map(|error| clip_text(error, 200))
        .filter(|error| !error.is_empty())
        .collect())
}

fn clip_text(text: &str, max_chars: usize) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(max_chars)
        .collect::<String>()
        .trim()
        .to_owned()
}

// ----------------------------------------------------------------------- desktop health

/// The desktop facts a system change must not break.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopSnapshot {
    /// Enabled monitors.
    pub monitors: u64,
    pub config_errors: Vec<String>,
}

/// Something that can take a desktop snapshot (the compositor, or a test double).
pub trait DesktopProbe: Send + Sync {
    fn snapshot(&self) -> Result<DesktopSnapshot, String>;
}

impl DesktopProbe for HyprlandIpc {
    fn snapshot(&self) -> Result<DesktopSnapshot, String> {
        snapshot_of(self)
    }
}

pub(crate) fn snapshot_of(ipc: &dyn DesktopIpc) -> Result<DesktopSnapshot, String> {
    Ok(DesktopSnapshot {
        monitors: monitors(ipc)?
            .iter()
            .filter(|monitor| !monitor.disabled)
            .count() as u64,
        config_errors: config_errors(ipc)?,
    })
}

/// Problems the desktop shows now compared with `baseline`: an unresponsive compositor, lost
/// monitors, or configuration errors that were not there before.
pub fn desktop_problems(baseline: &DesktopSnapshot, probe: &dyn DesktopProbe) -> Vec<String> {
    let now = match probe.snapshot() {
        Ok(now) => now,
        Err(_) => return vec!["the compositor no longer responds".to_owned()],
    };
    let mut problems = Vec::new();
    if now.monitors < baseline.monitors {
        problems.push(format!(
            "{} monitor(s) were lost",
            baseline.monitors - now.monitors
        ));
    }
    for error in &now.config_errors {
        if !baseline.config_errors.contains(error) {
            problems.push(format!("new compositor configuration error: {error}"));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::{
        DesktopIpc, DesktopProbe, DesktopSnapshot, HyprlandIpc, Query, desktop_problems,
        parse_response, snapshot_of, summarize,
    };
    use crate::fsutil::testutil::TempDir;
    use crate::json::Json;
    use std::collections::BTreeMap;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;

    /// Canned answers per query, as Hyprland would print them.
    struct Canned(BTreeMap<&'static str, Result<&'static str, &'static str>>);

    impl DesktopIpc for Canned {
        fn query(&self, query: Query) -> Result<Json, String> {
            match self.0.get(query.request()) {
                Some(Ok(text)) => parse_response(text),
                Some(Err(error)) => Err((*error).to_owned()),
                None => Err("unexpected query".to_owned()),
            }
        }
    }

    const VERSION: &str = r#"{"branch":"v0.55.4","version":"0.55.4","tag":"v0.55.4"}"#;
    const MONITORS: &str = r#"[{"id":0,"name":"eDP-1","width":1920,"height":1080,"refreshRate":60.05200,"focused":true,"disabled":false},
        {"id":1,"name":"HDMI-A-1","width":2560,"height":1440,"refreshRate":59.95100,"focused":false,"disabled":true}]"#;
    const WORKSPACES: &str = r#"[{"id":1,"name":"1","monitor":"eDP-1","windows":2,"lastwindowtitle":"private title"},
        {"id":2,"name":"2","monitor":"eDP-1","windows":1,"lastwindowtitle":"another\tprivate"}]"#;

    fn healthy() -> Canned {
        Canned(BTreeMap::from([
            ("j/version", Ok(VERSION)),
            ("j/monitors", Ok(MONITORS)),
            ("j/workspaces", Ok(WORKSPACES)),
            (
                "j/activewindow",
                Ok(r#"{"class":"kitty","title":"secret"}"#),
            ),
            ("j/configerrors", Ok("[\n\t\"\"\n]")),
        ]))
    }

    #[test]
    fn the_summary_reports_monitors_workspaces_and_counts_but_never_titles() {
        let summary = summarize(&healthy()).unwrap();
        assert_eq!(summary.version, "0.55.4");
        assert_eq!(summary.monitors.len(), 2);
        assert_eq!(summary.monitors[0].name, "eDP-1");
        assert_eq!(
            (summary.monitors[0].width, summary.monitors[0].height),
            (1920, 1080)
        );
        assert!((summary.monitors[0].refresh_hz - 60.052).abs() < 1e-9);
        assert!(summary.monitors[0].focused && summary.monitors[1].disabled);
        assert_eq!(summary.workspaces.len(), 2);
        assert_eq!(summary.window_count, 3);
        assert_eq!(summary.active_window_class.as_deref(), Some("kitty"));
        assert!(summary.config_errors.is_empty(), "[\"\"] means no errors");
        let debug = format!("{summary:?}");
        assert!(!debug.contains("private") && !debug.contains("secret"));
    }

    #[test]
    fn health_snapshots_count_enabled_monitors_and_real_config_errors() {
        let snapshot = snapshot_of(&healthy()).unwrap();
        assert_eq!(
            snapshot,
            DesktopSnapshot {
                monitors: 1,
                config_errors: vec![]
            }
        );
        let mut broken = healthy();
        broken
            .0
            .insert("j/configerrors", Ok(r#"["config line 3: bad value", ""]"#));
        assert_eq!(
            snapshot_of(&broken).unwrap().config_errors,
            ["config line 3: bad value"]
        );
        let mut unreachable = healthy();
        unreachable.0.insert("j/monitors", Err("socket closed"));
        assert!(snapshot_of(&unreachable).is_err());
    }

    struct Fixed(Result<DesktopSnapshot, String>);

    impl DesktopProbe for Fixed {
        fn snapshot(&self) -> Result<DesktopSnapshot, String> {
            self.0.clone()
        }
    }

    #[test]
    fn only_new_desktop_problems_count_against_a_change() {
        let baseline = DesktopSnapshot {
            monitors: 2,
            config_errors: vec!["old error".into()],
        };
        let same = Fixed(Ok(baseline.clone()));
        assert!(desktop_problems(&baseline, &same).is_empty());
        let lost = Fixed(Ok(DesktopSnapshot {
            monitors: 1,
            config_errors: vec!["old error".into()],
        }));
        assert_eq!(
            desktop_problems(&baseline, &lost),
            ["1 monitor(s) were lost"]
        );
        let new_error = Fixed(Ok(DesktopSnapshot {
            monitors: 2,
            config_errors: vec!["old error".into(), "new error".into()],
        }));
        let problems = desktop_problems(&baseline, &new_error);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("new error"));
        let dead = Fixed(Err("connection refused".into()));
        assert_eq!(
            desktop_problems(&baseline, &dead),
            ["the compositor no longer responds"]
        );
    }

    #[test]
    fn raw_control_characters_inside_strings_do_not_break_parsing() {
        let value = parse_response("[{\"title\":\"a\tb\nc\",\"windows\":1}\n]").unwrap();
        assert_eq!(value.as_array().unwrap().len(), 1);
        assert!(parse_response("not json").is_err());
        // Escaped quotes do not end the string early.
        assert!(parse_response(r#"{"a":"x\"y\\"}"#).is_ok());
    }

    #[test]
    fn only_the_allow_listed_read_only_requests_exist() {
        for query in [
            Query::Version,
            Query::Monitors,
            Query::Workspaces,
            Query::ActiveWindow,
            Query::ConfigErrors,
        ] {
            let request = query.request();
            assert!(request.starts_with("j/"), "{request}");
            for forbidden in [
                "dispatch",
                "keyword",
                "reload",
                "exec",
                "kill",
                "setcursor",
                "output",
            ] {
                assert!(!request.contains(forbidden), "{request}");
            }
        }
    }

    #[test]
    fn the_socket_client_speaks_the_wire_protocol_and_survives_a_dead_compositor() {
        let dir = TempDir::new("hypr");
        let socket = dir.path().join(".socket.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = std::thread::spawn(move || {
            // Serve exactly the three requests of the test, recording what was asked.
            let mut asked = Vec::new();
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0u8; 64];
                let read = stream.read(&mut request).unwrap();
                let request = String::from_utf8_lossy(&request[..read]).into_owned();
                let answer = match request.as_str() {
                    "j/version" => VERSION,
                    "j/configerrors" => "[\n\t\"\"\n]",
                    _ => "[]",
                };
                stream.write_all(answer.as_bytes()).unwrap();
                asked.push(request);
            }
            asked
        });
        let ipc = HyprlandIpc::at(&socket);
        assert_eq!(
            ipc.query(Query::Version)
                .unwrap()
                .get("version")
                .and_then(Json::as_str),
            Some("0.55.4")
        );
        assert!(ipc.query(Query::ConfigErrors).unwrap().as_array().is_some());
        assert!(
            ipc.query(Query::Monitors)
                .unwrap()
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            server.join().unwrap(),
            ["j/version", "j/configerrors", "j/monitors"]
        );
        // The compositor is gone: a clean error, not a hang.
        drop(std::fs::remove_file(&socket));
        assert!(
            ipc.query(Query::Version)
                .unwrap_err()
                .contains("could not reach")
        );
        assert!(ipc.snapshot().is_err());
    }

    #[test]
    fn the_session_socket_is_found_from_the_environment_or_not_at_all() {
        let dir = TempDir::new("hypr");
        let session = dir.path().join("hypr").join("abc_123");
        std::fs::create_dir_all(&session).unwrap();
        let _listener = UnixListener::bind(session.join(".socket.sock")).unwrap();
        let runtime = dir.path().to_string_lossy().into_owned();
        let env = |key: &str| match key {
            "HYPRLAND_INSTANCE_SIGNATURE" => Some("abc_123".to_owned()),
            "XDG_RUNTIME_DIR" => Some(runtime.clone()),
            _ => None,
        };
        let found = HyprlandIpc::from_env(&env).unwrap();
        assert!(found.socket().ends_with("hypr/abc_123/.socket.sock"));
        // No signature, hostile signatures and missing sockets all mean "no session".
        assert!(HyprlandIpc::from_env(&|_| None).is_none());
        for signature in ["../../etc", "a/b", "", "sig with space"] {
            let hostile =
                |key: &str| (key == "HYPRLAND_INSTANCE_SIGNATURE").then(|| signature.to_owned());
            assert!(HyprlandIpc::from_env(&hostile).is_none(), "{signature:?}");
        }
        let missing = |key: &str| match key {
            "HYPRLAND_INSTANCE_SIGNATURE" => Some("other".to_owned()),
            "XDG_RUNTIME_DIR" => Some(runtime.clone()),
            _ => None,
        };
        assert!(HyprlandIpc::from_env(&missing).is_none());
    }
}
