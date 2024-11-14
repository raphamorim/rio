---
title: 'Color automation for navigation'
language: 'en'
---

Rio allows specifying color for tabs based on program and path contexts, using the `program` and `path` options.

It is possible to combine `program` and `path`.

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

It is possible to combine `path` and `program`.

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
