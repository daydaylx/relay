mod ask;
mod desktop;

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::File;
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use relay::{
    ApplyOutcome, ApplyReport, Change, Confirmation, Engine, HealthPolicy, Host, HyprlandIpc,
    IndexKind, JournalEntry, NixAdapter, PlanOutcome, ProcessRunner, Runner, SearchIndex, StateDir,
    Value, generations, json_string, parse_intent, render_managed, system_summary_live,
    validate_change,
};

const OPERATION_FAILED_ROLLED_BACK: u8 = 2;

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("relay: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<ExitCode, String> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err(usage().to_owned());
    };
    let rest = &args[1..];
    match command {
        "--help" | "help" => println!("{}", usage()),
        "--version" | "version" => println!("relay {}", env!("CARGO_PKG_VERSION")),
        "status" => status(rest)?,
        "generations" => list_generations(rest)?,
        "health" => health(rest)?,
        "index-options" | "index-packages" => run_index(command, rest)?,
        "search-option" | "search-package" => run_search(command, rest)?,
        "check" => check(rest)?,
        "render-managed" => {
            let parsed = parse(rest, &[], &[])?;
            print!("{}", render_managed(&[parse_change(&parsed.positional)?])?);
        }
        "init" => init(rest)?,
        "plan" => plan(rest)?,
        "preview" => preview(rest)?,
        "show" => show(rest)?,
        "history" => history(rest)?,
        "discard" => discard(rest)?,
        "apply" => return apply(rest),
        "undo" => return undo(rest),
        "recover" => return recover(rest),
        "ask" => return ask::ask(rest),
        "desktop" => return desktop::desktop(rest),
        _ => return Err(format!("unknown command '{command}'\n{}", usage())),
    }
    Ok(ExitCode::SUCCESS)
}

// ------------------------------------------------------------------ argument parsing

#[derive(Debug, Default)]
struct Parsed {
    positional: Vec<String>,
    values: BTreeMap<&'static str, Vec<String>>,
    switches: BTreeSet<&'static str>,
}

impl Parsed {
    fn one(&self, flag: &str) -> Result<Option<&str>, String> {
        match self.values.get(flag).map(Vec::as_slice) {
            None => Ok(None),
            Some([value]) => Ok(Some(value)),
            Some(_) => Err(format!("{flag} may be given only once")),
        }
    }

    fn required(&self, flag: &str) -> Result<&str, String> {
        self.one(flag)?
            .ok_or_else(|| format!("{flag} is required\n{}", usage()))
    }

    fn all(&self, flag: &str) -> &[String] {
        self.values.get(flag).map_or(&[], Vec::as_slice)
    }

    fn has(&self, switch: &str) -> bool {
        self.switches.contains(switch)
    }

    fn id(&self) -> Result<&str, String> {
        match self.positional.as_slice() {
            [id] => Ok(id),
            _ => Err(usage().to_owned()),
        }
    }

    fn no_positional(&self) -> Result<(), String> {
        if self.positional.is_empty() {
            Ok(())
        } else {
            Err(usage().to_owned())
        }
    }
}

