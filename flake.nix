{
  description = "Rio | A hardware-accelerated GPU terminal emulator";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    systems.url = "github:nix-systems/default";
  };

  outputs = inputs @ {flake-parts, ...}:
    flake-parts.lib.mkFlake {inherit inputs;} {
      imports = [flake-parts.flakeModules.easyOverlay];

      systems = import inputs.systems;

      perSystem = {
        self',
        inputs',
        pkgs,
        system,
        lib,
        ...
      }: let
        # Defines a devshell using the `rust-toolchain`, allowing for
        # different versions of rust to be used.
        mkDevShell = rust-toolchain: let
          runtimeDeps = self'.packages.rio.runtimeDependencies;
          tools =
            self'.packages.rio.nativeBuildInputs ++ self'.packages.rio.buildInputs ++ [rust-toolchain];
        in
          pkgs.mkShell {
            packages = [self'.formatter] ++ tools;
            LD_LIBRARY_PATH = "${lib.makeLibraryPath runtimeDeps}";
          };
        toolchains = rec {
          msrv = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          stable = pkgs.rust-bin.stable.latest.minimal;
          nightly = pkgs.rust-bin.selectLatestNightlyWith (toolchain: toolchain.minimal);
          default = msrv;
        };
      in {
        formatter = pkgs.alejandra;
        _module.args.pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [(import inputs.rust-overlay)];
        };

        # Create overlay to override `rio` with this flake's default
        overlayAttrs = {rio = self'.legacyPackages.rio.default;};
        packages = rec {
          rio = self'.legacyPackages.rio.default;
          default = rio;
        };
        # Use `legacyPackages` to allow the `pkg.subpkg` style.
        legacyPackages.rio =
          lib.mapAttrs (
            _: v: pkgs.callPackage ./pkgRio.nix {rust-toolchain = v;}
          )
          toolchains;
        # Different devshells for different rust versions
        devShells = lib.mapAttrs (_: v: mkDevShell v) toolchains;
      };
    };
}
