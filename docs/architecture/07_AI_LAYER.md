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
