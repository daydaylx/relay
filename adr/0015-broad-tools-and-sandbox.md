# ADR 0015 – Breite Pi-Werkzeuge mit OS-Grenze

Status: Accepted for staged implementation

## Entscheidung

Pi bekommt arbeitsfähige Relay-Varianten für `bash`, `read`, `write`, `edit`, `search`, `grep`, `find`,
Prozessausführung, `git` und NixOS-/systemd-/Hardware-/Netzwerk-/Audio-/Storage-/Hyprland-Diagnose.
Bekannte Aktionen verwenden zuerst Relay High-Level Tools. Unbekannte Diagnose darf strukturierte
und Shell-Werkzeuge kombinieren. Kein Modellaufruf erhält einen nackten Host-Subprocess- oder sudo-
Pfad.

Shell-/Interpreter-Kommandos sind wegen Expansion, Pipes, redirection, Kindern und Plugins nicht
vollständig per Whitelist klassifizierbar. `OBSERVE`-Programme laufen daher in einer OS-erzwungenen,
read-only Bubblewrap-Sandbox. Sie erhalten nur den rekursiven Nix-Store-Closure des ausgewählten
Programms unter einem nicht beschreibbaren `/nix/store`-Verzeichnis, ausgewählte read-only
Hardware-Bäume (`/sys/devices`, `class`, `bus`, `block`, `dev`, `module`), einen privaten `/proc`-
und `/dev`-Baum sowie ein verworfenes `/tmp`. `/sys/firmware`,
`/sys/fs`, Netzwerk, `/home`, `/etc`, `/run`, D-Bus-, Wayland- und Nix-Daemon-Sockets sind nicht eingebunden.
Ein User-systemd-Scope erzwingt Speicher-, Prozesszahl- und CPU-Grenzen; Relay erzwingt zusätzlich
Zeit- und Ausgabegrenzen. Ohne Bubblewrap oder User-systemd-Scope wird OBSERVE verweigert.
Kontrollierte User Writes laufen später über begrenzte, transaktionale Writable Scopes.
Runtime Mutations verwenden getypte Relay-Adapter; Nix Changes den vorhandenen Candidate Core.

Umgesetzt ist zunächst nur ein read-only `relay_observe_command` für strukturierte Programm-/Argument-
Aufrufe; es gibt keinen Host-Shell-, Datei-Schreib- oder Service-Mutationspfad. Der explizite
NixOS-Host-Smoke-Test prüft, dass `/etc`, `/home` und `/run` fehlen, ausgewählte Sysfs-Metadaten
lesbar bleiben, EFI-Variablen nicht sichtbar sind, Schreibversuche auf Hostdateien und Sysfs scheitern,
Netzwerkzugriff scheitert und auch das `/nix/store`-Wurzelverzeichnis nicht beschreibbar ist. `/proc`
zeigt eine private PID-Ansicht, nur vier erwartete Umgebungsvariablen sind vorhanden und `/tmp` wird
pro Aufruf verworfen. Der Test prüft außerdem, dass Kindprozesse keine neuen
User-/Mount-Namespaces und keine Mounts einrichten können. Diese expliziten Hosttests bestanden am
2026-10-04. Sie erhöhen die Escape-Abdeckung, sind aber kein vollständiger Kernel- oder Sandbox-Audit
und beweisen keine allgemeine Sicherheit gegen unbekannte Kernel-Lücken. Das ist weiterhin keine
Freigabe für User-Dateimutationen, privilegierte Befehle oder uneingeschränkte Agenten-Werkzeuge.
Pi/RPC-Prozessgrenze allein ist keine Sandbox.

## Konsequenzen

`python`, `bash`, `node` oder `git` werden nach Pfad, argv, cwd, env, file descriptors, target, Privilege
und Sandbox-Profil beurteilt. Parserklassifikation unterstützt Review, ist aber nie der alleinige
Schutz. Das Backend verlangt ein echtes unprivilegiertes User Namespace, verbietet weitere User
Namespaces im Kindprozess und lehnt fehlende User-systemd-Ressourcengrenzen geschlossen ab.
