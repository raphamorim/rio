---
layout: docs
class: docs
title: 'Key Bindings'
language: 'en'
---

### MacOS

Open configuration: `Command + Comma (,)`

Copy: `Command + C`

Paste: `Command + V`

Create new window: `Command + N`

Create new tab: `Command + T`

Move to next tab: `Command + Shift + RightBracket (])`

Move to previous tab: `Command + Shift + LeftBracket ([)` or `Control + Shift + Tab`

Switch tabs by created order: `Control + Tab`

Increase font size: `Command + Plus (+)`

Decrease font size: `Command + Minus (-)`

Reset font size: `Command + 0`

Minimize window: `Command + M`

Quit: `Command + Q`

Close tab: `Command + W`

Select the first tab: `Command + 1`

Select the second tab: `Command + 2`

Select the third tab: `Command + 3`

Select the fourth tab: `Command + 4`

Select the fifth tab: `Command + 5`

Select the sixth tab: `Command + 6`

Select the seventh tab: `Command + 7`

Select the eighth tab: `Command + 8`

Select the last tab: `Command + 9`

### Windows

Open configuration: `Control + Shift + Comma (,)`

Copy: `Control + Shift + C`

Paste: `Control + Shift + V`

Create new window: `Control + Shift + N`

Create new tab: `Control + Shift + T`

Move to next tab: `Control + Shift + RightBracket (])`

Move to previous tab: `Control + Shift + LeftBracket ([)`

Switch tabs by created order: `Control + Tab`

Increase font size: `Control + Plus (+)`

Decrease font size: `Control + Minus (-)`

Reset font size: `Control + 0`

Close tab or quit: `Control + Shift + W`

### Linux and BSD

Open configuration: `Control + Shift + Comma (,)`

Copy: `Control + Shift + C`

Paste: `Control + Shift + V`

Create new window: `Control + Shift + N`

Create new tab: `Control + Shift + T`

Move to next tab: `Control + Shift + RightBracket (])`

Move to previous tab: `Control + Shift + LeftBracket ([)`

Switch tabs by created order: `Control + Tab`

Increase font size: `Control + Plus (+)`

Decrease font size: `Control + Minus (-)`

Reset font size: `Control + 0`

Close tab or quit: `Control + Shift + W`

<br/>

## [Custom key bindings](#custom-key-bindings)

Rio also allow you to add key bindings per configuration or ovewritte any default key bindings listed above.

To achieve it you will need to change your configuration file with the key binding rules.

```toml
[bindings]
keys = [
	{ key = "q", with = "super", action = "Quit" }
	# Bytes[27, 91, 53, 126] is equivalent to "\x1b[5~"
	{ key = "home", with = "super | shift", bytes = [27, 91, 53, 126] }
]
```

### [Key](#key)

Each value in key binding will specify an identifier of the key pressed:

- `a-z`
- `0-9`
- `F1-F24`
- `tab` `esc`
- `home` `space` `delete` `insert` `pageup` `pagedown` `end`  `back`
- `up` `down` `left` `right`
- `@` `colon` `.` `return` `[` `]` `;` `\\` `+` `,` `/` `=` `-` `*`
- `numpadenter` `numpadadd` `numpadcomma` `numpaddivide` `numpadequals` `numpadsubtract` `numpadmultiply`
- `numpad1` `numpad2` `numpad3` `numpad4` `numpad5` `numpad6` `numpad7` `numpad8` `numpad9` `numpad0`

### [Action](#action)

Execute a predefined action in Rio terminal.

#### [Basic Actions](#basic-actions)

| Action | Description |
| :-- | :-- |
| None | |
| ReceiveChar | |
| Paste | Paste command |
| Copy | |
| OpenConfigEditor | |
| ResetFontSize | |
| IncreaseFontSize | |
| DecreaseFontSize | |

#### [Window Actions](#window-actions)

| Action | Description |
| :-- | :-- |
| CreateWindow | |
| Quit | |

#### [Pane Actions](#pane-actions)

| Action | Description |
| :-- | :-- |
| SplitHorizontally | |
| SplitVertically | |
| ClosePane | |

#### [Tab Actions](#tab-actions)

| Action | Description |
| :-- | :-- |
| CreateTab | |
| CloseTab | |
| SelectPrevTab | |
| SelectNextTab | |
| SelectTab1 | |
| SelectTab2 | |
| SelectTab3 | |
| SelectTab4 | |
| SelectTab5 | |
| SelectTab6 | |
| SelectTab7 | |
| SelectTab8 | |
| SelectTab9 | |
| SelectLastTab | |

#### [Scroll Actions](#scroll-actions)

| Action | Description |
| :-- | :-- |
| ScrollPageUp | |
| ScrollPageDown | |
| ScrollHalfPageUp | |
| ScrollHalfPageDown | |
| ScrollToTop | |
| ScrollToBottom | |
| ScrollLineUp | |
| ScrollLineDown | |

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

```bash
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

### [Overwriting](#overwriting)

Bindings are always filled by default, but will be replaced when a new binding with the same triggers is defined.  To unset a default binding, it can be mapped to the `ReceiveChar` action. Alternatively, you can use `None` for a no-op if you do not wish to receive input characters for that binding.

The example below will disable window creation binding in the macos:

```toml
[bindings]
keys = [
   { key = "n", with = "super", action = "ReceiveChar" }
}
```

`ReceiveChar` will treat the binding as non existent and simply receive the input and put the character into the terminal.

Optionally you can ignore/disable completely a binding using `None`. In the example below, whenever you use key "n" along with "super" key nothing will happen.

```toml
[bindings]
keys = [
   { key = "n", with = "super", action = "None" }
}
```

If you are missing a key binding that you believe that should be a default in the platform that you are using, feel free to [open an issue](https://github.com/raphamorim/rio).
