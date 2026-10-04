# Relay/Pi-Kontrollzentrum – Ist-/Soll-Audit

Stand: 2026-10-03. Dieser Audit beschreibt den Repository-Stand und die Architektur für den
erweiterten Systemkontrollzentrum-Auftrag. Er verwendet ausschließlich Dateien dieses Repositories
und dessen festgelegte `agent/node_modules`-Abhängigkeiten. Das persönliche Pi-Setup des Nutzers,
insbesondere `~/.pi`, wurde nicht gelesen oder untersucht.

## Ist-Zustand in Relay

| Bereich | Vorhanden | Grenze des Ist-Stands |
| --- | --- | --- |
| NixOS Safety Core | Rust Candidate, Eval/Build, Config-/Closure-Diff, Risk, Protected Resources, Source-/Runtime-Drift, `dry-activate`, `test`, Health/Desktop Checks, switch/boot, Journal, Undo, Recovery und Single-Writer-Lock | Eigentümer der Systemmutation; Managed Write bleibt `relay/managed.nix` |
| Strukturierter Adapter | Nix-Aufrufe zentral in `nix.rs`, Unit-/Health-Daten, Prozess-/Hardware-/Netzwerk-/Bluetooth-/Journal-/Hyprland-Readouts | Keine allgemeine process gateway; Journal zeigt bewusst nur klassifizierte Metadaten |
| Nix Knowledge | lokal generierbare, revisionsgebundene Options-/Paketindizes und Suche | Option-/Paket-Suche gibt begrenzte Beschreibungen; kein vollständiger Versionsmanual-/Manpage-Service |
| Agent | `agent/` nutzt direkt `pi-agent-core`, `pi-ai`, `pi-tui` und TypeScript; Tasks, Event-Journal, begrenzter Kontext, sequenzielle Toolcalls, Relay-Plan/Apply/Undo/Recovery und wenige Goal-Verifier | kein Pi RPC, keine Pi Sessions/Compaction, keine breiten Prozess-/Dateiwerkzeuge, keine Ownership Map, kein persistent SystemContext, keine Web/MCP-Evidence |
| Agent-Konfiguration | Relay-eigener Pfad `~/.config/relay/agent.json`; spezifische `RELAY_AGENT_*` Variablen | Pi `~/.pi` wird per Anwendungscode nicht geladen; der neue Runtime-Entwurf muss zusätzlich CWD, Resource Loader, Session-Pfad und Provider-Konfiguration isolieren |
| Änderungen außerhalb des Nix-Core | nur streng begrenztes, lesendes Safe-Read unter konfiguriertem Flake | keine Datei-Transaktionen für User Config, Runtime-Aktionen oder externe Dateiänderungen |
| Desktop | Hyprland IPC Status, Monitor-/Workspace-Metadaten und Health Gate | keine konfigurations-Ownership-Erkennung, keine Runtime Dispatcher oder persistenten Änderungen |
| Tasks | JSONL-Ereignisse, Zustandsautomat, Change-IDs und verifizierte Abschlüsse | eingeschränkte gewünschte Zustände; Notes, Recherche, Evidence, Mutationsarten und recoveryfähige Fremdänderungen fehlen |
| UI / Einstieg | `relay` kann aus dem kombinierten Paket den Agent starten; CLI Core bleibt erhalten | TUI und Agent-Service noch nicht als stabile, vollständige GUI-API produktreif |

## Pi-Architektur und Integrationsentscheidung

Der bestehende Pi-Code verwendet die Low-Level-Bausteine `pi-agent-core`, `pi-ai` und `pi-tui` direkt.
Damit werden weder der Coding-Agent Session-/Resource-Loader noch CLI-RPC, Persistenz, Compaction,
Settings oder MCP-Verwaltung genutzt.

Pi dokumentiert zwei passende Host-Grenzen: TypeScript/Bun-Hosts können das SDK direkt im Prozess
verwenden; nicht-TypeScript-Hosts und Integrationen mit Prozessgrenze können eine lang laufende RPC-
Runtime über LF-begrenztes JSONL steuern. RPC streamt getrennte Response- und Session-Events, kann
Dialogs über die Extension-UI zurück an den Client leiten und verlangt, dass der Client Startfehler,
Abbruch, Fristen und unerwarteten Prozessausstieg selbst behandelt. Pi-SDK-Sessions bieten dagegen
direkten Zugriff auf Agent, Session, Toolset, Resource Loader und Context-/Compaction-Lifecycle.

