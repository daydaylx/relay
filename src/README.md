# src

Platzhalter aus der Planungsphase. Die Implementierung liegt im Cargo-Workspace unter
`crates/relay/src`; die Modulübersicht steht in `docs/DEVELOPMENT.md`.

Die frühen Entwurfsbereiche entsprechen heute diesen Modulen:

| Bereich | Modul(e) |
| --- | --- |
| `core/` | `change.rs`, `intent.rs`, `engine.rs`, `journal.rs`, `state.rs` |
| `system/` | `lib.rs` (Observer), `host.rs`, `health.rs` |
| `nix/` | `nix.rs`, `exec.rs` |
| `changes/` | `change.rs`, `source.rs`, `engine.rs` |
| `recovery/` | `engine.rs` (`roll_back`, `undo`, `recover`), `journal.rs` |
| `knowledge/` | `index.rs` |
| `ai/` | `ai.rs` (Provider, Prompt, Validierung), `intent.rs`, CLI `ask.rs` |
| `ui/` | CLI in `crates/relay/src/main.rs`, `desktop.rs`; Hyprland-Zugriff in `hypr.rs` |
