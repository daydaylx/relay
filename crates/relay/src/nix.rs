//! The single adapter that constructs Nix and NixOS commands. Nothing outside this module builds
//! `nix`, `nix-env` or `switch-to-configuration` command lines, and only this module can create
//! privileged invocations (one typed action each, never an arbitrary command).

use std::fmt;
use std::path::Path;
use std::sync::Arc;

use crate::exec::{Invocation, Outcome, ProcessRunner, Runner};

const SYSTEM_PROFILE: &str = "/nix/var/nix/profiles/system";
const FEATURES: [&str; 2] = ["--extra-experimental-features", "nix-command flakes"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexKind {
    Options,
    Packages,
}

impl IndexKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Options => "option",
            Self::Packages => "package",
        }
    }
}

/// How `switch-to-configuration` should treat a prepared system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activation {
    /// Preview only; changes nothing on the running system.
    DryActivate,
    /// Temporary activation. Does not touch the boot configuration and is not a rollback.
    Test,
    Switch,
    Boot,
}

impl Activation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DryActivate => "dry-activate",
            Self::Test => "test",
            Self::Switch => "switch",
            Self::Boot => "boot",
        }
    }
}

/// Where a flake is evaluated from.
#[derive(Clone, Copy, Debug)]
pub enum FlakeSource<'a> {
    /// The user's real flake directory (git-aware, exactly as `nixos-rebuild` would see it).
    Live(&'a Path),
    /// A Relay-owned copy; always addressed as `path:` so an enclosing git repository is ignored.
    Isolated(&'a Path),
}

/// A failed command. `message` is safe to print; `log` may contain evaluated values and belongs
/// only in a private (0600) diagnostic file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NixError {
    pub message: String,
    pub log: Vec<u8>,
}

impl NixError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            log: Vec::new(),
        }
    }
}

impl fmt::Display for NixError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl From<String> for NixError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<NixError> for String {
    fn from(error: NixError) -> Self {
        error.message
    }
}

#[derive(Clone)]
pub struct NixAdapter {
    nix: String,
    runner: Arc<dyn Runner>,
}

impl fmt::Debug for NixAdapter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NixAdapter")
            .field("nix", &self.nix)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildResult {
    pub derivation_path: String,
    pub output_path: String,
}

impl Default for NixAdapter {
    fn default() -> Self {
        Self::new("nix")
    }
}

impl NixAdapter {
    pub fn new(executable: impl AsRef<Path>) -> Self {
        Self::with_runner(
            executable.as_ref().to_string_lossy().into_owned(),
            Arc::new(ProcessRunner::default()),
        )
    }

    pub fn with_runner(executable: impl Into<String>, runner: Arc<dyn Runner>) -> Self {
        Self {
            nix: executable.into(),
            runner,
        }
    }

    pub fn runner(&self) -> Arc<dyn Runner> {
        Arc::clone(&self.runner)
    }

    /// Return the recursive closure for an already resolved store executable. Callers use this
    /// to expose only that executable's immutable runtime to an isolated observer process.
    pub fn store_closure(&self, store_path: &str) -> Result<Vec<String>, NixError> {
        validate_store_path(store_path).map_err(NixError::new)?;
        let output = self.run(
            Invocation::new(&self.nix)
                .args(["path-info", "--recursive", "--json", store_path])
                .timeout(std::time::Duration::from_secs(20))
                .output_limit(1024 * 1024),
            "Nix store closure query",
        )?;
        let value = crate::json::Json::parse(&output.stdout_text()?)
            .map_err(|_| NixError::new("Nix returned invalid store closure data"))?;
        let object = value
            .as_object()
            .ok_or_else(|| NixError::new("Nix returned invalid store closure data"))?;
        if object.is_empty() || object.len() > 4096 {
            return Err(NixError::new("Nix store closure is empty or too large"));
        }
        let mut paths = Vec::with_capacity(object.len());
        for path in object.keys() {
            validate_store_path(path).map_err(NixError::new)?;
            if path.bytes().any(|byte| byte.is_ascii_whitespace()) {
                return Err(NixError::new("Nix returned an invalid store closure path"));
            }
            paths.push(path.clone());
        }
        if !paths.iter().any(|path| path == store_path) {
            return Err(NixError::new(
                "Nix closure omitted its requested executable",
            ));
        }
        Ok(paths)
    }

