{
  description = "Rio | A hardware-accelerated GPU terminal emulator";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    rust-overlay.url = "github:oxalica/rust-overlay";
    systems = {
      url = "github:nix-systems/default";
      flake = false;
    };
  };

  outputs = inputs @ {flake-parts, ...}:
    flake-parts.lib.mkFlake {inherit inputs;} {
      imports = [];

      systems = import inputs.systems;

      perSystem = {
        config,
        self',
        inputs',
        pkgs,
        system,
        lib,
        ...
      }: let
        mkRio = import ./pkgRio.nix;

        mkDevShell = rust-toolchain: let
          runtimeDeps = self'.packages.rio.runtimeDependencies;
          tools = self'.packages.rio.nativeBuildInputs ++ self'.packages.rio.buildInputs ++ [rust-toolchain];
        in
          pkgs.mkShell {
            packages =
              [
                # Derivations in `rust-toolchain` provide the toolchain,
                # which must be listed first to take precedence over nightly.
                rust-toolchain

                # Use rustfmt, and other tools that require nightly features.
                (pkgs.rust-bin.selectLatestNightlyWith (toolchain:
                  toolchain.minimal.override {
                    extensions = ["rustfmt" "rust-analyzer"];
                  }))
              ]
              ++ tools;
            LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath runtimeDeps}";
          };
      in {
        _module.args.pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [(import inputs.rust-overlay)];
        };

        formatter = pkgs.alejandra;
        packages.default = self'.packages.rio;
        devShells.default = self'.devShells.msrv;

        apps.default = {
          type = "app";
          program = self'.packages.default;
        };
        packages.rio = pkgs.callPackage mkRio {rust-toolchain = pkgs.rust-bin.stable.latest.minimal;};

        devShells.msrv = mkDevShell (pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml);
        devShells.stable = mkDevShell pkgs.rust-bin.stable.latest.minimal;
        devShells.nightly = mkDevShell (pkgs.rust-bin.selectLatestNightlyWith (toolchain: toolchain.minimal));
      };
    };
}
