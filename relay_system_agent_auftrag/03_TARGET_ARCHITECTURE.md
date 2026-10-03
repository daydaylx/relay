# 03 – Zielarchitektur

## Schichten

### A. Agent Shell

TypeScript auf Original-Pi-Bausteinen.

Verantwortlich für:

- Modellzugriff
- Konversation
- Tool Loop
- Streaming
- Session Lifecycle
- TUI
- natürliche Sprache

Nicht verantwortlich für die finale Sicherheitsentscheidung einer Root-Mutation.

### B. System Intelligence

Neue Relay-Komponente.

Verantwortlich für:

- System Inventory
- Context Retrieval
- Diagnostics
- Task Classification
- Routing
- Effect Verification

### C. System Tools

Lesende Werkzeuge, z. B.:

- filesystem inspect
- process inspect
- systemd
- journal
- NixOS generations
- Nix store metadata
- network state
- hardware state
- Hyprland IPC read
- config discovery

Lesen soll breit möglich sein, aber Secret-Pfade müssen gesondert behandelt werden.

### D. Relay Core

Bestehende Rust-Sicherheitskomponente.

Verantwortlich für:

- typed intents
- managed Nix changes
- candidate isolation
- evaluation
- build
- diff
- risk
- confirmation contract
- activation
- health
- journal
- undo
- recovery

### E. Spätere User-Space Backends

Nicht in Phase 1 erzwingen.

Mögliche spätere Backends:

- Home Manager
- Hyprland config
- Waybar
- launcher
- shell config
- user systemd services

Jedes Backend benötigt eigene Write Boundary und Recovery-Semantik.

## Prozessgrenze

Empfohlen:

```text
relay-agent (TS)
       ↓ typed JSON protocol
relay-core (Rust)
       ↓
NixOS
```

Der Agent soll `relay-core` nicht über freie Shell-Strings steuern.

Es ist eine versionierte maschinenlesbare Schnittstelle zu definieren.

Beispielkategorien:

```text
inspect
plan
show
apply
undo
recover
health
```

Die konkrete API wird erst nach Audit des bestehenden CLI festgelegt.