**Architekturentscheidung für die nächste Runtime: Pi als separat gestarteter, exakt gepinnter RPC-
Prozess; Relay Rust ist RPC-Host und Execution Gateway.** Das entspricht Relays Rust-Kern, begrenzt
Crash-/Memory-State auf einen Kindprozess, gibt Relay Kontrolle über Session-ID, CWD, Env, Session-
Speicher und Lifecycle und hält die UI unabhängig. Das ist Prozessisolation, aber für sich allein
keine Sicherheits-Sandbox: Kindprozess, Extensions und MCP-Server laufen ohne zusätzliche OS-Grenze
mit den Rechten desselben Benutzers. Deshalb werden alle mutierenden und diagnosebezogenen Werkzeuge
Relay-seitig bereitgestellt oder durch Relay's Execution Gateway ausgeführt. Pi-eigene generische
Datei-/Bash-Tools dürfen nicht direkt auf dem Host laufen.

Der Runtime-Paketbaum samt CLI, Extension und allen transitiven Versionen muss aus dem Repository-
Lockfile bezogen werden. Der Pi-Prozess erhält ein Relay-eigenes Config-/State-Verzeichnis und einen
Relay-kontrollierten Arbeitsordner, benutzt keine User-/Projektressourcen und startet offline ohne
Update-Telemetrie. Die Sessionablage bleibt unter Relay-State. Diese Werte werden explizit und nicht
aus einem Pi-Default-Verzeichnis aufgelöst.

Pi-Dokumentation und RPC-Details ändern sich. Jede Implementierung wird gegen Version und Typen im
Projekt-Lockfile geprüft; die aktuellen Online-Dokumente sind ein API-Mapping und keine stillschweigende
Abhängigkeit auf "latest".

## Soll-Mapping

| Soll-Komponente | Bestehende Grundlage | Notwendige Erweiterung | Eigentümer / ADR |
| --- | --- | --- | --- |
| Pi RPC Runtime + Relay Sessions | eingebetteter Agent Loop, Provider-Adapter, TUI | CLI als exakte Repository-Abhängigkeit, RPC framing/client, isolierter Runtime-Prozess, Relay Sessions und sichere Recovery | Relay Runtime / ADR 0011 |
| SystemContext | `status`, Host-/Nix-Metadaten, generations, diagnostics, Hyprland summary | typisierte Snapshot-Fakten samt Provenance, TTL, system/config identity, capabilities, policy, history und aktuellem task context | Relay Core / ADR 0012 |
| Ownership Map | strikter `relay/managed.nix` Scope und source drift | NixOS/Home Manager/User/Generated/Unknown Zuordnung; nur fundierte Quellen werden als verifiziert registriert | Resolver / ADR 0013 |
| Execution Gateway | getypter Nix-Protokoll, `Runner`, Prozessfristen, Core Policies | gemeinsamer Operationstyp, Klassifikation über executable/args/env/cwd/paths/privilege/target; Policy-Enforcement und Evidence/Journaling | Rust Gateway / ADR 0014 |
| Bash/read/write/edit/search/process | begrenzte Safe-Read und Core `Runner` | Relay-gebrandete Tools über Gateway; systemnahe Sandboxing/Writable scopes, keine naiven String-Whitelists | Gateway adapters / ADR 0015 |
| File Transactions | nur managed Nix Source/Runtime Recovery | private Before-/After-Metadaten, atomic replacement, foreign-edit hash check, directory/permission/link policy, recovery journal | Transaction layer / ADR 0016 |
| Task + Desired State | Task JSONL und vier konkrete Checks | desired-state data, hypotheses vs facts, Research/Evidence/Actions, unbegrenzte abgeschlossene Schritte innerhalb fester Sicherheitsbudgets, continue/stop/recover | Controller / ADR 0017 |
| System Knowledge / Evidence | `nixpkgs`-revisiongebundener Options-/Paketindex | Fakten- und Quellenhierarchie, NixOS/Hyprland/systemd-Manpages, veraltete Daten erkennen, Claims mit applicability/URL/version/time | Knowledge / ADR 0018 |
| MCP/Web | derzeit bewusst nicht integriert | Relay-eigene MCP registry, Trust Classes, read-only-first Policy, fetch/search mit SSRF-/Credential-/Längenlimits, Quelle/Evidence | Knowledge broker / ADR 0019 |
| Privilege Broker | konkrete Nix Aktivierungsaufrufe privilegiert | kurzlebige, getypte Root-Operationen, task-scope grants und separate Freigaben für unbekannt/destruktiv/kritisch | Core / ADR 0020 |
| Hyprland | read-only `hyprctl -j`/Health | version-aware descriptions, bind/config fact, runtime-vs-persistent APIs und Ownership-Routing | Desktop adapters / ADR 0021 |
| GUI/API | Task Events, CLI/TUI | schema-versioniertes, UI-neutrales Event-/Command API für Tasks, State, Changes, Confirmations, Health und History | Relay API / ADR 0022 |

## Sicherheitsentscheidung für breite Tools

