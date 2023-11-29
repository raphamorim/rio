---
title: 'Configuration file'
language: 'en'
---

The configuration should be the following paths otherwise Rio will use the default configuration.

MacOS and Linux configuration file path is `~/.config/rio/config.toml`.

Windows configuration file path is `C:\Users\USER\AppData\Local\rio\config.toml` (replace "USER" with your user name).

Updates to the configuration file automatically triggers Rio to render the terminal with the new configuration.

## Cursor

Default cursor is `Block`.

Other available options are: `_` and `|`

```toml
cursor = '▇'
```

## Editor

Default editor is `vi`.

Whenever the key binding `OpenConfigEditor` is triggered it will use the value of the editor along with the rio configuration path.

An example, considering you have VS Code installed and you want to use it as your editor:

```toml
editor = 'code'
```

Whenever `OpenConfigEditor` runs it will trigger `$ code <path-to-rio-configuration-file>`.

## Blinking Cursor

Default is `false`

```toml
blinking-cursor = false
```

## Scroll Speed Multiplier

Default is `3.0`.

You can change how many lines are scrolled each time by setting this option.

```toml
scroll-multiplier = 3.0
```

## Ignore theme selection foreground color

Default is `false`

```toml
ignore-selection-foreground-color = false
```

## Enable Kitty Keyboard protocol

Default is false

```toml
use-kitty-keyboard-protocol = false
```

## Performance

Set WGPU rendering performance

- `High`: Adapter that has the highest performance. This is often a discrete GPU.
- `Low`: Adapter that uses the least possible power. This is often an integrated GPU.

```toml
performance = "High"
```

## Themes

Rio looks for a specified theme in the themes folder.

- MacOS and Linux: `~/.config/rio/themes/dracula.toml`
- Windows: `C:\Users\USER\AppData\Local\rio\themes\dracula.toml`

```toml
theme = "dracula"
```

## Padding-x

Define x axis padding (default is 10)

```toml
padding-x = 10
```

## Option as Alt

This config only works on MacOs.

Possible choices: `both`, `left` and `right`.

```toml
option-as-alt = 'left'
```

## Window

- `width` - define the intial window width.
	- Default: `600`

- `height` - define the inital window height.
	- Default: `400`

- `mode` - define how the window will be created
  - `Windowed` (default) is based on width and height
  - `Maximized` window is created with maximized
  - `Fullscreen` window is created with fullscreen

- `foreground-opacity` Set text opacity.
	- Default: `1.0`.

- `background-opacity` Set background opacity.
	- Default: `1.0`.

- `blur` Set blur on the window background. Changing this config requires restarting Rio to take effect.
	- Default: `false`.

- `background-image` Set an image as background.
	- Default: `None`

Example:

```toml
[window]
width = 600
height = 400
mode = "Windowed"
foreground-opacity = 1.0
background-opacity = 1.0
blur = false
```

### Using blur and background opacity:

```toml
[window]
foreground-opacity = 1.0
background-opacity = 0.5
blur = true
```

![Demo blur and background opacity](/assets/demos/demo-macos-blur.png)

### Using image as background:

```toml
[window.background-image]
path = "/Users/hugoamor/Desktop/musashi.png"
opacity = 0.5
width = 400.0
height = 400.0
x = 0.0
y = -100.0
```

![Demo image as background](/assets/demos/demo-background-image.png)

## Fonts

Configure fonts used by the terminal

Note: You can set different font families but Rio terminal
will always look for regular font bounds whene

You can also set family on root to overwrite all fonts.

```toml
[fonts]
family = "cascadiamono"
```

You can also specify extra fonts to load:

```toml
[fonts]
extras = [{ family = "Microsoft JhengHei" }]
```

The font configuration default:

```toml
[fonts]
size = 18

[fonts.regular]
family = "cascadiamono"
style = "normal"
weight = 400

[fonts.bold]
family = "cascadiamono"
style = "normal"
weight = 800

[fonts.italic]
family = "cascadiamono"
style = "italic"
weight = 400

[fonts.bold-italic]
family = "cascadiamono"
style = "italic"
weight = 800
```

## Navigation

- `mode` - Define navigation mode
	- `NativeTab` (MacOs only)
	- `CollapsedTab`
	- `BottomTab`
	- `TopTab`
	- `Breadcrumb`
	- `Plain`

- `clickable` - Enable click on tabs to switch.
- `use-current-path` - Use same path whenever a new tab is created.
- `color-automation` - Set a specific color for the tab whenever a specific program is running.
- `macos-hide-window-buttons` - (MacOS only) Hide window buttons

```toml
[navigation]
mode = "CollapsedTab"
clickable = false
use-current-path = false
color-automation = []
macos-hide-window-buttons = false
```

## Shell

You can set `shell.program` to the path of your favorite shell, e.g. `/bin/fish`.

Entries in `shell.args` are passed unmodified as arguments to the shell.

Default:
	- (macOS) user login shell
	- (Linux/BSD) user login shell
	- (Windows) powershell

### Shell Examples

1. MacOS using fish shell from bin path:

```toml
shell = { program = "/bin/fish", args = ["--login"] }
```

2. Windows using powershell

```toml
shell = { program = "pwsh", args = [] }
```

3. Windows using powershell with login

```toml
shell = { program = "pwsh", args = ["-l"] }
```

## Startup directory

Directory the shell is started in. If this is unset the working directory of the parent process will be used.

This configuration only works if `use-fork` is disabled

```toml
working-dir = "/Users/raphael/Documents/"
```

## Environment variables

The example below sets fish as the default SHELL using env vars, please do not copy this if you do not need it.

```toml
env-vars = []
```

## Disable render when unfocused

This property disables the renderer process when Rio no longer has focus.

```toml
disable-renderer-when-unfocused = false
```

## Use fork

Defaults for POSIX-based systems (Windows is not configurable):

- MacOS: spawn processes
- Linux/BSD: fork processes

```toml
use-fork = false
```

## Colors

Defining colors in the configuration file will override any colors set in the theme if you're using a theme.

The default configuration is without a theme.

Example:

```toml
[colors]
background = '#0F0D0E'
foreground = '#F9F4DA'
cursor = '#F38BA3'
tabs = '#443d40'
tabs-active = '#F38BA3'
green = '#0BA95B'
red = '#ED203D'
blue = '#12B5E5'
yellow = '#FCBA28'
```

## Bindings

You can create custom key bindings for Rio terminal, [more information](/docs/key-bindings#custom-key-bindings)

```toml
[bindings]
keys = [
  { key = "q", with = "super", action = "Quit" },
  # Bytes[27, 91, 53, 126] is equivalent to "\x1b[5~"
  { key = "home", with = "super | shift", bytes = [27, 91, 53, 126] }
]
```

## Log level

This property enables log level filter. Default is "OFF".

```toml
[developer]
log-level = "OFF"
```

If you have any suggestion of configuration ideas to Rio, please feel free to [open an issue](https://github.com/raphamorim/rio/issues/new).
