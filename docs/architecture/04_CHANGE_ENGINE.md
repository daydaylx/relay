# Change Engine

## Standardworkflow

```text
1. Intent
2. System Snapshot
3. Option / Package Resolve
4. Current Evaluated State
5. Change Object
6. Candidate Config
7. Evaluation
8. Build
9. Closure Diff
10. dry-activate Preview
11. Switch-Inhibitors
12. Risk Classification
13. Confirmation
14. Drift Check
15. Source Apply
16. Re-evaluation
17. Candidate Identity Check
18. test / boot / switch
19. Health Check
20. Journal
```

Live-Konfiguration bleibt bis zur Freigabe unverändert.

Vor Apply wird der Source-Hash erneut geprüft. Bei Drift wird abgebrochen.

## Umsetzung (Stand T2–T4)

| Schritt | Umsetzung |
| --- | --- |
| 1–2 Intent, System Snapshot | typisierte `Change`-Werte (CLI oder `intent.rs`); Running-System, Quell-Hash, Runtime-Stempel |
| 3–4 Resolve, Current State | lokaler Index (`index.rs`); Managed-Baseline aus `relay/managed.nix` (strikter Parser) |
| 5–6 Change, Candidate | `ManagedState::apply`, deterministischer Renderer; Kopie des Quellbaums unter `<state>/candidates/<id>/src` |
| 7–8 Evaluation, Build | `nix eval --raw …drvPath`, `nix build --no-link --print-out-paths <drv>^out` auf dem `path:`-Kandidaten, nie auf der Live-Quelle |
| 9 Closure Diff | `nix store diff-closures` (nur Anzeige) plus Evidenz aus Dateien: `kernel`, `initrd`, `kernel-modules`, `systemd`, `switch-inhibitors` |
| 10 dry-activate | `switch-to-configuration dry-activate` (privilegiert, nur Vorschau); in `apply` ein Gate |
| 11 Switch-Inhibitors | gleiche Regel wie NixOS: Schlüssel in beiden Systemen, anderer Wert ⇒ Inhibitor ⇒ Boot-Pfad |
| 12 Risk | `LIVE_SWITCHABLE` / `REBOOT_REQUIRED` / `MIGRATION_REQUIRED` / `PROTECTED`: Namensregeln, nur durch Closure-Evidenz erhöhbar |
| 13 Confirmation | `Confirmation` wird nur vom Frontend nach Anzeige des Reviews erzeugt (`--yes` oder Eingabe `yes`) |
| 14 Drift Check | Quellbaum-Hash, laufendes System, Systemprofil, Kandidaten-Integrität, Runtime-Stempel |
| 15 Source Apply | atomares Schreiben von `relay/managed.nix` (Symlinks werden nicht gefolgt), Journal `source-applied` vorher |
| 16–17 Re-Evaluation, Identity | Live-Quelle neu evaluieren; Derivation muss der des Kandidaten entsprechen |
| 18 test / boot / switch | siehe `05_ACTIVATION_AND_RECOVERY.md` |
| 19–20 Health, Journal | baseline-relative Health-Prüfung; Write-ahead-Journal |

Ein Plan ist erst `built`, wenn Evaluation, Build, Aktivierbarkeit (`bin/switch-to-configuration`),
Risiko, Closure-Diff und alle Hashes im Plan-Record stehen. Ein Plan, der die Live-Quelle
verändern würde, ohne dass die Host-Konfiguration das Managed-Modul importiert, wird abgelehnt.
