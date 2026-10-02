# Arbeitsauftrag – Relay MVP

## Ziel

Baue Relay als eigenständiges NixOS-Systemwerkzeug.

Kein Pi.
Kein Coding-Agent-Core.
Kein generischer Root-Agent.

## Phase 1 – System Discovery

Read-only:

```text
system.summary
system.health
system.generations
system.runningPath
system.bootedGeneration
config.backend
config.identity
```

## Phase 2 – Nix Adapter

Alle Nix-/nixos-rebuild-Aufrufe in einem Adapter kapseln.

Strukturierte Ausgaben bevorzugen.

## Phase 3 – Options Index

Host-spezifischer lokaler Index mit:

```text
name
type
default
description
example
declarations
readOnly
relatedPackages
```

## Phase 4 – Managed Config

V1 unterstützt:

- bool
- integer
- string
- enum
- einfache Listen
- einfache Attrsets
- environment.systemPackages

Keine arbitrary Nix expressions.

## Phase 5 – Candidate Engine

```text
evaluate
build
capture output path
diff closures
dry-activate preview
```

## Phase 6 – Risk Engine

Klassifiziere:

```text
LIVE_SWITCHABLE
REBOOT_REQUIRED
MIGRATION_REQUIRED
PROTECTED
```

## Phase 7 – Activation

LIVE_SWITCHABLE:

```text
test → health → switch
```

REBOOT_REQUIRED:

```text
boot → reboot → post-boot health
```

## Phase 8 – Recovery

Source + Runtime gemeinsam versionieren und rücksetzen.

## Phase 9 – AI optional

AI darf nur strukturierte Intents erzeugen.

Keine direkte Nix-Dateibearbeitung.
Keine direkte Shell.

## Pflicht-MVP-Szenarien

1. Systemzustand anzeigen
2. Option für Bluetooth finden
3. Bluetooth aktivieren
4. test + Health
5. switch
6. letzte Änderung rückgängig machen
7. VLC suchen
8. VLC hinzufügen
9. VLC entfernen

## Nicht im MVP

- Home Manager
- Secret Management
- Partitionierung
- LUKS
- Bootloader
- system.stateVersion
- Datenbankmigrationen
- Major NixOS Upgrade
- Subagenten
- MCP
- Remote Management

Schwierigkeiten: 8/10 | Thinking: high
