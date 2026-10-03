//! Runtime health, read through structured systemd interfaces only.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::exec::{Invocation, Runner};
use crate::json::Json;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HealthSnapshot {
    /// Output of `systemctl is-system-running` (`running`, `degraded`, `starting`, ...).
    pub system_state: String,
    /// Units that are `failed` or crash-looping (`auto-restart`).
    pub unhealthy_units: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitSummary {
    pub name: String,
    pub load: String,
    pub active: String,
    pub sub: String,
}

#[derive(Clone, Copy, Debug)]
pub struct HealthPolicy {
    /// How long to wait for a freshly activated system to leave `starting`/`initializing`.
    pub settle_timeout: Duration,
    pub poll_interval: Duration,
    /// Minimum time to watch the activated system before judging it, so units that start and
    /// then crash shortly afterwards are caught.
    pub observe: Duration,
}

impl Default for HealthPolicy {
    fn default() -> Self {
        Self {
            settle_timeout: Duration::from_secs(30),
            poll_interval: Duration::from_secs(1),
            observe: Duration::from_secs(5),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HealthReport {
    pub healthy: bool,
    /// Human-readable reasons; empty when healthy.
    pub problems: Vec<String>,
    pub snapshot: HealthSnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NixosVersion {
    pub version: String,
    pub nixpkgs_revision: Option<String>,
    pub configuration_revision: Option<String>,
}

#[derive(Clone)]
pub struct SystemAdapter {
    runner: Arc<dyn Runner>,
}

impl SystemAdapter {
    pub fn new(runner: Arc<dyn Runner>) -> Self {
        Self { runner }
    }

    pub fn system_state(&self) -> Result<String, String> {
        // A non-zero exit is normal here (`degraded`, `starting`); only the printed state counts.
        let outcome = self
            .runner
            .run(&Invocation::new("systemctl").arg("is-system-running"))?;
        let state = outcome.stdout_text()?;
        if !matches!(
            state.as_str(),
            "running"
                | "degraded"
                | "starting"
                | "stopping"
                | "maintenance"
                | "initializing"
                | "offline"
                | "unknown"
        ) {
            return Err("systemctl did not report a system state".into());
        }
        Ok(state)
    }

    pub fn unhealthy_units(&self) -> Result<BTreeSet<String>, String> {
        let outcome = self.runner.run(&Invocation::new("systemctl").args([
            "list-units",
            "--all",
            "--output=json",
            "--no-pager",
        ]))?;
        if !outcome.success() {
            return Err("systemctl could not list units".into());
        }
        let value = Json::parse(&outcome.stdout_text()?)?;
        let units = value
            .as_array()
            .ok_or_else(|| "systemctl unit list is not a JSON array".to_owned())?;
        let mut unhealthy = BTreeSet::new();
        for unit in units {
            let name = unit
                .get("unit")
                .and_then(Json::as_str)
                .ok_or_else(|| "systemctl unit entry has no name".to_owned())?;
            if validate_unit_name(name).is_err() {
                continue;
            }
            let active = unit
                .get("active")
                .and_then(Json::as_str)
                .unwrap_or_default();
            let sub = unit.get("sub").and_then(Json::as_str).unwrap_or_default();
            if active == "failed" || sub == "auto-restart" {
                unhealthy.insert(name.to_owned());
            }
        }
        Ok(unhealthy)
    }

    /// List service names and their machine-readable systemd states, optionally filtered by a
    /// literal substring. Descriptions and process arguments are intentionally omitted.
    pub fn unit_summaries(&self, filter: &str, limit: usize) -> Result<Vec<UnitSummary>, String> {
        if filter.len() > 128 || filter.chars().any(char::is_control) {
            return Err("unit filter is invalid".into());
        }
        let outcome = self.runner.run(&Invocation::new("systemctl").args([
            "list-units",
            "--type=service",
            "--all",
            "--output=json",
            "--no-pager",
        ]))?;
        if !outcome.success() {
            return Err("systemctl could not list service units".into());
        }
        let value = Json::parse(&outcome.stdout_text()?)?;
        let units = value
            .as_array()
            .ok_or_else(|| "systemctl service list is not a JSON array".to_owned())?;
        let needle = filter.to_ascii_lowercase();
        let mut result = Vec::new();
        for unit in units {
            let name = unit
                .get("unit")
                .and_then(Json::as_str)
                .ok_or_else(|| "systemctl service entry has no name".to_owned())?;
            if validate_unit_name(name).is_err() || !name.ends_with(".service") {
                continue;
            }
            if !needle.is_empty() && !name.to_ascii_lowercase().contains(&needle) {
                continue;
            }
            result.push(UnitSummary {
                name: name.to_owned(),
                load: unit
                    .get("load")
                    .and_then(Json::as_str)
                    .filter(|value| {
                        matches!(
                            *value,
                            "loaded"
                                | "not-found"
                                | "error"
                                | "masked"
                                | "stub"
                                | "merged"
                                | "unknown"
                        )
                    })
                    .unwrap_or("unknown")
                    .to_owned(),
                active: unit
                    .get("active")
                    .and_then(Json::as_str)
                    .filter(|value| {
                        matches!(
                            *value,
                            "active"
                                | "reloading"
                                | "inactive"
                                | "failed"
                                | "activating"
                                | "deactivating"
                                | "maintenance"
                                | "unknown"
                        )
                    })
                    .unwrap_or("unknown")
                    .to_owned(),
                sub: unit
                    .get("sub")
                    .and_then(Json::as_str)
                    .filter(|value| {
                        value.len() <= 32
                            && value.bytes().all(|byte| {
                                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')
                            })
                    })
                    .unwrap_or("unknown")
                    .to_owned(),
            });
            if result.len() >= limit.clamp(1, 100) {
                break;
            }
        }
        Ok(result)
    }

    pub fn is_active(&self, unit: &str) -> Result<bool, String> {
        validate_unit_name(unit)?;
        let outcome =
            self.runner
                .run(&Invocation::new("systemctl").args(["is-active", "--quiet", unit]))?;
        Ok(outcome.success())
    }

    pub fn snapshot(&self) -> Result<HealthSnapshot, String> {
        Ok(HealthSnapshot {
            system_state: self.system_state()?,
            unhealthy_units: self.unhealthy_units()?,
        })
    }

    pub fn nixos_version(&self) -> Result<NixosVersion, String> {
        let outcome = self
            .runner
            .run(&Invocation::new("nixos-version").arg("--json"))?;
        if !outcome.success() {
            return Err("nixos-version failed".into());
        }
        let value = Json::parse(&outcome.stdout_text()?)?;
        let text = |key: &str| {
            value
                .get(key)
                .and_then(Json::as_str)
                .filter(|text| !text.is_empty())
                .map(str::to_owned)
        };
        Ok(NixosVersion {
            version: text("nixosVersion")
                .ok_or_else(|| "nixos-version did not report a version".to_owned())?,
            nixpkgs_revision: text("nixpkgsRevision"),
            configuration_revision: text("configurationRevision"),
        })
    }

    /// Wait for the system to settle, then judge it against `baseline`: only failures that are
    /// *new* since the baseline, an unsettled/broken system state, or inactive expected units
    /// make the system unhealthy. Units that were already failing before the change are not
    /// blamed on it.
    pub fn verify(
        &self,
        baseline: &HealthSnapshot,
        expect_active: &[String],
        policy: HealthPolicy,
    ) -> Result<HealthReport, String> {
        for unit in expect_active {
            validate_unit_name(unit)?;
        }
        if !policy.observe.is_zero() {
            std::thread::sleep(policy.observe);
        }
        let started = Instant::now();
        let mut snapshot = self.snapshot()?;
        while matches!(snapshot.system_state.as_str(), "initializing" | "starting")
            && started.elapsed() < policy.settle_timeout
        {
            std::thread::sleep(policy.poll_interval);
            snapshot = self.snapshot()?;
        }
        let mut problems = Vec::new();
        match snapshot.system_state.as_str() {
            "running" | "degraded" => {}
            state => problems.push(format!("system state is '{state}'")),
        }
        for unit in snapshot
            .unhealthy_units
            .difference(&baseline.unhealthy_units)
        {
            problems.push(format!("unit {unit} failed after the change"));
        }
        for unit in expect_active {
            if !self.is_active(unit)? {
                problems.push(format!("expected unit {unit} is not active"));
            }
        }
        Ok(HealthReport {
            healthy: problems.is_empty(),
            problems,
            snapshot,
        })
    }
}

/// Unit names are passed to `systemctl` as arguments; keep them plain and option-free.
pub fn validate_unit_name(unit: &str) -> Result<(), String> {
    const SUFFIXES: [&str; 6] = [
        ".service", ".socket", ".target", ".timer", ".mount", ".path",
    ];
    let stem_valid = SUFFIXES
        .iter()
        .find_map(|suffix| unit.strip_suffix(suffix))
        .is_some_and(|stem| {
            !stem.is_empty()
                && !stem.starts_with('-')
                && stem
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '@' | ':'))
        });
    if stem_valid {
        Ok(())
    } else {
        Err(format!("invalid systemd unit name '{unit}'"))
    }
}

#[cfg(test)]
mod tests {
    use super::{HealthPolicy, HealthSnapshot, SystemAdapter, validate_unit_name};
    use crate::exec::{Invocation, Outcome, Runner};
    use std::collections::BTreeSet;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// Scripted systemd: successive `is-system-running` answers, a unit list and active units.
    struct FakeSystemd {
        states: Mutex<Vec<&'static str>>,
        units: &'static str,
        active: &'static [&'static str],
    }

    impl Runner for FakeSystemd {
        fn run(&self, invocation: &Invocation) -> Result<Outcome, String> {
            let args = invocation.arguments();
            let ok = |text: &str| {
                Ok(Outcome {
                    code: Some(0),
                    stdout: text.as_bytes().to_vec(),
                    stderr: Vec::new(),
                })
            };
            match (invocation.program(), args.first().map(String::as_str)) {
                ("systemctl", Some("is-system-running")) => {
                    let mut states = self.states.lock().unwrap();
                    let state = if states.len() > 1 {
                        states.remove(0)
                    } else {
                        states[0]
                    };
                    Ok(Outcome {
                        code: Some(i32::from(state != "running")),
                        stdout: state.as_bytes().to_vec(),
                        stderr: Vec::new(),
                    })
                }
                ("systemctl", Some("list-units")) => ok(self.units),
                ("systemctl", Some("is-active")) => {
                    let unit = args.last().unwrap();
                    Ok(Outcome {
                        code: Some(i32::from(!self.active.contains(&unit.as_str()))),
                        ..Outcome::default()
                    })
                }
                ("nixos-version", _) => ok(
                    r#"{"nixosVersion":"26.05.1","nixpkgsRevision":"abc","configurationRevision":null}"#,
                ),
                other => panic!("unexpected command {other:?}"),
            }
        }
    }

    fn adapter(
        states: Vec<&'static str>,
        units: &'static str,
        active: &'static [&'static str],
    ) -> SystemAdapter {
        SystemAdapter::new(Arc::new(FakeSystemd {
            states: Mutex::new(states),
            units,
            active,
        }))
    }

    const NO_WAIT: HealthPolicy = HealthPolicy {
        settle_timeout: Duration::from_secs(5),
        poll_interval: Duration::ZERO,
        observe: Duration::ZERO,
    };

    const UNITS_WITH_OLD_FAILURE: &str = r#"[
        {"unit":"a.service","load":"loaded","active":"active","sub":"running"},
        {"unit":"old.service","load":"loaded","active":"failed","sub":"failed"}]"#;

    #[test]
    fn snapshot_collects_failed_and_crash_looping_units() {
        let system = adapter(
            vec!["degraded"],
            r#"[{"unit":"x.service","active":"failed","sub":"failed"},
                {"unit":"y.service","active":"activating","sub":"auto-restart"},
                {"unit":"z.service","active":"active","sub":"running"}]"#,
            &[],
        );
        let snapshot = system.snapshot().unwrap();
        assert_eq!(snapshot.system_state, "degraded");
        assert_eq!(
            snapshot.unhealthy_units,
            BTreeSet::from(["x.service".to_owned(), "y.service".to_owned()])
        );
    }

    #[test]
    fn preexisting_failures_are_not_blamed_on_the_change() {
        let baseline = adapter(vec!["degraded"], UNITS_WITH_OLD_FAILURE, &[])
            .snapshot()
            .unwrap();
        let after = adapter(vec!["degraded"], UNITS_WITH_OLD_FAILURE, &[]);
        let report = after.verify(&baseline, &[], NO_WAIT).unwrap();
        assert!(report.healthy, "{:?}", report.problems);
    }

    #[test]
    fn new_failures_and_inactive_expected_units_are_unhealthy() {
        let baseline = HealthSnapshot {
            system_state: "running".into(),
            unhealthy_units: BTreeSet::new(),
        };
        let broken = adapter(
            vec!["degraded"],
            UNITS_WITH_OLD_FAILURE,
            &["bluetooth.service"],
        );
        let report = broken
            .verify(
                &baseline,
                &["bluetooth.service".into(), "other.service".into()],
                NO_WAIT,
            )
            .unwrap();
        assert!(!report.healthy);
        assert!(report.problems.iter().any(|p| p.contains("old.service")));
        assert!(report.problems.iter().any(|p| p.contains("other.service")));
        assert!(
            !report
                .problems
                .iter()
                .any(|p| p.contains("bluetooth.service"))
        );
    }

    #[test]
    fn waits_for_the_system_to_settle_and_fails_if_it_never_does() {
        let baseline = HealthSnapshot {
            system_state: "running".into(),
            unhealthy_units: BTreeSet::new(),
        };
        let settling = adapter(vec!["starting", "starting", "running"], "[]", &[]);
        assert!(settling.verify(&baseline, &[], NO_WAIT).unwrap().healthy);

        let never = adapter(vec!["starting"], "[]", &[]);
        let policy = HealthPolicy {
            settle_timeout: Duration::from_millis(20),
            poll_interval: Duration::from_millis(1),
            observe: Duration::ZERO,
        };
        let report = never.verify(&baseline, &[], policy).unwrap();
        assert!(!report.healthy);
        assert!(report.problems[0].contains("starting"));

        let stopping = adapter(vec!["maintenance"], "[]", &[]);
        assert!(!stopping.verify(&baseline, &[], NO_WAIT).unwrap().healthy);
    }

    #[test]
    fn unit_names_cannot_smuggle_options_or_paths() {
        assert!(validate_unit_name("bluetooth.service").is_ok());
        assert!(validate_unit_name("user@1000.service").is_ok());
        for bad in [
            "--now.service",
            "x",
            "a b.service",
            "../x.service",
            "x.service;reboot",
            ".service",
        ] {
            assert!(validate_unit_name(bad).is_err(), "accepted {bad}");
        }
    }

    #[test]
    fn nixos_version_reports_revisions_when_present() {
        let version = adapter(vec!["running"], "[]", &[]).nixos_version().unwrap();
        assert_eq!(version.version, "26.05.1");
        assert_eq!(version.nixpkgs_revision.as_deref(), Some("abc"));
        assert_eq!(version.configuration_revision, None);
    }
}
