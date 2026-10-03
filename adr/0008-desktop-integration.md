# ADR 0008 – Desktop-Integration: Hyprland read-only und als Teil der Health-Prüfung

Status: Accepted

## Kontext

Zielzustand T6 nennt ein Hyprland-Systemmodul, Runtime-IPC, Workspaces/Fenster/Monitore und
Desktop-Health. Home Manager bleibt eine getrennte spätere Entscheidung. „Hyprland-Automatisierung“
(Fenster steuern, Kompositor-Konfiguration ändern) ist laut Projektstatus ausdrücklich zurückgestellt.

## Entscheidung

- **Read-only IPC.** Relay spricht den eigenen Socket des Kompositors
  (`$XDG_RUNTIME_DIR/hypr/<signature>/.socket.sock`) mit einer festen Allow-List von JSON-Abfragen
  (`version`, `monitors`, `workspaces`, `activewindow`, `configerrors`). Es gibt keinen Pfad für
  `dispatch`, `keyword`, `reload` oder `exec`; ein Test erzwingt das. Fenstertitel werden nie in
  Relays Zusammenfassungen übernommen (Privatsphäre); gezeigt werden nur Zahlen, Monitore,
  Workspaces und die Fensterklasse des aktiven Fensters.
- **Desktop-Health im Sicherheitskreislauf.** Läuft eine Hyprland-Sitzung, nimmt `apply` vor der
  Änderung eine Baseline (aktivierte Monitore, Konfigurationsfehler) und prüft sie nach `test`
  und Systemhealth erneut. Verlorene Monitore, ein nicht mehr antwortender Kompositor oder *neue*
  Konfigurationsfehler führen zum Rollback (`desktop-check-failed`). Vorbestehende Fehler werden
  nicht der Änderung angelastet. Ist der Kompositor beim Start nicht erreichbar (TTY, SSH), wird
  die Prüfung sichtbar übersprungen (Hinweis im Ergebnis), nicht stillschweigend.
  `--no-desktop-check` schaltet sie ab. Reboot-Änderungen und `recover` nach einem Neustart prüfen
  den Desktop nicht (die Sitzung gibt es dann nicht).
- **Systemmodul.** Hyprland wird über die NixOS-Optionen `programs.hyprland.*` verwaltet, wie jede
  andere Option typisiert und evaluiert (live schaltbar, durch die Desktop-Health abgesichert).
  Session-Fundamente (`services.displayManager.*`, `services.xserver.displayManager.*`,
  `services.greetd.*`, `programs.regreet.*`, `programs.uwsm.*`, `services.cage.*`) gelten als
  `REBOOT_REQUIRED`, weil ein Live-Wechsel die laufende grafische Sitzung beenden könnte.
- **Nicht angefasst:** Dotfiles (`hypr/*.conf` im Nutzer-Flake), Home Manager, Steuerung des
  Kompositors.

## Folgen

- `relay desktop status|health` und das Feld `desktop` in `relay status` sind rein lesend und laufen
  nur in der Sitzung.
- Die NixOS-VM des Tests enthält keinen Kompositor; die Desktop-Prüfung ist im Simulator, gegen
  einen Fake-Socket und manuell gegen eine laufende Hyprland-Sitzung belegt.
