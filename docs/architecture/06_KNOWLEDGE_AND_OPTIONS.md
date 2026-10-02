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

- **Optionen:** Metadaten aus der Auswertung der NixOS-Moduloptionen dieser
  Host-Konfiguration ableiten. Die NixOS-Optionsdokumentation
  (`nixosOptionsDoc`) ist die Referenz für die verfügbaren Dokumentationsfelder;
  Relay normalisiert sie in ein eigenes versioniertes JSON-Schema. Der Index
  enthält nur Optionsmetadaten, keine aktuellen Konfigurationswerte oder
  ausgewerteten Secret-Inhalte. Nicht verfügbare Felder bleiben explizit leer;
  `relatedPackages` wird nicht geraten.
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
- **Validierung:** Der Generator muss seine konkrete JSON-Schnittstelle auf der
  unterstützten NixOS-/nixpkgs-Version nachweisen. Fixtures und Schema-Tests
  decken Normalisierung, fehlende Felder, fehlerhafte JSON-Daten und veraltete
  Cache-Identität ab. Bis diese Prüfung vorliegt, bleiben
  `search-option`/`search-package` nicht verfügbar.

Der CLI-Index-Leser akzeptiert aktuell nur explizit über `--index PATH`
übergebene JSON-Dateien. Das Schema ist Version 1 und bindet einen Index mit
`kind` (`option` oder `package`), `host`, `nixpkgsRevision`, `lockfileHash`,
`configIdentity`, `targetSystem` und `entries`. Options-Einträge enthalten
`name`, `type`, `default`, `description`, `example`, `declarations`,
`readOnly` und `relatedPackages`; Paket-Einträge mindestens `name` und
`description`. Die Suche gleicht den Begriff ohne Beachtung der Groß-/Kleinschreibung
gegen den Namen ab.

Der Leser prüft Syntax und Schema, verifiziert aber weder Lockfile-/Config-Hash
noch Host-Identität oder Aktualität; die CLI gibt dafür eine Warnung aus. Er
führt keine Nix-Befehle aus und erzeugt keinen Index. Die konkrete CLI-Form des
`nixosOptionsDoc`-Aufrufs und die stabile Paket-Metadatenprojektion müssen
weiterhin gegen mindestens eine unterstützte Toolchain validiert werden; daher
ist noch kein Generator oder automatischer Host-Index aktiviert.
