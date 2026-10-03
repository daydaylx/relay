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

## Interaktiver Pi Agent (optional)

`nix run .#agent` startet einen eigenen TypeScript-Prozess mit Original-Pi `pi-agent-core`, `pi-ai`
und `pi-tui`. Er liest weder `~/.pi` noch Projektprompts oder Extensions. Die eigene Konfiguration
liegt in `~/.config/relay/agent.json`. Der Rust-Core bleibt ohne Node und Pi vollständig nutzbar.

Der Agent spricht mit `relay protocol --stdio` über schema-versionierte JSON-Zeilen. Seine
Modellwerkzeuge sind `relay_system_status`, `relay_system_health`, `relay_list_units`,
`relay_plan_change` und `relay_show_plan`. Vollständige Preview-Werte werden lokal in der TUI
gezeigt; dem Modell werden Diffs, Planwerte und Store-Pfade nicht zurückgegeben. Der Router ordnet
Anfragen vor dem Modellaufruf ein und hält bekannte geschützte sowie noch nicht unterstützte
Backend-Anfragen lokal an. Core-Validierung bleibt unabhängig vom Router.

Apply, Undo und Recover können Nutzer in der TUI direkt starten. Diese Aktionen sind keine
Modellwerkzeuge und benötigen eine exakte, zielgebundene Bestätigung. Der Core prüft das Ziel unter
seiner Änderungssperre erneut. Die Diagnoseoberfläche deckt bislang Status, Health und eine
begrenzte systemd-Dienstliste ab; Journal-, Netzwerk-, Bluetooth-, Hardware- und Kontextabfragen
sind noch offen. Siehe [`SYSTEM_AGENT_V1_RESULT.md`](../audits/SYSTEM_AGENT_V1_RESULT.md).
