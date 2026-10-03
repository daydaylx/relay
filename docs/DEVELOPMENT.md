# Development

## Rust workspace

Relay is a dependency-free Rust workspace (standard library only). It requires Rust/Cargo 1.85 or
newer (edition 2024). On NixOS use the flake's dev shell:

```sh
nix develop            # cargo, rustc, clippy, rustfmt
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
```

The GitHub Actions workflow runs the same four checks. No system packages or NixOS activation are
used by CI.

## Nix checks

```sh
nix build .#default                       # package; runs the Rust tests in the sandbox
nix build .#checks.x86_64-linux.activation -L   # real activation/recovery in a NixOS VM
```

New files must be tracked by Git (`git add`) before a flake in a Git checkout can see them.

## Module map (`crates/relay/src`)

| Module | Responsibility |
| --- | --- |
| `lib.rs` | observer: `system_summary`, `generations`, live status sources |
| `nix.rs` | **the only place that builds Nix/NixOS command lines**; typed privileged actions |
| `exec.rs` | `Runner` seam (real processes, `sudo` escalation, `NIXOS_NO_CHECK` scrubbing) |
| `change.rs` | typed changes, protected-resource policy, risk classes, strict `managed.nix` renderer/parser |
| `intent.rs` | strict JSON intent schema (what a script or optional AI provider may submit) |
| `source.rs` | source tree listing (what Nix sees), content identity, isolated candidate copies |
| `host.rs` | read-only view of system links, switch inhibitors, reboot-relevant components |
| `health.rs` | systemd health snapshot/verification (structured JSON only) |
| `journal.rs` | append-only, write-ahead state machine (no configuration values) |
| `state.rs` | state directory, plan records, single-writer lock |
| `engine.rs` | plan, preview, apply, undo, recover, explain |
| `ai.rs` | optional natural-language layer: prompt, strict answer extraction, index cross-check, providers (`command`, `openai`, `anthropic`) |
| `hypr.rs` | Hyprland read-only IPC (fixed allow-list), desktop summary and desktop health |
| `index.rs`, `json.rs`, `sha256.rs`, `fsutil.rs` | options/package index, JSON parser, SHA-256, atomic file writes |
| `main.rs`, `ask.rs`, `desktop.rs` | CLI (`ask` and `desktop` are separate binary modules) |

## Commands

Observer (read-only): `status`, `generations`, `health`, `index-options`, `index-packages`,
`search-option`, `search-package`.

```sh
relay status --flake /etc/nixos     # adds config identity, managed-module sync, pending change
relay index-options --flake /etc/nixos --host myhost --output ~/.cache/relay/options.json
relay search-option bluetooth --index ~/.cache/relay/options.json --flake /etc/nixos --host myhost
```

Offline checks (no Nix, no files): `check <change>` validates a typed change and prints its name-based risk;
`render-managed <change>` prints the managed module for that change.

Change workflow: `init`, `plan`, `preview`, `show`, `apply`, `undo`, `recover`, `history`, `discard`.
Optional front ends: `ask` (natural language, see below) and `desktop status|health` (Hyprland, read-only).
Machine-readable results go to stdout as JSON; progress goes to stderr. Exit codes: `0` success
(including `reboot-pending`), `1` refused/failed before anything was changed, `2` the change was
rolled back (or `recover` rolled something back).

A change is one of `add-package NAME`, `remove-package NAME`,
`set-option OPTION bool|integer|string|string-list VALUE`, or a JSON intent
(`plan --intent FILE|-`, schema in `intent.rs`). State lives in `$RELAY_STATE_DIR`, else
`$XDG_STATE_HOME/relay`, else `~/.local/state/relay`, and must not be inside the flake directory.

## Natural language and desktop

`relay ask "<request>" --host HOST [--flake PATH] --provider command|openai|anthropic`: the model only
proposes a typed intent; Relay validates it, cross-checks it against `--options-index` and
`--packages-index` (optional, from `index-*`), plans it in an isolated candidate and asks a person
before anything is applied (`--yes` is refused). `--show-prompt` prints exactly what a provider would
receive and sends nothing. Configuration by flag or environment: `RELAY_AI_PROVIDER`,
`RELAY_AI_MODEL`, `RELAY_AI_BASE_URL`, `RELAY_AI_COMMAND`, and the key only via `RELAY_AI_API_KEY` or
`RELAY_AI_API_KEY_FILE`. A `command` provider is any program that reads `{"system":…,"user":…}` on
stdin and prints the model's answer; `openai` also talks to local servers such as Ollama
(`--base-url http://localhost:11434/v1`). The HTTP providers use the system `curl`.

