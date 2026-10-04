//! Read-only command execution for agent observation. The command receives one immutable Nix
//! closure, host hardware metadata, a private proc/dev view and a disposable `/tmp`; it receives
//! no home, configuration, session sockets, network, or writeable host filesystem.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::exec::{Invocation, ProcessRunner, Runner};
use crate::nix::NixAdapter;

pub const MAX_ARGUMENTS: usize = 64;
pub const MAX_ARGUMENT_BYTES: usize = 4096;
pub const MAX_OUTPUT_BYTES: usize = 64 * 1024;
const MAX_TOTAL_ARGUMENT_BYTES: usize = 16 * 1024;
const TIME_LIMIT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObserveResult {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandboxError(&'static str);

impl std::fmt::Display for SandboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

pub struct SandboxRunner {
    nix: NixAdapter,
    runner: Box<dyn Runner>,
    bwrap: PathBuf,
    systemd_run: PathBuf,
}

impl Default for SandboxRunner {
    fn default() -> Self {
        Self::new(
            NixAdapter::default(),
            Box::new(ProcessRunner::default()),
            std::env::var_os("RELAY_BWRAP_PATH").map(PathBuf::from),
            PathBuf::from("/run/current-system/sw/bin/systemd-run"),
        )
    }
}

impl SandboxRunner {
    pub fn new(
        nix: NixAdapter,
        runner: Box<dyn Runner>,
        bwrap: Option<PathBuf>,
        systemd_run: PathBuf,
    ) -> Self {
        Self {
            nix,
            runner,
            bwrap: bwrap.unwrap_or_default(),
            systemd_run,
        }
    }

    pub fn run(&self, program: &str, args: &[String]) -> Result<ObserveResult, SandboxError> {
        validate_request(program, args)?;
        if !Path::new("/run/current-system/sw/bin").is_dir()
            || !Path::new("/nix/store").is_dir()
            || !Path::new("/sys").is_dir()
        {
            return Err(SandboxError("OBSERVE sandbox requires a live NixOS host"));
        }
        let bwrap = trusted_store_executable(&self.bwrap).ok_or(SandboxError(
            "Bubblewrap is not configured as a trusted Nix package",
        ))?;
        let systemd_run = self
            .systemd_run
            .canonicalize()
            .map_err(|_| SandboxError("systemd-run is unavailable"))?;
        let systemd_run_text = systemd_run
            .to_str()
            .ok_or(SandboxError("systemd-run path is invalid"))?;
        if !systemd_run_text.starts_with("/nix/store/") {
            return Err(SandboxError("systemd-run is not provided by the Nix store"));
        }

        let host_executable = Path::new("/run/current-system/sw/bin").join(program);
        let executable = resolve_profile_executable(&host_executable)?;
        let executable_text = executable
            .to_str()
            .ok_or(SandboxError("requested diagnostic program path is invalid"))?;
        let store_entry = store_entry(executable_text)?;
        let closure = self
            .nix
            .store_closure(&store_entry)
            .map_err(|_| SandboxError("could not resolve the diagnostic program closure"))?;
        let invocation = build_invocation(&systemd_run, &bwrap, executable_text, args, &closure)?;
        let outcome = self
            .runner
            .run(&invocation)
            .map_err(|_| SandboxError("OBSERVE command could not run inside its sandbox"))?;
        Ok(ObserveResult {
            code: outcome.code,
            stdout: String::from_utf8_lossy(&outcome.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&outcome.stderr).into_owned(),
        })
    }
}

fn validate_request(program: &str, args: &[String]) -> Result<(), SandboxError> {
    if program.is_empty()
        || program.len() > 128
        || !program
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
    {
        return Err(SandboxError("diagnostic program name is invalid"));
    }
    if args.len() > MAX_ARGUMENTS {
        return Err(SandboxError("too many diagnostic arguments"));
    }
    let mut total = 0usize;
    for arg in args {
        if arg.len() > MAX_ARGUMENT_BYTES || arg.contains('\0') {
            return Err(SandboxError("diagnostic argument is invalid or too large"));
        }
        total = total.saturating_add(arg.len());
    }
    if total > MAX_TOTAL_ARGUMENT_BYTES {
        return Err(SandboxError(
            "diagnostic arguments exceed the total size limit",
        ));
    }
    Ok(())
}

fn trusted_store_executable(path: &Path) -> Option<PathBuf> {
    let canonical = path.canonicalize().ok()?;
    let text = canonical.to_str()?;
    if !text.starts_with("/nix/store/") || !canonical.is_file() {
        return None;
    }
    let metadata = fs::metadata(&canonical).ok()?;
    if metadata.permissions().mode() & 0o111 == 0 {
        return None;
    }
    Some(canonical)
}

fn store_entry(executable: &str) -> Result<String, SandboxError> {
    let suffix = executable.strip_prefix("/nix/store/").ok_or(SandboxError(
        "diagnostic executable is outside the Nix store",
    ))?;
    let name = suffix
        .split('/')
        .next()
        .filter(|name| !name.is_empty())
        .ok_or(SandboxError("diagnostic executable path is invalid"))?;
    validate_store_entry(name)?;
    Ok(format!("/nix/store/{name}"))
}

fn validate_store_entry(name: &str) -> Result<(), SandboxError> {
    if name.starts_with('.')
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._+=?".contains(&byte))
    {
        return Err(SandboxError(
            "Nix returned an invalid diagnostic closure path",
        ));
    }
    Ok(())
}

fn resolve_profile_executable(path: &Path) -> Result<PathBuf, SandboxError> {
    let target = match fs::read_link(path) {
        Ok(target) if target.is_absolute() => target,
        Ok(target) => path
            .parent()
            .ok_or(SandboxError("diagnostic executable path is invalid"))?
            .join(target),
        Err(_) => path
            .canonicalize()
            .map_err(|_| SandboxError("requested diagnostic program is unavailable"))?,
    };
    let target_text = target
        .to_str()
        .ok_or(SandboxError("requested diagnostic program path is invalid"))?;
    if !target_text.starts_with("/nix/store/") || !target.is_file() {
        return Err(SandboxError(
            "diagnostic executable is outside the Nix store",
        ));
    }
    Ok(target)
}

fn build_invocation(
    systemd_run: &Path,
    bwrap: &Path,
    executable: &str,
    args: &[String],
    closure: &[String],
) -> Result<Invocation, SandboxError> {
    if closure.is_empty() || closure.len() > 4096 {
        return Err(SandboxError("diagnostic executable closure is invalid"));
    }
    let mut bubblewrap_args = vec![
        "--die-with-parent".to_owned(),
        "--new-session".to_owned(),
        "--unshare-user".to_owned(),
        "--unshare-ipc".to_owned(),
        "--unshare-pid".to_owned(),
        "--unshare-net".to_owned(),
        "--unshare-uts".to_owned(),
        "--disable-userns".to_owned(),
        "--cap-drop".to_owned(),
        "ALL".to_owned(),
        "--clearenv".to_owned(),
        "--dir".to_owned(),
        "/nix".to_owned(),
        "--dir".to_owned(),
        "/nix/store".to_owned(),
    ];
    for path in closure {
        let name = path
            .strip_prefix("/nix/store/")
            .ok_or(SandboxError("Nix returned an invalid diagnostic closure"))?;
        if name.contains('/') || validate_store_entry(name).is_err() {
            return Err(SandboxError("Nix returned an invalid diagnostic closure"));
        }
        bubblewrap_args.extend(["--ro-bind".into(), path.clone(), path.clone()]);
    }
    let search_path = closure
        .iter()
        .flat_map(|path| [format!("{path}/bin"), format!("{path}/sbin")])
        .filter(|path| Path::new(path).is_dir())
        .collect::<Vec<_>>()
        .join(":");
    bubblewrap_args.extend([
        "--tmpfs".into(),
        "/sys".into(),
        "--ro-bind".into(),
        "/sys/devices".into(),
        "/sys/devices".into(),
        "--ro-bind".into(),
        "/sys/class".into(),
        "/sys/class".into(),
        "--ro-bind".into(),
        "/sys/bus".into(),
        "/sys/bus".into(),
        "--ro-bind".into(),
        "/sys/block".into(),
        "/sys/block".into(),
        "--ro-bind".into(),
        "/sys/dev".into(),
        "/sys/dev".into(),
        "--ro-bind".into(),
        "/sys/module".into(),
        "/sys/module".into(),
        "--proc".into(),
        "/proc".into(),
        "--dev".into(),
        "/dev".into(),
        "--tmpfs".into(),
        "/tmp".into(),
        "--chdir".into(),
        "/tmp".into(),
        "--setenv".into(),
        "PATH".into(),
        search_path,
        "--setenv".into(),
        "HOME".into(),
        "/tmp".into(),
        "--setenv".into(),
        "TMPDIR".into(),
        "/tmp".into(),
        "--".into(),
        executable.into(),
    ]);
    bubblewrap_args.extend(args.iter().cloned());

    let mut invocation = Invocation::new(systemd_run.to_string_lossy().into_owned()).args([
        "--user",
        "--scope",
        "--quiet",
        "--collect",
        "--property=MemoryMax=512M",
        "--property=TasksMax=64",
        "--property=CPUQuota=100%",
        "--",
    ]);
    invocation = invocation
        .arg(bwrap.to_string_lossy())
        .args(bubblewrap_args)
        .timeout(TIME_LIMIT)
        .output_limit(MAX_OUTPUT_BYTES);
    Ok(invocation)
}

use std::os::unix::fs::PermissionsExt;

#[cfg(test)]
mod tests {
    use super::{
        MAX_ARGUMENT_BYTES, MAX_ARGUMENTS, build_invocation, store_entry, validate_request,
    };
    use std::path::Path;

    #[test]
    fn validates_program_and_bounds_arguments() {
        assert!(validate_request("journalctl", &["--boot".into()]).is_ok());
        assert!(validate_request("bash -c id", &[]).is_err());
        assert!(validate_request("journalctl", &["x\0y".into()]).is_err());
        assert!(validate_request("journalctl", &["x".repeat(MAX_ARGUMENT_BYTES + 1)]).is_err());
        assert!(validate_request("journalctl", &vec!["x".into(); MAX_ARGUMENTS + 1]).is_err());
    }

    #[test]
    fn store_entry_restricts_executables_to_nix_store() {
        assert_eq!(
            store_entry("/nix/store/hash-tool/bin/tool").unwrap(),
            "/nix/store/hash-tool"
        );
        assert!(store_entry("/usr/bin/tool").is_err());
        assert!(store_entry("/nix/store/../etc/passwd").is_err());
    }

    #[test]
    fn invocation_has_hard_namespace_and_mount_boundaries() {
        let invocation = build_invocation(
            Path::new("/nix/store/systemd-run/bin/systemd-run"),
            Path::new("/nix/store/bubblewrap/bin/bwrap"),
            "/nix/store/tool/bin/tool",
            &["--version".into()],
            &["/nix/store/tool".into()],
        )
        .unwrap();
        let args = invocation.arguments();
        for required in [
            "--unshare-user",
            "--unshare-net",
            "--unshare-pid",
            "--disable-userns",
            "--cap-drop",
            "--tmpfs",
            "--ro-bind",
            "MemoryMax=512M",
            "TasksMax=64",
        ] {
            assert!(
                args.iter()
                    .any(|arg| arg == required || arg.ends_with(required)),
                "missing {required}"
            );
        }
        for forbidden in ["/home", "/etc", "/run", "/nix/var/nix/daemon-socket"] {
            assert!(!args.iter().any(|arg| arg == forbidden));
        }
    }

    #[test]
    fn sandbox_host_smoke_when_explicitly_enabled() {
        if std::env::var_os("RELAY_RUN_SANDBOX_SMOKE").is_none() {
            return;
        }
        let sandbox = super::SandboxRunner::default();
        let result = sandbox
            .run(
                "bash",
                &["-c".into(), "test ! -e /etc/passwd && test ! -e /home && test ! -e /run && test -e /sys/devices/system/cpu/online && test ! -e /sys/firmware/efi/efivars && echo okay >/tmp/relay-observe && test -f /tmp/relay-observe && ! echo x >/etc/relay-observe && ! echo 0 >/sys/devices/system/cpu/online && ! echo x >/dev/tcp/1.1.1.1/80 && echo sandbox-ok".into()],
            )
            .expect("configured host sandbox should run");
        assert_eq!(result.code, Some(0), "{}", result.stderr);
        assert!(result.stdout.contains("sandbox-ok"));

        let nested_userns = sandbox
            .run(
                "unshare",
                &["--user".into(), "--map-root-user".into(), "true".into()],
            )
            .expect("unshare diagnostic should run inside the sandbox");
        assert_ne!(
            nested_userns.code,
            Some(0),
            "nested user namespaces must be disabled"
        );
    }
}
