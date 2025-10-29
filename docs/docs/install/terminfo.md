---
title: 'Terminfo'
language: 'en'
---

To make sure Rio works correctly, the Rio terminfo must be used. The rio terminfo will be picked up automatically if it is installed.

## Installation Status

Check if the terminfo is already installed:

```sh
infocmp rio
```

If this command returns without errors, the terminfo is installed and Rio will work correctly.

## Installation Methods

### Package-specific Information

#### Debian/Ubuntu (.deb packages)

**Important:** Rio's .deb packages do not include terminfo files to avoid conflicts with system packages.

- **Debian 13+ / Ubuntu 24.04+**: Terminfo is provided by the system's `ncurses-term` package (6.5+)
- **Ubuntu 22.04 and older**: Manual installation required (see below)

#### RPM packages (Fedora, RHEL, openSUSE)

Rio's RPM packages include the terminfo files. No additional installation needed.

### System Package Manager

#### Debian 13+ / Ubuntu 24.04+

These distributions include Rio's terminfo in the `ncurses-term` package:

```sh
# Ensure ncurses-term is installed
sudo apt update
sudo apt install ncurses-term
```

#### Arch Linux

Arch Linux provides a separate package:

```sh
sudo pacman -S rio-terminfo
```

### Manual Installation

For distributions that don't include Rio's terminfo, or when using older package versions:

#### Quick Install

```sh
# Download and install terminfo
curl -o rio.terminfo https://raw.githubusercontent.com/raphamorim/rio/main/misc/rio.terminfo
sudo tic -xe xterm-rio,rio rio.terminfo
rm rio.terminfo
```

#### From Repository

If you've cloned the Rio repository:

```sh
cd /path/to/rio
sudo tic -xe xterm-rio,rio misc/rio.terminfo
```

## Terminfo Entries

The Rio terminfo provides two entries:

- `xterm-rio`: A compatibility entry for applications that look for "xterm-" prefixed terminals (used by default since v0.2.28)
- `rio`: The original terminfo entry (used by default until v0.2.27)

Both entries provide the same terminal capabilities.

## Troubleshooting

### Package Installation Issues

#### "File already exists" error on Debian/Ubuntu

If you see:
```
trying to overwrite '/usr/share/terminfo/r/rio', which is also in package ncurses-term
```

This means your system already provides Rio's terminfo via ncurses-term. The Rio .deb package correctly doesn't include the terminfo to avoid this conflict.

#### Ubuntu 22.04 specific

Ubuntu 22.04 ships with ncurses-term 6.3, which doesn't include Rio's terminfo. After installing Rio's .deb package, manually install the terminfo:

```sh
curl -o rio.terminfo https://raw.githubusercontent.com/raphamorim/rio/main/misc/rio.terminfo
sudo tic -xe xterm-rio,rio rio.terminfo
rm rio.terminfo
```

### SSH Connections

When connecting to remote systems via SSH, the remote system also needs the Rio terminfo. Options:

1. **Install on remote system**: Use the manual installation method above
2. **Use fallback TERM**: `TERM=xterm-256color ssh user@host`
3. **Configure Rio**: Set a different TERM in your Rio config file
4. **Automatic copy**: Some users create scripts to automatically copy terminfo when SSHing:
   ```sh
   infocmp -a xterm-rio | ssh myserver tic -x -o ~/.terminfo /dev/stdin
   ```

### Verification

After installation, verify everything works:

```sh
# Check terminfo is installed
infocmp rio
```
