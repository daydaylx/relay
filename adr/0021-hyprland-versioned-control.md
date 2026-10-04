# ADR 0021 – Versionsgebundene Hyprland-Kontrolle

Status: Accepted for staged implementation

## Entscheidung

Hyprland Facts kommen bevorzugt aus `hyprctl -j` und dem zur installierten Version gehörenden
`hyprctl descriptions`. Adapter trennen lesende Monitors/Workspaces/Windows/Devices/Binds/Animations/
Errors, Runtime Dispatcher und persistente Konfiguration. Persistent changes gehen zuerst durch
Ownership Map zu NixOS, Home Manager oder User Config; nur bestätigte Runtime Actions verwenden IPC.

## Konsequenzen

IPC Actions erhalten strukturierte Parameter und dürfen kein beliebiges `dispatch` string passthrough
sein. Runtime State wird nach Action neu gelesen; Config Change bleibt in File Transaction oder Nix Core.
