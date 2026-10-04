# ADR 0015 – Breite Pi-Werkzeuge mit OS-Grenze

Status: Accepted for staged implementation

## Entscheidung

Pi bekommt arbeitsfähige Relay-Varianten für `bash`, `read`, `write`, `edit`, `search`, `grep`, `find`,
Prozessausführung, `git` und NixOS-/systemd-/Hardware-/Netzwerk-/Audio-/Storage-/Hyprland-Diagnose.
Bekannte Aktionen verwenden zuerst Relay High-Level Tools. Unbekannte Diagnose darf strukturierte
und Shell-Werkzeuge kombinieren. Kein Modellaufruf erhält einen nackten Host-Subprocess- oder sudo-
Pfad.

Shell-/Interpreter-Kommandos sind wegen Expansion, Pipes, redirection, Kindern und Plugins nicht
vollständig per Whitelist klassifizierbar. `OBSERVE`-Bash läuft daher in einer OS-erzwungenen, read-
only Linux Sandbox mit deny-read Secrets, getrennten Namespaces, limits und gesperrten mutierenden
System-/Session-Sockets. Kontrollierte User Writes laufen über begrenzte, transaktionale Writable Scopes.
Runtime Mutations verwenden getypte Relay-Adapter; Nix Changes den vorhandenen Candidate Core.

Bis Linux Enforcement, Scope Leakage und adversariale Escape-Tests bestehen, bleibt der bisherige
kleinere Read-Only-Toolpfad verfügbar und allgemeine Bash-Mutation gesperrt. Pi/RPC-Prozessgrenze allein
ist keine Sandbox.

## Konsequenzen

`python`, `bash`, `node` oder `git` werden nach Pfad, argv, cwd, env, file descriptors, target, Privilege
und Sandbox-Profil beurteilt. Parserklassifikation unterstützt Review, ist aber nie der alleinige
Schutz. Sandbox Backend muss NixOS und unprivilegierte User Namespaces praktisch nachweisen.
