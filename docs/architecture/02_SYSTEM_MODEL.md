# Systemmodell

Relay unterscheidet vier Zustände.

## Source State

Konfiguration wie Flake, Lockfile, Host-Modul, Hardware-Konfiguration und Relay Managed Config.

## Evaluated State

Der effektive Wert nach Zusammenführung aller NixOS-Module.

## Built / Deployed State

Gebauter System-Store-Pfad und Generation.

## Runtime State

Kernel, systemd, Hardware, Netzwerk, Desktop und laufende Dienste.

## Grundregel

Relay darf Source, Evaluation, Deployment und Runtime niemals vermischen.
