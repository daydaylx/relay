# Config Backends

## V1: Flake-first

Gründe:

- flake.lock
- eindeutige nixpkgs Revision
- reproduzierbare Inputs
- spätere Drittanbieter-Module
- maschinenlesbare Metadaten

## Später: system.nix

NixOS 26.05 führt `system.nix` als offiziellen gepinnten Einstiegspunkt ein. Relay soll das später als zweites Backend unterstützen.

## Git-Falle

Candidate Evaluation soll explizite lokale Pfade verwenden, damit neue untracked Dateien nicht versehentlich durch Git-Flake-Semantik fehlen.
