---
title: 'Build from source'
language: 'en'
---

## Build from the source

Before compiling Rio terminal, you'll have to first clone the source code:

```sh
git clone --depth=1 https://github.com/raphamorim/rio.git
```

Then install the Rust compiler with `rustup` ([rustup.rs](https://rustup.rs/)).

After installation of Rust, ensure you have the correct Rust compiler installed by running:

```sh
rustup override set stable
rustup update stable
```

### Dependencies

These are the minimum dependencies required to build Rio terminal, please note that with some setups additional dependencies might be desired.

If you're running Wayland with an Nvidia GPU and you are planning to use Rio with `OpenGL` as primary renderer backend, you'll likely want the _EGL_ drivers installed too (these are called `libegl1-mesa-dev` on Ubuntu).

#### Debian/Ubuntu

If you'd like to build a local version manually, you need a few extra libraries to build Rio. Here's an apt command that should install all of them. If something is still found to be missing, please open an issue. This has been tested on Debian Bookworm.

```sh
apt install cmake pkg-config libfreetype6-dev libfontconfig1-dev libxcb-xfixes0-dev libxkbcommon-dev python3
```

#### Arch Linux

On Arch Linux, you need a few extra libraries to build Rio. Here's a `pacman` command that should install all of them. If something is still found to be missing, please open an issue.

```sh
pacman -S cmake freetype2 fontconfig pkg-config make libxcb libxkbcommon python
```

#### Fedora

On Fedora, you need a few extra libraries to build Rio. Here's a `dnf` command that should install all of them. If something is still found to be missing, please open an issue.

```sh
dnf install cmake freetype-devel fontconfig-devel libxcb-devel libxkbcommon-devel g++
```

#### Void Linux

On Void Linux, install following packages before compiling Rio:

```sh
xbps-install cmake freetype-devel expat-devel fontconfig-devel libxcb-devel pkg-config python3
```

#### FreeBSD

On FreeBSD, you need a few extra libraries to build Rio. Here's a `pkg` command that should install all of them. If something is still found to be missing, please open an issue.

```sh
pkg install cmake freetype2 fontconfig pkgconf python3
```

### Building

Linux with X11:

```sh
# Build for X11
cargo build -p rioterm --release --no-default-features --features=x11

# Build for X11 with audio bell support
cargo build -p rioterm --release --no-default-features --features=x11,audio

# Running it
target/release/rio
```

Linux with Wayland:

```sh
# Build for Wayland
cargo build -p rioterm --release --no-default-features --features=wayland

# Build for Wayland with audio bell support
cargo build -p rioterm --release --no-default-features --features=wayland,audio

# Running it
target/release/rio
```

Linux with both X11 and Wayland:

```sh
# Build with both display protocols (default)
cargo build -p rioterm --release

# Build with both display protocols and audio bell support
cargo build -p rioterm --release --features=audio

# Running it
target/release/rio
```

#### Audio Feature

The `audio` feature flag enables audio bell support on Linux and BSD systems. When enabled, Rio will play a 440Hz tone when the terminal bell is triggered. This feature:

- Is **optional** and disabled by default to minimize dependencies
- Adds the `cpal` audio library as a dependency
- Is **not needed** on macOS and Windows (they use system notification sounds)
- Can be enabled by adding `audio` to the features list during compilation

If audio support is not compiled in, Rio will log a debug message when the bell is triggered but won't play any sound.

MacOS:

```sh
make release-macos
```

After the command execution an application called "Rio.app" will be created inside of a folder "release" (this folder is generated only after the command execution).

Windows:

```sh
cargo build -p rioterm --release
```

After the command execution an executable will be created called Rio.exe inside of "target/release"

Optionally you can also build and run the terminal with "cargo run".

If all goes well, this should place a zip file with Rio application inside at `release` (folder created in rio root path after the command execution).
