# Optionale AI Layer

Relay muss ohne AI funktionieren.

AI übersetzt natürliche Sprache nur in strukturierte Relay-Aktionen.

Beispiel:

```text
User:
"Aktiviere Bluetooth."

AI:
SetOption {
  option: "hardware.bluetooth.enable",
  value: true
}
```

Danach übernimmt ausschließlich Relay.

Kein direkter Root-Zugriff, keine generische Shell, keine direkte Nix-Dateibearbeitung durch das Modell.

## Umsetzung (T5)

`relay ask "<Anfrage>" --host HOST [--flake PATH] --provider command|openai|anthropic …`

```text
Anfrage → Hinweise aus dem lokalen Index → Prompt (deterministisch, ohne Werte/Pfade/Secrets)
→ Provider → Antwort muss EIN bares (oder einmal eingezäuntes) JSON-Objekt sein
→ strikte Schema-Validierung + Schutzliste → Gegenprüfung gegen Options-/Paketindex
→ plan (isolierter Kandidat, echte Evaluation, Build, Review) → Mensch bestätigt → apply
```

- Aktionen, die ein Modell vorschlagen darf: Änderungen, `undo`, `recover`, `status`, `history`,
  `unsupported`. `undo`/`recover`/`--apply` verlangen eine getippte Bestätigung; `--yes` wird
  abgelehnt. Siehe ADR 0007.
- `--show-prompt` druckt exakt, was ein Provider erhielte, und sendet nichts. `--explain` fügt eine
  vom Provider geschriebene Erklärung des deterministischen Reviews hinzu (als nicht maßgeblich
  gekennzeichnet).
- Konfiguration: `RELAY_AI_PROVIDER`, `RELAY_AI_MODEL`, `RELAY_AI_BASE_URL`, `RELAY_AI_COMMAND`,
  `RELAY_AI_API_KEY` bzw. `RELAY_AI_API_KEY_FILE` (der Schlüssel nie als Argument).
- Ohne Provider, ohne Netz oder bei Fehlern ändert sich nichts; alle anderen Befehle sind
  unberührt.

## Eingebettete Pi Agent Runtime (optional)

Relay bettet ausgewählte, exakt versionierte Pakete von Pi für Agent Loop, Provider/Streaming und
TUI ein. Das ist eine Runtime-Komponente von Relay, kein Zugriff auf ein separates Pi-Produkt. Sie
lädt, liest, kopiert oder untersucht keine persönliche Pi-Installation, `~/.pi`, Profile, Prompts,
Extensions, Sessions, Einstellungen oder Credentials. Relay-Konfiguration liegt separat unter
`~/.config/relay/agent.json`. Der Rust-Core bleibt ohne Node, Pi und Provider vollständig nutzbar.

Interaktive Nutzereingaben erzeugen persistierte Relay Tasks. Innerhalb eines Tasks ruft ein einzelner
Pi Loop nacheinander strukturierte Relay-Tools auf: Beobachtung und Diagnose, Nix-Option-/Paket-Suche,
begrenztes sicheres Lesen, typed Plan, Review, Discard, bestätigtes Apply/Undo/Recover und
Goal-Verifikation. Beobachtung und Planung brauchen keine Rückfrage. Mutation pausiert den Toolaufruf,
zeigt die Core-Vorschau lokal und benötigt eine direkte Bestätigung, die an Task, Aktion und Preview
gebunden ist. Nach Apply kann derselbe Task weiterarbeiten. Nur eine passende strukturierte Core-
Beobachtung kann den Task abschließen.

Task-Ereignisse sind lokal append-only; Transkript und vollständige Logs werden nicht persistiert.
Der Router liefert nur einen ersten Topic-Hinweis und ist keine Berechtigungsgrenze. Schreibschutz,
Protected Resources, Drift, Risk, Recovery und Aktivierung bleiben Entscheidungen des Relay Core.
Es gibt keine allgemeine Shell, keine willkürlichen Schreib- oder Root-Tools, Extensions, MCP,
Codemode oder Subagenten. Details und Grenzen stehen in
[`08_AGENT_TASK_RUNTIME.md`](08_AGENT_TASK_RUNTIME.md) und [`agent/README.md`](../../agent/README.md).

Aktuell ist strukturierte Zielverifikation für Bluetooth-Bereitschaft, konkret benannte Dienste,
Systemgesundheit und Paketverfügbarkeit implementiert. Ziele wie MIME-Defaults, Monitor-/Workspace-
Layout oder Generationenbereinigung sind noch nicht verifizierbar und bleiben offen.
