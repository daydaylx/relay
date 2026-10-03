//! Optional natural-language layer (target state T5).
//!
//! The model is a *translator*, never an authority: it turns a request into one strict JSON
//! document (`intent.rs`), and everything after that is the deterministic core. A proposal is
//! validated against the schema and the protected-resource policy, cross-checked against the
//! local option/package index, evaluated and built in an isolated candidate, and applied only
//! after a person confirms. The model cannot reach the system, Nix, a shell or privileges; the
//! core works identically without any provider.
//!
//! Data that leaves the machine is exactly the [`Prompt`] (see `relay ask --show-prompt`): the
//! request text, the schema rules, the host name and a few index entries (names, types and
//! descriptions). No configuration values, file contents, paths or credentials are included.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use crate::change::{Change, Value};
use crate::exec::{Invocation, Runner};
use crate::index::{IndexEntry, SearchIndex};
use crate::intent::{Proposal, parse_proposal};
use crate::json::Json;
use crate::json_string;

pub const MAX_REQUEST_CHARS: usize = 2000;
const PROVIDER_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_RESPONSE_BYTES: usize = 256 * 1024;
const MAX_HINTS_PER_KIND: usize = 12;
const HINTS_PER_TOKEN: usize = 6;

/// Words that never identify an option or package.
const STOP_WORDS: &[&str] = &[
    "the",
    "and",
    "for",
    "with",
    "please",
    "enable",
    "disable",
    "install",
    "remove",
    "add",
    "set",
    "turn",
    "make",
    "can",
    "you",
    "my",
    "bitte",
    "aktiviere",
    "aktivieren",
    "deaktiviere",
    "installiere",
    "installieren",
    "entferne",
    "entfernen",
    "hinzufügen",
    "füge",
    "und",
    "mit",
    "für",
    "den",
    "die",
    "das",
    "dem",
    "ein",
    "eine",
    "mach",
    "mache",
    "schalte",
    "ein",
    "aus",
    "auf",
    "mein",
    "meinen",
    "system",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prompt {
    pub system: String,
    pub user: String,
}

impl Prompt {
    /// The exact payload a provider receives, for `--show-prompt`.
    pub fn to_json(&self) -> String {
        format!(
            "{{\"system\":{},\"user\":{}}}",
            json_string(&self.system),
            json_string(&self.user)
        )
    }
}

/// Something that can complete a prompt. Implementations must not log or echo credentials.
pub trait Provider: Send + Sync {
    /// Human-readable description without credentials, e.g. `openai-compatible https://… (model)`.
    fn label(&self) -> String;
    fn complete(&self, prompt: &Prompt) -> Result<String, String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AskError {
    /// The provider could not be reached or failed. Nothing was proposed; the core is unaffected.
    Unavailable(String),
    /// The model answered, but the answer is not an acceptable proposal.
    Rejected(String),
}

impl fmt::Display for AskError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(reason) => write!(formatter, "AI provider unavailable: {reason}"),
            Self::Rejected(reason) => write!(formatter, "model output rejected: {reason}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub proposal: Proposal,
    /// What the provider answered (for the audit file; contains no configuration values).
    pub raw: String,
    pub provider: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hints {
    pub options: Vec<IndexEntry>,
    pub packages: Vec<IndexEntry>,
}

/// Validate and normalise the user's request text.
pub fn validate_request(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("the request must not be empty".into());
    }
    if text.chars().count() > MAX_REQUEST_CHARS {
        return Err(format!(
            "the request is longer than {MAX_REQUEST_CHARS} characters"
        ));
    }
    if text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err("the request must not contain control characters".into());
    }
    Ok(text.to_owned())
}

/// Candidate options and packages from the local index, found by keywords of the request.
pub fn collect_hints(
    request: &str,
    options: Option<&SearchIndex>,
    packages: Option<&SearchIndex>,
) -> Hints {
    let tokens = request
        .to_lowercase()
        .split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
        .filter(|token| token.chars().count() >= 3 && !STOP_WORDS.contains(token))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let gather = |index: Option<&SearchIndex>, exact_first: bool| {
        let mut found: Vec<IndexEntry> = Vec::new();
        let Some(index) = index else {
            return found;
        };
        for token in &tokens {
            let mut matches = index.find(token);
            matches.sort_by_key(|entry| {
                (
                    exact_first && entry.name.to_lowercase() != *token,
                    !entry.name.ends_with(".enable"),
                    entry.name.len(),
                )
            });
            for entry in matches.into_iter().take(HINTS_PER_TOKEN) {
                if found.len() < MAX_HINTS_PER_KIND && !found.iter().any(|e| e.name == entry.name) {
                    found.push(entry);
                }
            }
        }
        found
    };
    Hints {
        options: gather(options, false),
        packages: gather(packages, true),
    }
}

/// The deterministic prompt for a request. Contains no credentials, values or paths.
pub fn build_prompt(request: &str, host: &str, hints: &Hints) -> Prompt {
    let mut system = String::from(
        "You translate a user's request about their NixOS system into ONE JSON object. \
Output ONLY that JSON object: no prose, no markdown, no code fences.\n\
\n\
Schema (version 1), choose exactly one form:\n\
{\"schema\":1,\"changes\":[ITEM,...]}   (1 to 32 items)\n\
{\"schema\":1,\"action\":\"undo\"}      (revert the last change made by Relay)\n\
{\"schema\":1,\"action\":\"recover\"}   (resolve an interrupted change)\n\
{\"schema\":1,\"action\":\"status\"}    {\"schema\":1,\"action\":\"history\"}\n\
{\"schema\":1,\"action\":\"unsupported\",\"reason\":\"short explanation, at most 200 characters\"}\n\
ITEM is one of:\n\
{\"op\":\"set_option\",\"option\":\"dotted.option.path\",\"value\":true|false|INTEGER|\"string\"|[\"string\",...]}\n\
{\"op\":\"add_package\",\"package\":\"nixpkgs-attribute\"}\n\
{\"op\":\"remove_package\",\"package\":\"nixpkgs-attribute\"}\n\
\n\
Rules:\n\
- Use only option names and package attributes from the candidate lists below, or that you are \
certain exist in NixOS. Never invent names.\n\
- You cannot change system.stateVersion, the bootloader, filesystems, partitions, disk \
encryption, Nix daemon trust settings, users, passwords, SSH or sudo, secrets, or database \
major versions. For those answer with action \"unsupported\".\n\
- Never output Nix code, shell commands or file paths. Never include passwords, tokens or other \
secrets in any value.\n\
- If the request is ambiguous or is not about the system configuration, answer with action \
\"unsupported\" and a short reason.\n\
- Your answer is only a proposal: it is validated, built in a sandbox copy and confirmed by the \
user before anything changes.\n",
    );
    system.push_str(&format!("\nHost: {host}\n"));
    if !hints.options.is_empty() {
        system.push_str("\nCandidate options (name | type | description):\n");
        for entry in &hints.options {
            system.push_str(&format!(
                "- {} | {} | {}\n",
                entry.name,
                entry.type_name.as_deref().unwrap_or("?"),
                clip(&entry.description, 120)
            ));
        }
    }
    if !hints.packages.is_empty() {
        system.push_str("\nCandidate packages (attribute | description):\n");
        for entry in &hints.packages {
            system.push_str(&format!(
                "- {} | {}\n",
                entry.name,
                clip(&entry.description, 120)
            ));
        }
    }
    Prompt {
        system,
        user: format!("Request:\n<<<\n{request}\n>>>"),
    }
}

/// Prompt for a plain-language explanation of a (deterministic) change review.
pub fn build_explain_prompt(review: &str) -> Prompt {
    Prompt {
        system: "Explain, in plain language and in at most eight sentences, what the following \
Relay change review means for the user's NixOS system: what changes, how risky it is and how it \
can be undone. Answer in the language the user would most likely use if it is German, otherwise \
in English. Plain text only. Do not invent facts that are not in the review."
            .to_owned(),
        user: format!("Review:\n<<<\n{review}\n>>>"),
    }
}

/// Shorten and flatten text for prompts and display; control characters become spaces.
pub fn clip(text: &str, max_chars: usize) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let flat = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max_chars {
        flat
    } else {
        let kept: String = flat.chars().take(max_chars).collect();
        format!("{kept}…")
    }
}

