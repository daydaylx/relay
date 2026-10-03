# Security Model

Das LLM ist niemals die Security Boundary.

Relay läuft normal unprivilegiert.

Privilegierung nur für konkrete Aktivierungsaktionen.

Kein dauerhaftes Root-Relay.
Keine generische Root-Shell.

## Protected im MVP

```text
system.stateVersion
partitioning
filesystems
LUKS
bootloader
secure boot
nix daemon trust
trusted-users
fundamentale auth/user changes
SSH access foundation
secret management
database major upgrades
major nixpkgs release upgrades
```

Secrets dürfen nicht in Flake, managed.nix, Journal, AI Context oder Nix Store gelangen.

Switch Inhibitors dürfen nicht automatisch umgangen werden.

## KI-Schicht und Datenabfluss (T5)

- Das Modell erhält nur den Prompt, den `relay ask --show-prompt` zeigt: Anfragetext, Schemaregeln,
  Hostname und wenige Index-Einträge. Keine Konfigurationswerte, Dateiinhalte, Pfade, Journal-
  Inhalte oder Zugangsdaten. `--explain` sendet zusätzlich das Review (Diff, Store-Pfade).
- Secrets gehören nicht in Anfragen. Der API-Schlüssel kommt nur aus der Umgebung/Schlüsseldatei,
  geht per stdin an `curl` und erscheint weder in Argumenten noch in Logs noch im Journal.
- Modellausgabe ist nicht vertrauenswürdig: strikte Validierung, Schutzliste im Code, Gegenprüfung
  gegen den lokalen Index, echte Evaluation, menschliche Bestätigung.

## Desktop (T6)

- Der Zugriff auf den Kompositor ist rein lesend und auf eine feste Abfrageliste beschränkt;
  Fenstertitel werden nicht gelesen.
