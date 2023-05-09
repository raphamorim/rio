---
layout: docs
class: docs
title: 'Documentation'
language: 'en'
---

## Summary

- [The Rio terminal](#about-rio)
- [The configuration file](#configuration-file)

## About Rio

Rio is a terminal application that's built with Rust, WebGPU, Tokio runtime. It targets to have the best frame per second experience as long you want, but is also configurable to use as minimal from GPU.

The terminal renderer is based on redux state machine, lines that has not updated will not suffer a redraw. Looking for the minimal rendering process in most of the time. Rio is also designed to support WebAssembly runtime so in the future you will be able to define how a tab system will work with a WASM plugin written in your favorite language.

Rio uses WGPU, which is an implementation of WebGPU for use outside of a browser and as backend for firefox's WebGPU implementation. WebGPU allows for more efficient usage of modern GPU's than WebGL. **[More info](https://users.rust-lang.org/t/what-is-webgpu-and-is-it-ready-for-use/62331/8)**

It also relies on Rust memory behavior, since Rust is a memory-safe language that employs a compiler to track the ownership of values that can be used once and a borrow checker that manages how data is used without relying on traditional garbage collection techniques. **[More info](https://stanford-cs242.github.io/f18/lectures/05-1-rust-memory-safety.html)**

## Configuration File

The configuration should be the following paths otherwise Rio will use the default configuration.

In macOS path is in "~/.rio/config.toml".

Default configuration of config.toml:

{% highlight toml %}
performance = "High"
height = 438
width = 662

[style]
font = "Firamono"
font-size = 16
theme = "Basic"

[advanced]
tab-character-active = '●'
tab-character-inactive = '■'
disable-renderer-when-unfocused = false

[developer]
enable-fps-counter = false
log-level = 'OFF'

[colors]
background       = '#0F0D0E'
black            = '#231F20'
blue             = '#006EE6'
cursor           = '#F38BA3'
cyan             = '#88DAF2'
foreground       = '#F9F4DA'
green            = '#0BA95B'
magenta          = '#7B5EA7'
red              = '#ED203D'
tabs             = '#F9C5D1'
tabs-active      = '#FC7428'
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
light-black      = '#2C2728'
light-blue       = '#44C9F0'
light-cyan       = '#7BE1FF'
light-foreground = '#F2EFE2'
light-green      = '#0ED372'
light-magenta    = '#9E88BE'
light-red        = '#F25E73'
light-white      = '#FFFFFF'
light-yellow     = '#FDF170'
{% endhighlight %}

## performance

Set terminal WGPU rendering perfomance.

• **High**: Adapter that has the highest performance. This is often a discrete GPU.

• **Low**: Adapter that uses the least possible power. This is often an integrated GPU.

See more in https://docs.rs/wgpu/latest/wgpu/enum.PowerPreference.html

{% highlight toml %}
# <performance> Set WGPU rendering perfomance
# default: High
# options: High, Low
# High: Adapter that has the highest performance. This is often a discrete GPU.
# Low: Adapter that uses the least possible power. This is often an integrated GPU.
performance = "High"
{% endhighlight %}

## height

Set terminal window height.

{% highlight toml %}
# <height> Set default height
# default: 438
height = 400
{% endhighlight %}

## width

Set terminal window width.

{% highlight toml %}
# <width> Set default width
# default: 662
width = 800
{% endhighlight %}

## cursor

Set cursor character. Default cursor is block ('█').

{% highlight toml %}
# (Underline)
cursor = '_'
# (Beam)
cursor = '|'
{% endhighlight %}

## font

Default font is Firamono.

{% highlight toml %}
[style]
font = "Monaco"
{% endhighlight %}

## font-size

Sets font size.

{% highlight toml %}
[style]
font-size = 16.0
{% endhighlight %}

## tab-character-active

This property sets a character for an active tab.

{% highlight toml %}
[style]
tab-character-active = '●'
{% endhighlight %}

## tab-character-inactive

This property sets a character for an inactive tab.

{% highlight toml %}
[style]
tab-character-inactive = '■'
{% endhighlight %}

## disable-renderer-when-unfocused

This property disable renderer processes until focus on Rio term again.

{% highlight toml %}
[style]
disable-renderer-when-unfocused = false
{% endhighlight %}

## log-level

This property enables log level filter. Default is "OFF".

{% highlight toml %}
[style]
log-level = 'INFO'
{% endhighlight %}

## enable-fps-counter

This property enables frame per second counter.

{% highlight toml %}
[style]
enable-fps-counter = false
{% endhighlight %}