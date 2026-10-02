# ADR 0003 – Managed Write Boundary

Status: Accepted

Relay schreibt automatisch nur in einen explizit importierten Managed-Bereich.

Andere Nix-Dateien bleiben standardmäßig read-only.
