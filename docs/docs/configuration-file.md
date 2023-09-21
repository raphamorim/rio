---
layout: docs
class: docs
title: 'Configuration file'
language: 'en'
---

## Configuration File

Note: Configuration file is not created in the installation process. Rio always assume the file doesn't exist and proceed with default configuration.

The configuration should be the following paths otherwise Rio will use the default configuration.

MacOS and Linux configuration file path is <span class="keyword">~/.config/rio/config.toml</span>.

Windows	configuration file path is <span class="keyword">C:\Users\USER\AppData\Local\rio\config.toml</span> (replace "USER" with your user name).

Any file update in the configuration file will trigger a render operation in Rio terminal with the new configuration.

{% highlight toml %}
# Cursor
#
# Default cursor is Block
# Other available options are: '_' and '|'
cursor = '_'

# Blinking Cursor
#
# Default is false
#
blinking-cursor = true

# Performance
#
# Set WGPU rendering performance
# High: Adapter that has the highest performance. This is often a discrete GPU.
# Low: Adapter that uses the least possible power. This is often an integrated GPU.
#
performance = "High"

# Theme
#
# It makes Rio look for the specified theme in the themes folder
# (macos and linux: ~/.config/rio/themes/dracula.toml)
# (windows: C:\Users\USER\AppData\Local\rio\themes\dracula.toml)
#
# Dracula theme code is available in:
# https://github.com/dracula/rio-terminal
#
theme = "dracula"

# Ignore theme selection foreground color
#
# Default is false
#
ignore_theme_selection_fg_color = false

# Adaptive Theme
#
# Changes theme based on the system theme (light and dark)
# Only works for Web, MacOS and Windows
# This configuration is disabled by default
#
[adaptive-theme]
light = "belafonte-day"
dark = "belafonte-night"

# Padding-x
#
# define x axis padding (default is 10)
#
padding-x = 0

# Option as Alt
#
# This config only works on MacOs.
# Possible choices: 'both', 'left' and 'right'.
#
option-as-alt = 'both'

# Window configuration
#
# • width - define the intial window width.
#   Default: 600
#
# • height - define the inital window height.
#   Default: 400
#
# • mode - define how the window will be created
#     - "Windowed" (default) is based on width and height
#     - "Maximized" window is created with maximized
#     - "Fullscreen" window is created with fullscreen
#
[window]
width = 600
height = 400
mode = "Windowed"

# Background configuration
#
# • opacity - changes the background transparency state
#   Default: 1.0
#
# • mode - defines background mode bewteen "Color" and "Image"
#   Default: Color
#
# • image - Set an image as background
#   Default: None
#
[background]
mode = "Image"
opacity = 1.0
[background.image]
path = "/Users/rapha/Desktop/eastward.jpg"
width = 200.0
height = 200.0
x = 0.0
y = 0.0

# Fonts
#
# Configure fonts used by the terminal
#
# Note: You can set different font families but Rio terminal
# will always look for regular font bounds whene
#
# Another Note: "cascadiamono" font is built-in.
#
[fonts]
size = 18

# # You can also set family on root to overwritte all fonts
# family = "cascadiamono"

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

# Navigation
#
# "mode" - Define navigation mode
#   - NativeTab (MacOs only)
#   - CollapsedTab
#   - BottomTab
#   - TopTab
#   - Breadcrumb
#   - Plain
#
# "clickable" - Enable click on tabs to switch.
# "use-current-path" - Use same path whenever a new tab is created.
# "color-automation" - Set a specific color for the tab whenever a specific program is running.
# "macos-hide-window-buttons" - (MacOS only) Hide window buttons
#
[navigation]
mode = "BottomTab"
clickable = false
use-current-path = true
color-automation = [
  { program = "nvim", color = "#FFFF00" }
]
macos-hide-window-buttons = false

# Shell
#
# You can set `shell.program` to the path of your favorite shell, e.g. `/bin/fish`.
# Entries in `shell.args` are passed unmodified as arguments to the shell.
#
# Default:
#   - (macOS) user login shell
#   - (Linux/BSD) user login shell
#   - (Windows) powershell
#
shell = { program = "/bin/zsh", args = ["--login"] }

# Startup directory
#
# Directory the shell is started in. If this is unset the working
# directory of the parent process will be used.
#
# This configuration only has effect if use-fork is disabled
#
working-dir = "/Users/raphael/Documents/"

# Environment variables
#
# The example below sets fish as the default SHELL using env vars
# please do not copy this if you do not need
#
env-vars = ['SHELL=/opt/homebrew/bin/fish']

# Disable render when unfocused
#
# This property disable renderer processes while Rio is unfocused.
#
disable-renderer-when-unfocused = false

# Use fork
#
# Defaults for POSIX-based systems (Windows is not configurable):
# MacOS: spawn processes
# Linux/BSD: fork processes
#
use-fork = false

# Colors
#
# Colors definition will overwrite any property in theme
# (considering if theme folder does exists and is being used)
#
[colors]
background = "#BBBD64"
foreground = "#040400"
cursor = "#242805"
tabs-active = "#F8A145"
blue = "#454A12"


# Bindings
#
# Create custom Key bindings for Rio terminal
# More information in: raphamorim.io/rio/docs/custom-key-bindings
#
[bindings]
keys = [
  { key = "q", with = "super", action = "Quit" },
  # Bytes[27, 91, 53, 126] is equivalent to "\x1b[5~"
  { key = "home", with = "super | shift", bytes = [27, 91, 53, 126] }
]

[developer]
# Log level
#
# This property enables log level filter. Default is "OFF".
#
log-level = "INFO"
{% endhighlight %}

If you have any suggestion of configuration ideas to Rio, please feel free to [open an issue](https://github.com/raphamorim/rio/issues/new).

--

[Move to default colors and themes ->](/rio/docs/default-colors-and-themes#default-colors-and-themes)
