//! Bounded, value-minimizing system diagnostics for agent frontends.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crate::HyprlandIpc;
use crate::exec::{Invocation, Runner};
use crate::health::validate_unit_name;
use crate::json::Json;

const MAX_JOURNAL_RECORDS: usize = 50;
const MAX_PROCESSES: usize = 64;
const MAX_NETWORK_INTERFACES: usize = 32;
const MAX_BLOCK_DEVICES: usize = 32;

pub struct Diagnostics {
    root: std::path::PathBuf,
    runner: Arc<dyn Runner>,
}

impl Diagnostics {
    pub fn new(root: impl Into<std::path::PathBuf>, runner: Arc<dyn Runner>) -> Self {
        Self {
            root: root.into(),
            runner,
        }
    }

    pub fn network_json(&self) -> Result<String, String> {
        let path = self.root.join("sys/class/net");
        let entries =
            fs::read_dir(path).map_err(|_| "network interface inventory is unavailable")?;
        let mut interfaces = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == "lo" || !safe_kernel_name(&name) {
                continue;
            }
            let dir = entry.path();
            let state = read_trimmed(&dir.join("operstate"))
                .filter(|value| {
                    matches!(
                        value.as_str(),
                        "up" | "down"
                            | "dormant"
                            | "notpresent"
                            | "lowerlayerdown"
                            | "testing"
                            | "unknown"
                    )
                })
                .unwrap_or_else(|| "unknown".into());
            let kind = read_trimmed(&dir.join("type"))
                .filter(|value| value.parse::<u16>().is_ok())
                .unwrap_or_else(|| "unknown".into());
            let carrier = read_trimmed(&dir.join("carrier"));
            interfaces.push(format!(
                "{{\"name\":{},\"type\":{},\"state\":{},\"carrier\":{}}}",
                crate::json_string(&name),
                crate::json_string(&kind),
                crate::json_string(&state),
                carrier
                    .as_deref()
                    .and_then(|value| match value {
                        "0" => Some("false"),
                        "1" => Some("true"),
                        _ => None,
                    })
                    .unwrap_or("null")
            ));
            if interfaces.len() >= MAX_NETWORK_INTERFACES {
                break;
            }
        }
        interfaces.sort();
        let rfkill = self
            .runner
            .run(
                &Invocation::new("rfkill")
                    .args(["--json", "list"])
                    .timeout(Duration::from_secs(3)),
            )
            .ok()
            .and_then(|outcome| {
                if !outcome.success() {
                    return None;
                }
                Json::parse(&outcome.stdout_text().ok()?).ok()
            });
        let devices = rfkill
            .as_ref()
            .and_then(|value| value.get("rfkilldevices"))
            .and_then(Json::as_array);
        let mut wlan_soft_blocked = 0usize;
        let mut wlan_hard_blocked = 0usize;
        if let Some(devices) = devices {
            for device in devices
                .iter()
                .filter(|device| device.get("type").and_then(Json::as_str) == Some("wlan"))
            {
                wlan_soft_blocked +=
                    usize::from(device.get("soft").and_then(Json::as_str) == Some("blocked"));
                wlan_hard_blocked +=
                    usize::from(device.get("hard").and_then(Json::as_str) == Some("blocked"));
            }
        }
        Ok(format!(
            "{{\"rfkill_available\":{},\"wlan_soft_blocked_count\":{},\"wlan_hard_blocked_count\":{},\"interfaces\":[{}]}}",
            if devices.is_some() { "true" } else { "false" },
            wlan_soft_blocked,
            wlan_hard_blocked,
            interfaces.join(",")
        ))
    }

    pub fn bluetooth_json(&self) -> String {
        let service = self
            .runner
            .run(
                &Invocation::new("systemctl")
                    .args(["is-active", "bluetooth.service"])
                    .timeout(Duration::from_secs(3)),
            )
            .ok()
            .map(|outcome| outcome.success());
        let rfkill = self
            .runner
            .run(
                &Invocation::new("rfkill")
                    .args(["--json", "list"])
                    .timeout(Duration::from_secs(3)),
            )
            .ok()
            .and_then(|outcome| {
                if !outcome.success() {
                    return None;
                }
                Json::parse(&outcome.stdout_text().ok()?).ok()
            });
        let devices = rfkill
            .as_ref()
            .and_then(|value| value.get("rfkilldevices"))
            .and_then(Json::as_array);
        let mut count = 0usize;
        let mut soft_blocked = 0usize;
        let mut hard_blocked = 0usize;
        if let Some(devices) = devices {
            for device in devices
                .iter()
                .filter(|device| device.get("type").and_then(Json::as_str) == Some("bluetooth"))
            {
                count += 1;
                soft_blocked +=
                    usize::from(device.get("soft").and_then(Json::as_str) == Some("blocked"));
                hard_blocked +=
                    usize::from(device.get("hard").and_then(Json::as_str) == Some("blocked"));
            }
        }
        let controller_count = fs::read_dir(self.root.join("sys/class/bluetooth"))
            .map(|entries| entries.flatten().count())
            .unwrap_or(0);
        format!(
            "{{\"service_active\":{},\"rfkill_available\":{},\"rfkill_adapter_count\":{},\"controller_count\":{},\"soft_blocked_count\":{},\"hard_blocked_count\":{}}}",
            service.map_or("null", |value| if value { "true" } else { "false" }),
            if devices.is_some() { "true" } else { "false" },
            count,
            controller_count,
            soft_blocked,
            hard_blocked
        )
    }

    pub fn package_json(&self, name: &str) -> Result<String, String> {
        if name.is_empty()
            || name.len() > 128
            || !name
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "-+_.".contains(character))
            || name.starts_with('.')
        {
            return Err("package executable name is invalid".to_owned());
        }
        let executable = self.root.join("run/current-system/sw/bin").join(name);
        let available = fs::metadata(executable)
            .map(|metadata| metadata.is_file() && is_executable(&metadata))
            .unwrap_or(false);
        Ok(format!(
            "{{\"name\":{},\"available\":{}}}",
            crate::json_string(name),
            available
        ))
    }

    pub fn hardware_json(&self) -> String {
        let cpuinfo = fs::read_to_string(self.root.join("proc/cpuinfo")).unwrap_or_default();
        let cpu_facts = cpuinfo
            .lines()
            .filter_map(|line| line.split_once(':'))
            .map(|(key, value)| (key.trim(), value.trim()))
            .collect::<BTreeMap<_, _>>();
        let cpu_vendor = cpu_facts.get("vendor_id").map(|value| safe_code(value));
        let cpu_family = cpu_facts
            .get("cpu family")
            .and_then(|value| value.parse::<u32>().ok());
        let cpu_model = cpu_facts
            .get("model")
            .and_then(|value| value.parse::<u32>().ok());
        let cpu_count = cpuinfo
            .lines()
            .filter(|line| line.starts_with("processor\t") || line.starts_with("processor :"))
            .count();
        let block_path = self.root.join("sys/class/block");
        let mut blocks = Vec::new();
        if let Ok(entries) = fs::read_dir(block_path) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !safe_kernel_name(&name) {
                    continue;
                }
                let dir = entry.path();
                let sectors = read_trimmed(&dir.join("size"))
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0);
                if sectors == 0 {
                    continue;
                }
                let read_only = read_trimmed(&dir.join("ro")).as_deref() == Some("1");
                blocks.push(format!(
                    "{{\"name\":{},\"size_bytes\":{},\"read_only\":{}}}",
                    crate::json_string(&name),
                    sectors.saturating_mul(512),
                    read_only
                ));
                if blocks.len() >= MAX_BLOCK_DEVICES {
                    break;
                }
            }
        }
        blocks.sort();
        let mut pci = Vec::new();
        if let Ok(entries) = fs::read_dir(self.root.join("sys/bus/pci/devices")) {
            for entry in entries.flatten() {
                let dir = entry.path();
                let class = read_trimmed(&dir.join("class")).unwrap_or_default();
                let category = match class.as_bytes().get(2..4) {
                    Some(b"03") => "display",
                    Some(b"02") => "network",
                    _ => continue,
                };
                let vendor = read_trimmed(&dir.join("vendor")).unwrap_or_default();
                let device = read_trimmed(&dir.join("device")).unwrap_or_default();
                if !safe_pci_id(&vendor) || !safe_pci_id(&device) {
                    continue;
                }
                pci.push(format!(
                    "{{\"category\":{},\"vendor_id\":{},\"device_id\":{}}}",
                    crate::json_string(category),
                    crate::json_string(&vendor),
                    crate::json_string(&device)
                ));
                if pci.len() >= 32 {
                    break;
                }
            }
        }
        pci.sort();
        format!(
            "{{\"architecture\":{},\"cpu_vendor_id\":{},\"cpu_family\":{},\"cpu_model_id\":{},\"cpu_count\":{},\"pci_adapters\":[{}],\"block_devices\":[{}]}}",
            crate::json_string(std::env::consts::ARCH),
            cpu_vendor
                .as_deref()
                .map(crate::json_string)
                .unwrap_or_else(|| "null".into()),
            cpu_family
                .map(|value| value.to_string())
                .unwrap_or_else(|| "null".into()),
            cpu_model
                .map(|value| value.to_string())
                .unwrap_or_else(|| "null".into()),
            cpu_count,
            pci.join(","),
            blocks.join(",")
        )
    }

    pub fn processes_json(&self, filter: &str) -> Result<String, String> {
        if filter.len() > 64
            || !filter
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
        {
            return Err(
                "process filter must contain only letters, digits, dot, underscore or dash".into(),
            );
        }
        let mut counts = BTreeMap::<String, usize>::new();
        let proc_path = self.root.join("proc");
        for entry in fs::read_dir(proc_path)
            .map_err(|_| "process inventory is unavailable")?
            .flatten()
        {
            if !entry
                .file_name()
                .to_string_lossy()
                .bytes()
                .all(|byte| byte.is_ascii_digit())
            {
                continue;
            }
            let Some(comm) = read_trimmed(&entry.path().join("comm")) else {
                continue;
            };
            if comm.is_empty()
                || !comm
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
            {
                continue;
            }
            if !filter.is_empty()
                && !comm
                    .to_ascii_lowercase()
                    .contains(&filter.to_ascii_lowercase())
            {
                continue;
            }
            *counts.entry(comm).or_default() += 1;
        }
        let processes = counts
            .into_iter()
            .take(MAX_PROCESSES)
            .map(|(name, count)| {
                format!(
                    "{{\"name\":{},\"count\":{count}}}",
                    crate::json_string(&name)
                )
            })
            .collect::<Vec<_>>();
        Ok(format!("{{\"processes\":[{}]}}", processes.join(",")))
    }

    /// Read log messages only inside this process to classify a fixed, secret-free error category.
    /// Raw MESSAGE values are never serialized, logged or returned to an agent.
    pub fn journal_json(&self, unit: Option<&str>, limit: usize) -> Result<String, String> {
        if let Some(unit) = unit {
            validate_unit_name(unit)?;
        }
        let limit = limit.clamp(1, MAX_JOURNAL_RECORDS);
        let mut command = Invocation::new("journalctl")
            .args([
                "--system",
                "--boot=0",
                "--priority=warning",
                "--output=json",
                "--output-fields=__REALTIME_TIMESTAMP,_SYSTEMD_UNIT,PRIORITY,MESSAGE_ID,MESSAGE",
                "--no-pager",
                "--quiet",
                "--lines",
                &limit.to_string(),
            ])
            .timeout(Duration::from_secs(5));
        if let Some(unit) = unit {
            command = command.args(["--unit", unit]);
        }
        let unavailable = |reason: &str| {
            format!(
                "{{\"available\":false,\"reason\":{},\"contains_message_text\":false,\"records\":[]}}",
                crate::json_string(reason)
            )
        };
        let outcome = match self.runner.run(&command) {
            Ok(outcome) => outcome,
            Err(_) => return Ok(unavailable("unavailable")),
        };
        if !outcome.success() {
            let reason = if String::from_utf8_lossy(&outcome.stderr)
                .to_ascii_lowercase()
                .contains("permission denied")
            {
                "permission_denied"
            } else {
                "unavailable"
            };
            return Ok(unavailable(reason));
        }
        let text = outcome.stdout_text()?;
        if text.len() > 256 * 1024 {
            return Ok(unavailable("output_limit"));
        }
        let mut records = Vec::new();
        for line in text.lines().take(limit) {
            let Ok(value) = Json::parse(line) else {
                continue;
            };
            let timestamp = value
                .get("__REALTIME_TIMESTAMP")
                .and_then(Json::as_str)
                .filter(|s| s.bytes().all(|b| b.is_ascii_digit()))
                .unwrap_or("");
            let unit = value
                .get("_SYSTEMD_UNIT")
                .and_then(Json::as_str)
                .filter(|s| validate_unit_name(s).is_ok());
            let priority = value
                .get("PRIORITY")
                .and_then(Json::as_str)
                .and_then(|s| s.parse::<u8>().ok())
                .filter(|n| *n <= 4);
            let message_id = value
                .get("MESSAGE_ID")
                .and_then(Json::as_str)
                .filter(|s| s.len() <= 64 && s.bytes().all(|b| b.is_ascii_hexdigit()));
            let message_kind = value
                .get("MESSAGE")
                .and_then(Json::as_str)
                .map(classify_journal_message)
                .unwrap_or("other");
            records.push(format!(
                "{{\"timestamp_usec\":{},\"unit\":{},\"priority\":{},\"message_id\":{},\"message_kind\":{}}}",
                if timestamp.is_empty() {
                    "null".to_owned()
                } else {
                    timestamp.to_owned()
                },
                unit.map(crate::json_string)
                    .unwrap_or_else(|| "null".into()),
                priority
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "null".into()),
                message_id
                    .map(crate::json_string)
                    .unwrap_or_else(|| "null".into()),
                crate::json_string(message_kind)
            ));
        }
        Ok(format!(
            "{{\"available\":true,\"reason\":null,\"contains_message_text\":false,\"records\":[{}]}}",
            records.join(",")
        ))
    }

    /// Expose only counts and booleans from the compositor; omit window classes, names and errors.
    pub fn desktop_json(&self) -> String {
        let Some(ipc) = HyprlandIpc::from_env(&|key| std::env::var(key).ok()) else {
            return "{\"available\":false}".into();
        };
        let Ok(summary) = ipc.summary() else {
            return "{\"available\":false}".into();
        };
        let monitors = summary.monitors.iter().filter(|monitor| {
            !monitor.name.is_empty()
                && monitor.name.len() <= 64
                && monitor.name.chars().all(|c| c.is_ascii_alphanumeric() || "_.:-".contains(c))
                && monitor.refresh_hz.is_finite()
                && (0.0..=1000.0).contains(&monitor.refresh_hz)
        }).map(|monitor| format!(
            "{{\"name\":{},\"width\":{},\"height\":{},\"refresh_hz\":{},\"focused\":{},\"disabled\":{}}}",
            crate::json_string(&monitor.name), monitor.width, monitor.height, monitor.refresh_hz,
            monitor.focused, monitor.disabled
        )).collect::<Vec<_>>().join(",");
        let workspaces = summary
            .workspaces
            .iter()
            .filter(|workspace| {
                workspace.name.len() <= 64
                    && workspace.monitor.len() <= 64
                    && workspace
                        .name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "_.:+-".contains(c))
                    && workspace
                        .monitor
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "_.:-".contains(c))
            })
            .map(|workspace| {
                format!(
                    "{{\"id\":{},\"name\":{},\"monitor\":{},\"windows\":{}}}",
                    workspace.id,
                    crate::json_string(&workspace.name),
                    crate::json_string(&workspace.monitor),
                    workspace.windows
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"available\":true,\"version\":{},\"monitor_count\":{},\"workspace_count\":{},\"window_count\":{},\"monitors\":[{}],\"workspaces\":[{}]}}",
            crate::json_string(&summary.version),
            summary.monitors.len(),
            summary.workspaces.len(),
            summary.window_count,
            monitors,
            workspaces,
        )
    }
}

