# ADR 0001 – Eigenständiges Systemwerkzeug

Status: Superseded by ADR 0009 for the optional agent front end; retained for the Relay Core boundary.

The Relay Core does not depend on Pi or a coding-agent runtime. ADR 0009 allows a separate,
optional front end to use selected official Pi libraries while keeping the core independently
usable and authoritative for every system change.

Der Systemkern muss deterministisch, testbar, AI-unabhängig, recoverbar und sicherheitsgerichtet sein.
