# Relay System Agent V1 – Ergebnis

Stand: 2026-10-03. Diese Umsetzung ergänzt den Rust-Core um einen optionalen, lokalen
Original-Pi-Frontend-Prototyp. Sie erfüllt noch nicht alle Abnahmekriterien aus dem Master-Auftrag;
offene Gates sind unten ausdrücklich markiert.

## Architektur

```text
relay-agent (TypeScript, Original-Pi)
  ├─ eigenes Profil und Session nur im Speicher
  ├─ fester Tool-Allowlist und lokaler Anfrage-Router
  ├─ direkte Nutzereingaben für Apply / Undo / Recover
  └─ versioniertes JSON über stdin/stdout
       ↓
relay (Rust)
  ├─ strukturierte Status-, Health- und Dienstabfrage
  ├─ typisierte Plan-/Show-Aktionen
  └─ Apply / Undo / Recover samt bestehender Safety Gates
```

Der Core läuft ohne Node, Pi oder Modellprovider weiter. Das Agentenprofil liegt unter
`$XDG_CONFIG_HOME/relay/agent.json` (Standard `~/.config/relay/agent.json`, Modus `0600`). Der
Agent lädt weder `~/.pi` noch Projektprompts, Extensions, MCP-Server oder ein Coding-Agent-Profil.

## Phasenplan und Gate-Status

| Phase | Ergebnis | Gate |
| --- | --- | --- |
| 0 – Baseline | Ausgangscommit und 166 Rust-Tests festgehalten; Branch `work/system-agent-pi`. | bestanden |
| 1 – Pi-Spike | Original-Pi-Agent/TUI, eigenes Profil, isolierter Tool-Allowlist und CLI-Start ohne Pi-Profil. | Code/Nix-CLI-Check bestanden; interaktive Modell-Session noch nicht live geprüft |
| 2 – Core-Protokoll | JSON-Zeilen v1 mit festen Aktionen, Versions-/Feldprüfung und Fehlercodes. | Unit- und lokaler Status-/Dienst-Smoke bestanden |
| 3 – Read-only-Agent | Status, Health, Dienste, Netzwerk, Bluetooth, Hardware, Prozesse, Journalmetadaten, Desktop und Generationen. | Code-, Protokoll- und Core-Tests bestanden; 20 Fragen prüfen erwartete Topic-Routen und Read-only-Aktionen |
| 4 – Router | Deterministische lokale Kategorien plus Regressionstests für Standardrouten, geschützte und nicht unterstützte Anliegen. | teilweise; Heuristik ist kein formaler Intent-Klassifikator |
| 5 – Mutationsbridge | Plan/Show im Modelltoolset; Apply/Undo/Recover nur als direkte, zielgebundene Aktionen durch Relay Core. | Core-Bridge und geteiltes Bestätigungsgate für Plan, Show, Apply, Undo und Recovery im NixOS-VM-Test am 2026-10-03 erfolgreich; Pi-TUI/Provider nicht gestartet |
| 6 – UX | Risiko-/Previewtexte sowie explizite Bestätigungen und Recovery-Preview im TUI. | implementiert; kein begleiteter Usability-Pilot |
| 7 – User-Space-Backends | Home Manager/Hyprland-Schreibbackends. | nicht begonnen; bleibt außerhalb V1 |
| 8 – Daily Driver | harmlose reale Änderungsfolge. | offen; keine Live-Systemänderung durchgeführt |

## Pi-Komponenten

- Verwendet: `@earendil-works/pi-agent-core@1.0.0`, `@earendil-works/pi-ai@1.0.0`,
  `@earendil-works/pi-tui@1.0.0` sowie `typebox@1.3.27`.
- Nicht verwendet: `@earendil-works/pi-coding-agent`; dessen Resource-/Extension-Lifecycle ist
  für die begrenzte Systemoberfläche unnötig. Kein Fork und kein `daydaylx/pi`-Runtime-Code.
- Details und Gründe: [ADR 0009](../../adr/0009-pi-core-integration.md).

## Core-Protokoll und Werkzeuge

`relay protocol --stdio` verwendet JSON-Zeilen mit `schema_version: 1`, Request-ID, festen
Aktionen und stabilen Fehlercodes. Requests werden streng validiert, in einen festen Relay-CLI-
Argumentvektor übersetzt und niemals über eine Shell gestartet. Request- und Response-Größen sind
begrenzt.

Modellwerkzeuge:

- `relay_system_status`: gefilterte Systemübersicht ohne Store-Pfade/Konfigurationsrevision.
- `relay_system_health`: laufender Systemzustand und fehlerhafte Dienste.
- `relay_list_units`: maximal 100 Dienstnamen und Zustände, keine Beschreibungen oder Prozessargs.
- `relay_plan_change`: Schema-1 Intent durch Core-Prüfung und isolierte Kandidatenplanung.
- `relay_show_plan`: Preview lokal anzeigen; vollständiger Inhalt wird nicht dem Modell übergeben.

Die Agent-Antwort auf Plan-Tools enthält nur ID, Risiko, Anwendbarkeit, Reboot-Komponenten und
Inhibitoren. Reviewwerte, Diff und Kandidatenpfade gehen als TUI-Details an die Person.

## Router und Berechtigungsmodell

Der lokale Router klassifiziert Eingaben als `INSPECT`, `DIAGNOSE`, `RELAY_CHANGE`, `BLOCKED` oder
`DEVELOPMENT_REQUIRED`. Bekannte geschützte Ressourcen und bekannte noch nicht unterstützte
Backend-Anfragen werden vor dem Modellaufruf angehalten. Das ist ein Usability-Filter; Core-Intent-
Validierung und Schutzliste bleiben die Sicherheitsgrenze. Unbekannte Kategorien autorisieren
keine Mutation.

