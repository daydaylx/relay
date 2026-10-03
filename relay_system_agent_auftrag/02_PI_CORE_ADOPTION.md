# 02 – Original-Pi als Agentenbasis

## Quelle

Nur das Original-Pi verwenden:

- https://pi.dev/
- https://github.com/earendil-works/pi

Nicht `daydaylx/pi` als Implementierungsbasis verwenden.

## Zu prüfende offizielle Pi-Komponenten

Priorisiert untersuchen:

- `@earendil-works/pi-agent-core`
- `@earendil-works/pi-ai`
- `@earendil-works/pi-tui`
- `@earendil-works/pi-coding-agent` SDK

Pi dokumentiert direkte SDK-Einbettung sowie eine Trennung zwischen Agent Runtime,
AI und TUI. Genau diese Trennung nutzen.

## Bevorzugte Strategie

### Option A – bevorzugt: Pi-Pakete einbetten

Eigenes Relay-Agent-Paket anlegen und die offiziellen Pakete als Dependencies verwenden.

Vorteile:

- Relay bleibt eigenständiges Produkt.
- Updates des Pi-Cores können kontrolliert gepinnt werden.
- Keine komplette Fork-Synchronisation.
- Eigene Tools, Prompts und Sicherheitsregeln.
- Eigener Produktname und eigener Config-Pfad.

### Option B – Source Fork

Nur wählen, wenn die SDK/Paket-Schnittstellen eine für Relay notwendige Funktion
nachweislich nicht anbieten.

Ein Fork darf nicht gewählt werden, nur weil es kurzfristig einfacher ist.

## Entscheidungsgate

Vor Implementierung eine ADR erstellen:

`adr/0009-pi-core-integration.md`

Sie muss beantworten:

1. SDK/Pakete oder Fork?
2. Welche Pi-Pakete werden verwendet?
3. Welche Version wird gepinnt?
4. Welche Teile des Pi Coding Agents werden ausdrücklich nicht übernommen?
5. Wie werden Upgrades getestet?
6. Wo liegt die neue Relay-Agent-Konfiguration?
7. Wie wird verhindert, dass ein vorhandenes `~/.pi`-Setup versehentlich geladen wird?

## Isolation

Der neue Agent muss einen eigenen Config-/State-Bereich besitzen, beispielsweise:

```text
~/.config/relay/
~/.local/state/relay/
```

Keine implizite Nutzung von:

```text
~/.pi/
~/.pi/agent/
```

sofern dies nicht ausdrücklich und bewusst als Importfunktion implementiert wird.

## Nicht übernehmen

Nicht automatisch übernehmen:

- Pi Coding Tools `write/edit/bash` in uneingeschränkter Form
- Drittanbieter-Extensions
- MCP
- Subagenten
- Skills aus dem persönlichen Profil
- Projekt-Prompts aus beliebigen Arbeitsverzeichnissen
- unkontrollierte Projekt-Extensions

Ein Systemagent hat eine andere Trust Boundary als ein Coding-Agent.
