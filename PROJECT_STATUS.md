# Relay – Project Status

## Current phase

**MVP implemented (T1–T4), plus the optional natural-language layer (T5), read-only Hyprland
integration (T6), Pi RPC task runtime and first SystemContext (T7 / migration phases 1–2).**

The optional runtime in `agent/` uses exact project dependencies and starts one isolated Pi RPC session
per task. Relay tools are exposed through a task-scoped extension and private Unix socket. A verified
SystemContext is added to the Pi system prompt and persisted in the task journal. The runtime has
persisted Task state, safe bounded reads, plan/review, locally confirmed Core mutations and structured
verification for limited goals. It never loads, reads, copies or inspects the user's personal Pi setup.
Rust Core remains independently usable and owns all system mutations. A live provider session,
usability pilot and real-system deployment remain open. See [the runtime architecture](docs/architecture/08_AGENT_TASK_RUNTIME.md)
and [the audit](docs/audits/SYSTEM_AGENT_V1_RESULT.md).

The expanded target also includes an initial SystemContext, a conservative metadata-only Ownership
Resolver and the first OBSERVE Execution Gateway: structured diagnostic commands run through
Bubblewrap with restricted Nix closures and resource limits. Full operation evidence, adversarial
escape coverage, automatic source-of-truth mapping, file transactions, full Desired State, versioned
Knowledge, MCP/Web and a Privilege Broker remain open. No host-mutating Bash, MCP, Web fetch or User
Config write is enabled.
See the staged gates in [`docs/planning/09_CONTROL_CENTER_MIGRATION.md`](docs/planning/09_CONTROL_CENTER_MIGRATION.md).

Relay observes the system, plans a typed change in an isolated candidate, evaluates and builds it,
shows diff/risk/preview, applies it with `test` → health → `switch` (or `boot` for reboot-class
changes), and can undo it or recover from an interruption — source and runtime together, without AI.
All of it is dependency-free Rust (standard library only) behind one Nix adapter. A model may only *propose*
a typed intent (`relay ask`); the deterministic core validates, plans and a person confirms. Under
Hyprland, `apply` also watches monitors and the compositor and rolls back on damage.

What has *not* happened: activation on the author's daily-driver NixOS install. A read-only run on
2026-10-03 confirmed that `/etc/nixos` publishes the managed module (`managed_module: in-sync`),
but Relay refused a plan because the evaluated configuration source differs from the running
system. Comparing the current and evaluated `/etc` trees found one real difference: the source
`desktop.nix` starts `greetd` with `tuigreet --battery --asterisks`, while the running generation
does not. The package closure is unchanged. Separately, the user's Hyprbars and Quickshell working
tree edits are live through direct home-directory symlinks; the user chose to keep them. Relay has
not switched the system generation. Real Relay activation was only ever exercised inside a NixOS VM.

## Evidence per target state

| Target | State | Evidence |
| --- | --- | --- |
| T0 Repository ready | done | README, AGENTS.md, ADR 0001–0009, security model, Cargo workspace, flake (package, dev shell, checks), CI. MIT licensed (`LICENSE`). Baseline committed and pushed to the public repo `daydaylx/relay` on the author's instruction (no tag). |
| T1 Read-only observer | done | `relay status` reports NixOS version, host, kernel, active/booted generation, running system path, config identity (`--flake`), `nixos-version --json` revision, failed units, desktop session, managed-module sync, pending change; `generations`, `health`, `index-*`, `search-*`. Run against the real host (read-only). |
| T2 Safe candidate builder | done | typed change + renderer + isolation + evaluation + build + closure diff + dry-activate + risk + protected rejection + drift detection. Real Nix run: `plan` of `add-package hello` against a real minimal NixOS flake (`path:` candidate, real `nix eval`/`nix build`/`diff-closures`), live source untouched. Failure paths against real Nix: unknown package and unknown option fail in evaluation, journaled, stderr kept private. |
| T3 Controlled activator | done (VM) | NixOS VM test `checks.x86_64-linux.activation`: real `sudo`, `nix-env`, `switch-to-configuration dry-activate/test/switch/boot`, real systemd health. A package change and an option change run end to end. |
| T4 Recoverable MVP | done (VM + simulator) | history, undo (source + runtime), recovery from crash at every step, pending-change detection, AI-independent. Pflichtszenarien: Bluetooth/option, VLC add, VLC remove, undo last change, failed change detected and rolled back. |
| T5 Natural language | done (core + fakes; not live) | `relay ask`: strict proposal schema, index cross-check, isolated candidate, typed confirmation (`--yes` refused), providers `command`, `openai` (also Ollama/local), `anthropic` via `curl` with the key only on stdin. Tests: hostile and malformed model output, AI unavailable, key never in arguments/Debug, fake-`curl` request shapes; a real `ask` → plan against real Nix with a command provider; VM subtest. **The hosted APIs (OpenAI, Anthropic) were never called live.** |
| T6 Desktop | done (read-only) | `relay desktop status/health` and `status.desktop` against a real Hyprland 0.55.4 session; fixed allow-list of read-only queries (a test forbids `dispatch`/`keyword`/`reload`/`exec`); desktop gate in `apply` proven in the simulator and against a fake socket. No compositor in the VM. |
| T7 Pi task runtime | RPC runtime and tool bridge implemented; bounded-goal pilot open | Exact-pinned Pi RPC process per task; only Relay task extension; private Unix socket; tool-call limits/journal; SystemContext in prompt and Task journal. Typed plan/show/discard/apply/undo/recover still require exact local confirmation. Supported checks: Bluetooth readiness, named service activity, system health and package availability. Automated Pi startup/extension/bridge and fake-Core workflow tests pass; live provider/TUI usability and real-host activation remain open. |