/// Only `--long` options are flags, so negative integers stay usable as values.
fn parse(
    args: &[String],
    value_flags: &[&'static str],
    switch_flags: &[&'static str],
) -> Result<Parsed, String> {
    let mut parsed = Parsed::default();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if !arg.starts_with("--") {
            parsed.positional.push(arg.clone());
        } else if let Some(flag) = value_flags.iter().find(|flag| **flag == arg) {
            let value = iter
                .next()
                .ok_or_else(|| format!("{flag} needs a value\n{}", usage()))?;
            parsed.values.entry(flag).or_default().push(value.clone());
        } else if let Some(flag) = switch_flags.iter().find(|flag| **flag == arg) {
            parsed.switches.insert(flag);
        } else {
            return Err(format!("unknown option '{arg}'\n{}", usage()));
        }
    }
    Ok(parsed)
}

fn root_of(parsed: &Parsed) -> Result<PathBuf, String> {
    Ok(parsed
        .one("--root")?
        .map_or_else(|| PathBuf::from("/"), PathBuf::from))
}

fn state_dir_of(parsed: &Parsed) -> Result<PathBuf, String> {
    match parsed.one("--state-dir")? {
        Some(path) => Ok(PathBuf::from(path)),
        None => StateDir::default_location(),
    }
}

fn build_engine(parsed: &Parsed) -> Result<Engine, String> {
    let runner: Arc<dyn Runner> = Arc::new(ProcessRunner::default());
    let mut health = HealthPolicy::default();
    if let Some(seconds) = parsed.one("--observe")? {
        let seconds: u64 = seconds
            .parse()
            .map_err(|_| "--observe takes a whole number of seconds".to_owned())?;
        health.observe = std::time::Duration::from_secs(seconds.min(600));
    }
    let mut engine = Engine::new(
        StateDir::new(state_dir_of(parsed)?),
        Host::new(root_of(parsed)?),
        NixAdapter::with_runner("nix", Arc::clone(&runner)),
        runner,
        health,
    )
    .with_reporter(Arc::new(|message| eprintln!("relay: {message}")));
    // A live host with a running Hyprland session: the compositor joins the safety loop.
    if root_of(parsed)? == Path::new("/") && !parsed.has("--no-desktop-check") {
        if let Some(ipc) = HyprlandIpc::from_env(&|key| std::env::var(key).ok()) {
            engine = engine.with_desktop(Arc::new(ipc));
        }
    }
    Ok(engine)
}

fn parse_change(args: &[String]) -> Result<Change, String> {
    match args {
        [operation, name] if operation == "add-package" => {
            Ok(Change::AddPackage { name: name.clone() })
        }
        [operation, name] if operation == "remove-package" => {
            Ok(Change::RemovePackage { name: name.clone() })
        }
        [operation, name, kind, value] if operation == "set-option" => {
            let value = match kind.as_str() {
                "bool" => match value.as_str() {
                    "true" => Value::Bool(true),
                    "false" => Value::Bool(false),
                    _ => return Err("boolean value must be 'true' or 'false'".into()),
                },
                "integer" => Value::Integer(value.parse().map_err(|_| "integer value is invalid")?),
                "string" => Value::String(value.clone()),
                "string-list" => Value::StringList(if value.is_empty() {
                    Vec::new()
                } else {
                    value.split(',').map(str::to_owned).collect()
                }),
                _ => return Err("value type must be bool, integer, string, or string-list".into()),
            };
            Ok(Change::SetOption {
                name: name.clone(),
                value,
            })
        }
        _ => Err(usage().to_owned()),
    }
}

// ------------------------------------------------------------------- observer commands

fn status(args: &[String]) -> Result<(), String> {
    let parsed = parse(args, &["--root", "--flake", "--state-dir"], &[])?;
    parsed.no_positional()?;
    let runner: Arc<dyn Runner> = Arc::new(ProcessRunner::default());
    let flake = parsed.one("--flake")?.map(PathBuf::from);
    let state = state_dir_of(&parsed).ok().map(StateDir::new);
    let summary = system_summary_live(
        &root_of(&parsed)?,
        &runner,
        flake.as_deref(),
        state.as_ref(),
    );
    println!(
        "{{\"hostname\":{},\"os_name\":{},\"os_version\":{},\"kernel\":{},\"running_system_path\":{},\"active_generation\":{},\"booted_generation\":{},\"config_identity\":{},\"configuration_revision\":{},\"nixpkgs_revision\":{},\"failed_units\":{},\"desktop_session\":{},\"managed_module\":{},\"unresolved_change\":{},\"desktop\":{}}}",
        optional_json(summary.hostname.as_deref()),
        optional_json(summary.os_name.as_deref()),
        optional_json(summary.os_version.as_deref()),
        optional_json(summary.kernel.as_deref()),
        optional_json(summary.running_system_path.as_deref()),
        optional_number(summary.active_generation),
        optional_number(summary.booted_generation),
        optional_json(summary.config_identity.as_deref()),
        optional_json(summary.configuration_revision.as_deref()),
        optional_json(summary.nixpkgs_revision.as_deref()),
        optional_strings_json(summary.failed_units.as_deref()),
        optional_json(summary.desktop_session.as_deref()),
        optional_json(summary.managed_module.as_deref()),
        optional_json(summary.unresolved_change.as_deref()),
        summary
            .desktop
            .as_ref()
            .map_or_else(|| "null".to_owned(), desktop::summary_json),
    );
    Ok(())
}

fn list_generations(args: &[String]) -> Result<(), String> {
    let parsed = parse(args, &["--root"], &[])?;
    parsed.no_positional()?;
    let values = generations(&root_of(&parsed)?)
        .map_err(|error| format!("could not read system generations: {error}"))?;
    let json = values
        .iter()
        .map(|generation| {
            format!(
                "{{\"number\":{},\"system_path\":{},\"active\":{},\"booted\":{}}}",
                generation.number,
                json_string(&generation.system_path),
                generation.active,
                generation.booted
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    println!("[{json}]");
    Ok(())
}

fn health(args: &[String]) -> Result<(), String> {
    let parsed = parse(args, &[], &[])?;
    parsed.no_positional()?;
    let runner: Arc<dyn Runner> = Arc::new(ProcessRunner::default());
    let snapshot = relay::SystemAdapter::new(runner).snapshot()?;
    let units = snapshot.unhealthy_units.into_iter().collect::<Vec<_>>();
    println!(
        "{{\"system_state\":{},\"unhealthy_units\":{}}}",
        json_string(&snapshot.system_state),
        optional_strings_json(Some(&units)),
    );
    Ok(())
}

fn run_search(command: &str, args: &[String]) -> Result<(), String> {
    let parsed = parse(args, &["--index", "--flake", "--host", "--root"], &[])?;
    let query = match parsed.positional.as_slice() {
        [query] => query.clone(),
        _ => return Err(usage().to_owned()),
    };
    if query.trim().is_empty() {
        return Err("search query must not be empty".to_owned());
    }
    let index_path = PathBuf::from(parsed.required("--index")?);
    let flake = PathBuf::from(parsed.required("--flake")?);
    let host = parsed.required("--host")?.to_owned();
    let root = root_of(&parsed)?;
    let kind = if command == "search-option" {
        IndexKind::Options
    } else {
        IndexKind::Packages
    };
    let index = SearchIndex::read(&index_path, kind.as_str())?;
    let current_json = NixAdapter::default().current_index_identity_json(&flake, &host, kind)?;
    let current = SearchIndex::from_json(&current_json, kind.as_str())?;
    index.validate_identity(&current)?;
    let running_host = relay::system_summary(&root).hostname;
    if let Some(host) = running_host.as_deref() {
        index.validate_host(host)?;
    }
    let results = index.search(&query);
    if running_host.is_none() {
        eprintln!("warning: running host identity unavailable; flake index identity was verified");
    }
    println!(
        "{{\"index\":{},\"results\":[{}]}}",
        index.metadata_json(),
        results.join(",")
    );
    Ok(())
}

fn run_index(command: &str, args: &[String]) -> Result<(), String> {
    let parsed = parse(args, &["--flake", "--host", "--output"], &[])?;
    parsed.no_positional()?;
    let flake = PathBuf::from(parsed.required("--flake")?);
    let host = parsed.required("--host")?.to_owned();
    let output = PathBuf::from(parsed.required("--output")?);
    let kind = if command == "index-options" {
        IndexKind::Options
    } else {
        IndexKind::Packages
    };
    let json = NixAdapter::default().generate_index_json(&flake, &host, kind)?;
    SearchIndex::from_json(&json, kind.as_str())?;
    relay::write_atomic_file(&output, json.as_bytes())?;
    println!("wrote {} index to {}", kind.as_str(), output.display());
    Ok(())
}

// --------------------------------------------------------------- offline change checks

fn check(args: &[String]) -> Result<(), String> {
    let parsed = parse(args, &[], &[])?;
    let change = parse_change(&parsed.positional)?;
    let risk = validate_change(&change)?;
    println!(
        "{{\"risk\":{},\"change\":{}}}",
        json_string(risk.as_str()),
        json_string(&format!("{change:?}"))
    );
    Ok(())
}

// ---------------------------------------------------------------------- change workflow

fn init(args: &[String]) -> Result<(), String> {
    let parsed = parse(args, &["--flake", "--state-dir", "--root"], &[])?;
    parsed.no_positional()?;
    let flake = PathBuf::from(parsed.required("--flake")?);
    let report = build_engine(&parsed)?.init(&flake)?;
    let mut steps = vec![
        "Add `./relay/managed.nix` to the `modules` (or `imports`) of your host configuration; Relay never edits other files."
            .to_owned(),
    ];
    if report.tracked_by_git == Some(false) {
        steps.push("Run `git add relay/managed.nix`: flakes only see tracked files.".to_owned());
    }
    steps.push(
        "Rebuild once yourself (`sudo nixos-rebuild switch --flake .#<host>`) so the running system publishes /etc/relay/managed.nix."
            .to_owned(),
    );
    println!(
        "{{\"path\":{},\"created\":{},\"next_steps\":{}}}",
        json_string(&report.path.to_string_lossy()),
        report.created,
        optional_strings_json(Some(&steps)),
    );
    Ok(())
}

fn plan(args: &[String]) -> Result<(), String> {
    let parsed = parse(
        args,
        &["--flake", "--host", "--state-dir", "--root", "--intent"],
        &["--preview"],
    )?;
    let flake = PathBuf::from(parsed.required("--flake")?);
    let host = parsed.required("--host")?.to_owned();
    let changes = match (parsed.one("--intent")?, parsed.positional.is_empty()) {
        (Some(source), true) => parse_intent(&read_intent(source)?)?,
        (None, false) => vec![parse_change(&parsed.positional)?],
        _ => return Err(format!("give either a change or --intent\n{}", usage())),
    };
    let engine = build_engine(&parsed)?;
    plan_with(&engine, &flake, &host, &changes, parsed.has("--preview"))?;
    Ok(())
}

/// Plan, optionally preview, and print the plan as JSON.
fn plan_with(
    engine: &Engine,
    flake: &Path,
    host: &str,
    changes: &[Change],
    preview: bool,
) -> Result<PlanOutcome, String> {
    let outcome = engine.plan(flake, host, changes)?;
    if preview {
        let text = engine.preview(&outcome.record.id)?;
        eprintln!("relay: dry-activate preview (nothing was changed):\n{text}");
    }
    println!("{}", plan_json(&outcome));
    Ok(outcome)
}

fn read_intent(source: &str) -> Result<String, String> {
    const LIMIT: u64 = 70 * 1024;
    let mut text = String::new();
    let result = if source == "-" {
        std::io::stdin().take(LIMIT).read_to_string(&mut text)
    } else {
        File::open(source)
            .map_err(|error| format!("could not read intent {source}: {error}"))?
            .take(LIMIT)
            .read_to_string(&mut text)
    };
    result.map_err(|error| format!("could not read intent: {error}"))?;
    Ok(text)
}

fn plan_json(outcome: &PlanOutcome) -> String {
    let record = &outcome.record;
    format!(
        "{{\"id\":{},\"risk\":{},\"applicable\":{},\"base_system\":{},\"candidate_system\":{},\"candidate_drv\":{},\"reboot_components\":{},\"inhibitors\":{},\"managed_diff\":{},\"closure_diff\":{},\"next\":{}}}",
        json_string(&record.id),
        json_string(record.risk.as_str()),
        outcome.applicable,
        json_string(&record.base_system_path),
        json_string(&record.candidate_system_path),
        json_string(&record.candidate_drv),
        optional_strings_json(Some(&record.reboot_components)),
        optional_strings_json(Some(&record.inhibitors)),
        optional_strings_json(Some(&outcome.managed_diff)),
        optional_json(outcome.closure_diff.as_deref()),
        if outcome.applicable {
            json_string(&format!("relay apply {}", record.id))
        } else {
            "null".to_owned()
        },
    )
}

fn preview(args: &[String]) -> Result<(), String> {
    let parsed = parse(args, &["--state-dir", "--root"], &[])?;
    let text = build_engine(&parsed)?.preview(parsed.id()?)?;
    println!("{text}");
    Ok(())
}

fn show(args: &[String]) -> Result<(), String> {
    let parsed = parse(args, &["--state-dir", "--root"], &[])?;
    println!("{}", build_engine(&parsed)?.review(parsed.id()?)?);
    Ok(())
}

fn discard(args: &[String]) -> Result<(), String> {
    let parsed = parse(args, &["--state-dir", "--root"], &[])?;
    build_engine(&parsed)?.discard(parsed.id()?)?;
    println!(
        "{{\"id\":{},\"state\":\"failed\",\"detail\":\"discarded\"}}",
        json_string(parsed.id()?)
    );
    Ok(())
}

fn history(args: &[String]) -> Result<(), String> {
    let parsed = parse(args, &["--state-dir", "--root"], &[])?;
    parsed.no_positional()?;
    history_with(&build_engine(&parsed)?)
}

fn history_with(engine: &Engine) -> Result<(), String> {
    let entries = engine.history()?;
    println!(
        "[{}]",
        entries.iter().map(entry_json).collect::<Vec<_>>().join(",")
    );
    Ok(())
}

fn entry_json(entry: &JournalEntry) -> String {
    format!(
        "{{\"id\":{},\"state\":{},\"timestamp\":{},\"detail\":{},\"previous_system\":{},\"candidate_system\":{}}}",
        json_string(&entry.id),
        json_string(entry.state.as_str()),
        entry.timestamp,
        optional_json(entry.detail.as_deref()),
        optional_json(entry.previous_system_path.as_deref()),
        optional_json(entry.candidate_system_path.as_deref()),
    )
}

fn apply(args: &[String]) -> Result<ExitCode, String> {
    let parsed = parse(
        args,
        &["--state-dir", "--root", "--expect-active", "--observe"],
        &["--yes", "--no-desktop-check"],
    )?;
    let engine = build_engine(&parsed)?;
    apply_with(
        &engine,
        parsed.id()?,
        parsed.has("--yes"),
        parsed.all("--expect-active"),
    )
}

fn apply_with(
    engine: &Engine,
    id: &str,
    assume_yes: bool,
    expect_active: &[String],
) -> Result<ExitCode, String> {
    let confirmation = confirm(&engine.review(id)?, "Apply this change?", assume_yes)?;
    let report = engine.apply(id, confirmation, expect_active)?;
    Ok(finish(&report, false))
}

fn undo(args: &[String]) -> Result<ExitCode, String> {
    let parsed = parse(args, &["--state-dir", "--root", "--observe"], &["--yes"])?;
    parsed.no_positional()?;
    undo_with(&build_engine(&parsed)?, parsed.has("--yes"))
}

fn undo_with(engine: &Engine, assume_yes: bool) -> Result<ExitCode, String> {
    let target = engine.undo_target()?;
    let summary = format!(
        "Undo the most recent applied change:\n{}",
        engine.explain(&target)?
    );
    let confirmation = confirm(&summary, "Undo it?", assume_yes)?;
    Ok(finish(&engine.undo(confirmation)?, true))
}

fn recover(args: &[String]) -> Result<ExitCode, String> {
    let parsed = parse(
        args,
        &["--state-dir", "--root", "--observe"],
        &["--abort-pending"],
    )?;
    parsed.no_positional()?;
    recover_with(&build_engine(&parsed)?, parsed.has("--abort-pending"))
}

fn recover_with(engine: &Engine, abort_pending: bool) -> Result<ExitCode, String> {
    let results = engine.recover(abort_pending)?;
    let items = results
        .iter()
        .map(|entry| {
            format!(
                "{{\"id\":{},\"summary\":{},\"outcome\":{}}}",
                json_string(&entry.id),
                json_string(&entry.summary),
                entry.outcome.as_ref().map_or_else(
                    || "null".to_owned(),
                    |outcome| json_string(outcome_name(outcome))
                ),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    println!("[{items}]");
    let failed = results.iter().any(|entry| {
        matches!(
            entry.outcome,
            Some(ApplyOutcome::RolledBack { .. } | ApplyOutcome::RollbackIncomplete { .. })
        )
    });
    Ok(if failed {
        ExitCode::from(OPERATION_FAILED_ROLLED_BACK)
    } else {
        ExitCode::SUCCESS
    })
}

/// An undo that restored the previous state is a success; any other rollback is not.
fn finish(report: &ApplyReport, is_undo: bool) -> ExitCode {
    let reason = match &report.outcome {
        ApplyOutcome::RolledBack { reason } | ApplyOutcome::RollbackIncomplete { reason } => {
            Some(reason.as_str())
        }
        _ => None,
    };
    println!(
        "{{\"id\":{},\"outcome\":{},\"reason\":{},\"notes\":{}}}",
        json_string(&report.id),
        json_string(outcome_name(&report.outcome)),
        optional_json(reason),
        optional_strings_json(Some(&report.notes)),
    );
    let undone = is_undo
        && reason == Some("undo")
        && matches!(report.outcome, ApplyOutcome::RolledBack { .. });
    if report.outcome.succeeded() || undone {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(OPERATION_FAILED_ROLLED_BACK)
    }
}

fn outcome_name(outcome: &ApplyOutcome) -> &'static str {
    match outcome {
        ApplyOutcome::Switched => "switched",
        ApplyOutcome::RebootPending => "reboot-pending",
        ApplyOutcome::RolledBack { .. } => "rolled-back",
        ApplyOutcome::RollbackIncomplete { .. } => "rollback-incomplete",
    }
}

/// Show what is about to happen and obtain a deliberate "yes" (or an explicit `--yes`).
fn confirm(summary: &str, question: &str, assume_yes: bool) -> Result<Confirmation, String> {
    eprintln!("{summary}\n");
    if assume_yes {
        return Ok(Confirmation::granted());
    }
    eprint!("{question} Type 'yes' to continue: ");
    let _ = std::io::stderr().flush();
    let mut answer = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut answer)
        .map_err(|error| format!("could not read the confirmation: {error}"))?;
    if answer.trim() == "yes" {
        Ok(Confirmation::granted())
    } else {
        Err("not confirmed; nothing was changed".to_owned())
    }
}

// ---------------------------------------------------------------------------- output helpers

fn optional_json(value: Option<&str>) -> String {
    value.map(json_string).unwrap_or_else(|| "null".to_owned())
}

fn optional_number(value: Option<u64>) -> String {
    value
        .map(|number| number.to_string())
        .unwrap_or_else(|| "null".to_owned())
}

fn optional_strings_json(values: Option<&[String]>) -> String {
    let Some(values) = values else {
        return "null".to_owned();
    };
    let values = values
        .iter()
        .map(|value| json_string(value))
        .collect::<Vec<_>>()
        .join(",");
    format!("[{values}]")
}

fn usage() -> &'static str {
    "usage: relay status [--root PATH] [--flake PATH] [--state-dir DIR]
       relay generations [--root PATH]
       relay health
       relay <index-options|index-packages> --flake PATH --host HOST --output PATH
       relay <search-option|search-package> <QUERY> --index PATH --flake PATH --host HOST [--root PATH]
       relay check <add-package|remove-package> <PACKAGE>
       relay check set-option <OPTION> <bool|integer|string|string-list> <VALUE>
       relay render-managed <change as for check>
       relay init --flake PATH
       relay plan --flake PATH --host HOST [--preview] [--state-dir DIR] (<change as for check> | --intent FILE|-)
       relay show|preview|discard <CHANGE-ID> [--state-dir DIR]
       relay history [--state-dir DIR]
       relay apply <CHANGE-ID> [--yes] [--expect-active UNIT]... [--observe SECONDS] [--no-desktop-check] [--state-dir DIR]
       relay undo [--yes] [--observe SECONDS] [--state-dir DIR]
       relay recover [--abort-pending] [--observe SECONDS] [--state-dir DIR]
       relay ask <REQUEST> --host HOST [--flake PATH] [--provider command|openai|anthropic] [--model M] [--show-prompt] [--explain] [--apply] [--options-index P] [--packages-index P]
       relay desktop <status|health>
       relay <help|--help|version|--version>"
}

#[cfg(test)]
mod tests {
    use super::{
        optional_json, optional_number, optional_strings_json, parse, parse_change, root_of,
    };
    use relay::{Change, Value};

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn root_option_defaults_to_host_root() {
        let none = parse(&[], &["--root"], &[]).unwrap();
        assert_eq!(root_of(&none).unwrap(), std::path::PathBuf::from("/"));
        let fixture = parse(&strings(&["--root", "/tmp/fixture"]), &["--root"], &[]).unwrap();
        assert_eq!(
            root_of(&fixture).unwrap(),
            std::path::PathBuf::from("/tmp/fixture")
        );
        assert!(parse(&strings(&["--root"]), &["--root"], &[]).is_err());
        assert!(
            parse(&strings(&["--root", "a", "--root", "b"]), &["--root"], &[])
                .map(|p| root_of(&p))
                .unwrap()
                .is_err()
        );
    }

    #[test]
    fn parser_separates_flags_switches_and_positionals_and_rejects_unknown_options() {
        let parsed = parse(
            &strings(&[
                "id-1",
                "--yes",
                "--expect-active",
                "a.service",
                "--expect-active",
                "b.service",
            ]),
            &["--expect-active"],
            &["--yes"],
        )
        .unwrap();
        assert_eq!(parsed.positional, ["id-1"]);
        assert!(parsed.has("--yes"));
        assert_eq!(parsed.all("--expect-active"), ["a.service", "b.service"]);
        assert!(parse(&strings(&["--bogus"]), &[], &[]).is_err());
        assert!(parse(&strings(&["--yes"]), &["--expect-active"], &[]).is_err());
        // Negative integers are values, not flags.
        let negative = parse(&strings(&["set-option", "a.b", "integer", "-5"]), &[], &[]).unwrap();
        assert_eq!(
            parse_change(&negative.positional).unwrap(),
            Change::SetOption {
                name: "a.b".into(),
                value: Value::Integer(-5)
            }
        );
    }

    #[test]
    fn search_and_index_commands_require_all_arguments() {
        assert!(super::run_search("search-option", &strings(&["--index", "x.json"])).is_err());
        assert!(super::run_search("search-option", &strings(&["term"])).is_err());
        assert!(
            super::run_search("search-option", &strings(&["term", "--index", "x.json"])).is_err()
        );
        assert!(
            super::run_search(
                "search-option",
                &strings(&["one", "two", "--index", "x.json"])
            )
            .is_err()
        );
        assert!(super::run_index("index-options", &strings(&["--flake", "/etc/nixos"])).is_err());
    }

    #[test]
    fn change_arguments_map_to_typed_changes() {
        assert_eq!(
            parse_change(&strings(&["add-package", "vlc"])).unwrap(),
            Change::AddPackage { name: "vlc".into() }
        );
        assert_eq!(
            parse_change(&strings(&[
                "set-option",
                "hardware.bluetooth.enable",
                "bool",
                "true"
            ]))
            .unwrap(),
            Change::SetOption {
                name: "hardware.bluetooth.enable".into(),
                value: Value::Bool(true)
            }
        );
        assert!(parse_change(&strings(&["set-option", "a.b", "bool", "yes"])).is_err());
        assert!(parse_change(&strings(&["set-option", "a.b", "float", "1.5"])).is_err());
        assert!(parse_change(&strings(&["run", "rm -rf /"])).is_err());
    }

    #[test]
    fn missing_status_fields_are_json_null() {
        assert_eq!(optional_json(None), "null");
        assert_eq!(optional_json(Some("host")), "\"host\"");
        assert_eq!(optional_number(None), "null");
        assert_eq!(optional_number(Some(7)), "7");
        assert_eq!(optional_strings_json(None), "null");
        assert_eq!(optional_strings_json(Some(&[])), "[]");
        assert_eq!(
            optional_strings_json(Some(&["failed.service".to_owned()])),
            "[\"failed.service\"]"
        );
    }
}
