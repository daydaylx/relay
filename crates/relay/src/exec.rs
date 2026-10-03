//! Process execution seam. Every external command Relay runs goes through a [`Runner`], which
//! lets the workflow be tested against a simulated system and keeps privilege escalation behind
//! typed constructors instead of an arbitrary "run as root" API.

use std::fs;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Environment variables that could weaken NixOS' own pre-switch safety checks. They are removed
/// from every child process so a stray shell export can never bypass switch inhibitors.
const SCRUBBED_ENVIRONMENT: &[&str] = &["NIXOS_NO_CHECK"];

/// Bytes written to a child's stdin. May carry credentials, so it never appears in `Debug`.
#[derive(Clone, PartialEq, Eq)]
struct Input(Vec<u8>);

impl std::fmt::Debug for Input {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "<{} bytes>", self.0.len())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invocation {
    program: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
    privileged: bool,
    stdin: Option<Input>,
    timeout: Option<Duration>,
}

impl Invocation {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
            privileged: false,
            stdin: None,
            timeout: None,
        }
    }

    /// Only the NixOS adapter may construct privileged invocations, one typed action at a time.
    pub(crate) fn privileged(program: impl Into<String>) -> Self {
        Self {
            privileged: true,
            ..Self::new(program)
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// Feed `bytes` to the child's standard input (used for prompts and credentials, which must
    /// not appear in the process list).
    pub fn stdin(mut self, bytes: impl Into<Vec<u8>>) -> Self {
        self.stdin = Some(Input(bytes.into()));
        self
    }

    /// Kill the child and report an error if it runs longer than `limit`.
    pub fn timeout(mut self, limit: Duration) -> Self {
        self.timeout = Some(limit);
        self
    }

    pub fn program(&self) -> &str {
        &self.program
    }

    pub fn arguments(&self) -> &[String] {
        &self.args
    }

    pub fn environment(&self) -> &[(String, String)] {
        &self.env
    }

    pub fn input(&self) -> Option<&[u8]> {
        self.stdin.as_ref().map(|input| input.0.as_slice())
    }

    pub fn is_privileged(&self) -> bool {
        self.privileged
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl Outcome {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }

    pub fn stdout_text(&self) -> Result<String, String> {
        String::from_utf8(self.stdout.clone())
            .map(|text| text.trim().to_owned())
            .map_err(|_| "command returned non-UTF-8 output".to_owned())
    }
}

pub trait Runner: Send + Sync {
    /// Run to completion. `Err` means the process could not be started at all; a non-zero exit
    /// status is reported through [`Outcome::code`].
    fn run(&self, invocation: &Invocation) -> Result<Outcome, String>;
}

/// Runs real processes. Privileged invocations are prefixed with a privilege-escalation command
/// (`sudo` by default) unless Relay already runs as root; nothing else is ever escalated.
#[derive(Clone, Debug)]
pub struct ProcessRunner {
    escalation: String,
}

impl Default for ProcessRunner {
    fn default() -> Self {
        Self {
            escalation: "sudo".to_owned(),
        }
    }
}

impl ProcessRunner {
    pub fn with_escalation(escalation: impl Into<String>) -> Self {
        Self {
            escalation: escalation.into(),
        }
    }
}

impl Runner for ProcessRunner {
    fn run(&self, invocation: &Invocation) -> Result<Outcome, String> {
        if let Some((key, _)) = invocation
            .env
            .iter()
            .find(|(key, _)| SCRUBBED_ENVIRONMENT.contains(&key.as_str()))
        {
            return Err(format!("refusing to pass {key} to a child process"));
        }
        let mut command = if invocation.privileged && !running_as_root() {
            let mut command = Command::new(&self.escalation);
            command.arg("--").arg(&invocation.program);
            command
        } else {
            Command::new(&invocation.program)
        };
        command.args(&invocation.args);
        for key in SCRUBBED_ENVIRONMENT {
            command.env_remove(key);
        }
        for (key, value) in &invocation.env {
            command.env(key, value);
        }
        let start_error = |error: std::io::Error| {
            let program = if invocation.privileged {
                &self.escalation
            } else {
                &invocation.program
            };
            format!("could not start {program}: {error}")
        };
        if invocation.stdin.is_none() && invocation.timeout.is_none() {
            let output = command.output().map_err(start_error)?;
            return Ok(Outcome {
                code: output.status.code(),
                stdout: output.stdout,
                stderr: output.stderr,
            });
        }
        run_supervised(command, invocation, start_error)
    }
}

/// Spawn with piped stdio so input can be fed and a deadline enforced.
fn run_supervised(
    mut command: Command,
    invocation: &Invocation,
    start_error: impl Fn(std::io::Error) -> String,
) -> Result<Outcome, String> {
    command
        .stdin(if invocation.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(start_error)?;
    let writer = child.stdin.take().map(|mut pipe| {
        let data = invocation.stdin.clone().unwrap_or(Input(Vec::new())).0;
        // Dropping the pipe at the end of the thread closes the child's stdin.
        std::thread::spawn(move || {
            let _ = pipe.write_all(&data);
        })
    });
    let reader = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut buffer = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut buffer);
            }
            buffer
        })
    };
    let stdout = reader(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let stderr = reader(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let deadline = invocation.timeout.map(|limit| Instant::now() + limit);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                return Err(format!(
                    "could not wait for {}: {error}",
                    invocation.program
                ));
            }
        }
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            let _ = child.kill();
            let _ = child.wait();
            // Grandchildren may keep the pipes open; do not wait for the reader threads.
            return Err(format!(
                "{} did not finish within {} seconds",
                invocation.program,
                invocation.timeout.unwrap_or_default().as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    if let Some(writer) = writer {
        let _ = writer.join();
    }
    Ok(Outcome {
        code: status.code(),
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

/// Effective-UID check via `/proc` (the crate forbids `unsafe`, so no libc call).
pub fn running_as_root() -> bool {
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find_map(|line| line.strip_prefix("Uid:"))
                .and_then(|ids| ids.split_whitespace().nth(1).map(|uid| uid == "0"))
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{Invocation, ProcessRunner, Runner, running_as_root};

    #[test]
    fn unprivileged_commands_run_directly_and_capture_output() {
        let outcome = ProcessRunner::default()
            .run(&Invocation::new("sh").args(["-c", "printf out; printf err >&2; exit 3"]))
            .unwrap();
        assert_eq!(outcome.code, Some(3));
        assert_eq!(outcome.stdout, b"out");
        assert_eq!(outcome.stderr, b"err");
    }

    #[test]
    fn missing_programs_are_start_errors_not_exit_codes() {
        let error = ProcessRunner::default()
            .run(&Invocation::new("/nonexistent/relay-test-binary"))
            .unwrap_err();
        assert!(error.contains("could not start"));
    }

    #[test]
    fn stdin_is_delivered_without_appearing_in_the_process_arguments_or_debug_output() {
        let invocation = Invocation::new("sh")
            .args(["-c", "cat; printf done >&2"])
            .stdin("secret-token-123".as_bytes());
        assert!(!format!("{invocation:?}").contains("secret-token"));
        assert!(
            invocation
                .arguments()
                .iter()
                .all(|arg| !arg.contains("secret"))
        );
        let outcome = ProcessRunner::default().run(&invocation).unwrap();
        assert_eq!(outcome.stdout, b"secret-token-123");
        assert_eq!(outcome.stderr, b"done");
    }

    #[test]
    fn large_input_and_output_do_not_deadlock() {
        let data = vec![b'x'; 1 << 20];
        let outcome = ProcessRunner::default()
            .run(
                &Invocation::new("cat")
                    .stdin(data.clone())
                    .timeout(std::time::Duration::from_secs(20)),
            )
            .unwrap();
        assert_eq!(outcome.stdout.len(), data.len());
    }

    #[test]
    fn a_child_that_exceeds_its_time_limit_is_killed() {
        let started = std::time::Instant::now();
        let error = ProcessRunner::default()
            .run(
                &Invocation::new("sleep")
                    .arg("30")
                    .timeout(std::time::Duration::from_millis(200)),
            )
            .unwrap_err();
        assert!(error.contains("did not finish"), "{error}");
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
    }

    #[test]
    fn switch_check_bypass_variables_can_never_be_passed_to_children() {
        let error = ProcessRunner::default()
            .run(&Invocation::new("true").env("NIXOS_NO_CHECK", "1"))
            .unwrap_err();
        assert!(error.contains("NIXOS_NO_CHECK"));
    }

    #[test]
    fn privileged_invocations_are_prefixed_with_the_escalation_command() {
        if running_as_root() {
            // Already root: the command is executed directly, without escalation.
            return;
        }
        let dir = crate::fsutil::testutil::TempDir::new("exec");
        let script = dir.path().join("fake-sudo");
        std::fs::write(&script, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let runner = ProcessRunner::with_escalation(script.to_string_lossy());
        let outcome = runner
            .run(
                &Invocation::privileged("/nix/store/x-system/bin/switch-to-configuration")
                    .arg("test"),
            )
            .unwrap();
        assert_eq!(
            outcome.stdout_text().unwrap(),
            "--\n/nix/store/x-system/bin/switch-to-configuration\ntest"
        );
    }
}
