//! Read-only view of the running machine's system links and of prepared system closures.
//! A `root` other than `/` lets tests (and `--root`) point this at a fixture tree.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::json::Json;

/// Closure components whose change means a reboot is needed to take full effect.
pub const REBOOT_COMPONENTS: [&str; 4] = ["kernel", "kernel-modules", "initrd", "systemd"];

#[derive(Clone, Debug)]
pub struct Host {
    root: PathBuf,
}

impl Host {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `/run/current-system` as a store path.
    pub fn current_system(&self) -> Option<String> {
        self.link("run/current-system")
    }

    /// `/run/booted-system` as a store path.
    pub fn booted_system(&self) -> Option<String> {
        self.link("run/booted-system")
    }

    /// The store path the system profile currently points at (what the bootloader will boot).
    pub fn profile_system(&self) -> Option<String> {
        let profiles = self.root.join("nix/var/nix/profiles");
        let first = fs::read_link(profiles.join("system")).ok()?;
        let generation = if first.is_absolute() {
            first
        } else {
            profiles.join(first)
        };
        let target = fs::read_link(&generation).ok()?;
        Some(self.logical(&target))
    }

    /// Whether `system` is present in the store and activatable.
    pub fn is_activatable_system(&self, system: &str) -> bool {
        let directory = self.store_path(system);
        directory.join("bin/switch-to-configuration").exists()
            && directory.join("nixos-version").exists()
    }

    /// Switch inhibitors of a system (`<system>/switch-inhibitors`, a JSON object of strings).
    /// A system without the file has none.
    pub fn switch_inhibitors(&self, system: &str) -> Result<BTreeMap<String, String>, String> {
        let path = self.store_path(system).join("switch-inhibitors");
        let contents = match fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(BTreeMap::new());
            }
            Err(error) => return Err(format!("could not read switch inhibitors: {error}")),
        };
        let value = Json::parse(&contents)?;
        let object = value
            .as_object()
            .ok_or_else(|| "switch-inhibitors must be a JSON object".to_owned())?;
        object
            .iter()
            .map(|(key, value)| {
                value
                    .as_str()
                    .map(|value| (key.clone(), value.to_owned()))
                    .ok_or_else(|| "switch-inhibitors values must be strings".to_owned())
            })
            .collect()
    }

    /// Link targets of the reboot-relevant components of a system closure.
    pub fn reboot_components(&self, system: &str) -> BTreeMap<&'static str, Option<String>> {
        let directory = self.store_path(system);
        REBOOT_COMPONENTS
            .iter()
            .map(|component| {
                let target = fs::read_link(directory.join(component))
                    .ok()
                    .map(|target| self.logical(&target));
                (*component, target)
            })
            .collect()
    }

    /// Content of `/etc/relay/managed.nix` as published by the running system, if present.
    pub fn runtime_managed_module(&self) -> Option<Vec<u8>> {
        fs::read(self.root.join("etc/relay/managed.nix")).ok()
    }

    fn link(&self, relative: &str) -> Option<String> {
        let path = self.root.join(relative);
        let target = fs::read_link(&path).ok()?;
        let resolved = if target.is_absolute() {
            target
        } else {
            path.parent()?.join(target)
        };
        Some(self.logical(&resolved))
    }

    /// Map a link target back to the path the host itself would see (strip a fixture root).
    fn logical(&self, target: &Path) -> String {
        let stripped = if self.root == Path::new("/") {
            None
        } else {
            target.strip_prefix(&self.root).ok()
        };
        match stripped {
            Some(stripped) => Path::new("/").join(stripped).to_string_lossy().into_owned(),
            None => target.to_string_lossy().into_owned(),
        }
    }

    fn store_path(&self, system: &str) -> PathBuf {
        self.root.join(system.trim_start_matches('/'))
    }
}

/// What changed between two systems that makes a plain `switch` unsuitable.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Evidence {
    /// Reboot-relevant components whose store path differs.
    pub reboot_components: Vec<String>,
    /// Switch inhibitors that NixOS itself would trip on: `key: old -> new`.
    pub inhibitors: Vec<String>,
}

impl Evidence {
    pub fn requires_reboot(&self) -> bool {
        !self.reboot_components.is_empty() || !self.inhibitors.is_empty()
    }
}

/// Compare `from` (running) and `to` (candidate). Mirrors NixOS' own inhibitor rule: a key that
/// exists in both systems with different values blocks `switch`/`test`.
pub fn compare_systems(host: &Host, from: &str, to: &str) -> Result<Evidence, String> {
    let before = host.reboot_components(from);
    let after = host.reboot_components(to);
    let reboot_components = REBOOT_COMPONENTS
        .iter()
        .filter(|component| before.get(*component) != after.get(*component))
        .map(|component| (*component).to_owned())
        .collect();
    let old = host.switch_inhibitors(from)?;
    let new = host.switch_inhibitors(to)?;
    let inhibitors = old
        .iter()
        .filter_map(|(key, old_value)| {
            new.get(key)
                .filter(|new_value| *new_value != old_value)
                .map(|new_value| format!("{key}: {old_value} -> {new_value}"))
        })
        .collect();
    Ok(Evidence {
        reboot_components,
        inhibitors,
    })
}

