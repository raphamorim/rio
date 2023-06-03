---
layout: docs
class: docs
title: 'Install'
language: 'en'
---

## Install

You can download Rio terminal application for macOS platform, although is not stable and lacking features since the development of a beta release version is still in process.

- [Download macOS - canary-5c027675341a3743348dbfd2c13739a6195a43e3 (unstable)](https://github.com/raphamorim/rio/releases/download/canary-5c027675341a3743348dbfd2c13739a6195a43e3/macos-rio.zip)

New versions are created by weekly and monthly basis, so if you are using an unstable version please make sure to keep updated.

Even before proceed to download please check if the version specified to download is the latest in [https://github.com/raphamorim/rio/releases](https://github.com/raphamorim/rio/releases), because fresh canary versions are often more stable than the previous ones.

For Linux the installation option is build from the source.<br/><br/>

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

### Debian/Ubuntu

If you'd like to build a local version manually, you need a few extra libraries to build Rio. Here's an apt command that should install all of them. If something is still found to be missing, please open an issue.

{% highlight bash %}
apt install cmake pkg-config libfreetype6-dev libfontconfig1-dev libxcb-xfixes0-dev libxkbcommon-dev python3
{% endhighlight %}

### Arch Linux

On Arch Linux, you need a few extra libraries to build Rio. Here's a <span class="keyword">pacman</span> command that should install all of them. If something is still found to be missing, please open an issue.

{% highlight bash %}
pacman -S cmake freetype2 fontconfig pkg-config make libxcb libxkbcommon python
{% endhighlight %}

### Fedora

On Fedora, you need a few extra libraries to build Rio. Here's a <span class="keyword">dnf</span> command that should install all of them. If something is still found to be missing, please open an issue.

{% highlight bash %}
dnf install cmake freetype-devel fontconfig-devel libxcb-devel libxkbcommon-devel g++
{% endhighlight %}

<br/>

## Building

Linux / BSD:

{% highlight bash %}
cargo build --release
{% endhighlight %}

If all goes well, this should place a binary at <span class="keyword">target/release/rio</span>.

macOS:

{% highlight bash %}
make release-macos
{% endhighlight %}

If all goes well, this should place a zip file with Rio application inside at <span class="keyword">release</span> (folder created in rio root path after the command execution).