/// Accept the whole answer as one JSON object, optionally inside a single code fence. Anything
/// else (prose around it, several objects) is rejected rather than guessed at.
pub fn extract_json_object(output: &str) -> Result<&str, String> {
    let trimmed = output.trim();
    let body = match trimmed.strip_prefix("```") {
        Some(rest) => {
            let rest = rest.strip_prefix("json").unwrap_or(rest);
            rest.trim_start()
                .strip_suffix("```")
                .ok_or_else(|| "unterminated code fence".to_owned())?
                .trim()
        }
        None => trimmed,
    };
    if body.starts_with('{') && body.ends_with('}') {
        Ok(body)
    } else {
        Err("the answer is not a single bare JSON object".into())
    }
}

/// Ask the provider and turn its answer into a validated proposal. Never touches the system.
pub fn propose(
    provider: &dyn Provider,
    request: &str,
    host: &str,
    options: Option<&SearchIndex>,
    packages: Option<&SearchIndex>,
) -> Result<Resolved, AskError> {
    let request = validate_request(request).map_err(AskError::Rejected)?;
    let hints = collect_hints(&request, options, packages);
    let prompt = build_prompt(&request, host, &hints);
    let raw = provider.complete(&prompt).map_err(AskError::Unavailable)?;
    if raw.len() > MAX_RESPONSE_BYTES {
        return Err(AskError::Rejected("the answer is too large".into()));
    }
    let json = extract_json_object(&raw).map_err(AskError::Rejected)?;
    let proposal = parse_proposal(json).map_err(AskError::Rejected)?;
    if let Proposal::Changes(changes) = &proposal {
        verify_changes(changes, options, packages).map_err(AskError::Rejected)?;
    }
    Ok(Resolved {
        proposal,
        raw,
        provider: provider.label(),
    })
}

