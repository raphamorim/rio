---
title: 'Wide Color Gamut Support'
language: 'en'
---

Rio terminal supports wide color gamut displays, allowing you to take advantage of the expanded color range available on modern displays.

## What is Wide Color Gamut?

Wide color gamut refers to displays that can reproduce a larger range of colors than traditional sRGB displays. This includes:

- **Display P3**: Used in modern Apple devices, offering about 25% more colors than sRGB
- **Rec. 2020**: An even wider color space used in HDR content, offering significantly more colors

## Configuration

You can configure the colorspace in your Rio configuration file:

```toml
[window]
colorspace = "display-p3"
```

### Available Options

- `srgb` - Standard sRGB colorspace (default on non-macOS platforms)
- `display-p3` - Display P3 wide color gamut (default on macOS)
- `rec2020` - Rec. 2020 ultra-wide color gamut

## Platform Support

### macOS

Wide color gamut support is fully implemented on macOS, where Rio automatically defaults to Display P3 colorspace on compatible displays. This takes advantage of the P3 displays found in:

- MacBook Pro (2016 and later)
- iMac (2017 and later)
- iMac Pro
- Pro Display XDR
- Studio Display

### Other Platforms

On Linux and Windows, the colorspace setting is available but may have limited effect depending on the display and graphics drivers. Rio will attempt to configure the appropriate colorspace but falls back to sRGB if wide color gamut is not supported.

## Benefits

When using a wide color gamut display with appropriate colorspace configuration:

- More vibrant and accurate colors in terminal output
- Better color reproduction for images displayed via iTerm2 image protocol or Sixel
- Improved visual experience when using colorful themes and syntax highlighting

## Technical Details

Rio implements wide color gamut support by:

1. Configuring the window's colorspace at the platform level
2. Setting up the appropriate WGPU surface format for the selected colorspace
3. Ensuring proper color space handling throughout the rendering pipeline

The implementation automatically handles the differences between colorspaces, ensuring that colors are displayed correctly regardless of the selected option.