    pub fn version(&self) -> Result<String, NixError> {
        let output = self.run(Invocation::new(&self.nix).arg("--version"), "nix --version")?;
        let version = output.stdout_text()?;
        if version.is_empty() {
            return Err(NixError::new("Nix returned an empty version string"));
        }
        Ok(version)
    }

    /// Evaluate the toplevel derivation of the live flake without changing host configuration.
    pub fn evaluate_toplevel(&self, flake: &Path, host: &str) -> Result<String, NixError> {
        self.evaluate_toplevel_in(FlakeSource::Live(flake), host)
    }

    pub fn evaluate_toplevel_in(
        &self,
        source: FlakeSource<'_>,
        host: &str,
    ) -> Result<String, NixError> {
        let reference = attribute_reference(source, host, "config.system.build.toplevel.drvPath")?;
        let output = self.run(
            Invocation::new(&self.nix).args(FEATURES).args([
                "eval",
                "--raw",
                "--no-write-lock-file",
                &reference,
            ]),
            "Nix evaluation",
        )?;
        let path = output.stdout_text()?;
        validate_store_path(&path)?;
        if !path.ends_with(".drv") {
            return Err(NixError::new(
                "Nix returned a toplevel that is not a derivation",
            ));
        }
        Ok(path)
    }

    /// The store path the host configuration would build to (evaluation only, nothing is built).
    /// Equal to the running system exactly when the source is fully applied.
    pub fn evaluate_toplevel_output(
        &self,
        source: FlakeSource<'_>,
        host: &str,
    ) -> Result<String, NixError> {
        let reference = attribute_reference(source, host, "config.system.build.toplevel.outPath")?;
        let output = self.run(
            Invocation::new(&self.nix).args(FEATURES).args([
                "eval",
                "--raw",
                "--no-write-lock-file",
                &reference,
            ]),
            "Nix evaluation",
        )?;
        let path = output.stdout_text()?;
        validate_store_path(&path)?;
        Ok(path)
    }

