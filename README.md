# Rio: ⚡ terminal app 

> tl;dr: Rio is built to run everywhere, as a native desktop applications by Rust/WebGPU or even in the browser powered by WebAssembly/WebGPU.

![Demo MacOs](docs/demo-macos.png)

## Summary

- [About Rio](#about-rio)
- [Configuration file](#configuration-file)
    - [Performance](#performance)
    - [Height](#height)
    - [Width](#width)
    - [Style](#style)
    - [Advanced](#advanced)
    - [Colors](#colors)
    - [Developer](#developer)
- [Status](#development-status)
- [Acknowledgments](#acknowledgments)

## About Rio

Website: https://raphamorim.io/rio

> If you are using or want to help in any way please consider to donate via [Github Sponsors](https://github.com/sponsors/raphamorim).
> 
> Rio would not be possible without [few acknowledgements](#acknowledgements) and specially [Alacritty](https://github.com/alacritty/alacritty/), since a lot of Rio functionalities (e.g: ANSI parser, events, grid system) was originally written (and still uses a good amount) of Alacritty code.
>

A terminal application that's built with Rust, WebGPU, Tokio runtime. It targets to have the best frame per second experience as long you want, but is also configurable to use as minimal from GPU.

Below some of Rio's features:

- Cross-platform.
- Configurable (Render level, colors, icons, fonts).
- Offloads rendering to the GPU for lower system load.
- Uses threaded rendering for absolutely minimal latency.
- Tabs support.

Rio uses WGPU, which is an implementation of WebGPU for use outside of a browser and as backend for firefox's WebGPU implementation. WebGPU allows for more efficient usage of modern GPU's than WebGL. **[More info](https://users.rust-lang.org/t/what-is-webgpu-and-is-it-ready-for-use/62331/8)**

It also relies on Rust memory behavior, since Rust is a memory-safe language that employs a compiler to track the ownership of values that can be used once and a borrow checker that manages how data is used without relying on traditional garbage collection techniques. **[More info](https://stanford-cs242.github.io/f18/lectures/05-1-rust-memory-safety.html)**

## Configuration File

The configuration should be the following paths otherwise Rio will use the default configuration.

- macOS path: `~/.rio/config.toml`

Default configuration of `config.toml`:

```toml
# Rio default configuration file
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
enable-logs = false

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
tabs             = '#FFFFFF'
tabs-active      = '#FC7428'
white            = '#FFFFFF'
yellow           = '#FFFFFF'
dim-black        = '#FFFFFF'
dim-blue         = '#FFFFFF'
dim-cyan         = '#FFFFFF'
dim-foreground   = '#FFFFFF'
dim-green        = '#FFFFFF'
dim-magenta      = '#FFFFFF'
dim-red          = '#FFFFFF'
dim-white        = '#FFFFFF'
dim-yellow       = '#FFFFFF'
light-black      = '#FFFFFF'
light-blue       = '#FFFFFF'
light-cyan       = '#FFFFFF'
light-foreground = '#FFFFFF'
light-green      = '#FFFFFF'
light-magenta    = '#FFFFFF'
light-red        = '#FFFFFF'
light-white      = '#FFFFFF'
light-yellow     = '#FFFFFF'
```

#### `performance`

Set terminal WGPU rendering perfomance.

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

#### `height`

Set terminal window height.

```toml
# <height> Set default height
# default: 438
height = 400
```

#### `width`

Set terminal window width.

```toml
# <width> Set default width
# default: 662
width = 800
```

### Style

#### `font`

This property will change later to an actual font path. Currently Rio has 2 fonts builtin: `Firamono`, `Novamono`.

```toml
[style]
font = "Firamono"
```

#### `font-size`

Sets font size.

```toml
[style]
font-size = 16.0
```

### Advanced

#### `tab-character-active`

This property sets a `char` for an active tab.

```toml
[style]
tab-character-active = '●'
```

#### `tab-character-inactive`

This property sets a `char` for an inactive tab.

```toml
[style]
tab-character-inactive = '■'
```

#### `disable-renderer-when-unfocused`

This property disable renderer processes until focus on Rio term again.

```toml
[style]
disable-renderer-when-unfocused = false
```

## Developer

#### `enable-fps-counter`

This property enables frame per second counter.

```toml
[style]
enable-fps-counter = false
```

#### `enable-logs`

This property enables Rio logging.

```toml
[style]
enable-logs = false
```

## Colors

Default color palette demo:

Usage example running the following bash script:

```bash
for x in {0..8}; do
    for i in {30..37}; do
        for a in {40..47}; do
            echo -ne "\e[$x;$i;$a""m\\\e[$x;$i;$a""m\e[0;37;40m "
        done
        echo
    done
done
echo ""
```

Or one-liner:

```bash
for x in {0..8}; do for i in {30..37}; do for a in {40..47}; do echo -ne "\e[$x;$i;$a""m\\\e[$x;$i;$a""m\e[0;37;40m "; done; echo; done; done; echo ""
```

## Development Status

Basic features are under development for MacOs right now.

| Platform | Development Status |
| --- | --- |
| MacOs _as desktop application_ | In development 👷 |
| Linux _as desktop application_ | In development 👷 * |
| Windows _as desktop application_ | Not started yet |
| Web Browser _(tests on Chrome and Firefox)_ | Not started yet |
| Nintendo Switch | Not started yet |

_* Development and tests are targeting Wayland, probably is not stable on X11 yet._

## Acknowledgments

- The default color palette is based on the colors of [ui.dev](https://ui.dev/).
- Text glyph render is from https://github.com/hecrj/wgpu_glyph
- https://github.com/wez/wezterm
- https://www.gaijin.at/en/infos/ascii-ansi-character-table#asciicontrol
