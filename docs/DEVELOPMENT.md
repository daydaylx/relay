# Development

## Rust workspace

Relay is a dependency-free Rust workspace. It requires Rust/Cargo 1.85 or newer (edition 2024).

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
```

The GitHub Actions workflow runs the same checks. No system packages or NixOS activation are used by CI.

## Current observer prototype

```sh
cargo run -p relay -- status
cargo run -p relay -- generations
```

Both commands emit JSON and read only host metadata and Nix generation symlinks. `status` reports active and booted generation numbers separately when their symlink targets match an entry in the system profile. For fixture-based local development, pass `--root PATH` after the command. Unavailable status fields are returned as `null`; an unreadable generation profile directory is an explicit error. `config_identity`, `nixpkgs_revision`, failed systemd units and desktop/session data remain `null` until a validated, read-only source is implemented.

`search-option` and `search-package` are not implemented yet. The proposed version-bound local index source and cache identity are documented in [`architecture/06_KNOWLEDGE_AND_OPTIONS.md`](architecture/06_KNOWLEDGE_AND_OPTIONS.md). Generator commands and JSON compatibility still need validation against a supported NixOS/nixpkgs toolchain before either search command is enabled. No Nix evaluation or network fetch is performed by the current prototype.

## Environment status

The development shell used for the observer work reports Rust/Cargo 1.98.1 and Nix 2.34.8. It does not provide `nixos-version` or `nixos-rebuild`, so no NixOS version or compatibility range has been validated. The user reports that NixOS is installed on disk; its boot/runtime access and version remain unverified in this shell. Do not use the daily-driver host for activation tests; use an isolated NixOS VM for integration validation.

The local options/package index generators remain blocked until structured JSON output is validated against an explicitly supported NixOS/nixpkgs baseline. No index generation, Nix evaluation or network fetch is performed by the current prototype.
