# 06 – Task Router und Workflows

## Ziel

Der Nutzer soll nicht wählen müssen, welches Backend verwendet wird.

## V1-Klassen

### INSPECT

Beispiele:
- „Welche NixOS-Version läuft?“
- „Welche Services sind fehlgeschlagen?“

Nur lesen.

### DIAGNOSE

Beispiele:
- „Warum geht Bluetooth nicht?“
- „Wieso startet mein Dock nicht?“

Mehrere strukturierte Reads, Hypothesenbildung, noch keine Mutation.

### RELAY_CHANGE

Beispiele:
- Paket installieren/entfernen
- unterstützte NixOS-Option setzen

Pfad:

```text
Intent
→ Relay plan
→ preview
→ confirm
→ apply
→ verify
```

### USER_CONFIG_CHANGE

Später:

- Hyprland
- Home Manager
- Waybar

Darf erst aktiviert werden, wenn dafür ein eigenes sicheres Backend existiert.

### DEVELOPMENT_REQUIRED

Wenn eine gewünschte Änderung außerhalb der vorhandenen Backends liegt:

- nicht improvisiert Root-Shell verwenden,
- erklären, dass eine Konfigurations-/Entwicklungsänderung nötig ist,
- optional einen Plan erzeugen.

## Router

Der Router darf modellgestützt sein, aber die erlaubten Actions und Backend-Capabilities
müssen technisch validiert werden.

Das Modell kann sagen:

```json
{"route":"relay_change", "intent": ...}
```

Der Code entscheidet, ob diese Route tatsächlich zulässig ist.

## Fallback

Bei Unsicherheit:

```text
inspect/diagnose → erklären → nicht mutieren
```

Nicht „best guess + sudo“.
