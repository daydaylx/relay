# Relay – NixOS-first System Control Layer

Relay ist ein eigenständiges lokales Systemwerkzeug für NixOS.

Relay ist ein intelligentes, AI-unterstütztes NixOS-Systemkontrollzentrum. Es verwendet Pi als
Reasoning- und Orchestrierungsschicht; Relay stellt Systemkontext, Ownership, Policies, Ausführung,
Recovery und Verifikation bereit. Relay ist **kein Pi-Fork**, Coding-Agent oder allgemeine Root-Shell
mit KI.

Ziel:

> Nutzerabsicht → strukturierte Relay-Aktion → NixOS-Konfiguration → Build/Preview/Test → Health Check → Switch oder Recovery.

## Kernprinzipien

- NixOS ist Source of Truth.
- Relay besitzt nur einen kleinen, klar abgegrenzten Managed-Bereich.
- Das LLM erzeugt strukturierte Absichten, keinen beliebigen Nix-Code.
- Jede Mutation wird vor Aktivierung evaluiert und gebaut.
- `dry-activate` ist Preview, kein Sicherheitsbeweis.
- `test` ist temporäre Aktivierung, kein automatischer Rollback.
- Source-State und Runtime-State werden gemeinsam versioniert.
- Stateful Daten werden separat betrachtet.
- AI und Agent-Runtime sind optional; der Rust-Core bleibt unabhängig davon verwendbar.
- Pi orchestriert Relay-Tools. Nur Relay Core darf Änderungen validieren und ausführen.
- Relay lädt oder untersucht kein persönliches Pi-Setup (`~/.pi`, Profile, Prompts, Extensions,
  Sessions oder Einstellungen). Agent-Konfiguration liegt separat unter `~/.config/relay/`.
- Relay selbst läuft nicht dauerhaft als root.

## MVP

Relay V1 kann zuverlässig:

1. Systemzustand anzeigen (`status`, `generations`, `health`).
2. NixOS-Optionen und Pakete lokal durchsuchen (`index-*`, `search-*`).
3. einfache Optionen kontrolliert setzen und Pakete hinzufügen/entfernen (`plan`).
4. eine isolierte Kandidatenkonfiguration erzeugen, evaluieren und bauen.
5. System-Closure vergleichen und Risiko klassifizieren.
6. `dry-activate` als Preview auswerten.
7. `test` + Health Check, dann `switch` bzw. `boot` (`apply`).
8. die letzte Relay-Änderung rückgängig machen – Source und Runtime gemeinsam (`undo`).
9. unterbrochene Änderungen nach einem Absturz erkennen und zurückrollen (`recover`).
10. optional: Wünsche in natürlicher Sprache in geprüfte, typisierte Vorschläge übersetzen (`ask`).
11. unter Hyprland Monitore, Workspaces und Kompositor-Gesundheit lesen und in die Sicherheitsprüfung einbeziehen (`desktop`).
12. optionale Pi-Agent-Runtime für mehrstufige Ziele mit Relay-Task-Journal, Diagnose-Tools,
    bestätigungsgebundenen Core-Änderungen und strukturierter Nachprüfung (`relay` oder
    `nix run .#agent`).

Die breitere Zielarchitektur ergänzt danach vollständige Ownership-Auflösung, transaktionale
User-Dateiänderungen sowie Relay-eigenes MCP/Web-Wissen. Pi RPC ist bereits der normale Task-Agent
und erhält Relay-eigene Tools über eine isolierte Extension und einen privaten Socket. Ein erster
verifizierter SystemContext wird bei Taskstart injiziert und journalisiert. Das Execution Gateway
hat nun einen ersten OBSERVE-Pfad: ausgewählte Diagnoseprogramme laufen in Bubblewrap mit begrenztem,
nicht beschreibbarem Nix-Closure, getrennten Namespaces, ohne Host-Netzwerk und ohne Zugriff auf
`/home`, `/etc`, `/run`, EFI-Variablen oder Cgroup-Sysfs. Der explizite Host-Smoke-Test prüft zusätzlich private PID-,
Umgebungs- und `/tmp`-Ansichten sowie blockierte Namespace- und Mount-Escapes. Das ist kein
vollständiger Kernel-Sandbox-Audit. User-Datei- und Root-Mutationen bleiben gesperrt. Ist-/Soll-Audit
und Migrationsphasen stehen in
[`docs/audits/RELAY_PI_CONTROL_CENTER_BASELINE.md`](docs/audits/RELAY_PI_CONTROL_CENTER_BASELINE.md)
und [`docs/planning/09_CONTROL_CENTER_MIGRATION.md`](docs/planning/09_CONTROL_CENTER_MIGRATION.md).

Relay schreibt automatisch nur in einen eigenen kontrollierten Bereich: `relay/managed.nix`.
Geschützte Ressourcen (`system.stateVersion`, Bootloader, Dateisysteme, LUKS, Nix-Trust, Auth/SSH,
Secrets, Datenbank-Major-Upgrades, …) werden im Code blockiert, nicht nur im Prompt.

## Schnellstart

```sh
nix develop                      # Rust-Toolchain (oder: nix run .# -- help)
cargo build --release

# einmalig: Managed-Modul anlegen und selbst importieren (Relay editiert keine anderen Dateien)
relay init --flake /etc/nixos
#   → ./relay/managed.nix in modules der Host-Konfiguration eintragen, `git add`, einmal
#     `sudo nixos-rebuild switch --flake .#host` ausführen

