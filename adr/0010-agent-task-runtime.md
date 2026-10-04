# ADR 0010 – Pi Agent Runtime und Relay Tasks

Status: Superseded by ADR 0011–0022 (2026-10-03)

## Kontext

Relay besitzt bereits den deterministischen NixOS-Core, ein separates Pi-Frontend, Read-Tools,
Diagnosebereiche und einen Pi-Tool-Loop. Der aktuelle Frontend-Ablauf modelliert aber kein dauerhaftes
Nutzerziel: Diagnose-Routing geschieht vor dem Agenten, Apply/Undo/Recovery laufen als direkte
TUI-Kommandos außerhalb seines Laufs, und nach einem Apply wird kein Task-Ziel erneut geprüft.
Eine Anfrage kann daher nicht verlässlich mehrere Relay-Changes und Verifikationsschritte umfassen.

## Entscheidung der ersten Ausbaustufe

Diese Entscheidung dokumentiert die erste enge Task-Runtime. Sie bleibt gültig für die Task-/Core-
Trennung und den Schutz vor dem persönlichen Pi-Setup, definiert aber **nicht mehr** den langfristigen
Toolumfang. Die jüngere Kontrollzentrum-Architektur in
[`docs/audits/RELAY_PI_CONTROL_CENTER_BASELINE.md`](../docs/audits/RELAY_PI_CONTROL_CENTER_BASELINE.md)
ersetzt die frühere direkte TypeScript Runtime-Einbettung durch eine zu prüfende Pi-RPC-Grenze und
einen Relay Execution Gateway.

## Frühere Entscheidung

- Relay bleibt das eigenständige Systemprodukt. Die ausgewählten Pi-Komponenten `pi-agent-core` und
  `pi-ai` liefern den Agent-Loop, Provider-Auflösung, Streaming und Tool-Ergebnisfluss. Die TUI bleibt
  eine austauschbare Oberfläche; Pi-Coding-Agent, Resource Loader, Extensions, Skills, MCP, Codemode,
  Plugins und Subagenten werden nicht eingebunden.
- Die Integration nutzt nur Projekt-Abhängigkeiten, die in `agent/package.json` und
  `agent/package-lock.json` festgelegt sind. Relay liest, kopiert und untersucht keine lokale
  persönliche Pi-Installation oder deren `~/.pi`-Daten, Provider-Profile, Prompts, Extensions,
  Einstellungen, Sessions oder Credentials. Provider-Zugangsdaten werden ausschließlich über Relays
  eigene, dokumentierte Konfiguration/API gelesen.
- Ein Nutzerziel ist ein persistierter `Task`; einzelne NixOS-Änderungen bleiben eigene Core-Journal-
  Einträge. Ein Task enthält Zustand, zeitliche Ereignisse, begrenzte Beobachtungszusammenfassungen,
  Change-IDs, Bestätigungs- und Verifikationsergebnisse. Er enthält keine Zugangsdaten und standardmäßig
  keine vollständigen Gesprächs- oder Log-Rohdaten.
- Der Agent darf iterativ ausschließlich die explizite Relay-Tool-Liste aufrufen. Beobachtung und
  Planung benötigen keine Rückfrage. Jede echte Mutation erzeugt einen planbezogenen
  `confirmation_required`-Ereigniszustand und wartet auf die direkte Nutzerbestätigung der konkreten
  Core-Vorschau. Bestätigungszustand wird lokal vom UI an die Toolausführung zurückgereicht; Modelltext
  kann ihn weder erzeugen noch ersetzen.
- Nach Apply ruft derselbe Task Read-/Health-/Diagnose-Tools auf und bewertet das Nutzerziel erneut.
  Bei Fehlschlag kann der Agent weiter untersuchen und einen neuen Kandidaten erstellen. Undo und
  Recovery bleiben Core-Operationen mit Preview und zielgebundener direkter Bestätigung.
- Pi State bleibt flüchtiger Laufzeitzustand. Relay persistiert Task-Ereignisse atomar in einem
  schema-versionierten lokalen Task-Store unter `~/.local/state/relay/tasks/`. Persistierte
  Zusammenfassungen werden vor erneutem Modellkontext auf Secret-Indikatoren und Größenlimits geprüft.
- Es gibt kein allgemeines `shell`, `sudo`, `exec`, oder beliebiges Dateilesen. System-, Nix-,
  Dienst-, Netzwerk-, Prozess- und Desktop-Beobachtung laufen über strukturierte Core-Aktionen.
  `files.read_safe` ist auf explizit öffentliche, nicht-symlink-aufgelöste Bereiche begrenzt; Secrets,
  Home-Auth-Verzeichnisse und geschützte Konfigurationen werden code-seitig verweigert.
- Der Rust-Core bleibt eigenständig ausführbar. Das integrierte Agent-Frontend wird als optionale
  Runtime ausgeliefert und über den Relay-Einstieg gestartet; die bestehenden deterministischen
  Core-Kommandos behalten ihre Schnittstelle.
- `relay ask` bleibt für einmalige, nicht interaktive Vorschläge als Fast Path erhalten. Es nutzt
  dieselbe Core-Validierung und konkurriert nicht mit Task-Sessions: interaktive Ziele laufen durch
  die Pi Runtime.

## Task-Zustände

```text
created → investigating → planning → waiting_confirmation
        → applying → verifying → continuing → completed
                    ↘ failed / blocked / cancelled
```

Jeder Übergang erzeugt ein append-only Task-Ereignis. Ein Change wird nur dann als ausgeführt
markiert, wenn der Core ein passendes Ergebnis samt Change-ID zurückgibt. Ein Task wird nur dann
`completed`, wenn eine strukturierte Verifikation das Ziel erreicht bestätigt; Modellbehauptungen
allein sind kein Nachweis.

## Pi-Bausteine

| Pi-Komponente | Entscheidung | Grund |
| --- | --- | --- |
| `pi-agent-core` `Agent` | übernehmen | Stateful Tool-Loop, `prompt`, `abort`, Events, sequenzielle Tools |
| `pi-ai` | übernehmen | Provider-Abstraktion und Streaming |
| `pi-tui` | UI-Adapter behalten | Terminaloberfläche; nicht Teil der Task-/Agent-Service-API |
| `pi-coding-agent` | nicht übernehmen | läde zusätzliche Coding-/Resource-Loader-Flächen, die Relay nicht braucht |
| Pi Extensions / Skills / MCP / Codemode | nicht übernehmen | außerhalb der expliziten Relay-Tool-Grenze |
| Subagents | nicht übernehmen | einzelner nachvollziehbarer Agent-Loop im ersten Ausbau |

## Konsequenzen

- Task-/Session-Service und UI werden getrennte TypeScript-Module. UI zeigt typisierte
  `AgentEvent`s und bestätigt nur Core-validierte Vorschauen.
- Agent-Tools erhalten strukturiertes Goal-/Change-Handling und mehrere Read-Werkzeuge.
- Pro Task sind Toolanzahl, Kontextbytes, Einzeltool-Ausgabe, Laufzeit und Wiederholungen begrenzt.
- Core-Protokoll und Journal bleiben maßgeblich für jede Änderung und Recovery.
- Der Agent ist weiterhin optional; AI-Ausfall verhindert weder Core-Nutzung noch Recovery.

## Quellen

- Pi Agent Core API: <https://github.com/earendil-works/pi/blob/main/packages/agent/README.md>
- Pi SDK Übersicht: <https://pi.dev/docs/latest/sdk>
- Pi-Projekt und Paketgrenzen: <https://github.com/earendil-works/pi>
