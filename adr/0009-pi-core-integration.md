# ADR 0009 – Optionaler Agent auf Original-Pi-Bausteinen

Status: Accepted

## Kontext

Der bestehende Relay-Core ist ein eigenständiges, AI-unabhängiges NixOS-Systemwerkzeug.
Er besitzt Intent-Validierung, Schutzregeln, Candidate-Isolation, Evaluation, Build,
Aktivierung, Health, Journal, Undo und Recovery. `relay ask` ist bereits eine optionale
Vorschlagsoberfläche, aber kein interaktiver Agent mit strukturierten Diagnosewerkzeugen.

Der System-Agent-Auftrag vom 2026-10-03 verlangt einen eigenständigen conversational Layer,
während die bestehenden Projektregeln verbieten, die Sicherheitsgrenze in ein Modell oder
einen Coding-Agenten zu verlagern.

## Entscheidung

- Relay bleibt ein eigenständiges Systemprodukt. Der optionale Agent ist ein Frontend und
  keine Core-Abhängigkeit. `relay` und alle sicherheitskritischen Abläufe bleiben ohne Node,
  Pi oder Netzwerk benutzbar.
- Der Agent verwendet ausschließlich offizielle Pakete aus `earendil-works/pi`: zunächst
  `@earendil-works/pi-agent-core`, `@earendil-works/pi-ai` und `@earendil-works/pi-tui`.
  `@earendil-works/pi-coding-agent` wird nicht eingebunden: sein Ressourcen- und
  Konfigurations-Ökosystem ist für V1 unnötig breit und bringt zusätzliche Coding-Agent-
  Erweiterungspunkte mit.
- Direkte Pi-Abhängigkeiten werden exakt gepinnt und über `agent/package-lock.json`
  festgeschrieben. Zum Entscheidungszeitpunkt melden alle drei Pakete Version `1.0.0` und
  verlangen Node.js `>=22.19.0`. Upgrades erfordern Lockfile-Review, Isolationstests und
  Kompatibilitätsprüfung.
- Relay baut seinen eigenen Agentenstart, Session-Speicher und Systemprompt. Es werden keine
  Pi-CLI-Defaults, Extensions, Skills, Projekt-Prompts, Tool-Discovery, MCP, Codemode,
  Subagenten, Shell-Tools oder persönlichen `~/.pi`-Dateien geladen.
- Pi erhält ausschließlich selbst definierte Relay-Tools. Diagnosewerkzeuge liefern begrenzte,
  strukturierte Daten. Apply, Undo und Recover sind keine Modellwerkzeuge; der lokale TUI ruft sie
  nach direkten, zielgebundenen Bestätigungen über das versionierte JSON-Protokoll auf.
  Core-Policy, geschützte Ressourcen, Bestätigung und Recovery bleiben allein dort.
- Das Agentenprofil und Sitzungen liegen getrennt unter `~/.config/relay/` und
  `~/.local/state/relay/`. Modellprovider und Datenweitergabe müssen für Nutzer sichtbar
  konfiguriert sein. Modellantworten und Prompts werden nicht dauerhaft gespeichert, sofern
  der Nutzer dies nicht ausdrücklich aktiviert.
- Keine generische Shell und kein frei formulierter privilegierter Befehl werden dem Modell
  angeboten. Ein Tool-Aufruf autorisiert keine Systemänderung.

## Integrationsform

Der Agent läuft als separater Node-Prozess und startet `relay` mit einem festen Argumentvektor
und versionierten JSON-Anfragen/-Antworten. Damit bleiben Sprach- und Core-Laufzeit isoliert,
und die bestehende CLI bleibt ein vollständiger unabhängiger Zugang.

```text
relay-agent (TypeScript, Original-Pi)
  ├─ eigene Session-/Ressourcenverwaltung
  ├─ feste read-only Tools und Intent-Router
  └─ versioniertes JSON über stdin/stdout
       ↓
relay (Rust)
  ├─ Inspect / Plan / Show
  └─ Apply / Undo / Recover mit bestehenden Safety-Gates
```

## Konsequenzen

- ADR 0001 wird für den Frontend-Integrationspunkt durch diese Entscheidung superseded. Seine
  Forderung, dass der Core unabhängig bleibt, gilt weiter.
- Die JSON-Schnittstelle enthält versionierte Lese-, Plan-, Preview- und Mutationsaktionen. Die
  Mutationsaktionen verlangen direkte Bestätigung und werden vom Core mit dem bestätigten
  Plan-/Recovery-Ziel abgeglichen.
- Pi bietet laut offizieller Dokumentation selbst keine Permission-Sandbox. Relay muss daher
  Tool-Allowlist, Ressourcenisolation und Prozessgrenze selbst erzwingen.
- V1 bleibt lokal, interaktiv und synchron; keine Remote-Verwaltung, Hintergrundautonomie,
  Plugin-Ausführung oder User-Space-Schreibbackends.

## Quellen und Versionsstand

Geprüft am 2026-10-03:

- [Pi SDK](https://pi.dev/docs/latest/sdk): direkte Sessionsteuerung, explizite Tools und
  kontrollierbarer ResourceLoader; SDK-Defaults können Ressourcen und Tools laden.
- [Pi Agent Core](https://github.com/earendil-works/pi/tree/main/packages/agent): Agent-Klasse,
  eigene Toolliste und Event-Loop.
- [Pi Paketübersicht](https://github.com/earendil-works/pi): Paketgrenzen, Node-Anforderung
  und Aussage, dass Pi keine eingebaute Permission-Sandbox besitzt.
- npm Registry: `pi-agent-core@1.0.0`, `pi-ai@1.0.0`, `pi-tui@1.0.0`.
