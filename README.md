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

Relay V1 soll zuverlässig können:

1. Systemzustand anzeigen.
2. NixOS-Optionen lokal durchsuchen.
3. einfache Optionen kontrolliert setzen.
4. Pakete hinzufügen/entfernen.
5. Kandidatenkonfiguration erzeugen.
6. evaluieren und bauen.
7. System-Closure vergleichen.
8. `dry-activate` auswerten.
9. `test` + Health Check.
10. `switch` bzw. `boot`.
11. letzte Relay-Änderung rückgängig machen.

## Wichtige Grenze

Relay soll nicht jede beliebige Nix-Datei automatisch umschreiben.

Der MVP schreibt nur in einen eigenen kontrollierten Bereich, z. B. `relay/managed.nix`.


## Aktueller Implementierungsstand

Ein dependency-freies Rust-Workspace mit read-only `status` / `generations` sowie Suche in explizit bereitgestellten JSON-Indizes (`search-option` / `search-package`) liegt vor. Die Suche validiert das Index-Schema, aber nicht dessen Frische oder Identität zum laufenden Host; ein Generator ist noch nicht aktiviert. Details und Indexformat: [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md).

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

Repo-Templates für Issues und Pull Requests liegen unter `.github/`.
