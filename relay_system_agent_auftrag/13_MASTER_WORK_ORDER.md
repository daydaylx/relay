# MASTER WORK ORDER – Relay zum eigenständigen Systemagenten ausbauen

## Auftrag

Baue `https://github.com/daydaylx/relay` schrittweise zu einem eigenständigen
NixOS-Systemagenten aus.

Als Agentenbasis darf ausschließlich das Original-Pi von `https://pi.dev/`
bzw. `https://github.com/earendil-works/pi` verwendet werden.

Das persönliche Repository `daydaylx/pi` ist **keine Implementierungsbasis**
und darf weder kopiert noch als Runtime-Abhängigkeit verwendet werden.

## Ziel

Relay soll natürliche Sprache verstehen, das lokale NixOS gezielt untersuchen,
Probleme diagnostizieren und sichere Systemänderungen durchführen können.

Der Nutzer soll nicht selbst wissen müssen, welches Backend erforderlich ist.

## Architekturvorgabe

Behalte den bestehenden Rust-Relay-Core als deterministische Sicherheits- und
Transaktionsschicht.

Baue darüber einen eigenständigen Agent-Layer auf Original-Pi-Bausteinen.

Bevorzugt prüfen:

- `@earendil-works/pi-agent-core`
- `@earendil-works/pi-ai`
- `@earendil-works/pi-tui`
- `@earendil-works/pi-coding-agent` SDK

Keine vollständige Pi-Fork-Übernahme, sofern die benötigte Funktion über stabile
Pakete/SDK erreichbar ist.

## Harte Grenzen

1. Keine generische Root-Shell für das Modell.
2. Keine direkte Umgehung des Relay Cores.
3. Protected Resources bleiben technisch blockiert.
4. Persönliches `~/.pi` darf nicht automatisch geladen werden.
5. Keine Rabbit-/Subagent-/Verifier-/Benchmark-Komponenten aus `daydaylx/pi`.
6. Keine Systemmutation allein aufgrund von Prompttext.
7. Unbekannte Situation → Diagnose/Plan statt Mutation.
8. Secrets nicht in normalen Modellkontext oder Audit übernehmen.
9. Alle mutierenden Pfade brauchen Verifikation und Recovery.
10. Kein Big-Bang-Rewrite.

## Arbeitsreihenfolge

### Phase 0
Baseline auditieren und alle bestehenden Relay-Safety-Invarianten dokumentieren.

### Phase 1
Original-Pi SDK/Pakete untersuchen und ADR zur Integrationsstrategie schreiben.
Einen minimalen Agent-Spike ohne Systemmutation bauen.

### Phase 2
Versionierte, maschinenlesbare Schnittstelle zwischen Agent und Rust-Core schaffen.

### Phase 3
System Inventory und strukturierte read-only Diagnosewerkzeuge implementieren.

### Phase 4
Task Router für INSPECT, DIAGNOSE, RELAY_CHANGE, BLOCKED und
DEVELOPMENT_REQUIRED implementieren.

### Phase 5
Relay plan/show/apply/undo/recover über den Agenten integrieren.
Keine freie privilegierte Shell.

### Phase 6
TUI so erweitern, dass Backend, Risiko, Preview, Bestätigung und Recovery
verständlich dargestellt werden.

### Phase 7
Daily-Driver-Pilot mit harmlosen Änderungen.

### Phase 8
Erst danach prüfen, ob separate sichere Backends für Home Manager und Hyprland
notwendig sind.

## Vor jeder Phase

- relevanten Code vollständig lesen
- Tests definieren
- Risiken benennen
- keine Annahme aus Dokumentation ungeprüft übernehmen

## Nach jeder Phase

- gezielte Tests
- Regression Suite
- Dokumentation
- Statusdatei
- offene Risiken

## Definition of Done für erste Ausbaustufe

Der Auftrag ist erfolgreich, wenn:

1. Relay als eigenständiger Agent gestartet werden kann.
2. Kein vorhandenes persönliches Pi-Setup benötigt wird.
3. natürliche Sprache über Original-Pi-Agent-Core verarbeitet wird.
4. der Agent Systemzustand strukturiert lesen kann.
5. Diagnose ohne Mutation möglich ist.
6. einfache unterstützte NixOS-Änderungen vollständig über Relay Core laufen.
7. der Nutzer Preview und Risiko vor Systemänderung sieht.
8. Health Check nach Änderung läuft.
9. Undo und Recovery vom Agenten aus erreichbar sind.
10. ein Modell die Rust-Sicherheitsgrenze nicht umgehen kann.
11. CI sowohl Agent-Layer als auch Relay-Core prüft.
12. ein dokumentierter Daily-Driver-Pilot bestanden wurde.

## Abschlussbericht

Erstelle:

`docs/audits/SYSTEM_AGENT_V1_RESULT.md`

mit:

- Architektur
- verwendete Pi-Komponenten + Version
- nicht übernommene Pi-Komponenten
- Relay-Core Änderungen
- Toolliste
- Permissionmodell
- Routerlogik
- Tests
- CI
- reale Pilot-Ergebnisse
- offene P0/P1/P2 Risiken
- Empfehlung für V2

Schwierigkeiten: 9/10 | Thinking: xhigh
