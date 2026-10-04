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

## Pi Control Center (Zielarchitektur, gestufte Umsetzung)

Die erste Pi-Task-Runtime hat keine allgemeine Shell, keine User-Datei-Transaktionen, kein MCP und
keinen Webzugriff. Der erweiterte Auftrag fügt diese Fähigkeiten nicht unmittelbar frei, sondern
ordnet sie einem gemeinsamen Rust Execution Gateway mit Relay-eigener SystemContext-, Ownership-,
Policy- und Evidence-Schicht zu. Der [Baseline-Audit](../audits/RELAY_PI_CONTROL_CENTER_BASELINE.md)
und ADR 0011–0022 sind Soll-Architektur; einzelne Fähigkeiten gelten erst nach ihrer Phase und ihren
Security Tests als verfügbar.

- Pi RPC ist eine Prozess-/API-Grenze, keine OS-Sandbox. Eingebaute Hosttools und MCP-Server werden
  vor echter Sandbox-/Gateway-Prüfung nicht für allgemeine Host-Mutationen aktiviert.
- Shells/Interpreter werden nicht anhand einer simplen Executable-Whitelist autorisiert. Policy
  bewertet argv, cwd, env, redirections, file descriptors, resolved targets, ownership, privilege,
  scope, erwartete Wirkung und Rücknehmbarkeit; Enforcement erfolgt zusätzlich per Linux OS-Grenze.
- Managed Nix mutations bleiben exklusiv im bisherigen Candidate-/Safety-Core. Dateiänderungen laufen
  nur nach Ownership-Auflösung durch File Transactions; Undo prüft After-Hash und schützt fremde Edits.
- Root bleibt kurzlebig und operation-gebunden. Task-Grants erlauben nur freigegebene Klassen; Root,
  unknown, secret und destructive actions verlangen eigene direkte Entscheidung oder bleiben blockiert.
- Relay lädt niemals automatisch `~/.pi`, User-/Projektprompts, Pi Skills/Extensions/Packages/MCP oder
  Pi Sessions. Relays eigene Runtime-/Session-/MCP-Pfade werden explizit isoliert.
- Web-/MCP-/GitHub-/Datei-/Log-Antworten sind untrusted Daten. Fetch hat öffentliche HTTP(S)-Ziele,
  SSRF-Schutz und Limits; Credentials/Cookies gehen nicht in Knowledge-Abfragen.
- Keine Task wird anhand eines Exitcodes oder Modelltexts abgeschlossen. Desired State wird mit frischen,
  strukturierten Observations verglichen und Evidence/Action-IDs belegen die Resultate.
