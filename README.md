# Rio: ⚡ terminal app 

Website: https://raphamorim.io/rio

This project depends of donations, so if you are using or want to help in any way please consider to donate via [Github Sponsors](https://github.com/sponsors/raphamorim).

#### Status

Under development.

Last testing build for macOS (c603bdcffb6c23a137cb491a505dd23e5f6329c5):

![Demo macOS](docs/demo-macos.png)

#### WPGU based

WPGU is an implementation of WebGPU for use outside of a browser and as backend for firefox's WebGPU implementation. WebGPU allows for more efficient usage of modern GPU's than WebGL. [More info](https://users.rust-lang.org/t/what-is-webgpu-and-is-it-ready-for-use/62331/8)

#### Low CPU and memory usage

You want to avoid a browser-based application to reduce memory and CPU consumption. Electron for example, uses Chromium under the hood so your user sees the same on Windows, Linux and macOS but Rio have same compability rendering based on WGPU.

Rio also relies on Rust memory behavior: Rust is a memory-safe language that employs a compiler to track the ownership of values that can be used once and a borrow checker that manages how data is used without relying on traditional garbage collection techniques. [More info](https://stanford-cs242.github.io/f18/lectures/05-1-rust-memory-safety.html)

## Configuration

The configuration should be the following paths otherwise Rio will use the default configuration.

- macOs path: `~/.rio/config.toml`

#### config.toml

```toml
# Rio configuration file

# <perfomance> Set WGPU rendering perfomance
# default: high
# options: high, average, low
perfomance = "high"
```

## TODO

- [x] WGPU rendering
	- [ ] Render font with custom color, size and family
	- [ ] Fix topbar when resize
- [ ] Read and use configuration
- [ ] Keyboard input
- [ ] Screen resizing
- [ ] Allow use set different font-size
- [ ] Themes support
- [ ] Style rendering (italic, bold, underline)
- [ ] Character set

## References

- https://github.com/hecrj/wgpu_glyph
- https://fonts.google.com/specimen/Silkscreen
- https://github.com/alexheretic/glyph-brush/tree/master/glyph-brush
- https://chromestatus.com/feature/6213121689518080
- https://dmnsgn.me/blog/from-glsl-to-wgsl-the-future-of-shaders-on-the-web/
- http://www.linusakesson.net/programming/tty/index.php
- https://www.uninformativ.de/blog/postings/2018-02-24/0/POSTING-en.html
- https://github.com/bisqwit/that_terminal
- https://en.wikipedia.org/wiki/Fira_(typeface)#Fira_Mono
