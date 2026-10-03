use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Bool(bool),
    Integer(i64),
    String(String),
    StringList(Vec<String>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    SetOption { name: String, value: Value },
    AddPackage { name: String },
    RemovePackage { name: String },
}

/// Ordered by severity so `max` yields the most restrictive classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Risk {
    LiveSwitchable,
    RebootRequired,
    MigrationRequired,
    Protected,
}

impl Risk {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LiveSwitchable => "LIVE_SWITCHABLE",
            Self::RebootRequired => "REBOOT_REQUIRED",
            Self::MigrationRequired => "MIGRATION_REQUIRED",
            Self::Protected => "PROTECTED",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "LIVE_SWITCHABLE" => Some(Self::LiveSwitchable),
            "REBOOT_REQUIRED" => Some(Self::RebootRequired),
            "MIGRATION_REQUIRED" => Some(Self::MigrationRequired),
            "PROTECTED" => Some(Self::Protected),
            _ => None,
        }
    }
}

/// Everything Relay manages, as data. `relay/managed.nix` is a pure function of this state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ManagedState {
    pub options: BTreeMap<String, Value>,
    pub packages: BTreeSet<String>,
}

impl ManagedState {
    pub fn apply(&mut self, change: &Change) -> Result<(), String> {
        validate(change)?;
        match change {
            Change::SetOption { name, value } => {
                self.options.insert(name.clone(), value.clone());
            }
            Change::AddPackage { name } => {
                self.packages.insert(name.clone());
            }
            Change::RemovePackage { name } => {
                if !self.packages.remove(name) {
                    return Err(format!(
                        "cannot remove package '{name}' without it being present in the managed baseline"
                    ));
                }
            }
        }
        Ok(())
    }
}

const HEADER: &str =
    "# Managed by Relay. Edit only through `relay plan` and `relay apply`.\n{ pkgs, ... }:\n{\n";
/// Makes the applied managed state observable at runtime (`/etc/relay/managed.nix`) and proves
/// that the host configuration really imports this file.
const ETC_STAMP: &str = "  environment.etc.\"relay/managed.nix\".source = ./managed.nix;\n";
const PACKAGES_OPEN: &str = "  environment.systemPackages = with pkgs; [\n";
const PACKAGES_CLOSE: &str = "  ];\n";
const MAX_LIST_ITEMS: usize = 256;

pub fn validate(change: &Change) -> Result<Risk, String> {
    let risk = match change {
        Change::SetOption { name, value } => {
            validate_option_name(name)?;
            validate_value(value)?;
            classify_option(name)
        }
        Change::AddPackage { name } | Change::RemovePackage { name } => {
            validate_package_name(name)?;
            Risk::LiveSwitchable
        }
    };
    if risk == Risk::Protected {
        let category = match change {
            Change::SetOption { name, .. } => protected_category(&name.to_ascii_lowercase()),
            _ => None,
        };
        return Err(format!(
            "change targets a protected resource{} and cannot be applied automatically",
            category.map_or(String::new(), |label| format!(" ({label})"))
        ));
    }
    Ok(risk)
}

/// Classify a whole change set; protected members make the set invalid.
pub fn classify(changes: &[Change]) -> Result<Risk, String> {
    if changes.is_empty() {
        return Err("a change set must contain at least one change".into());
    }
    let mut risk = Risk::LiveSwitchable;
    for change in changes {
        risk = risk.max(validate(change)?);
    }
    Ok(risk)
}

pub fn render(changes: &[Change]) -> Result<String, String> {
    render_with_packages(&[], changes)
}

pub fn render_with_packages(
    current_packages: &[String],
    changes: &[Change],
) -> Result<String, String> {
    let mut state = ManagedState::default();
    for package in current_packages {
        validate_package_name(package)?;
        state.packages.insert(package.clone());
    }
    for change in changes {
        state.apply(change)?;
    }
    Ok(render_state(&state))
}

/// Deterministically render the managed module for `state`.
pub fn render_state(state: &ManagedState) -> String {
    let mut output = String::from(HEADER);
    output.push_str(ETC_STAMP);
    if !state.packages.is_empty() {
        output.push_str(PACKAGES_OPEN);
        for package in &state.packages {
            output.push_str("    ");
            output.push_str(package);
            output.push('\n');
        }
        output.push_str(PACKAGES_CLOSE);
    }
    for (name, value) in &state.options {
        output.push_str("  ");
        output.push_str(name);
        output.push_str(" = ");
        output.push_str(&render_value(value));
        output.push_str(";\n");
    }
    output.push_str("}\n");
    output
}

