# Relay – Zielzustände

Die Zielzustände verhindern, dass das Projekt zu früh in Desktop-, AI- oder Komfortfunktionen abdriftet.

---

# T0 – Repository Ready

## Ziel

Das Projekt ist reproduzierbar vorbereitet, aber enthält noch keinen produktiven Systemmanager.

## Muss erfüllt sein

- Git-Repository initialisiert
- README vorhanden
- AGENTS.md vorhanden
- ADRs vorhanden
- Security Model vorhanden
- Scope festgeschrieben
- Rust-Workspace/Projektgerüst vorbereitet
- Development/Test-Umgebung dokumentiert
- CI-Grundstruktur vorbereitet
- erster Baseline-Tag oder Baseline-Commit dokumentiert

## Darf noch fehlen

- NixOS System Discovery
- Change Engine
- AI
- UI

---

# T1 – Read-only NixOS Observer

## Ziel

Relay kennt den eigenen NixOS-Host zuverlässig, verändert aber nichts.

## Muss können

```text
relay status
relay generations
relay search-option <query>
relay search-package <query>
```

Konzeptionelle CLI-Namen; konkrete Syntax darf später angepasst werden.

## Muss wissen

- NixOS-Version
- Host
- Kernel
- aktive/Boot-Generation
- running system store path
- Flake/config identity
- nixpkgs revision
- failed systemd units
- grundlegender Desktop-/Sessionstatus

## Exit-Kriterium

Keine Standarddiagnose benötigt freie Shell-Exploration durch ein LLM.

---

# T2 – Safe Candidate Builder

## Ziel

Relay kann eine einfache geplante Änderung vollständig prüfen, ohne das Livesystem zu verändern.

## Muss können

- typed Change Object
- deterministic managed.nix renderer
- candidate isolation
- evaluation
- build
- candidate store path
- closure diff
- dry-activate preview
- risk classification
- protected-resource rejection
- drift detection

## Exit-Kriterium

Bluetooth oder ein Paket kann bis einschließlich Build/Preview geplant werden, während die Live-Konfiguration unverändert bleibt.

---

# T3 – Controlled Activator

## Ziel

Relay darf ungefährliche Änderungen kontrolliert auf einem Testsystem aktivieren.

## Muss können

```text
Candidate
→ test
→ health
→ switch
```

oder:

```text
Candidate
→ boot
→ reboot
→ post-boot health
```

## Muss zusätzlich können

- privilege boundary
- switch inhibitor handling
- activation classification
- failure stop
- journal entry

## Exit-Kriterium

Mindestens eine Optionsänderung und eine Paketänderung funktionieren Ende-zu-Ende.

---

# T4 – Recoverable Daily Driver MVP

## Ziel

Relay kann als persönliches Werkzeug benutzt werden, ohne dass AI für Recovery erforderlich ist.

## Muss können

- Change History
- Source rollback
- Runtime rollback
- combined Relay Undo
- crash/restart recovery
- stale/pending change detection
- recovery CLI
- AI-independent operation

## Pflichtszenarien

- Bluetooth aktivieren
- VLC hinzufügen
- VLC entfernen
- letzte Relay-Änderung rückgängig machen
- fehlgeschlagenen Change sauber erkennen

## Exit-Kriterium

Relay kann die definierten MVP-Änderungen zuverlässig durchführen und rückgängig machen.

---

# T5 – Natural Language Layer

## Ziel

ChatGPT oder ein anderer Provider kann Relay bedienen, ohne Sicherheitslogik zu besitzen.

## Muss können

- Provider abstraction
- typed intent parsing
- schema validation
- malformed intent rejection
- human-readable explanations

## Darf nicht können

- Policy umgehen
- arbitrary Nix schreiben
- Root Shell erhalten
- Recoverylogik ersetzen

## Exit-Kriterium

Dieselben T4-Funktionen funktionieren per natürlicher Sprache und weiterhin ohne AI über CLI/API.

---

# T6 – Desktop Integration

## Ziel

NixOS-Systemintegration für Hyprland/KDE erweitern.

## Erst jetzt

- Hyprland system module
- Runtime IPC
- Workspaces/windows/monitors
- Desktop health

Home Manager bleibt eine separate spätere Entscheidung.

---

# Langfristiger Zielzustand

Relay ist ein kleines, verlässliches NixOS-Control-Center:

```text
User
 ↓
Relay UI / CLI / Natural Language
 ↓
Typed System Intent
 ↓
Candidate + Safety + Recovery
 ↓
NixOS
```

Nicht:

```text
User
 ↓
LLM
 ↓
Root Shell
```
