---
title: 'Key Bindings'
language: 'en'
---

Rio allows you to add new keybindings and overwrite any default key bindings.

Bindings are built using the following trigger fields:

| Name          | Description     |
| ------------- | --------------- |
| [key](#key)   | The key pressed |
| [with](#with) | Modifier keys   |
| [mode](#mode) | Terminal mode   |

Whom can be be combined with the following effect fields:

| Name              | Description            |
| ----------------- | ---------------------- |
| [action](#action) | Predefined Rio actions |
| [esc](#esc)       | Send escape sequence   |

```toml
[bindings]
keys = [
  { key = "q", with = "super", action = "Quit" },
  # Send escape sequence to clear screen and move cursor to home
  { key = "l", with = "control", esc = "\u001b[2J\u001b[H" },
  # Remove existing keybind
  { key = "v", with = "control | shift", action = "none" },
  # Panel resize actions (customize as needed)
  { key = "up", with = "control | super", action = "MoveDividerUp" },
  { key = "down", with = "control | super", action = "MoveDividerDown" },
  { key = "left", with = "control | super", action = "MoveDividerLeft" },
  { key = "right", with = "control | super", action = "MoveDividerRight" },
]
```

## [Key](#key)

Each value in key binding will specify an identifier of the key pressed:

- `a-z`
- `0-9`
- `F1-F24`
- `tab` `esc`
- `home` `space` `delete` `insert` `pageup` `pagedown` `end` `back`
- `up` `down` `left` `right`
- `@` `colon` `.` `return` `[` `]` `;` `\\` `+` `,` `/` `=` `-` `*`
- `numpadenter` `numpadadd` `numpadcomma` `numpaddivide` `numpadequals` `numpadsubtract` `numpadmultiply`
- `numpad1` `numpad2` `numpad3` `numpad4` `numpad5` `numpad6` `numpad7` `numpad8` `numpad9` `numpad0`

## [Action](#action)

Execute a predefined action in Rio terminal.

### [Basic Actions](#basic-actions)

| Action           | Description                                                                   |
| :--------------- | :---------------------------------------------------------------------------- |
| None             | |
| ReceiveChar      | |
| ToggleVIMode     | |
| Paste            | Paste command |
| Copy             | Copy command |
| OpenConfigEditor | Open configuration file on configured editor property |
| ResetFontSize    | |
| IncreaseFontSize | |
| DecreaseFontSize | |
| Run(string)      | Example: Running command `Run(code)` or `Run(code ~/.config/rio/config.toml)` |
| PasteSelection   | |
| ClearSelection   | |
| CreateWindow     | Create a Rio window instance |
| Quit             | Exit Rio |
| ToggleFullscreen | Toggle fullscreen |

### [Split Actions](#split-actions)

| Action               | Description                                                                |
| :------------------- | :------------------------------------------------------------------------- |
| SplitRight           | Create a split by right side |
| SplitDown            | Create a split by under current pane |
| SelectNextSplit      | Select next split |
| SelectPrevSplit      | Select previous split |
| CloseSplitOrTab      | Close split, if split is the last then will close the tab |
| SelectNextSplitOrTab | Select next split if available if not next tab |
| SelectPrevSplitOrTab | Select previous split if available if not previous tab |
| MoveDividerUp        | Move the split divider up to resize panels |
| MoveDividerDown      | Move the split divider down to resize panels |
| MoveDividerLeft      | Move the split divider left to resize panels |
| MoveDividerRight     | Move the split divider right to resize panels |

### [Tab Actions](#tab-actions)

| Action               | Description                                                             |
| :------------------- | :---------------------------------------------------------------------- |
| CreateTab            | Create new tab                                                          |
| CloseTab             | Close current tab                                                       |
| CloseUnfocusedTabs   | Close all tabs that are not currently focused                           |
| SelectNextTab        | Select next tab                                                         |
| SelectPrevTab        | Select pervious tab                                                     |
| SelectLastTab        | Select last tab                                                         |
| MoveCurrentTabToNext | Move the current focused tab to the next slot, or first when last       |
| MoveCurrentTabToPrev | Move the current focused tab to the previous slot, or last when first   |
| SelectTab(tab_index) | Example: Select first tab `SelectTab(0)`, second tab `SelectTab(1)`     |

### [Scroll Actions](#scroll-actions)

| Action             | Description                                                                |
| :----------------- | :------------------------------------------------------------------------- |
| Scroll(int)        | Example: Scroll up 8 lines `Scroll(8)` or scroll down 5 lines `Scroll(-5)` |
| ScrollPageUp       |                                                                            |
| ScrollPageDown     |                                                                            |
| ScrollHalfPageUp   |                                                                            |
| ScrollHalfPageDown |                                                                            |
| ScrollToTop        |                                                                            |
| ScrollToBottom     |                                                                            |

### [Search Actions](#search-actions)

| Action             | Description                                                                |
| :----------------- | :------------------------------------------------------------------------- |
| SearchForward      |  |
| SearchBackward     |  |
| SearchConfirm      |  |
| SearchClear        |  |
| SearchFocusNext    |  |
| SearchFocusPrevious  | |
| SearchDeleteWord     | |
| SearchHistoryNext     | |
| SearchHistoryPrevious | |

## [Esc](#esc)

Send escape sequences to the running application.

The `esc` field writes the specified escape sequence to the terminal. This makes it possible to send control sequences like clearing the screen, moving the cursor, or any other ANSI escape sequences. The sequences are sent directly to the PTY without any text manipulation.

### Common Escape Sequences

```toml
[bindings]
keys = [
  # Clear screen (ESC [ 2 J)
  { key = "k", with = "control", esc = "\u001b[2J" },

  # Clear screen and move cursor to home (ESC [ 2 J  ESC [ H)
  { key = "l", with = "control", esc = "\u001b[2J\u001b[H" },

  # Send form feed character (Ctrl+L) — works in most shells
  { key = "l", with = "control", esc = "\u000c" },

  # Move cursor to beginning of line (ESC [ H)
  { key = "a", with = "control", esc = "\u001b[H" },

  # Delete from cursor to end of line (ESC [ K)
  { key = "k", with = "control", esc = "\u001b[K" },

  # Send custom escape sequence (example: tmux prefix)
  { key = "a", with = "control", esc = "\u001ba" },

  # Page Up (ESC [ 5 ~)
  { key = "PageUp", esc = "\u001b[5~" },

  # Page Down (ESC [ 6 ~)
  { key = "PageDown", esc = "\u001b[6~" },
]
```

### Escape Sequence Format

Escape sequences must use **Unicode escape notation** in TOML:

* `\u001b` → ESC character (ASCII 27)
* `\u000c` → Form feed (Ctrl+L)
* `\n` → Newline
* `\r` → Carriage return
* `\t` → Tab

**Important**: In TOML configuration files, you must use `\u001b` for the ESC character. The `\x1b` notation will **not** work in TOML strings.

## [With](#with)

Key modifiers to filter binding actions

- `none`
- `control`
- `option`
- `super`
- `shift`
- `alt`

Multiple modifiers can be combined using `|` like this:

```toml
with = "control | shift"
```

<!--
 - `mode`: Indicate a binding for only specific terminal reported modes
    This is mainly used to send applications the correct escape sequences
    when in different modes.
    - AppCursor
    - AppKeypad
    - Alt
    A `~` operator can be used before a mode to apply the binding whenever
    the mode is *not* active, e.g. `~Alt`. -->

## [Mode](#mode)

There is currently four different modes:

- `vi`
- `alt` (Alt screen)
- `appcursor`
- `appkeypad`

`~` can be prefixed to disable the keybind while in that mode.

```toml
[bindings]
keys = [
  # Enable VI mode on escape, when not in VI mode.
  { key = "esc", mode = "~vi", action = "ToggleVIMode" },
]
```

## [Overwriting](#overwriting)

Bindings are always filled by default, but will be replaced when a new binding with the same triggers is defined. To unset a default binding, it can be mapped to the `ReceiveChar` action. Alternatively, you can use `None` for a no-op if you do not wish to receive input characters for that binding.

The example below will disable window creation binding in the macos:

```toml
[bindings]
keys = [
  { key = "n", with = "super", action = "ReceiveChar" }
]
```

`ReceiveChar` will treat the binding as non existent and simply receive the input and put the character into the terminal.

Optionally you can ignore/disable completely a binding using `None`. In the example below, whenever you use key "n" along with "super" key nothing will happen.

```toml
[bindings]
keys = [
  { key = "n", with = "super", action = "None" }
]
```

If you are missing a key binding that you believe that should be a default in the platform that you are using, feel free to [open an issue](https://github.com/raphamorim/rio).

## Default

### MacOS

| Action                 | Key                                                            |
| ---------------------- | -------------------------------------------------------------- |
| Open configuration     | `Command + Comma (,)`                                          |
| Toggle VI Mode         | `Option + Shift + Space`                                          |
| Copy                   | `Command + C`                                                  |
| Paste                  | `Command + V`                                                  |
| Create new window      | `Command + N`                                                  |
| Create new tab         | `Command + T`                                                  |
| Move to next tab       | `Control + Tab` or `Command + Shift + RightBracket (])`        |
| Move to previous tab   | `Control + Shift + Tab` or `Command + Shift + LeftBracket ([)` |
| Increase font size     | `Command + Plus (+)`                                           |
| Decrease font size     | `Command + Minus (-)`                                          |
| Reset font size        | `Command + 0`                                                  |
| Minimize window        | `Command + M`                                                  |
| Quit                   | `Command + Q`                                                  |
| Close tab              | `Command + W`                                                  |
| Select the first tab   | `Command + 1`                                                  |
| Select the second tab  | `Command + 2`                                                  |
| Select the third tab   | `Command + 3`                                                  |
| Select the fourth tab  | `Command + 4`                                                  |
| Select the fifth tab   | `Command + 5`                                                  |
| Select the sixth tab   | `Command + 6`                                                  |
| Select the seventh tab | `Command + 7`                                                  |
| Select the eighth tab  | `Command + 8`                                                  |
| Select the last tab    | `Command + 9`          |
| Search forward         | `Command + f`          |
| Search backward        | `Command + b`          |
| Split right        | `Command + d`          |
| Split down         | `Command + Shift + d`  |
| Select next split      | `Command + ]`          |
| Select prev split      | `Command + [`          |
| Move divider up        | `Control + Command + ArrowUp`    |
| Move divider down      | `Control + Command + ArrowDown`  |
| Move divider left      | `Control + Command + ArrowLeft`  |
| Move divider right     | `Control + Command + ArrowRight` |

### Windows

| Action               | Key                                                         |
| -------------------- | ----------------------------------------------------------- |
| Open configuration   | `Control + Shift + Comma (,)`                               |
| Toggle VI Mode       | `Control + Shift + Space`                                   |
| Copy                 | `Control + Shift + C`                                       |
| Paste                | `Control + Shift + V`                                       |
| Create new window    | `Control + Shift + N`                                       |
| Create new tab       | `Control + Shift + T`                                       |
| Move to next tab     | `Control + Tab` or `Control + Shift + RightBracket (])`     |
| Move to previous tab | `Control + Shift + Tab` `Control + Shift + LeftBracket ([)` |
| Increase font size   | `Control + Plus (+)`                                        |
| Decrease font size   | `Control + Minus (-)`                                       |
| Reset font size      | `Control + 0`                                               |
| Close tab or quit    | `Control + Shift + W`                                       |
| Select the first tab   | `Control + Shift + 1`                                                  |
| Select the second tab  | `Control + Shift + 2`                                                  |
| Select the third tab   | `Control + Shift + 3`                                                  |
| Select the fourth tab  | `Control + Shift + 4`                                                  |
| Select the fifth tab   | `Control + Shift + 5`                                                  |
| Select the sixth tab   | `Control + Shift + 6`                                                  |
| Select the seventh tab | `Control + Shift + 7`                                                  |
| Select the eighth tab  | `Control + Shift + 8`                                                  |
| Select the last tab    | `Control + Shift + 9`   |
| Search forward         | `Control + Shift + f`  |
| Search backward        | `Control + Shift + b`  |
| Split right        | `Control + Shift + r`  |
| Split down         | `Control + Shift + d`  |
| Select next split      | `Control + Shift + ]`  |
| Select prev split      | `Control + Shift + [`  |
| Move divider up        | `Control + Shift + Alt + ArrowUp`    |
| Move divider down      | `Control + Shift + Alt + ArrowDown`  |
| Move divider left      | `Control + Shift + Alt + ArrowLeft`  |
| Move divider right     | `Control + Shift + Alt + ArrowRight` |

### Linux and BSD

| Action               | Key                                                            |
| -------------------- | -------------------------------------------------------------- |
| Open configuration   | `Control + Shift + Comma (,)`                                  |
| Toggle VI Mode       | `Alt + Shift + Space`                                          |
| Copy                 | `Control + Shift + C`                                          |
| Paste                | `Control + Shift + V`                                          |
| Create new window    | `Control + Shift + N`                                          |
| Create new tab       | `Control + Shift + T`                                          |
| Move to next tab     | `Control + Tab` or `Control + Shift + RightBracket (])`        |
| Move to previous tab | `Control + Shift + Tab` or `Control + Shift + LeftBracket ([)` |
| Increase font size   | `Control + Plus (+)`                                           |
| Decrease font size   | `Control + Minus (-)`                                          |
| Reset font size      | `Control + 0`                                                  |
| Close tab or quit    | `Control + Shift + W`                                          |
| Select the first tab   | `Control + Shift + 1`                                                  |
| Select the second tab  | `Control + Shift + 2`                                                  |
| Select the third tab   | `Control + Shift + 3`                                                  |
| Select the fourth tab  | `Control + Shift + 4`                                                  |
| Select the fifth tab   | `Control + Shift + 5`                                                  |
| Select the sixth tab   | `Control + Shift + 6`                                                  |
| Select the seventh tab | `Control + Shift + 7`                                                  |
| Select the eighth tab  | `Control + Shift + 8`                                                  |
| Select the last tab    | `Control + Shift + 9`          |
| Search forward         | `Control + Shift + f`  |
| Search backward        | `Control + Shift + b`  |
| Split right        | `Control + Shift + r`  |
| Split down         | `Control + Shift + d`  |
| Select next split      | `Control + Shift + ]`  |
| Select prev split      | `Control + Shift + [`  |
| Move divider up        | `Control + Shift + Alt + ArrowUp`    |
| Move divider down      | `Control + Shift + Alt + ArrowDown`  |
| Move divider left      | `Control + Shift + Alt + ArrowLeft`  |
| Move divider right     | `Control + Shift + Alt + ArrowRight` |

### Search

| Action                  | Key                                    |
| ----------------------- | -------------------------------------- |
| Quit search             | `Escape` (while search is open)        |
| Quit search             | `Control + c` (while search is open)   |
| Focus next              | `Enter` (while search is open)         |
| Focus previous          | `Shift + Enter` (while search is open) |
| Clear search word       | `Control + w` (while search is open)   |
| Search history previous | `Control + p` (while search is open)   |
| Search history previous | `ArrowUp` (while search is open)       |
| Search history next     | `Control + n` (while search is open)   |
| Search history next     | `ArrowDown` (while search is open)     |

