---
layout: docs
class: docs
title: 'Key Bindings'
language: 'en'
---

## Key bindings

Summary

- [• MacOS](#macos)
- [• Windows](#windows)
- [• Linux / BSD](#linux-and-bsd)
- [• Custom key bindings](#custom-key-bindings)

### MacOS

Open configuration: <span class="keyword">Command + Comma (,)</span> 

Copy: <span class="keyword">Command + C</span>

Paste: <span class="keyword">Command + V</span>

Create new window: <span class="keyword">Command + N</span>

Create new tab: <span class="keyword">Command + T</span>

Move to next tab: <span class="keyword">Command + Shift + RightBracket (])</span>

Move to previous tab: <span class="keyword">Command + Shift + LeftBracket ([)</span> or <span class="keyword">Control + Shift + Tab</span>

Switch tabs by created order: <span class="keyword">Control + Tab</span>

Increase font size: <span class="keyword">Command + Plus (+)</span>

Decrease font size: <span class="keyword">Command + Minus (-)</span>

Reset font size: <span class="keyword">Command + 0</span>

Minimize window: <span class="keyword">Command + M</span>

Quit: <span class="keyword">Command + Q</span>

Close tab: <span class="keyword">Command + W</span>

Select the first tab: <span class="keyword">Command + 1</span>

Select the second tab: <span class="keyword">Command + 2</span>

Select the third tab: <span class="keyword">Command + 3</span>

Select the fourth tab: <span class="keyword">Command + 4</span>

Select the fifth tab: <span class="keyword">Command + 5</span>

Select the sixth tab: <span class="keyword">Command + 6</span>

Select the seventh tab: <span class="keyword">Command + 7</span>

Select the eighth tab: <span class="keyword">Command + 8</span>

Select the last tab: <span class="keyword">Command + 9</span>

### Windows

Open configuration: <span class="keyword">Control + Shift + Comma (,)</span>

Copy: <span class="keyword">Control + Shift + C</span>

Paste: <span class="keyword">Control + Shift + V</span>

Create new window: <span class="keyword">Control + Shift + N</span>

Create new tab: <span class="keyword">Control + Shift + T</span>

Move to next tab: <span class="keyword">Control + Shift + RightBracket (])</span>

Move to previous tab: <span class="keyword">Control + Shift + LeftBracket ([)</span>

Switch tabs by created order: <span class="keyword">Control + Tab</span>

Increase font size: <span class="keyword">Control + Plus (+)</span>

Decrease font size: <span class="keyword">Control + Minus (-)</span>

Reset font size: <span class="keyword">Control + 0</span>

Close tab or quit: <span class="keyword">Control + Shift + W</span>

### Linux and BSD

Open configuration: <span class="keyword">Control + Shift + Comma (,)</span>

Copy: <span class="keyword">Control + Shift + C</span>

Paste: <span class="keyword">Control + Shift + V</span>

Create new window: <span class="keyword">Control + Shift + N</span>

Create new tab: <span class="keyword">Control + Shift + T</span>

Move to next tab: <span class="keyword">Control + Shift + RightBracket (])</span>

Move to previous tab: <span class="keyword">Control + Shift + LeftBracket ([)</span>

Switch tabs by created order: <span class="keyword">Control + Tab</span>

Increase font size: <span class="keyword">Control + Plus (+)</span>

Decrease font size: <span class="keyword">Control + Minus (-)</span>

Reset font size: <span class="keyword">Control + 0</span>

Close tab or quit: <span class="keyword">Control + Shift + W</span>

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

- <span class="keyword">a-z</span>
- <span class="keyword">0-9</span>
- <span class="keyword">F1-F24</span>
- <span class="keyword">tab</span> <span class="keyword">esc</span>
- <span class="keyword">home</span> <span class="keyword">space</span> <span class="keyword">delete</span> <span class="keyword">insert</span> <span class="keyword">pageup</span> <span class="keyword">pagedown</span> <span class="keyword">end</span>  <span class="keyword">back</span> 
- <span class="keyword">up</span> <span class="keyword">down</span> <span class="keyword">left</span> <span class="keyword">right</span>
- <span class="keyword">@</span> <span class="keyword">colon</span> <span class="keyword">.</span> <span class="keyword">return</span> <span class="keyword">[</span> <span class="keyword">]</span> <span class="keyword">;</span> <span class="keyword">\\</span> <span class="keyword">+</span> <span class="keyword">,</span> <span class="keyword">/</span> <span class="keyword">=</span> <span class="keyword">-</span> <span class="keyword">*</span>
- <span class="keyword">numpadenter</span> <span class="keyword">numpadadd</span> <span class="keyword">numpadcomma</span> <span class="keyword">numpaddivide</span> <span class="keyword">numpadequals</span> <span class="keyword">numpadsubtract</span> <span class="keyword">numpadmultiply</span>
- <span class="keyword">numpad1</span> <span class="keyword">numpad2</span> <span class="keyword">numpad3</span> <span class="keyword">numpad4</span> <span class="keyword">numpad5</span> <span class="keyword">numpad6</span> <span class="keyword">numpad7</span> <span class="keyword">numpad8</span> <span class="keyword">numpad9</span> <span class="keyword">numpad0</span>

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

The <span class="keyword">bytes</span> field writes the specified string to the terminal. This makes
it possible to pass escape sequences, like <span class="keyword">PageUp</span> ("\x1b[5~"). Note that applications use terminfo to map escape sequences back
to keys. It is therefore required to update the terminfo when changing an escape sequence.

### [With](#with)

Key modifiers to filter binding actions

- <span class="keyword">none</span>
- <span class="keyword">control</span>
- <span class="keyword">option</span>
- <span class="keyword">super</span>
- <span class="keyword">shift</span>
- <span class="keyword">alt</span>

Multiple modifiers can be combined using <span class="keyword">|</span> like this:

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

Bindings are always filled by default, but will be replaced when a new binding with the same triggers is defined.  To unset a default binding, it can be mapped to the <span class="keyword">ReceiveChar</span> action. Alternatively, you can use <span class="keyword">None</span> for a no-op if you do not wish to receive input characters for that binding.

The example below will disable window creation binding in the macos:

```toml
[bindings]
keys = [
   { key = "n", with = "super", action = "ReceiveChar" }
}
```

<span class="keyword">ReceiveChar</span> will treat the binding as non existent and simply receive the input and put the character into the terminal.

Optionally you can ignore/disable completely a binding using <span class="keyword">None</span>. In the example below, whenever you use key "n" along with "super" key nothing will happen.

```toml
[bindings]
keys = [
   { key = "n", with = "super", action = "None" }
}
```

If you are missing a key binding that you believe that should be a default in the platform that you are using, feel free to [open an issue](https://github.com/raphamorim/rio).
