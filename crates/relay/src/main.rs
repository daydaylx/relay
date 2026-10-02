use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use relay::{SearchIndex, generations, json_string, system_summary};

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("relay: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err(usage().to_owned());
    };

    if command == "--help" || command == "help" {
        println!("{}", usage());
        return Ok(());
    }
    if command == "--version" || command == "version" {
        println!("relay {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    if command == "search-option" || command == "search-package" {
        return run_search(command, &args[1..]);
    }

    let root = parse_root(&args[1..])?;
    match command {
        "status" => {
            let summary = system_summary(&root);
            println!(
                "{{\"hostname\":{},\"os_name\":{},\"os_version\":{},\"kernel\":{},\"running_system_path\":{},\"active_generation\":{},\"booted_generation\":{},\"config_identity\":{},\"nixpkgs_revision\":{},\"failed_units\":{},\"desktop_session\":{}}}",
                optional_json(summary.hostname.as_deref()),
                optional_json(summary.os_name.as_deref()),
                optional_json(summary.os_version.as_deref()),
                optional_json(summary.kernel.as_deref()),
                optional_json(summary.running_system_path.as_deref()),
                optional_number(summary.active_generation),
                optional_number(summary.booted_generation),
                optional_json(summary.config_identity.as_deref()),
                optional_json(summary.nixpkgs_revision.as_deref()),
                optional_strings_json(summary.failed_units.as_deref()),
                optional_json(summary.desktop_session.as_deref()),
            );
            Ok(())
        }
        "generations" => {
            let values = generations(&root)
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
        _ => Err(format!("unknown command '{command}'\n{}", usage())),
    }
}

fn run_search(command: &str, args: &[String]) -> Result<(), String> {
    let (query, index_path) = parse_search_args(args)?;
    let kind = if command == "search-option" {
        "option"
    } else {
        "package"
    };
    let index = SearchIndex::read(&index_path, kind)?;
    let results = index.search(&query);
    eprintln!("warning: supplied index freshness and host identity have not been verified");
    println!(
        "{{\"index\":{},\"results\":[{}]}}",
        index.metadata_json(),
        results.join(",")
    );
    Ok(())
}

fn parse_search_args(args: &[String]) -> Result<(String, PathBuf), String> {
    let mut query = None;
    let mut index_path = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--index" => {
                let path = iter.next().ok_or_else(|| usage().to_owned())?;
                index_path = Some(PathBuf::from(path));
            }
            value if value.starts_with('-') => return Err(usage().to_owned()),
            value if query.is_none() => query = Some(value.to_owned()),
            _ => return Err(usage().to_owned()),
        }
    }
    let query = query.ok_or_else(|| usage().to_owned())?;
    if query.trim().is_empty() {
        return Err("search query must not be empty".to_owned());
    }
    let index_path = index_path.ok_or_else(|| usage().to_owned())?;
    Ok((query, index_path))
}

fn parse_root(args: &[String]) -> Result<PathBuf, String> {
    match args {
        [] => Ok(PathBuf::from("/")),
        [flag, path] if flag == "--root" => Ok(PathBuf::from(path)),
        _ => Err(usage().to_owned()),
    }
}

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
    "usage: relay <status|generations> [--root PATH]\n       relay <search-option|search-package> <QUERY> --index PATH\n       relay <help|--help|version|--version>"
}

#[cfg(test)]
mod tests {
    use super::{
        optional_json, optional_number, optional_strings_json, parse_root, parse_search_args,
    };

    #[test]
    fn root_option_defaults_to_host_root() {
        assert_eq!(parse_root(&[]).unwrap(), std::path::PathBuf::from("/"));
        assert_eq!(
            parse_root(&["--root".to_owned(), "/tmp/fixture".to_owned()]).unwrap(),
            std::path::PathBuf::from("/tmp/fixture")
        );
        assert!(parse_root(&["--root".to_owned()]).is_err());
    }

    #[test]
    fn search_requires_one_query_and_an_index_path() {
        assert_eq!(
            parse_search_args(&[
                "bluetooth".to_owned(),
                "--index".to_owned(),
                "options.json".to_owned()
            ])
            .unwrap(),
            (
                "bluetooth".to_owned(),
                std::path::PathBuf::from("options.json")
            )
        );
        assert!(parse_search_args(&["--index".to_owned(), "x.json".to_owned()]).is_err());
        assert!(parse_search_args(&["term".to_owned()]).is_err());
        assert!(
            parse_search_args(&[
                "one".to_owned(),
                "two".to_owned(),
                "--index".to_owned(),
                "x.json".to_owned()
            ])
            .is_err()
        );
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
