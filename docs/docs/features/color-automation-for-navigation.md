---
title: 'Color automation for navigation'
language: 'en'
---

Rio allows to specify color overwrites for tabs based on program context.

The example below sets `#FFFF00` as color background whenever `nvim` is running.

<p>
<img alt="example navigation with color automation" src="/rio/assets/features/demo-colorized-navigation.png" width="48%"/>

<img alt="second example navigation with color automation" src="/rio/assets/features/demo-colorized-navigation-2.png" width="48%"/>
</p>

The configuration would be like:

```toml
[navigation]
color-automation = [
  { program = "nvim", color = "#FFFF00" }
]
```
