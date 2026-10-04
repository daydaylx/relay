# Relay Agent Runtime und Task-Modell

Relay bettet ausgewählte Komponenten aus Pi als Reasoning- und Tool-Orchestrierungsschicht ein.
Relay bleibt für Policy, Kandidaten, Evaluation, Build, Risiko, Bestätigung, Aktivierung,
Verifikation und Recovery zuständig.

```text
relay CLI / spätere GUI
        ↓ goal + UI commands
Task Service ─── Task Store
        ↓ typed AgentEvents
Pi Agent (`pi-agent-core` + `pi-ai`)
        ↓ feste Relay Tools, sequenziell und begrenzt
Relay Tool Service
        ↓ versioniertes lokales Core-Protokoll
Rust Safety Core
        ↓
NixOS
```

## Verantwortlichkeiten

- **Pi Runtime:** Nutzerziel verstehen, Read-Tools auswählen, Hypothesen prüfen, Change-Vorschläge
  als strukturierte Relay-Intents formulieren und nach Resultaten weiterarbeiten.
- **Installationsgrenze:** Nur explizite, gelockte Pi-NPM-Abhängigkeiten aus diesem Repository.
  Persönliche Pi-Installation, `~/.pi`, lokale Profile, Prompts, Extensions, Sessions und Credentials
  werden weder gelesen noch kopiert oder untersucht.
- **Task Service:** Zustandsmaschine, Iterations-/Zeitlimits, Abbruch, Event-Ausgabe und Resume von
  Task-Zusammenfassungen. UI und Pi SDK bleiben austauschbar.
- **Relay Tool Service:** Argumente validieren, Core-Protokoll aufrufen, Ausgaben minimieren und
  Mutationen an eine wartende lokale Bestätigung binden.
- **Rust Core:** alleinige Autorität für geschützte Ressourcen, Candidate-Identität, Drift,
  Evaluation, Build, Switch-Inhibitoren, Aktivierung, Journal, Undo und Recovery.
- **Task Store:** atomare, append-only Task-Ereignisse; getrennt vom Change-Journal. Keine Secrets,
  API-Keys oder standardmäßige Speicherung vollständiger Rohlogs/Conversation-Payloads.
- **UI:** präsentiert Status, Toolstarts/-ergebnisse, Vorschau, Bestätigungsaufforderung und
  Abschluss. Keine Policy-Entscheidungen.

## Task und Change

```text
Task
  id, goal, created_at, state
  observations[]       # geprüfte, redigierte Zusammenfassungen
  hypotheses[]         # als Agentenannahmen gekennzeichnet
  planned_actions[]    # strukturierte Vorschläge
  change_ids[]         # Referenzen zu Core-Journal-Einträgen
  verification[]       # Toolbeleg, Zeitpunkt, Status
  final_result

TaskEvent
  schema_version, sequence, task_id, timestamp, kind, bounded_payload
```

Ein Task kann null oder mehrere Core-Changes erzeugen. Das Core-Journal bleibt unverändert die
Quelle für Change-Status und Recovery. Ein Taskabschluss benötigt Core-/Read-Tool-Belege; eine
natürlichsprachliche Behauptung des Modells reicht nicht.

## Zustandsübergänge

| Zustand | Auslöser / Folge |
| --- | --- |
| `created` | Goal validiert; keine Provider-Anfrage vor dem Nutzer-Start |
| `investigating` | Pi nutzt Read-/Diagnose-Tools; strukturierte Ergebnisse werden redigiert gespeichert |
| `planning` | Pi erzeugt typed Intent; Relay plant Candidate und führt Eval/Build aus |
| `waiting_confirmation` | Vorschau/Risiko/Recovery-Plan liegt vor; UI fragt direkte Zustimmung ab |
| `applying` | Bestätigung passt exakt zu Task, Plan-ID und Core-Review; Core wird aufgerufen |
| `verifying` | dieselben oder unabhängige Read-Tools prüfen das Ziel nach Apply |
| `continuing` | Ziel verfehlt; Pi darf innerhalb des Iterationsbudgets weiter untersuchen |
| `completed` | Ziel durch strukturierte Beobachtung bestätigt |
| `blocked` | geschützte Ressource, Drift, fehlende Rechte/Backends oder nicht auflösbarer Zustand |
| `failed` | Core/Provider/Task-Store scheitert; Ergebnis und Recovery-Hinweis sind gespeichert |
| `cancelled` | Nutzer bricht ab; laufendes Tool erhält AbortSignal; keine Bestätigung bleibt gültig |

