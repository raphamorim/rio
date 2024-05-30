{
  rust-toolchain,
  makeRustPlatform,
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
  cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
  rustPlatform = makeRustPlatform {
    cargo = rust-toolchain;
    rustc = rust-toolchain;
  };
  rlinkLibs =
    if stdenv.isDarwin
    then [
      darwin.libobjc
      darwin.apple_sdk_11_0.frameworks.AppKit
      darwin.apple_sdk_11_0.frameworks.AVFoundation
      darwin.apple_sdk_11_0.frameworks.MetalKit
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
    cargoLock = {
      lockFile = ./Cargo.lock;

      outputHashes = {
        "dpi-0.1.1" = "sha256-LoA66thPDtA9Q6QkSkQU1M2ekYM3kN1qFnGEJFojFPs=";
      };
    };

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
  }
