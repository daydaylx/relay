# ADR 0011 – Pi als Relay-kontrollierte RPC-Runtime

Status: Accepted; task-scoped RPC runtime and Relay tool bridge implemented; lifecycle hardening remains

## Kontext

Relays Kern ist Rust. Die bisherige optionale TypeScript-Oberfläche bettet `pi-agent-core` direkt ein
und modelliert Relay Tasks separat. Das verbindet Pi-Control und UI eng mit Relay und lässt Pi-Sessions,
Context Compaction und Resource Lifecycles ungenutzt.

## Entscheidung

Pi läuft als separater Kindprozess im aktuellen Nutzerkontext und spricht version-gepinntes JSONL RPC
mit Relays Agent Host. Eine pro Task erzeugte, Relay-eigene Pi-Extension vermittelt Tool-Aufrufe über
einen privaten Unix-Socket an eine feste Tool-Registry. Der Host besitzt Prozess-Lifecycle,
Request/Event-Framing, Zeitlimits, Abbruch, Session-Zuordnung und Task-Journal. Pi-Prozessisolation
allein gilt nicht als Security Boundary. Pi erhält weder eigene Relay-Core-Policies noch generische
Shell-/Datei-/Root-Tools.

Die CLI-/RPC-Runtime kommt als exakte Repository-Abhängigkeit samt Lockfile; keine globale Pi-
Installation ist Voraussetzung. `~/.pi` und projektlokale Pi-Ressourcen werden nie automatisch
geladen. Runtime erhält eigene Config-, Session-, Prompt-, Skill-, Extension- und State-Verzeichnisse
unter Relays XDG-Pfaden und einen explizit kontrollierten CWD. Offline-/Telemetry-Einstellungen sind
explizit; nur die task-scoped Relay-Extension wird explizit geladen. Sessions sind Relay-Tasks
zugeordnet; Secrets werden nicht in Session-/Task-Transkripte kopiert. MCP, Web und persönliche
Pi-Ressourcen sind nicht konfiguriert.

## Konsequenzen

Der RPC-Client splittet JSONL, begrenzt Records und stderr, korreliert Requests und wartet auf
`agent_settled`; die Extension signalisiert den erfolgreichen Start vor Nutzung. Crash-Recovery,
Streaming-Vollständigkeit und UI-Subprotocol-Mapping müssen weiter gehärtet werden. Pi-Version-Updates
werden anhand gelockter Typen/CLI-Protokolle reviewed. Siehe ADR 0012–0022 für SystemContext,
Gateway, Knowledge und Ausführung.

## Verworfen

- Pi SDK direkt im Rust-Kern: keine Rust API.
- Aktuelle TypeScript SDK-In-Process-Verbindung als Produktgrenze: weniger Prozessisolation und
  Runtime-/Host-Lifecycle eng gekoppelt.
- Systemweit installiertes Pi/`~/.pi`: versteckte Version-/Profilabhängigkeit.

## Quellen

- [Pi RPC](https://pi.dev/docs/latest/rpc)
- [Pi SDK](https://pi.dev/docs/latest/sdk)
- [Pi Context und Sessions](https://pi.dev/docs/latest/how-pi-works)
