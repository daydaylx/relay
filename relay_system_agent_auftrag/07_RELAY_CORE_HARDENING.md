# 07 – Relay Core vor Agentenanbindung härten

Der bereits geplante Hardening-Auftrag bleibt gültig.

## P0

### Privilege / PATH
- privilegierte Executables nicht blind über manipulierbaren PATH auflösen
- PATH-poisoning Tests
- `sudo`, `nix-env`, Nix-Aktivierung besonders prüfen

### CI
- Rust checks
- `nix flake check`
- `nix build .#default`
- NixOS Activation/VM Check sichtbar integrieren

### Secret Safety
- Freitext vor externem Modell auf wahrscheinliche Secrets prüfen
- Raw Prompt/Antwort nicht standardmäßig dauerhaft speichern
- Provider-Übertragung transparent machen

### Recovery / Lock
- Crashpunkte testen
- beschädigte State-Dateien
- PID-Reuse beim Lock prüfen
- Recovery-Artefakte

## P1

- Impact-Klassifikation für Service/Network/Desktop/Boot
- modulare Health Checks
- Installation/Ownership
- Daily Driver Pilot

## Sicherheitsinvariante

Der neue Agent-Layer darf **keine bestehende Relay-Sicherheitsprüfung umgehen**.

Ein erfolgreicher Agent-Toolcall ist noch keine Berechtigung zur Systemmutation.