/// Cross-check a model's changes against the local index (when one is available). This catches
/// invented names and impossible values before any evaluation; the evaluation of the candidate
/// remains the authority.
pub fn verify_changes(
    changes: &[Change],
    options: Option<&SearchIndex>,
    packages: Option<&SearchIndex>,
) -> Result<(), String> {
    for change in changes {
        match change {
            Change::SetOption { name, value } => {
                let Some(index) = options else { continue };
                let entry = index.entry(name).ok_or_else(|| {
                    format!(
                        "option '{name}' is not in the local options index (invented or stale?)"
                    )
                })?;
                if entry.read_only {
                    return Err(format!("option '{name}' is read-only"));
                }
                if let Some(type_name) = &entry.type_name {
                    if !value_matches_type(value, type_name) {
                        return Err(format!(
                            "the value for '{name}' does not fit its type ({type_name})"
                        ));
                    }
                }
            }
            Change::AddPackage { name } => {
                if let Some(index) = packages {
                    if index.entry(name).is_none() {
                        return Err(format!(
                            "package '{name}' is not in the local package index (invented or stale?)"
                        ));
                    }
                }
            }
            Change::RemovePackage { .. } => {}
        }
    }
    Ok(())
}

fn value_matches_type(value: &Value, type_name: &str) -> bool {
    let lower = type_name.to_lowercase();
    if lower.contains("anything") || lower.contains("unspecified") {
        return true;
    }
    match value {
        Value::Bool(_) => lower.contains("boolean"),
        Value::Integer(_) => lower.contains("integer") || lower.contains("number"),
        Value::String(_) => {
            lower.contains("string") || lower.contains("path") || lower.contains("one of")
        }
        Value::StringList(_) => lower.contains("list of"),
    }
}

// ----------------------------------------------------------------------- providers

/// Runs a user-configured program: the prompt goes to its stdin as `{"system":…,"user":…}` and
/// the model's text is read from its stdout. This keeps network clients, credentials and local
/// model runners entirely outside Relay (for example a small script around `ollama` or `curl`).
pub struct CommandProvider {
    runner: Arc<dyn Runner>,
    program: String,
    args: Vec<String>,
}

