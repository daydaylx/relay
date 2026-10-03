# 01 – Produktvision

## Benutzererlebnis

Der Nutzer soll beispielsweise schreiben können:

- „Installiere VLC.“
- „Warum geht Bluetooth nicht?“
- „Mach die Workspace-Animation etwas schneller.“
- „Welche Dienste sind kaputt?“
- „Seit der letzten Änderung geht WLAN nicht mehr.“
- „Mach das Dock kleiner, aber ändere sonst nichts.“
- „Was hat sich seit gestern am System geändert?“

Der Nutzer soll **nicht** wissen müssen, ob dafür NixOS, Home Manager, systemd,
Hyprland, journalctl, Relay oder eine Config-Datei benötigt wird.

## Verhalten

Der Agent soll:

1. die Absicht verstehen,
2. den aktuellen Systemzustand ermitteln,
3. relevante Daten lesen,
4. Ursache oder Zielzustand bestimmen,
5. den passenden Änderungspfad wählen,
6. eine verständliche Vorschau liefern,
7. notwendige Bestätigung einholen,
8. Änderung kontrolliert durchführen,
9. tatsächlichen Effekt prüfen,
10. den neuen Zustand dokumentieren.

## Kein Coding-Agent-Klon

Das Produkt soll nicht versuchen, Codex oder Claude Code vollständig nachzubauen.

Komplexe Entwicklungsarbeiten dürfen weiterhin außerhalb des täglichen Systemagenten
stattfinden. Der Systemagent wird für **lokale Systemverwaltung, Diagnose,
Konfiguration und sichere Änderungen** optimiert.

## Erfolgskriterium

Ein normaler NixOS-Anwender soll wiederkehrende Systemarbeit überwiegend in einfacher
Sprache erledigen können, ohne jedes Mal einen allgemeinen Coding-Agenten in das
Konfigurationsrepo schicken zu müssen.