fn classify_journal_message(message: &str) -> &'static str {
    let message = message.to_ascii_lowercase();
    if message.contains("permission denied") || message.contains("operation not permitted") {
        "permission_denied"
    } else if message.contains("no such file") || message.contains("not found") {
        "missing_resource"
    } else if message.contains("connection refused") {
        "connection_refused"
    } else if message.contains("address already in use") || message.contains("address in use") {
        "address_conflict"
    } else if message.contains("out of memory") || message.contains("oom-kill") {
        "memory_pressure"
    } else if message.contains("timed out") || message.contains("timeout") {
        "timeout"
    } else if message.contains("segfault") || message.contains("segmentation fault") {
        "process_crash"
    } else if message.contains("failed") || message.contains("failure") || message.contains("error")
    {
        "service_error"
    } else {
        "other"
    }
}

#[cfg(unix)]
fn is_executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_: &fs::Metadata) -> bool {
    false
}

fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_owned())
}

fn safe_kernel_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
}

fn safe_pci_id(value: &str) -> bool {
    value.len() == 6
        && value.starts_with("0x")
        && value
            .as_bytes()
            .get(2..)
            .is_some_and(|bytes| bytes.iter().all(u8::is_ascii_hexdigit))
}

fn safe_code(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || "_-".contains(*c))
        .take(32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::Diagnostics;
    use crate::exec::{Invocation, Outcome, Runner};
    use crate::fsutil::testutil::TempDir;
    use std::sync::Arc;

    struct JournalRunner;

    impl Runner for JournalRunner {
        fn run(&self, invocation: &Invocation) -> Result<Outcome, String> {
            assert_eq!(invocation.program(), "journalctl");
            assert!(invocation.arguments().iter().any(|arg| arg
                == "--output-fields=__REALTIME_TIMESTAMP,_SYSTEMD_UNIT,PRIORITY,MESSAGE_ID,MESSAGE"));
            assert!(
                invocation
                    .arguments()
                    .iter()
                    .all(|arg| !arg.contains("MESSAGE,MESSAGE"))
            );
            Ok(Outcome {
                code: Some(0),
                stdout: br#"{"__REALTIME_TIMESTAMP":"1730000000000000","_SYSTEMD_UNIT":"nginx.service","PRIORITY":"3","MESSAGE_ID":"abcd","MESSAGE":"connection refused private-token"}"#.to_vec(),
                stderr: Vec::new(),
            })
        }
    }

    struct RadioRunner;

    impl Runner for RadioRunner {
        fn run(&self, invocation: &Invocation) -> Result<Outcome, String> {
            let stdout = match (invocation.program(), invocation.arguments().first().map(String::as_str)) {
                ("systemctl", Some("is-active")) => b"active\n".to_vec(),
                ("rfkill", Some("--json")) => br#"{"rfkilldevices":[{"type":"bluetooth","device":"private-radio-name","soft":"blocked","hard":"unblocked"},{"type":"wlan","device":"wifi0","soft":"unblocked","hard":"unblocked"}]}"#.to_vec(),
                other => panic!("unexpected diagnostic command: {other:?}"),
            };
            Ok(Outcome {
                code: Some(0),
                stdout,
                stderr: Vec::new(),
            })
        }
    }

    #[test]
    fn journal_output_contains_only_whitelisted_metadata_and_never_message_values() {
        let diagnostics = Diagnostics::new("/unused", Arc::new(JournalRunner));
        let output = diagnostics.journal_json(Some("nginx.service"), 8).unwrap();
        assert!(output.contains("nginx.service"));
        assert!(output.contains("\"priority\":3"));
        assert!(!output.contains("private-token"));
        assert!(!output.contains("MESSAGE"));
        assert!(output.contains("connection_refused"));
    }

    #[test]
    fn journal_unit_and_filter_arguments_are_strict() {
        let diagnostics = Diagnostics::new("/unused", Arc::new(JournalRunner));
        assert!(diagnostics.journal_json(Some("--help"), 8).is_err());
        assert!(diagnostics.processes_json("--help").is_err());
    }

    #[test]
    fn hardware_inventory_uses_ids_and_sizes_without_serials_or_mounts() {
        let root = TempDir::new("inventory");
        let write = |relative: &str, contents: &str| {
            let path = root.path().join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        };
        write(
            "proc/cpuinfo",
            "processor : 0\nvendor_id : GenuineIntel\ncpu family : 6\nmodel : 170\n\nprocessor : 1\nvendor_id : GenuineIntel\ncpu family : 6\nmodel : 170\n",
        );
        write("sys/class/block/nvme0n1/size", "1024\n");
        write("sys/class/block/nvme0n1/ro", "0\n");
        write("sys/bus/pci/devices/0000:00:02.0/class", "0x030000\n");
        write("sys/bus/pci/devices/0000:00:02.0/vendor", "0x8086\n");
        write("sys/bus/pci/devices/0000:00:02.0/device", "0x1234\n");
        let diagnostics = Diagnostics::new(root.path(), Arc::new(JournalRunner));
        let output = diagnostics.hardware_json();
        assert!(output.contains("\"cpu_count\":2"));
        assert!(output.contains("\"size_bytes\":524288"));
        assert!(output.contains("\"category\":\"display\""));
        assert!(!output.contains("serial"));
        assert!(!output.contains("mount"));
    }

    #[test]
    fn process_inventory_only_returns_safe_names_and_aggregated_counts() {
        let root = TempDir::new("processes");
        let dir = root.path().join("proc/123");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("comm"), "systemd\n").unwrap();
        let diagnostics = Diagnostics::new(root.path(), Arc::new(JournalRunner));
        let output = diagnostics.processes_json("system").unwrap();
        assert!(output.contains("\"name\":\"systemd\",\"count\":1"));
        assert!(!output.contains("123"));
    }

    #[test]
    fn network_and_radio_reports_keep_only_safe_link_and_block_state() {
        let root = TempDir::new("network");
        let write = |relative: &str, contents: &str| {
            let path = root.path().join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        };
        write("sys/class/net/eth0/operstate", "up\n");
        write("sys/class/net/eth0/type", "1\n");
        write("sys/class/net/eth0/carrier", "1\n");
        write("sys/class/net/lo/operstate", "unknown\n");
        write("sys/class/net/lo/type", "772\n");
        std::fs::create_dir_all(root.path().join("sys/class/bluetooth/hci0")).unwrap();
        let diagnostics = Diagnostics::new(root.path(), Arc::new(RadioRunner));
        let network = diagnostics.network_json().unwrap();
        assert!(network.contains("eth0"));
        assert!(!network.contains("\"name\":\"lo\""));
        assert!(network.contains("wlan_soft_blocked_count"));
        let bluetooth = diagnostics.bluetooth_json();
        assert!(bluetooth.contains("\"soft_blocked_count\":1"));
        assert!(bluetooth.contains("\"controller_count\":1"));
        assert!(!bluetooth.contains("private-radio-name"));
        assert!(!bluetooth.contains("wifi0"));
    }
}
