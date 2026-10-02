# Relay Managed Area

Empfohlene Struktur:

```text
~/relay-system/
├── flake.nix
├── flake.lock
├── hosts/
│   └── laptop.nix
├── hardware-configuration.nix
└── relay/
    └── managed.nix
```

Relay schreibt automatisch nur in `relay/managed.nix`.

Andere Nix-Dateien sind im MVP read-only.

Das LLM schreibt `managed.nix` nicht selbst. Es erzeugt typed intents, z. B.:

```text
SetOption("hardware.bluetooth.enable", true)
```

Relay rendert daraus deterministisch Nix-Konfiguration.