#[cfg(test)]
pub(crate) mod fixture {
    use super::Host;
    use std::fs;
    use std::os::unix::fs::symlink;

    /// Create `<root>/<system>/` with the files an activatable NixOS system has.
    pub(crate) fn make_system(host: &Host, system: &str, kernel: &str, inhibitors: &str) {
        let directory = host.root().join(system.trim_start_matches('/'));
        fs::create_dir_all(directory.join("bin")).unwrap();
        fs::write(directory.join("bin/switch-to-configuration"), "#!/bin/sh\n").unwrap();
        fs::write(directory.join("nixos-version"), "26.05").unwrap();
        fs::write(directory.join("switch-inhibitors"), inhibitors).unwrap();
        for component in ["kernel-modules", "initrd", "systemd"] {
            let _ = fs::remove_file(directory.join(component));
            symlink(
                format!("/nix/store/{component}-shared"),
                directory.join(component),
            )
            .unwrap();
        }
        let _ = fs::remove_file(directory.join("kernel"));
        symlink(kernel, directory.join("kernel")).unwrap();
    }

    pub(crate) fn point(host: &Host, link: &str, target: &str) {
        let path = host.root().join(link);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let _ = fs::remove_file(&path);
        symlink(target, &path).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::fixture::{make_system, point};
    use super::{Host, compare_systems};
    use crate::fsutil::testutil::TempDir;
    use std::fs;

    const OLD: &str = "/nix/store/aaaa-nixos-system-old";
    const NEW: &str = "/nix/store/bbbb-nixos-system-new";

    #[test]
    fn reads_current_booted_and_profile_systems_relative_to_a_fixture_root() {
        let dir = TempDir::new("host");
        let host = Host::new(dir.path());
        make_system(&host, OLD, "/nix/store/kernel-1", "{}");
        make_system(&host, NEW, "/nix/store/kernel-1", "{}");
        point(&host, "run/current-system", OLD);
        point(&host, "run/booted-system", NEW);
        point(&host, "nix/var/nix/profiles/system-7-link", OLD);
        point(&host, "nix/var/nix/profiles/system", "system-7-link");
        assert_eq!(host.current_system().as_deref(), Some(OLD));
        assert_eq!(host.booted_system().as_deref(), Some(NEW));
        assert_eq!(host.profile_system().as_deref(), Some(OLD));
        assert!(host.is_activatable_system(OLD));
        assert!(!host.is_activatable_system("/nix/store/missing-system"));
    }

    #[test]
    fn unchanged_closures_need_no_reboot() {
        let dir = TempDir::new("host");
        let host = Host::new(dir.path());
        make_system(&host, OLD, "/nix/store/kernel-1", r#"{"a":"1"}"#);
        make_system(&host, NEW, "/nix/store/kernel-1", r#"{"a":"1"}"#);
        let evidence = compare_systems(&host, OLD, NEW).unwrap();
        assert!(!evidence.requires_reboot(), "{evidence:?}");
    }

    #[test]
    fn kernel_change_requires_a_reboot() {
        let dir = TempDir::new("host");
        let host = Host::new(dir.path());
        make_system(&host, OLD, "/nix/store/kernel-1", "{}");
        make_system(&host, NEW, "/nix/store/kernel-2", "{}");
        let evidence = compare_systems(&host, OLD, NEW).unwrap();
        assert_eq!(evidence.reboot_components, ["kernel"]);
        assert!(evidence.requires_reboot());
    }

    #[test]
    fn changed_switch_inhibitors_are_detected_like_nixos_does() {
        let dir = TempDir::new("host");
        let host = Host::new(dir.path());
        make_system(
            &host,
            OLD,
            "/nix/store/kernel-1",
            r#"{"systemd":"257","gone":"x"}"#,
        );
        make_system(
            &host,
            NEW,
            "/nix/store/kernel-1",
            r#"{"systemd":"258","new":"y"}"#,
        );
        let evidence = compare_systems(&host, OLD, NEW).unwrap();
        // Only keys present in both generations with different values inhibit.
        assert_eq!(evidence.inhibitors, ["systemd: 257 -> 258"]);
    }

    #[test]
    fn missing_inhibitor_files_mean_none_and_malformed_ones_are_errors() {
        let dir = TempDir::new("host");
        let host = Host::new(dir.path());
        make_system(&host, OLD, "/nix/store/kernel-1", "{}");
        fs::remove_file(
            dir.path()
                .join("nix/store/aaaa-nixos-system-old/switch-inhibitors"),
        )
        .unwrap();
        assert!(host.switch_inhibitors(OLD).unwrap().is_empty());
        make_system(
            &host,
            NEW,
            "/nix/store/kernel-1",
            r#"["not","an","object"]"#,
        );
        assert!(host.switch_inhibitors(NEW).is_err());
        make_system(&host, NEW, "/nix/store/kernel-1", r#"{"k":1}"#);
        assert!(host.switch_inhibitors(NEW).is_err());
    }
}