    /// Whether the host configuration imports `relay/managed.nix` (the module publishes
    /// `/etc/relay/managed.nix`, which makes the import observable without parsing Nix).
    pub fn imports_managed_module(
        &self,
        source: FlakeSource<'_>,
        host: &str,
    ) -> Result<bool, NixError> {
        let reference = attribute_reference(source, host, "config.environment.etc")?;
        let output = self.run(
            Invocation::new(&self.nix).args(FEATURES).args([
                "eval",
                "--json",
                "--no-write-lock-file",
                "--apply",
                "etc: etc ? \"relay/managed.nix\"",
                &reference,
            ]),
            "Nix evaluation",
        )?;
        match output.stdout_text()?.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(NixError::new(
                "Nix returned an unexpected import check result",
            )),
        }
    }

    /// Build a previously evaluated derivation. Nix may fetch locked inputs absent from the store.
    pub fn build(&self, derivation_path: &str) -> Result<String, NixError> {
        validate_store_path(derivation_path)?;
        if !derivation_path.ends_with(".drv") {
            return Err(NixError::new(
                "build requires a derivation store path ending in '.drv'",
            ));
        }
        let output_selection = format!("{derivation_path}^out");
        let output = self.run(
            Invocation::new(&self.nix).args(FEATURES).args([
                "build",
                "--no-link",
                "--print-out-paths",
                &output_selection,
            ]),
            "Nix build",
        )?;
        let paths = output.stdout_text()?;
        let mut lines = paths.lines();
        let output_path = lines.next().unwrap_or_default().to_owned();
        if lines.next().is_some() {
            return Err(NixError::new(
                "Nix returned multiple build output paths; expected one toplevel output",
            ));
        }
        validate_store_path(&output_path)?;
        Ok(output_path)
    }

    pub fn evaluate_and_build(&self, flake: &Path, host: &str) -> Result<BuildResult, NixError> {
        let derivation_path = self.evaluate_toplevel(flake, host)?;
        let output_path = self.build(&derivation_path)?;
        Ok(BuildResult {
            derivation_path,
            output_path,
        })
    }

    /// Human-readable closure comparison. Display only: no decision is derived from this text.
    pub fn diff_closures(&self, from: &str, to: &str) -> Result<String, NixError> {
        validate_system_path(from)?;
        validate_system_path(to)?;
        let output = self.run(
            Invocation::new(&self.nix)
                .args(FEATURES)
                .args(["store", "diff-closures", from, to])
                .env("NO_COLOR", "1"),
            "Nix closure diff",
        )?;
        Ok(strip_ansi(&output.stdout_text()?))
    }

    /// Generate a schema-v1 option or package index from the locked host flake.
    pub fn generate_index_json(
        &self,
        flake: &Path,
        host: &str,
        kind: IndexKind,
    ) -> Result<String, NixError> {
        self.eval_index(flake, host, kind, false)
    }

    /// Return current identity metadata without collecting potentially large entries.
    pub fn current_index_identity_json(
        &self,
        flake: &Path,
        host: &str,
        kind: IndexKind,
    ) -> Result<String, NixError> {
        self.eval_index(flake, host, kind, true)
    }

    fn eval_index(
        &self,
        flake: &Path,
        host: &str,
        kind: IndexKind,
        identity_only: bool,
    ) -> Result<String, NixError> {
        validate_host(host)?;
        let flake = flake
            .canonicalize()
            .map_err(|_| "flake path must point to a readable local flake directory".to_owned())?;
        let flake_text = flake.to_string_lossy();
        if flake_text
            .chars()
            .any(|character| matches!(character, '#' | '?' | '\n' | '\r'))
        {
            return Err(NixError::new(
                "flake path must not contain Nix reference delimiters or newlines",
            ));
        }
        if !flake.is_dir() || !flake.join("flake.lock").is_file() {
            return Err(NixError::new("flake path must contain flake.lock"));
        }
        let reference = format!("{}#nixosConfigurations.{host}", flake.display());
        // Nix stderr can include evaluated values; it stays in `NixError::log`.
        let output = self.run(
            Invocation::new(&self.nix)
                .args(FEATURES)
                .args([
                    "eval",
                    "--json",
                    "--impure",
                    "--no-write-lock-file",
                    "--apply",
                    INDEX_EXPRESSION,
                    &reference,
                ])
                .env("RELAY_FLAKE_PATH", flake.to_string_lossy())
                .env("RELAY_HOST", host)
                .env("RELAY_INDEX_KIND", kind.as_str())
                .env("RELAY_IDENTITY_ONLY", if identity_only { "1" } else { "0" }),
            "Nix index evaluation",
        )?;
        String::from_utf8(output.stdout)
            .map_err(|_| NixError::new("Nix returned non-UTF-8 index output"))
    }

    /// Point the system profile at `system` (creates a new generation). Requires root.
    pub fn set_system_profile(&self, system: &str) -> Result<Outcome, NixError> {
        validate_system_path(system)?;
        self.run_privileged(
            Invocation::privileged("nix-env").args(["-p", SYSTEM_PROFILE, "--set", system]),
            "setting the system profile",
        )
    }

    /// Run `switch-to-configuration` of a prepared system. Requires root. NixOS' own pre-switch
    /// checks (including switch inhibitors) are never bypassed: `NIXOS_NO_CHECK` is scrubbed.
    pub fn activate(&self, system: &str, action: Activation) -> Result<Outcome, NixError> {
        validate_system_path(system)?;
        self.run_privileged(
            Invocation::privileged(format!("{system}/bin/switch-to-configuration"))
                .arg(action.as_str()),
            &format!("switch-to-configuration {}", action.as_str()),
        )
    }

    fn run(&self, invocation: Invocation, what: &str) -> Result<Outcome, NixError> {
        let outcome = self.runner.run(&invocation)?;
        if outcome.success() {
            return Ok(outcome);
        }
        Err(NixError {
            message: format!("{what} failed with {}", describe_status(&outcome)),
            log: outcome.stderr,
        })
    }

    fn run_privileged(&self, invocation: Invocation, what: &str) -> Result<Outcome, NixError> {
        debug_assert!(invocation.is_privileged());
        let outcome = self.runner.run(&invocation)?;
        if outcome.success() {
            return Ok(outcome);
        }
        let mut log = outcome.stdout.clone();
        log.extend_from_slice(&outcome.stderr);
        Err(NixError {
            message: format!("{what} failed with {}", describe_status(&outcome)),
            log,
        })
    }
}

