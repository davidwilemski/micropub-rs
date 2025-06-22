{
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    naersk.url = "github:nix-community/naersk";
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.05";
  };

  outputs = { self, flake-utils, naersk, nixpkgs }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = (import nixpkgs) {
          inherit system;
        };

        nativeBuildInputs = with pkgs; [
          # general build stuff
          pkg-config

          # openssl
          openssl

          # magick-rust
          imagemagick
          llvmPackages.clang
          llvmPackages.libclang
          llvmPackages.lld
          llvmPackages.llvm
          # llvmPackages.llvm.dev

          # micropub-rs
          sqlite
          diesel-cli

          cargo
          rustc

          coreutils
          which
          clang

          fnm
        ];

        naersk' = pkgs.callPackage naersk {};
        naerskBuildPackage = args:
          naersk'.buildPackage (
            args // {
              nativeBuildInputs = nativeBuildInputs;

              # Taken from nixos.wiki
              # https://nixos.wiki/wiki/Rust
              LIBCLANG_PATH= pkgs.lib.makeLibraryPath [ pkgs.llvmPackages_latest.libclang.lib ];
            }
          );

      # Suggestion from https://christine.website/blog/how-i-start-nix-2020-03-08
      # tell nix-build to ignore the `target` directory
      # The base naersk README does not suggest this filter
      src = builtins.filterSource
        (path: type: type != "directory" || builtins.baseNameOf path != "target")
        ./.;

      in rec {

        # For `nix build` & `nix run`:
        packages = {
          default = naerskBuildPackage {
            src = src;
          };
          # Run `nix build .#check` to check code
          check = naerskBuildPackage {
            src = src;
            mode = "check";
          };
          # Run `nix build .#test` to run tests
          test = naerskBuildPackage {
            src = src;
            mode = "test";
          };
          # Run `nix build .#clippy` to lint code
          clippy = naerskBuildPackage {
            src = src;
            mode = "clippy";
          };
        };

        # For `nix develop` (optional, can be skipped):
        devShell = pkgs.mkShell {
          nativeBuildInputs = nativeBuildInputs;

          # Taken from nixos.wiki
          # https://nixos.wiki/wiki/Rust
          LIBCLANG_PATH= pkgs.lib.makeLibraryPath [ pkgs.llvmPackages_latest.libclang.lib ];
        };
      }
    );
}
