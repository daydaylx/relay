# Abnahmekriterien

- [ ] kein Pi zur Laufzeit
- [ ] Relay funktioniert ohne AI
- [ ] NixOS-Version/Host/Generation/Store Path bekannt
- [ ] lokaler Options-Index
- [ ] eigener Managed-Schreibbereich
- [ ] deterministischer Renderer
- [ ] Drift Detection
- [ ] Candidate bleibt bis Freigabe isoliert
- [ ] Evaluation + Build + Store Path
- [ ] Closure Diff
- [ ] dry-activate Preview
- [ ] Switch Inhibitors respektiert
- [ ] system.stateVersion geschützt
- [ ] `test` korrekt behandelt
- [ ] Health Check
- [ ] `switch` erst nach erfolgreichem Check
- [ ] reboot-required Workflow
- [ ] Source + Runtime Recovery
- [ ] Recovery ohne AI
- [ ] AI nur typed intents
- [ ] keine generische Shell

## MVP Ende-zu-Ende

```text
Option Change:
Intent → Option → Candidate → Build → Test → Health → Switch → Undo

Package Change:
Intent → Package → Candidate → Build → Diff → Test → Switch → Undo

Diagnosis:
Question → System State → Config/Runtime → Explanation
```
