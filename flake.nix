{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    systems.url = "github:nix-systems/default-linux";
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

      devShellls = eachSystem (system:
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
          default = pkgs.mkShell rec {
            packages = with pkgs;
              [
                rust-toolchain

                pkg-config
                cmake
                fontconfig

                xorg.libX11
                xorg.libXcursor
                xorg.libXrandr
                xorg.libXi
                xorg.libxkbfile
                xorg.xkbutils
                xorg.xkbevd
                xorg.libXScrnSaver
                libxkbcommon

                directx-shader-compiler
                libGL
                vulkan-headers
                vulkan-loader
                vulkan-tools

                wayland
              ] ++ [ rust-toolchain ];

            LD_LIBRARY_PATH = "$LD_LIBRARY_PATH:${builtins.toString (pkgs.lib.makeLibraryPath packages) }";
          };
        });
    };
}
