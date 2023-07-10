---
layout: features
class: features
title: 'Features'
language: 'en'
---

## Features

Short introduction of Rio terminal features. Many other features are in development.

- [• Cross Platform](#cross-platform)
- [• Fast](#Fast)
- [• Minimal tabs](#minimal-tabs)
- [• Multi windows architecture](#multi-windows)

### Cross-platform

Rio is available for Microsoft Windows, Linux distros, FreeBSD and Apple MacOS.

### Fast

Rio is perceived fast, there's few reasons behind the speed. First reason is that Rio is built in Rust ("Speed of Rust vs C" [kornel.ski/rust-c-speed](https://kornel.ski/rust-c-speed)). The terminal is also built over ANSI handler and parser is built from Alacritty terminal's VTE [github.com/alacritty/vte](https://github.com/alacritty/vte/).

The renderer called Sugarloaf has a "sugar" architecture created for minimal and quick interactions in render steps using WebGPU with performance at highest.

<img src="https://miro.medium.com/v2/resize:fit:1400/1*1enyoIVZivAcHY_kfYXUvQ.gif" width="100%" />

### Minimal tabs

Most of the times you don't want to be spammed by on-going processes that are happening in other tabs and if you are actively following multi processes then you can use tools like tmux to keep minimal and easy to the eyes.

<img src="https://miro.medium.com/v2/resize:fit:1400/format:webp/1*gMLWcZkniSHUT6Cb7L06Gg.png" width="100%" />

In the future new functionalities will be added to the Rio minimal tabs, to make even easier to navigate or gather information quickly.

### Multi windows

The terminal supports multi window features in the following platforms: Windows, MacOS, FreeBSD and Linux.

<img src="https://miro.medium.com/v2/resize:fit:2914/format:webp/1*KyVD4EJ-wQU8pTmOFTwaQg.png" width="100%" />

