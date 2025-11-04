---
title: 'Changelog'
language: 'en'
---

# Changelog

## 0.3.0 (unreleased)

- Native Metal Support.
- Native Vulkan Support.
- Quake window support.
- Kitty image protocol.
- Breaking: `Decorations` as `Transparent` is default on MacOS (instead of `Enabled`).

## 0.2.36

- Fix DECSCUSR.

## 0.2.35

- GPU memory usage drop 83%.
- Sync input render logic (macos).

## 0.2.34

- Fix issue for finding fonts introduced with the v0.2.33 new font loader.

## 0.2.33

- **Platform-specific configuration improvements** [#1341](https://github.com/raphamorim/rio/issues/1341):
  - Added support for platform-specific environment variables via `env-vars` field in platform config
  - Platform-specific env-vars are now appended to global env-vars instead of replacing them
  - Fixed configuration inheritance: platform overrides now use field-level merging instead of replacing entire sections
  - Window, Navigation, and Renderer settings can now be partially overridden per platform without duplicating all fields
  - Added `theme` field to platform config for per-platform theme selection
  - Shell configuration continues to use complete replacement for simplicity
- Fix `ScrollPageUp` and `ScrollPageDown` actions not working in custom keybindings [#1275](https://github.com/raphamorim/rio/issues/1275).
- Fix Noticeably slower startup compared to wezterm, foot [#1346](https://github.com/raphamorim/rio/issues/1346).
- Fix Font loader taking a LOT of time to load fonts [#1339](https://github.com/raphamorim/rio/issues/1339).
- Fix Rio panics on launch on a Raspberry Pi 5 [#1332](https://github.com/raphamorim/rio/issues/1332).
- Fix kitty keyboard protocol.
- Support reporting terminal version via XTVERSION.

## 0.2.32

- Updated WGPU to v27.0.1.
- Fix No backend are enabled on FreeBSD #1235.

## 0.2.31

- Update Rust to v1.90.
- Fix kitty keyboard recognition.
- **Breaking: Simplified key binding escape sequences**
  - Replaced separate `text` and `bytes` fields with a single `esc` field
  - Escape sequences are now sent directly to the PTY without text manipulation
  - Migration: Replace `bytes = [27, 91, 72]` with `esc = "\u001b[H"`
  - Migration: Replace `text = "some text"` with `esc = "some text"`
  - Example: `{ key = "l", with = "control", esc = "\u001b[2J\u001b[H" }` to clear screen
- **Fix key binding conflicts**: Resolved issues where keys like `PageUp`, `PageDown`, and `Alt+Enter` required explicit `"None"` bindings before they could be reassigned
  - Simplified binding conflict resolution logic to automatically remove conflicting default bindings
  - User-defined bindings now always take precedence without requiring placeholder "None" entries

## 0.2.30

- **Fix Debian/Ubuntu package installation**: Resolved terminfo conflicts with system packages [#1264](https://github.com/raphamorim/rio/issues/1264)
  - Debian (.deb) packages no longer include terminfo files to avoid conflicts with ncurses-term
  - Users on Ubuntu 22.04 and older need to manually install terminfo after package installation
  - Debian 13+ and Ubuntu 24.04+ users get terminfo from system's ncurses-term package
  - RPM packages continue to include terminfo as before
- Add audible & visual bell support [#1284](https://github.com/raphamorim/rio/pull/1284).

## 0.2.29

- Fix blinking cursor issue [#1269](https://github.com/raphamorim/rio/issues/1269).
- Fix Rio uses UNC (\?\) path as working directory, breaking Neovim subprocesses on Windows.
- Add NSCameraUseContinuityCameraDeviceType to plist for macOS.

## 0.2.28

- **Optimized rendering pipeline for improved performance**: Implemented deferred damage checking and render coalescing
  - Added Wakeup events to batch multiple rapid terminal updates into single render passes
  - Deferred damage calculation until render time to reduce unnecessary computations
  - Skip rendering for unfocused windows when `disable_unfocused_render` is enabled
  - Skip rendering for occluded windows when `disable_occluded_render` is enabled
  - Improved damage merging to always accumulate updates even when already marked dirty
  - Enhanced performance for rapid terminal output by coalescing non-synchronized updates

## 0.2.27

- Breaking: If `xterm-rio` is installed we prioritized it over `rio` terminfo.
- **Fix sixel/iterm2 graphics persistence issue**: Fixed graphics remaining visible when overwritten by text
  - Graphics are now properly removed when cells containing them are overwritten
  - Fixes issues with file managers like Yazi where images would persist incorrectly
  - Simplified graphics cleanup logic by removing unused ClearSubregion functionality
- **CJK Font Metrics**: Fixed CJK characters displaying "higher" than Latin characters [#1071](https://github.com/raphamorim/rio/issues/1071)
  - Implemented comprehensive CJK font metrics handling with consistent baseline adjustment
  - Fixed scrolling issues for mixed Latin and CJK text content
  - Added CJK character width measurement using "水" (water ideograph) as reference
  - Created consistent cell dimensions across different font types
  - Developed extensive test suite with 40+ font-related tests to verify fixes

## 0.2.26

- **Fix frame dropping in release builds**: Fixed an issue where release builds would drop frames due to damage event timing
  - Damage events are now emitted directly after parsing PTY data, ensuring proper batching
  - Removed redundant Wakeup event mechanism that was causing multiple renders per update
  - Synchronized update timeouts now properly emit damage events
  - Significantly improves rendering smoothness in optimized builds

## 0.2.25

- Fix: Rio doesn't launch from context menu on Windows.
- Fix: Rio lacks embedded icon on Windows 10 by [@christianjann](https://github.com/christianjann).
- **Fix custom shells in /usr/local/bin not found on macOS**: Fixed an issue where custom shells installed in `/usr/local/bin` were not found when Rio was launched from Finder or other GUI applications
  - On macOS, Rio now uses `/usr/bin/login` to spawn shells, ensuring proper login shell environment with full PATH
  - Custom shells like Fish, Nushell, or custom Zsh installations in `/usr/local/bin` will now work correctly

## 0.2.24

- Fix game mode regression.
- **Hint Label Damage Tracking**: Improved hint label rendering performance with proper damage tracking
  - Hint label areas are now properly marked for re-rendering when cleared
  - Eliminates visual artifacts when hint labels are removed
  - Optimized rendering to only update affected screen regions
- **Configurable Hyperlink Hover Keys**: Hyperlink hover modifier keys are now configurable
  - Configure custom modifier keys through the hints system in `config.toml`
  - Default behavior unchanged: Command on macOS, Alt on other platforms
  - Supports any combination of Shift, Control, Alt, and Super/Command keys
  - Example: `mouse = { enabled = true, mods = ["Shift"] }` to use Shift key
- **Hints Configuration**: Renamed `hints.enabled` to `hints.rules` for better clarity
  - Update your configuration: `[[hints.enabled]]` → `[[hints.rules]]`
  - All hint configuration sections now use `hints.rules.*` instead of `hints.enabled.*`
  - Functionality remains the same, only the configuration key names changed

## 0.2.23

- Fix some rendering regressions introduced by 0.2.21.
- Improve performance by stopping locking on rendering run steps.
- Fix: [X11: WM_CLASS has an empty string property](https://github.com/raphamorim/rio/issues/1155).

## 0.2.22

- Fix some regressions introduced by 0.2.21.

## 0.2.21

- Breaking: `navigation.use-current-directory` has been renamed to `navigation.current-working-directory`.

### Performance Optimizations

- **Major**: Implemented efficient CVDisplayLink-based VSync synchronization for macOS
  - Perfect frame timing aligned with display hardware refresh cycles
  - Eliminates screen tearing and stuttering through hardware VSync synchronization
  - Adaptive refresh rate support: automatically handles 60Hz, 120Hz, ProMotion displays
  - Multi-display support: adapts when windows move between displays with different refresh rates
  - Grand Central Dispatch (GCD) integration for thread-safe cross-thread communication
  - **Smart rendering**: Only renders when content actually changes using dirty flag system
  - Power efficient: skips unnecessary redraws when content is static, reducing CPU usage
  - Professional rendering quality with smooth, tear-free visual updates
  - CVDisplayLink runs on dedicated background thread, never blocking UI operations
- **macOS VSync Optimization**: Disabled redundant software-based vsync calculations on macOS
  - CVDisplayLink already provides hardware-synchronized VSync timing
  - Eliminates unnecessary frame timing calculations and monitor refresh rate queries
  - Reduces CPU overhead and improves rendering performance
  - Software vsync logic remains active on other platforms for compatibility
- **Major**: Implemented a new text run caching system replacing line-based caching
  - Up to 96% reduction in text shaping overhead for repeated content
  - Individual text runs (words, operators, keywords) cached and reused across frames
  - 256-bucket hash table with LRU eviction for optimal memory usage
- **Cache Warming**: Pre-populate cache with 100+ common terminal patterns on startup
  - Programming keywords: `const`, `let`, `function`, `class`, `import`, `export`, etc.
  - Indentation patterns: 4/8/12/16 spaces, single/double/triple tabs
  - Shell commands: `ls`, `cd`, `git`, `npm`, `cargo`, `sudo`, etc.
  - Operators & punctuation: `=`, `==`, `=>`, `();`, `{}`, `[]`, etc.
  - File extensions: `.js`, `.ts`, `.rs`, `.py`, `.json`, `.md`, etc.
  - Error/log patterns: `Error:`, `[INFO]`, `FAILED`, `SUCCESS`, etc.
  - Immediate cache hits eliminate cold start shaping delays
- **SIMD-Optimized Whitespace Detection**: Multi-tier optimization for indentation processing
  - AVX2: 32 bytes per instruction (x86-64 with AVX2 support)
  - SSE2: 16 bytes per instruction (x86-64 with SSE2 support)
  - NEON: 16 bytes per instruction (ARM64/aarch64)
  - Optimized scalar: 8-byte chunks (universal fallback)
  - Up to 32x performance improvement for long indentation sequences
  - Critical for Python, nested JavaScript/TypeScript, YAML, and heavily indented code
- **Memory Pool for Vertices**: High-performance vertex buffer pooling system
  - Size-categorized pools: Small (64), Medium (256), Large (1024), XLarge (4096) vertices
  - Zero allocation overhead through buffer reuse across frames
  - LRU management with automatic cleanup when pools reach capacity
  - Thread-safe concurrent access with performance monitoring
  - Eliminates GC pressure and improves frame rate consistency
- **Background Font Operations**: Non-blocking font management
  - Font data release and cleanup in dedicated background thread
  - System font scanning and preloading without blocking main thread
  - Prevents frame rate drops during font operations
- **Occlusion-Based Rendering**: Skip rendering for occluded windows/tabs
  - Automatically detects when windows are completely hidden by other windows
  - Skips rendering for occluded windows to save GPU resources and improve performance
  - Renders one frame when window becomes visible again to ensure display is updated
  - Configurable via `[renderer] disable-occluded-render = true` (enabled by default)
  - Significantly improves performance when running multiple tabs or windows

### Other Improvements

- Optimize the character cluster cache for wide space characters.
- New font atlas, more efficient.
- Implemented around 75% Memory Reduction: Text glyphs now use R8 (1 byte) instead of RGBA (4 bytes).
- **Hint Label Damage Tracking**: Improved hint label rendering performance with proper damage tracking
  - Hint label areas are now properly marked for re-rendering when cleared
  - Eliminates visual artifacts when hint labels are removed
  - Optimized rendering to only update affected screen regions
- **IME Cursor Positioning**: Added configurable IME cursor positioning based on terminal cell coordinates
  - IME input popups now appear precisely at the cursor position
  - Improves input experience for CJK languages (Chinese, Japanese, Korean)
  - Configurable via `[keyboard] ime-cursor-positioning = true` (enabled by default)
- **Shift+Click Selection**: Added Shift+click support for expanding text selections
  - Shift+clicking now extends the current selection to the clicked cell
  - Provides standard terminal selection behavior expected by users
  - Regular clicking without Shift still clears selection and starts new one as before
- **CLI accepts relative paths for working directory CLI argument**: When invoking rio from other terminals using `rio --working-dir=<path>`, a relative path is now correctly processed

### Bug Fixes

- **Cursor Damage Tracking**: Fixed cursor rendering issues after `clear` command and during rapid typing
  - Replaced complex point-based damage tracking with simplified line-based approach
  - Eliminates edge cases where cursor updates were missed during fast typing sequences
  - Improved reliability by always damaging entire lines instead of tracking column ranges
  - Aligns with modern terminal design principles for more robust damage calculation
- **Selection Rendering**: Fixed selection highlight not appearing on first render
  - Selection changes now properly trigger damage tracking and rendering
  - Optimized selection damage to only redraw affected lines for better performance
  - Selection highlights now appear immediately when making selections
- **Text Selection**: Fixed selection behavior during input and paste operations
  - Selection properly clears when typing or pasting text (both bracketed and regular paste)
  - Selection coordinates remain stable during viewport scrolling
  - Prevents selection from being lost unexpectedly during normal terminal usage
- **Auto-scroll on Input**: Fixed issue where typing after scrolling up wouldn't automatically scroll to bottom
  - Now properly scrolls to bottom for both keyboard input and IME/paste operations
  - Ensures cursor remains visible when typing new content
- **Scroll Performance**: Improved scrolling performance by optimizing render event handling
  - Moved scroll display offset update before mouse cursor dirty event
  - Removed redundant render calls during scroll operations
  - Implemented centralized damage-based rendering in event loop for better performance
- **macOS IME Improvements**: Fixed emoji input and IME stability issues
  - Resolved `IMKCFRunLoopWakeUpReliable` errors when using emoji picker
  - Improved coordinate validation and error handling for IME positioning
  - Better handling of direct Unicode input (emoji picker, character viewer)
  - Added throttling to prevent excessive IME coordinate updates
- **Documentation**: Added comprehensive manual pages (man pages) for Unix-like systems
  - `man rio` - Main Rio terminal manual page with command-line options
  - `man 5 rio` - Complete configuration file format documentation
  - `man 5 rio-bindings` - Key bindings reference and customization guide
  - Available in `extra/man/` directory with build instructions
- **Terminfo Compatibility**: Improved terminal compatibility by adding `xterm-rio` terminfo entry
  - Added `xterm-rio` as primary terminfo entry with `rio` as alias for better application compatibility
  - Applications that look for "xterm-" prefixed terminals (like termwiz-based apps) now work correctly
  - Maintains `TERM=rio` environment variable for consistency with terminal identity
  - Fixes crashes with applications like `gitu` and other termwiz-based terminal programs
  - Follows same pattern as other modern terminals (Alacritty, Ghostty) for maximum compatibility

### Technical Details

The performance optimizations in this release represent a significant architectural improvement to Rio's text rendering pipeline:

- **Text Run Caching**: Replaces line-based caching with individual text run caching. Each unique text sequence (word, operator, keyword) is shaped once and reused across all occurrences.
- **SIMD Implementation**: Platform-adaptive SIMD instructions automatically detect and use the best available CPU features (AVX2 > SSE2 > NEON > optimized scalar) for maximum performance across different architectures.
- **Memory Management**: The vertex pool system uses size-categorized buffers with LRU eviction, eliminating allocation overhead while preventing memory bloat.
- **Cache Strategy**: Two-level caching (render data + text runs) with 256-bucket hash table using FxHasher for optimal lookup performance.
- **Compatibility**: All optimizations maintain full backward compatibility with existing Rio APIs and configurations.

These changes are particularly beneficial for:

- Programming workflows with repetitive code patterns
- Terminal sessions with heavy indentation (Python, nested JS/TS, YAML)
- Long-running sessions where cache warming provides sustained performance benefits
- Systems with limited memory where reduced allocation overhead improves overall responsiveness

### Bug Fixes

- **Backspace Key Compatibility**: Fixed backspace key not working properly in vim when `TERM=xterm-256color`
  - Changed backspace key bindings to send BS (0x08) instead of DEL (0x7F)
  - Updated Rio terminfo and termcap entries to match actual key behavior
  - Updated XTGETTCAP response to return `^H` for `kbs` capability
  - Ensures compatibility with applications expecting xterm-256color backspace behavior
  - Fixes issue where vim would display `^?` instead of performing backspace operation

## 0.2.20

- Performance: Implemented SIMD-accelerated UTF-8 validation throughout Rio terminal using the `simdutf8` crate.
  - Architecture support: AVX2/SSE4.2 (x86-64), NEON (ARM64), SIMD128 (WASM)
  - Automatic optimization: Runtime detection selects fastest implementation available
- Support for XTGETTCAP (XTerm Get Termcap) escape sequence for querying terminal capabilities.
- Font library is now under a RWLock instead of Mutex to allow multiple tabs readings same font data.
- Fix: crash on openSUSE Tumbleweed [#1160](https://github.com/raphamorim/rio/issues/1160).

## 0.2.19

- Reduced the bundle size by ~20.81% (MacOS, Linux, BSD).
- Performance: stop saving empty images in the image cache.
- Fix: On MacOS, keybind definition to ignore cmd-w does not work [#879](https://github.com/raphamorim/rio/issues/879).
- Fix: Build for MacOS 26 Tahoe.
- Fix: `Enter`,`Tab`, `Backspace` not disambiguated with `shift` in kitty keyboard's disambiguate mode.
- Fix: line-height adds small gaps for box-drawing characters [#1126](https://github.com/raphamorim/rio/issues/1126).
- Search matching a wrapping fullwidth character in the last column.
- Update Rust to 1.87.0.

## 0.2.18

- Fix image display crashing the application whenever f16 is available.

## 0.2.17

- _Breaking:_ Decorations as `Enabled` is default on MacOS (instead of `Transparent`).
- F16 Texture supports whenever is available.
- Clear font atlas whenever the font is changed.
- Skip passing sandbox env in Flatpak, fixes user environment in spawned shell [#1116](https://github.com/raphamorim/rio/pull/1116) by [@ranisalt](https://github.com/ranisalt).
- On Windows, fixed crash in should_apps_use_dark_mode() for Windows versions < 17763.

## 0.2.16

- _Breaking_: support reading from config directory using `$XDG_CONFIG_HOME` on Linux [#1105](https://github.com/raphamorim/rio/pull/1105) by [@ranisalt](https://github.com/ranisalt).
- Fix: Crash on whenever attempting to clean an invalid line index.
- Add metainfo and screenshots for appstream by [@ranisalt](https://github.com/ranisalt).

## 0.2.15

- Fix: In some cases, the first typed character doesn't display until after a delay, or until another key is hit [#1098](https://github.com/raphamorim/rio/issues/1098).
- Fix: Anomalous behavior occurs with the Bookmark tab style in the new versions 0.14 and 0.13. [#1094](https://github.com/raphamorim/rio/issues/1094).

## 0.2.14

- Fix: panic and crash of terminal window during sudo apt update [#1093](https://github.com/raphamorim/rio/issues/1093).

## 0.2.13

- _Breaking change_: For Windows and Linux users, hyperlink trigger whenever hovering a link was changed from `alt` to `shift`.
- Fix dimension for whenever a new tab is created from a view with splits.
- Drop subtables with empty coverage by [@xorgy](https://github.com/xorgy).
- Fix font size affecting tabs size.
- Support to drawable characters by using `fonts.use-drawable-chars = true`.
- Fix: Wrong unicode character alignment [#616](https://github.com/raphamorim/rio/issues/616).
- Fix: Built-in font for box drawing #974 [#974](https://github.com/raphamorim/rio/issues/974).
- Fix: U+E0B6 and U+E0B4 Unicode with different sizes [#895](https://github.com/raphamorim/rio/issues/895).
- Update wgpu to v25.
- Fix: Custom rendering (alignment) of Braille symbols [#1057](https://github.com/raphamorim/rio/issues/1057).
- Fix: Drawing char ⡿ in column 1 causes the entire terminal to stutter [#1033](https://github.com/raphamorim/rio/issues/1033).
- Fix: Some glyphs (e.g. braille symbol) are rendered with gaps in between [#930](https://github.com/raphamorim/rio/issues/930).
- Introduce `fonts.disable-warnings-not-found` to disable warning regarding fonts not found.
- Fix: Request: silently ignore missing fonts from fonts.family and fonts.family.extras [#1031](https://github.com/raphamorim/rio/issues/1031).
- Fix: Add branch drawing symbols to box characters [#761](https://github.com/raphamorim/rio/issues/761).
- Fix: macOS: fallback for missing font glyph? [#913](https://github.com/raphamorim/rio/issues/913).
- Fix: FPS calculation, before it was rendering avg 48 on 60fps screen, however it was due to wrong frame scheduling computations, now it's up to 56-58.
- Fix: Shift+Tab event is doubled, as if hit twice [#1061](https://github.com/raphamorim/rio/issues/1061).
- Fix: Request: Option to change click-link modifier key [#1059](https://github.com/raphamorim/rio/issues/1059).
- Fix: Unexpected tmux previous-window [#1062](https://github.com/raphamorim/rio/issues/1062).
- Rewrite the way Rio deals with line diff and updates computation.
- Support for setting a custom config directory using `$RIO_CONFIG_HOME`
- Support for additional font dirs using `fonts.additional-dirs`
- Rio's MSRV is 1.85.0.
- Support to Sextants.
- Fix: Octant support [#814](https://github.com/raphamorim/rio/issues/814).
- Fix: Issue regarding split not updating opacity style when getting unfocused.
- Add support for custom parsing of APC, SOS and PM sequences.

## 0.2.12

- Fix crash regarding fonts not found whenever trying to run Rio.

## 0.2.11

- Fix filter scanlines not appearing.
- rt(wgpu): clamp texture size to device limits by [@chyyran](https://github.com/chyyran).
- Support to builtin filters: `newpixiecrt` and `fubax_vr`.
- Fix dimension computation whenever resizing Rio.
- Removed `fonts.ui` property, now Rio will always use primary font for UI.
- Removed Text renderer mod by migrating to RichText renderer.
- _Breaking:_ `renderer.strategy = "Continuous"` was renamed to `renderer.strategy = "Game"`
- Fix search bar can't show chinese [#844](https://github.com/raphamorim/rio/issues/844).

## 0.2.10

- Fix computation of lines on screen.
- Fix dimension of the first tab whenever TopTab or BottomTab is created.
- Fix flaky test issue, test_update_title_with_logical_or failing randomly on aarch64 [#994](https://github.com/raphamorim/rio/issues/994).
- Support to `navigation.unfocused_split_opacity`, default is `0.5`.
- Sugarloaf: Fix foreground color opacity not being computed.

## 0.2.9

- Support to symbol map configuration: `fonts.symbol-map`:

```toml
# covers: '⊗','⊘','⊙'
fonts.symbol-map = [{ start = "2297", end = "2299", font-family = "Cascadia Code NF" }]
```

- Add Switch to Next/Prev Split or Tab command by [@vlabo](https://github.com/vlabo).
- Fix issue whenever the first main font cannot be found.

## 0.2.8

- Support to `.rpm` files! (thanks [@vedantmgoyal9](https://github.com/vedantmgoyal9) and [@caarlos0](https://github.com/caarlos0))
- OSC 7 Escape sequences to advise the terminal of the working directory.
- Use [GoReleaser](https://goreleaser.com) to build & release Rio ([#921](https://github.com/raphamorim/rio/pull/921)), thanks [@caarlos0](https://github.com/caarlos0) and [@vedantmgoyal9](https://github.com/vedantmgoyal9)
- Cache GSUB and GPOS features independently.
- Updated `windows-sys` to `v0.59`.
  - To match the corresponding changes in `windows-sys`, the `HWND`, `HMONITOR`, and `HMENU` types now alias to `*mut c_void` instead of `isize`.

## 0.2.7

- Shifted key reported without a shift when using kitty keyboard protocol.
- fix: Set cursor color via ANSI escape sequence [#945](https://github.com/raphamorim/rio/issues/945).
- fix: Can the "base 16" colors be changed at runtime through Ansi escape sequences? [#188](https://github.com/raphamorim/rio/issues/188)
- fix: Changing release and nightly build Ubuntu runners for x86 (`ubuntu-latest` to `ubuntu-22.04`) and arm (`ubuntu-24.04-arm` to `ubuntu-22.04-arm`)

## 0.2.6

- Fix: 0.2.5 doesn't render grey scale font on macOS [#937](https://github.com/raphamorim/rio/issues/937).
- fix: fix duplicate tab_id by monotonic counter for unique tab IDs by [@hilaolu](https://github.com/hilaolu).
- Add backslash to invalid characters for URL regex.
- fix regression introduced by 0.2.5 on light colors.
- fix: CMD+W open new tab but not new window occasionally [#756](https://github.com/raphamorim/rio/issues/756).
- fix: Error getting window dimensions on Wayland [#768](https://github.com/raphamorim/rio/issues/768).

## 0.2.5

- Introduced `draw-bold-text-with-light-colors` config, default is `false`.
- If light or dark colors are not specified Rio will try to convert it based on the regular color.
- Fix: Block writing to the shell when rendering the `Assistant` route.
- Fix: Immediately render the `Terminal` route when switching from the `Assistant`, `ConfirmToQuit` or `Welcome`, thus avoiding the need to double press `Enter`.
- Fix: MacOS Unable to type Option + Number for special characters [#916](https://github.com/raphamorim/rio/issues/916).
- Fix: Looking forward to having a color converter [#850](https://github.com/raphamorim/rio/issues/850).
- Fix: Unexpected basic 16 terminal colors displayed on some apps [#464](https://github.com/raphamorim/rio/issues/464).

## 0.2.4

- Breaking: Rio now doesn't allow anymore disable kitty keyboard protocol.
- Fullwidth semantic escape characters.
- Fix: report of Enter/Tab/Backspace in kitty keyboard.
- Fix: use-kitty-keyboard-protocol = true doesn't work with tmux [#599](https://github.com/raphamorim/rio/issues/599).
- Fix: use-kitty-keyboard-protocol breaks F[5-12] on macOS [#904](https://github.com/raphamorim/rio/issues/904).
- Downgrade MSRV to 1.80.1
- Update wgpu to 24.0.0.

## 0.2.3

- Rio now allows you to configure window title through configuration via template. Possible options:
  - `TITLE`: terminal title via OSC sequences for setting terminal title
  - `PROGRAM`: (e.g `fish`, `zsh`, `bash`, `vim`, etc...)
  - `ABSOLUTE_PATH`: (e.g `/Users/rapha/Documents/a/rio`)
  <!-- - `CANONICAL_PATH`: (e.g `.../Documents/a/rio`, `~/Documents/a`) -->
  - `COLUMNS`: current columns
  - `LINES`: current lines
    - So, for example if you have: `{{COLUMNS}}x{{LINES}}` would show something like `88x66`.
- Perf improvement on text selection [#898](https://github.com/raphamorim/rio/pull/898) by [@marc2332](https://github.com/marc2332).
- Window title is now updated regardless the Navigation Mode.
- Performance: Background and foreground data are only retrieved if is asked (either color automation is enabled or `window.title` contains any request for it).
- Fixed: Nix build [#853](https://github.com/raphamorim/rio/pull/853).
- Support to `window.macos-use-shadow` (enable or disable shadow on MacOS).
- Support to `window.windows-corner-preference` (options: `Default`, `DoNotRound`,`Round` and `RoundSmall`).
- Support to `window.windows-use-undecorated-shadow` (default is enabled).
- Support to `window.windows-use-no-redirection-bitmap` (This sets `WS_EX_NOREDIRECTIONBITMAP`).
- Minimal stable rust version 1.84.0.
- Support for Unicode 16 characters.
- Support to line height.
- Renamed `--title` to `--title-placeholder` on CLI.
- Fixed: Deb package name 'rio' conflicts with existing one in Ubuntu [#876](https://github.com/raphamorim/rio/issues/876).
- Fixed: Unremovable bottom padding when using line-height [#449](https://github.com/raphamorim/rio/issues/449).
- On macOS, fixed undocumented cursors (e.g. zoom, resize, help) always appearing to be invalid and falling back to the default cursor.
- Introduce `SwitchCurrentTabToPrev` and `SwitchCurrentTabToNext` actions [#854](https://github.com/raphamorim/rio/pull/854/files) by [@agjini](https://github.com/agjini).
- On X11, Wayland, Windows and macOS, improved scancode conversions for more obscure key codes.
  - On macOS, fixed the scancode conversion for audio volume keys.
  - On macOS, fixed the scancode conversion for `IntlBackslash`.
- Kitty keyboard protocol is now enabled by default.
- Allow `Renderer` to be configured cross-platform by `Platform` property.
- Add `ToggleFullscreen` to configurable actions.
- Escape sequence to move cursor forward tabs ( CSI Ps I ).
- Always emit `1` for the first parameter when having modifiers in kitty keyboard protocol.
- Microsoft Windows: fix the event loop not waking on accessibility requests.
- Wayland: disable title text drawn with crossfont crate, use ab_glyph crate instead.
- Sugarloaf: Expose wgpu.

## 0.2.2

- Fix iterm2 image protocol.
- Allow setting initial window title [#806](https://github.com/raphamorim/rio/pull/806) by [@xsadia](https://github.com/xsadia).
- Fix runtime error after changing to a specific retroarch shader on windows [#788](https://github.com/raphamorim/rio/issues/788) by [@chyyran](https://github.com/chyyran).
- Makes editor.args and shell.args optional in config.toml [#801](https://github.com/raphamorim/rio/pull/803) by [@Nylme](https://github.com/Nylme).
- Introduce `navigation.open-config-with-split`.

## 0.2.1

- Fix: Search seems broken in 0.2.0 [#785](https://github.com/raphamorim/rio/issues/785).
- Regular font is now 400 as default weight.
- Support to chooseing font width [#507](https://github.com/raphamorim/rio/issues/507).
- Support to multiconfiguration. Rio now allows you to have different configurations per OS, you can write ovewrite `Shell`, `Navigation` and `Window`.

Example:

```toml
[shell]
# default (in this case will be used only on MacOS)
program = "/bin/fish"
args = ["--login"]

[platform]
# Microsoft Windows overwrite
windows.shell.program = "pwsh"
windows.shell.args = ["-l"]

# Linux overwrite
linux.shell.program = "tmux"
linux.shell.args = ["new-session", "-c", "/var/www"]
```

- Fix: Grey triangle in the titlebar [#778](https://github.com/raphamorim/rio/issues/778)
- Update window title straight away ([#779](https://github.com/raphamorim/rio/pull/779) by [@hunger](https://github.com/hunger))
- Always update the title on windows and MacOS ([#780](https://github.com/raphamorim/rio/pull/780) by [@hunger](https://github.com/hunger))

## 0.2.0

- Note: The migration from 0.1.x to v0.2.x changed considerably the renderer source code, although it was tested for 3 weeks it's entirely possible that introduced bugs (hopefully not!).
- Performance gains!
  - Sugarloaf: Major rewrite of font glyph logic.
  - Sugarloaf: Removal of some unnecessary processing on shaping logic.
  - Sugarloaf: Rewrite/Change of render architecture, now sugarloaf does not have any reference to column/lines logic.
- _Breaking:_ Minimum MacOS version went from El Captain to Big Sur on ARM64 and Catalina on Intel x86.
- Microsoft Windows: [Rio terminal is now available on WinGet packages](https://github.com/microsoft/winget-pkgs/pull/184792).
- Microsoft Windows: [Rio terminal is now available on MINGW packages](https://github.com/msys2/MINGW-packages/pull/22248).
- Microsoft Windows: Rio support on ARM architecture by [@andreban](https://github.com/andreban).
- Allow MacOS automation via events.
- MacOS: Support titlebar unified: `window.macos-use-unified-titlebar = false`,
- Support disable font hinting: `fonts.hinting = false`.
- Fix: Configuration updates triggered multiple times on one save.
- Support to RetroArch shaders [@igorsaux](https://github.com/igorsaux).
- Fix: Set notepad as a default editor on Windows by [@igorsaux](https://github.com/igorsaux).
- Increased Linux font fallbacks list.
- Early initial split support (this feature is not yet stable).
- Fix: Preserve current working directory when opening new tabs [#725](https://github.com/raphamorim/rio/issues/725).
- Added `SplitDown`, `SplitRight`, `CloseSplitOrTab`, `SelectNextSplit` and `SelectPrevSplit` actions.
- Fix: Window doesn't receive mouse events on Windows 11 by [@igorsaux](https://github.com/igorsaux).
- Support to hex RGBA (example: `#43ff64d9`) on colors/theme by [@bio](https://github.com/bio) on [#696](https://github.com/raphamorim/rio/pull/696).
- Introduced `renderer.strategy`, options are `Events` and `Continuous`.
- Microsoft Windows: make `ControlFlow::WaitUntil` work more precisely using `CREATE_WAITABLE_TIMER_HIGH_RESOLUTION`.
- Fix: Window output lost when rio loses focus [#706](https://github.com/raphamorim/rio/issues/706).
- Updated wgpu to `23.0.0`.

## 0.1.17

- Fix flash of white during startup on Microsoft Windows [#640](https://github.com/raphamorim/rio/issues/640).
- Add DWMWA_CLOAK support on Microsoft Windows.
- VI Mode now supports search by [@orhun](https://github.com/orhun).
- Use max frame per seconds based on the current monitor refresh rate.
- _breaking_ `renderer.max-fps` has been changed to `renderer.target-fps`.
- Fix background color for underline and beam cursors when using transparent window.
- Fix IME color for underline and beam cursors.
- Add default for Style property on Sugarloaf font.

## 0.1.16

- Support auto bold on fonts.
- Support auto italic on fonts.
- Reduced default regular weight to 300 instead of 400.
- MacOS: Add dock menu.
- MacOS: Add Shell and Edit menu.
- MacOS: Support to native modal that asks if wants to close app.
- MacOS: Fix `confirm-before-quit` property.

## 0.1.15

- Introduce `cursor.blinking-interval`, default value is 800ms.
- Fix blinking cursor lag issue.
- performance: Use `Vec` (std based) instead of ArrayVec for copa.
- Fix adaptive theme background color on macos.
- Decorations as `Transparent` is default on MacOS.
- Navigation mode as `NativeTab` is default on MacOS.
- `keyboard.use-kitty-keyboard-protocol` is now `false` by default.
- Add support for msys2/mingw builds release [#635](https://github.com/raphamorim/rio/issues/635) by [@Kreijstal](https://github.com/Kreijstal).

## 0.1.14

- `developer.log-file` has been renamed to `developer.enable-log-file`.
- **breaking**: `CollapsedTab` has been renamed to `Bookmark`.
- Memory usage reduced by 75% (avg ~201mb to 48mb on first screen render).
- Implemented font data deallocator.
- Reduced font atlas buffer size to `1024`.
- Added lifetimes to application level (allowing to deallocate window structs once is removed).
- Migrated font context from `RwLock` to `Arc<FairMutex>`.
- MacOS does not clear with background operation anymore, instead it relies on window background.
- Background color has changed to `#0F0D0E`.
- Fix font emoji width.
- Fix MacOS tabbing when spawned from a new window.

## 0.1.13

- Support to iTerm2 image protocol.
- Fix: Issue building rio for Void Linux [#656](https://github.com/raphamorim/rio/issues/656).
- Fix: Adaptive theme doesn't appear to work correctly on macOS [#660](https://github.com/raphamorim/rio/issues/660).
- Fix: Image background support to OpenGL targets.
- Fix: Unable to render images with sixel protocol & ratatui-image [#639](https://github.com/raphamorim/rio/issues/639).
- Implement LRU to cache on layout and draw methods.
- Reenable set subtitle on MacOS native tabs.

## 0.1.12

- Introduce: `renderer.max-fps`.
- Fix: Cursor making text with ligatures hidden.
- Fix: Underline cursor not working.
- Fix: sixel: Text doesn't overwrite sixels [#636](https://github.com/raphamorim/rio/issues/636).
- Initial support to Sixel protocol.
- Support to `fonts.emoji`. You can also specify which emoji font you would like to use, by default will be loaded a built-in Twemoji color by Mozilla.

In case you would like to change:

```toml
# Apple
# [fonts.emoji]
# family = "Apple Color Emoji"

# In case you have Noto Color Emoji installed
# [fonts.emoji]
# family = "Noto Color Emoji"
```

- Support to `fonts.ui`. You can specify user interface font on Rio.

Note: `fonts.ui` does not have live reload configuration update, you need to close and open Rio again.

```toml
[fonts.ui]
family = "Departure Mono"
```

- **breaking:** Revamp the cursor configuration

Before:

```toml
cursor = '▇'
blinking-cursor = false
```

After:

```toml
[cursor]
shape = 'block'
blinking = false
```

## 0.1.11

- Experimental support to Sixel protocol.
- Clipboard has been moved to Application level and shared to all windows.
- Replace `run` with `run_app`.
- Support CSI_t 16 (Report Cell Size in Pixels).
- Support CSI_t 14 (Report Terminal Window Size in Pixels).
- Fix on all the issues regarding whenever the font atlas reaches the limit.
- _breaking change_: collapsed tabs use now `tabs-active-highlight` instead of `tabs-active`.
- Default font for UI has changed to [DepartureMono](https://departuremono.com/).
- Performance: drop extra texture creation and manipulation.
- Fix on windows: If editor is not found, the app panics [#641](https://github.com/raphamorim/rio/issues/641).
- Improvements on `window.background-image` as respect width and height properties if were used.
- Macos: remove grab cursor when dragging and use default instead.
- Fix `tabs-active-highlight` config key [#618](https://github.com/raphamorim/rio/pull/618).
- Add `tabs-active-foreground` config key [#619](https://github.com/raphamorim/rio/pull/619).
- Add `tabs-foreground` config key.
- `use-kitty-keyboard-protocol` is now `true` as default.
- Remove tokio runtime.
- Allow configuring with lowercase values for enums.
- Rename `hide-cursor-when-typing` to `hide-mouse-cursor-when-typing`.
- Cleanup selection once happens a resize.
- Windows: Reduce WM_PAINT messages of thread target window.

## 0.1.10

- Refactor/Simplify close tabs logic internally.
- Fix: NativeTab margin top when `hide-if-single` is true.
- Fix: Search bar width on 1.0 dpi screens.
- Fix: Windows - The behavior of using a complete shell command and a shell command with parameters is inconsistent [#533](https://github.com/raphamorim/rio/issues/533).
- X11: Replace libxcursor with custom cursor code.
- Fix: Kitty keyboard protocol shifted key codes are reported in wrong order [#596](https://github.com/raphamorim/rio/issues/596).
- Fix: Mouse pointer hidden (Ubuntu Wayland) / Cursor icon not changing [#383](https://github.com/raphamorim/rio/issues/383).
- Enable search functionality as default on Linux.
- Enable search functionality as default on Microsoft Windows.
- Add command for closing all tabs except the current one (`CloseUnfocusedTabs`)

## 0.1.9

- Search support.
- New theme properties `search-match-background`, `search-match-foreground`, `search-focused-match-background` and `search-focused-match-foreground`.
- Fix bug Tab indicator doesn't disappear [#493](https://github.com/raphamorim/rio/issues/493).
- Fix color automation on tabs for linux.
- Update tabs UI styles (make it larger and able to show more text when necessary).
- Corrections on underline render proportions for different DPIs.
- Support writing the config to a custom/default location via `--write-config` (Ref: #605).
- Fix scale update on transitioning between screens with different DPI.
- Support a short variant (`-w`) for `--working-dir` argument.

## 0.1.8

- **breaking:** Introduced a new property in theme called `tabs-active-highlight`, default color is `#ff00ff`.
- **breaking:** Removed breadcrumb navigation.
- **breaking:** Introduced a new property in theme called `bar`, default color changed is `#1b1a1a`.
- **breaking:** `CollapsedTab` is now default for all platforms.
- Tab UI got some updates.
- Introduce `navigation.hide-if-single` property (Ref: [#595](https://github.com/raphamorim/rio/issues/595)).
- Performance update: Remove lock dependencies on render calls.
- Performance update: Render repeated styled fragments as one rect.
- Sugarloaf API has changed from `Sugar` primitives to `Content`.
- Fix: `[editor]` overshadow headerless parameters in default config. (Ref: #601)

## 0.1.7

**Breaking**

Editor property have changed from `String` to allow input arguments as well.

Before:

```toml
editor = "vi"
```

Now:

```toml
[editor]
program = "code"
args = ["-w"]
```

- Fix: editor doesn't handle arguments [#550](https://github.com/raphamorim/rio/issues/550).
- Fix: Weird rendering behaviour on setting padding-x in config [#590](https://github.com/raphamorim/rio/issues/590).
- Upgrade Rust to 1.80.1.

## 0.1.6

- Support custom colors on all underlines.
- Support for advaned formatting (squiggly underline?) [#370](https://github.com/raphamorim/rio/issues/370)
- Performance improvements!
  - Cache strategy has improved to cover any line that have been previously rendered.
  - Render backgrounds and cursors in one pass.
- Update tokio

## 0.1.5

- Fix Bug cell disappearance [#579](https://github.com/raphamorim/rio/issues/579).
- Fix Bug Rendering problem with TUIs using cursor movement control sequences in rio (v0.1.1+) [#574](https://github.com/raphamorim/rio/issues/574).
- Changed default font family to Cascadia Code.
- Changed default width to 800 and default height to 500.

## 0.1.4

- Fix Bug Text Rendering Bug [#543](https://github.com/raphamorim/rio/issues/543).
- Fix Abnormal font display and incomplete Navigation content display [#554](https://github.com/raphamorim/rio/issues/554).
- Fix Bug switch tabs doesn't work [#536](https://github.com/raphamorim/rio/issues/536).
- Update Cascadia Code to 2404.23.
- Change Cascadia builtin font from ttf to otf.
- Improvements for mouse selection.
- Performance improvements for background renders for all navigations besides `Plain` and `NativeTab`.
- Fix Cursor blinking is triggered by changes in inactive tabs [#437](https://github.com/raphamorim/rio/issues/437).
- Fix key bindings when key is uppercased (`alt` or `shift` is inputted along).
- Support to padding-y (ref: [#400](https://github.com/raphamorim/rio/issues/400))

Define y axis padding based on a format `[top, bottom]`, default is `[0, 0]`.

Example:

```toml
padding-y = [30, 10]
```

- Update swash (0.1.18), ab_glyph (0.2.28) and remove double hashmap implementation.

## 0.1.3

- Added support to font features (ref: #548 #551)

```toml
[fonts]
features = ["ss01", "ss02", "ss03", "ss04", "ss05", "ss06", "ss07", "ss08", "ss09"]
```

Note: Font features do not have support to live reload on configuration, so to reflect your changes, you will need to close and reopen Rio.

- fix: Wayland - No input after first run [#566](https://github.com/raphamorim/rio/issues/566).
- fix: Mouse pointer location differs from selected text #573.
- fix: IO Safety violation from dropping RawFd (fatal runtime error: IO Safety violation: owned file descriptor already closed).
- Upgrade to Rust 1.80.0.

## 0.1.2

- Upgrade wgpu to v22.0.0.
- Restrict of cells width.
- Wayland: update dependencies.
- Wayland: avoid crashing when compositor is misbehaving. (ref: raphamorim/winit 22522c9b37e9734c9a2408fae8d34b2599ff4574).
- Performance upgrades for lines rendered previously.

## 0.1.1

- Fix the validation errors whenever a surface is used with the vulkan backend.
- Clean up weak references to texture views and bind groups to prevent memory leaks.
- Fix crashes whenever reading binary files.
- Improvements on font loader (avoid set weight or style in the lookup if isn't defined).
- Fallbacks fonts doesn't trigger alerts anymore.

## 0.1.0

**Breaking change: Opacity API has changed**

- `background-opacity` has been renamed to `opacity`. It sets window background opacity.
- Removed `foreground-opacity` property.
- Removed support to DX11.

Example:

```toml
[window]
opacity = 0.8
```

- Major rewrite on sugarloaf.
  - New rendering architecture.
  - Sugarloaf now uses same render pass for each render.
  - Ignore equal renderers.
  - Compute layout updates only if layout is different.
- `BottomTab` navigation is now default for Linux and Windows.
- Support to font ligatures.
- Support bluetooth access on MacOs.
- Upgraded wgpu to 0.20.0.
- Support "open here" for Microsoft Windows.
- Fixes on font search for Microsoft Windows.
- Open Url support for MacOS.
- All tabs/window instances now use same font data.
- Disabled `line-height` configuration in this version (it will be re added eventually).
- Updated ttf-parser and memmap2 on sugarloaf.

#### Bug fixes

- closed: #514 Odd background transparency on macOS (Intel)
- closed: #398 Neovim and Helix rendering with line spacing
- closed: #512 Visible lines on transparent background
- closed: #491 Noticeable text update
- closed: #476 Glyphs have very weird rendering
- closed: #422 Background opacity
- closed: #355 Issues with double-width chars
- closed: #259 Sugarloaf: Positioning glyphs
- closed: #167 Tab bar overlaps text
- closed: #328 Some font issues
- closed: #225 Doesn't work with touchscreen
- closed: #307 default offset height is above the bottom position since update
- closed: #392 Box drawing issue with Berkeley Mono on MacOS

## 0.0.39

- Minor fix on fixed transparency on backgrounds for Welcome/Dialog.

## 0.0.38

- Corrections for transparency and blur for MacOS windows.
- Apply dynamic background logic only for images and keep alpha channel on background.

## 0.0.37

- _Breaking change:_ Reduced font size to `16.0`.
- _Breaking change:_ Set `VI mode` trigger with CTRL + SHIFT + SPACE on Windows.
- Update winit to 0.30.0.
- Update rust version to 1.77.2.
- Initial touch support by [@androw](https://github.com/androw) [#226](https://github.com/raphamorim/rio/pull/226)

## 0.0.36

- fixes for x11 freeze issue.
- update winit to 0.29.15.
- update wix (toolset that builds Windows Installer) from 4.0.1 to 4.0.4.

## 0.0.35

- Bump wayland dependencies: `wayland-backend`, `wayland-client`, `wayland-cursor` and `wayland-scanner`.
- Refactor: disable cursor blink on selection (ref #437) #441 by @hougesen .
- Rewrite hash logic to use `BuildHasher::hash_one`.
- Report focus change https://terminalguide.namepad.de/mode/p1004/.
- update rust version to 1.75.0.
- update winit to 0.29.11.

## 0.0.34

- use Fowler–Noll–Vo hash function implementation for sugar cache (more efficient for smaller hash keys)
- update winit to 0.29.9

## 0.0.33

- **Breaking**: Removed `macos-hide-toolbar-buttons` in favor of `window.decorations` api.
- Fix: Rio failing to draw blur upon launch #379
- Fix: Window transparency does not work on X11 #361
- Added support for path based color automation.
- Added `window.decorations` property, available options are `Enabled`, `Disabled`, `Transparent` and `Buttonless`.

## 0.0.32

- Fix: font order priority.
- Fix: add default values to keyboard config (#382)

## 0.0.31

- **Breaking**: Configuration `performance` has moved to `renderer.performance`.
- **Breaking**: Configuration `disable-renderer-when-unfocused` has moved to `renderer.disable-renderer-when-unfocused`.
- **Breaking**: Configuration `use-kitty-keyboard-protocol` has moved to `keyboard.use-kitty-keyboard-protocol`.

- Introduction of new configuration property called `keyboard`.

```toml
[keyboard]
use-kitty-keyboard-protocol = false
disable-ctlseqs-alt = false
```

- Introduction of `keyboard.disable-ctlseqs-alt`: Disable ctlseqs with ALT keys. It is useful for example if you would like Rio to replicate Terminal.app, since it does not deal with ctlseqs with ALT keys

- Introduction of new configuration property called `renderer`.

```toml
[renderer]
performance = "High"
disable-renderer-when-unfocused = false
backend = "Automatic"

# backend options:
# Automatic: Leave Sugarloaf/WGPU to decide
# GL: Supported on Linux/Android, and Windows and macOS/iOS via ANGLE
# Vulkan: Supported on Windows, Linux/Android
# DX12: Supported on Windows 10
# DX11: Supported on Windows 7+
# Metal: Supported on macOS/iOS
```

- Fix: update padding top on config change [#378](https://github.com/raphamorim/rio/pull/378) by [@hougesen](https://github.com/hougesen)
- Fixed bug where color automation did not work on Linux because of line ending character.
- Fix: Control + Up/Down don't works as expected on neovim [#371](https://github.com/raphamorim/rio/issues/371)
- Fix: remove duplicate kitty backspace keybinds [#375](https://github.com/raphamorim/rio/pull/375) by [@hougesen](https://github.com/hougesen)
- Fix: Kitty-keyboard-protocol causes Backspace to delete 2 characters. [#344](https://github.com/raphamorim/rio/issues/344) by [@hougesen](https://github.com/hougesen)

## 0.0.30

- Fix regression with color ansi when transparency is off.
- **Breaking**: Config `navigation.macos-hide-window-buttons` has moved to `window.macos-hide-toolbar-buttons`.
- **Breaking**: Config property `padding-x` has been updated from 5.0 to 0.0 on MacOS.

## 0.0.29

- Fix compiled binary shows nothing inside the app window [#366](https://github.com/raphamorim/rio/issues/366).
- Fix command key + left and right strange behavior [#359](https://github.com/raphamorim/rio/issues/359).
- **New scroll API**: Scroll calculation for canonical mode will be based on `(accumulated scroll * multiplier / divider)` so if you want quicker scroll, keep increasing the multiplier if you want to reduce you increase the divider. Can use both properties also to find the best scroll for you:

```toml
[scroll]
multiplier = 3.0
divider = 1.0
```

- Corrections for TMUX scroll calculations.

## 0.0.28

- **Breaking**: Settings UI has been removed and `editor` property has been added.
- **Breaking**: default `padding-x` for MacOS has moved from `10.0` to `5.0`.
- **Breaking: Background API has moved to Window**

Example:

```toml
[window]
width = 600
height = 400
mode = "Windowed"
foreground-opacity = 1.0
background-opacity = 1.0
```

Using image as background:

```toml
[window.background-image]
path = "/Users/rapha/Desktop/eastward.jpg"
width = 200.0
height = 200.0
x = 0.0
y = 0.0
```

- **Breaking:** MacOS default navigation mode will become `NativeTab`.
- Support for blur background.
- Support opacity for foreground and background.
- Cursor hide feature is now behind configuration `hide-cursor-when-typing`.
- Confirm before quite (it can be disabled through configuration `confirm-before-quit`).
- Close the last tab in MacOS when using `command + w` (Ref: [#296](https://github.com/raphamorim/rio/issues/296))
- OSC 8 (Hyperlinks).
- Fix current path on new tab is not working when using Native Tab (Ref [#323](https://github.com/raphamorim/rio/issues/323)).
- Change `POLLING_TIMEOUT` for configuration update from 1s to 2s.
- Update `.icns` file with more format and add new icon (Ref: [#329](https://github.com/raphamorim/rio/pull/329)) by [@nix6839](https://github.com/nix6839).
- Update `.ico` files with more resolution and add new icon (Ref: [#329](https://github.com/raphamorim/rio/pull/329)) by [@nix6839](https://github.com/nix6839).

## 0.0.27

- Activate the hyperlink check whenever a modifier is changed (`alt` for windows/linux/bsd and `command` for macos).
- Fix Error when Double click on terminal side (Ref [#316](https://github.com/raphamorim/rio/issues/316)).

## 0.0.26

- Upgrade winit to 0.29.3.
- Support for `Run` actions key bindings for Microsoft Windows.
- Hyperlink support (Ref [#60](https://github.com/raphamorim/rio/issues/60))

## 0.0.25

- Upgrade wgpu to 0.18.0.
- Desktop OpenGL 3.3+ Support on Windows through WebGPU.
- Display the shell name on the tab title for MacOS Native Tab (Ref [#311](https://github.com/raphamorim/rio/issues/311) by [@eduronqui](https://github.com/eduronqui)).
- Fix VI cursor disappearing whenever perform a scroll..
- Fix flagged dimmed colors (cases where it does not comes from rgb index).
- Fix MacOS fullscreen empty space on margin top.
- Upgrade winit to 0.29.2.

## 0.0.24

- Improvements on selection text for scale factor >= 2.0.
- Improvements on cursor sugar creation, dropped unnecessary usage of clone.
- Colors/Themes got a new property called `vi-cursor`, you can specify any color you wish for VI Cursor.
- Alacritty's VI Mode.

## 0.0.23

#### Breaking changes

- `navigation.mode = "Plain"` now only shutdowns the key bindings related to tab creation/manipulation.
- `ignore-selection-fg-color` has been renamed to `ignore-selection-foreground-color`.
- Kitty keyboard protocol has been disabled by default in this version, for enable it you need to use `use-kitty-keyboard-protocol = true`.
- `CollapsedTab` is not based on reverse order anymore.
- Actions `SelectTab1`, `SelectTab2`, ..., `SelectTab9` have been removed in favor of the new select tab API:

```toml
[bindings]
keys = [
    { key = "1", with = "super", action = "SelectTab(0)" },
    { key = "2", with = "super", action = "SelectTab(1)" },
    { key = "3", with = "super", action = "SelectTab(2)" }
]
```

- Actions `ScrollLineUp` and `ScrollLineDown` have been removed in favor of the new Scroll API:

```toml
[bindings]
keys = [
    # Scroll up 8 lines
    { key = "up", with = "super", action = "Scroll(8)" },
    # Scroll down 5 lines
    { key = "down", with = "super", action = "Scroll(-5)" }
]
```

#### Other changes

- Rendering performance small improvements towards to Sugar text for regular font, dropped in redundancy processing (avg 68ms to 22ms with tests using 155x94 without repetition like `vim Cargo.lock`).
- Rendering performance small improvements towards to Sugar rect calculation, dropped in redundancy processing. Now Sugarloaf computes better Rects duplication in a line. It gains significant performance for large screens (avg ~12ms).
- Fix Backspace behaviour misplace on Windows (Ref https://github.com/raphamorim/rio/issues/220).
- `ClearHistory` key binding is available to use per configuration file.
- Introduce Alacritty's VI Mode (Ref https://github.com/raphamorim/rio/issues/186).
- Implement `ClearSelection` key binding action.
- Fix Cursor shape isn't restored (Ref https://github.com/raphamorim/rio/issues/279).
- Fix color automation for breadcrumb mode (Ref https://github.com/raphamorim/rio/issues/251).
- Fix text copy (OSC 52) is broken (tmux, zellij) (Ref https://github.com/raphamorim/rio/issues/276).
- Fix lines calculation for different fonts.
- Fix bug whenever is not closing terminal for non native tabs (Ref https://github.com/raphamorim/rio/issues/255).
- Removal of hide cursor functionality when start to type for all platforms besides Apple MacOS.
- Support to new scroll action API key binding.
- Support to new select tab action API key binding.
- Support to execute programs as actions for key bindings:

```toml
[bindings]
keys = [
    { key = "p", with = "super", action = "Run(code)" },
    { key = "o", with = "super", action = "Run(sublime ~/.config/rio/config.toml)" }
]
```

- Upgrade rust to 1.73.0 by @igorvieira.

## 0.0.22

- Now you can add extra fonts to load:

```toml
[fonts]
extras = [{ family = "Microsoft JhengHei" }]
```

- Added `ScrollLineUp`, `ScrollLineDown`, `ScrollHalfPageUp`, `ScrollHalfPageDown`, `ScrollToTop`and `ScrollToBottom` to bindings.
- Fix japanese characters on Microsoft Windows (Ref: https://github.com/raphamorim/rio/issues/266).
- Navigation fonts now use the CascadiaCode built-in font and cannot be changed.
- Proper select adapter with `is_srgb` filter check.
- Switched to queue rendering instead of use staging_belt.
- Fixed leaks whenever buffer dropped map callbacks.
- Forked and embedded glyph-brush project to sugarloaf. Glyph-brush was originally created @alexheretic and is licensed under Apache-2.0 license.
- Upgrade wgpu to 0.17.1.

## 0.0.21

- Hide other applications in MacOS #262 by @sonbui00.
- Implemented `working-dir` parameter to cli https://github.com/raphamorim/rio/issues/258.
- Remove legacy icns icons from bundle.

## 0.0.20

- Fix retrieve foreground process name to tabs.
- Fix cursor disappearing in the first tab whenever a new tab is created with NativeTab.
- Fix settings for NativeTabs.
- New docs.
- Removal of RIO_CONFIG environment variable.
- Add ToggleFullscreen Action #229 (Ref: https://github.com/raphamorim/rio/pull/249)
- fix: Command + H can't hide rio on macOS (Ref: https://github.com/raphamorim/rio/pull/244).
- Added fontconfig to font loader.
- New Rio terminal logo.
- Update Rust to 1.72.1 (Ref: https://github.com/raphamorim/rio/pull/238).
- Enable CPU-specific optimizations on aarch64-apple-darwin (Ref: https://github.com/raphamorim/rio/pull/235).
- Use release profile with optimization level as 3 (Ref: https://github.com/raphamorim/rio/pull/236).
- Use fixed dependency versions in sugarloaf
- Added split support along with the following actions `SplitVertically`, `SplitHorizontally` and `ClosePane` (support to split is still not available).

## 0.0.19

**Breaking change**

Configuration properties: `window_height`, `window_width` and `window_opacity` has been moved to a new window/background API:

```toml
# Window configuration
#
# • width - define the initial window width.
#   Default: 600
#
# • height - define the initial window height.
#   Default: 400
#
# • mode - define how the window will be created
#     - "Windowed" (default) is based on width and height
#     - "Maximized" window is created with maximized
#     - "Fullscreen" window is created with fullscreen
#
[window]
width = 600
height = 400
mode = "Windowed"

# Background configuration
#
# • opacity - changes the background transparency state
#   Default: 1.0
#
# • mode - defines background mode between "Color" and "Image"
#   Default: Color
#
# • image - Set an image as background
#   Default: None
#
[background]
mode = "Image"
opacity = 1.0
[background.image]
path = "/Users/rapha/Desktop/eastward.jpg"
width = 200.0
height = 200.0
x = 0.0
```

- Fix for retrieving shell environment variable when running inside of Flatpak sandbox (Ref: https://github.com/raphamorim/rio/issues/198).
- Rio terminal is now also available in crates.io: https://crates.io/crates/rioterm .
- Added `navigation.mode = "Plain"`, it basically disables all platform key bindings for tabs, windows and panels creation (Ref https://github.com/raphamorim/rio/issues/213).
- Support for blinking cursor (Ref: https://github.com/raphamorim/rio/issues/137) (this option is not enabled by default).
- Migrated font-kit to a custom font loader.
- Support to MacOS tile window positioning feature (left or right).
- Added support to MacOS display native top bar items.
- Support to adaptive theme (theme selection based on user system theme variant `dark` or `light`).
- Implemented `ScrollPageUp`, `ScrollPageDown`, `ScrollHalfPageUp`, `ScrollHalfPageDown`, `ScrollToTop`, `ScrollToBottom`, `ScrollLineUp`, `ScrollLineDown` (Ref: https://github.com/raphamorim/rio/issues/206).
- Support to `fonts.family` (it overwrites regular, bold, bold-italic and italic font families).
- Added a welcome screen UI.
- Added a settings UI.
- Exposes `RIO_CONFIG` environment variable that contains the path of the configuration.
- Rio creates a configuration file with all defaults if does not exist.
- Added `OpenConfigEditor` key binding for all platforms.
- Configuration property `editor` was removed.
- Created Assistant, Rio terminal UI for display error (Ref: https://github.com/raphamorim/rio/issues/168).
- Fix 'Backspace' keypress triggers Ctrl+h keybinding in Zellij instead of deleting character. (Ref: https://github.com/raphamorim/rio/issues/197).
- Implemented `TERM_PROGRAM` and `TERM_PROGRAM_VERSION` (Ref: https://github.com/raphamorim/rio/issues/200).
- Whenever native tabs is on disable macos deadzone logic.

## 0.0.18

- Upgraded to Rust 1.72.0.
- Fix delete key inputs square character.
- Fix Breadcrumb navigation crash.

## 0.0.17

#### Breaking changes

- Configuration `font` does not work anymore, a new configuration API of font selection has been introduced.

```toml
[fonts]
size = 18

[fonts.regular]
family = "cascadiamono"
style = "normal"
weight = 400

[fonts.bold]
family = "cascadiamono"
style = "normal"
weight = 800

[fonts.italic]
family = "cascadiamono"
style = "italic"
weight = 400

[fonts.bold-italic]
family = "cascadiamono"
style = "italic"
weight = 800
```

- Action `TabSwitchNext` and `TabSwitchPrev` has been renamed to `SelectNextTab` and `SelectPrevTab`.

#### Rest of 0.0.17 changelog

- Support to `NativeTab` (MacOS only).
- Support for kitty's keyboard protocol (`CSI u`). Ref: https://sw.kovidgoyal.net/kitty/keyboard-protocol/
- Added new actions for tab selection: `SelectTab1`, `SelectTab2`, `SelectTab3`, `SelectTab4`, `SelectTab5`, `SelectTab6`, `SelectTab7`, `SelectTab8`, `SelectTab9`, `SelectLastTab`.
- Support lowercased action and fix overwrite for actions in custom key bindings.
- Added action `Minimize` for minimize Rio terminal window.
- Added action `ClearHistory` for clear terminal saved history.
- Added action `ReceiveChar` for custom key bindings.
- New default key bindings for Linux and Windows so that conflicts with readline key bindings are removed.
- Winit Version 0.29.1-beta.
- Allow paste with the middle mouse of the button (fixes https://github.com/raphamorim/rio/issues/123).
- Support startup notify protocol to raise initial window on Wayland/X11.
- Fix Double-tap by touchpad on the titlebar doesn't maximize/unmaximize the window in GNOME 44, Wayland.

## 0.0.16

- Fix tab/breadcrumb bug introduced in 0.0.15
- Introduce new configuration property: `navigation.macos-hide-window-button`.

## 0.0.15

- Introduce configurable navigation with the following options: `CollapsedTab` (default), `Breadcrumb`, `TopTab` and `BottomTab`.

An example of configuration:

```toml
[navigation]
mode = "BottomTab"
use-current-path = true
clickable = false
```

- Performance improvements with Sugarloaf de-duplication of input data.
  - Before: `~253.5µs`.
  - Now: `~51.5µs`.
- Introduce `navigation.use-current-path` which sets if a tab/breacrumb should be open from the current context path.
- Fix rendering unicode with 1 width glyphs (fix [#160](https://github.com/raphamorim/rio/issues/160)).
- Increased max tabs from 9 to 20.
- Default colors `selection-foreground` and `selection-background` has changed.
- Default colors `tab` and `tab-active` has changed.

## 0.0.14

- Implementation of custom key bindings ([#117](https://github.com/raphamorim/rio/issues/117)).
- Fix .deb packing in GH Actions.
- Fix key binding for switch tab next (MacOS only).
- Fix scroll when copying text outside of offset.
- Fix copy key bindings.

## 0.0.13

- Fix Fuzzy Finder issue ([#132](https://github.com/raphamorim/rio/issues/132)).
- Introduce Copa (Alacritty's VTE forked version to introduce new sequences/instructions in next versions).
- Upgraded Winit to 0.29.0-beta.0.
- Support for keybindings with dead keys.
- `Back`/`Forward` mouse buttons support in bindings.
- Fix unconditional query of xdg-portal settings on Wayland.
- Fix `Maximized` startup mode not filling the screen properly on GNOME Wayland.
- Fix Default Vi key bindings for `Last`/`First` actions not working on X11/Wayland.
- Set `padding-x` to 0 for non-macos.
- Set `app_id`/`WM_CLASS` property on Wayland/X11.

## 0.0.12

- Strip binary is on for release builds.
- Each paste or key binding that has writing leads to clear selection and scroll bottom.
- Fixed over-rendering when scrolling.
- Fix selection.
- Support to copy using VIM.
- Fix for MacOS deadzone changing cursor to draggable on window buttons.
- Fix for scroll using tmux.

## 0.0.11

- Fix for font styles using CachedSugar.

## 0.0.10

- Major refactor of Sugarloaf.
  - Performance improvements around 80-110%.
  - Introduced CachedSugar.
  - Usage of PixelScale.
  - Line-height support.
- Open new tab using the current tab directory.
- Fix some symbols break the horizontal and vertical alignment of lines (ref [#148](https://github.com/raphamorim/rio/issues/148)).
- Fix font size configuration is confusing (ref [#139](https://github.com/raphamorim/rio/issues/139)).
- Fix Glyph not rendered in prompt (ref: [#135](https://github.com/raphamorim/rio/issues/135)).
- Use fork by default in context tests.
- Updated terminfo.
- Increased default font size to 18.
- Move to next and prev tab using keybindings.
- Setting editor by keybindings and new property called `editor` in configuration file.
- Rio creates `.deb` packages (canary and release).
- Binary size optimization (ref: [#152](https://github.com/raphamorim/rio/pull/152)) by [@OlshaMB]

## 0.0.9

- Created "rio" terminfo.
- Breaking changes for configuration file regarding `Advanced`. The configuration `Advanced` has moved to root level and `disable-render-when-unfocused` renamed to `disable-unfocused-render`.

**before**

```toml
theme = "dracula"

[advanced]
disable-render-when-unfocused = true
```

**now**

```toml
theme = "dracula"
disable-unfocused-render = true
```

- Support to **spawn and fork processes**, spawn has became default. Spawn increases Rio compatibility in a broad range, like old MacOS versions (older or equal to Big Sur). However, If you want to use Rio terminal to fork processes instead of spawning processes, enable `use-fork` in the configuration file:

```toml
use-fork = true
```

- Introduced `RIO_LOG_LEVEL` variable usage. (`e.g: RIO_LOG_LEVEL=debug rio -e "echo 1"`)
- Increased max tabs from 6 to 9.
- Fix Incorrect cursor position when using multi-byte characters (Ref: [#127](https://github.com/raphamorim/rio/issues/127))
- Fix bug ["black screen with nearly zero interactivity"](https://github.com/raphamorim/rio/issues/112) and new tab hanging.
- Fix cursor disappearing after resize.
- Introduction of `shell` and `working_dir` in configuration file.
- Multi window support [#97](https://github.com/raphamorim/rio/issues/97).
- Corrections on select and scroll experience (it was using wrongly font-bound for line calculation).
- Add selection color to the theme config (closed [#125](https://github.com/raphamorim/rio/issues/125)).
- Implemented Inverse (fix [#92](https://github.com/raphamorim/rio/issues/92)).
- Proper choose formats that matches with `TextureFormat::is_srgb` (it fixed the Vulkan driver, related [#122](https://github.com/raphamorim/rio/issues/122)).
- Corcovado: Filter windows crate dependency to only Windows targets (related: [#119](https://github.com/raphamorim/rio/issues/119)).
- Teletypewriter: Fixes for musl as target_env (related: [#119](https://github.com/raphamorim/rio/issues/119)).
- FreeBSD support, implementation by [yurivict](https://github.com/yurivict) ([Commit](https://github.com/freebsd/freebsd-ports/commit/8582b8c59459a7dc5112a94a39de45f6cc124c3e), Ref: [#115](https://github.com/raphamorim/rio/issues/115))

## 0.0.8

- Added generation of `.msi` and `.exe` files to the release pipeline (stable and canary).
- Support to Microsoft Windows.
- Ability to in|decrease font size using keyboard shortcut during session (ref: [#109](https://github.com/raphamorim/rio/issues/109))
- Inverted Canary and Stable icons.
- ANSI mouse reports (e.g: scroll and click working on VIM).
- Scroll and apply selection.
- Semantic and line selection.
- Rio is available in Homebrew casks (ref [github.com/Homebrew/homebrew-cask/pull/149824](https://github.com/Homebrew/homebrew-cask/pull/149824)).
- Rio stable versions are notarized now.
- Migration of mio, mio-extras, mio-signal-hook to Corcovado.
- Changed default black color to `#4c4345`.
- Fix mouse position for when selecting text.

## 0.0.7

- Breaking changes for configuration file regarding `Style` property.

before:

```toml
performance = "High"
[style]
font-size = 18
theme = "lucario"
```

now:

```toml
performance = "High"
theme = "lucario"
font-size = 18
```

- Fix Background color not entirely set on vim [#88](https://github.com/raphamorim/rio/issues/88)
- Scroll now works for x11 and wayland.
- No longer renders to macos and x11 windows that are fully occluded / not directly visible.
- Introduced `window-opacity` config property for WebAssembly and Wayland builds.
- Add permissions instructions to Rio macos builds (Fix [#99](https://github.com/raphamorim/rio/issues/99)).
- Fixes for x11 and wayland rendering (Related: [#98](https://github.com/raphamorim/rio/issues/98) and [#100](https://github.com/raphamorim/rio/issues/100)).
- Performance fixes (Related: [#101](https://github.com/raphamorim/rio/issues/101)).
- Sugarloaf WebAssembly support.
- Fixed resize for all contexts: removed the glitch when resizing and switching between tabs.
- Fixed cursor inconsistencies [#95](https://github.com/raphamorim/rio/issues/95).
- Added command line interface support (`--help`, `--version`, `-e` and `--command`).
- Added a fallback for WPGU request device operation: downlevel limits, which will allow the code to run on all possible hardware.
- Added `padding-x` to configuration.
- Reload automatically when the configuration file is changed ([#69](https://github.com/raphamorim/rio/issues/69)).
- Fix `Ctrl+D`.
- Fix `exit` command not closing the app ([#87](https://github.com/raphamorim/rio/issues/87)).
- Changed default `light-black` color.

## 0.0.6

- Fix: support to clipboard in linux by [@joseemds](https://github.com/joseemds).
- Font style for custom fonts by [@OlshaMB](https://github.com/OlshaMB) (closed [#80](https://github.com/raphamorim/rio/issues/80) and [#81](https://github.com/raphamorim/rio/issues/81))
- Text styles Underline and Strikethrough (closed [#79](https://github.com/raphamorim/rio/issues/79)).
- Update default colors for tabs/tabs-active.
- Tabs support.
- Fix rendering tab and hidden chars by replacing to space by [@niuez](https://github.com/niuez), (closed [#56](https://github.com/raphamorim/rio/issues/56)).
- Block cursor hover a character and still allow it to be visible.
- Support to caret Beam and Underline cursor [#67](https://github.com/raphamorim/rio/issues/67) by [@niuez](https://github.com/niuez).
- Fix panics if custom font is not found [#68](https://github.com/raphamorim/rio/issues/68).
- MacOs ignore alt key in cntrlseq (same behavior as Terminal.app, Hyper, iTerm and etecetera).

## 0.0.5

- Fix ctlseqs modifiers for bindings.
- Add RioEvent::ColorRequest events to write color updates on pty.
- Fix to render specific 24bit colors (#66) by [@niuez](https://github.com/niuez).
- Cross build for arm64 and x86
- Bold and Italic support (https://github.com/raphamorim/rio/issues/33).
- Theme support (eae39bc81b5b561882b7a37b2c03896633276c27)
- Fix font-size dependency for serialization (f278102)
- Fix cursor visibility on VI mode and scroll (https://github.com/raphamorim/rio/issues/51)
- Performance fixes for rendering from teletypewriter updates.
- Fix scale issues for 1.0 scale factor or using monitor with different scale factor. (https://github.com/raphamorim/rio/issues/50)
- Improve `make pack-osx-arm` and `make pack-osx-x86` to only contain Rio.app file. (https://github.com/raphamorim/rio/issues/54)

## 0.0.4

- Fix CPU large usage when scrolling.
- Task scheduler.
- Copy feature.
- Selection feature (selection doesn't work when scrolling yet).
- Change default cursor icon for Text (`winit::window::CursorIcon`).
- Scroll bottom when display offset is different than zero.
- Fix for user interaction "close Rio terminal" using UI interface (`ExitWithCode(0)`).
- Hide cursor when typing and make it visible again with scroll and cursor interactions.
- Implementation of paste files to string path.

## 0.0.3

- Added Input Method Engine (IME) support. Note: only works for preedit with single character now, which means that still need to fix for other keyboards as Japanese, Chinese [...].
- Common Keybindings and keybindings for MacOS.
- Allow to configure `option-as-alt` for Winit on MacOs. Issue originally bought by Alacritty on Winit (https://github.com/rust-windowing/winit/issues/768).
- Allow to configure environment variables through config file.
- Stabilization of Sugarloaf render on emojis, symbols and unicode.

## 0.0.2

- `log-level` as configurable (`DEBUG`, `INFO`, `TRACE`, `ERROR`, `WARN` and `OFF`). `OFF` by default.
- Introduction of rendering engine called Sugarloaf.
- System font loader (tested and implemented for MacOs).
- Font loader with not native emoji font (emojis aren't stable yet).
- Rect renderer based on provided color (text background), stabilized for monospaced fonts.

## 0.0.1

- Basic move/goto functionalities.
- Initial definition of Rio default colors.
- Set and reset color by ANSI parser.
- Clear/Tabs functionalities.
- Grid introduction.
- Desktop delta scroll (up and down, without scrollbar UI component).
- `Teletypewriter` 2.0.0 usage for macos and linux.
- Resize support.
- $SHELL login on macos, by default: `/bin/zsh --login` (if $SHELL is settled as other could as run `/bin/bash --login`, `/bin/fish --login` ...).
- Cursor initial support (without VI mode).
