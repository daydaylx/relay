{ lib, rustPlatform, bubblewrap, makeWrapper }:

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
  nativeBuildInputs = [ makeWrapper ];
  postInstall = ''
    wrapProgram $out/bin/relay --set RELAY_BWRAP_PATH ${bubblewrap}/bin/bwrap
  '';
  passthru.runtimeDependencies = [ bubblewrap ];

  meta = {
    description = "NixOS-first system control tool: intent to safe, recoverable NixOS changes";
    license = lib.licenses.mit;
    mainProgram = "relay";
    platforms = lib.platforms.linux;
  };
}
