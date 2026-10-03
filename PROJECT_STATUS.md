# Relay – Project Status

## Current phase

**MVP implemented (T1–T4), plus the optional natural-language layer (T5) and read-only Hyprland integration (T6).**

Relay observes the system, plans a typed change in an isolated candidate, evaluates and builds it,
shows diff/risk/preview, applies it with `test` → health → `switch` (or `boot` for reboot-class
changes), and can undo it or recover from an interruption — source and runtime together, without AI.
All of it is dependency-free Rust (standard library only) behind one Nix adapter. A model may only *propose*
a typed intent (`relay ask`); the deterministic core validates, plans and a person confirms. Under
Hyprland, `apply` also watches monitors and the compositor and rolls back on damage.

What has *not* happened: a pilot on the author's daily-driver NixOS install. That install does not
yet import `relay/managed.nix`, so it has no `/etc/relay/managed.nix` and Relay (correctly) refuses
to plan against it until the one-time setup in the README is done. Real activation was only ever
exercised inside a NixOS VM.

## Evidence per target state

| Target | State | Evidence |
| --- | --- | --- |
| T0 Repository ready | done | README, AGENTS.md, ADR 0001–0008, security model, Cargo workspace, flake (package, dev shell, checks), CI. MIT licensed (`LICENSE`). Baseline committed and pushed to the public repo `daydaylx/relay` on the author's instruction (no tag). |
| T1 Read-only observer | done | `relay status` reports NixOS version, host, kernel, active/booted generation, running system path, config identity (`--flake`), `nixos-version --json` revision, failed units, desktop session, managed-module sync, pending change; `generations`, `health`, `index-*`, `search-*`. Run against the real host (read-only). |
| T2 Safe candidate builder | done | typed change + renderer + isolation + evaluation + build + closure diff + dry-activate + risk + protected rejection + drift detection. Real Nix run: `plan` of `add-package hello` against a real minimal NixOS flake (`path:` candidate, real `nix eval`/`nix build`/`diff-closures`), live source untouched. Failure paths against real Nix: unknown package and unknown option fail in evaluation, journaled, stderr kept private. |
| T3 Controlled activator | done (VM) | NixOS VM test `checks.x86_64-linux.activation`: real `sudo`, `nix-env`, `switch-to-configuration dry-activate/test/switch/boot`, real systemd health. A package change and an option change run end to end. |
| T4 Recoverable MVP | done (VM + simulator) | history, undo (source + runtime), recovery from crash at every step, pending-change detection, AI-independent. Pflichtszenarien: Bluetooth/option, VLC add, VLC remove, undo last change, failed change detected and rolled back. |
| T5 Natural language | done (core + fakes; not live) | `relay ask`: strict proposal schema, index cross-check, isolated candidate, typed confirmation (`--yes` refused), providers `command`, `openai` (also Ollama/local), `anthropic` via `curl` with the key only on stdin. Tests: hostile and malformed model output, AI unavailable, key never in arguments/Debug, fake-`curl` request shapes; a real `ask` → plan against real Nix with a command provider; VM subtest. **The hosted APIs (OpenAI, Anthropic) were never called live.** |
| T6 Desktop | done (read-only) | `relay desktop status/health` and `status.desktop` against a real Hyprland 0.55.4 session; fixed allow-list of read-only queries (a test forbids `dispatch`/`keyword`/`reload`/`exec`); desktop gate in `apply` proven in the simulator and against a fake socket. No compositor in the VM. |

## Test status

`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`
and `cargo build --release` pass (see `docs/DEVELOPMENT.md` for what the 160+ tests cover and the
mapping to the mandatory safety categories in `tests/README.md`). `nix build .#default` builds and
runs the tests in the sandbox. The VM check passed on 2026-10-03 on the development machine (KVM, about five minutes).

## Known limits and open decisions

- **Tags** are the author's decision. The repository is public under the MIT license.
- **Daily-driver pilot** is open: do the one-time setup, then pilot on a non-critical change first.
  `docs/planning/07_IMPLEMENTATION_ORDER.md` says recovery must work before a daily-driver pilot; it
  works in the VM, not yet on this hardware.
- **Reboot verification** after a real reboot is covered by the simulator, not by the VM (the test VM
  boots directly into its kernel and has no bootloader to select the new generation).
- **Bootloader installation** is part of `switch`/`boot`. A bootloader failure after a successful
  activation is reported by NixOS as a failed `switch`; Relay then rolls back (seen and handled in the
  VM). Bootloader *configuration* stays protected.
- **Health is a snapshot after an observation window** (default 5 s). A unit that fails later is not
  detected; use `--observe` and `--expect-active`.
- **Applied source only**: `plan` refuses while the live source evaluates to something other than the
  running system (unapplied edits anywhere in the configuration). Rebuild first, or revert the edits.
- **Evaluation identity**: configurations that embed `self.rev`/`self.lastModified` differ between the
  isolated candidate and a Git checkout and are refused before activation.
- **Compatibility**: only Nix 2.34.8 / NixOS 26.05 (nixpkgs `4feb8eb`) were exercised. `dry-activate`
  and `diff-closures` output is shown to people, never parsed for decisions.
- **Privilege prompts**: Relay escalates with `sudo -- <typed command>`; a password prompt needs a
  terminal. A hard kill of Relay can leave a `switch-to-configuration` child holding NixOS' lock;
  `relay recover` then reports an incomplete rollback and can be repeated.
- **Option defaults** in the options index remain `null` (forcing arbitrary defaults can fail).
- **AI providers** were exercised through fakes only; model quality (will it propose valid names?) is
  unmeasured. Text sent to a provider is exactly `relay ask --show-prompt`; `--explain` also sends the
  review (diff, store paths).
- **Desktop**: control of the compositor (dispatch, keyword, reload) and dotfile management stay out of
  scope; the desktop gate only exists while a session is reachable and is skipped visibly otherwise.
- **Scope**: Home Manager, secrets, partitioning, bootloader, `stateVersion`, major upgrades,
  subagents, MCP, plugins and remote management remain out of the MVP.

## Current decisions

- Product name: Relay
- Target: NixOS-first
- Runtime: standalone system tool, Pi dependency: none, AI: optional
- Language: Rust (std only), config backend: flake (later: `system.nix`)
- Managed write boundary: `relay/managed.nix`
- Candidate isolation by tree copy and strict canonical module (ADR 0005); typed privileges and
  evidence-based recovery (ADR 0006); the model proposes, the core decides (ADR 0007); Hyprland
  read-only and part of the health gate (ADR 0008)
