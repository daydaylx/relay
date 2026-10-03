# Relay → eigenständiger NixOS-Systemagent

**Zielrepo:** https://github.com/daydaylx/relay
**Agent-Basis:** Original Pi von https://pi.dev/
**Pi-Quellrepo:** https://github.com/earendil-works/pi
**Relay-Baseline:** `a39e473b53d1f0d044d727d66bdc002424e11cb0`
**Stand:** 2026-10-03

## Ziel

Relay soll von einem sicheren NixOS-Transaktionswerkzeug zu einem **eigenständigen lokalen
NixOS-Systemagenten** ausgebaut werden, der sich per natürlicher Sprache bedienen lässt,
das System gezielt untersuchen kann, Änderungen plant, passende Backends auswählt,
Änderungen verifiziert und bei Fehlern recovern kann.

Das neue Produkt ist **nicht** das persönliche `daydaylx/pi`-Setup.

Es darf keine Abhängigkeit zu folgenden persönlichen Pi-Komponenten geben:

- Rabbitmode
- eigene Subagenten
- eigener Verifier
- eigene Permission-Erweiterungen
- Benchmarks/Duel
- Task-Tiers
- Aurora-spezifische Pi-Erweiterungen
- persönliche Prompts/AGENTS/Settings

Verwendet werden darf nur das **Original-Pi als technische Agentenbasis**.

## Empfohlene Architektur

```text
User
  ↓ natürliche Sprache
Relay Agent Shell (TypeScript / Pi-Bausteine)
  ├─ Conversation / Sessions
  ├─ Model Provider
  ├─ Tool Loop
  ├─ Streaming
  ├─ TUI
  ├─ System Context
  ├─ Inventory
  ├─ Diagnostics
  └─ Task Router
          ↓
  ┌───────────────────────────────┐
  │ Relay Core (Rust)             │
  │ deterministische Änderungen   │
  │ Nix eval/build                │
  │ risk / preview                │
  │ activation                    │
  │ health                        │
  │ journal / undo / recovery     │
  └───────────────────────────────┘
          ↓
        NixOS
```

## Kernregel

**Pi liefert den Agent-Motor. Relay bleibt die System-Sicherheitsgrenze.**

Nicht zulässig:

```text
LLM → freie sudo-Shell → System
```

Bevor mit Implementierung begonnen wird, alle nummerierten Dokumente lesen.
