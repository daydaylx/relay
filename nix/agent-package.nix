{ buildNpmPackage, lib, makeWrapper, nodejs_22, relay }:

buildNpmPackage {
  pname = "relay-system-agent";
  version = "0.1.0";
  src = ../agent;
  npmDepsHash = "sha256-psEruMkKfYs/Xw8e0nxeJYteM4los39fiQHmce4Vruo=";
  npmDepsFetcherVersion = 2;
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
      --add-flags "$out/lib/relay-agent/src/main.ts" \
      --prefix PATH : ${relay}/bin
    runHook postInstall
  '';

  meta = {
    description = "Optional embedded Relay task runtime using official Pi agent libraries";
    mainProgram = "relay-agent";
    platforms = [ "x86_64-linux" ];
  };
}