/// Remove terminal escape sequences so display-only text is plain and safe to print.
fn strip_ansi(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(c) = characters.next() {
        if c != '\u{1b}' {
            output.push(c);
            continue;
        }
        if characters.next_if_eq(&'[').is_some() {
            // CSI: parameters/intermediates up to and including the final byte @..~.
            for next in characters.by_ref() {
                if ('\u{40}'..='\u{7e}').contains(&next) {
                    break;
                }
            }
        }
    }
    output
}

fn describe_status(outcome: &Outcome) -> String {
    match outcome.code {
        Some(code) => format!("exit status {code}"),
        None => "a signal".to_owned(),
    }
}

fn attribute_reference(
    source: FlakeSource<'_>,
    host: &str,
    attribute: &str,
) -> Result<String, NixError> {
    validate_host(host)?;
    let (prefix, path) = match source {
        FlakeSource::Live(path) => ("", path),
        FlakeSource::Isolated(path) => ("path:", path),
    };
    let text = path.to_string_lossy();
    if !path.is_absolute() {
        return Err(NixError::new("flake path must be absolute"));
    }
    let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | '+');
    let isolated = !prefix.is_empty();
    if text
        .chars()
        .any(|c| matches!(c, '#' | '?' | '\n' | '\r' | '\0'))
        || (isolated && !text.chars().all(allowed))
    {
        return Err(NixError::new(
            "flake path contains characters that are not allowed in a Nix flake reference",
        ));
    }
    Ok(format!(
        "{prefix}{text}#nixosConfigurations.{host}.{attribute}"
    ))
}

const INDEX_EXPRESSION: &str = r#"
  n:
  let
    root = builtins.getEnv "RELAY_FLAKE_PATH";
    host = builtins.getEnv "RELAY_HOST";
    kind = builtins.getEnv "RELAY_INDEX_KIND";
    identityOnly = builtins.getEnv "RELAY_IDENTITY_ONLY" == "1";
    lock = builtins.fromJSON (builtins.readFile (root + "/flake.lock"));
    rootNode = lock.nodes.${lock.root};
    nixpkgsInput = rootNode.inputs.nixpkgs;
    nixpkgsNode = if builtins.isString nixpkgsInput then lock.nodes.${nixpkgsInput} else null;
    sanitizeJson = depth: value:
      let type = builtins.typeOf value;
      in if depth > 8 then null
         else if type == "string" then builtins.unsafeDiscardStringContext value
         else if type == "path" then builtins.unsafeDiscardStringContext (toString value)
         else if type == "list" then map (sanitizeJson (depth + 1)) value
         else if type == "set" then
           if (value.type or null) == "derivation" then null
           else builtins.mapAttrs (_: item: sanitizeJson (depth + 1) item) value
         else if type == "int" || type == "bool" || type == "float" || type == "null" then value
         else null;
    safeJson = value:
      let sanitized = sanitizeJson 0 value;
          encoded = builtins.tryEval (builtins.deepSeq sanitized (builtins.toJSON sanitized));
      in if encoded.success then builtins.fromJSON encoded.value else null;
    flattenOptions = prefix: value:
      builtins.concatLists (builtins.map (key:
        let item = value.${key}; name = if prefix == "" then key else prefix + "." + key;
        in if builtins.isAttrs item && item ? type && builtins.isAttrs item.type && item.type ? name then
          [{
            inherit name;
            type = item.type.name;
            # Resolving arbitrary defaults can force host configuration and may
            # fail independently of the option schema. Keep the cache metadata-only.
            default = null;
            description = if builtins.isString (item.description or "") then item.description or "" else "";
            example = if item ? example then safeJson item.example else null;
            declarations = if builtins.isList (item.declarations or []) then safeJson (item.declarations or []) else [];
            readOnly = (item.readOnly or false) == true;
            relatedPackages = [];
          }]
        else if builtins.isAttrs item then flattenOptions name item else []
      ) (builtins.attrNames value));
    packageEntry = name:
      let evaluated = builtins.tryEval (let
        package = n.pkgs.${name};
        description = if builtins.isAttrs package && (package.type or null) == "derivation"
          then package.meta.description or "" else "";
        entry = if builtins.isAttrs package && (package.type or null) == "derivation" then {
          inherit name;
          description = if builtins.isString description then description else "";
        } else null;
      in builtins.deepSeq entry entry);
      in if evaluated.success then evaluated.value else null;
    packages = builtins.filter (entry: entry != null)
      (map packageEntry (builtins.attrNames n.pkgs));
    entries = if identityOnly then [] else if kind == "option" then flattenOptions "" n.options else packages;
  in {
    schemaVersion = 1;
    inherit kind entries;
    host = n.config.networking.hostName or host;
    nixpkgsRevision = if nixpkgsNode == null then "unknown" else nixpkgsNode.locked.rev or "unknown";
    lockfileHash = builtins.hashFile "sha256" (root + "/flake.lock");
    configIdentity = n.config.system.build.toplevel.drvPath;
    targetSystem = n.config.nixpkgs.hostPlatform.system;
  }