"Volle Arbeitsfähigkeit" bedeutet, dass Pi bei einem unbekannten Fehler Shell, Prozesslisten,
Konfigurationsleser, Recherche und passende Systemadapter wählen kann. Es bedeutet nicht, dass das
Modell ungeprüft Host-I/O ausführen kann. `bash`, Interpreter, Pipes und dynamisch gestartete Kinder
sind als Operationen nicht statisch vollständig klassifizierbar. Daher ist der Gateway-Entwurf eine
Kombination aus strukturierter Intent-Klassifikation, Linux OS-Enforcement, scopes und Nachher-
Beobachtung:

1. **OBSERVE:** lesender Host-Kontext über schmale Adapter; Shell-/Interpreter-Prozesse laufen nur
   unter einer OS-Read-Sandbox mit geheimen Pfaden gesperrt, zeit-/ressourcenbegrenzt und ohne
   mutierende System-Sockets.
2. **CONTROL:** High-Level Relay-Aktionen zuerst. User-Config-Aktionen verwenden Ownership-Resolver
   und File Transactions. Runtime-Aktionen wie Service-/Hyprland-Steuerung nutzen konkrete Adapter.
   Managed Nix Änderungen bleiben vollständig im bestehenden Candidate/Safety Core.
3. **ADMIN:** Task-scope gilt nur für klar deklarierte, rücknehmbare Operationen. Root, unbekannte
   Commands, secret access und destruktive Operationen benötigen separate direkte Freigabe; ein
   root shell oder ein allgemeines sudo-Tool wird nicht freigegeben.

Operationen haben mindestens `READ_ONLY`, `USER_MUTATION`, `RUNTIME_MUTATION`,
`MANAGED_SYSTEM_CHANGE`, `PRIVILEGED_CHANGE`, `DESTRUCTIVE_CHANGE`, `SECRET_ACCESS` und `UNKNOWN`.
Entscheidungen berücksichtigen executable und resolved path, Argumente, cwd, Umgebung, redirections,
Target/Ownership, effektive Privilegien, erwartete Mutation und Rücknehmbarkeit. Shell-Parsing dient
der Erklärung/Klassifikation; alleinige Berechtigung entsteht nur durch OS-Grenze und explizite
Autorisierung.

Untrusted Web-/MCP-/Log-/Dateiinhalte sind Daten und Evidence, keine Anweisungen. MCP annotations
werden nicht als vertrauenswürdige Policy verwendet. Credentials werden nicht in Pi-Prozessumgebung
oder externen Content-Kontext kopiert, sofern ein Provider sie nicht für Requests benötigt.

## Umsetzungsstufen und Abnahme

1. Baseline, Ziel- und Migrationsarchitektur (dieser Audit und ADRs 0011–0022).
2. Version-gepinnte RPC-Paketierung, kontrollierter Start/Stop, Relay Sessions und isolierte Resource-
   und Credential-Pfade; keine Host-Mutationen.
3. `relay context [--json]`, Machine-/Configuration Identity, LiveSnapshot und Capability Facts.
4. Ownership Map mit verifiziert/unbekannt, insbesondere NixOS, Home Manager, Hyprland und generierte
   Dateien.
5. Rust Execution Gateway mit OBSERVE-Policy, strukturiertem Command Model, realer Linux-Sandbox,
   bounded output/time/process trees und adversarial escape tests.
6. File Transactions und Runtime adapter journal/rollback.
7. MCP/Web Knowledge Broker + evidence store, zuerst nur `TRUSTED_OFFICIAL` und `EXTERNAL_READ`.
8. Task Desired State/Controller Loop und SystemContext injection auch nach Compaction/Resume.
9. Task-scoped CONTROL/ADMIN und Privilege Broker erst nach Sandbox-/transaction gate.
10. Hyprland version-aware read/runtime/config control und GUI Event/API.

Sicherheitsgates blockieren die jeweilige Phase: kein mutierendes Shell-/MCP-/Bash-Tool vor OS-
Sandbox und Transaction Journal; kein Privilege Broker vor adversarial Gateway-Tests; keine automatische
Ownership-Mutation bei `Unknown`; keine Task-Abschließung ohne echten Goal-Comparator und Evidence.

## Pi-Referenzen

- [Pi RPC Mode](https://pi.dev/docs/latest/rpc)
- [Pi SDK und Session Lifecycle](https://pi.dev/docs/latest/sdk)
- [How Pi Works: Agent Loop, Context, Sessions, Permissions](https://pi.dev/docs/latest/how-pi-works)
- [Pi Extensions: Tool, MCP, Session und Context hooks](https://pi.dev/docs/latest/extensions)
- [Pi MCP Servers](https://pi.dev/docs/latest/mcp)
- [Pi Tool/Process Isolation](https://pi.dev/docs/latest/containerization)
