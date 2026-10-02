# Definition of Ready / Definition of Done

## Feature – Ready

Eine Implementierungsaufgabe ist bereit, wenn:

- Zielzustand bekannt ist
- Scope klar ist
- betroffene Systemdomäne bekannt ist
- Source of Truth feststeht
- benötigte Privilegien bekannt sind
- Protected-Scope-Prüfung erfolgt ist
- Recovery grundsätzlich möglich ist
- Teststrategie bekannt ist

Wenn einer dieser Punkte fehlt, zuerst Design klären.

## Feature – Done

Eine read-only Funktion ist Done, wenn:

- strukturierte Ausgabe vorhanden
- Fehlerzustände behandelt
- Version/Capability erkannt
- kein unnötiger Shell-Fallback
- Tests vorhanden

Eine mutierende Funktion ist Done, wenn zusätzlich:

- Before-State erfasst
- Candidate/Plan vorhanden
- Drift geprüft
- Risk klassifiziert
- Recovery vorbereitet
- Apply begrenzt
- Zielwirkung verifiziert
- Journal geschrieben
- Failure Path getestet
- Recovery getestet

## Phase – Done

Eine Projektphase gilt erst als abgeschlossen, wenn alle Exit-Kriterien des jeweiligen Target State erfüllt sind.

Keine Phase gilt als abgeschlossen, nur weil die Happy-Path-Demo funktioniert.
