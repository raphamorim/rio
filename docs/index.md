---
layout: home
class: home
title: 'Meet Rio'
language: 'en'
---

## About Rio

Rio is a terminal application that's built with Rust, WebGPU, Tokio runtime. It targets to have the best frame per second experience as long you want, but is also configurable to use as minimal from GPU.

The terminal renderer is based on redux state machine, lines that has not updated will not suffer a redraw. Looking for the minimal rendering process in most of the time. Rio is also designed to support WebAssembly runtime so in the future you will be able to define how a tab system will work with a WASM plugin written in your favorite language.

Rio uses WGPU, which is an implementation of WebGPU for use outside of a browser and as backend for firefox's WebGPU implementation. WebGPU allows for more efficient usage of modern GPU's than WebGL. **[More info](https://users.rust-lang.org/t/what-is-webgpu-and-is-it-ready-for-use/62331/8)**

Applications using WPGU run natively on Vulkan, Metal, DirectX 11/12, and OpenGL ES; and browsers via WebAssembly on WebGPU and WebGL2.

It also relies on Rust memory behavior, since Rust is a memory-safe language that employs a compiler to track the ownership of values that can be used once and a borrow checker that manages how data is used without relying on traditional garbage collection techniques. **[More info](https://stanford-cs242.github.io/f18/lectures/05-1-rust-memory-safety.html)**

## Development status?

You can follow me on Twitter ([@raphamorims](https://twitter.com/raphamorims)) or Bluesky ([@mustache.bsky.social](https://bsky.app/profile/mustache.bsky.social)) to follow any update regarding Rio terminal development.
