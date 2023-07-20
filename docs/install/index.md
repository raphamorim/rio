---
layout: docs
class: docs
title: 'Install'
language: 'en'
---

## Install

- [1. About stable and canary versions](#stable-and-canary)
- [2. Platforms](#platforms)
	- [2.1 MacOS](#macos)
	- [2.2 Linux](#linux)
	- [2.3 Windows](#windows)
	- [2.4 FreeBSD](#freebsd)
	- [2.5 WebAssembly](#webassembly)
- [3. Build from the source](#build-from-the-source)
	- [3.1 Dependencies](#dependencies)
	- [3.2 Debian/Ubuntu](#debianubuntu)
	- [3.3 Arch Linux](#arch-linux)
	- [3.4 Void Linux](#void-linux)
	- [3.5 FreeBSD](#freebsd-1)
	- [3.6 Fedora](#fedora)
	- [3.7 Building](#building)
- [4. Terminfo](#terminfo)

## Stable and Canary

Rio terminal applications have two type of builds: stable and canary.

While stable versions are thoroughly tested and takes longer to release, canary versions are created by daily and weekly basis.

To check if you are using a stable or canary version, you can check the version: stable follows the semantic versioning so you would often see stable releases similar to <span class="keyword">v0.0.0</span> pattern, while canary appends the "-canary" suffix to the version (eg. <span class="keyword">v0.0.0-canary</span>).

Another difference (only available for MacOS) is the application icon:

#### Stable icon

<img alt="Rio terminal stable icon" src="/rio/assets/rio-stable.png" width="240" />

#### Canary icon

<img alt="Rio terminal canary icon" src="/rio/assets/rio-canary.png" width="240" />

<br/>

## Platforms

Rio is avaible for [MacOS](#macos), [Linux](#linux), [Windows](#windows) and browsers by [WebAssembly](#webassembly).

### MacOS

You can download Rio terminal application for macOS platform:

- [Download macOS - v0.0.12](https://github.com/raphamorim/rio/releases/download/v0.0.12/Rio-v0.0.12.dmg)

Alternatively you can install Rio through [Homebrew](https://brew.sh/)

{% highlight bash %}
brew install --cask rio
{% endhighlight %}

Remember to run a "brew update" in case Homebrew cannot find a rio cask to install. 

Canary versions for MacOS are not notarized, so if you want to install a canary version you need to download and install the canary app from [github.com/raphamorim/rio/releases](https://github.com/raphamorim/rio/releases) and then follow the steps below:

- • Try to run, it will show a window explaining it cannot be opened because "Apple cannot check it for malicious software.", then click Ok.
- • Open System Preferences and select "Security & Privacy".
- • If the padlock in the bottom left is locked, click it and authenticate to unlock it.
- • Next to the message explaining the app "was blocked from use because it is not from an identified developer," click "Open Anyway".
- • Close System Preferences and run the app.
- • A notice will reiterate the warning about an inability to check if it is malicious, click Open.

### Linux

Installation options:

- • Arch Linux package: [aur.archlinux.org/packages/rio](https://aur.archlinux.org/packages/rio)
- • Nix package: [NixOS/nixpkgs/rio](https://github.com/NixOS/nixpkgs/blob/nixos-unstable/pkgs/applications/terminal-emulators/rio/default.nix)

In case your distro doesn't have the package manager option listed above, proceed to [build from the source](#build-from-the-source).

### FreeBSD

Installation options:

- • [FreeBSD Ports](https://ports.freebsd.org/cgi/ports.cgi?query=rio-terminal&stype=all&sektion=all)

### Windows

Note: Rio is only available for Windows 10 or later.

Prebuilt binaries for Windows:

- • [Download Microsoft Windows Installer - v0.0.12](https://github.com/raphamorim/rio/releases/download/v0.0.12/Rio-installer.msi)
- • [Download Rio Portable - v0.0.12](https://github.com/raphamorim/rio/releases/download/v0.0.12/Rio-portable.exe)

There's a few things to note about the installer and the portable version:

- • The browser will ask if you want to keep the file, click "Keep" to save the installer/executable on your computer.
- • When opening the file, Windows will give you a warning, click "More info" and then "Run anyway" to run the installer/executable.

If you want to change the default shell to the new PowerShell platform, change the following line in your config file (see [Docs](https://raphamorim.io/rio/docs/) for more information):

{% highlight bash %}
shell = { program = "pwsh", args = ["--login"] }
{% endhighlight %}

You may want to use a specific GPU on your system, specially if you're on a laptop configuration, this can enable hardware accelaration and improve performance of the application.
To make Windows utilize a GPU for a specific application through Windows display settings, follow the instructions:

- • 1. Simultaneously press the Windows key and the letter "i" on your keyboard to open Windows Settings.
- • 2. Select System.
- • 3. Choose the Display option.
- • 4. Click on the Graphics setting link located at the bottom of the page.
- • 5. Select the application from the list or press the Browse button, then select the executable file for the application.
- • 6. Click on the Options button to display the GPU selection window.
- • 7. Choose the GPU you want to prioritize for the selected application.
- • 8. Click on the Save button.

### WebAssembly

TBD in the version v0.0.12.

<br/>

## Build from the source

Before compiling Rio terminal, you'll have to first clone the source code:

{% highlight bash %}
git clone https://github.com/raphamorim/rio.git
{% endhighlight %}

Then install the Rust compiler with <span class="keyword">rustup</span> ([rustup.rs](https://rustup.rs/)).

After install, make sure you have the right Rust compiler installed by running:

{% highlight bash %}
rustup override set stable
rustup update stable
{% endhighlight %}

### Dependencies

These are the minimum dependencies required to build Rio terminal, please note that with some setups additional dependencies might be desired.

If you're running Wayland with an Nvidia GPU, you'll likely want the _EGL_ drivers installed too (these are called <span class="keyword"> libegl1-mesa-dev</span> on Ubuntu).

#### Debian/Ubuntu

If you'd like to build a local version manually, you need a few extra libraries to build Rio. Here's an apt command that should install all of them. If something is still found to be missing, please open an issue.

{% highlight bash %}
apt install cmake pkg-config libfreetype6-dev libfontconfig1-dev libxcb-xfixes0-dev libxkbcommon-dev python3
{% endhighlight %}

#### Arch Linux

On Arch Linux, you need a few extra libraries to build Rio. Here's a <span class="keyword">pacman</span> command that should install all of them. If something is still found to be missing, please open an issue.

{% highlight bash %}
pacman -S cmake freetype2 fontconfig pkg-config make libxcb libxkbcommon python
{% endhighlight %}

#### Fedora

On Fedora, you need a few extra libraries to build Rio. Here's a <span class="keyword">dnf</span> command that should install all of them. If something is still found to be missing, please open an issue.

{% highlight bash %}
dnf install cmake freetype-devel fontconfig-devel libxcb-devel libxkbcommon-devel g++
{% endhighlight %}

#### Void Linux

On Void Linux, install following packages before compiling Rio:

{% highlight bash %}
xbps-install cmake freetype-devel expat-devel fontconfig-devel libxcb-devel pkg-config python3
{% endhighlight %}

#### FreeBSD

On FreeBSD, you need a few extra libraries to build Rio. Here's a <span class="keyword">pkg</span> command that should install all of them. If something is still found to be missing, please open an issue.

{% highlight bash %}
pkg install cmake freetype2 fontconfig pkgconf python3
{% endhighlight %}

### Building

Linux with X11:

{% highlight bash %}
# Build for X11
cargo build --release --no-default-features --features=x11

# Running it
target/release/rio
{% endhighlight %}

Linux with Wayland:

{% highlight bash %}
# Build for Wayland
cargo build --release --no-default-features --features=wayland

# Running it
target/release/rio
{% endhighlight %}

MacOS:

{% highlight bash %}
make release-macos
{% endhighlight %}

After the command execution an application called "Rio.app" will be created inside of a folder "release" (this folder is generated only after the command execution).

Windows:

{% highlight bash %}
cargo build --release
{% endhighlight %}

After the command execution an executable will be created called Rio.exe inside of "target/release"

Optionally you can also build and run the terminal with "cargo run".

If all goes well, this should place a zip file with Rio application inside at <span class="keyword">release</span> (folder created in rio root path after the command execution).

## Terminfo

To make sure Rio works correctly, the "rio" terminfo must be used. The rio terminfo will be picked up automatically if it is installed.

If the following command returns without any errors, the rio terminfo is already installed:

{% highlight bash %}
infocmp rio
{% endhighlight %}

If it is not present already, you can install it globally with the following command:

{% highlight bash %}
sudo tic -xe rio misc/rio.terminfo
{% endhighlight %}
