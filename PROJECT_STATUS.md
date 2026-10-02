# Relay – Project Status

## Current phase

**T0 bootstrap started; T1 observer prototype remains incomplete.**

A dependency-free Rust workspace and read-only `status` / `generations` CLI exist. The observer reports active and booted generations separately when their profile links are available. Schema-v1 `search-option` / `search-package` commands can search an explicitly supplied JSON index; the index generator, host freshness/identity validation, and NixOS-host/VM integration remain blocked pending a validated NixOS/nixpkgs baseline. Fixture-based Rust checks run in the current development environment.

## Current decisions

- Product name: Relay
- Target: NixOS-first
- Runtime: standalone system tool
- Pi dependency: none
- AI: optional
- Preferred core language: Rust
- Initial config backend: Flake
- Later backend candidate: `system.nix`
- Managed write boundary: dedicated Relay-managed Nix module

## Current target

Reach **MVP Target State T1** defined in:

```text
docs/planning/04_TARGET_STATES.md
```

## Remaining bootstrap checks

- Confirm the Rust workspace/crate naming (`relay` is the current provisional name).
- Confirm access to an isolated development/test NixOS environment. The user reports that NixOS is installed on disk, but this session is not running `nixos-version`/`nixos-rebuild` and has not verified or booted that installation; do not use the daily-driver for activation tests.
- Development shell observed: Rust/Cargo 1.98.1 and Nix 2.34.8. These are not yet an established supported NixOS compatibility range; record a NixOS version after validating the installed system or isolated VM.
- Create milestones/issues from the work order.
- Decide on a license before public distribution; no license was selected or added.
- Establish and document a baseline commit when explicitly authorized; no commit has been created.
- Run the full Rust CI check set after implementation changes. `nixos-version` and `nixos-rebuild` are unavailable in the current shell, so NixOS integration and index-generator validation remain outstanding.
- Repository has no commits yet; all project files are currently untracked. Preserve this state; a baseline commit requires explicit authorization.

## Explicitly deferred

- Home Manager
- secrets
- Hyprland automation
- partition/storage management
- bootloader management
- system.stateVersion mutation
- major NixOS upgrades
- subagents
- MCP
- plugins
- remote management
