# ADR 0018 – Versionsgebundene Knowledge und Evidence

Status: Accepted for implementation

## Entscheidung

Knowledge Resolver verwendet strikt diese Reihenfolge: lokaler Systemkontext; Daten gegen den
installierten/gelockten nixpkgs und tatsächlich installierte Hyprland-Version; lokale versionierte
Manuals/Manpages; offizielle passende Online-Dokumentation; aktuelle offizielle Docs; Upstream source/
issues; etablierte Community; allgemeines Web. Versionsmismatch und Retrieval-Zeit werden angezeigt.

Jeder entscheidungsrelevante Claim speichert `claim`, URL/source id, source class, version/revision,
publication/retrieval time, relevance/applicability, Task-ID und redigierten Ausschnitt/Hash. Externe
Inhalte werden als untrusted data an Pi übergeben. `Evidence` ist kein ausführbarer Befehl und überschreibt
weder Relay Policy noch User Goal/SystemContext.

## Konsequenzen

NixOS Option/Package APIs nutzen die vorhandene lockfile-gebundene Index-Generierung. Hyprland
`descriptions`/JSON und lokale Manpages ergänzen versionierte Wissensdaten. Cache-Identitäten und
stale sources sind explizit.
