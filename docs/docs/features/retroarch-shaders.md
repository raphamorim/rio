---
title: 'RetroArch shaders'
language: 'en'
---

Rio allow to configure filters based on RetroArch shaders: [github.com/libretro/slang-shaders](https://github.com/libretro/slang-shaders).

```toml
[renderer]
filters = [
  # load builtin filter
  "newpixiecrt",
  
  # or load your own filter
  "/Users/raphael/Downloads/slang-shaders-master/crt/newpixie-crt.slangp"
]
```

## Requirements

The filter chain is implemented on top of [librashader](https://github.com/SnowflakePowered/librashader) and only runs on the `wgpu` backend. It is gated behind the `wgpu` Cargo feature.

- **Windows / WebAssembly** — `wgpu` is enabled by default; filters work out of the box.
- **macOS / Linux** — the default builds use the native Metal / Vulkan backends, which do not include the librashader filter chain. To use filters, build with the feature enabled and select a `wgpu`-backed renderer in your config:

  ```sh
  cargo build --release --features wgpu
  ```

  ```toml
  [renderer]
  # macOS: route through the wgpu Metal translation layer
  backend = "WgpuMetal"
  # Linux: pick a wgpu backend, e.g.
  # backend = "GL"
  ```

If `filters` is set but the active backend is one of the native backends (`Metal` on macOS, `Vulkan` on Linux), the configuration is accepted but the filter chain is not applied.

![Demo shaders](/assets/features/demo-retroarch-1.png)

![Demo shaders 2](/assets/features/demo-retroarch-2.png)

