---
title: 'Wide Color Gamut Support'
language: 'en'
---

Rio renders onto a wide-gamut surface on macOS (Display P3) so themes that use vivid, saturated colors can reach more of the display's range. The `[window] colorspace` setting controls how Rio *interprets* the color bytes in your config and in ANSI / direct-color escape sequences — it does **not** change the surface.

## Configuration

```toml
[window]
colorspace = "srgb"
```

### Available options

- `srgb` — **default.** Interpret hex values (`#ff0000`) and ANSI direct colors as sRGB. Rio converts them to Display P3 primaries before rendering, so `#ff0000` displays as the same red other apps draw (no oversaturation).
- `display-p3` — Interpret input values as already being in Display P3 primaries. A pure-red theme color will reach the full P3 red gamut and look more saturated than the sRGB standard. Use this if your theme was designed with a P3-aware color picker.
- `rec2020` — Interpret input values as Rec. 2020. Treated like `display-p3` for now; a proper Rec. 2020 → Display P3 matrix is planned.

## Platform support

### macOS

On macOS, Rio always uses a DisplayP3-tagged `CAMetalLayer` with a gamma-correct (`BGRA8Unorm_sRGB`) framebuffer, regardless of the setting above. This yields linear-light alpha blending (no dark halos around text) and access to the wider-gamut range. The colorspace config only affects color *interpretation*.

Compatible wide-gamut displays include:

- MacBook Pro (2016 and later)
- iMac (2017 and later)
- iMac Pro
- Pro Display XDR
- Studio Display

### Linux / Windows

The wgpu path does not yet implement the linear-light + wide-gamut pipeline. The config value is accepted but most platforms effectively behave as `srgb` until that path is updated.

## Matching other terminals

Rio's default (`srgb`) matches [ghostty's `window-colorspace` default](https://ghostty.org/docs/config/reference#window-colorspace) — meaning a given hex value renders identically in both terminals. If you want the old Rio macOS behaviour (colors treated as P3, looking more saturated), set:

```toml
[window]
colorspace = "display-p3"
```

## Technical details

The Metal path (`sugarloaf/src/renderer/renderer.metal`):

1. Linearizes sRGB-encoded input RGB (fragment side) with the IEC 61966-2-1 transfer curve.
2. If `input_colorspace == 0` (sRGB), applies a Bradford-adapted sRGB D65 → Display P3 D65 primaries matrix in linear light.
3. Writes the result to the `_sRGB` DisplayP3 drawable, so the hardware sRGB-encodes on write and alpha blends subsequent pixels in linear light.

The clear color (`MTLClearColor`) goes through the same transform on the Rust side (`sugarloaf::prepare_output_rgb_f64`) so the first cleared pixel lands in the same colorspace as shader-emitted pixels.