/// Read a managed module back into data. Only Relay's own canonical rendering is accepted: the
/// result must re-render to the identical bytes, so manual edits are reported instead of guessed.
pub fn parse_managed(text: &str) -> Result<ManagedState, String> {
    const EDITED: &str = "managed.nix is not in Relay's canonical form (edited outside Relay?)";
    let body = text
        .strip_prefix(HEADER)
        .and_then(|rest| rest.strip_prefix(ETC_STAMP))
        .and_then(|rest| rest.strip_suffix("}\n"))
        .ok_or_else(|| EDITED.to_owned())?;
    let mut state = ManagedState::default();
    let mut lines = body.split_inclusive('\n').peekable();
    if lines.peek().copied() == Some(PACKAGES_OPEN) {
        lines.next();
        loop {
            let line = lines.next().ok_or_else(|| EDITED.to_owned())?;
            if line == PACKAGES_CLOSE {
                break;
            }
            let name = line
                .strip_prefix("    ")
                .and_then(|rest| rest.strip_suffix('\n'))
                .ok_or_else(|| EDITED.to_owned())?;
            validate_package_name(name).map_err(|_| EDITED.to_owned())?;
            state.packages.insert(name.to_owned());
        }
    }
    for line in lines {
        let assignment = line
            .strip_prefix("  ")
            .and_then(|rest| rest.strip_suffix(";\n"))
            .ok_or_else(|| EDITED.to_owned())?;
        let (name, value) = assignment
            .split_once(" = ")
            .ok_or_else(|| EDITED.to_owned())?;
        validate_option_name(name).map_err(|_| EDITED.to_owned())?;
        let value = parse_value(value).ok_or_else(|| EDITED.to_owned())?;
        state.options.insert(name.to_owned(), value);
    }
    if render_state(&state) != text {
        return Err(EDITED.to_owned());
    }
    Ok(state)
}

fn validate_option_name(name: &str) -> Result<(), String> {
    if name == "environment.systemPackages" {
        return Err("use add-package/remove-package for environment.systemPackages".into());
    }
    let segments = name.split('.').collect::<Vec<_>>();
    if segments.iter().any(|segment| {
        segment.is_empty()
            || !segment
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            || !segment
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    }) {
        return Err(format!("invalid Nix option path '{name}'"));
    }
    Ok(())
}

fn validate_package_name(name: &str) -> Result<(), String> {
    let mut characters = name.chars();
    let valid_start = characters
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    if !valid_start || !characters.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(format!("invalid package attribute name '{name}'"));
    }
    Ok(())
}

fn validate_value(value: &Value) -> Result<(), String> {
    let strings: Vec<&str> = match value {
        Value::Bool(_) | Value::Integer(_) => return Ok(()),
        Value::String(value) => vec![value],
        Value::StringList(values) => {
            if values.len() > MAX_LIST_ITEMS {
                return Err(format!("lists are limited to {MAX_LIST_ITEMS} items"));
            }
            values.iter().map(String::as_str).collect()
        }
    };
    for value in strings {
        if value.len() > 16_384 {
            return Err("Nix string value exceeds 16384 bytes".into());
        }
        if value
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        {
            return Err("Nix string values must not contain control characters".into());
        }
    }
    Ok(())
}

