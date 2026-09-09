{
  description = "curatarr - ebook, comic and manga acquisition manager";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
  };

  outputs =
    { self
    , nixpkgs
    ,
    }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      linuxSystems = [ "x86_64-linux" "aarch64-linux" ];
      forLinux = f: nixpkgs.lib.genAttrs linuxSystems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (pkgs: rec {
        curatarr = pkgs.callPackage ./nix/package.nix { };
        default = curatarr;
      });

      overlays.default = final: _prev: {
        curatarr = final.callPackage ./nix/package.nix { };
      };

      # `services.curatarr.*` - see nix/module.nix for the option set.
      nixosModules = rec {
        curatarr = { pkgs, lib, ... }: {
          imports = [ ./nix/module.nix ];
          services.curatarr.package =
            lib.mkDefault self.packages.${pkgs.stdenv.hostPlatform.system}.curatarr;
        };
        default = curatarr;
      };

      checks =
        let
          common = forAllSystems (pkgs:
            let
              curatarr = self.packages.${pkgs.stdenv.hostPlatform.system}.curatarr;
            in
            {
              package = curatarr;

              clippy = curatarr.overrideAttrs (old: {
                pname = "curatarr-clippy";
                nativeBuildInputs = (old.nativeBuildInputs or [ ]) ++ [ pkgs.clippy ];
                buildPhase = ''
                  cargo clippy --workspace --all-targets --offline -- -D warnings
                '';
                installPhase = "touch $out";
                doCheck = false;
              });

              rustfmt = pkgs.runCommand "curatarr-rustfmt"
                { nativeBuildInputs = [ pkgs.cargo pkgs.rustfmt ]; }
                ''
                  cd ${curatarr.src}
                  cargo fmt --all --check
                  touch $out
                '';
            });

          vm = forLinux (pkgs: {
            nixos-module = import ./nix/vm-test.nix {
              inherit pkgs;
              module = self.nixosModules.default;
            };
          });
        in
        nixpkgs.lib.recursiveUpdate common vm;

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            rust-analyzer
            cargo-nextest
            sqlite
            nixpkgs-fmt
          ];
          shellHook = ''
            echo "curatarr dev shell"
            echo "  cargo nextest run --workspace"
            echo "  cargo clippy --workspace --all-targets -- -D warnings"
            echo "  nix build .#curatarr"
            echo "  nix flake check"
          '';
        };
      });
    };
}
