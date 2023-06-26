---
layout: docs
class: docs
title: 'Documentation'
language: 'en'
---

## Summary

- [1. About Rio](#about-rio)
- [2. Configuration file](#configuration-file)
- [3. Default colors](#default-colors)

## About Rio

Rio is a terminal application that's built with Rust, WebGPU, Tokio runtime. It targets to have the best frame per second experience as long you want, but is also configurable to use as minimal from GPU.

The terminal renderer is based on redux state machine, lines that has not updated will not suffer a redraw. Looking for the minimal rendering process in most of the time. Rio is also designed to support WebAssembly runtime so in the future you will be able to define how a tab system will work with a WASM plugin written in your favorite language.

Rio uses WGPU, which is an implementation of WebGPU for use outside of a browser and as backend for firefox's WebGPU implementation. WebGPU allows for more efficient usage of modern GPU's than WebGL. **[More info](https://users.rust-lang.org/t/what-is-webgpu-and-is-it-ready-for-use/62331/8)**

It also relies on Rust memory behavior, since Rust is a memory-safe language that employs a compiler to track the ownership of values that can be used once and a borrow checker that manages how data is used without relying on traditional garbage collection techniques. **[More info](https://stanford-cs242.github.io/f18/lectures/05-1-rust-memory-safety.html)**

## Configuration File

The configuration should be the following paths otherwise Rio will use the default configuration.

MacOS and Linux configuration file path is "~/.config/rio/config.toml".

Windows	configuration file path is "C:\Users\USER\AppData\Local\rio\config.toml" (replace "USER" with your user name).

{% highlight toml %}
font = "CascadiaMono"
font-size = 16

# Default cursor is Block
# Other available options are: '_' and '|'
cursor = '_'

# Set WGPU rendering performance
# High: Adapter that has the highest performance. This is often a discrete GPU.
# Low: Adapter that uses the least possible power. This is often an integrated GPU.
performance = "High"

# it will look for dracula.toml in themes folder
# (macos and linux: ~/.config/rio/themes/dracula.toml)
# ...
# dracula theme code is available in:
# https://github.com/raphamorim/rio-dracula/blob/master/dracula.toml
theme = "dracula"

# define x axis padding (default is 10)
padding-x = 0

# environment variables
# (the example below sets fish as the default SHELL in macos
# please do not copy this if you do not need)
env-vars = ['SHELL=/opt/homebrew/bin/fish']

# This config only works on MacOs.
# Possible choices: 'both', 'left' and 'right'.
option_as_alt = 'both'

# window-opacity changes the window transparency state.
# Only works for Windows / X11 / WebAssembly
window-opacity = 0.5

# Colors definition will overwrite any property in theme
# (considering if theme folder does exists and is being used)
[colors]
background = "#BBBD64"
foreground = "#040400"
cursor = "#242805"
tabs-active = "#F8A145"
blue = "#454A12"

[developer]
log-level = "INFO"
{% endhighlight %}

Any file update in the configuration file will trigger a render operation in Rio terminal with the new configuration.

If you have any suggestion of configuration ideas to Rio, please feel free to [open an issue](https://github.com/raphamorim/rio/issues/new).

## Default colors

Default Rio terminal colors.

{% highlight toml %}
[colors]
background       = '#0F0D0E'
black            = '#4C4345'
blue             = '#006EE6'
cursor           = '#F38BA3'
cyan             = '#88DAF2'
foreground       = '#F9F4DA'
green            = '#0BA95B'
magenta          = '#7B5EA7'
red              = '#ED203D'
tabs             = '#12B5E5'
tabs-active      = '#FCBA28'
white            = '#F1F1F1'
yellow           = '#FCBA28'
dim-black        = '#1C191A'
dim-blue         = '#0E91B7'
dim-cyan         = '#93D4E7'
dim-foreground   = '#ECDC8A'
dim-green        = '#098749'
dim-magenta      = '#624A87'
dim-red          = '#C7102A'
dim-white        = '#C1C1C1'
dim-yellow       = '#E6A003'
light-black      = '#ADA8A0'
light-blue       = '#44C9F0'
light-cyan       = '#7BE1FF'
light-foreground = '#F2EFE2'
light-green      = '#0ED372'
light-magenta    = '#9E88BE'
light-red        = '#F25E73'
light-white      = '#FFFFFF'
light-yellow     = '#FDF170'
{% endhighlight %}

<!-- 
## disable-renderer-when-unfocused

This property disable renderer processes until focus on Rio term again.

{% highlight toml %}
[advanced]
disable-renderer-when-unfocused = false
{% endhighlight %}

## log-level

This property enables log level filter. Default is "OFF".

{% highlight toml %}
[developer]
log-level = 'INFO'
{% endhighlight %}

## enable-fps-counter

This property enables frame per second counter.

{% highlight toml %}
[developer]
enable-fps-counter = false
{% endhighlight %} -->