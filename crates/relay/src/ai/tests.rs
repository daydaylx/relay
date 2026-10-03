use std::sync::{Arc, Mutex};

use super::{
    Api, AskError, CommandProvider, HttpProvider, Prompt, Provider, build_explain_prompt,
    build_prompt, clip, collect_hints, curl_quote, extract_json_object, propose, validate_request,
    verify_changes,
};
use crate::change::{Change, Value};
use crate::exec::{Invocation, Outcome, ProcessRunner, Runner};
use crate::fsutil::testutil::TempDir;
use crate::index::SearchIndex;
use crate::intent::{Action, Proposal};

const OPTIONS: &str = r#"{"schemaVersion":1,"kind":"option","host":"h","nixpkgsRevision":"r",
"lockfileHash":"l","configIdentity":"c","targetSystem":"x86_64-linux","entries":[
{"name":"hardware.bluetooth.enable","type":"boolean","default":null,"description":"Whether to enable support for Bluetooth.","example":null,"declarations":[],"readOnly":false,"relatedPackages":[]},
{"name":"hardware.bluetooth.powerOnBoot","type":"boolean","default":null,"description":"Power on boot.","example":null,"declarations":[],"readOnly":false,"relatedPackages":[]},
{"name":"services.example.port","type":"signed integer","default":null,"description":"Port.","example":null,"declarations":[],"readOnly":false,"relatedPackages":[]},
{"name":"services.example.message","type":"string","default":null,"description":"Message.","example":null,"declarations":[],"readOnly":false,"relatedPackages":[]},
{"name":"services.example.items","type":"list of string","default":null,"description":"Items.","example":null,"declarations":[],"readOnly":false,"relatedPackages":[]},
{"name":"services.example.frozen","type":"boolean","default":null,"description":"Read only.","example":null,"declarations":[],"readOnly":true,"relatedPackages":[]},
{"name":"system.stateVersion","type":"string","default":null,"description":"State version.","example":null,"declarations":[],"readOnly":false,"relatedPackages":[]}]}"#;

const PACKAGES: &str = r#"{"schemaVersion":1,"kind":"package","host":"h","nixpkgsRevision":"r",
"lockfileHash":"l","configIdentity":"c","targetSystem":"x86_64-linux","entries":[
{"name":"vlc","description":"Cross-platform media player"},
{"name":"vlc-bin","description":"VLC binary"},
{"name":"htop","description":"Interactive process viewer"}]}"#;

fn options() -> SearchIndex {
    SearchIndex::from_json(OPTIONS, "option").unwrap()
}

fn packages() -> SearchIndex {
    SearchIndex::from_json(PACKAGES, "package").unwrap()
}

/// Answers every prompt with a canned result and remembers what it was asked.
struct Canned {
    answer: Result<String, String>,
    seen: Mutex<Vec<Prompt>>,
}

impl Canned {
    fn new(answer: Result<&str, &str>) -> Self {
        Self {
            answer: answer.map(str::to_owned).map_err(str::to_owned),
            seen: Mutex::new(Vec::new()),
        }
    }
}

impl Provider for Canned {
    fn label(&self) -> String {
        "canned".into()
    }

    fn complete(&self, prompt: &Prompt) -> Result<String, String> {
        self.seen.lock().unwrap().push(prompt.clone());
        self.answer.clone()
    }
}

fn ask(answer: &str, request: &str) -> Result<Proposal, AskError> {
    propose(
        &Canned::new(Ok(answer)),
        request,
        "host",
        Some(&options()),
        Some(&packages()),
    )
    .map(|resolved| resolved.proposal)
}

fn rejected(answer: &str) -> String {
    match ask(answer, "do something") {
        Err(AskError::Rejected(reason)) => reason,
        other => panic!("expected a rejection, got {other:?}"),
    }
}

#[test]
fn requests_are_validated_before_anything_is_sent() {
    assert_eq!(
        validate_request("  enable bluetooth \n").unwrap(),
        "enable bluetooth"
    );
    assert!(validate_request("   ").is_err());
    assert!(validate_request(&"x".repeat(2001)).is_err());
    assert!(validate_request("a\u{1b}[31mb").is_err());
    assert!(validate_request("zeile eins\nzeile zwei").is_ok());
    let provider = Canned::new(Ok("{}"));
    assert!(propose(&provider, "", "host", None, None).is_err());
    assert!(provider.seen.lock().unwrap().is_empty());
}

