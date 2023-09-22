---
title: 'Build from source'
language: 'en'
---

## Build from the source

Before compiling Rio terminal, you'll have to first clone the source code:

```bash
git clone https://github.com/raphamorim/rio.git
```

Then install the Rust compiler with <span class="keyword">rustup</span> ([rustup.rs](https://rustup.rs/)).

After install, make sure you have the right Rust compiler installed by running:

```bash
rustup override set stable
rustup update stable
```

### Dependencies

These are the minimum dependencies required to build Rio terminal, please note that with some setups additional dependencies might be desired.

If you're running Wayland with an Nvidia GPU, you'll likely want the _EGL_ drivers installed too (these are called <span class="keyword"> libegl1-mesa-dev</span> on Ubuntu).

#### Debian/Ubuntu

If you'd like to build a local version manually, you need a few extra libraries to build Rio. Here's an apt command that should install all of them. If something is still found to be missing, please open an issue.

```bash
apt install cmake pkg-config libfreetype6-dev libfontconfig1-dev libxcb-xfixes0-dev libxkbcommon-dev python3
```

#### Arch Linux

On Arch Linux, you need a few extra libraries to build Rio. Here's a <span class="keyword">pacman</span> command that should install all of them. If something is still found to be missing, please open an issue.

```bash
pacman -S cmake freetype2 fontconfig pkg-config make libxcb libxkbcommon python
```

#### Fedora

On Fedora, you need a few extra libraries to build Rio. Here's a <span class="keyword">dnf</span> command that should install all of them. If something is still found to be missing, please open an issue.

```bash
dnf install cmake freetype-devel fontconfig-devel libxcb-devel libxkbcommon-devel g++
```

#### Void Linux

On Void Linux, install following packages before compiling Rio:

```bash
xbps-install cmake freetype-devel expat-devel fontconfig-devel libxcb-devel pkg-config python3
```

#### FreeBSD

On FreeBSD, you need a few extra libraries to build Rio. Here's a <span class="keyword">pkg</span> command that should install all of them. If something is still found to be missing, please open an issue.

```bash
pkg install cmake freetype2 fontconfig pkgconf python3
```

### Building

Linux with X11:

```bash
# Build for X11
cargo build --release --no-default-features --features=x11

# Running it
target/release/rio
```

Linux with Wayland:

```bash
# Build for Wayland
cargo build --release --no-default-features --features=wayland

# Running it
target/release/rio
```

MacOS:

```bash
make release-macos
```

After the command execution an application called "Rio.app" will be created inside of a folder "release" (this folder is generated only after the command execution).

Windows:

```bash
cargo build --release
```

After the command execution an executable will be created called Rio.exe inside of "target/release"

Optionally you can also build and run the terminal with "cargo run".

If all goes well, this should place a zip file with Rio application inside at <span class="keyword">release</span> (folder created in rio root path after the command execution).
