# ADR 0020 – Kurzlebiger Privilege Broker

Status: Accepted for staged implementation

## Entscheidung

Relay/Pi laufen unprivilegiert. Root-Aufgaben werden nur über konkrete, strukturierte Broker Actions
mit executable/argv, target, Vorbedingungen, Task-ID, Scope, Risiko, Recovery und beobachtetem Ergebnis
ausgeführt. Es gibt keinen permanenten Root-Daemon, kein modellzugängliches root shell und kein freies
`sudo <string>`.

CONTROL kann eine task-lokale Erlaubnis für abgegrenzte, reversible Aktionsklassen bündeln. ADMIN ist
separat explizit aktiviert. Unbekannte/privilegierte/destruktive Operationen, Disk/Boot/Auth/SSH/Secrets
und protected resources übersteuern Task-Grants und verlangen eigene direkte konkrete Bestätigung oder
bleiben verboten. Der bestehende typed Nix activation path bleibt eigenständig und vorrangig.

## Konsequenzen

Broker hat minimierte ausführbare Aktionen, keine vom Pi veränderbare Policy, kurze Lebensdauer,
request/response Audit und Fail-closed. Broker-Implementierung beginnt erst nach Gateway Sandbox und
adversarial Tests.
