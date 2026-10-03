# ADR 0007 – Optionale KI-Schicht: das Modell schlägt vor, der Kern entscheidet

Status: Accepted

## Kontext

ADR 0004 macht KI optional. Zielzustand T5 verlangt, dass ChatGPT oder ein anderer Provider
Relay bedienen kann, ohne Sicherheitslogik zu besitzen, und dass dieselben T4-Funktionen weiterhin
ohne KI per CLI laufen.

## Entscheidung

- **Vorschlag, nie Ausführung.** Ein Modell erzeugt ein einziges strikt validiertes JSON-Dokument
  (`intent.rs`): entweder Änderungen oder genau eine Aktion (`undo`, `recover`, `status`,
  `history`) oder `unsupported` mit kurzer Begründung. Alles andere wird abgelehnt, nicht repariert.
  Es gibt kein Nix, keine Pfade, keine Shell-Kommandos im Schema.
- **Die Prüfkette gehört dem Kern.** Schema → Schutzliste (Code) → Gegenprüfung gegen den lokalen
  Options-/Paketindex (erfundene Namen, schreibgeschützte Optionen, unpassende Wertetypen) →
  isolierter Kandidat → echte Evaluation und Build → Review. Das Modell kann keinen Schritt davon
  überspringen oder abschwächen.
- **Bestätigung bleibt menschlich.** `relay ask` plant nur. `--apply`, `undo` und `recover` aus
  einem Vorschlag verlangen eine getippte Bestätigung; `--yes` wird von `ask` abgelehnt.
- **Provider sind Adapter ohne Sicherheitslogik:**
  `command` (beliebiges Programm: Prompt auf stdin, Antwort auf stdout, z. B. ein Skript um ein
  lokales Modell), `openai` (Chat-Completions-kompatibel, auch lokale Server wie Ollama) und
  `anthropic`. Die HTTP-Provider rufen das System-`curl` auf; der API-Schlüssel kommt nur aus
  `RELAY_AI_API_KEY`/`RELAY_AI_API_KEY_FILE`, wird als curl-Konfiguration über stdin übergeben
  (nie in Argumenten, Journal oder Logs) und der Debug-Output redigiert ihn. Klartext-HTTP ist nur
  für Loopback erlaubt. Zeitlimit 60 s; ein nicht erreichbarer Provider ändert nichts.
- **Was das Modell sieht** (`relay ask --show-prompt` zeigt es exakt): Anfragetext, Schemaregeln,
  Hostname und wenige Index-Einträge (Name, Typ, Beschreibung). Keine Konfigurationswerte,
  Dateiinhalte, Pfade oder Zugangsdaten. `--explain` sendet zusätzlich das deterministische Review
  (Diff, Closure-Diff, Store-Pfade) und kennzeichnet die Antwort als nicht maßgeblich.
- **Audit.** Zu jedem aus einem Vorschlag entstandenen Plan liegt `ai.json` (Provider, Anfrage,
  Rohantwort) privat (0600) im Änderungsverzeichnis; das Journal bleibt wertefrei.

## Folgen

- Prompt-Injection kann höchstens zu einem schema-gültigen Vorschlag führen, der alle Prüfungen
  durchlaufen und von einem Menschen bestätigt werden muss.
- Die HTTP-Provider sind gegen eine Fake-`curl`-Schnittstelle getestet (Argumente, Konfiguration,
  Antwortformen, Fehlerpfade), nicht gegen die Live-APIs.
