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
- Flathub: [flathub.org/apps/com.rioterm.Rio](https://flathub.org/apps/com.rioterm.Rio)
- Void Linux package: https://github.com/void-linux/void-packages/tree/master/srcpkgs/rio
- Terra (Fedora) package: https://github.com/terrapkg/packages/tree/frawhide/anda/devs/rio

Note: Debian packages (`.deb`) and Red Hat packages (`.rpm`) are packaged and released along with [GitHub releases](https://github.com/raphamorim/rio/releases).

In case your distro doesn't have the package manager option listed above, proceed to [build from the source](/docs/install/build-from-source).

## NixOS Flake Installation

Note: If you are unsure if you should use the package from nixpkgs or the flakes package, always go for the nixpkgs derivation as the flakes package output is for development purposes only.

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

## Terminfo

To ensure Rio works correctly, the "rio" terminfo must be installed. The installation method depends on your distribution and package type:

### Debian/Ubuntu (Using .deb packages)

**Note:** Rio's .deb packages do not include terminfo files to avoid conflicts with system packages.

#### Debian 13+ / Ubuntu 24.04+

These distributions include Rio's terminfo in the `ncurses-term` package (version 6.5+). No additional steps needed:

```bash
# Verify terminfo is installed
infocmp rio
```

#### Ubuntu 22.04 and older Debian/Ubuntu versions

You'll need to install the terminfo manually after installing Rio:

```bash
# Check if terminfo is already installed
infocmp rio 2>/dev/null || {
  # If not found, install it manually
  curl -o rio.terminfo https://raw.githubusercontent.com/raphamorim/rio/main/misc/rio.terminfo
  sudo tic -xe xterm-rio,rio rio.terminfo
  rm rio.terminfo
}
```

### RPM-based distributions (Fedora, RHEL, openSUSE)

Rio's RPM packages include the terminfo files. No additional steps needed.

### Other distributions

For distributions using other package formats or building from source, install the terminfo manually:

```bash
curl -o rio.terminfo https://raw.githubusercontent.com/raphamorim/rio/main/misc/rio.terminfo
sudo tic -xe xterm-rio,rio rio.terminfo
rm rio.terminfo
```

For more details, see the [Terminfo documentation](/docs/install/terminfo).

## Audio Bell Support

On Linux and BSD systems, Rio can optionally play an audio bell sound (a 440Hz tone) when the terminal bell is triggered. This feature requires the `audio` feature flag to be enabled during compilation.

Most distribution packages do not include audio support by default to minimize dependencies. If you need audio bell support, you can:

1. Build from source with the `audio` feature enabled (see [Build from source](/docs/install/build-from-source))
2. Use the system's visual bell instead (enabled via configuration)
3. Configure your shell to handle the bell differently

Note: On macOS and Windows, the system notification sound is always used for the bell, regardless of compilation flags.

## Manual Pages

After installing Rio, you can optionally install manual pages for offline documentation:

```bash
# Install scdoc (required to build man pages)
# Ubuntu/Debian:
sudo apt install scdoc

# Arch Linux:
sudo pacman -S scdoc

# Fedora:
sudo dnf install scdoc

# openSUSE:
sudo zypper install scdoc

# Build and install man pages from source
git clone https://github.com/raphamorim/rio.git
cd rio/extra/man
make
sudo make install

# Access documentation
man rio                # Main Rio manual
man 5 rio             # Configuration file format
man 5 rio-bindings    # Key bindings reference
```
