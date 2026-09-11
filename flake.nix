{
  description = "embassy flake";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    systems.url = "github:nix-systems/default";
    flake-parts.url = "github:hercules-ci/flake-parts";
    fenix.url = "github:nix-community/fenix/monthly";
    naersk = {
      url = "github:nix-community/naersk";
      inputs.fenix.follows = "fenix";
    };
  };

  outputs = inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } ({ ... }: {
      systems = import inputs.systems;
      perSystem = { pkgs, system, inputs', ... }: 
      let
        probe-rs-tools = pkgs.probe-rs-tools.overrideAttrs {
          cargoBuildFeatures = [ "remote" ];
        };
        fpkgs = inputs'.fenix.packages;
        profile = fpkgs.complete;
        std-lib = fpkgs.targets.thumbv7em-none-eabihf.latest;
        rust-analyzer-nightly = fpkgs.rust-analyzer;
        rust-toolchain = fpkgs.combine [
          profile.rustc
          profile.miri
          profile.rust-src
          profile.cargo
          profile.rustfmt
          profile.clippy
          profile.llvm-tools
          std-lib.rust-std
        ];
        trex-probe = 
        (inputs.naersk.lib.${system}.override {
          cargo = rust-toolchain;
          rustc = rust-toolchain;
        }).buildPackage {
          src = ./.;
          cargoBuildOptions = a: a ++ ["-p trex-probe" "--target=$(rustc -vV | grep host | cut -d ' ' -f 2)"];
        };
        trex-cli = 
        (inputs.naersk.lib.${system}.override {
          cargo = rust-toolchain;
          rustc = rust-toolchain;
        }).buildPackage {
          src = ./.;
          cargoBuildOptions = a: a ++ ["-p trex-cli" "--target=$(rustc -vV | grep host | cut -d ' ' -f 2)"];
        };
      in {
        devShells.default =
        pkgs.mkShell {
          buildInputs = with pkgs; [
            rust-toolchain
            rust-analyzer-nightly

            # extra cargo tools
            cargo-edit
            cargo-expand
            cargo-show-asm
            cargo-binutils

            # utilities for shell scripts
            jq
            ripgrep

            # local tools
            trex-probe
            trex-cli

            # for flashing
            probe-rs-tools
          ];

          # set the rust src for rust_analyzer
          RUST_SRC_PATH = "${rust-toolchain}/lib/rustlib/src/rust/library";
          # set default defmt log level
          DEFMT_LOG = "info";
        };
      };
    });
}

