---
title: 'Features'
language: 'en'
---

## Features

Short introduction of Rio terminal features. Many other features are in development.

- [• Cross Platform](#cross-platform)
- [• Fast](#Fast)
- [• Multi windows architecture](#multi-windows)
- [• Minimal tabs](#minimal-tabs)
- [• Native tabs](#native-tabs)
- [• Adaptive Theme](#adaptive-theme)
- [• Spawn or Fork processes](#spawn-or-fork)
- [• Collapsed tabs, breadcrumb, expanded tabs on top or bottom](#navigation)
- [• Colorize tabs based on programs](#color-automation-for-navigation)
- [• Kitty's keyboard protocol](#kitty-keyboard-protocol)

### Cross-platform

Rio is available for Microsoft Windows, Linux distros, FreeBSD and Apple MacOS.

### Fast

Rio is perceived fast, there's few reasons behind the speed. First reason is that Rio is built in Rust ("Speed of Rust vs C" [kornel.ski/rust-c-speed](https://kornel.ski/rust-c-speed)). The terminal is also built over ANSI handler and parser is built from Alacritty terminal's VTE [github.com/alacritty/vte](https://github.com/alacritty/vte/).

The renderer called Sugarloaf has a "sugar" architecture created for minimal and quick interactions in render steps using WebGPU with performance at highest.

<img src="https://miro.medium.com/v2/resize:fit:1400/1*1enyoIVZivAcHY_kfYXUvQ.gif" width="100%" />

### Multi windows

The terminal supports multi window features in the following platforms: Windows, MacOS, FreeBSD and Linux.

<img src="https://miro.medium.com/v2/resize:fit:2914/format:webp/1*KyVD4EJ-wQU8pTmOFTwaQg.png" width="100%" />

### Minimal tabs

Most of the times you don't want to be spammed by on-going processes that are happening in other tabs and if you are actively following multi processes then you can use tools like tmux to keep minimal and easy to the eyes.

<img src="https://miro.medium.com/v2/resize:fit:1400/format:webp/1*gMLWcZkniSHUT6Cb7L06Gg.png" width="100%" />

In the future new functionalities will be added to the Rio minimal tabs, to make even easier to navigate or gather information quickly.

### Native tabs

<img alt="Demo native tabs" src="/rio/assets/posts/0.0.17/demo-native-tabs.png" width="70%"/>

Besides other navigation styles, Rio terminal does support native tabs as well.

To enable native tabs:

{% highlight toml %}
[navigation]
mode = "NativeTab"
{% endhighlight %}

### Adaptive theme

Rio support theme based on the system theme (light and dark). This configuration only works for Web, MacOS and Windows.

{% highlight toml %}
[adaptive-theme]
light = "belafonte-day"
dark = "belafonte-night"
{% endhighlight %}

Example of usage with MacOS:

![Adaptive theme](../../static/assets/features/adaptive-theme.gif)

### Spawn or Fork

In POSIX-based systems, Rio spawn processes instead of fork processes due to some compability issues between platforms.

However you can also switch from spawn to fork, forking a process is faster than spawning a process.

See how to configure it in the advanced section [here](/rio/docs).

### Navigation

Rio support 5 types of navigation modes:

<p>
<img alt="Demo Breadcrumb" src="/rio/assets/features/demo-breadcrumb.png" width="48%"/>
<img alt="Demo TopTab" src="/rio/assets/features/demo-top-tab.png" width="48%"/>
</p>

<p>
<img alt="Demo CollapsedTab" src="https://miro.medium.com/v2/resize:fit:1400/format:webp/1*gMLWcZkniSHUT6Cb7L06Gg.png" width="48%" />
<img alt="Demo BottomTab" src="/rio/assets/features/demo-bottom-tab.png" width="48%"/>
</p>

And native tabs for MacOS.

See more about it [here](/rio/docs/navigation).

### Color automation for navigation

Rio allows to specify color overwrites for tabs based on program context.

The example below sets <span class="keyword">#FFFF00</span> as color background whenever <span class="keyword">nvim</span> is running.

<p>
<img alt="example navigation with color automation" src="/rio/assets/features/demo-colorized-navigation.png" width="48%"/>

<img alt="second example navigation with color automation" src="/rio/assets/features/demo-colorized-navigation-2.png" width="48%"/>
</p>

The configuration would be like:

{% highlight toml %}
[navigation]
color-automation = [
	{ program = "nvim", color = "#FFFF00" }
]
{% endhighlight %}

### Kitty keyboard protocol

Rio terminal implements Kitty keyboard protocol.

There are various problems with the current state of keyboard handling in terminals. They include:

- • No way to use modifiers other than ctrl and alt

- • No way to reliably use multiple modifier keys, other than, shift+alt and ctrl+alt.

- • Many of the existing escape codes used to encode these events are ambiguous with different key presses mapping to the same escape code.

- • No way to handle different types of keyboard events, such as press, release or repeat

- • No reliable way to distinguish single Esc key presses from the start of a escape sequence. Currently, client programs use fragile timing related hacks for this, leading to bugs, for example: [neovim #2035](https://github.com/neovim/neovim/issues/2035).

To solve these issues and others, kitty has created a new keyboard protocol, that is backward compatible but allows applications to opt-in to support more advanced usages.
