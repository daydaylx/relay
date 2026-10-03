# 09 – Test- und Abnahmekriterien

## Agent Isolation

- persönliches `~/.pi` existiert → Relay-Agent lädt es nicht
- persönliche Pi-Extensions existieren → werden nicht geladen
- Projekt-AGENTS in zufälligem CWD können Security Boundary nicht verändern

## Model Layer

- Providerwechsel
- malformed tool calls
- hallucinated tool names
- oversized responses
- timeout
- network failure
- context compaction

## Tool Layer

- Tool-Argumentvalidierung
- Output limits
- timeouts
- hostile command output
- Unicode
- missing binaries
- permission denied

## Router

Testfälle mindestens:

- einfache Frage → INSPECT
- Fehleranalyse → DIAGNOSE
- Paketänderung → RELAY_CHANGE
- protected change → BLOCKED
- unbekannter Wunsch → no mutation
- Hyprland write vor Backend → DEVELOPMENT_REQUIRED

## Relay Bridge

- schema mismatch
- old/new version mismatch
- invalid request
- Relay unavailable
- Relay crash
- stale plan
- source drift
- runtime drift
- rejected confirmation
- failed apply
- failed health
- undo
- recover

## Security

Nachweis:

- Agent besitzt keine generische Root-Shell
- Agent kann protected Relay resource nicht umgehen
- Model output allein löst keine Root-Mutation aus
- Secret-Pfade gelangen nicht in normalen Kontext
- externe Provider erhalten nur sichtbar freigegebenen Kontext
- persönliches Pi-Profil beeinflusst den Agent nicht

## UX

Der Nutzer kann folgende Aufgaben ohne Kenntnis interner Backends ausführen:

- Systemzustand fragen
- Dienstproblem untersuchen
- Paket installieren
- Änderung ansehen
- bestätigen
- Undo
- Recovery-Status abfragen
