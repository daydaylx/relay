# Hinweise für den ausführenden Agenten

Dieser Auftragsordner ist **Planungs- und Architekturinput**.

Reihenfolge:

1. `00_START_HERE.md`
2. `01_PRODUCT_VISION.md`
3. `02_PI_CORE_ADOPTION.md`
4. `03_TARGET_ARCHITECTURE.md`
5. `04_SYSTEM_MODEL.md`
6. `05_TOOLS_AND_PERMISSIONS.md`
7. `06_ROUTER_AND_WORKFLOWS.md`
8. `07_RELAY_CORE_HARDENING.md`
9. `08_PHASE_PLAN.md`
10. `09_TEST_AND_ACCEPTANCE.md`
11. `10_REPO_RESTRUCTURE.md`
12. `11_NON_GOALS.md`
13. `12_DECISIONS_REQUIRED.md`
14. `13_MASTER_WORK_ORDER.md`

## Arbeitsregel

Nicht alle Phasen in einem einzigen unübersichtlichen Commit umsetzen.

Empfohlene Commit-Grenzen:

- docs/architecture baseline
- Pi integration spike
- protocol
- read-only inventory/tools
- router
- mutation bridge
- UI
- daily-driver fixes

## Quellenlage

Die Pi-API ändert sich. Vor tatsächlicher Implementierung immer die aktuelle
Dokumentation auf `pi.dev/docs/latest/` und den aktuellen `earendil-works/pi`
Stand prüfen.

Keine alten Paketnamen oder APIs aus Erinnerungen übernehmen.
