# ADR 0016 – User-Datei-Transaktionen und Foreign Edit Protection

Status: Accepted for implementation

## Entscheidung

Jede Relay-gesteuerte Dateiänderung erhält Task-/Action-ID und private Before-/After-Evidence:
kanonischer Pfad, Ownership, Dateityp, Mode, erlaubte Metadaten, SHA-256 und Content/Blob-Referenz.
Secrets werden nie im Klartext archiviert oder in den Agent-Kontext geladen. Writes gehen über sichere
`openat`/NoFollow-Verzeichniswalks und atomisches Tempfile/rename; Symlinks, Hardlink-Escapes,
unsichere Mount-/Path-Wechsel und unbekannte Ownership blockieren den Vorgang.

Undo schreibt `before` nur zurück, wenn der aktuelle Zustand exakt dem von Relay erzeugten `after`
entspricht. Abweichung ist Foreign Edit und verhindert blindes Überschreiben. Original-Owner/ACL/xattrs
werden nur unterstützt, wenn sicher nachweisbar und wiederherstellbar; andernfalls wird nicht mutiert.
Content-Blobs liegen mode-geschützt in Relays lokalem State, getrennt von Task-Kontext und normalen Logs.

## Konsequenzen

Shell-Schreibzugriff kann erst freigeschaltet werden, wenn Sandbox-Writes in Diffs überführt und durch
dieselbe Transaction API committed werden. Runtime Actions erhalten einen eigenen journaled Undo-Typ;
nicht reversible Operationen sind vor Ausführung markiert.
