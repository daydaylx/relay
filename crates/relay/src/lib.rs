use std::fs;
use std::io;
use std::path::{Path, PathBuf};

mod index;
pub use index::SearchIndex;

#[derive(Debug, PartialEq, Eq)]
pub struct SystemSummary {
    pub hostname: Option<String>,
    pub os_name: Option<String>,
    pub os_version: Option<String>,
    pub kernel: Option<String>,
    pub running_system_path: Option<String>,
    pub active_generation: Option<u64>,
    pub booted_generation: Option<u64>,
    pub config_identity: Option<String>,
    pub nixpkgs_revision: Option<String>,
    pub failed_units: Option<Vec<String>>,
    pub desktop_session: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Generation {
    pub number: u64,
    pub system_path: String,
    pub active: bool,
    pub booted: bool,
}

pub fn system_summary(root: &Path) -> SystemSummary {
    let os_release = read_os_release(&root.join("etc/os-release"));
    let running_system_path = read_link_string(&root.join("run/current-system"));
    let booted_system_path = read_link_string(&root.join("run/booted-system"));
    let host_generations = generations(root).unwrap_or_default();
    SystemSummary {
        hostname: read_trimmed(&root.join("etc/hostname")),
        os_name: os_release.get("NAME").cloned(),
        os_version: os_release.get("VERSION_ID").cloned(),
        kernel: read_trimmed(&root.join("proc/sys/kernel/osrelease")),
        active_generation: generation_for_path(&host_generations, running_system_path.as_deref()),
        booted_generation: generation_for_path(&host_generations, booted_system_path.as_deref()),
        running_system_path,
        // These require a validated, host-specific NixOS metadata source.
        config_identity: None,
        nixpkgs_revision: None,
        // Do not infer health or desktop state from incomplete fixture/root data.
        failed_units: None,
        desktop_session: None,
    }
}

pub fn generations(root: &Path) -> io::Result<Vec<Generation>> {
    let profile_dir = root.join("nix/var/nix/profiles");
    let active_path = read_link_string(&root.join("run/current-system"));
    let booted_path = read_link_string(&root.join("run/booted-system"));
    let mut found = Vec::new();

    for entry in fs::read_dir(profile_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(number) = generation_number(&name) else {
            continue;
        };
        let path = read_link_string(&entry.path())
            .unwrap_or_else(|| entry.path().to_string_lossy().into_owned());
        found.push(Generation {
            active: active_path.as_deref() == Some(path.as_str()),
            booted: booted_path.as_deref() == Some(path.as_str()),
            number,
            system_path: path,
        });
    }

    found.sort_by_key(|generation| generation.number);
    Ok(found)
}

fn generation_for_path(generations: &[Generation], path: Option<&str>) -> Option<u64> {
    let path = path?;
    generations
        .iter()
        .find(|generation| generation.system_path == path)
        .map(|generation| generation.number)
}

fn generation_number(name: &str) -> Option<u64> {
    name.strip_prefix("system-")?
        .strip_suffix("-link")?
        .parse()
        .ok()
}

fn read_os_release(path: &Path) -> std::collections::BTreeMap<String, String> {
    let mut values = std::collections::BTreeMap::new();
    let Ok(contents) = fs::read_to_string(path) else {
        return values;
    };
    for line in contents.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value);
        values.insert(key.trim().to_owned(), value.to_owned());
    }
    values
}

fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn read_link_string(path: &Path) -> Option<String> {
    let target: PathBuf = fs::read_link(path).ok()?;
    let resolved = if target.is_absolute() {
        target
    } else {
        path.parent()?.join(target)
    };
    Some(resolved.to_string_lossy().into_owned())
}

pub fn json_string(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character.is_control() => {
                use std::fmt::Write;
                let _ = write!(output, "\\u{:04x}", character as u32);
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

#[cfg(test)]
mod tests {
    use super::{generation_number, generations, json_string, system_summary};
    use std::fs;
    use std::path::PathBuf;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::current_dir()
                .unwrap()
                .join("target")
                .join(format!("relay-fixture-{}-{nonce}", std::process::id()));
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }

        fn root(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn parses_only_system_generation_links() {
        assert_eq!(generation_number("system-42-link"), Some(42));
        assert_eq!(generation_number("system-link"), None);
        assert_eq!(generation_number("system-2"), None);
        assert_eq!(generation_number("profile-42-link"), None);
    }

    #[test]
    fn escapes_json_control_characters() {
        assert_eq!(json_string("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
    }

    #[test]
    fn distinguishes_active_and_booted_generations_from_fixture_links() {
        use std::os::unix::fs::symlink;

        let fixture = Fixture::new();
        let profile = fixture.root().join("nix/var/nix/profiles");
        let run = fixture.root().join("run");
        fs::create_dir_all(&profile).unwrap();
        fs::create_dir_all(&run).unwrap();
        symlink(
            "/nix/store/system-generation-41",
            profile.join("system-41-link"),
        )
        .unwrap();
        symlink(
            "/nix/store/system-generation-42",
            profile.join("system-42-link"),
        )
        .unwrap();
        symlink(
            "/nix/store/system-generation-42",
            run.join("current-system"),
        )
        .unwrap();
        symlink("/nix/store/system-generation-41", run.join("booted-system")).unwrap();

        let generations = generations(fixture.root()).unwrap();
        assert_eq!(generations.len(), 2);
        assert_eq!(
            (
                generations[0].number,
                generations[0].active,
                generations[0].booted
            ),
            (41, false, true)
        );
        assert_eq!(
            (
                generations[1].number,
                generations[1].active,
                generations[1].booted
            ),
            (42, true, false)
        );

        let summary = system_summary(fixture.root());
        assert_eq!(summary.active_generation, Some(42));
        assert_eq!(summary.booted_generation, Some(41));
        assert_eq!(
            summary.running_system_path.as_deref(),
            Some("/nix/store/system-generation-42")
        );
        assert_eq!(summary.config_identity, None);
        assert_eq!(summary.nixpkgs_revision, None);
    }

    #[test]
    fn missing_root_metadata_is_reported_as_unavailable() {
        let fixture = Fixture::new();
        let summary = system_summary(fixture.root());
        assert_eq!(summary.hostname, None);
        assert_eq!(summary.active_generation, None);
        assert_eq!(summary.booted_generation, None);
        assert_eq!(summary.failed_units, None);
        assert_eq!(summary.desktop_session, None);
    }
}
