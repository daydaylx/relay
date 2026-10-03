# Relay – NixOS-first System Control Layer

Relay ist ein eigenständiges lokales Systemwerkzeug für NixOS.

Es ist **kein Coding-Agent**, kein Pi-Fork und keine allgemeine Root-Shell mit KI.

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
- AI ist optional.
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
12. optional mit dem separaten Original-Pi-Agenten natürlichsprachlich inspizieren, planen und
    bestätigte Relay-Änderungen ausführen (`nix run .#agent`).

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

Die Zielzustände T1 (Observer), T2 (Candidate Builder), T3 (Activator), T4 (Recovery), T5
(optionale KI-Schicht) und T6 (Hyprland, read-only) sind implementiert und durch Simulator-Tests,
echte Nix-Läufe, eine laufende Hyprland-Sitzung und einen NixOS-VM-Test belegt. Der optionale
Pi-Agent (T7) ist implementiert; interaktive Provider-Nutzung und Usability sind noch nicht live
erprobt. Der aktuelle Hoststatus und alle offenen Punkte stehen in [`PROJECT_STATUS.md`](PROJECT_STATUS.md).

## Installation auf NixOS

Die Flake baut Relay als Paket:

```sh
nix profile install /home/g/Projekte/Relay_NixOS_Project#default
```

Die Installation ändert nur das Benutzerprofil, nicht die Systemkonfiguration.

Der optionale Agent wird separat gebaut und gestartet. Das Nix-Paket bringt Node.js mit; für die
Nutzung muss Node nicht separat installiert sein:

```sh
nix run .#agent -- --init-config
nix run .#agent
```

Für natürliche Sprache braucht der Agent einen konfigurierten Modellprovider. Er lädt kein
persönliches Pi-Profil und bietet dem Modell weder MCP noch allgemeine Datei- oder Shell-Werkzeuge.
Seine feste Werkzeugliste umfasst Status, Health, begrenzte Dienst- und Themendiagnosen sowie
Änderungsplanung und Planansicht. Apply, Undo und Recovery erfordern direkte, zielgebundene Bestätigung durch
den Nutzer. Toolumfang, Bestätigung und offene Pilot-/Diagnosegrenzen stehen in
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
