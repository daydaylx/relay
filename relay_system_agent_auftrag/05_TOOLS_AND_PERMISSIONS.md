# 05 – Tools und Permissions

## Grundsatz

Systemdiagnose braucht breiten Lesezugriff. Schreibrechte müssen deutlich enger sein.

```text
READ: breit, aber secret-aware
WRITE: backendgebunden
ROOT WRITE: nur deterministisch / bestätigt / recoverbar
```

## Read Tools V1

Eigene strukturierte Tools bevorzugen gegenüber freiem Bash:

- system_info
- list_units
- unit_status
- journal_query
- process_status
- network_status
- bluetooth_status
- nix_generations
- nix_config_status
- file_read
- file_find
- hyprland_status
- disk_status_readonly

Toolantworten möglichst strukturiert zurückgeben.

## Shell

Keine generische Root-Shell für das Modell.

Eine unprivilegierte Diagnose-Shell ist nur dann zulässig, wenn:

- Argumente sichtbar sind,
- Timeout besteht,
- Output begrenzt wird,
- gefährliche Interpreter-/Redirection-Ketten kontrolliert werden,
- Secret-Pfade geschützt sind.

Bevorzugt werden spezialisierte Tools.

## Sudo

Das Modell darf kein allgemeines:

```text
sudo <beliebiger Befehl>
```

erhalten.

Privilegierte Änderungen laufen über Relay Core bzw. später über explizit definierte
privilegierte Adapter.

## Bestätigung

Kategorien definieren:

- READ_ONLY → keine Bestätigung
- SAFE_USER_CHANGE → je nach Einstellung
- SYSTEM_CHANGE → Vorschau + Bestätigung
- REBOOT → explizite Bestätigung
- PROTECTED → keine automatische Ausführung
- UNKNOWN → verweigern bzw. Plan ausgeben

Die Sicherheitsklassifikation muss technisch erzwungen werden, nicht nur per Prompt.
