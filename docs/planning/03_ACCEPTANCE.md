# Abnahmekriterien

Belege: **U** = Unit-Test, **S** = Simulator-Test (`engine/tests.rs`), **N** = echter Nix-Lauf,
**V** = NixOS-VM-Test (`nix/tests/activation.nix`). Namen der Tests: `tests/README.md`.

- [x] kein Pi zur Laufzeit — keine Abhängigkeiten, kein Netzwerk-Code (`Cargo.lock` enthält nur `relay`)
- [x] Relay funktioniert ohne AI — der gesamte Workflow läuft ohne Provider (S, V)
- [x] NixOS-Version/Host/Generation/Store Path bekannt — `relay status` (N, gegen den echten Host)
- [x] lokaler Options-Index — `index-options` / `search-option`, Identitätsprüfung (U, N)
- [x] eigener Managed-Schreibbereich — `relay/managed.nix`, Symlinks werden nicht gefolgt (U, S)
- [x] deterministischer Renderer — Parser/Renderer-Round-Trip, handgeänderte Dateien abgelehnt (U)
- [x] Drift Detection — Quelle, Runtime, Profil, Kandidat, Runtime-Stempel (S)
- [x] Candidate bleibt bis Freigabe isoliert — Kopie unter `<state>/candidates`, Live-Quelle unverändert (S, N)
- [x] Evaluation + Build + Store Path — `path:`-Kandidat, echtes `nix eval`/`nix build` (N)
- [x] Closure Diff — `diff-closures` (Anzeige) plus Evidenz aus Systemdateien (U, N)
- [x] dry-activate Preview — `plan --preview`/`preview`, Gate in `apply` (S, V)
- [x] Switch Inhibitors respektiert — NixOS-Regel nachgebildet und gegen NixOS selbst geprüft; `NIXOS_NO_CHECK` entfernt (U, S, V)
- [x] system.stateVersion geschützt — im Code, auch über Intents (U, S)
- [x] `test` korrekt behandelt — Rückaktivierung bei Fehler, kein Vertrauen auf Selbstheilung (S, V)
- [x] Health Check — baseline-relativ, Beobachtungsfenster, erwartete Units (U, S, V)
- [x] `switch` erst nach erfolgreichem Check — Journal erzwingt Reihenfolge (U, S, V)
- [x] reboot-required Workflow — Boot-Pfad, `recover` nach Reboot, Abbruch (S; V bis `reboot-pending` und Abbruch). **Der Reboot selbst wurde nur im Simulator ausgeführt.**
- [x] Source + Runtime Recovery — Rollback, `undo`, `recover` nach Crash (S, V)
- [x] Recovery ohne AI (S)
- [x] AI nur typed intents — striktes Schema, Schutzliste, Indexabgleich, Bestätigung durch Menschen; Provider (`command`, `openai`, `anthropic`) sind reine Adapter (U, S, V; Live-APIs nicht aufgerufen)
- [x] keine generische Shell — privilegiert sind nur zwei typisierte Aufrufe (U, V)
- [x] Desktop-Health (T6) — Monitore/Kompositor/Konfigurationsfehler im Apply-Kreislauf; Kompositor nur lesend (U, S; echte Hyprland-Sitzung manuell)

## MVP Ende-zu-Ende

```text
Option Change:
Intent → Option → Candidate → Build → Test → Health → Switch → Undo      (S, V)

Package Change:
Intent → Package → Candidate → Build → Diff → Test → Switch → Undo      (S, V; Build/Diff real in N)

Diagnosis:
Question → System State → Config/Runtime → Explanation                  (status, health, show/explain)
```

Die Evaluation/Build-Stufe ist in der VM durch einen Shim ersetzt (kein Netzwerk, kein Build eines
NixOS-Systems aus Quellen möglich) und wird deshalb getrennt gegen echtes Nix belegt (N). Die
Kombination „echte Evaluation und echte Aktivierung auf derselben Maschine“ wurde nicht erprobt.
Aktivierung und Recovery werden in der NixOS-VM getestet; der Daily Driver ist keine Testumgebung.
Ein späterer Live-Einsatz erfordert eine separate Betriebsentscheidung (siehe `PROJECT_STATUS.md`).
