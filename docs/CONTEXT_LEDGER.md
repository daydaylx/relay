# Relay – Context Ledger

## Projektidentität und Ziel

Relay ist ein eigenständiges, NixOS-orientiertes Systemwerkzeug. NixOS bleibt die Source of Truth;
Relay übersetzt Nutzerabsichten in sichere, strukturierte Konfigurationsänderungen und kann sie
nachvollziehbar prüfen, aktivieren und wiederherstellen. Relay ist kein Pi-Fork, Coding-Agent,
allgemeiner Root-Agent oder Shell-Wrapper mit LLM.

## Bestätigte Architekturentscheidungen

- Der deterministische Systemkern funktioniert ohne KI. KI ist optional, liefert nur strukturierte
  Intents und ist nie die Sicherheitsgrenze.
- PLAN- und Read-Operationen mutieren das System nicht. Ein Plan legt nur Relay-eigenen State an
  (Journal, Plan-Record, Kandidatenkopie) und fügt dem Nix-Store Kandidaten hinzu.
- Nix/NixOS-Kommandokonstruktion gehört in einen Adapter (`nix.rs`). Strukturierte Ausgaben sind
  Terminaltext vorzuziehen; `dry-activate`/`diff-closures` sind reine Anzeige.
- Managed-Write-Bereich ist ausschließlich `relay/managed.nix` (ADR 0003), kanonisch erzeugt und nur
  im Kanonischen zurückgelesen (ADR 0005).
- Jede Mutation hat Preflight, Recovery-Plan, Risikoklasse und Verifikation. Source und Runtime
  werden gemeinsam zurückgesetzt. Journal ist write-ahead, Recovery liest Evidenz (ADR 0006).
- `dry-activate` ist nur Preview; `test` ist temporäre Aktivierung und kein Rollback.
- Kein permanenter Root-Prozess, keine generische Root-Shell; privilegiert sind nur
  `nix-env … --set <system>` und `switch-to-configuration <aktion>` für exakt geplante Store-Pfade.
- Switch-Inhibitoren und NixOS-Prüfungen werden nie umgangen (`NIXOS_NO_CHECK` wird entfernt).
- Keine Subagenten/MCP/Plugins/Remote-Verwaltung im MVP.

## Geschützte Bereiche und Nicht-Ziele

Automatische Mutation ist verboten für `system.stateVersion`, Partitionierung/Dateisysteme/LUKS,
Bootloader/initrd/Secure Boot, Nix-Daemon-Trust und `trusted-users`, grundlegende Auth-/SSH-/
Sudo-/PAM-Konfiguration, Secrets (inkl. geheimnistragender Optionsnamen), Datenbank-Major-Upgrades
(`MIGRATION_REQUIRED` bzw. geschützt) und große NixOS-Upgrades. Diese Bereiche werden im Code
blockiert (`change.rs::protected_category`), nicht nur im Prompt.

Home Manager, Secret Management, Partitionierung, Bootloader, Major-Upgrades sowie AI-/Desktop-/
Remote-Funktionen sind im MVP ausgeschlossen, sofern nicht später ausdrücklich neu entschieden.

## MVP-Zielzustände

- T1 read-only Observer — umgesetzt. T2 Safe Candidate Builder — umgesetzt. T3 Controlled Activator —
  umgesetzt, in der NixOS-VM belegt. T4 Recoverable MVP — umgesetzt (Simulator + VM).
- T5 Natural Language Layer — umgesetzt (`relay ask`, Provider als Adapter, Modell nur Vorschlag; ADR 0007).
  T6 Desktop — umgesetzt als read-only Hyprland-IPC plus Desktop-Gate in `apply` (ADR 0008).
  Live-APIs der Provider wurden nie aufgerufen; Steuerung des Kompositors ist nicht Teil davon.

## Qualitäts- und Prüfregeln

Tests belegen Sicherheitsverhalten (Zuordnung zu den Pflichtkategorien: `tests/README.md`). Eine
mutierende Funktion ist erst fertig, wenn Before-State, Kandidat/Plan, Drift, Risiko, Recovery,
begrenztes Apply, Verifikation, Journal sowie Fehler- und Recovery-Pfade geprüft sind. Echte
Aktivierung wird nie auf dem Daily-Driver getestet, nur in der VM.

## Offene Punkte

- Daily-Driver-Pilot (Einmal-Setup, dann harmlose Änderung) steht aus.
- Lizenz: MIT. Das Repository ist öffentlich (`daydaylx/relay`). Tags erfolgen nur auf ausdrückliche
  Nutzeranweisung.
- Provider-Qualität und Live-APIs ungetestet; Prompt/Antwort nur gegen Fakes.
- Reboot-Verifikation nach echtem Neustart ist nur im Simulator belegt.
- Health ist ein Snapshot nach Beobachtungsfenster; spätere Ausfälle werden nicht erkannt.
- Nur Nix 2.34.8 / NixOS 26.05 (nixpkgs `4feb8eb`) wurden geprüft.
