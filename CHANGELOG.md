# Changelog

## In progress

- Added `OpenConfigEditor` key binding for all platforms.
- Created Settings UI inside of the Rio terminal.
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
- Fix for MacOS deadzone chaging cursor to draggable on window buttons.
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

- Support to **spawn and fork processes**, spawn has became default. Spawn increases Rio compability in a broad range, like old MacOS versions (older or equal to Big Sur). However, If you want to use Rio terminal to fork processes instead of spawning processes, enable `use-fork` in the configuration file:

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
- Fixed cursor incosistencies [#95](https://github.com/raphamorim/rio/issues/95).
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
- Stabilization of Sugarloaf render on emojis, symbos and unicode.

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
