# Security Model

Das LLM ist niemals die Security Boundary.

Relay läuft normal unprivilegiert.

Privilegierung nur für konkrete Aktivierungsaktionen.

Kein dauerhaftes Root-Relay.
Keine generische Root-Shell.

## Protected im MVP

```text
system.stateVersion
partitioning
filesystems
LUKS
bootloader
secure boot
nix daemon trust
trusted-users
fundamentale auth/user changes
SSH access foundation
secret management
database major upgrades
major nixpkgs release upgrades
```

Secrets dürfen nicht in Flake, managed.nix, Journal, AI Context oder Nix Store gelangen.

Switch Inhibitors dürfen nicht automatisch umgangen werden.