`relay desktop status|health` read Hyprland through its own socket (monitors, workspaces, counts,
configuration errors; never window titles). With a running session, `apply` also checks the
desktop and rolls back on lost monitors, a dead compositor or new configuration errors
(`--no-desktop-check` disables it).

## Test architecture

Tests demonstrate safety properties, not coverage.

1. **Unit tests** (`cargo test`): policy, renderer/parser round trips, intent schema, SHA-256
   vectors, journal state machine, health logic, adapter command lines.
2. **Simulator tests** (`engine/tests.rs`): the whole engine against a simulated NixOS machine
   (fixture `run/current-system`, system profile, booted system, `/etc/relay/managed.nix`,
   scripted systemd, privileged activation that also enforces NixOS' switch-inhibitor rule).
   Crashes are simulated by panicking inside the simulated machine at a chosen call and then
   starting a fresh engine on the same state directory. Categories: source drift, candidate
   isolation, failed evaluation/build, switch inhibitor, failed test activation, failed health
   check, failed switch, reboot-required change and post-boot verification, protected-resource
   rejection, source rollback, runtime rollback, crash/restart at every step, recovery refusing to
   guess, malformed model intent, AI unavailable (the core never needs a provider).
3. **Real Nix runs** (manual): evaluation, build and closure diff of a real NixOS flake through the
   real adapter. Recipe: write a small flake whose host imports `./relay/managed.nix` (a minimal
   `boot.loader.grub.enable = false` config evaluates in seconds and needs almost no downloads),
   run `relay init --flake DIR`, build the host toplevel once, create a fixture root `R` with
   `R/nix/store -> /nix/store`, `R/run/{current,booted}-system -> <toplevel>`,
   `R/nix/var/nix/profiles/system{,-1-link}` and `R/etc/relay/managed.nix`, then
   `relay plan --flake DIR --host HOST --root R --state-dir S add-package hello`. Nothing is
   activated; do not use `apply` with a fixture root.
4. **AI and desktop tests** (`ai/tests.rs`, `hypr.rs`, `ask.rs`, engine tests): providers against a fake `curl`
   and real child processes (stdin, timeouts), strict extraction, index cross-check, hostile model
   output, "AI unavailable"; the Hyprland client against a fake Unix socket server; desktop gate in the
   simulator. `relay desktop status` was also run against a real Hyprland 0.55.4 session.
5. **NixOS VM test** (`nix/tests/activation.nix`): real sudo, `nix-env`, `switch-to-configuration`
   (`dry-activate`, `test`, `switch`, `boot` with NixOS' own inhibitor check), real systemd health
   and real source restore/undo/recovery. Only evaluation/build of the candidate is stubbed
   (the VM has no network): candidates are NixOS specialisations selected by the content of the
   candidate's `relay/managed.nix`.

Never test activation on a daily-driver system; use the VM check.

## Environment status

Developed with Rust/Cargo 1.95 and Nix 2.34.8 on NixOS 26.05 (nixpkgs
`4feb8eb8bf30f323a8a5d285f14ee51d6a7197b1`). That is the only validated combination; no wider
compatibility range is claimed. Relay passes `--extra-experimental-features 'nix-command flakes'`
itself and never writes a lock file (`--no-write-lock-file`).

Index generation (`index-options`, `index-packages`) evaluates the host's locked flake with
`--impure` and environment variables; `default` is intentionally `null` because forcing arbitrary
option defaults can fail while evaluating some host options.

## Limits to know about

- Evaluation of the isolated candidate and of the live source must give the same derivation.
  Configurations that embed `self.rev` or `self.lastModified` differ between a `path:` candidate and
  a Git checkout and are refused before activation.
- `dry-activate` and `diff-closures` have no structured output; their text is shown to people and
  stored, never used for decisions. Decisions use files and links (`switch-inhibitors`, `kernel`,
  `initrd`, `kernel-modules`, `systemd`, `run/current-system`, the system profile).
- Health is judged relative to a baseline snapshot after an observation window (default 5 s,
  `--observe SECONDS`); a unit that fails later is not detected. `--expect-active UNIT` makes the
  expected effect explicit.
- A hard kill of Relay can leave a `switch-to-configuration` child running that holds NixOS' lock;
  `relay recover` then reports an incomplete rollback and can be repeated.
