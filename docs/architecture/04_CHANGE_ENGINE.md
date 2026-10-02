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
