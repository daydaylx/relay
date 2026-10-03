# Relay system agent

The optional TypeScript front end embeds only the official Pi agent runtime, model API and TUI.
The Rust `relay` binary remains usable by itself and owns policy, Nix operations, confirmation,
activation, verification and recovery.

## Current integration stage

The agent uses Original Pi's agent loop and TUI and exposes six model-callable Relay tools:

- `relay_system_status`
- `relay_system_health`
- `relay_list_units` (bounded systemd service names and states)
- `relay_diagnose` (on-demand system, network, Bluetooth, hardware, process, journal, desktop,
  generation and managed-config context)
- `relay_plan_change` (creates an unapplied candidate plan)
- `relay_show_plan`

The local request router classifies requests as `INSPECT`, `DIAGNOSE`, `RELAY_CHANGE`, `BLOCKED` or
`DEVELOPMENT_REQUIRED`. It stops protected requests and requests that need unsupported backends
before contacting a model. It is a usability gate; the Core's typed intent parser and protected
resource checks remain the security boundary.

Apply, undo and recovery are direct user commands in the TUI. Apply requires a matching reviewed
plan, `/apply <id>`, and the exact phrase `APPLY <id>`. Undo and recovery first show a preview and
then require an exact target-bound phrase. Their requests go through the versioned Core protocol,
which rechecks the plan or pending recovery IDs. No model tool can invoke these actions. There is no
shell, filesystem-write, extension discovery, MCP, codemode or subagent tool.

Planning needs a configured flake and host. Exact review data is shown in the local TUI, while
model-facing tool results omit diffs, option values and store paths. Diagnostics are requested one
topic at a time. Journal results contain timestamps, systemd units, priority and message IDs; the
`MESSAGE` field and other free-form log values are excluded before they reach the model. Hardware
results omit serial numbers and mountpoints, and process results aggregate safe names into counts.
The 20-question corpus is in `src/diagnosis-corpus.ts` and is tested for topic routing and
read-only Core calls.

The shared mutation gate is exercised against the real Core and activation path in the isolated
NixOS VM test. A real Daily-Driver pilot and a live hosted-model session still require a prepared
host/provider and have not been claimed.

## Run

Install the standalone package with Nix:

```sh
nix run .#agent -- --init-config
nix run .#agent
```

The config is created at `$XDG_CONFIG_HOME/relay/agent.json` (default:
`~/.config/relay/agent.json`) with mode `0600`. Set `provider`, `model`, `flake` and `host` there,
or use `RELAY_AGENT_PROVIDER`, `RELAY_AGENT_MODEL`, `RELAY_AGENT_FLAKE` and `RELAY_AGENT_HOST`.
The selected Pi provider reads its credentials using Pi's provider API. Relay never loads
`~/.pi`, a project resource loader or a working-directory prompt file.

The agent starts Relay from `PATH`; `RELAY_CORE_PATH` can select an explicit `relay` executable.
No API provider is contacted by `--check`.

## Development

```sh
npm ci --ignore-scripts
npm run typecheck
npm test
npm start -- --check
```

Direct Pi package versions are exact in `package.json` and transitives are fixed by
`package-lock.json`. Review Pi package updates as executable dependencies.
