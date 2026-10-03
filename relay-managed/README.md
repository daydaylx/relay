# relay-managed

Dokumentiert den Relay-Schreibbereich. Produktiv besitzt Relay genau eine Datei im Konfigurations-
Flake des Nutzers:

```text
relay/managed.nix
```

`relay init --flake PATH` legt sie an. Relay editiert keine andere Datei; das Modul muss einmalig von
Hand in die Host-Konfiguration importiert werden (`modules = [ … ./relay/managed.nix ];`) und von
Git verfolgt sein.

## Kanonische Form

Die Datei ist eine reine Funktion des Relay-Datenmodells und wird deterministisch erzeugt:

```nix
# Managed by Relay. Edit only through `relay plan` and `relay apply`.
{ pkgs, ... }:
{
  environment.etc."relay/managed.nix".source = ./managed.nix;
  environment.systemPackages = with pkgs; [
    vlc
  ];
  hardware.bluetooth.enable = true;
}
```

- Pakete alphabetisch, Optionen alphabetisch nach Pfad.
- Unterstützte Werte: `true`/`false`, ganze Zahlen, Strings, Listen von Strings. Strings werden
  escaped (`\"`, `\\`, `\n`, `\r`, `\t`, `\${`).
- Die Stempel-Zeile veröffentlicht die angewendete Datei als `/etc/relay/managed.nix`. Damit
  prüft Relay, dass das Modul importiert ist und dass laufendes System und Quelle zusammenpassen.
- Relay liest die Datei nur zurück, wenn sie byte-genau der kanonischen Ausgabe entspricht. Von
  Hand bearbeitete Dateien werden gemeldet und nie überschrieben.
- Geheimnisse gehören nicht hierher (Option-Namen mit `password`, `secret`, `token`, `key`-ähnlichen
  Bestandteilen werden abgelehnt), denn alles landet im Nix-Store.
