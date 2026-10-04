# ADR 0022 – UI-neutrale Relay Agent API

Status: Accepted for implementation

## Entscheidung

Task-, Context-, Change-, Health-, History- und Confirmation Services bieten ein versioniertes,
UI-neutrales Relay API. Events umfassen `task_started`, `status`, `observation`, `research_started`,
`evidence_found`, `tool_started`, `tool_finished`, `mutation_detected`, `confirmation_required`,
`applying`, `verifying`, `continuing`, `completed`, `blocked` und `failed`. Event payloads sind
strukturierte, secret-gesäuberte und größenbegrenzte Daten.

CLI/TUI und eine spätere GUI abonnieren dasselbe API. Pi RPC Events werden in Relay Events adaptiert;
Pi-Protokollobjekte und UI-rendering verlassen den Agent Adapter nicht. Confirmation ist eine eigene
API mit Preview/Action-Hash und direkter Relay User Input.

## Konsequenzen

Event-Schema-Änderungen sind versioniert und tests prüfen Reconnect/Gap/Resume. UI darf keinen
Execution Gateway umgehen.
