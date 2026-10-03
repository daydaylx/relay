# Relay – Project State

Checkpoint für Fortsetzungen. Maßgeblich bleiben `PROJECT_STATUS.md` (Belege, Grenzen) und
`docs/planning/04_TARGET_STATES.md` (Ziele).

## Aktuelle Phase

MVP (T1–T4) sowie T5 (optionale KI-Schicht, `relay ask`) und T6 (Hyprland read-only, Desktop-Gate in `apply`) sind
implementiert und getestet.
Der Stand wurde auf Anweisung des Nutzers als Baseline committet (siehe `git log`).

## Arbeitsumgebung

- Repository: `/home/g/Projekte/Relay_NixOS_Project`; die Sitzung lief auf NixOS 26.05
  (`nixos`, nixpkgs `4feb8eb`). `cargo` ist nicht im PATH: `nix develop` bzw.
  `nix shell nixpkgs#cargo nixpkgs#rustc nixpkgs#clippy nixpkgs#rustfmt nixpkgs#gcc`.
- Prüfungen: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, `cargo build --release`; `nix build .#default`;
  `nix build .#checks.x86_64-linux.activation -L` (NixOS-VM, braucht `/dev/kvm`, ca. 5 Minuten).
- Neue Dateien (`nix/`, `adr/0005…`, `adr/0006…`, `docs/…`) müssen für Flake-Builds in einem
  Git-Checkout mit `git add` bekannt gemacht werden. Zum Testen ohne den Index anzufassen wurde
  eine Kopie ohne `target/` und `.git` mit `path:` gebaut.
- Das laufende NixOS-System wurde **nie** durch Relay aktiviert oder verändert. Aktivierung lief
  nur in der VM.
- Am 2026-10-03 liefen `status`, `health`, `generations` und `desktop status` erfolgreich gegen
  den Host. `managed_module` war `in-sync`; `plan add-package hello` brach wegen Source-Drift ab.
- `/home/g/nixos-config` hat lokale Änderungen in `README.md`, `hypr/plugins.conf` und zwei
  Quickshell-Dateien; der Nutzer entschied, sie zu behalten. Diese Dateien sind live über
  Home-Verzeichnis-Symlinks eingebunden. Relay-Planung scheitert wegen eines separaten, echten
  Systemunterschieds: `desktop.nix` enthält für `greetd` `tuigreet --battery --asterisks`, die
  laufende Generation enthält diese Argumente nicht. Der Paketbestand ist gleich. Hyprbars war
  geladen, Quickshell meldete `minimizeWindow`, und `nixos-rebuild dry-build` gelang. Relay hat
  die NixOS-Generation nicht gewechselt.
- Die drei neuen Rust-Module und das optionale Agent-Paket sind im Git-Index erfasst, damit Flake-
  Builds sie sehen. `nix build .#default`, `nix build .#checks.x86_64-linux.activation -L` und
  `nix run .#agent -- --check` liefen am 2026-10-03 erfolgreich. Nichts wurde committet.

## Was gebaut wurde

Module in `crates/relay/src`: `nix.rs` (einziger Command-Builder, typisierte privilegierte
Aktionen), `exec.rs`, `change.rs`, `intent.rs`, `source.rs`, `host.rs`, `health.rs`, `journal.rs`,
`state.rs`, `engine.rs` (plan/preview/apply/undo/recover/explain), `ai.rs` (optionale KI-Schicht),
`hypr.rs` (Hyprland read-only), dazu `sha256.rs`, `json.rs`, `fsutil.rs`; CLI-Module `ask.rs`, `desktop.rs`. CLI: `relay init|plan|preview|show|apply|undo|recover|history|discard`, Observer wie
zuvor. Entscheidungen: ADR 0005 (Kandidat = Kopie, kanonisches Managed-Modul, Laufzeit-Stempel) und
ADR 0006 (typisierte Privilegien, Write-ahead-Journal, evidenzbasierte Recovery), ADR 0007 (KI: Modell
schlägt vor, Kern entscheidet), ADR 0008 (Desktop read-only, Teil der Health-Prüfung).

Wichtige Details, die man nicht aus dem Code raten sollte:

- Das Managed-Modul veröffentlicht sich als `/etc/relay/managed.nix`; ohne diesen Stempel im
  laufenden System (oder bei Abweichung zur Quelle) plant Relay nicht.
- In Git-Checkouts sieht Nix nur verfolgte Dateien; `relay/managed.nix` muss mit `git add` bekannt
  sein. Relay ruft Git nur lesend auf (`git ls-files`).
- Die VM hat kein Netzwerk: dort ersetzt ein Shim nur `nix eval`/`nix build` des Kandidaten; Kandidaten
  sind NixOS-Specialisations, ausgewählt über den Hash von `relay/managed.nix`.
- `systemctl is-system-running`, `list-units --output=json` und `nixos-version --json` sind die
  einzigen Laufzeitquellen für Health/Status.

## Nächste Schritte

1. Den ausstehenden `greetd`-Unterschied bewusst über NixOS aktivieren. Ein `switch` kann den
   Display-Manager neu starten und die aktuelle grafische Sitzung beenden; dafür ist ausdrückliche
   Zustimmung nötig.
2. Danach Relay-Status und `plan add-package hello` erneut prüfen. Relay hat bis dahin korrekt
   abgebrochen.
3. Optional: den Agent-TUI-Workflow mit einem Testprovider durchspielen; Live-Provider und
   Usability-Pilot sind gesonderte optionale Prüfungen. Release-Tags erfolgen nur auf ausdrückliche
   Anweisung.
