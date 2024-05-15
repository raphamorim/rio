---
title: 'Rio is Fast'
language: 'en'
---

Rio is perceived fast, there's few reasons behind the speed. First reason is that Rio is built in Rust ("Speed of Rust vs C" [kornel.ski/rust-c-speed](https://kornel.ski/rust-c-speed)). The terminal is also built over ANSI handler and parser is built from Alacritty terminal's VTE [github.com/alacritty/vte](https://github.com/alacritty/vte/).

The renderer called Sugarloaf has a "sugar" architecture created for minimal and quick interactions in render steps using WebGPU with performance at highest.

<img src="https://miro.medium.com/v2/resize:fit:1400/1*1enyoIVZivAcHY_kfYXUvQ.gif" width="100%" />