Apply/Undo/Recover sind keine Modelltools. Für Apply muss eine anwendbare, zuvor angezeigte Plan-ID
vorliegen. Nach `/apply <id>` verlangt das TUI exakt `APPLY <id>`. Undo und Recovery zeigen vorher
die konkreten Ziele und fordern `UNDO <id>` beziehungsweise `RECOVER <ids...>`. Das Protokoll
verlangt `confirmed: true`; Core prüft Undo-Ziel oder Recovery-ID-Menge erneut, während die
Engine-Sperre gehalten wird. Die vorhandenen Preflight-, Risiko-, Health-, Journal- und
Recovery-Abläufe liegen weiterhin im Rust-Core.

Secrets werden vor dem Provideraufruf mit einem heuristischen Musterfilter abgelehnt. Das ist keine
vollständige DLP-Garantie. Normale Modellantworten und Sessionverläufe werden nicht persistiert.

## Pi-Frontend-Dateien und Tests

- Implementierung: [`agent/src`](../../agent/src), Paket: [`agent/package.json`](../../agent/package.json).
- Core-Protokoll: [`protocol.rs`](../../crates/relay/src/protocol.rs) und
  [`protocol_io.rs`](../../crates/relay/src/protocol_io.rs).
- CI: Rust-Job plus Node 22 Agent-Job in [ci.yml](../../.github/workflows/ci.yml).
- Geprüft am 2026-10-03: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` (169 Lib- und 11 CLI-Tests), `npm run typecheck`,
  `npm test` (14 bestanden, 1 VM-Test standardmäßig übersprungen), Nix-Paketbau und `--check`.
  Der abschließende NixOS-VM-Lauf mit Agent-Bridge-Workflow bestand am 2026-10-03.
- Baseline vor Änderungen: 156 Lib- und 10 CLI-Tests bestanden.
- Echte Provider-Anfrage und gestarteter Pi-TUI-Dialog sind noch nicht als bestanden dokumentiert.

## Verbleibende offene Abnahmetore

- Die Diagnose-Abdeckung liefert strukturierte, begrenzte Daten. Journaldiagnose gibt ausschließlich
  freigegebene Metadaten zurück; `MESSAGE`-Texte werden nicht abgefragt oder an den Agenten gegeben.
- Der VM-Workflow prüft den echten Core-Prozess und das lokale Bestätigungsgate, aber keinen
  interaktiven Pi-TUI-Lauf. Drift-, Build-/Healthfehler und Reboot-Szenarien bleiben in Core- und
  NixOS-VM-Tests abgedeckt, nicht als Agent-TUI-Pilot.
- Kein Aktivierungspilot: Ein späterer Read-only-Lauf am 2026-10-03 bestätigte
  `managed_module: in-sync`, aber `plan add-package hello` wurde wegen Source-Drift abgelehnt.
  Der konkrete Systemunterschied ist `tuigreet --battery --asterisks` in `desktop.nix`, der in
  der laufenden Generation fehlt. Die separat geänderten README-, Hyprbars- und Quickshell-Dateien
  sollen erhalten bleiben; Hyprbars und der Quickshell-IPC-Handler waren zur Laufzeit aktiv.
  `nixos-rebuild dry-build` gelang; Relay hat die NixOS-Generation nicht gewechselt.
- Providerwechsel und echte Providerverfügbarkeit wurden nicht live geprüft.
- TUI-Preview und Bestätigung sind funktional implementiert, aber noch nicht in einem begleiteten
  Usability-Pilot bewertet.

## Offene Risiken

- **P1 – Quellabweichung:** `desktop.nix` enthält `--battery --asterisks` für `greetd`, die in der
  laufenden Generation fehlen. Relay verweigert deshalb Planung korrekt. Eine Aktivierung würde
  den Display-Manager betreffen und braucht eine bewusste Betriebsfreigabe.
- **P1 – Provider-Datenfluss:** TUI weist auf externe Übertragung hin; reale Providerkonfiguration,
  Credential-Auflösung und Datenminimierung müssen vor täglicher Nutzung weiter geprüft werden.
- **P2 – Routerheuristik:** Wortmuster können umformulierte Anfragen übersehen oder unpassend
  klassifizieren. Das erlaubt keine Core-geschützte Änderung, kann aber zu schlechter UX führen.
- **P2 – Secret-Filter:** Heuristik kann unbekannte Secret-Formate verpassen. Keine Logs sollten
  ungeprüft zum Modell gelangen.
- **P2 – Nix-Paketreproduzierbarkeit:** Der lokale Nix-Build mit festem `npmDepsHash` war
  erfolgreich; wiederholte CI-Ausführung bleibt abzuwarten.

## Empfehlung für V2

1. Nix-Paketbau und Agent-CLI-Check in CI ergänzen, wenn CI-Runner NixOS/KVM verlässlich bereitstellt.
2. Interaktive Pi-TUI-Session und Providerdatenfluss mit einem vorbereiteten Testkonto prüfen.
3. Begleiteten Usability-Pilot für Risiko-, Diff-, Bestätigungs- und Recoverytexte durchführen.
4. Nach Auflösung der Quellabweichung zuerst Plan, Apply und Recovery in der NixOS-VM validieren.
   Einen späteren Live-Einsatz als separate Betriebsentscheidung mit genauem Diff und Recovery-Plan
   behandeln, nicht als Testlauf.