impl CommandProvider {
    pub fn new(runner: Arc<dyn Runner>, program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            runner,
            program: program.into(),
            args,
        }
    }
}

impl Provider for CommandProvider {
    fn label(&self) -> String {
        format!("command {}", self.program)
    }

    fn complete(&self, prompt: &Prompt) -> Result<String, String> {
        let outcome = self.runner.run(
            &Invocation::new(&self.program)
                .args(&self.args)
                .stdin(prompt.to_json())
                .timeout(PROVIDER_TIMEOUT),
        )?;
        if !outcome.success() {
            return Err(match outcome.code {
                Some(code) => format!("the provider command exited with status {code}"),
                None => "the provider command was terminated by a signal".to_owned(),
            });
        }
        outcome.stdout_text()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Api {
    /// OpenAI chat completions (also served by Ollama, vLLM, LM Studio, …).
    OpenAiCompatible,
    /// Anthropic Messages API.
    Anthropic,
}

/// Talks to a hosted or local model API through the system's `curl`, so Relay needs no network
/// or TLS code of its own. The request, including the API key, travels over curl's stdin as a
/// config file: nothing sensitive appears in the process list, the journal or any log.
pub struct HttpProvider {
    runner: Arc<dyn Runner>,
    api: Api,
    base_url: String,
    model: String,
    key: Option<String>,
}

impl fmt::Debug for HttpProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpProvider")
            .field("api", &self.api)
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("key", &self.key.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

impl HttpProvider {
    pub fn new(
        runner: Arc<dyn Runner>,
        api: Api,
        base_url: Option<&str>,
        model: &str,
        key: Option<String>,
    ) -> Result<Self, String> {
        let default = match api {
            Api::OpenAiCompatible => "https://api.openai.com/v1",
            Api::Anthropic => "https://api.anthropic.com/v1",
        };
        let base_url = base_url.unwrap_or(default).trim_end_matches('/').to_owned();
        validate_base_url(&base_url)?;
        if model.is_empty()
            || !model
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '/' | '-'))
        {
            return Err("invalid model name".into());
        }
        let key_ok = key.as_ref().is_none_or(|key| {
            !key.is_empty()
                && key.chars().all(|c| {
                    c.is_ascii_alphanumeric()
                        || matches!(c, '.' | '_' | '~' | '+' | '/' | '=' | '-')
                })
        });
        if !key_ok {
            return Err("the API key has an unexpected format".into());
        }
        if api == Api::Anthropic && key.is_none() {
            return Err("the Anthropic API needs an API key (RELAY_AI_API_KEY)".into());
        }
        Ok(Self {
            runner,
            api,
            base_url,
            model: model.to_owned(),
            key,
        })
    }

    fn endpoint(&self) -> String {
        match self.api {
            Api::OpenAiCompatible => format!("{}/chat/completions", self.base_url),
            Api::Anthropic => format!("{}/messages", self.base_url),
        }
    }

    fn request_body(&self, prompt: &Prompt) -> String {
        match self.api {
            Api::OpenAiCompatible => format!(
                "{{\"model\":{},\"temperature\":0,\"messages\":[{{\"role\":\"system\",\"content\":{}}},{{\"role\":\"user\",\"content\":{}}}]}}",
                json_string(&self.model),
                json_string(&prompt.system),
                json_string(&prompt.user)
            ),
            Api::Anthropic => format!(
                "{{\"model\":{},\"max_tokens\":1024,\"temperature\":0,\"system\":{},\"messages\":[{{\"role\":\"user\",\"content\":{}}}]}}",
                json_string(&self.model),
                json_string(&prompt.system),
                json_string(&prompt.user)
            ),
        }
    }

    /// The curl config fed through stdin.
    fn curl_config(&self, prompt: &Prompt) -> String {
        let mut config = format!(
            "url = {}\nrequest = \"POST\"\nheader = \"Content-Type: application/json\"\n",
            curl_quote(&self.endpoint())
        );
        match (self.api, &self.key) {
            (Api::OpenAiCompatible, Some(key)) => {
                config.push_str(&format!("header = \"Authorization: Bearer {key}\"\n"));
            }
            (Api::Anthropic, Some(key)) => {
                config.push_str(&format!("header = \"x-api-key: {key}\"\n"));
                config.push_str("header = \"anthropic-version: 2023-06-01\"\n");
            }
            _ => {}
        }
        config.push_str(&format!(
            "data = {}\n",
            curl_quote(&self.request_body(prompt))
        ));
        config
    }
}

impl Provider for HttpProvider {
    fn label(&self) -> String {
        format!(
            "{} {} ({})",
            match self.api {
                Api::OpenAiCompatible => "openai-compatible",
                Api::Anthropic => "anthropic",
            },
            self.base_url,
            self.model
        )
    }