Illegale Übergänge werden im Task Service abgelehnt. Nach Prozessneustart bleiben Tasks `blocked`
oder `failed`, bis der Zustand durch Core-Journal und neue Beobachtung wieder aufgenommen wird.

## Events für CLI und GUI

```text
task_started | status_summary | tool_started | tool_result | plan_ready
confirmation_required | applying | verification | task_continuing
completed | blocked | failed | cancelled
```

Events sind versionierte Datenobjekte, nicht formatierte Terminalstrings. Ein späteres GUI kann
denselben Task Service abonnieren, ohne Pi- oder Core-Interna direkt aufzurufen.

## Ressourcen- und Wiederholungsgrenzen

- ein Agent pro Task; Tools sequenziell
- begrenzte Zahl Provider-Turns, Tools pro Task, Toollaufzeit und Toolausgabe
- strukturierte Ausschnitte statt kompletter Logs oder Verzeichnisbäume
- Toolfehler sind `isError`; wiederholte identische Fehler beenden den Task als `blocked`
- sicher abbrechen über `Agent.abort()`/AbortSignal; offene Bestätigungen verfallen bei Abbruch
- keine Dateilesung außerhalb einer expliziten safe-read-Allowlist und kein generischer Command-Runner
- keine Pi-Projekt-/User-Ressourcen, Extensions, Skills, Shell, MCP, Codemode oder Subagents

## Toolkatalog

Read-Tools umfassen Status, Health, Generationen, Services/Logs, Hardware, Netzwerk, Prozesse,
Nix-Options-/Paket-Suche, Konfigurationsübersicht sowie Desktop-Status/Health. Schreibaktionen sind
typisierte Plan-, Preview-, Apply-, Discard-, Undo- und Recovery-Aufrufe. Die Toolbeschreibung und
Modellinstruktion erteilen keine Berechtigung; der Rust-Core führt seine Prüfungen erneut aus.

## Vorhandene Bausteine und Änderungen

| Bestand | Weiterverwendung / Änderung |
| --- | --- |
| Pi `Agent`, `pi-ai`, Streaming | behalten; auf Task Service/Event Sink setzen, Limits und Abort hinzufügen |
| vorhandene Relay Read-/Diagnose-Core-Aktionen | behalten; Options-/Paket-Suche und Konfigurations-Read strukturiert anbinden |
| bestehende Plan-/Show-/Apply-/Undo-/Recover Core-Gates | behalten; Apply-Tool wartet auf lokale Task-Bestätigung und liefert Ergebnis an denselben Loop zurück |
| Router mit vorab erzwungenen Routen | nur als leichte Vorschlagsheuristik behalten; nicht vorab blockieren, solange kein Core-Schutz greift |
| `relay ask` | für einmaligen Fast Path behalten; interaktive Tasks rufen denselben Core an |
| Pi-TUI im `main.ts` | als CLI-Adapter entkoppeln; Agent-/Task-API liefert strukturierte Events |
| Change-Journal | unverändert maßgeblich; Task Store speichert nur Change-IDs/Belege |

## Quellen und API-Abgleich

Die Runtime nutzt die `Agent`-Klasse aus dem offiziellen Pi Agent Core: sie bietet stateful
`prompt()`, sequenzielle Toolausführung, Tool-Events, `abort()` und Kontext-Hooks. Relay benötigt
nicht die höherstufige Coding-Agent-ResourceLoader-Schicht. Versionen bleiben exact-pinned im
Lockfile; Upgrades verlangen API- und Sicherheitstest.

- <https://github.com/earendil-works/pi/blob/main/packages/agent/README.md>
- <https://pi.dev/docs/latest/sdk>
