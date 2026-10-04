{
  description = "Relay: NixOS-first system control tool";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in {
      packages = forAllSystems (pkgs:
        let
          relay = pkgs.callPackage ./nix/package.nix { };
          agent = pkgs.callPackage ./nix/agent-package.nix { inherit relay; };
        in {
          core = relay;
          inherit relay agent;
          default = pkgs.symlinkJoin {
            name = "relay-with-agent";
            paths = [ relay agent ];
            meta = {
              description = "Relay NixOS system control tool with the optional Pi agent runtime";
              mainProgram = "relay";
            };
          };
        });

      apps = forAllSystems (pkgs: {
        default = {
          type = "app";
          program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.default}/bin/relay";
        };
        agent = {
          type = "app";
          program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.agent}/bin/relay-agent";
        };
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [ cargo rustc clippy rustfmt nodejs_22 bubblewrap ];
          RELAY_BWRAP_PATH = "${pkgs.bubblewrap}/bin/bwrap";
        };
      });

      checks = forAllSystems (pkgs:
        let
          relay = self.packages.${pkgs.stdenv.hostPlatform.system}.relay;
          agent = self.packages.${pkgs.stdenv.hostPlatform.system}.agent;
        in {
          # Builds the package and runs its unit and simulator tests.
          package = relay;
          # Real activation and recovery inside a NixOS VM; never touches the host system.
          activation = pkgs.callPackage ./nix/tests/activation.nix {
            inherit relay;
            inherit agent;
          };
        });
    };
}
