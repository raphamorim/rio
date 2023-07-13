---
layout: docs
class: docs
title: 'Documentation'
language: 'en'
---

## Configuration File

Note: Configuration file is not created in the installation process. Rio always assume the file doesn't exist and proceed with default configuration.

The configuration should be the following paths otherwise Rio will use the default configuration.

MacOS and Linux configuration file path is <span class="keyword">~/.config/rio/config.toml</span>.

Windows	configuration file path is <span class="keyword">C:\Users\USER\AppData\Local\rio\config.toml</span> (replace "USER" with your user name).

Any file update in the configuration file will trigger a render operation in Rio terminal with the new configuration.

{% highlight toml %}
font = "CascadiaMono"
font-size = 16

# Cursor
#
# Default cursor is Block
# Other available options are: '_' and '|'
cursor = '_'

# Perfomance
#
# Set WGPU rendering performance
# High: Adapter that has the highest performance. This is often a discrete GPU.
# Low: Adapter that uses the least possible power. This is often an integrated GPU.
performance = "High"

# Theme
#
# It makes Rio look for the specified theme in the themes folder
# (macos and linux: ~/.config/rio/themes/dracula.toml)
# (windows: C:\Users\USER\AppData\Local\rio\themes\dracula.toml)
# 
# Dracula theme code is available in:
# https://github.com/dracula/rio-terminal
theme = "dracula"

# Padding-x
#
# define x axis padding (default is 10)
padding-x = 0

# Option as Alt
#
# This config only works on MacOs.
# Possible choices: 'both', 'left' and 'right'.
option_as_alt = 'both'

# Window Opacity
#
# window-opacity changes the window transparency state.
# Only works for Windows / X11 / WebAssembly
window-opacity = 0.5

# Shell
#
# You can set `shell.program` to the path of your favorite shell, e.g. `/bin/fish`.
# Entries in `shell.args` are passed unmodified as arguments to the shell.
#
# Default:
#   - (macOS) /bin/zsh --login
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
working_dir = "/Users/raphael/Documents/"

# Environment variables
#
# The example below sets fish as the default SHELL using env vars
# please do not copy this if you do not need
env-vars = ['SHELL=/opt/homebrew/bin/fish']

# Disable render when unfocused
#
# This property disable renderer processes while Rio is unfocused.
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
[colors]
background = "#BBBD64"
foreground = "#040400"
cursor = "#242805"
tabs-active = "#F8A145"
blue = "#454A12"

[developer]
# Log level
#
# This property enables log level filter. Default is "OFF".
log-level = "INFO"
{% endhighlight %}

If you have any suggestion of configuration ideas to Rio, please feel free to [open an issue](https://github.com/raphamorim/rio/issues/new).

[Move to default colors ->](/rio/docs/default-colors)
