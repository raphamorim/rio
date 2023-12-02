{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { nixpkgs, rust-overlay, ... }:
    let
      system = "x86_64-linux";

      pkgs = import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
      };

      rust-toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
    in
    {
      devShells.${system}.default = pkgs.mkShell rec {
        packages = with pkgs; [
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
        ] ++ [ rust-toolchain ];

        LD_LIBRARY_PATH = "$LD_LIBRARY_PATH:${builtins.toString (pkgs.lib.makeLibraryPath packages)}";
      };
    };
}