## Test status

`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`
and `nix build .#default` pass (178 library and 13 CLI tests; Agent typecheck and 35 tests pass with
one existing opt-in test skipped). The explicit host OBSERVE smoke test also passes against real
Bubblewrap/user-systemd. See `docs/DEVELOPMENT.md` for the safety-test mapping. The VM check passed on
2026-10-03 on the development machine (KVM, about five minutes).

## Known limits and open decisions

- **Tags** are the author's decision. The repository is public under the MIT license.
- **Live activation** is not validated on this machine. The development policy is to test activation
  and recovery in the NixOS VM, not on the daily-driver system. A real-system change needs a separate
  reviewed deployment decision after the source drift is resolved.
- **Agent provider/TUI workflow** uses fake Pi model streams and a fake Core bridge in TypeScript
  tests. The isolated NixOS VM checks the real Core mutation/recovery boundary but does not start the
  Pi TUI or contact a hosted model provider.
- **Reboot verification** after a real reboot is covered by the simulator, not by the VM (the test VM
  boots directly into its kernel and has no bootloader to select the new generation).
- **Bootloader installation** is part of `switch`/`boot`. A bootloader failure after a successful
  activation is reported by NixOS as a failed `switch`; Relay then rolls back (seen and handled in the
  VM). Bootloader *configuration* stays protected.
- **Health is a snapshot after an observation window** (default 5 s). A unit that fails later is not
  detected; use `--observe` and `--expect-active`.
- **Applied source only**: `plan` refuses while the live source evaluates to something other than
  the running system. Here the pending change is the `greetd` command in `desktop.nix`; apply it
  through the normal NixOS workflow before planning. The guard is working as designed.
- **Evaluation identity**: configurations that embed `self.rev`/`self.lastModified` differ between the
  isolated candidate and a Git checkout and are refused before activation.
- **Compatibility**: only Nix 2.34.8 / NixOS 26.05 (nixpkgs `4feb8eb`) were exercised. `dry-activate`
  and `diff-closures` output is shown to people, never parsed for decisions.
- **Privilege prompts**: Relay escalates with `sudo -- <typed command>`; a password prompt needs a
  terminal. A hard kill of Relay can leave a `switch-to-configuration` child holding NixOS' lock;
  `relay recover` then reports an incomplete rollback and can be repeated.
- **Option defaults** in the options index remain `null` (forcing arbitrary defaults can fail).
- **AI providers** in both paths were exercised through fakes only; model quality (will it propose valid names?) is
  unmeasured. Text sent to a provider is exactly `relay ask --show-prompt`; `--explain` also sends the
  review (diff, store paths).
- **Desktop**: control of the compositor (dispatch, keyword, reload) and dotfile management stay out of
  scope; the desktop gate only exists while a session is reachable and is skipped visibly otherwise.
- **Scope**: Home Manager, secrets, partitioning, bootloader, `stateVersion`, major upgrades,
  subagents, MCP, plugins and remote management remain out of the MVP.

## Current decisions

- Product name: Relay
- Target: NixOS-first
- Runtime: standalone Rust Core plus optional embedded Pi Agent runtime, AI: optional
- Language: Rust (std only), config backend: flake (later: `system.nix`)
- Managed write boundary: `relay/managed.nix`
- Candidate isolation by tree copy and strict canonical module (ADR 0005); typed privileges and
  evidence-based recovery (ADR 0006); the model proposes, the core decides (ADR 0007); Hyprland
  read-only and part of the health gate (ADR 0008)
