# Rio: ⚡ terminal app 

Website: https://raphamorim.io/rio

This project depends of donations, so if you are using or want to help in any way please consider to donate via [Github Sponsors](https://github.com/sponsors/raphamorim).

- Cross-platform
- Offloads rendering to the GPU for lower system load
- Uses threaded rendering for absolutely minimal latency
- Tabs support

Note: Rio wouldn't be possible without [few acknowledgements](#acknowledgments).

#### Status

Basic features are under development for MacOs right now.

| Platform | Development Status |
| --- | --- |
| MacOs | In development 👷 |
| Linux | Not started yet |
| Windows | Not started yet |

Last testing build for macOS (338b4c411ac62dc4bdf68b5c9b8e5da7d7bab8ca):

#### Available themes

[See more in configuration](#theme)

| Basic | Modern * |
| --- | --- |
| ![Demo basic macOS](docs/demo-macos-basic.png) | ![Demo macOS](docs/demo-macos.png) |

_* Modern theme is only available in Developer builds._

#### WGPU based

WGPU is an implementation of WebGPU for use outside of a browser and as backend for firefox's WebGPU implementation. WebGPU allows for more efficient usage of modern GPU's than WebGL. [More info](https://users.rust-lang.org/t/what-is-webgpu-and-is-it-ready-for-use/62331/8)

#### Low CPU and memory usage

You want to avoid a browser-based application to reduce memory and CPU consumption. Electron for example, uses Chromium under the hood so your user sees the same on Windows, Linux and macOS but Rio have same compatibility rendering based on WGPU.

Rio also relies on Rust memory behavior: Rust is a memory-safe language that employs a compiler to track the ownership of values that can be used once and a borrow checker that manages how data is used without relying on traditional garbage collection techniques. [More info](https://stanford-cs242.github.io/f18/lectures/05-1-rust-memory-safety.html)

## Configuration

The configuration should be the following paths otherwise Rio will use the default configuration.

- macOS path: `~/.config/rio.toml`

> `$XDG_CONFIG_HOME` defines the base directory relative to which user specific configuration files should be stored. If `$XDG_CONFIG_HOME` is either not set or empty, a default equal to `$HOME/.config` should be used.

Default configuration of `rio.toml`:

```toml
# Rio default configuration file
performance = "High"
height = 438
width = 662

[colors]
background = '#151515'
foreground = '#FFFFFF'
cursor = '#8E12CC'
tabs-active = '#F8A145'

[style]
font = "Firamono"
font-size = 16
theme = "Basic"

[advanced]
tab-character = '■'
monochrome = false
enable-fps-counter = false
```

_* Monochrome is not implemented yet._

### Perfomance

Description: Set terminal WGPU rendering perfomance.

- High: Adapter that has the highest performance. This is often a discrete GPU.
- Low: Adapter that uses the least possible power. This is often an integrated GPU.

See more in https://docs.rs/wgpu/latest/wgpu/enum.PowerPreference.html

```toml
# <performance> Set WGPU rendering perfomance
# default: High
# options: High, Low
# High: Adapter that has the highest performance. This is often a discrete GPU.
# Low: Adapter that uses the least possible power. This is often an integrated GPU.
performance = "High"
```

### Height

Description: Set terminal window height.

```toml
# <height> Set default height
# default: 438
height = 400
```

### Width

Description: Set terminal window width.

```toml
# <width> Set default width
# default: 662
width = 800
```

### Columns

Description: Defines the column width of your console window. This mode is system dependant, and returns an error if you specify a column width that is not supported by your operating system.

```toml
# <width> Set default width
# default: 80 (macOs)
columns = 80
```

### Rows

Description: Define the row height of your console window. This mode is system dependant, and returns an error if you specify a row height that is not supported by your operating system. 

```toml
# <width> Set default width
# default: 25 (macOs)
rows = 25
```

### Colors

- `background` - Set background color.
- `cursor` - Set cursor color.
- `foreground` - Set foreground color.
- `tabs` - Set tab color.
- `tabs-active` - Set active tab color.

```toml
[colors]
background = "#151515"
cursor = "#8E12CC"
foreground = "#FFFFFF"
tabs = "#8E12CC"
tabs-active = "#8E12CC"
```

### Style

#### Font

This property will change later to an actual font path. Currently Rio has 2 fonts builtin: `Firamono`, `Novamono`.

- `font` - Set font size.

```toml
[style]
font = "Firamono"
```

#### Font Size

- `font_size` - Set font size.

```toml
[style]
font_size = 16.0
```

#### Theme

Description: Set Rio terminal theme

- Modern
- Basic

```toml
# <theme> Set Rio terminal theme
# default: Basic
# options: Modern, Basic
theme = "Modern"
```

## Acknowledgments

- Term and Grid system was built originally on Alacritty's code https://github.com/alacritty/alacritty/blob/766a3b5582fa8ee13506c0f23c9c145ff0012078/alacritty_terminal/src/grid/mod.rs
- Text glyph render is from https://github.com/hecrj/wgpu_glyph
- https://github.com/wez/wezterm
- https://www.gaijin.at/en/infos/ascii-ansi-character-table#asciicontrol
