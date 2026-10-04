# Migrationsplan: intelligentes NixOS-Kontrollzentrum

Stand: Pi RPC ist die produktive Task-Runtime. Die Task-scoped Relay-Erweiterung spricht über einen
privaten Unix-Socket mit dem begrenzten Host-Tool-Dispatcher. SystemContext wird lokal gesammelt,
in den Pi Systemprompt injiziert, redigiert im Task-Journal gespeichert und bei Resume wieder
injiziert. Die Ownership-Aussage ist derzeit bewusst konservativ: nur Relay-managed NixOS ist
schreibbar; Home Manager und User Config sind unbekannt/read-only. Weitere Phasen bleiben offen.

## Phasen und Gates

| Phase | Inhalt | Gate für die nächste Phase |
| --- | --- | --- |
| 0 – Baseline | Ist-/Soll-Audit, Pi API Mapping und ADR 0011–0022; bestehende Core Regression grün | unveränderter Core-Vertrag, private Pi-Konfiguration ausgeschlossen |
| 1 – Pi RPC | **Task-Runtime und Tool-Bridge umgesetzt:** exakte CLI Dependency, isolierte Config-/Home-/Session-Pfade, JSONL, task-scoped Extension + Unix-Socket, Tool-Event-Journal, Pi Systemprompt und direkter Bestätigungsworkflow. `--pi-rpc-check` bleibt Diagnose. Crash-Recovery/Streaming/Event-Gap-Härtung und Live-Provider-Usability bleiben offen. | keine persönlichen Pi-Ressourcen; nur Relay-Tools erreichbar; lokale Bestätigung bleibt zwingend |
| 2 – SystemContext | **Erste read-only Version umgesetzt:** Agent-Tool `relay_system_context` sammelt NixOS-Identität, Generationen, Hardware, Desktop, Capabilities und Provenance; wird zu Taskstart in Systemprompt und Journal gegeben. Noch offen: CLI `relay context`, TTL/Refresh-Invalidation, Nix/Hyprland-Versionen und vollständige Host-/Flake-Identität. | nur verifizierte lokale Adapterdaten; Größen-/Secret-Filter; Resume führt SystemContext erneut mit |
| 3 – Ownership | **Sicherheitsgrenze umgesetzt, Map unvollständig:** Relay-managed Modul wird als Relay-owned markiert. Home Manager/User-/generierte/externe Quellen werden als unbekannt/read-only ausgewiesen. Automatische Source-Erkennung und Evidence-Graph offen. | unknown/generated sources blockieren persistente Writes |
| 4 – OBSERVE Gateway | typed Operation model, process-tree limits, Linux read-only sandbox, structured diagnostics | adversarial bash/interpreter/path/env/socket/namespace escape suite muss blocken |
| 5 – Read Knowledge | NixOS/nixpkgs versioned search/get/current; Hyprland descriptions; docs/manpages | nixpkgs/Hyprland version mismatch muss markiert werden; keine secret/rich output leakage |
| 6 – File Transactions | Relay user-scope write/edit, metadata/evidence journal, compare-before-undo | crash safety, symlink/hardlink/race tests, foreign edit preserved, metadata restore matrix |
| 7 – Tasks/Desired State | desired state, research/evidence/notes, controller loop, comparator, context reinjection | apply failure → continue → second action → actual goal success end-to-end |
| 8 – Runtime CONTROL | services/audio/network/Bluetooth/Hyprland runtime adapters plus task-scope permission | targeted action, no shell passthrough, post-action observation and reversible outcome |
| 9 – Knowledge MCP/Web | Relay-owned server registry, read MCP tools, search/fetch, SSRF and Trust Classes | hostile/injected responses cannot change policy; localhost/private/network redirect blocked |
| 10 – Privilege Broker | bounded targeted root actions, task grants, ADMIN mode | exact argv/target, ask overrides, rollback/evidence, no generic root child; exhaustive tests |
| 11 – Desktop + UI API | Hyprland version-aware state/config, GUI-ready event/query endpoints | Runtime vs persistent ownership preserved and all Events replay/resume tested |

## Migrationseigentümer

- **Core/Nix mutation:** bleibt `crates/relay/src/change.rs`, `engine.rs`, `nix.rs`, `journal.rs`,
  `state.rs` und `protocol.rs`; nicht neu schreiben.
- **Pi host boundary:** bestehendes `agent/` wird vom direkten SDK auf RPC Host/Client migriert, falls
  der Spike mit der gepinnten Pi-Version die Session-/UI-Bedürfnisse erfüllt.
- **SystemContext/Ownership/Gateway:** neuer Rust-Modulbereich mit schema-versionierten, read-only
  Protocol-Actions und Simulatoren; Agent/RPC kennt keine Root- oder direkten OS write handles.
- **Files/Runtime changes:** eigene Journaltypen/Undo records; nicht in Nix `Change` hineinmodellieren.
- **Knowledge:** vorhandene Lockfile-gebundene Index-Generatoren weiterverwenden und mit
  Versionsidentität/Provenance ergänzen.

## Sicherheits- und Funktionstests

- bestehende `cargo test --workspace`, Nix package/build und Aktivierungs-VM unverändert grün.
- RPC: starten/stoppen/crash, fehlerhafte LF JSONL, backpressure, stream events, duplicate request ID,
  cancel, compaction, resumed session, private config/session isolation.
- SystemContext: NixOS/Nix/kernel/Hyprland versions, flake/lock identity, generation, TTL invalidation,
  incomplete/stale facts and secret redaction.
- Ownership: NixOS, Home Manager, direct user file, symlink, generated path, conflict, missing source.
- Gateway: bash pipes/redirection/subshell/eval/interpreters, env injection, symlinks, mount points,
  process descendants, D-Bus, sysfs/procfs, network, secret paths and sandbox escapes.
- Transactions: create/replace/delete, mode/owner/xattrs where supported, crash at every boundary,
  foreign edit, symlink race, undo and task-wide mixed Nix/file/runtime history.
- Knowledge/Web/MCP: offline preference, version mismatch, source classes, untrusted prompt payloads,
  SSRF across redirect/DNS, private address aliases, oversized content, timeout, malformed servers.
- Controller: desired state survives compaction/restart; model success claims rejected; first verify
  fails, second step succeeds; human grant expires on restart; resource limit reaches blocked state.

## Umsetzungshinweis

Die Task-Runtime verwendet Pi RPC und Relay-Tools, hat aber keinen vollständigen Desired-State-
Comparator oder automatische Ownership-Auflösung. Ein erster task-scoped SystemContext ist umgesetzt;
CLI-Inventar, TTL/Invalidation und ein Ownership Graph fehlen. Es gibt kein allgemeines Execution
Gateway, keine transaktionalen User-Dateiänderungen und keinen MCP/Web Broker. Diese Anforderungen
gelten nicht als erledigt, nur weil ADR und Schnittstellen beschrieben sind. Breite Hostmutationen
bleiben gesperrt, bis die jeweilige Phase ihr Gate nachweislich erfüllt.
