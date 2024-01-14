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
        cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
        rust-toolchain = (pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml).override {
          extensions = ["rust-src" "rust-analyzer"];
        };

        mkRio = {
          rustPlatform,
          stdenv,
          lib,
          fontconfig,
          darwin,
          gcc-unwrapped,
          libGL,
          libxkbcommon,
          vulkan-loader,
          libX11,
          libXcursor,
          libXi,
          libXrandr,
          libxcb,
          wayland,
          ncurses,
          pkg-config,
          cmake,
          autoPatchelfHook,
          withX11 ? !stdenv.isDarwin,
          withWayland ? !stdenv.isDarwin,
          ...
        }: let
          rlinkLibs =
            if stdenv.isDarwin
            then [
              darwin.libobjc
              darwin.apple_sdk_11_0.frameworks.AppKit
              darwin.apple_sdk_11_0.frameworks.AVFoundation
              darwin.apple_sdk_11_0.frameworks.Vision
            ]
            else
              [
                (lib.getLib gcc-unwrapped)
                fontconfig
                libGL
                libxkbcommon
                vulkan-loader
              ]
              ++ lib.optionals withX11 [
                libX11
                libXcursor
                libXi
                libXrandr
                libxcb
              ]
              ++ lib.optionals withWayland [
                wayland
              ];
        in
          rustPlatform.buildRustPackage {
            inherit (cargoToml.workspace.package) version;
            name = "rio";
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;

            cargoBuildFlags = "-p rioterm";

            buildInputs = rlinkLibs;
            runtimeDependencies = rlinkLibs;

            nativeBuildInputs =
              [
                ncurses
              ]
              ++ lib.optionals stdenv.isLinux [
                pkg-config
                cmake
                autoPatchelfHook
              ];

            buildNoDefaultFeatures = true;
            buildFeatures = ["x11" "wayland"];
            meta = {
              description = "A hardware-accelerated GPU terminal emulator focusing to run in desktops and browsers";
              homepage = "https://raphamorim.io/rio";
              license = lib.licenses.mit;
              platforms = lib.platforms.unix;
              changelog = "https://github.com/raphamorim/rio/blob/master/CHANGELOG.md";
              mainProgram = "rio";
            };
          };

        mkDevShell = rust-toolchain: let
          dependencies = self'.packages.rio.nativeBuildInputs ++ self'.packages.rio.buildInputs;
        in
          pkgs.mkShell {
            LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath dependencies}:$LD_LIBRARY_PATH";
            packages = dependencies ++ [rust-toolchain];
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
        packages.rio = pkgs.callPackage mkRio {};

        devShells.msrv = mkDevShell rust-toolchain;
        devShells.stable = mkDevShell pkgs.rust-bin.stable.latest.default;
        devShells.nightly = mkDevShell (pkgs.rust-bin.selectLatestNightlyWith (toolchain: toolchain.default));
      };
    };
}
