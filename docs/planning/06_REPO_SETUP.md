# Repository Setup

## Empfohlener Repository-Name

```text
relay
```

Alternativ, falls der Name bereits belegt oder zu allgemein ist:

```text
relay-nixos
system-relay
relay-os
```

Produktname kann weiterhin `Relay` bleiben.

## Repository-Typ

Zu Beginn:

```text
private
```

Empfehlung für den persönlichen Daily-Driver-Aufbau.

Später kann das Repository bewusst geöffnet werden.

## Baseline

Der erste Commit soll nur enthalten:

- Planung
- Architektur
- ADRs
- AGENTS.md
- Repo-Metadaten
- leeres Implementierungsgerüst

Noch keine halb fertige Systemmutation.

Empfohlene Commit Message:

```text
chore: bootstrap Relay architecture and project plan
```

Optional Tag:

```text
planning-v1
```

## Branching

Für ein kleines persönliches Projekt:

```text
main
feature/<name>
fix/<name>
```

Kein komplexes GitFlow.

`main` soll grundsätzlich einen konsistenten Zustand enthalten.

## Milestones

### M0 – Repository Ready
Target State T0

### M1 – Read-only Observer
Target State T1

### M2 – Candidate Engine
Target State T2

### M3 – Controlled Activation
Target State T3

### M4 – Recoverable MVP
Target State T4

### M5 – AI Layer
Target State T5

### M6 – Desktop Integration
Target State T6

## Initial Issues

### M0

- Bootstrap Rust workspace
- Add dev/test environment
- Add CI baseline
- Record supported NixOS/Nix versions
- Decide persistence format

### M1

- Implement system identity
- Implement generation discovery
- Implement config/flake identity
- Implement health snapshot
- Implement options index
- Implement package search

### M2

- Define Change Object
- Implement managed.nix renderer
- Candidate directory
- Evaluation
- Build
- Closure diff
- dry-activate parsing
- Drift detection
- Risk classification

### M3

- Privilege boundary
- test activation
- health verification
- switch
- boot/reboot path
- switch inhibitors

### M4

- Journal
- source restore
- runtime restore
- Relay Undo
- crash recovery
- pending change recovery

## Labels

Empfohlen:

```text
area:core
area:nix
area:system
area:change
area:recovery
area:knowledge
area:ai
area:ui

type:feature
type:bug
type:research
type:test
type:security
type:docs

priority:p0
priority:p1
priority:p2

risk:low
risk:medium
risk:high
risk:protected

status:blocked
status:needs-design
```

## Pull Requests

PRs sollen klein bleiben.

Keine Kombination wie:

```text
"Implement Candidate Engine + ChatGPT + Hyprland"
```

stattdessen getrennte vertikale Änderungen.

## CI – erste Stufe

Sobald Rust-Code existiert:

- format check
- lint
- unit tests
- build
- documentation check

Systemtests mit echter NixOS-Aktivierung nicht auf normale CI-Runner loslassen.

Dafür separate VM/NixOS-Teststrategie definieren.

## NixOS Integration Tests

Mittelfristig soll Relay echte NixOS-VM-Tests verwenden.

Besonders für:

- build
- test activation
- service changes
- reboot flow
- rollback

Live-Daily-Driver-System nicht als primäre Testumgebung verwenden.

## Release Strategy

Vor T4 keine stabilen Releases behaupten.

Vorschlag:

```text
0.1.0 – read-only observer
0.2.0 – candidate builder
0.3.0 – controlled activation
0.4.0 – recoverable MVP
0.5.0 – optional AI
```

## License

Nicht automatisch festlegen.

Vor öffentlicher Veröffentlichung bewusst entscheiden, z. B.:

- MIT
- Apache-2.0
- MPL-2.0
- GPL-3.0-or-later

Bis dahin keine zufällige Lizenzdatei generieren.
