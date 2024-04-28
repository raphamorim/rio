---
layout: docs
class: docs
title: 'Key Bindings'
language: 'en'
---

### MacOS

| Action                 | Key                                                            |
| ---------------------- | -------------------------------------------------------------- |
| Open configuration     | `Command + Comma (,)`                                          |
| Toggle VI Mode         | `Alt + Shift + Space`                                          |
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
| Select the last tab    | `Command + 9`                                                  |

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

<br/>

## [Custom key bindings](#custom-key-bindings)

Rio allows you to add new keybindings and overwrite any default key bindings.

Keybinds are built using the following trigger fields:

| Name          | Description     |
| ------------- | --------------- |
| [key](#key)   | The key pressed |
| [with](#with) | Modifier keys   |
| [mode](#mode) | Terminal mode   |

Whom can be be combined with the following effect fields:

| Name              | Description            |
| ----------------- | ---------------------- |
| [action](#action) | Predefined Rio actions |
| [bytes](#bytes)   | Write byte sequence    |
| [text](#text)     | Write text sequence    |

```toml
[bindings]
keys = [
  { key = "q", with = "super", action = "Quit" },
  # Bytes[27, 91, 53, 126] is equivalent to "\x1b[5~"
  { key = "home", with = "super | shift", bytes = [27, 91, 53, 126] },
  # Remove existing keybind
  { key = "v", with = "control | shift", action = "none" },
]
```

### [Key](#key)

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

### [Action](#action)

Execute a predefined action in Rio terminal.

#### [Basic Actions](#basic-actions)

| Action           | Description                                                                   |
| :--------------- | :---------------------------------------------------------------------------- |
| None             |                                                                               |
| ReceiveChar      |                                                                               |
| ToggleVIMode     |                                                                               |
| Paste            | Paste command                                                                 |
| Copy             |                                                                               |
| OpenConfigEditor |                                                                               |
| ResetFontSize    |                                                                               |
| IncreaseFontSize |                                                                               |
| DecreaseFontSize |                                                                               |
| Run(string)      | Example: Running command `Run(code)` or `Run(code ~/.config/rio/config.toml)` |
| PasteSelection   |                                                                               |
| ClearSelection   |                                                                               |

#### [Window Actions](#window-actions)

| Action       | Description |
| :----------- | :---------- |
| CreateWindow |             |
| Quit         |             |

#### [Pane Actions](#pane-actions)

| Action            | Description |
| :---------------- | :---------- |
| SplitHorizontally |             |
| SplitVertically   |             |
| ClosePane         |             |

#### [Tab Actions](#tab-actions)

| Action               | Description                                                         |
| :------------------- | :------------------------------------------------------------------ |
| CreateTab            |                                                                     |
| CloseTab             |                                                                     |
| SelectPrevTab        |                                                                     |
| SelectNextTab        |                                                                     |
| SelectLastTab        |                                                                     |
| SelectTab(tab_index) | Example: Select first tab `SelectTab(0)`, second tab `SelectTab(1)` |

#### [Scroll Actions](#scroll-actions)

| Action             | Description                                                                |
| :----------------- | :------------------------------------------------------------------------- |
| Scroll(int)        | Example: Scroll up 8 lines `Scroll(8)` or scroll down 5 lines `Scroll(-5)` |
| ScrollPageUp       |                                                                            |
| ScrollPageDown     |                                                                            |
| ScrollHalfPageUp   |                                                                            |
| ScrollHalfPageDown |                                                                            |
| ScrollToTop        |                                                                            |
| ScrollToBottom     |                                                                            |

### [Bytes](#bytes)

Send a byte sequence to the running application.

The `bytes` field writes the specified string to the terminal. This makes
it possible to pass escape sequences, like `PageUp` ("\x1b[5~"). Note that applications use terminfo to map escape sequences back
to keys. It is therefore required to update the terminfo when changing an escape sequence.

### [With](#with)

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

### [Mode](#mode)

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

### [Text](#text)

`text` can be used to write specific text on key press:

```toml
[bindings]
keys = [
  # Write `Rio is awesome!` on `Control + r`
  { key = "r", with = "control", text = "Rio is awesome!" },
]
```

### [Overwriting](#overwriting)

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
