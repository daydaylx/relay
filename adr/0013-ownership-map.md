# ADR 0013 – Konfigurations-Ownership Map

Status: Accepted for implementation

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

Ownership ist ein überprüfbarer SystemContext-Fakt mit `Unknown` als sicherem Default. Home Manager,
user dotfiles und NixOS bleiben getrennte Sources of Truth; bekannte Zustände werden nicht durch
Pi-Interpretation umklassifiziert.
