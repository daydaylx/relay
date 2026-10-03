# 12 – Entscheidungen, die der Implementierer dokumentieren muss

## D1 – Pi Integration

SDK/Packages oder Source Fork?

**Voreinstellung:** SDK/Packages.

## D2 – Prozessarchitektur

TS Agent + Rust Core per Prozess/JSON oder FFI?

**Voreinstellung:** Prozessgrenze + versioniertes JSON. Einfacher zu auditieren und zu isolieren.

## D3 – Config

Relay-spezifischer Pfad?

**Voreinstellung:** ja; keine implizite Pi-Konfiguration.

## D4 – Built-in Shell

Freie unprivilegierte Shell vorhanden?

**Voreinstellung:** für V1 möglichst nein; strukturierte Diagnosewerkzeuge zuerst.

## D5 – Externe Provider

Welche Modelle/Provider gehören zu V1?

Mindestens Provider-Abstraktion erhalten; keine Security-Regeln an ein Modell koppeln.

## D6 – Local model

Nicht Pflicht für V1, aber Architektur darf lokale Provider nicht verhindern.

## D7 – Home Manager

Nicht V1. Erst nach stabilem Daily-Driver-Systemkern.

Alle Entscheidungen als ADR festhalten.
