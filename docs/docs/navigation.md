---
title: 'Navigation'
language: 'en'
---

Rio allows to choose navigation between the following options:

### CollapsedTab

The `CollapsedTab` is Rio terminal default navigation mode for Linux, BSD and Windows.

Note: The example below is using Dracula color scheme instead of Rio default colors.

<img src="https://miro.medium.com/v2/resize:fit:1400/format:webp/1*gMLWcZkniSHUT6Cb7L06Gg.png" width="60%" />

Usage:

```toml
[navigation]
mode = "CollapsedTab"
```

### NativeTab (MacOS only)

The `NativeTab` is Rio terminal default navigation mode for MacOs.

Note: NativeTab only works for MacOS.

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

### Breadcrumb

Note: `Breadcrumb` does not support click mode yet and is only available for MacOS, BSD and Linux.

<img alt="Demo Breadcrumb" src="/rio/assets/features/demo-breadcrumb.png" width="70%"/>

Usage:

```toml
[navigation]
mode = "Breadcrumb"
```

### Plain

Plain navigation mode will simply turn off any tab key binding.

This mode is perfect if you use Rio terminal with tmux or zellij.

Usage:

```toml
[navigation]
mode = "Plain"
```

### Color automation for navigation

Rio allows to specify color overwrites for tabs based on program and path contexts, using the `program` and `path` options.

It is possible to use `program` and `path` at the same time.

Note: `path` is only available for MacOS, BSD and Linux.

#### Program

The example below sets `#FFFF00` as color background whenever `nvim` is running.

<p>
<img alt="example navigation with program color automation using TopTab" src="/rio/assets/features/demo-colorized-navigation.png" width="48%"/>

<img alt="example navigation with program color automation using CollapsedTab" src="/rio/assets/features/demo-colorized-navigation-2.png" width="48%"/>
</p>

The configuration would be like:

```toml
[navigation]
color-automation = [
  { program = "nvim", color = "#FFFF00" }
]
```

#### Path

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

<img alt="example navigation with path color automation using CollapsedTab" src="/rio/assets/features/demo-colorized-navigation-path-2.png" width="48%"/>
</p>

#### Program and path

It is also possible to use both `path` and `program` at the same time.

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

<img alt="example navigation with program and path color automation using CollapsedTab" src="/rio/assets/features/demo-colorized-navigation-program-and-path-2.png" width="48%"/>
</p>
