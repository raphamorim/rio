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

  outputs = inputs @ { flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [ ];

      systems = import inputs.systems;

      perSystem =
        { config
        , self'
        , inputs'
        , pkgs
        , system
        , lib
        , ...
        }:
        let
          rust-toolchain = (pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml).override {
            extensions = [ "rust-src" "rust-analyzer" ];
          };

          mkRio = import ./pkgRio.nix;

          mkDevShell = rust-toolchain:
            let
              dependencies = self'.packages.rio.nativeBuildInputs ++ self'.packages.rio.buildInputs;
            in
            pkgs.mkShell {
              LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath dependencies}:$LD_LIBRARY_PATH";
              packages = dependencies ++ [ rust-toolchain ];
            };
        in
        {
          _module.args.pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [ (import inputs.rust-overlay) ];
          };

          formatter = pkgs.alejandra;
          packages.default = self'.packages.rio;
          devShells.default = self'.devShells.msrv;

          apps.default = {
            type = "app";
            program = self'.packages.default;
          };
          packages.rio = pkgs.callPackage mkRio { };

          devShells.msrv = mkDevShell rust-toolchain;
          devShells.stable = mkDevShell pkgs.rust-bin.stable.latest.default;
          devShells.nightly = mkDevShell (pkgs.rust-bin.selectLatestNightlyWith (toolchain: toolchain.default));
        };
    };
}
