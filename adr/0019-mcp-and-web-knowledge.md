# ADR 0019 – Relay Knowledge Broker für MCP und Web

Status: Accepted for staged implementation

## Entscheidung

MCP und Web Search/Fetch sind Relay-eigene, konfigurierbare Knowledge-Adapter; keine Pi-User-/Projekt-
MCP-Konfiguration wird importiert. Trust Classes sind `TRUSTED_LOCAL`, `TRUSTED_OFFICIAL`,
`EXTERNAL_READ`, `EXTERNAL_WRITE`, `PRIVILEGED`. Erste Server sind nur offizielle read-only
Dokumentationsquellen. Jede Server-Toolausführung läuft durch Relay Gateway; deklarierte MCP Hints
werden nicht als Sicherheitsregel vertraut.

Web Search ist bei Bedarf autonom verfügbar. Fetch hat nur HTTP(S), öffentliche Internetziele,
DNS-/Redirect Revalidation, private/loopback/link-local/reserved IP Sperren, connect/read deadlines,
body/MIME/redirect bounds und keinerlei Cookies/Auth/session credentials. Prompt injection bleibt
untrusted content. Write/PRIVILEGED MCP Tools sind initial deaktiviert und benötigen spätere eigene
Policy/ADR.

## Konsequenzen

Server-Binärdatei/URL, Version, Berechtigungen, Trust Class und Env-Allowlist stammen aus Relays eigener
Config. Stdio Server laufen mit eigener minimaler Env unter Sandbox. Fetch/Search liefern Evidence-
Datensätze statt unmarkiertem Prompttext.
