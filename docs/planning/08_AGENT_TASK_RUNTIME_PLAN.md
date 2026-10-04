# Implementierungsplan: Agent Tasks mit Pi

Status: Architektur und erste Implementierung umgesetzt. Die konkrete Abdeckung offener Abnahmetests
und weitere Verifikationsziele stehen am Ende; die Liste unten dient zugleich als Nachweis, wo die
erste Implementierung vollständig oder nur teilweise reicht.

## Bestandsaufnahme (2026-10-03)

- Relay hat einen unabhängigen Rust-Core mit Plan, Candidate, Eval/Build, Preview, Risiko, Apply,
  Health, Journal, Undo und Recovery; diese Gates bleiben bestehen.
- `agent/` verwendet bereits Pi `Agent`, `pi-ai`, `pi-tui`, Provider-Streaming und sequenzielle Tools.
- Der Bridge/Core-Protocol kennt Status, Health, Units, Generationen, Diagnose, Plan, Show, Apply,
  Undo und Recovery. Diagnose umfasst Netzwerk, Bluetooth, Hardware, Prozesse, Journal und Desktop.
- Agent-Routing entscheidet vor dem Modell, welcher einzelne Diagnoseschritt passt. Eine Aufgabe
  lässt sich zwar von Pi über mehrere Modell-/Tool-Turns abarbeiten, aber Task-ID, Task-Persistenz,
  Event-Vertrag, kontextbegrenztes Resume und Task-Abschlussverifikation fehlen.
- Apply/Undo/Recovery sind TUI-Direktkommandos außerhalb der Pi-Toolausführung. Nach Apply kommt das
  Resultat nicht als Relay-Toolergebnis in denselben Reasoning-Loop zurück.
- Es fehlen Core-Tools für Options-/Paket-Suche als Agent-API und ein enges, policy-geprüftes
  `files.read_safe`. Kein unbeschränkter Shell-Runner wird eingeführt.
- `nix run .#agent` ist ein eigener Einstieg. Der Nutzereinstieg `relay` hat keinen Agentenmodus.
- Der Prototyp lädt bereits kein `~/.pi` und keine Projekt-Ressourcen. Diese Isolation bleibt eine
  harte Vorgabe: ausschließlich gelockte offizielle Pi-Pakete, keinerlei Zugriff auf das persönliche
  Pi-Setup des Nutzers.

## Umsetzungsstand

- **A – Architektur/Dokumentation:** umgesetzt in ADR 0010, Architektur 08 und dieser Planung.
- **B – Task/Runtime:** Task-Journal, zulässige Zustandsübergänge, Wiederaufnahme, lokale Events,
  begrenzte Kontextsynthese, Abbruch, Tool-/Turn-/Fehlerlimits und Pi Tool-Loop umgesetzt.
- **C – Lesen/Suchen:** Status, Health, Dienste, Diagnosebereiche, Options-/Paket-Suche und begrenztes
  Safe-Read umgesetzt. Vollständige Logs bleiben ausgeschlossen; Journaldiagnose liefert klassifizierte
  Fehlerarten statt Rohtext.
- **D – Änderung:** Plan/Review/Discard sowie im selben Task bestätigtes Apply/Undo/Recover umgesetzt;
  direkte TUI-Kommandos bestehen für Core-Bedienung weiter.
- **E – Nachprüfung/Einstieg:** `relay` startet den Agenten aus dem kombinierten Paket; Task kann nach
  fehlgeschlagener unterstützter Prüfung weiterarbeiten und wird nur durch passende strukturierte
  Core-Beobachtung abgeschlossen.
- **Persönliches Pi-Setup:** keine Laufzeitkopplung; Test deckt ab, dass sicherer Dateizugriff `~/.pi`
  nicht öffnet und Relay-Einstellungen aus Relays eigenem Konfigurationspfad kommen.

## Phasen

### A. Dokumentierter Vertrag

- ADR 0010, Agent-Task-Architektur, Task-Zustände, Toolkatalog, Grenzen, Bestand-zu-Ziel-Mapping und
  Abnahmekriterien festschreiben.
- Produkteinordnung aktualisieren: eigenständiges Relay mit ausgewählten eingebetteten Pi-Runtime-
  Komponenten; der Rust-Core bleibt eigenständig und sicherheitsgebend.

### B. Task-/Event-Grundlage

- Typisierte Task-/Event-Schemas, erlaubte Zustandsübergänge und Größenlimits.
- Atomarer lokaler Task Store, schema-versioniert, append-only, ohne Gesprächs-/Logrohtext und ohne
  Secrets; Restart-/Beschädigungsfälle behandeln.
- `TaskService` als UI-unabhängige API: `start`, `resume`, `submitGoal`, `confirm`, `cancel`, Events.
- Pi Agent Loop pro Task, iterativ, sequenziell, mit Turn-/Tool-/Zeitlimits, AbortSignal und
  `isError`-Behandlung. Gleicher Task-Kontext erhält Beobachtungen, Hypothesen und Change-IDs.

### C. Read-Tools und Kontext

