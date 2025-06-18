{
  sources ? import ./nix/sources.nix,
  pkgs ? import sources.nixpkgs { }
}:

let
  naersk = pkgs.callPackage sources.naersk {};

  # Suggestion from https://christine.website/blog/how-i-start-nix-2020-03-08
  # tell nix-build to ignore the `target` directory
  # The base naersk README does not suggest this filter
  src = builtins.filterSource
    (path: type: type != "directory" || builtins.baseNameOf path != "target")
    ./.;
in naersk.buildPackage {
  src = src;

  nativeBuildInputs = with pkgs; [
    # general build stuff
    pkg-config

    # openssl
    openssl

    # magick-rust
    imagemagick
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
  ];

  # Taken from nixos.wiki
  # https://nixos.wiki/wiki/Rust
  LIBCLANG_PATH= pkgs.lib.makeLibraryPath [ pkgs.llvmPackages_latest.libclang.lib ];
  BINDGEN_EXTRA_CLANG_ARGS = 
  # Includes with normal include path
  (builtins.map (a: ''-I"${a}/include"'') [
    pkgs.glibc.dev 
  ])
  # Includes with special directory paths
  ++ [
    ''-I"${pkgs.llvmPackages_latest.libclang.lib}/lib/clang/${pkgs.llvmPackages_latest.libclang.version}/include"''
    ''-I"${pkgs.glib.dev}/include/glib-2.0"''
    ''-I${pkgs.glib.out}/lib/glib-2.0/include/''
  ];
}
