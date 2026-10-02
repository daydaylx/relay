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
