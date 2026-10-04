# ADR 0013 – Konfigurations-Ownership Map

Status: Accepted; conservative metadata-only path resolver implemented, source graph pending

## Entscheidung

Jeder relevante Config-/Runtime-Knoten wird `NIXOS`, `HOME_MANAGER`, `USER_CONFIG`, `GENERATED`,
`RUNTIME_ONLY`, `EXTERNAL` oder `UNKNOWN` zugeordnet. Kanten enthalten die beobachtete Quelle, Hash,
Zeit, Konfidenz und Evidence. Der Resolver bevorzugt Nix-Evaluation/Flake-Imports, Home Manager
Evaluation/Store-Link-Metadaten, Symlink-Ziele und App-/Desktop-Adapter; Dateiname oder Modellmeinung
allein erzeugt keine Ownership.

Bei unbekannter oder widersprüchlicher Ownership darf Pi lesen/diagnostizieren und nachweisen, aber
nicht den persistenten Pfad mutieren. Generierte Dateien werden nie direkt editiert. Relay verwaltet
weiterhin ausschließlich `relay/managed.nix`, bis ein eigener ADR den Nix-Write-Scope erweitert.

## Konsequenzen

Ownership ist ein überprüfbarer SystemContext-Fakt mit `Unknown` als sicherem Default. Der erste
Resolver liest ausschließlich Pfad-/Symlink-/Dateityp-Metadaten und blockiert Secret-/Pi-/fremde
Pfade. Er erkennt Relay-managed und NixOS-Quellen sowie direkte User-Config, Runtime, Store-Outputs
und Home-Manager-Pfade anhand eines Output-Markers. Das ist noch keine Source-of-Truth-Erkennung:
Flake-Import- und Home-Manager-Evaluation sowie ein Evidence-Graph folgen separat. Home Manager,
user dotfiles und NixOS bleiben getrennte Sources of Truth; bekannte Zustände werden nicht durch
Pi-Interpretation umklassifiziert.
