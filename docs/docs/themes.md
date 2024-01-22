---
title: 'Themes'
language: 'en'
---

The configuration property `theme` is used for specifying the theme. Rio will look in the `themes` folder for the theme.

You can see common paths for the `themes` directory here:

Note: Remember to replace "YOUR_USERNAME" with your actual user name.

| Platform | Path                                              |
| -------- | ------------------------------------------------- |
| Mac      | `/Users/YOUR_USERNAME/.config/rio/themes`         |
| Linux    | `/home/YOUR_USERNAME/.config/rio/themes`          |
| Windows  | `C:\Users\YOUR_USERNAME\AppData\Local\rio\themes` |

In the example below, we will setup the Dracula theme for Rio. The theme can be downloaded from [github.com/dracula/rio-terminal](https://github.com/dracula/rio-terminal).

After downloading the `dracula.toml` file, move it inside the folder `themes` in the configuration folder.

```toml
#  ~/.config/rio/config.toml
theme = "dracula"
```

It should look like this:

![Dracula theme example](/assets/posts/0.0.5/dracula-nvim.png)

Another option would be to install the [Lucario color scheme for Rio terminal](https://github.com/raphamorim/lucario/#rio-terminal), by moving the downloaded file to `~/.config/rio/themes/lucario.toml` and setting the `theme` property:

```toml
#  ~/.config/rio/config.toml
theme = "lucario"
```

![Lucario theme example](https://github.com/raphamorim/lucario/raw/main/images/rio.png)

You can find more than 250 themes for Rio terminal in this repository: [mbadolato/iTerm2-Color-Schemes/tree/master/rio](https://github.com/mbadolato/iTerm2-Color-Schemes/tree/master/rio).

## Building your own theme

Building your own theme for Rio is very straightforward.

Simply create a new theme file in your configuration themes folder (E.g. `~/.config/rio/themes/foobar.toml`) and choose your preferred colors:

Note: Missing fields will use the default for Rio.

```toml
# ~/.config/rio/themes/foobar.toml

[colors]
background = ""
foreground = ""
selection-background = ""
selection-foreground = ""
tabs-active = ""
cursor = ""
vi-cursor = ""
# Regular colors
black = ""
blue = ""
cyan = ""
green = ""
magenta = ""
red = ""
tabs = ""
white = ""
yellow = ""
# Dim colors
dim-black = ""
dim-blue = ""
dim-cyan = ""
dim-foreground = ""
dim-green = ""
dim-magenta = ""
dim-red = ""
dim-white = ""
dim-yellow = ""
# Light colors
light-black = ""
light-blue = ""
light-cyan = ""
light-foreground = ""
light-green = ""
light-magenta = ""
light-red = ""
light-white = ""
light-yellow = ""
```

After that all you have to do is set the `theme` property in your configuration file.

```toml
# ~/.config/rio/config.toml
theme = "foobar"
```

Proud of your new theme? Why not share it on the [Rio Discord](https://discord.gg/zRvJjmKGwS)!
