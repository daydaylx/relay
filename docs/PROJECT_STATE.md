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
- Das laufende NixOS-System wurde **nie** aktiviert oder verändert. Aktivierung lief nur in der VM.
  Das echte `/etc/nixos` (→ `/home/g/nixos-config`) wurde nur gelesen (`status --flake`).

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

## Genau drei nächste Schritte

1. **Einmal-Setup und Pilot** auf dem echten System: `relay init --flake /etc/nixos`, `./relay/managed.nix`
   in die Host-Module eintragen, `git add`, einmal selbst `nixos-rebuild switch`, dann eine harmlose Änderung
   (`add-package`) mit `plan`/`show`/`apply` und `undo` durchspielen. Vorher `relay status --flake /etc/nixos`
   prüfen (`managed_module` muss `in-sync` sein).
2. **Lizenz** wählen (nichts wurde angelegt) und optional taggen — nur auf ausdrückliche Anweisung des Nutzers.
3. **Provider live ausprobieren**: `relay ask --show-prompt` ansehen, dann mit einem lokalen Modell (Ollama) oder
   einem gehosteten Provider eine harmlose Anfrage stellen; die HTTP-Provider wurden nur gegen Fakes getestet.
   Danach ggf. die Hyprland-Integration erweitern (Steuerung des Kompositors erst mit eigenem Recovery-Entwurf).

