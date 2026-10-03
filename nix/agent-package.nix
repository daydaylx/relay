{ buildNpmPackage, lib, makeWrapper, nodejs_22 }:

buildNpmPackage {
  pname = "relay-system-agent";
  version = "0.1.0";
  src = ../agent;
  npmDepsHash = "sha256-DFGJqMw+SERqqpTK8754QgE3EO/0JtUUxGzZbZzNO2w=";
  npmInstallFlags = [ "--ignore-scripts" ];
  npmBuildScript = "typecheck";
  nativeBuildInputs = [ makeWrapper ];
  dontNpmPrune = true;

  installPhase = ''
    runHook preInstall
    mkdir -p "$out/lib/relay-agent" "$out/bin"
    cp -r src node_modules package.json "$out/lib/relay-agent/"
    makeWrapper ${nodejs_22}/bin/node "$out/bin/relay-agent" \
      --add-flags "$out/lib/relay-agent/node_modules/tsx/dist/cli.mjs" \
      --add-flags "$out/lib/relay-agent/src/main.ts"
    runHook postInstall
  '';

  meta = {
    description = "Optional local Relay system agent using official Pi packages";
    mainProgram = "relay-agent";
    platforms = [ "x86_64-linux" ];
  };
}
