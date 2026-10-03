//! `relay desktop`: read-only Hyprland status and health (target state T6).

use std::process::ExitCode;

use relay::{DesktopProbe, DesktopSummary, HyprlandIpc, json_string};

use crate::{optional_json, optional_strings_json, parse};

const NO_SESSION: &str = "no Hyprland session detected (HYPRLAND_INSTANCE_SIGNATURE is unset or its \
socket is missing); run this inside the session";

fn session() -> Result<HyprlandIpc, String> {
    HyprlandIpc::from_env(&|key| std::env::var(key).ok()).ok_or_else(|| NO_SESSION.to_owned())
}

pub fn desktop(args: &[String]) -> Result<ExitCode, String> {
    let (subcommand, rest) = match args.split_first() {
        Some((subcommand, rest)) => (subcommand.as_str(), rest),
        None => return Err(crate::usage().to_owned()),
    };
    parse(rest, &[], &[])?.no_positional()?;
    match subcommand {
        "status" => {
            println!("{}", summary_json(&session()?.summary()?));
            Ok(ExitCode::SUCCESS)
        }
        "health" => {
            let ipc = session()?;
            match ipc.snapshot() {
                Ok(snapshot) => {
                    println!(
                        "{{\"responsive\":true,\"monitors\":{},\"config_errors\":{}}}",
                        snapshot.monitors,
                        optional_strings_json(Some(&snapshot.config_errors))
                    );
                    Ok(ExitCode::SUCCESS)
                }
                Err(error) => {
                    println!("{{\"responsive\":false,\"error\":{}}}", json_string(&error));
                    Ok(ExitCode::FAILURE)
                }
            }
        }
        _ => Err(crate::usage().to_owned()),
    }
}

pub fn summary_json(summary: &DesktopSummary) -> String {
    let monitors = summary
        .monitors
        .iter()
        .map(|monitor| {
            format!(
                "{{\"name\":{},\"width\":{},\"height\":{},\"refresh_hz\":{:.2},\"focused\":{},\"disabled\":{}}}",
                json_string(&monitor.name),
                monitor.width,
                monitor.height,
                monitor.refresh_hz,
                monitor.focused,
                monitor.disabled
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let workspaces = summary
        .workspaces
        .iter()
        .map(|workspace| {
            format!(
                "{{\"id\":{},\"name\":{},\"monitor\":{},\"windows\":{}}}",
                workspace.id,
                json_string(&workspace.name),
                json_string(&workspace.monitor),
                workspace.windows
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"compositor\":{},\"version\":{},\"monitors\":[{monitors}],\"workspaces\":[{workspaces}],\"window_count\":{},\"active_window_class\":{},\"config_errors\":{}}}",
        json_string(summary.compositor),
        json_string(&summary.version),
        summary.window_count,
        optional_json(summary.active_window_class.as_deref()),
        optional_strings_json(Some(&summary.config_errors)),
    )
}
