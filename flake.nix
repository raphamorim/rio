{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    systems.url = "github:nix-systems/default";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = inputs@{ self, nixpkgs, systems, rust-overlay, ... }:
    let
      # Using nix-systems to identify architecture
      eachSystem = nixpkgs.lib.genAttrs (import systems);
      pkgsFor = eachSystem (system:
        import nixpkgs {
          localSystem = system;
          overlays = [
            rust-overlay.overlays.default
          ];
        });

      mkRio = ({ rustPlatform, lib, pkgs, ... }:
        let
          rlinkLibs = with pkgs; if stdenv.isDarwin then [
            darwin.libobjc
            darwin.apple_sdk_11_0.frameworks.AppKit
            darwin.apple_sdk_11_0.frameworks.AVFoundation
            darwin.apple_sdk_11_0.frameworks.Vision
          ] else [
            fontconfig
            libGL
            libxkbcommon
            vulkan-loader
          ] ++ (with pkgs; [
            xorg.libX11
            xorg.libXcursor
            xorg.libXi
            xorg.libXrandr
            xorg.libxcb
            wayland
          ]);
        in
        rustPlatform.buildRustPackage {
          pname = "rio";
          name = "rio"; # attribute name for packages
          src = ./.;
          nativeBuildInputs = with pkgs; [
            ncurses
            cmake
            pkg-config
            autoPatchelfHook
          ];

          runtimeDependencies = rlinkLibs;
          buildInputs = rlinkLibs;
          cargoLock.lockFile = ./Cargo.lock;

          meta = {
            description = "A hardware-accelerated GPU terminal emulator powered by WebGPU";
            homepage = "https://raphamorim.io/rio";
            license = lib.licenses.mit;
            platforms = lib.platforms.unix;
            changelog = "https://github.com/raphamorim/rio/blob/master/CHANGELOG.md";
            mainProgram = "rio";
          };
        });
    in
    {
      overlays.default = final: prev: {
        rio = prev.callPackage mkRio { };
      };

      # `nix build` works
      packages = eachSystem (system: {
        default = pkgsFor.${system}.callPackage mkRio { };
      });

      # `nix develop` works
      devShells = eachSystem (system:
        let
          rust-toolchain = (pkgsFor.${system}.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml).override {
            extensions = [ "rust-src" "rust-analyzer" ];
          };
        in
        {
          default = pkgsFor.${system}.mkShell {
            packages = with pkgsFor.${system}; if stdenv.isDarwin then [
              darwin.libobjc
              darwin.apple_sdk_11_0.frameworks.AppKit
              darwin.apple_sdk_11_0.frameworks.AVFoundation
              darwin.apple_sdk_11_0.frameworks.Vision
            ] else [
              fontconfig
              libGL
              libxkbcommon
              vulkan-loader
              ncurses
              cmake
              pkg-config
              autoPatchelfHook
              rust-toolchain
              xorg.libX11
              xorg.libXcursor
              xorg.libXi
              xorg.libXrandr
              xorg.libxcb
              wayland
            ];
          };
        }
      );
    };
}