/// Options Relay must never change automatically, with a human-readable category.
fn protected_category(lower: &str) -> Option<&'static str> {
    const PREFIXES: &[(&str, &str)] = &[
        ("system.stateversion", "system.stateVersion"),
        ("system.autoupgrade", "major release upgrades"),
        ("boot.loader.", "bootloader"),
        ("boot.initrd.", "initrd, disk encryption and early boot"),
        ("boot.zfs.", "filesystems"),
        ("filesystems.", "filesystems"),
        ("swapdevices", "filesystems"),
        ("services.zfs.", "filesystems"),
        ("services.lvm", "filesystems"),
        ("disko.", "partitioning"),
        ("users.mutableusers", "user access"),
        ("users.users.", "user access"),
        ("users.groups.", "user access"),
        ("users.users", "user access"),
        ("security.sudo", "privilege configuration"),
        ("security.doas", "privilege configuration"),
        ("security.polkit", "privilege configuration"),
        ("security.pam.", "authentication"),
        ("services.openssh", "SSH access"),
        ("services.getty", "login access"),
        ("nix.settings.trusted-", "Nix daemon trust"),
        ("nix.settings.substituters", "Nix daemon trust"),
        ("nix.settings.allowed-users", "Nix daemon trust"),
        ("nix.settings.require-sigs", "Nix daemon trust"),
        ("nix.trustedusers", "Nix daemon trust"),
        ("nix.extraoptions", "Nix daemon trust"),
        ("sops.", "secret management"),
        ("age.", "secret management"),
    ];
    for (prefix, label) in PREFIXES {
        if lower.starts_with(prefix) {
            return Some(label);
        }
    }
    const SUBSTRINGS: &[(&str, &str)] = &[
        ("luks", "disk encryption"),
        ("secureboot", "Secure Boot"),
        ("secure.boot", "Secure Boot"),
        ("lanzaboote", "Secure Boot"),
        ("partition", "partitioning"),
        ("trusted-users", "Nix daemon trust"),
        // Secrets must never end up in managed.nix, the Nix store or the journal.
        ("password", "secret material"),
        ("passwd", "secret material"),
        ("passphrase", "secret material"),
        ("secret", "secret material"),
        ("token", "secret material"),
        ("privatekey", "secret material"),
        ("private-key", "secret material"),
        ("apikey", "secret material"),
        ("api-key", "secret material"),
        ("credential", "secret material"),
        ("psk", "secret material"),
    ];
    for (needle, label) in SUBSTRINGS {
        if lower.contains(needle) {
            return Some(label);
        }
    }
    const DATABASES: &[&str] = &[
        "postgresql",
        "mysql",
        "mariadb",
        "mongodb",
        "redis",
        "influxdb",
        "couchdb",
        "neo4j",
        "clickhouse",
        "cassandra",
        "elasticsearch",
        "opensearch",
    ];
    if lower.starts_with("services.")
        && lower.ends_with(".package")
        && DATABASES.iter().any(|database| lower.contains(database))
    {
        return Some("database major-version migration");
    }
    None
}

/// Name-based classification. Closure evidence (kernel, initrd, switch inhibitors) can only
/// escalate this later; it never lowers it.
fn classify_option(name: &str) -> Risk {
    let lower = name.to_ascii_lowercase();
    if protected_category(&lower).is_some() {
        Risk::Protected
    } else if lower.starts_with("services.")
        && (lower.contains("database") || lower.contains("postgresql") || lower.contains("mysql"))
    {
        Risk::MigrationRequired
    } else if lower.starts_with("boot.") || is_session_foundation(&lower) {
        Risk::RebootRequired
    } else {
        Risk::LiveSwitchable
    }
}

/// Display manager and session plumbing: changing it live could end the user's graphical session,
/// so such changes only take effect through the boot path.
fn is_session_foundation(lower: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "services.displaymanager.",
        "services.xserver.displaymanager.",
        "services.greetd.",
        "programs.regreet.",
        "programs.uwsm.",
        "services.cage.",
    ];
    PREFIXES.iter().any(|prefix| lower.starts_with(prefix))
}

fn render_value(value: &Value) -> String {
    match value {
        Value::Bool(value) => value.to_string(),
        Value::Integer(value) => value.to_string(),
        Value::String(value) => nix_string(value),
        Value::StringList(values) if values.is_empty() => "[ ]".to_owned(),
        Value::StringList(values) => format!(
            "[ {} ]",
            values
                .iter()
                .map(|value| nix_string(value))
                .collect::<Vec<_>>()
                .join(" ")
        ),
    }
}

fn nix_string(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    let mut characters = value.chars().peekable();
    while let Some(c) = characters.next() {
        match c {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '$' if characters.peek() == Some(&'{') => {
                let _ = characters.next();
                output.push_str("\\${");
            }
            // Rejected by validation; never emit raw control characters into Nix source.
            c if c.is_control() => output.push(' '),
            c => output.push(c),
        }
    }
    output.push('"');
    output
}

fn parse_value(text: &str) -> Option<Value> {
    match text {
        "true" => return Some(Value::Bool(true)),
        "false" => return Some(Value::Bool(false)),
        "[ ]" => return Some(Value::StringList(Vec::new())),
        _ => {}
    }
    if text.starts_with('"') {
        let (value, rest) = parse_nix_string(text)?;
        return rest.is_empty().then_some(Value::String(value));
    }
    if let Some(inner) = text
        .strip_prefix("[ ")
        .and_then(|rest| rest.strip_suffix(" ]"))
    {
        let mut values = Vec::new();
        let mut rest = inner;
        loop {
            let (value, remaining) = parse_nix_string(rest)?;
            values.push(value);
            if remaining.is_empty() {
                return Some(Value::StringList(values));
            }
            rest = remaining.strip_prefix(' ')?;
        }
    }
    let digits = text.strip_prefix('-').unwrap_or(text);
    if !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return text.parse().ok().map(Value::Integer);
    }
    None
}