# planen: nichts am Live-System wird verändert
relay plan --flake /etc/nixos --host nixos set-option hardware.bluetooth.enable bool true
relay show <id>                  # Erklärung, Diff, Closure-Diff, Recovery-Plan

# anwenden: dry-activate → test → Health → switch   (fragt nach Bestätigung)
relay apply <id> --expect-active bluetooth.service

relay undo                       # Source + Runtime zurück
relay recover                    # nach Absturz/Stromausfall
relay context                    # verifizierten Systemzustand und Ownership anzeigen
relay context --json              # denselben SystemContext strukturiert ausgeben
```

Optional – natürliche Sprache (das Modell schlägt nur vor, Relay prüft, plant und fragt nach):

```sh
export RELAY_AI_PROVIDER=openai RELAY_AI_MODEL=gpt-4o-mini RELAY_AI_API_KEY_FILE=~/.config/relay/key
relay ask "Aktiviere Bluetooth" --host nixos --flake /etc/nixos --explain --apply
relay ask "Aktiviere Bluetooth" --host nixos --show-prompt   # zeigt, was gesendet würde; sendet nichts
```

Hyprland (nur lesend, in der Sitzung): `relay desktop status`, `relay desktop health`. Läuft eine
Sitzung, prüft `apply` zusätzlich Monitore und Kompositor und rollt bei Schäden zurück.

Beispiele und Ausgaben: [`examples/README.md`](examples/README.md). Entwicklung, Tests und
Teststrategie: [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md).

## Aktueller Stand

T1 bis T6 sind durch Core-, Simulator- und NixOS-VM-Tests belegt. T7 enthält nun die eingebettete
Pi-Agent-Runtime, persistierte Tasks, mehrstufige Tool-Aufrufe, lokale Mutationsbestätigung und
strukturierte Verifikation für unterstützte Ziele. Ein echter interaktiver Providerlauf und ein
Usability-Pilot sind offen. Aktuell kann die Task-Verifikation Bluetooth-Bereitschaft, einen
konkreten Dienst, Systemgesundheit und Paketverfügbarkeit prüfen; weitere Ziele wie MIME-Defaults,
Monitorlayout oder Generationenbereinigung sind noch nicht Ende-zu-Ende unterstützt. Details stehen
in [`PROJECT_STATUS.md`](PROJECT_STATUS.md).

## Installation auf NixOS

Die Flake baut Relay als Paket:

```sh
nix profile install /home/g/Projekte/Relay_NixOS_Project#default
```

Das Standardpaket enthält den Core und den optionalen Agent-Einstieg `relay`. Die Installation
ändert nur das Benutzerprofil, nicht die Systemkonfiguration. Für einen reinen Rust-Core ohne
Agent-Runtime gibt es `.#core`.

Der optionale Agent wird separat gebaut und gestartet. Das Nix-Paket bringt Node.js mit; für die
Nutzung muss Node nicht separat installiert sein:

```sh
nix run .#agent -- --init-config
nix run .#agent -- --pi-rpc-check
nix run .#agent
```

Für natürliche Sprache braucht der Agent einen konfigurierten Modellprovider. Er lädt kein
persönliches Pi-Profil und bietet dem Modell weder MCP noch allgemeine Datei-Schreibwerkzeuge.
Für unerwartete Diagnosefälle gibt es `relay_observe_command`: strukturierte Programme und Argumente
laufen in einer read-only Bubblewrap-Sandbox. Dafür müssen Bubblewrap aus dem Relay-Paket und ein
laufender User-systemd-Manager verfügbar sein; andernfalls wird der Aufruf verweigert.
`--pi-rpc-check` prüft den isolierten Pi-RPC-Prozess mit Relay-eigenem HOME, XDG-, Konfigurations-
und Sessionpfad; es sendet keinen Modellprompt. Die normale interaktive Runtime startet pro Task
eine eigene Pi-RPC-Session und lädt ausschließlich Relay-Tools aus einer privaten Task-Erweiterung.
Alle Pi-Pakete sind
im Repository fixiert; Provider-Einstellungen stehen in Relays eigener Konfiguration. Diagnose,
Änderungsvorschlag und Nachprüfung laufen als
Task. Apply, Undo und Recovery erfordern direkte, an die konkrete Core-Vorschau gebundene
Bestätigung. Bash-/Dateimutationen auf dem Host, User-Dateitransaktionen, MCP und Webzugriff sind
noch nicht aktiviert. Toolumfang und bekannte Grenzen stehen in
[`agent/README.md`](agent/README.md) und [`docs/audits/SYSTEM_AGENT_V1_RESULT.md`](docs/audits/SYSTEM_AGENT_V1_RESULT.md).

## Projektführung

Für die Umsetzung zuerst lesen:

1. `AGENTS.md`
2. `PROJECT_STATUS.md`
3. `docs/planning/04_TARGET_STATES.md`
4. `docs/planning/02_WORK_ORDER.md`
5. `docs/planning/05_DEFINITION_OF_READY_DONE.md`
6. `docs/planning/06_REPO_SETUP.md`
7. `docs/security/01_SECURITY_MODEL.md`
8. `docs/security/02_RISK_REGISTER.md`
9. `adr/` (insbesondere 0005 und 0006)

Repo-Templates für Issues und Pull Requests liegen unter `.github/`.

## Lizenz

MIT, siehe [`LICENSE`](LICENSE).
