# ADR 0017 – Desired-State Tasks und Controller Loop

Status: Accepted for implementation

## Entscheidung

Ein User-Ziel wird als langlebiger Task mit `desired_state`, Beobachtungen, Hypothesen/Agent Notes,
Recherche/Evidence, Aktions-/Mutation-IDs, Verifikationen und Resultat geführt. Task-State umfasst
`created`, `investigating`, `researching`, `planning`, `waiting_confirmation`, `executing`,
`verifying`, `continuing`, `completed`, `blocked`, `failed`, `cancelled`. Relay Facts bleiben getrennt
von Pi-Notes.

Pi darf bei fehlender Verifikation weiter beobachten, recherchieren, eine neue Aktion planen und den
Zielvergleich erneut versuchen. `completed` erfordert einen strukturierten Desired-State-Comparator
gegen frische Core Observations samt Evidence; Exitcode und Modelltext allein reichen nicht. Budgets
gelten für Ressourcen/Loops, nicht als Ein-Change-pro-Aufgabe-Schranke.

## Konsequenzen

Task enthält keine secrets; Rohtranskripte liegen separat in Relay Pi Session Store mit definierter
Retention. Neustart invalidiert offene Human Grants. In-flight mutations gehen zuerst durch vorhandene
Recovery, danach kann Pi den Task fortsetzen.
