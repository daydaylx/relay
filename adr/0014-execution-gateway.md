# ADR 0014 – Gemeinsames Execution Gateway

Status: Accepted for implementation

## Entscheidung

Alle Pi-Toolcalls, Relay High-Level Actions, MCP-Aufrufe und Kindprozesse durchlaufen ein Rust-
Execution Gateway. Eine `Operation` enthält mindestens executable/argv oder Tool-ID, cwd, bereinigte
Umgebung, Datei-/Socket-Ziele, erwartete Wirkung, effektive Privilege, Task-ID, Ownership, Risiko,
Reversibilität und angeforderte Scope-Freigabe. Der Gateway klassifiziert in `READ_ONLY`,
`USER_MUTATION`, `RUNTIME_MUTATION`, `MANAGED_SYSTEM_CHANGE`, `PRIVILEGED_CHANGE`,
`DESTRUCTIVE_CHANGE`, `SECRET_ACCESS`, `UNKNOWN`; die Klassifikation wird aus strukturiertem Aufruf,
Target und OS-Grenze abgeleitet, nicht allein aus Toolname oder Prompt.

Vor Ausführung: Policy-, Scope-, Ownership-, Secret-, Recovery- und Human-Confirmation-Prüfung.
Nachher: begrenzte Observation, Veränderungsnachweis, Transaction-/Action-Journal und Pi-sicheres
Resultat. Unbekannte Mutation wird standardmäßig blockiert oder als konkrete Vorschau/Anfrage an den
Nutzer zurückgegeben. Der NixOS Safety Core bleibt unverändert zuständig für managed Nix Changes.

## Konsequenzen

Pi Toolannotations und Shell-Parsergebnisse sind Hinweise, keine Autorisierung. Alle Adapter und MCP
Calls müssen Gateway IDs/Evidence referenzieren; direkte Tool-zu-System-Ausführung ist unzulässig.
