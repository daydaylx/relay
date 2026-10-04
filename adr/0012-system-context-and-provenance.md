# ADR 0012 – SystemContext und geprüfte Fakten

Status: Accepted for implementation

## Entscheidung

Rust Core erzeugt einen schema-versionierten `SystemContext` als deterministische Beobachtung, nicht
als Modellzusammenfassung. Er enthält Machine Identity, Configuration Identity, Ownership Snapshot,
Live Snapshot, Capabilities, aktive Policy, Knowledge-Quellen, jüngste Change History und einen
Task-Verweis. Jede Fact trägt `source`, `observed_at`, Versions-/Identity-Bezug, Gültigkeitsstatus,
TTL und `verified`-Kennzeichnung. Agent-Hypothesen und Notes sind separat und können niemals geprüfte
Fakten überschreiben.

Context wird modular und bei jeder relevanten Pi-RPC-Anfrage frisch aus Cache/Adapter zusammengesetzt.
Stale oder unbekannte Werte bleiben ausdrücklich unknown. `relay context [--json]` ist read-only.
Paketnamen, vollständige Hostpfade, Seriennummern und sensible freie Texte werden nach Zweck minimiert.

## Konsequenzen

Versionen, Config Identity und Live State werden effizient einmal erhoben und nach TTL/Change Events
invalidiert. Pi erhält nur für den aktuellen Task benötigte Module und Provenance; externe Modellkontext-
Redaktion bleibt vorgeschaltet. Nicht vorhandene Fakten werden nicht geraten.
