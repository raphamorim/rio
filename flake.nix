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
          rio = msrv;
          default = rio;
        };
        i3WorkspaceRedrawE2EScripts = pkgs.runCommandLocal "rio-i3-workspace-redraw-e2e-scripts" {} ''
          install -D -m 755 \
            ${./misc/scripts/test-i3-workspace-redraw.sh} \
            $out/test-i3-workspace-redraw.sh
          install -D -m 755 \
            ${./misc/scripts/test-i3-workspace-redraw-headless.sh} \
            $out/test-i3-workspace-redraw-headless.sh
        '';
        i3WorkspaceRedrawE2E = pkgs.writeShellApplication {
          name = "rio-i3-workspace-redraw-e2e";
          runtimeInputs = with pkgs; [
            bash
            coreutils
            gawk
            gnused
            i3
            imagemagick
            jq
            xvfb-run
            xauth
          ];
          text = ''
            shopt -s nullglob
            lavapipe_icds=(${pkgs.mesa}/share/vulkan/icd.d/lvp_icd.*.json)
            if (( ''${#lavapipe_icds[@]} != 1 )); then
              printf 'expected exactly one Lavapipe ICD, found %d\n' \
                "''${#lavapipe_icds[@]}" >&2
              exit 2
            fi
            export VK_DRIVER_FILES="''${lavapipe_icds[0]}"
            exec ${i3WorkspaceRedrawE2EScripts}/test-i3-workspace-redraw-headless.sh "$@"
          '';
        };
      in {
        formatter = pkgs.alejandra;
        _module.args.pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [(import inputs.rust-overlay)];
        };

        # Create overlay to override `rio` with this flake's default
        overlayAttrs = {inherit (self'.packages) rio;};
        packages =
          (lib.mapAttrs' (
              k: v: {
                name =
                  if builtins.elem k ["rio" "default"]
                  then k
                  else "rio-${k}";
                value = pkgs.callPackage ./pkgRio.nix {rust-toolchain = v;};
              }
            )
            toolchains)
          // lib.optionalAttrs pkgs.stdenv.isLinux {
            i3-workspace-redraw-e2e = i3WorkspaceRedrawE2E;
          };
        apps = lib.optionalAttrs pkgs.stdenv.isLinux {
          i3-workspace-redraw-e2e = {
            type = "app";
            program = lib.getExe i3WorkspaceRedrawE2E;
            meta.description = "Run the headless i3 workspace redraw E2E test";
          };
        };
        # Different devshells for different rust versions
        devShells = lib.mapAttrs (_: v: mkDevShell v) toolchains;
      };
    };
}