"#;

pub(crate) fn validate_host(host: &str) -> Result<(), String> {
    if host.is_empty()
        || !host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("invalid NixOS flake host name".into());
    }
    Ok(())
}

pub(crate) fn validate_store_path(path: &str) -> Result<(), String> {
    if !path.starts_with("/nix/store/") || path.contains('\n') || path.contains('\0') {
        return Err("Nix returned an invalid store path".into());
    }
    Ok(())
}

/// A single top-level store entry (no sub-paths). Required before anything is run as root.
pub(crate) fn validate_system_path(path: &str) -> Result<(), String> {
    let name = path.strip_prefix("/nix/store/").unwrap_or_default();
    if name.is_empty()
        || name.starts_with('.')
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '+' | '=' | '?'))
    {
        return Err("system path must be a single Nix store entry".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        Activation, FlakeSource, NixAdapter, attribute_reference, validate_host,
        validate_store_path, validate_system_path,
    };
    use crate::exec::{Invocation, Outcome, Runner};
    use std::fs;
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn closure_diffs_are_stripped_of_terminal_escapes() {
        assert_eq!(
            super::strip_ansi("hello: ∅ → 2.12.3, \u{1b}[31;1m273.1 KiB\u{1b}[0m\n"),
            "hello: ∅ → 2.12.3, 273.1 KiB\n"
        );
        assert_eq!(super::strip_ansi("a\u{1b}b"), "ab");
    }

    #[test]
    fn rejects_host_names_that_could_change_the_flake_reference() {
        assert!(validate_host("workstation").is_ok());
        assert!(validate_host("host;builtins.abort").is_err());
        assert!(validate_host("../other").is_err());
    }

    #[test]
    fn accepts_only_single_line_store_paths() {
        assert!(validate_store_path("/nix/store/abc-system").is_ok());
        assert!(validate_store_path("/tmp/system").is_err());
        assert!(validate_store_path("/nix/store/a\n/nix/store/b").is_err());
    }

    #[test]
    fn privileged_actions_accept_only_single_store_entries() {
        assert!(validate_system_path("/nix/store/abc-nixos-system-host-26.05").is_ok());
        for bad in [
            "/nix/store/",
            "/nix/store/abc/bin/sh",
            "/nix/store/../etc/passwd",
            "/nix/store/.hidden",
            "/tmp/abc",
            "/nix/store/abc def",
            "/nix/store/abc;reboot",
        ] {
            assert!(validate_system_path(bad).is_err(), "accepted {bad}");
        }
    }

    #[test]
    fn isolated_candidates_are_addressed_as_path_flakes() {
        let live =
            attribute_reference(FlakeSource::Live(Path::new("/etc/nixos")), "h", "x").unwrap();
        assert_eq!(live, "/etc/nixos#nixosConfigurations.h.x");
        let isolated = attribute_reference(
            FlakeSource::Isolated(Path::new("/home/u/.local/state/relay/candidates/c1/src")),
            "h",
            "x",
        )
        .unwrap();
        assert!(isolated.starts_with("path:/home/u/"));
        assert!(
            attribute_reference(FlakeSource::Isolated(Path::new("/tmp/a b")), "h", "x").is_err()
        );
        assert!(attribute_reference(FlakeSource::Live(Path::new("relative")), "h", "x").is_err());
        assert!(attribute_reference(FlakeSource::Live(Path::new("/a#b")), "h", "x").is_err());
    }

    struct Recording(Mutex<Vec<Invocation>>, Outcome);

    impl Runner for Recording {
        fn run(&self, invocation: &Invocation) -> Result<Outcome, String> {
            self.0.lock().unwrap().push(invocation.clone());
            Ok(self.1.clone())
        }
    }

    #[test]
    fn evaluation_never_writes_the_lock_file_and_failures_keep_logs_private() {
        let runner = Arc::new(Recording(
            Mutex::new(Vec::new()),
            Outcome {
                code: Some(1),
                stdout: Vec::new(),
                stderr: b"error: value 'hunter2' is wrong".to_vec(),
            },
        ));
        let nix = NixAdapter::with_runner("nix", runner.clone());
        let error = nix
            .evaluate_toplevel(Path::new("/home/u/flake"), "host")
            .unwrap_err();
        assert!(!error.message.contains("hunter2"));
        assert!(String::from_utf8_lossy(&error.log).contains("hunter2"));
        let calls = runner.0.lock().unwrap();
        assert!(
            calls[0]
                .arguments()
                .iter()
                .any(|a| a == "--no-write-lock-file")
        );
        assert!(!calls[0].is_privileged());
    }

    #[test]
    fn store_closure_is_structured_bounded_and_confined_to_store_paths() {
        let runner = Arc::new(Recording(
            Mutex::new(Vec::new()),
            Outcome {
                code: Some(0),
                stdout: br#"{"/nix/store/aaa-tool":{},"/nix/store/bbb-lib":{}}"#.to_vec(),
                stderr: Vec::new(),
            },
        ));
        let nix = NixAdapter::with_runner("nix", runner.clone());
        let closure = nix.store_closure("/nix/store/aaa-tool").unwrap();
        assert_eq!(closure, ["/nix/store/aaa-tool", "/nix/store/bbb-lib"]);
        let invocation = runner.0.lock().unwrap();
        assert_eq!(
            invocation[0].arguments()[0..3],
            ["path-info", "--recursive", "--json"]
        );
    }

    #[test]
    fn only_typed_activation_actions_are_privileged_and_use_the_exact_system_path() {
        let runner = Arc::new(Recording(
            Mutex::new(Vec::new()),
            Outcome {
                code: Some(0),
                ..Outcome::default()
            },
        ));
        let nix = NixAdapter::with_runner("nix", runner.clone());
        let system = "/nix/store/aaaa-nixos-system-host";
        nix.set_system_profile(system).unwrap();
        nix.activate(system, Activation::Test).unwrap();
        assert!(
            nix.activate("/nix/store/aaaa/../../bin/sh", Activation::Test)
                .is_err()
        );
        nix.evaluate_toplevel_in(FlakeSource::Live(Path::new("/f")), "h")
            .ok();
        let calls = runner.0.lock().unwrap();
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0].program(), "nix-env");
        assert_eq!(
            calls[0].arguments(),
            ["-p", "/nix/var/nix/profiles/system", "--set", system]
        );
        assert_eq!(
            calls[1].program(),
            "/nix/store/aaaa-nixos-system-host/bin/switch-to-configuration"
        );
        assert_eq!(calls[1].arguments(), ["test"]);
        assert!(calls[0].is_privileged() && calls[1].is_privileged());
        assert!(!calls[2].is_privileged());
    }

    #[test]
    fn build_selects_the_derivation_output_instead_of_the_drv_file() {
        use std::os::unix::fs::PermissionsExt;

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let script =
            std::env::temp_dir().join(format!("relay-fake-nix-{}-{nonce}", std::process::id()));
        fs::write(
            &script,
            "#!/bin/sh\nlast=\nfor arg do last=$arg; done\n[ \"$last\" = \"/nix/store/test.drv^out\" ] || exit 17\nprintf '%s\\n' '/nix/store/system-output'\n",
        ).unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let result = NixAdapter::new(&script).build("/nix/store/test.drv");
        let _ = fs::remove_file(script);
        assert_eq!(result.unwrap(), "/nix/store/system-output");
    }
}
