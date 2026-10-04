# Relay – Context Ledger

## Projektidentität und Ziel

Relay ist ein eigenständiges, NixOS-orientiertes Systemwerkzeug. NixOS bleibt die Source of Truth;
Relay übersetzt Nutzerabsichten in sichere, strukturierte Konfigurationsänderungen und kann sie
nachvollziehbar prüfen, aktivieren und wiederherstellen. Für natürliche Sprache bettet Relay
ausgewählte Pi-Agent-Runtime-Komponenten ein; es ist kein Pi-Fork, Coding-Agent, allgemeiner Root-Agent
oder Shell-Wrapper mit LLM.

## Bestätigte Architekturentscheidungen

- Der deterministische Systemkern funktioniert ohne KI. KI ist optional, liefert nur strukturierte
  Intents und ist nie die Sicherheitsgrenze.
- Pi-Komponenten kommen ausschließlich aus den exakt versionierten Repository-Abhängigkeiten. Keine
  Vermischung mit persönlichem Pi-Setup, `~/.pi`, lokalen Profilen, Prompts, Sessions, Extensions,
  Einstellungen oder Credentials.
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
- T7 Agent Task Runtime — erste integrierte Version mit Task-Journal, Pi Tool-Loop, lokaler
  Mutationbestätigung und strukturierten Goal-Verifikationen. Verifikation deckt momentan Bluetooth,
  konkreten Dienst, Systemgesundheit und Paketverfügbarkeit ab. Live-TUI/Provider-Pilot und weitere
  Ziele (MIME-Defaults, Monitorlayout, Generationenbereinigung) bleiben offen (ADR 0010).

## Qualitäts- und Prüfregeln

Tests belegen Sicherheitsverhalten (Zuordnung zu den Pflichtkategorien: `tests/README.md`). Eine
mutierende Funktion ist erst fertig, wenn Before-State, Kandidat/Plan, Drift, Risiko, Recovery,
begrenztes Apply, Verifikation, Journal sowie Fehler- und Recovery-Pfade geprüft sind. Echte
Aktivierung wird nie auf dem Daily-Driver getestet, nur in der VM.

## Offene Punkte

- Live-Aktivierung auf dem Daily Driver ist nicht validiert. Aktivierung und Recovery werden in
  der NixOS-VM geprüft; ein späterer Einsatz ist eine separate Betriebsentscheidung, kein Test.
- Am 2026-10-03 meldete Relay `managed_module: in-sync`, verweigerte aber Planung wegen einer
  abweichenden ausgewerteten Quellkonfiguration. Der konkrete Unterschied liegt im `greetd`-
  Startbefehl: `desktop.nix` enthält `--battery --asterisks`, die laufende Generation nicht.
  Der Paketbestand ist gleich. Die separaten README-, Hyprbars- und Quickshell-Änderungen sollen
  erhalten bleiben; Hyprbars und `minimizeWindow` waren zur Laufzeit aktiv. Ein Dry-Build war
  erfolgreich, die NixOS-Generation wurde nicht gewechselt.
- Lizenz: MIT. Das Repository ist öffentlich (`daydaylx/relay`). Tags erfolgen nur auf ausdrückliche
  Nutzeranweisung.
- Provider-Qualität und Live-APIs ungetestet; Prompt/Antwort nur gegen Fakes.
- Reboot-Verifikation nach echtem Neustart ist nur im Simulator belegt.
- Health ist ein Snapshot nach Beobachtungsfenster; spätere Ausfälle werden nicht erkannt.
- Nur Nix 2.34.8 / NixOS 26.05 (nixpkgs `4feb8eb`) wurden geprüft.
