# System-Agent-Ausbau – Baseline 2026-10-03

## Ausgangspunkt

- Git-Baseline: `a39e473b53d1f0d044d727d66bdc002424e11cb0`.
- Rust Workspace: ein `relay`-Crate, Standardbibliothek-only; CLI und deterministischer
  Change Engine sind bereits implementiert.
- `relay ask` erlaubt optionale Provider und lässt das Modell ein strikt validiertes Intent
  vorschlagen. Status, Health, Verlauf, Planen und Recovery werden weiter vom Rust-Core
  ausgeführt.
- Read-only Beobachtung umfasst Status, Generationen, Health, Nix-Options-/Paketindex sowie
  Hyprland IPC. Strukturierte Diagnose zu Journal, Netzwerk, Prozessen und Hardware ist noch
  nicht als Agent-Tool-Schnittstelle vorhanden.
- CLI-Ausgaben enthalten bereits JSON, aber es gibt noch kein gemeinsames, versioniertes
  Request/Response-Protokoll mit stabilen Fehlercodes für einen externen Frontend-Prozess.
- Es gibt keinen Agentprozess, keine Pi-Abhängigkeit, keinen eigenen Agent-State und keine
  dedizierte Agent-Testmatrix.
- Der Daily-Driver-Pilot ist offen. Externe Provider wurden nicht live getestet; VM-Aktivierung
  wurde getestet.

## Baseline-Prüfungen

`nix develop -c cargo test --workspace` am 2026-10-03:

- 156 Bibliothekstests bestanden.
- 10 CLI-Tests bestanden.
- 0 fehlgeschlagene Tests.

## Harte Invarianten für den Ausbau

1. Das Rust-Core bleibt ohne Node/Pi vollständig verwendbar.
2. Agent-Tool-Liste ist explizit und enthält keine Shell-/Dateischreib-/Extension-Ausführung.
3. Modelltext allein kann keine Mutation auslösen.
4. Jeder Mutationsaufruf nutzt bestehende Plan-, Preview-, Risiko-, Bestätigungs-, Health-,
   Journal- und Recovery-Gates.
5. Secret-Pfade und ungeprüfte Konfigurationsinhalte gelangen nicht in Modellkontext oder
   Journal.
6. `~/.pi`, zufällige Projektanweisungen und Extensions werden nicht geladen.
7. Agent- und Core-Kommunikation ist schema-versioniert; ungültige oder unbekannte Requests
   führen zu keiner Aktion.

## Stufen und Abnahmetore

1. **Architektur/Spike:** Pi SDK-Integration mit Dummy-Tool, getrennte Konfiguration, keine
   Mutation. Tor: Start ohne persönliches Pi-Profil; Agent-Core-Test und Rust-Regression.
2. **Protokoll:** versioniertes JSON für `inspect/status`, `health`, `plan`, `show`; unbekannte
   Versionen/Actions und fehlerhafte JSON-Requests werden abgelehnt.
3. **Read-only Agent:** strukturierte Systemübersicht und gezielte Diagnosewerkzeuge mit
   Ausgabe- und Zeitlimits. Tor: definierte Fragen ohne Seiteneffekt.
4. **Router:** inspect, diagnose, relay_change, blocked, development_required; unbekannte oder
   geschützte Absichten bleiben ohne Mutation.
5. **Mutation Bridge:** Plan/Preview/Bestätigung/Apply/Verify/Undo/Recover über Core-Protokoll.
   Tor: Simulator-/VM-Nachweis und Drift-/Crash-/Fehlerfälle.
6. **UX/Pilot:** TUI mit Backend, Risiko, Preview und Recovery; erst nach erfolgreichem
   Realgeräte-Pilot als tägliches Frontend empfehlen.

User-Space-Backends wie Home Manager oder Hyprland-Schreibzugriff bleiben außerhalb dieser
Umsetzung, bis ein eigener sicherer Write-Boundary- und Recovery-Entwurf vorliegt.
