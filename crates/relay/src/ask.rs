//! `relay ask`: the optional natural-language front end (target state T5).
//!
//! The model only *proposes* a typed intent. Everything that follows is the deterministic core
//! (validation, protected-resource policy, index cross-check, isolated candidate, evaluation,
//! build, review) and a person confirms before anything changes. `--yes` is deliberately not
//! accepted here: a model-originated action is never confirmed on the user's behalf.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use relay::{
    Action, Api, AskError, CommandProvider, HttpProvider, ProcessRunner, Proposal, Provider,
    Runner, SearchIndex, build_explain_prompt, build_prompt, clip, collect_hints, json_string,
    propose, validate_request,
};

use crate::{
    Parsed, apply_with, build_engine, history_with, parse, plan_with, recover_with, status,
    undo_with,
};

const NO_PROVIDER: &str = "no AI provider is configured (--provider command|openai|anthropic or \
RELAY_AI_PROVIDER). Relay works without one: use `relay plan` with a change or an --intent file";

const VALUE_FLAGS: &[&str] = &[
    "--flake",
    "--host",
    "--state-dir",
    "--root",
    "--observe",
    "--expect-active",
    "--options-index",
    "--packages-index",
    "--provider",
    "--model",
    "--base-url",
    "--provider-command",
    "--provider-arg",
];
const SWITCHES: &[&str] = &[
    "--show-prompt",
    "--explain",
    "--apply",
    "--preview",
    "--no-desktop-check",
];

