# 08 – Implementierungsphasen

## Phase 0 – Baseline einfrieren

- aktuellen Relay-Stand dokumentieren
- Tests ausführen
- Architekturdiagramm aktualisieren
- Arbeitsbranch erstellen
- keine Feature-Änderung

**Gate:** Baseline reproduzierbar.

## Phase 1 – Pi Integration Spike

Nur Prototyp, noch keine Systemmutation.

- Original-Pi SDK/Pakete evaluieren
- minimaler eigener Agent-Start
- eigenes Config-Verzeichnis
- eigenes Modellprofil
- eine Dummy-Tooldefinition
- Session Lifecycle prüfen
- TUI/Streaming prüfen

**Gate:** Agent funktioniert vollständig ohne persönliches `~/.pi`-Setup.

## Phase 2 – Versioniertes Core-Protokoll

- maschinenlesbare Relay-Core-Schnittstelle
- keine Parsing-Abhängigkeit von hübscher CLI-Ausgabe
- Schema-Version
- Fehlercodes
- typed requests/responses

**Gate:** TS-Agent kann `status/health/plan/show` sicher ansprechen.

## Phase 3 – Read-only Systemagent

- Inventory
- systemd
- journal
- network
- Bluetooth
- Nix generations
- Hyprland read
- context retrieval

**Gate:** 20 definierte Diagnosefragen ohne Mutation zuverlässig beantworten.

## Phase 4 – Router

- inspect
- diagnose
- relay_change
- unsupported/development_required

**Gate:** Testmatrix verhindert Mutation bei unsicherem Routing.

## Phase 5 – Relay Mutation Integration

- plan
- preview
- confirmation
- apply
- verify
- undo
- recover

**Gate:** Agent kann harmlose NixOS-Änderung vollständig über Relay durchführen,
ohne freie privilegierte Shell.

## Phase 6 – Agent UX

- TUI
- Statusleiste
- klare Change Preview
- sichtbare Risiko-/Backend-Anzeige
- Undo/Recover prominent

**Gate:** Daily-Driver-Pilot ohne direkte Relay-CLI-Nutzung möglich.

## Phase 7 – User-Space Backend Design

Erst jetzt Home Manager/Hyprland-Schreibzugriff evaluieren.

Kein Direktedit als Schnelllösung.

## Phase 8 – Daily Driver

Testfolge:

1. read-only status
2. Diagnose
3. Paket installieren
4. Undo
5. Option setzen
6. Drift
7. absichtlicher Buildfehler
8. Service Health Failure
9. Recover
10. später Reboot Change
