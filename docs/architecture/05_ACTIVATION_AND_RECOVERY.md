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
