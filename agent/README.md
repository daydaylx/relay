# Relay Agent Runtime

Relay is an independent NixOS system control tool. This optional TypeScript layer embeds selected
Pi runtime components for its agent loop, provider abstraction, streaming and terminal UI. Pi
interprets user goals and orchestrates typed Relay tools; the Rust Relay Core remains the authority
for plans, validation, confirmation, activation, journaling and recovery.

## Separation from a personal Pi setup

The runtime uses only exact-version Pi dependencies declared by this repository and its lockfile,
including the Pi coding-agent CLI used as Relay's normal task runtime over RPC.
It does not load, read, copy or inspect `~/.pi`, Pi profiles, local prompts, extensions, sessions,
settings or credentials. Relay configuration is stored under `$XDG_CONFIG_HOME/relay/agent.json`
(default `~/.config/relay/agent.json`). Provider selection comes from that Relay configuration or
the documented `RELAY_AGENT_*` variables. Each task starts its own Pi RPC session and Relay-owned
extension. The runtime redirects `HOME`, XDG paths, configuration and sessions into Relay-owned
directories, strips inherited `PI_*` variables, disables Pi's discovered extensions, built-in tools,
context files, skills, templates and themes, and explicitly loads only the generated Relay tool
extension. The extension reaches the host through a private task-scoped Unix socket. Relay bounds
tool names, payload sizes and calls, records tool events, and closes the socket when the task ends.
Only the selected provider's API-key environment variable is forwarded.

Every task receives a locally assembled verified SystemContext in Pi's system prompt. A redacted
copy is stored in the task journal and re-injected on resume. Context includes NixOS identity,
configuration revisions, generations, hardware and desktop summaries, capabilities, and the
currently known ownership boundary. `relay_resolve_ownership` classifies requested paths from
filesystem metadata only; it does not inspect file contents. Unknown Home Manager/user configuration
remains read-only, and the resolver does not yet prove source ownership through flake evaluation.
`relay context` displays this same verified snapshot for the user; `relay context --json` emits its
structured form. Both commands use the existing read-only Relay Core adapters.

## Runtime and task flow

`relay` starts the interactive assistant from the combined default package. `nix run .#agent` is an
equivalent optional entry point. Existing deterministic commands (`relay status`, `plan`, `apply`,
`undo`, `recover`, and others) remain available; `relay ask` remains the one-shot typed-intent path.

Each interactive goal is a separate persisted Task under `$XDG_STATE_HOME/relay/tasks/` (default
`~/.local/state/relay/tasks/`). Task events are append-only and mode `0600` inside a mode `0700`
directory. The transcript and raw logs are not persisted. `/tasks` lists tasks and `/resume TASK_ID`
continues a recoverable task. A stale confirmation expires on restart; an interrupted Core apply
requires explicit Core recovery. Completion requires a supported structured Relay verification.

The Agent uses one sequential Pi RPC loop per task, with a tool-call limit, bounded event and
context size, cancellation and failed-tool handling. Read-only actions need no confirmation. Apply,
Undo and Recovery pause the current tool call, show the concrete Relay Core review locally and wait
for exact direct user input tied to the task and review hash. Natural-language approval and model
claims cannot authorize a mutation.

## Relay tools

- Observe and diagnose: system status/health, bounded service lists, targeted service/system/network/
  Bluetooth/hardware/process/journal/desktop/generation/package information.
- Search: NixOS options and packages without returning option values or defaults.
- Safe configuration reads: small regular `.nix`, `.md`, `.toml` or `flake.lock` files under the
  configured flake only; hidden paths, symlinks, likely secrets and personal Pi data are rejected.
- Change workflow: typed plan, review, discard, locally confirmed apply, locally confirmed undo or
  recovery, and structured goal verification.

There is no general shell, `sudo`, arbitrary process execution, arbitrary file write, or generic
filesystem-read tool. Relay Core still blocks protected resources and refuses source drift,
inhibitors and unsupported changes in code. The Agent is never the security boundary.

Goal verification currently supports Bluetooth readiness, an explicitly named active service,
overall system health, and package/executable availability. This does not yet verify or configure
goals such as MIME defaults, monitor layouts, workspace behavior or generation cleanup. Such tasks
must remain incomplete instead of treating a model statement as proof.

## Run and configure

```sh
nix run .#agent -- --init-config
nix run .#agent -- --check
nix run .#agent -- --pi-rpc-check
nix run .#agent
```

The package supplies Node.js and the locked Pi libraries. `--check` lists the configured tools and
does not contact a model provider. `--pi-rpc-check` starts a headless Pi RPC session in a Relay-only
profile, requests its structured state, and exits without sending a prompt. Set `provider`, `model`, `flake` and `host` in Relay's own config,
or use `RELAY_AGENT_PROVIDER`, `RELAY_AGENT_MODEL`, `RELAY_AGENT_FLAKE` and `RELAY_AGENT_HOST`.
Provider keys must be provided through that provider's documented environment/API mechanism; no Pi
profile is read. The Agent warns that task goals and tool data may be sent to the configured model.

The Rust Core can also be installed by itself with `nix profile install .#core`.

## Development and checks

```sh
npm ci --ignore-scripts
npm run typecheck
npm test
```

The Pi package versions are exact in `package.json` and transitives are fixed by
`package-lock.json`. Agent tests use fake providers and fake Relay Core bridges; a live provider
session and a supervised usability pilot remain open.
