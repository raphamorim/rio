---
title: 'Linux'
language: 'en'
---

Installation options:

- Alpine Linux package: [pkgs.alpinelinux.org/packages?name=rio](https://pkgs.alpinelinux.org/packages?name=rio)
- Arch Linux package: [archlinux.org/packages/extra/x86_64/rio](https://archlinux.org/packages/extra/x86_64/rio) (or [rio-git](https://aur.archlinux.org/packages/rio-git) from AUR)
- Nix package: [NixOS/nixpkgs/rio](https://github.com/NixOS/nixpkgs/blob/nixos-unstable/pkgs/by-name/ri/rio/package.nix)
- Nix flake: [NixOS Flake Installation](#nixos-flake-installation)
- openSUSE package: [openSUSE:Factory/rioterm](https://software.opensuse.org/package/rioterm)
- Void Linux package: https://github.com/void-linux/void-packages/tree/master/srcpkgs/rio

Note: Debian packages (`.deb`) and Red Hat packages (`.rpm`) are packaged and released along with [GitHub releases](https://github.com/raphamorim/rio/releases).

In case your distro doesn't have the package manager option listed above, proceed to [build from the source](/docs/install/build-from-source).

## NixOS Flake Installation

### For NixOS

To integrate Rio into your NixOS system, add the following to your NixOS configuration:

```nix
{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    rio.url = "github:raphamorim/rio/main";
  };
  
  outputs = { self, nixpkgs, rio }: {
    nixosConfigurations.your-hostname = nixpkgs.lib.nixosSystem {
      modules = [
        ({ pkgs, ... }: {
          environment.systemPackages = [
            rio.packages.${pkgs.system}.rio
          ];
        })
      ];
    };
  };
}
```

### For Home-Manager

To configure Rio using Home-Manager, add the following to your home-manager configuration:

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rio.url = "github:raphamorim/rio/main";
  };

  outputs = { self, nixpkgs, rio }: {
    homeConfigurations.your-username = nixpkgs.lib.homeManagerConfiguration {
      pkgs = import nixpkgs;
      modules = [
        ({ pkgs, ... }: {
          programs.rio.enable = true;
          package = rio.packages.${pkgs.system}.rio;
        })
      ];
    };
  };
}
```

### Single User Installation

For a less declarative installation:

```bash
nix profile install github:raphamorim/rio/main
```
