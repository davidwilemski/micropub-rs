{ pkgs ? import <nixpkgs> { }
, pkgsLinux ? import <nixpkgs> { system = "x86_64-linux"; }
}:

let
  sources = import ./nix/sources.nix;
  pkgs = import sources.nixpkgs { };
  micropub-rs = import ./micropub-rs.nix { inherit sources pkgs; };

  micropubCargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
  micropubVersion = micropubCargoToml.package.version;

in pkgs.dockerTools.buildLayeredImage {
  name = "dtw0/micropub-rs";
  tag = micropubVersion;

  contents = [
    pkgsLinux.cacert
    micropub-rs
  ];

  config = {
    Cmd = [ "${micropub-rs}/bin/server" ];
  };
}

