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

# Default cursor is Block
# Other available options are: '_' and '|'
cursor = '_'

# Set WGPU rendering performance
# High: Adapter that has the highest performance. This is often a discrete GPU.
# Low: Adapter that uses the least possible power. This is often an integrated GPU.
performance = "High"

# it will look for dracula.toml in themes folder
# (macos and linux: ~/.config/rio/themes/dracula.toml)
# ...
# dracula theme code is available in:
# https://github.com/dracula/rio-terminal
theme = "dracula"

# define x axis padding (default is 10)
padding-x = 0

# environment variables
# (the example below sets fish as the default SHELL in macos
# please do not copy this if you do not need)
env-vars = ['SHELL=/opt/homebrew/bin/fish']

# This config only works on MacOs.
# Possible choices: 'both', 'left' and 'right'.
option_as_alt = 'both'

# window-opacity changes the window transparency state.
# Only works for Windows / X11 / WebAssembly
window-opacity = 0.5

# Colors definition will overwrite any property in theme
# (considering if theme folder does exists and is being used)
[colors]
background = "#BBBD64"
foreground = "#040400"
cursor = "#242805"
tabs-active = "#F8A145"
blue = "#454A12"

[developer]
# This property enables log level filter. Default is "OFF".
log-level = "INFO"

[advanced]
# This property disable renderer processes while Rio is unfocused.
disable-renderer-when-unfocused = false
{% endhighlight %}

If you have any suggestion of configuration ideas to Rio, please feel free to [open an issue](https://github.com/raphamorim/rio/issues/new).

[Move to default colors ->](/rio/docs/default-colors)