/// Parse one leading Nix string literal and return it with the unparsed remainder.
fn parse_nix_string(text: &str) -> Option<(String, &str)> {
    let mut characters = text.strip_prefix('"')?.char_indices();
    let mut value = String::new();
    while let Some((index, c)) = characters.next() {
        match c {
            '"' => return Some((value, &text[index + 2..])),
            '\\' => match characters.next()?.1 {
                '"' => value.push('"'),
                '\\' => value.push('\\'),
                'n' => value.push('\n'),
                'r' => value.push('\r'),
                't' => value.push('\t'),
                '$' => {
                    if characters.next()?.1 != '{' {
                        return None;
                    }
                    value.push_str("${");
                }
                _ => return None,
            },
            c => value.push(c),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        Change, ManagedState, Risk, Value, classify, parse_managed, render, render_state,
        render_with_packages, validate,
    };

    fn set(name: &str, value: Value) -> Change {
        Change::SetOption {
            name: name.into(),
            value,
        }
    }

    #[test]
    fn renders_changes_deterministically_and_escapes_strings() {
        let changes = vec![
            set(
                "services.example.message",
                Value::String("hello\"world".into()),
            ),
            Change::AddPackage { name: "vlc".into() },
        ];
        assert_eq!(
            render(&changes).unwrap(),
            "# Managed by Relay. Edit only through `relay plan` and `relay apply`.\n{ pkgs, ... }:\n{\n  environment.etc.\"relay/managed.nix\".source = ./managed.nix;\n  environment.systemPackages = with pkgs; [\n    vlc\n  ];\n  services.example.message = \"hello\\\"world\";\n}\n"
        );
        assert_eq!(render(&changes).unwrap(), render(&changes).unwrap());
    }

    #[test]
    fn nix_interpolation_in_user_strings_is_escaped() {
        let rendered = render(&[set(
            "services.example.message",
            Value::String("${builtins.abort \"no\"}".into()),
        )])
        .unwrap();
        assert!(rendered.contains("\\${builtins.abort"));
    }

    #[test]
    fn package_removal_requires_and_updates_a_managed_baseline() {
        let change = Change::RemovePackage { name: "vlc".into() };
        assert!(render(std::slice::from_ref(&change)).is_err());
        let output = render_with_packages(&["vlc".into(), "firefox".into()], &[change]).unwrap();
        assert!(output.contains("firefox"));
        assert!(!output.contains("vlc"));
    }

    #[test]
    fn rejects_protected_and_injection_shaped_changes() {
        let protected = validate(&set("system.stateVersion", Value::String("26.05".into())));
        assert!(protected.unwrap_err().contains("system.stateVersion"));
        assert!(validate(&set("services.x; import /tmp/payload", Value::Bool(true))).is_err());
        assert!(
            validate(&Change::AddPackage {
                name: "x; builtins.abort".into()
            })
            .is_err()
        );
        assert!(
            validate(&Change::AddPackage {
                name: "7zip".into()
            })
            .is_err()
        );
    }

    #[test]
    fn protected_resources_are_blocked_by_code_for_every_mvp_category() {
        for name in [
            "system.stateVersion",
            "boot.loader.grub.device",
            "boot.initrd.luks.devices",
            "fileSystems./.device",
            "swapDevices",
            "boot.loader.systemd-boot.enable",
            "nix.settings.trusted-users",
            "nix.settings.substituters",
            "users.users.root.password",
            "users.mutableUsers",
            "security.sudo.wheelNeedsPassword",
            "security.pam.services.login.text",
            "services.openssh.enable",
            "services.openssh.settings.PermitRootLogin",
            "sops.secrets.token",
            "services.postgresql.package",
            "services.mysql.package",
            "boot.lanzaboote.enable",
        ] {
            assert!(
                validate(&set(name, Value::Bool(true))).is_err(),
                "{name} must be protected"
            );
        }
    }

    #[test]
    fn secret_bearing_option_names_never_reach_managed_nix() {
        for name in [
            "services.foo.password",
            "networking.wireless.networks.home.psk",
            "services.bar.apiKey",
            "services.baz.authToken",
        ] {
            let error = validate(&set(name, Value::String("hunter2".into()))).unwrap_err();
            assert!(error.contains("secret material"), "{name}: {error}");
        }
    }

    #[test]
    fn control_characters_and_duplicate_package_management_are_rejected() {
        assert!(validate(&set("services.x.motd", Value::String("a\u{1b}[31m".into()))).is_err());
        assert!(
            validate(&set(
                "environment.systemPackages",
                Value::StringList(vec!["vlc".into()])
            ))
            .is_err()
        );
        assert!(
            validate(&set(
                "services.x.list",
                Value::StringList(vec!["a".into(); 257])
            ))
            .is_err()
        );
    }

    #[test]
    fn hyprland_options_are_live_but_session_foundations_need_the_boot_path() {
        for name in [
            "programs.hyprland.enable",
            "programs.hyprland.xwayland.enable",
            "programs.hyprland.withUWSM",
        ] {
            assert_eq!(
                validate(&set(name, Value::Bool(true))).unwrap(),
                Risk::LiveSwitchable,
                "{name}"
            );
        }
        for name in [
            "services.displayManager.sddm.enable",
            "services.displayManager.autoLogin.user",
            "services.xserver.displayManager.lightdm.enable",
            "services.greetd.enable",
            "programs.regreet.enable",
            "programs.uwsm.enable",
        ] {
            assert_eq!(
                validate(&set(name, Value::Bool(true))).unwrap(),
                Risk::RebootRequired,
                "{name}"
            );
        }
    }

    #[test]
    fn classifies_supported_option_and_package_changes() {
        assert_eq!(
            validate(&set("hardware.bluetooth.enable", Value::Bool(true))).unwrap(),
            Risk::LiveSwitchable
        );
        assert_eq!(
            validate(&Change::AddPackage { name: "vlc".into() }).unwrap(),
            Risk::LiveSwitchable
        );
        assert_eq!(
            validate(&set(
                "boot.kernelParams",
                Value::StringList(vec!["quiet".into()])
            ))
            .unwrap(),
            Risk::RebootRequired
        );
        assert_eq!(
            validate(&set("services.postgresql.enable", Value::Bool(true))).unwrap(),
            Risk::MigrationRequired
        );
        assert!(Risk::LiveSwitchable < Risk::RebootRequired);
        assert_eq!(
            classify(&[
                Change::AddPackage { name: "vlc".into() },
                set("boot.kernelParams", Value::StringList(vec![])),
            ])
            .unwrap(),
            Risk::RebootRequired
        );
        assert!(classify(&[]).is_err());
    }

    #[test]
    fn managed_module_round_trips_through_the_strict_parser() {
        let mut state = ManagedState::default();
        for change in [
            Change::AddPackage { name: "vlc".into() },
            Change::AddPackage {
                name: "htop".into(),
            },
            set("hardware.bluetooth.enable", Value::Bool(true)),
            set("services.example.port", Value::Integer(-8080)),
            set(
                "services.example.message",
                Value::String("a \"quoted\" \\ ${not} value\nnext\tline".into()),
            ),
            set("services.example.empty", Value::StringList(vec![])),
            set(
                "services.example.items",
                Value::StringList(vec!["one".into(), "t\"wo".into()]),
            ),
        ] {
            state.apply(&change).unwrap();
        }
        let text = render_state(&state);
        assert_eq!(parse_managed(&text).unwrap(), state);
        assert_eq!(render_state(&ManagedState::default()), render(&[]).unwrap());
        assert_eq!(
            parse_managed(&render_state(&ManagedState::default())).unwrap(),
            ManagedState::default()
        );
    }

    #[test]
    fn hand_edited_managed_modules_are_refused_instead_of_guessed() {
        let state = {
            let mut state = ManagedState::default();
            state
                .apply(&set("hardware.bluetooth.enable", Value::Bool(true)))
                .unwrap();
            state
        };
        let canonical = render_state(&state);
        for edited in [
            canonical.replace("true", "lib.mkForce true"),
            format!("{canonical}# trailing comment\n"),
            canonical.replace("{ pkgs, ... }:", "{ pkgs, lib, ... }:"),
            canonical.replace("  hardware", "    hardware"),
            canonical.replace(
                "  environment.etc",
                "  imports = [ ./other.nix ];\n  environment.etc",
            ),
            String::new(),
        ] {
            assert!(parse_managed(&edited).is_err(), "accepted:\n{edited}");
        }
    }
}
