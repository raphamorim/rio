---
title: 'IME Support'
language: 'en'
---

# IME Support

Rio terminal provides comprehensive Input Method Editor (IME) support for multilingual text input, with intelligent cursor positioning for an enhanced user experience.

## Features

### IME Cursor Positioning

Rio automatically positions IME input popups precisely at the terminal cursor location, providing a seamless input experience for:

- **CJK Languages**: Chinese, Japanese, Korean input methods
- **Emoji Input**: System emoji picker and character viewer
- **Special Characters**: Unicode character input dialogs
- **Accent Input**: Dead key combinations and accent menus

### Automatic Position Updates

The IME cursor position is automatically updated whenever the cursor moves through:

- Keyboard input and navigation
- Mouse clicks and selection
- Vi mode cursor movements
- Terminal escape sequences
- Scrolling and window operations

### Platform Support

IME cursor positioning is supported on:

- **macOS**: Full support with native Input Method Kit integration
- **Linux**: Support via X11 and Wayland input methods
- **Windows**: Support via Windows IME framework

## Configuration

IME cursor positioning can be configured in your `config.toml`:

```toml
[keyboard]
ime-cursor-positioning = true  # Enable IME cursor positioning (default)
```

### Options

- `true` (default): IME popups appear at the cursor position
- `false`: Use system default IME positioning behavior

## Benefits

### Enhanced User Experience

- **Precise Positioning**: IME popups appear exactly where you're typing
- **Visual Continuity**: No disconnect between cursor and input location
- **Reduced Eye Movement**: Input context stays in your field of view

### Multilingual Support

- **CJK Input**: Improved experience for Chinese, Japanese, Korean
- **Emoji Integration**: Seamless emoji picker positioning
- **Unicode Support**: Better handling of special character input

### Performance Optimized

- **Smart Throttling**: Only updates when cursor position changes significantly
- **Validation**: Prevents invalid coordinates that could cause system errors
- **Efficient Updates**: Minimal overhead with render-based positioning

## Technical Details

### Implementation

Rio uses a render-based approach for IME cursor positioning:

1. **Coordinate Calculation**: Converts terminal grid position to pixel coordinates
2. **Validation**: Ensures coordinates are valid before updating
3. **Throttling**: Prevents unnecessary updates for minimal position changes
4. **Platform Integration**: Uses native IME APIs for each operating system

### Error Handling

- Validates cell dimensions and coordinates
- Graceful fallback for invalid positions
- Comprehensive logging for debugging
- Safe defaults when positioning fails

## Troubleshooting

### IME Not Positioning Correctly

1. Ensure `ime-cursor-positioning = true` in your config
2. Check that your system IME is properly configured
3. Verify Rio has necessary permissions for input method access

### Performance Issues

If you experience performance issues with IME positioning:

```toml
[keyboard]
ime-cursor-positioning = false  # Disable to use system default
```

### Platform-Specific Notes

**macOS**: Requires Input Method Kit permissions
**Linux**: May require specific input method configuration
**Windows**: Works with Windows IME framework

## Related Configuration

- [Keyboard Configuration](/docs/config#keyboard)
- [Vi Mode](/docs/features/vi-mode)
- [Navigation](/docs/config#navigation)