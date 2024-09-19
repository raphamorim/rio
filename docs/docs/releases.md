---
title: 'Releases'
language: 'en'
---

# Releases

<!-- ## Unreleased -->

<!-- - Fix: MacOS Delete key doesn't work in kitty mode [#513](https://github.com/raphamorim/rio/issues/513). -->
<!-- - Fix: Kitty keyboard protocol doesn't work with tmux [#599](https://github.com/raphamorim/rio/issues/599). -->

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
- Fix:  sixel: Text doesn't overwrite sixels [#636](https://github.com/raphamorim/rio/issues/636).
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
- *breaking change*: collapsed tabs use now `tabs-active-highlight` instead of `tabs-active`.
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

Define y axis padding based on a format `[top, left]`, default is `[0, 0]`.

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

- *Breaking change:* Reduced font size to `16.0`.
- *Breaking change:* Set `VI mode` trigger with CTRL + SHIFT + SPACE on Windows.
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
