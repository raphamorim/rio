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

# <height> Set default height
# default: 400
height = 400

# <width> Set default width
# default: 600
width = 600

## TODO: Add more configs
```

## TODO

- [ ] Fix clippy
- [x] pty
- [ ] pty open
- [ ] Render PTY COLS and ROWS based on window size
	- [ ] Tests with VIM
- [ ] Add scroll to text
	- [ ] Ref: https://sotrh.github.io/learn-wgpu/intermediate/tutorial12-camera/#cleaning-up-lib-rs
- [x] WGPU rendering
	- [ ] Render font with custom color, size and family
	- [ ] Fix topbar when resize
	- [ ] Keep rendering with intervals
- [ ] Read and use configuration
- [ ] Keyboard input
	- [ ] Alphabet keys (uppercase/lowcase)
	- [ ] Numbers keys
	- [ ] Control keys
- [x] Window resizing
- [ ] Allow use set different font-size
- [ ] Themes support
- [ ] Style rendering (italic, bold, underline)
- [ ] Character set

## Credits

- Text mod code is from with https://github.com/hecrj/wgpu_glyph