#[test]
fn hints_come_from_the_index_and_ignore_filler_words() {
    let hints = collect_hints(
        "Aktiviere bitte Bluetooth und installiere vlc für mein system",
        Some(&options()),
        Some(&packages()),
    );
    let names = hints
        .options
        .iter()
        .map(|e| e.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names[0], "hardware.bluetooth.enable",
        "enable-options rank first: {names:?}"
    );
    assert!(names.contains(&"hardware.bluetooth.powerOnBoot"));
    assert!(
        !names.contains(&"system.stateVersion"),
        "\"system\" is a filler word"
    );
    let packages = hints
        .packages
        .iter()
        .map(|e| e.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(packages[0], "vlc", "exact matches rank first: {packages:?}");
    assert!(collect_hints("bluetooth", None, None).options.is_empty());
}

#[test]
fn the_prompt_states_the_schema_and_rules_and_carries_no_secrets_or_paths() {
    let hints = collect_hints("enable bluetooth", Some(&options()), Some(&packages()));
    let prompt = build_prompt("enable bluetooth", "myhost", &hints);
    assert!(prompt.system.contains("\"schema\":1"));
    assert!(prompt.system.contains("unsupported"));
    assert!(prompt.system.contains("system.stateVersion"));
    assert!(prompt.system.contains("Host: myhost"));
    assert!(prompt.system.contains(
        "- hardware.bluetooth.enable | boolean | Whether to enable support for Bluetooth."
    ));
    assert!(prompt.user.contains("enable bluetooth"));
    for forbidden in ["/home", "/nix/store", "/etc/", "password", "token", "sk-"] {
        let combined = format!("{}{}", prompt.system, prompt.user);
        assert!(
            !combined.contains(forbidden) || forbidden == "password" || forbidden == "token",
            "prompt mentions {forbidden}"
        );
    }
    assert_eq!(
        prompt,
        build_prompt("enable bluetooth", "myhost", &hints),
        "deterministic"
    );
    let json = prompt.to_json();
    assert!(json.starts_with("{\"system\":\"") && json.contains("\"user\":\"Request:\\n<<<"));
    assert_eq!(clip("a\nb\tc   d", 100), "a b c d");
    assert_eq!(clip("abcdef", 3), "abc…");
    assert!(
        build_explain_prompt("Risk: LIVE_SWITCHABLE")
            .user
            .contains("LIVE_SWITCHABLE")
    );
}

#[test]
fn only_a_bare_or_fenced_single_json_object_is_extracted() {
    assert_eq!(extract_json_object(" {\"a\":1}\n").unwrap(), "{\"a\":1}");
    assert_eq!(
        extract_json_object("```json\n{\"a\":1}\n```").unwrap(),
        "{\"a\":1}"
    );
    assert_eq!(
        extract_json_object("```\n{\"a\":1}\n```").unwrap(),
        "{\"a\":1}"
    );
    for bad in [
        "Sure! Here is the JSON: {\"a\":1}",
        "{\"a\":1} Hope this helps!",
        "```json\n{\"a\":1}",
        "[{\"a\":1}]",
        "",
        "I cannot do that.",
    ] {
        assert!(extract_json_object(bad).is_err(), "accepted {bad:?}");
    }
}

#[test]
fn valid_answers_become_proposals() {
    let proposal = ask(
        r#"{"schema":1,"changes":[{"op":"set_option","option":"hardware.bluetooth.enable","value":true}]}"#,
        "Aktiviere Bluetooth",
    )
    .unwrap();
    assert_eq!(
        proposal,
        Proposal::Changes(vec![Change::SetOption {
            name: "hardware.bluetooth.enable".into(),
            value: Value::Bool(true)
        }])
    );
    assert!(matches!(
        ask("```json\n{\"schema\":1,\"changes\":[{\"op\":\"add_package\",\"package\":\"vlc\"}]}\n```", "vlc").unwrap(),
        Proposal::Changes(_)
    ));
    assert_eq!(
        ask(r#"{"schema":1,"action":"undo"}"#, "mach das rückgängig").unwrap(),
        Proposal::Action(Action::Undo)
    );
    assert_eq!(
        ask(
            r#"{"schema":1,"action":"unsupported","reason":"I cannot change disks."}"#,
            "format my disk"
        )
        .unwrap(),
        Proposal::Unsupported("I cannot change disks.".into())
    );
}

#[test]
fn malformed_or_dangerous_model_output_is_rejected_without_any_side_effect() {
    // Not JSON / not the schema.
    assert!(rejected("Let me think about that.").contains("bare JSON"));
    assert!(
        rejected(r#"{"schema":1,"changes":[{"op":"run_shell","cmd":"id"}]}"#)
            .contains("unknown op")
    );
    assert!(
        rejected(r#"{"schema":1,"changes":[{"op":"add_package","package":"vlc","raw":"x"}]}"#)
            .contains("unknown field")
    );
    assert!(rejected(r#"{"schema":2,"action":"undo"}"#).contains("schema"));
    // Policy applies even though the option exists in the index.
    let error = rejected(
        r#"{"schema":1,"changes":[{"op":"set_option","option":"system.stateVersion","value":"99.11"}]}"#,
    );
    assert!(error.contains("protected"), "{error}");
    // Nix-shaped names never reach the renderer.
    assert!(
        rejected(
            r#"{"schema":1,"changes":[{"op":"add_package","package":"vlc; builtins.abort"}]}"#
        )
        .contains("invalid package")
    );
}

#[test]
fn invented_names_and_impossible_values_are_caught_against_the_local_index() {
    let unknown_option = rejected(
        r#"{"schema":1,"changes":[{"op":"set_option","option":"hardware.bluetoth.enable","value":true}]}"#,
    );
    assert!(
        unknown_option.contains("not in the local options index"),
        "{unknown_option}"
    );
    let unknown_package =
        rejected(r#"{"schema":1,"changes":[{"op":"add_package","package":"vlcc"}]}"#);
    assert!(
        unknown_package.contains("not in the local package index"),
        "{unknown_package}"
    );
    let read_only = rejected(
        r#"{"schema":1,"changes":[{"op":"set_option","option":"services.example.frozen","value":true}]}"#,
    );
    assert!(read_only.contains("read-only"));
    let mismatch = rejected(
        r#"{"schema":1,"changes":[{"op":"set_option","option":"hardware.bluetooth.enable","value":"yes"}]}"#,
    );
    assert!(mismatch.contains("does not fit its type"), "{mismatch}");
    for (option, value) in [
        ("services.example.port", "8080"),
        ("services.example.message", "\"hi\""),
        ("services.example.items", "[\"a\"]"),
        ("hardware.bluetooth.enable", "false"),
    ] {
        let answer = format!(
            r#"{{"schema":1,"changes":[{{"op":"set_option","option":"{option}","value":{value}}}]}}"#
        );
        assert!(ask(&answer, "x").is_ok(), "{option} = {value}");
    }
    // Without an index the cross-check is skipped; the candidate evaluation is the authority.
    let changes = [Change::SetOption {
        name: "anything.goes".into(),
        value: Value::Bool(true),
    }];
    assert!(verify_changes(&changes, None, None).is_ok());
    // Removing a package is not index-checked (the managed baseline decides).
    assert!(
        verify_changes(
            &[Change::RemovePackage {
                name: "gone".into()
            }],
            None,
            Some(&packages())
        )
        .is_ok()
    );
}

#[test]
fn an_unavailable_provider_or_oversized_answer_proposes_nothing() {
    let error = propose(
        &Canned::new(Err("connection refused")),
        "enable bluetooth",
        "h",
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(error, AskError::Unavailable("connection refused".into()));
    assert!(error.to_string().contains("unavailable"));
    let huge = format!(
        "{{\"schema\":1,\"action\":\"unsupported\",\"reason\":\"{}\"}}",
        "x".repeat(300 * 1024)
    );
    assert!(matches!(
        propose(&Canned::new(Ok(&huge)), "x", "h", None, None),
        Err(AskError::Rejected(reason)) if reason.contains("too large")
    ));
}

#[test]
fn a_command_provider_receives_the_prompt_on_stdin_and_returns_stdout() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new("ai");
    let script = dir.path().join("provider.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\ninput=$(cat)\ncase \"$input\" in *'\"system\"'*'\"user\"'*) ;; *) exit 3 ;; esac\ncase \"$input\" in *'enable bluetooth'*) ;; *) exit 4 ;; esac\nprintf '%s' '{\"schema\":1,\"action\":\"status\"}'\n",
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let runner: Arc<dyn Runner> = Arc::new(ProcessRunner::default());
    let provider = CommandProvider::new(runner.clone(), script.to_string_lossy(), Vec::new());
    assert!(provider.label().starts_with("command "));
    let resolved = propose(&provider, "enable bluetooth", "h", None, None).unwrap();
    assert_eq!(resolved.proposal, Proposal::Action(Action::Status));
    assert!(resolved.provider.contains("provider.sh"));

    let failing = CommandProvider::new(runner.clone(), "false", Vec::new());
    assert!(matches!(
        propose(&failing, "x", "h", None, None),
        Err(AskError::Unavailable(reason)) if reason.contains("status 1")
    ));
    let missing = CommandProvider::new(runner, "/nonexistent/relay-provider", Vec::new());
    assert!(matches!(
        propose(&missing, "x", "h", None, None),
        Err(AskError::Unavailable(_))
    ));
}

// ------------------------------------------------------------------- HTTP providers

/// Records the curl invocation and answers with a canned outcome.
struct FakeCurl {
    outcome: Outcome,
    seen: Mutex<Vec<Invocation>>,
}

impl FakeCurl {
    fn replying(code: i32, stdout: &str) -> Arc<Self> {
        Arc::new(Self {
            outcome: Outcome {
                code: Some(code),
                stdout: stdout.as_bytes().to_vec(),
                stderr: b"secret body".to_vec(),
            },
            seen: Mutex::new(Vec::new()),
        })
    }
}

impl Runner for FakeCurl {
    fn run(&self, invocation: &Invocation) -> Result<Outcome, String> {
        self.seen.lock().unwrap().push(invocation.clone());
        Ok(self.outcome.clone())
    }
}

const PROMPT: fn() -> Prompt = || Prompt {
    system: "rules \"quoted\"\nline2".into(),
    user: "request".into(),
};

#[test]
fn the_openai_compatible_provider_keeps_the_key_out_of_arguments_and_parses_the_answer() {
    let curl = FakeCurl::replying(
        0,
        r#"{"choices":[{"message":{"role":"assistant","content":"{\"schema\":1,\"action\":\"history\"}"}}]}"#,
    );
    let provider = HttpProvider::new(
        curl.clone(),
        Api::OpenAiCompatible,
        None,
        "gpt-4o-mini",
        Some("sk-test_KEY.123".into()),
    )
    .unwrap();
    assert_eq!(
        provider.label(),
        "openai-compatible https://api.openai.com/v1 (gpt-4o-mini)"
    );
    let answer = provider.complete(&PROMPT()).unwrap();
    assert_eq!(answer, "{\"schema\":1,\"action\":\"history\"}");

    let seen = curl.seen.lock().unwrap();
    let call = &seen[0];
    assert_eq!(call.program(), "curl");
    assert_eq!(call.arguments()[0], "-q", "never read ~/.curlrc");
    assert!(call.arguments().iter().any(|a| a == "--fail-with-body"));
    assert!(
        call.arguments()
            .windows(2)
            .any(|w| w == ["--proto", "=https"])
    );
    assert!(
        call.arguments()
            .iter()
            .all(|arg| !arg.contains("sk-test") && !arg.contains("rules"))
    );
    assert!(call.environment().is_empty());
    let config = String::from_utf8(call.input().unwrap().to_vec()).unwrap();
    assert!(config.contains("url = \"https://api.openai.com/v1/chat/completions\""));
    assert!(config.contains("header = \"Authorization: Bearer sk-test_KEY.123\""));
    assert!(config.contains("\\\"model\\\":\\\"gpt-4o-mini\\\""));
    assert!(
        config.contains("rules \\\\\\\"quoted\\\\\\\"\\\\nline2"),
        "body is JSON-escaped, then curl-quoted: {config}"
    );
    assert!(
        !format!("{call:?}").contains("sk-test"),
        "Debug output never shows the stdin payload"
    );
    assert!(!format!("{provider:?}").contains("sk-test"));
}

#[test]
fn the_anthropic_provider_uses_its_own_headers_and_response_shape() {
    let curl = FakeCurl::replying(
        0,
        r#"{"content":[{"type":"thinking","text":"x"},{"type":"text","text":"{\"schema\":1,\"action\":\"undo\"}"}]}"#,
    );
    let provider = HttpProvider::new(
        curl.clone(),
        Api::Anthropic,
        None,
        "claude-sonnet-5-5",
        Some("key123".into()),
    )
    .unwrap();
    assert_eq!(
        provider.complete(&PROMPT()).unwrap(),
        "{\"schema\":1,\"action\":\"undo\"}"
    );
    let seen = curl.seen.lock().unwrap();
    let config = String::from_utf8(seen[0].input().unwrap().to_vec()).unwrap();
    assert!(config.contains("url = \"https://api.anthropic.com/v1/messages\""));
    assert!(config.contains("header = \"x-api-key: key123\""));
    assert!(config.contains("header = \"anthropic-version: 2023-06-01\""));
    assert!(config.contains("\\\"max_tokens\\\":1024"));
    assert!(!config.contains("Authorization"));
    assert!(seen[0].arguments().iter().all(|a| !a.contains("key123")));
}

#[test]
fn local_servers_may_use_plain_http_but_nothing_else_may() {
    let curl = FakeCurl::replying(0, r#"{"choices":[{"message":{"content":"{}"}}]}"#);
    let local = HttpProvider::new(
        curl.clone(),
        Api::OpenAiCompatible,
        Some("http://localhost:11434/v1/"),
        "llama3.1:8b",
        None,
    )
    .unwrap();
    local.complete(&PROMPT()).unwrap();
    let seen = curl.seen.lock().unwrap();
    assert!(
        seen[0]
            .arguments()
            .windows(2)
            .any(|w| w == ["--proto", "=http"])
    );
    let config = String::from_utf8(seen[0].input().unwrap().to_vec()).unwrap();
    assert!(config.contains("url = \"http://localhost:11434/v1/chat/completions\""));
    assert!(!config.contains("Authorization"), "no key, no auth header");
    drop(seen);

    let runner: Arc<dyn Runner> = curl;
    for (url, why) in [
        ("http://example.com/v1", "plain http to a remote host"),
        ("ftp://example.com", "wrong scheme"),
        ("https://user:pw@example.com/v1", "credentials in the URL"),
        ("https://example.com/v1\"; data = \"x", "config injection"),
        ("https://", "no host"),
    ] {
        assert!(
            HttpProvider::new(runner.clone(), Api::OpenAiCompatible, Some(url), "m", None).is_err(),
            "{why}"
        );
    }
    assert!(
        HttpProvider::new(
            runner.clone(),
            Api::OpenAiCompatible,
            Some("http://127.0.0.1:8080"),
            "m",
            None
        )
        .is_ok()
    );
    assert!(
        HttpProvider::new(
            runner.clone(),
            Api::OpenAiCompatible,
            Some("http://[::1]:8080"),
            "m",
            None
        )
        .is_ok()
    );
}

#[test]
fn bad_keys_models_and_missing_anthropic_keys_are_refused_up_front() {
    let runner: Arc<dyn Runner> = FakeCurl::replying(0, "{}");
    for key in ["", "a b", "key\nheader = \"x: y\"", "k\"ey"] {
        assert!(
            HttpProvider::new(
                runner.clone(),
                Api::OpenAiCompatible,
                None,
                "m",
                Some(key.into())
            )
            .is_err(),
            "{key:?}"
        );
    }
    assert!(
        HttpProvider::new(
            runner.clone(),
            Api::OpenAiCompatible,
            None,
            "bad model",
            None
        )
        .is_err()
    );
    assert!(HttpProvider::new(runner.clone(), Api::OpenAiCompatible, None, "", None).is_err());
    assert!(HttpProvider::new(runner, Api::Anthropic, None, "m", None).is_err());
}

#[test]
fn http_failures_are_reported_without_leaking_the_response_body() {
    for (code, expected) in [
        (22, "HTTP error"),
        (7, "could not be reached"),
        (28, "could not be reached"),
        (2, "exit status 2"),
    ] {
        let curl = FakeCurl::replying(code, "secret response body");
        let provider =
            HttpProvider::new(curl, Api::OpenAiCompatible, None, "m", Some("k".into())).unwrap();
        let error = provider.complete(&PROMPT()).unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert!(!error.contains("secret"));
    }
    for body in [
        "not json",
        "{}",
        r#"{"choices":[]}"#,
        r#"{"choices":[{"message":{"content":7}}]}"#,
    ] {
        let curl = FakeCurl::replying(0, body);
        let provider = HttpProvider::new(curl, Api::OpenAiCompatible, None, "m", None).unwrap();
        assert!(provider.complete(&PROMPT()).is_err(), "{body}");
    }
}

#[test]
fn curl_config_values_are_quoted() {
    assert_eq!(curl_quote("a\"b\\c\nd\te"), "\"a\\\"b\\\\c\\nd\\te\"");
}
