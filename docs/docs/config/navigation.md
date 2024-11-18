---
title: 'navigation'
language: 'en'
---

- `hide-if-single` - Hide navigation UI if there is only one tab. It does not work for `NativeTab`. 
- `clickable` - Enable click on tabs to switch.
- `use-current-path` - Use same path whenever a new tab is created (Note: requires [`use-fork`](/docs/config/use-fork) to be set to false).
- `color-automation` - Set a specific color for the tab whenever a specific program is running, or in a specific directory.
- `use-split` - Enable split panels feature.
- `open-config-with-split` - Enable split for open configuration file.

```toml
[navigation]
mode = "Bookmark"
clickable = false
hide-if-single = true
use-current-path = false
color-automation = []
use-split = true
open-config-with-split = true
```

## Mode

Rio has multiple styles of showing navigation/tabs.

### Bookmark

Note: The example below is using the [Dracula](https://github.com/dracula/rio-terminal) color scheme instead of Rio default colors.

`Bookmark` is the default navigation mode.

<img src="https://miro.medium.com/v2/resize:fit:1400/format:webp/1*gMLWcZkniSHUT6Cb7L06Gg.png" width="60%" />

Usage:

```toml
[navigation]
mode = "Bookmark"
```

### NativeTab (MacOS only)

<img alt="Demo NativeTab" src="/rio/assets/posts/0.0.17/demo-native-tabs.png" width="60%"/>

Usage:

```toml
[navigation]
mode = "NativeTab"
```

### BottomTab

Note: `BottomTab` does not support click mode yet.

<img alt="Demo BottomTab" src="/rio/assets/features/demo-bottom-tab.png" width="58%"/>

Usage:

```toml
[colors]
tabs = "#000000"

[navigation]
mode = "BottomTab"
```

### TopTab

Note: `TopTab` does not support click mode yet.

<img alt="Demo TopTab" src="/rio/assets/features/demo-top-tab.png" width="70%"/>

Usage:

```toml
[colors]
tabs = "#000000"

[navigation]
mode = "TopTab"
```

### Plain

Plain navigation mode will simply turn off any tab key binding.

This mode is perfect if you use Rio terminal with tmux or zellij.

Usage:

```toml
[navigation]
mode = "Plain"
```

## Split

Enable split feature. It is enabled by default.

```toml
[navigation]
use-split = true
```

## Open config with split

Enable open configuration file by split. It is enabled by default.

```toml
[navigation]
open-config-with-split = true
```

![Demo split](/assets/features/demo-split.png)

## Hide if is only one tab

The property `hide-if-single` hides navigation UI if there is only one tab. It does not work for `NativeTab`.

Default is `true`.

```toml
[navigation]
hide-if-single = true
```

## Color automation for navigation

Rio supports specifying the color of tabs using the `program` and `path` options.

Note: `path` is only available for MacOS, BSD and Linux.

```toml
[navigation]
color-automation = [
  # Set tab to red (#FF0000) when NeoVim is open.
  { program = "nvim", color = "#FF0000" },
  # Set tab to green  (#00FF00) when in the projects folder
  { path = "/home/YOUR_USERNAME/projects", color = "#00FF00" },
    # Set tab to blue (#0000FF) when in the Rio folder AND vim is open
  { program = "vim", path = "/home/YOUR_USERNAME/projects/rio", color = "#0000FF" },
]
```

### Program

The example below sets `#FFFF00` as color background whenever `nvim` is running.

<p>
<img alt="example navigation with program color automation using BottomTab" src="/rio/assets/features/demo-colorized-navigation.png" width="48%"/>

<img alt="example navigation with program color automation using Bookmark" src="/rio/assets/features/demo-colorized-navigation-2.png" width="48%"/>
</p>

The configuration would be like:

```toml
[navigation]
color-automation = [
  { program = "nvim", color = "#FFFF00" }
]
```

### Path

The example below sets `#FFFF00` as color background when in the `/home/geg/.config/rio` path.

Note: `path` is only available for MacOS, BSD and Linux.

The configuration would be like:

```toml
[navigation]
color-automation = [
  { path = "/home/geg/.config/rio", color = "#FFFF00" }
]
```

<p>
<img alt="example navigation with path color automation using TopTab" src="/rio/assets/features/demo-colorized-navigation-path-1.png" width="48%"/>

<img alt="example navigation with path color automation using Bookmark" src="/rio/assets/features/demo-colorized-navigation-path-2.png" width="48%"/>
</p>

### Program and path

It is possible to use both `path` and `program` at the same time.

The example below sets `#FFFF00` as color background when in the `/home` path and `nvim` is open.

Note: `path` is only available for MacOS, BSD and Linux.

The configuration would be like:

```toml
[navigation]
color-automation = [
  { program = "nvim", path = "/home", color = "#FFFF00" }
]
```

<p>
<img alt="example navigation with program and path color automation using TopTab" src="/rio/assets/features/demo-colorized-navigation-program-and-path-1.png" width="48%"/>

<img alt="example navigation with program and path color automation using Bookmark" src="/rio/assets/features/demo-colorized-navigation-program-and-path-2.png" width="48%"/>
</p>