---
layout: docs
class: docs
title: 'Documentation'
language: 'en'
---

## Create custom key bindings

<!-- Key bindings are specified as a list of objects. For example, this is the

default paste binding:

`- { key: V, mods: Control|Shift, action: Paste }`

Each key binding will specify a:

- `key`: Identifier of the key pressed

   - A-Z

   - F1-F24

   - Key0-Key9

A full list with available key codes can be found here:
https://docs.rs/glutin/*/glutin/event/enum.VirtualKeyCode.html#variants

Instead of using the name of the keys, the `key` field also supports using
the scancode of the desired key. Scancodes have to be specified as a decimal number. This command will allow you to display the hex scancodes for certain keys: `showkey --scancodes`.

Then exactly one of:
 - `chars`: Send a byte sequence to the running application
    The `chars` field writes the specified string to the terminal. This makes
    it possible to pass escape sequences. To find escape codes for bindings
    like `PageUp` (`"\x1b[5~"`), you can run the command `showkey -a` outside
    of tmux. Note that applications use terminfo to map escape sequences back
    to keys. It is therefore required to update the terminfo when changing an
    escape sequence.
 - `action`: Execute a predefined action
   - Copy
   - Paste
   - PasteSelection
   - IncreaseFontSize
   - DecreaseFontSize
   - ResetFontSize
   - ScrollPageUp
   - ScrollPageDown
   - ScrollLineUp
   - ScrollLineDown
   - ScrollToTop
   - ScrollToBottom
   - ClearHistory
   - Hide
   - Minimize
   - Quit
   - ToggleFullscreen
   - SpawnNewInstance
   - ClearLogNotice
   - ReceiveChar
   - None
   (macOS only):
   - ToggleSimpleFullscreen: Enters fullscreen without occupying another space
 - `command`: Fork and execute a specified command plus arguments
    The `command` field must be a map containing a `program` string and an
    `args` array of command line parameter strings. For example:
       `{ program: "alacritty", args: ["-e", "vttest"] }`
 And optionally:
 - `mods`: Key modifiers to filter binding actions
    - Command
    - Control
    - Option
    - Super
    - Shift
    - Alt
    Multiple `mods` can be combined using `|` like this:
       `mods: Control|Shift`.
    Whitespace and capitalization are relevant and must match the example.
 - `mode`: Indicate a binding for only specific terminal reported modes
    This is mainly used to send applications the correct escape sequences
    when in different modes.
    - AppCursor
    - AppKeypad
    - Alt
    A `~` operator can be used before a mode to apply the binding whenever
    the mode is *not* active, e.g. `~Alt`.
 Bindings are always filled by default, but will be replaced when a new
 binding with the same triggers is defined. To unset a default binding, it can
 be mapped to the `ReceiveChar` action. Alternatively, you can use `None` for a no-op if you do not wish to receive input characters for that binding.

--

[Move to plugins ->](/rio/docs/plugins#plugins) -->