    fn complete(&self, prompt: &Prompt) -> Result<String, String> {
        let protocols = if self.base_url.starts_with("https://") {
            "=https"
        } else {
            "=http"
        };
        let outcome = self.runner.run(
            &Invocation::new("curl")
                // `-q` must come first: it stops curl from reading ~/.curlrc.
                .args(["-q", "--silent", "--show-error", "--fail-with-body"])
                .args(["--proto", protocols])
                .args(["--max-time", "60", "--max-filesize", "1048576"])
                .args(["--config", "-"])
                .stdin(self.curl_config(prompt))
                .timeout(PROVIDER_TIMEOUT + Duration::from_secs(10)),
        )?;
        if !outcome.success() {
            // The body and stderr are withheld: they can echo request data.
            return Err(match outcome.code {
                Some(22) => {
                    "the API answered with an HTTP error (check key, model and URL)".to_owned()
                }
                Some(6 | 7 | 28) => "the API could not be reached".to_owned(),
                Some(code) => format!("curl failed with exit status {code}"),
                None => "curl was terminated by a signal".to_owned(),
            });
        }
        let body = outcome.stdout_text()?;
        let value = Json::parse(&body).map_err(|_| "the API did not return JSON".to_owned())?;
        let text = match self.api {
            Api::OpenAiCompatible => value
                .get("choices")
                .and_then(Json::as_array)
                .and_then(|choices| choices.first())
                .and_then(|choice| choice.get("message"))
                .and_then(|message| message.get("content"))
                .and_then(Json::as_str),
            Api::Anthropic => value
                .get("content")
                .and_then(Json::as_array)
                .and_then(|parts| {
                    parts
                        .iter()
                        .find(|part| part.get("type").and_then(Json::as_str) == Some("text"))
                })
                .and_then(|part| part.get("text"))
                .and_then(Json::as_str),
        };
        text.map(str::to_owned)
            .ok_or_else(|| "the API response has an unexpected shape".to_owned())
    }
}

/// `https://` anywhere; plain `http://` only for loopback (a local model server).
fn validate_base_url(url: &str) -> Result<(), String> {
    let (secure, rest) = match url.strip_prefix("https://") {
        Some(rest) => (true, rest),
        None => match url.strip_prefix("http://") {
            Some(rest) => (false, rest),
            None => {
                return Err(
                    "the API base URL must start with https:// (or http:// for localhost)".into(),
                );
            }
        },
    };
    let authority = rest.split('/').next().unwrap_or_default();
    if authority.is_empty()
        || authority.contains('@')
        || !url.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, ':' | '/' | '.' | '-' | '_' | '[' | ']')
        })
    {
        return Err("the API base URL contains unsupported characters or credentials".into());
    }
    if !secure {
        let host = authority
            .strip_prefix('[')
            .and_then(|rest| rest.split(']').next())
            .unwrap_or_else(|| authority.split(':').next().unwrap_or_default());
        if !matches!(host, "localhost" | "127.0.0.1" | "::1") {
            return Err("plain http:// is only allowed for localhost".into());
        }
    }
    Ok(())
}

/// Quote a value for a curl config file.
fn curl_quote(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for c in value.chars() {
        match c {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests;
