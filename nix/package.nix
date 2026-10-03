{ lib, rustPlatform }:

let
  manifest = lib.importTOML ../crates/relay/Cargo.toml;
in
rustPlatform.buildRustPackage {
  pname = "relay";
  inherit (manifest.package) version;

  # Only the Rust sources: documentation edits must not rebuild the package.
  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../crates
    ];
  };
  cargoLock.lockFile = ../Cargo.lock;

  meta = {
    description = "NixOS-first system control tool: intent to safe, recoverable NixOS changes";
    mainProgram = "relay";
    platforms = lib.platforms.linux;
  };
}
