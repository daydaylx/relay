{
  description = "Relay: NixOS-first system control tool";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in {
      packages = forAllSystems (pkgs: rec {
        relay = pkgs.callPackage ./nix/package.nix { };
        default = relay;
      });

      apps = forAllSystems (pkgs: {
        default = {
          type = "app";
          program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.default}/bin/relay";
        };
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [ cargo rustc clippy rustfmt ];
        };
      });

      checks = forAllSystems (pkgs:
        let relay = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
        in {
          # Builds the package and runs its unit and simulator tests.
          package = relay;
          # Real activation and recovery inside a NixOS VM; never touches the host system.
          activation = pkgs.callPackage ./nix/tests/activation.nix { inherit relay; };
        });
    };
}