pub fn ask(args: &[String]) -> Result<ExitCode, String> {
    if args.iter().any(|arg| arg == "--yes") {
        return Err("--yes is not accepted by `relay ask`: confirm interactively".into());
    }
    let parsed = parse(args, VALUE_FLAGS, SWITCHES)?;
    let request = validate_request(&parsed.positional.join(" "))?;
    let host = parsed.required("--host")?.to_owned();
    let options = load_index(&parsed, "--options-index", "option")?;
    let packages = load_index(&parsed, "--packages-index", "package")?;

    if parsed.has("--show-prompt") {
        let hints = collect_hints(&request, options.as_ref(), packages.as_ref());
        println!("{}", build_prompt(&request, &host, &hints).to_json());
        eprintln!("relay: nothing was sent; this is exactly what a provider would receive");
        return Ok(ExitCode::SUCCESS);
    }

    let runner: Arc<dyn Runner> = Arc::new(ProcessRunner::default());
    let provider = build_provider(&parsed, runner, &|key| {
        std::env::var(key).ok().filter(|value| !value.is_empty())
    })?;
    eprintln!(
        "relay: asking {} (sent: your request, the schema rules, the host name and a few option/package names)",
        provider.label()
    );
    let resolved = propose(
        provider.as_ref(),
        &request,
        &host,
        options.as_ref(),
        packages.as_ref(),
    )
    .map_err(|error: AskError| error.to_string())?;

    match resolved.proposal {
        Proposal::Unsupported(reason) => {
            eprintln!("relay: the model declined: \"{}\"", clip(&reason, 200));
            Ok(ExitCode::FAILURE)
        }
        Proposal::Action(Action::Status) => {
            status(&forward(&parsed, &["--root", "--flake", "--state-dir"]))?;
            Ok(ExitCode::SUCCESS)
        }
        Proposal::Action(Action::History) => {
            history_with(&build_engine(&parsed)?)?;
            Ok(ExitCode::SUCCESS)
        }
        Proposal::Action(Action::Undo) => {
            eprintln!("relay: the model proposes: undo the last Relay change");
            undo_with(&build_engine(&parsed)?, false)
        }
        Proposal::Action(Action::Recover) => {
            eprintln!("relay: the model proposes: recover an interrupted change");
            recover_with(&build_engine(&parsed)?, false)
        }
        Proposal::Changes(changes) => {
            let flake = PathBuf::from(parsed.required("--flake")?);
            let engine = build_engine(&parsed)?;
            let outcome = plan_with(&engine, &flake, &host, &changes, parsed.has("--preview"))?;
            let id = outcome.record.id.clone();
            let audit = format!(
                "{{\"provider\":{},\"request\":{},\"answer\":{}}}\n",
                json_string(&resolved.provider),
                json_string(&request),
                json_string(&resolved.raw)
            );
            engine.state().save_file(&id, "ai.json", audit.as_bytes())?;
            if parsed.has("--explain") {
                explain(provider.as_ref(), &engine.review(&id)?);
            }
            if !outcome.applicable {
                eprintln!(
                    "relay: planned only; this class of change is never applied automatically"
                );
                return Ok(ExitCode::SUCCESS);
            }
            if parsed.has("--apply") {
                return apply_with(&engine, &id, false, parsed.all("--expect-active"));
            }
            eprintln!(
                "relay: nothing was applied. Review with `relay show {id}`, then `relay apply {id}`"
            );
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// A plain-language explanation of the deterministic review. Display only, labelled as generated.
fn explain(provider: &dyn Provider, review: &str) {
    match provider.complete(&build_explain_prompt(review)) {
        Ok(text) => eprintln!(
            "\n--- explanation written by the AI provider (not authoritative) ---\n{}\n---",
            clip(&text, 1500)
        ),
        Err(error) => eprintln!("relay: no AI explanation available: {error}"),
    }
}

fn load_index(parsed: &Parsed, flag: &str, kind: &str) -> Result<Option<SearchIndex>, String> {
    parsed
        .one(flag)?
        .map(|path| SearchIndex::read(std::path::Path::new(path), kind))
        .transpose()
}

fn forward(parsed: &Parsed, flags: &[&str]) -> Vec<String> {
    flags
        .iter()
        .filter_map(|flag| {
            parsed
                .one(flag)
                .ok()
                .flatten()
                .map(|value| [(*flag).to_owned(), value.to_owned()])
        })
        .flatten()
        .collect()
}

/// Provider selection: flags first, then `RELAY_AI_*` environment variables. The API key is only
/// ever read from the environment (or a key file), never from an argument.
fn build_provider(
    parsed: &Parsed,
    runner: Arc<dyn Runner>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<Box<dyn Provider>, String> {
    let flag_or_env = |flag: &str, variable: &str| -> Result<Option<String>, String> {
        Ok(parsed
            .one(flag)?
            .map(str::to_owned)
            .or_else(|| env(variable)))
    };
    let kind =
        flag_or_env("--provider", "RELAY_AI_PROVIDER")?.ok_or_else(|| NO_PROVIDER.to_owned())?;
    match kind.as_str() {
        "command" => {
            let program =
                flag_or_env("--provider-command", "RELAY_AI_COMMAND")?.ok_or_else(|| {
                    "--provider command needs --provider-command (or RELAY_AI_COMMAND)".to_owned()
                })?;
            Ok(Box::new(CommandProvider::new(
                runner,
                program,
                parsed.all("--provider-arg").to_vec(),
            )))
        }
        "openai" | "anthropic" => {
            let model = flag_or_env("--model", "RELAY_AI_MODEL")?
                .ok_or_else(|| "this provider needs --model (or RELAY_AI_MODEL)".to_owned())?;
            let base_url = flag_or_env("--base-url", "RELAY_AI_BASE_URL")?;
            let api = if kind == "openai" {
                Api::OpenAiCompatible
            } else {
                Api::Anthropic
            };
            let key = match env("RELAY_AI_API_KEY") {
                Some(key) => Some(key),
                None => match env("RELAY_AI_API_KEY_FILE") {
                    Some(path) => Some(
                        std::fs::read_to_string(&path)
                            .map_err(|error| format!("could not read the API key file: {error}"))?
                            .trim()
                            .to_owned(),
                    ),
                    None => None,
                },
            };
            Ok(Box::new(HttpProvider::new(
                runner,
                api,
                base_url.as_deref(),
                &model,
                key,
            )?))
        }
        other => Err(format!(
            "unknown provider '{other}' (expected command, openai or anthropic)"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{SWITCHES, VALUE_FLAGS, ask, build_provider, forward};
    use crate::parse;
    use relay::{ProcessRunner, Runner};
    use std::collections::BTreeMap;
    use std::sync::Arc;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn provider_label(args: &[&str], env: &[(&str, &str)]) -> Result<String, String> {
        let parsed = parse(&strings(args), VALUE_FLAGS, SWITCHES).unwrap();
        let env = env
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect::<BTreeMap<_, _>>();
        let runner: Arc<dyn Runner> = Arc::new(ProcessRunner::default());
        build_provider(&parsed, runner, &|key| env.get(key).cloned()).map(|p| p.label())
    }

    #[test]
    fn without_a_provider_the_error_explains_that_relay_works_without_one() {
        let error = provider_label(&["bluetooth"], &[]).unwrap_err();
        assert!(error.contains("works without one"), "{error}");
    }

    #[test]
    fn flags_beat_environment_and_each_provider_kind_is_configured_separately() {
        assert_eq!(
            provider_label(
                &["--provider", "command", "--provider-command", "/bin/true"],
                &[("RELAY_AI_PROVIDER", "openai")]
            )
            .unwrap(),
            "command /bin/true"
        );
        assert_eq!(
            provider_label(
                &[],
                &[
                    ("RELAY_AI_PROVIDER", "command"),
                    ("RELAY_AI_COMMAND", "./ask.sh")
                ]
            )
            .unwrap(),
            "command ./ask.sh"
        );
        assert_eq!(
            provider_label(
                &["--provider", "openai", "--model", "gpt-4o-mini"],
                &[("RELAY_AI_API_KEY", "sk-test")]
            )
            .unwrap(),
            "openai-compatible https://api.openai.com/v1 (gpt-4o-mini)"
        );
        assert_eq!(
            provider_label(
                &[
                    "--provider",
                    "openai",
                    "--model",
                    "llama3",
                    "--base-url",
                    "http://localhost:11434/v1"
                ],
                &[]
            )
            .unwrap(),
            "openai-compatible http://localhost:11434/v1 (llama3)"
        );
        assert!(
            provider_label(&["--provider", "openai"], &[])
                .unwrap_err()
                .contains("--model")
        );
        assert!(
            provider_label(&["--provider", "command"], &[])
                .unwrap_err()
                .contains("--provider-command")
        );
        assert!(
            provider_label(&["--provider", "anthropic", "--model", "m"], &[])
                .unwrap_err()
                .contains("API key")
        );
        assert!(
            provider_label(&["--provider", "gemini"], &[])
                .unwrap_err()
                .contains("unknown provider")
        );
    }

    #[test]
    fn the_api_key_comes_from_a_file_when_requested_and_is_never_taken_from_arguments() {
        let dir = std::env::temp_dir().join(format!("relay-ask-key-{}", std::process::id()));
        std::fs::write(&dir, "sk-from-file\n").unwrap();
        let path = dir.to_string_lossy().into_owned();
        assert!(
            provider_label(
                &["--provider", "anthropic", "--model", "m"],
                &[("RELAY_AI_API_KEY_FILE", path.as_str())]
            )
            .unwrap()
            .starts_with("anthropic ")
        );
        let _ = std::fs::remove_file(&dir);
        assert!(parse(&strings(&["--api-key", "x"]), VALUE_FLAGS, SWITCHES).is_err());
    }

    #[test]
    fn a_model_originated_action_can_never_be_confirmed_with_yes() {
        let error = ask(&strings(&["undo", "--host", "h", "--yes"])).unwrap_err();
        assert!(error.contains("--yes is not accepted"));
    }

    #[test]
    fn flags_are_forwarded_to_the_read_only_commands() {
        let parsed = parse(
            &strings(&["x", "--flake", "/f", "--state-dir", "/s", "--host", "h"]),
            VALUE_FLAGS,
            SWITCHES,
        )
        .unwrap();
        assert_eq!(
            forward(&parsed, &["--root", "--flake", "--state-dir"]),
            ["--flake", "/f", "--state-dir", "/s"]
        );
    }
}
