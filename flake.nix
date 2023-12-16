{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    systems.url = "github:nix-systems/default";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { systems, nixpkgs, rust-overlay, ... }:
    let
      eachSystem = nixpkgs.lib.genAttrs (import systems);

      mkRio = ({ rustPlatform, lib, ... }: rustPlatform.buildRustPackage {
        pname = "rio";

        src = ./.;
        cargoLock.lockFile = ./Cargo.lock;

        meta = {
          description = "A hardware-accelerated GPU terminal emulator focusing to run in desktops and browsers.";
          homepage = "https://raphamorim.io/rio/";
          license = lib.licenses.mit;
        };
      });
    in
    {
      overlays.default = final: prev: {
        rio = prev.callPackage mkRio { };
      };

      devShells = eachSystem
        (system:
          let
            pkgs = import nixpkgs {
              inherit system;
              overlays = [ rust-overlay.overlays.default ];
            };

            rust-toolchain = (pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml).override {
              extensions = [ "rust-src" "rust-analyzer" ];
            };
          in
          {
            default = pkgs.mkShell
              {
                packages = with pkgs; if stdenv.isDarwin then [
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
                ]) ++ [ rust-toolchain ];
              };
            # LD_LIBRARY_PATH = "$LD_LIBRARY_PATH:${builtins.toString (pkgs.lib.makeLibraryPath packages) }";
          });
    };
}
