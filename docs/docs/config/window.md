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
  - `Enabled` (default for Windows/Linux/BSD) enable window decorations.
  - `Disabled` disable all window decorations.
  - `Transparent` (default for MacOS) window decorations with transparency.
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
decorations = "enabled"
blur = true
```

![Demo blur and background opacity](/assets/demos/demo-macos-blur.png)

![Demo blur and background opacity 2](/assets/demos/demos-nixos-blur.png)

### Using image as background:

If both properties `width` and `height` are occluded then background image will use the terminal width/height.

```toml
[window.background-image]
path = "/Users/hugoamor/Desktop/musashi.png"
opacity = 0.5
x = 0.0
y = -100.0
```

![Demo image as background](/assets/demos/demo-background-image.png)

If any property `width` or `height` are used then background image will be respected.

```toml
[window.background-image]
path = "/Users/hugoamor/Desktop/harvest-moon.png"
width = 1200
height = 800
opacity = 0.5
x = 0.0
y = 0.0
```

![Demo image as background](/assets/demos/demo-background-image-partial.png)

### MacOS: Unified titlebar

You can use MacOS unified titlebar by config, it's disabled by default.

```toml
[window]
macos-use-unified-titlebar = false
```

![Demo unified titlebar](/assets/demos/demo-macos-unified-titlebar.png)