- Core-Protokoll für `search_option`, `search_package`, `service_logs`, `configuration_summary` und
  begrenztes Safe-Read erweitern oder vorhandene CLIs sicher wiederverwenden.
- Read-Tools für Status, Health, Generationen, Services, Netzwerk, Bluetooth, Hardware, Prozesse,
  Desktop, Nix-Optionen/-Pakete und Konfigurationsübersicht als strukturierte Tool-Schemas.
- Secret-Filter sowohl im Core-Rückgabepfad als auch vor dem Task-/Modellkontext; keine vollständigen
  Logs, Stores oder private Werte.

### D. Mutations-Toolfluss

- `change.plan`, `change.show/preview`, `change.discard` und `change.apply` als Pi-Tools anbinden.
- Vor jeder Apply-Ausführung erzeugt Core Preview + Risk + Recovery-Information. Tool hält den
  aktuellen Turn an und wartet auf direkte TUI-Bestätigung zu Task-ID, Plan-ID und Review-Hash.
- Erst danach Core Apply. Keine Bestätigung aus natürlicher Sprache, Toolargumenten oder gespeicherten
  Modelltexten akzeptieren.
- Undo/Recover als Tool-Workflows mit Preview und jeweils direkter Zielbestätigung.

### E. Nachkontrolle und integrierter Einstieg

- Nach Apply strukturiertes Goal-Verification-Tool ausführen und an denselben Pi Task zurückgeben.
- Wenn nicht erreicht, `continuing` setzen und weitere Diagnose/Planrunde innerhalb des Budgets
  zulassen; bei geschützten oder widersprüchlichen Zuständen `blocked`.
- `relay`-CLI erhält den interaktiven Agenten-Einstieg und behält alle bestehenden Core-Kommandos.
- TUI wird Adapter auf strukturierte Events; GUI kann später dieselbe Task-Service-Schnittstelle
  konsumieren. Agent-Frontend bleibt optional paketierbar.

## Abnahmetests

- Agent: mehrere Diagnose-Tools bis zu einer Ursache; ungültige/malformed Toolcalls; Toolfehler;
  Wiederholungslimit; Max-Iterationen; Cancel; Provider nicht verfügbar; abgegrenzter Kontext.
- Security: modellseitige Shell-/`sudo`-Versuche können kein Tool auslösen; Secret-Datei/geschützte
  Pfade sind auch bei `files.read_safe` unerreichbar; protected Relay-Intent wird vom Core blockiert;
  Modellbehauptung kann Task nicht abschließen.
- Mutation: Goal → Untersuchung → Plan → Hash-gebundene Bestätigung → Core Apply → erneute
  Untersuchung → abgeschlossen.
- Fortsetzung: Goal → Apply → Verifikation schlägt fehl → Diagnose → zweiter Plan → Bestätigung →
  Verifikation erfolgreich.
- Fehlerpfade: Abbruch vor/nach Bestätigung, Crash/Restart während Apply, Source Drift, fehlendes
  Backend, Recovery aus dem bestehenden Core-Journal; keine doppelte Bestätigung wiederverwendbar.
- Alle Core-Mutations- und VM-Abnahmetests bleiben grün.

## Noch offen

- Live-TUI- und Providerlauf (bisher Fake-Streaming in Tests); begleiteter Usability-Pilot.
- Mehrziel-Verifikation, insbesondere MIME-Defaults/Dateimanager, Monitor- und Workspace-Ziele sowie
  Generationenbereinigung. Der Agent muss diese Ziele derzeit als nicht verifiziert offenlassen.
- Vollständige Fortsetzungs-/Recovery-Akzeptanz im Integrationstest: fehlgeschlagene Verifikation,
  zweiter Plan, erneute Bestätigung und erfolgreicher Abschluss; Restart mitten im Core-Apply verlangt
  aktuell manuellen Core-Recovery-Schritt.
- Fuzz-/Adversarial-Abdeckung malformed Tool-Aufrufe, shell-/sudo-Versuche als Modelltext, Toolfehler
  und Provider-Retry-Abbruch; existierende Grenzen/Fehlertests decken davon nur einen Teil ab.

## Annahmen und offene Implementierungsentscheidungen

- Task-Metadaten liegen standardmäßig lokal unter `$XDG_STATE_HOME/relay/tasks` (Fallback
  `~/.local/state/relay/tasks`) mit Verzeichnis-/Dateimodus nur für den Nutzer.
- Unverschlüsselte Task-Metadaten speichern nur strukturierte, redigierte Zusammenfassungen,
  Ereignisse und IDs; Modelltranskript und Rohdiagnose bleiben flüchtig. Spätere verschlüsselte
  Conversation-Persistenz ist out of scope.
- Netzwerk- und Hardware-Fakten werden über strukturierte Core-Adapter gelesen. `files.read_safe`
  erhält zunächst nur öffentliche NixOS-Metadaten und von Relay generierte, redigierte
  Konfigurationsausschnitte; beliebige Dotfiles/Secrets bleiben gesperrt.
- Der `relay`-Core erhält keine Node-Abhängigkeit. Das kombinierte Nix-Paket stellt Rust-Core plus
  optionale Agent-Runtime bereit; ein reiner Core-Output bleibt verfügbar.
