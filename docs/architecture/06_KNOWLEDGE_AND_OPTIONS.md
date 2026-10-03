# Knowledge Layer

## System Knowledge

- NixOS-Version
- Kernel
- Host
- aktive Generation
- System Store Path
- Desktop
- Services
- Hardware
- nixpkgs Revision

## Versioned Knowledge

- NixOS Options
- NixOS Manual
- Nix Manual
- Nixpkgs Manual
- Package Index

## External Knowledge

Nur bei Bedarf für Bugs, Issues, Hardwareprobleme oder Release Notes.

## Options Index

Mindestens:

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

Cache wird an Host, nixpkgs-Revision, Lockfile und Config-Hash gebunden.

## T1 Index-Quelle und Erzeugung

Der kanonische Datenbestand ist das lokal verfügbare `nixpkgs`, das über den
Lock-Eintrag des Host-Flakes festgelegt ist. Relay darf für Index-Aufbau und
Suche weder einen ungebundenen Flake-Registry-Eintrag noch implizite Netzwerk-
oder `nix search`-Auflösung verwenden.

- **Optionen:** Metadaten aus der ausgewerteten Optionsstruktur der
  Host-Konfiguration ableiten. Der Index enthält Beschreibungen, Typen,
  Beispiele und Deklarationspfade, aber keine aktuellen Konfigurationswerte.
  `default` bleibt vorerst `null`, weil das Erzwingen beliebiger NixOS-Defaults
  zusätzliche Host-Auswertung auslösen und bei einzelnen Optionen fehlschlagen
  kann. Nicht verfügbare Beschreibungen bleiben leer; `relatedPackages` wird
  nicht geraten.
- **Pakete:** Namen und beschreibende Metadaten aus dem `pkgs`-Attributsatz
  desselben gepinnten `nixpkgs` beziehen. Der Index-Aufbau darf keine
  Paketderivationen bauen. Ein Name ist nur ein Suchtreffer, keine Zusage, dass
  das Paket für Host/System verfügbar oder installierbar ist; das bestätigt die
  spätere Candidate-Evaluation.
- **Transport:** Nur strukturierte JSON-Ausgabe wird eingelesen. Nix-Aufrufe
  bleiben im Nix-Adapter. Index-Dateien sind lokaler, regenerierbarer Cache und
  kein Source of Truth.
- **Identität:** Jeder Index wird mit Host-Identität, `nixpkgs`-Lock-Revision,
  Lockfile-Hash, relevanter Konfigurationsidentität, Zielsystem und
  Index-Schema-Version markiert. Bei fehlender oder abweichender Identität wird
  der Cache verworfen und nicht stillschweigend wiederverwendet.
- **Validierung:** Der Generator gibt schema-v1-JSON aus. Schema- und
  Identitätstests decken fehlerhafte Daten und veraltete Cache-Felder ab. Die
  Generatoren `index-options` und `index-packages` schreiben den geprüften
  Cache atomar an einen expliziten Ausgabepfad. Die Suchbefehle verlangen den
  gleichen lokalen Flake und Host, werten die aktuelle Identität erneut aus und
  verweigern bei Abweichungen die Suche.

Der CLI-Index-Leser akzeptiert aktuell nur explizit über `--index PATH`
übergebene JSON-Dateien. Das Schema ist Version 1 und bindet einen Index mit
`kind` (`option` oder `package`), `host`, `nixpkgsRevision`, `lockfileHash`,
`configIdentity`, `targetSystem` und `entries`. Options-Einträge enthalten
`name`, `type`, `default`, `description`, `example`, `declarations`,
`readOnly` und `relatedPackages`; Paket-Einträge mindestens `name` und
`description`. Die Suche gleicht den Begriff ohne Beachtung der Groß-/Kleinschreibung
gegen den Namen ab.

Die CLI prüft Syntax, Schema, Flake-Host, nixpkgs-Revision, Lockfile-Hash,
Konfigurationsidentität und Zielsystem vor jeder Suche. Nix-Auswertung bleibt im
Nix-Adapter und baut keine Paketderivationen. Fehlerhafte Paket-Metadaten einzelner
Attribute werden ausgelassen. Die Generatoren wurden mit Nix 2.34.8 und dem
NixOS-Host `nixos` (nixpkgs `4feb8eb`) ausgeführt; `default` bleibt wie oben
beschrieben leer.
