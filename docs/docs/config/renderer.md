---
title: 'renderer'
language: 'en'
---

## Performance

- `Performance` - Set WGPU rendering performance

  - `High`: Adapter that has the highest performance. This is often a discrete GPU.
  - `Low`: Adapter that uses the least possible power. This is often an integrated GPU.

```toml
[renderer]
performance = "High"
```

## Backend

- `Backend` - Set WGPU rendering backend

  - `Automatic`: Leave Sugarloaf/WGPU to decide
  - `GL`: Supported on Linux/Android, and Windows and macOS/iOS via ANGLE
  - `Vulkan`: Supported on Windows, Linux/Android
  - `DX12`: Supported on Windows 10
  - `Metal`: Supported on macOS/iOS

```toml
[renderer]
backend = "Automatic"
```

## Disable unfocused render

This property disable renderer processes while Rio is unfocused.

Default is false.

```toml
[renderer]
disable-unfocused-render = false
```

## Target FPS

This configuration is disabled by default but if isLimits the maximum number of frames per second that rio terminal will attempt to draw on a specific frame per second interval.

```toml
[renderer]
target-fps = 120
```

## Filter

Rio allow to configure filters based on RetroArch shaders: [github.com/libretro/slang-shaders](https://github.com/libretro/slang-shaders).

Note: Filters does not work with `GL` backend.

```toml
[renderer]
filters = [
  "/Users/raphael/Downloads/slang-shaders-master/crt/newpixie-crt.slangp"
]
```

![Demo shaders 2](/assets/features/demo-retroarch-2.png) 