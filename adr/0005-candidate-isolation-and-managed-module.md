# ADR 0005 – Candidate-Isolation und kanonisches Managed-Modul

Status: Accepted

## Kontext

Relay muss eine Änderung vollständig evaluieren und bauen können, ohne das Live-System oder die
Live-Konfiguration anzufassen (ADR 0003), und es muss vor dem Anwenden sicher erkennen, ob die
Quelle sich geändert hat.

## Entscheidung

- **Kandidat = Kopie.** Ein Plan kopiert den Quellbaum (bei Git-Checkouts nur die von Git
  verfolgten Dateien, also genau das, was Nix sieht) in ein Relay-eigenes Verzeichnis
  (`<state>/candidates/<id>/src`) und schreibt nur dort `relay/managed.nix`. Der Kandidat wird immer
  als `path:`-Flake adressiert, damit ein umgebendes Git-Repository ignoriert wird. Das State-
  Verzeichnis darf nicht innerhalb der Quelle liegen.
- **Identität = Inhaltshash.** Quell-, Kandidaten- und Managed-Identität sind SHA-256-Hashes über
  Pfad, Modusbit und Inhalt (bzw. Link-Ziel). Vor `apply` werden Quelle, laufendes System,
  Systemprofil und Kandidat gegen den Plan geprüft; jede Abweichung bricht ohne Mutation ab.
- **`managed.nix` ist eine reine Funktion eines Datenmodells** (`ManagedState`). Relay liest die
  Datei nur zurück, wenn sie exakt der eigenen kanonischen Ausgabe entspricht (Parse → Re-Render →
  Byte-Vergleich). Von Hand veränderte Dateien werden gemeldet, nie geraten oder überschrieben.
- **Laufzeit-Stempel.** Das Modul veröffentlicht sich selbst als `/etc/relay/managed.nix`. Damit
  ist ohne Nix-Parsing prüfbar, dass die Host-Konfiguration das Modul importiert, und Relay kann
  Quelle und laufendes System vergleichen. Weichen sie ab, wird nicht geplant.
- **Rollback nur auf bekanntem Inhalt.** Die Quelle wird nur zurückgesetzt, wenn die Datei noch
  exakt dem Stand entspricht, den die Änderung geschrieben hat. Fremde Änderungen bleiben liegen.

## Folgen

- Konfigurationen, die `self.rev`/`self.lastModified` in Derivationen einbauen, ergeben für
  Kandidat (`path:`) und Live-Quelle (Git) unterschiedliche Derivationen. Relay bricht dann vor der
  Aktivierung ab (fail closed).
- Nur Optionspfade aus Bezeichnern (`a.b-c`) und Pakete aus einfachen Attributnamen werden
  unterstützt; alles andere wird bei der Validierung abgelehnt.
