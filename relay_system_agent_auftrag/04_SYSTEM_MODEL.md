# 04 – System Inventory und Systemmodell

## Ziel

Der Agent soll nicht bei jeder Aufgabe das komplette System neu entdecken.

Er benötigt ein lokales, aktualisierbares und maschinenlesbares Systemmodell.

## Inventory V1

Mindestens erfassen:

### Basis
- NixOS-Version
- Kernel
- Architektur
- Hostname
- aktuelle Systemgeneration
- Bootgeneration
- aktueller Store-Systempfad

### Konfiguration
- Flake-Pfad
- Flake-Host
- relevante Module
- Relay managed module
- Git-Status der Config
- vorhandene unapplied changes

### Desktop
- Wayland/X11
- Hyprland erreichbar?
- Monitore
- Workspaces
- relevante Config-Pfade

### Services
- failed units
- aktive relevante Dienste
- enabled/disabled soweit strukturiert ermittelbar

### Hardware
- CPU
- GPU
- Netzwerkadapter
- Bluetoothadapter
- relevante Blockdevices nur lesend

### User Layer
- Home Manager vorhanden?
- Konfigurationspfad
- User-Systemd vorhanden?

## Speicherung

Nicht alles dauerhaft speichern.

Trennen:

```text
Static-ish facts
Runtime snapshot
Derived facts
Sensitive/forbidden facts
```

Passwörter, Tokens, private keys und Secret-Inhalte gehören nie in das Modell.

## Aktualisierung

Inventory muss:

- manuell refreshbar sein,
- vor riskanten Änderungen relevante Teile frisch lesen,
- nach Änderungen betroffene Teile erneut erfassen,
- stale Daten markieren.

## Context Retrieval

Nicht das komplette Inventory in jeden Modellprompt kippen.

Nur aufgabenrelevante Ausschnitte bereitstellen.
