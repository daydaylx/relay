# ADR 0006 – Privilegiengrenze und evidenzbasierte Recovery

Status: Accepted

## Kontext

Relay läuft als normaler Benutzer (kein permanenter Root-Prozess, keine Root-Shell für ein
Modell). Aktivierung braucht Root. `nixos-rebuild test` ist kein Rollback. Prozesse können jederzeit
sterben.

## Entscheidung

- **Typisierte Privilegien.** Nur der NixOS-Adapter kann privilegierte Aufrufe erzeugen, und nur
  zwei: `nix-env -p /nix/var/nix/profiles/system --set <system>` und
  `<system>/bin/switch-to-configuration <dry-activate|test|switch|boot>`. `<system>` muss ein
  einzelner Store-Eintrag sein und ist exakt der geplante, unveränderte Kandidat (oder das
  gespeicherte vorherige System). Eskaliert wird pro Aufruf mit `sudo --`; läuft Relay bereits als
  Root, entfällt das Präfix. Es gibt keine API, beliebige Kommandos zu eskalieren.
- **Switch-Inhibitoren werden nie umgangen.** Relay liest `<system>/switch-inhibitors` wie NixOS
  selbst (gleicher Schlüssel, anderer Wert ⇒ Inhibitor) und wählt dann den Boot-Pfad. Die Variable
  `NIXOS_NO_CHECK` wird aus jedem Kindprozess entfernt und kann nicht gesetzt werden.
- **Write-ahead-Journal.** Jeder Zustandsübergang wird *vor* der Aktion protokolliert, die er
  ankündigt (`source-applied`, `test-activated`, `reboot-pending`, `rollback-started`).
  Das Journal enthält Kennungen, Hashes, Store-Pfade und kurze Reason-Codes, keine
  Konfigurationswerte.
- **Recovery liest Evidenz, nicht Absicht.** Rollback und `relay recover` prüfen Dateihashes und
  die Links `run/current-system`, `run/booted-system` und das Systemprofil. Passt der Zustand weder
  zum vorherigen noch zum Kandidaten-System, wird nichts aktiviert (kein Raten). Alle Schritte sind
  idempotent und wiederholbar.
- **Richtung der Recovery.** Zurückrollen ist der sichere Weg. Eine bereits verifizierte Änderung
  wird nur nach erneutem Health-Check vorwärts abgeschlossen. Reboot-Änderungen werden nie live
  zurückgeschaltet, sondern über die Boot-Konfiguration.
- **Health ist baseline-relativ.** Neu fehlgeschlagene oder crash-loopende Units (und optional
  erwartete aktive Units) zählen; bereits vorher fehlerhafte Units werden nicht der Änderung
  angelastet. Ein Beobachtungsfenster fängt Units ab, die kurz nach dem Start abstürzen.

## Folgen

- Echte Aktivierung wird nur in einer NixOS-VM getestet (`checks.<system>.activation`), nie auf dem
  Host. Der Simulator-Test im Rust-Crate deckt Crash-/Rollback-Pfade deterministisch ab.
- Ein hängender `switch-to-configuration`-Kindprozess nach hartem Abbruch von Relay kann die
  NixOS-Sperre halten; `relay recover` meldet das als unvollständigen Rollback und kann
  wiederholt werden.
