---
title: 'Terminfo'
language: 'en'
---

To make sure Rio works correctly, the Rio terminfo must be used. The rio terminfo will be picked up automatically if it is installed.

## Installation Status

If the following command returns without any errors, the xterm-rio terminfo is already installed:

```sh
infocmp rio
```

## Installation Methods

### System Package Manager (Recommended)

#### Debian 13+ / Ubuntu 24.04+

These distributions include Rio's terminfo in the `ncurses-term` package (version 6.5+). No manual installation is needed:

```sh
# Verify ncurses-term is up to date
sudo apt update
sudo apt install ncurses-term
```

#### Arch Linux

Arch Linux provides a separate package for Rio's terminfo:

```sh
sudo pacman -S rio-terminfo
```

### Manual Installation

For distributions that don't include Rio's terminfo, you can install it manually:

#### From Repository

When cloned locally, from the root of the repository run:

```sh
sudo tic -xe xterm-rio,rio misc/rio.terminfo
```

#### Without Cloning

If the source code has not been cloned locally:

```sh
curl -o rio.terminfo https://raw.githubusercontent.com/raphamorim/rio/main/misc/rio.terminfo
sudo tic -xe xterm-rio,rio rio.terminfo
rm rio.terminfo
```

## Terminfo Entries

The Rio terminfo provides two entries:

- `xterm-rio`: A compatibility entry for applications that look for "xterm-" prefixed terminals to detect capabilities (used by default since v0.2.28)
- `rio`: The original terminfo entry (used by default until v0.2.27)

Both entries provide the same terminal capabilities, ensuring compatibility with a wide range of applications.

## Troubleshooting

### File Conflicts on Debian/Ubuntu

If you encounter an error like:

```
trying to overwrite '/usr/share/terminfo/r/rio', which is also in package ncurses-term
```

This means your system already has Rio's terminfo installed via ncurses-term. This is expected for Debian 13+ and Ubuntu 24.04+. The Rio package will use the system-provided terminfo automatically.

### SSH Connections

When connecting to remote systems via SSH, the remote system also needs the Rio terminfo. You have several options:

1. Install the terminfo on the remote system using the methods above
2. Use a fallback TERM value: `TERM=xterm-256color ssh user@host`
3. Configure Rio to use a different default TERM in your config file
