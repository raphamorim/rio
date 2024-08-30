---
title: 'window'
language: 'en'
---

- `width` - define the initial window width.

  - Default: `600`

- `height` - define the initial window height.

  - Default: `400`

- `mode` - define how the window will be created

  - `Windowed` (default) is based on width and height
  - `Maximized` window is created with maximized
  - `Fullscreen` window is created with fullscreen

- `opacity` Set window background opacity.

  - Default: `1.0`.

- `blur` Set blur on the window background. Changing this config requires restarting Rio to take effect.

  - Default: `false`.

- `background-image` Set an image as background.

  - Default: `None`

- `decorations` - Set window decorations
  - `Enabled` (default) enable window decorations.
  - `Disabled` disable all window decorations.
  - `Transparent` window decorations with transparency.
  - `Buttonless` remove buttons from window decorations.

Example:

```toml
[window]
width = 600
height = 400
mode = "Windowed"
opacity = 1.0
blur = false
decorations = "Enabled"
```

### Using blur and background opacity:

```toml
[window]
opacity = 0.5
blur = true
```

![Demo blur and background opacity](/assets/demos/demo-macos-blur.png)

![Demo blur and background opacity 2](/assets/demos/demos-nixos-blur.png)

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
