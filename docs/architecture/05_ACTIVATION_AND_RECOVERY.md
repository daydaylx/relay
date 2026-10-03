# Aktivierung und Recovery

## LIVE_SWITCHABLE

```text
build → preview → test → health → switch
```

## REBOOT_REQUIRED

```text
build → preview → boot → reboot → post-boot health
```

## MIGRATION_REQUIRED

Keine automatische Aktivierung im MVP.

## PROTECTED

Keine automatische Mutation im MVP.

Beispiele:

- system.stateVersion
- Partitionierung
- Filesystems
- LUKS
- Bootloader
- Secure Boot
- fundamentale Auth-/SSH-Konfiguration

## Wichtige Korrektur

`nixos-rebuild test` ist kein automatischer Rollback.

Relay Undo muss Source und Runtime gemeinsam zurücksetzen.

## Umsetzung (Stand T3–T4)

### Zustandsautomat des Journals

```text
planned → built → source-applied → test-activated → verified → switched
                       │                  │             │
                       ├→ reboot-pending ─┴──────────────┘ (reboot-pending → verified → switched nach dem Reboot)
                       └→ rollback-started → rolled-back
jeder nicht-terminale Zustand → failed
switched → rollback-started   (relay undo)
```

Übergänge werden vor der angekündigten Aktion geschrieben (write-ahead). `source-applied`,
`test-activated`, `verified`, `reboot-pending` und `rollback-started` sind „in flight“: solange ein
solcher Eintrag existiert, starten weder `plan`, `apply` noch `undo`; `relay recover` löst ihn auf.

### LIVE_SWITCHABLE

```text
preflight → dry-activate (Gate) → source-applied → managed.nix schreiben → Re-Evaluation/Identität
→ test-activated → switch-to-configuration test → Health (Baseline + Beobachtungsfenster)
→ verified → nix-env --set <kandidat> → switch-to-configuration switch → switched
```

Jeder Fehler ab `source-applied` führt zu `rollback-started` → Quelle und Runtime zurück →
`rolled-back` (Exit-Code 2) oder, wenn der Rollback selbst scheitert, `failed` mit Reason
`rollback-incomplete`.

### REBOOT_REQUIRED

```text
… → reboot-pending → nix-env --set <kandidat> → switch-to-configuration boot
Reboot, dann: relay recover → Health → verified → switched
oder: relay recover --abort-pending → Profil zurück + boot
```

`test` und `switch` werden dafür nie aufgerufen; auch das Zurückrollen geht nur über die
Boot-Konfiguration.

### Recovery aus Evidenz

`restore_source` schreibt nur zurück, wenn `managed.nix` noch exakt dem Inhalt entspricht, den die
Änderung geschrieben hat. `restore_runtime` liest `run/current-system` und das Systemprofil:

| Profil | Laufendes System | Aktion |
| --- | --- | --- |
| Kandidat | beliebig (Vorgänger/Kandidat) | Profil auf Vorgänger; `switch` bzw. `boot` des Vorgängers |
| Vorgänger | Kandidat | `test` des Vorgängers |
| Vorgänger | Vorgänger | nichts |
| irgendetwas anderes | irgendetwas anderes | **keine Aktion**, `rollback-incomplete` |

`recover` für `verified`: Ist die Quelle angewendet und läuft der Kandidat, wird nach erneutem
Health-Check vorwärts abgeschlossen; sonst Rollback. `undo` verweigert, wenn die Quelle seit der
Änderung verändert wurde oder das laufende System nicht mehr dem Kandidaten entspricht.
