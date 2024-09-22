---
title: 'renderer'
language: 'en'
---

- `Performance` - Set WGPU rendering performance

  - `High`: Adapter that has the highest performance. This is often a discrete GPU.
  - `Low`: Adapter that uses the least possible power. This is often an integrated GPU.

- `Backend` - Set WGPU rendering backend

  - `Automatic`: Leave Sugarloaf/WGPU to decide
  - `GL`: Supported on Linux/Android, and Windows and macOS/iOS via ANGLE
  - `Vulkan`: Supported on Windows, Linux/Android
  - `DX12`: Supported on Windows 10
  - `Metal`: Supported on macOS/iOS

- `disable-unfocused-render` - This property disable renderer processes while Rio is unfocused.

- `frame-interval` - Time scheduler between frames in milliseconds per second that rio terminal will attempt to draw. If you set as `0` then this value will be ignored. The default on MacOS/Windows is 1 and all other platforms is 3.

In case you would like define 60 frames per second as target, you would need to set each frame as 1/60th of one second long, so 16.67 milliseconds.

Example:

```toml
[renderer]
performance = "High"
backend = "Automatic"
disable-unfocused-render = false
frame-interval = 1
```
