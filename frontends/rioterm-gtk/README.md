# rioterm-gtk

Rio as a GTK 4 / libadwaita app, an alternative to `rioterm` on Linux.

- The terminal is [rio-vt](../../rio-vt), the same state machine, PTY
  driver and input encoding Rio runs.
- It is drawn the way Rio draws it, rio-grid's cells on Sugarloaf's CPU
  renderer, into a frame the terminal widget hands GTK as one texture per
  row. Only the rows that changed are repainted and uploaded; when the
  terminal scrolls, the rows it keeps are moved, in the frame and as
  textures, rather than drawn again.
- The window is GNOME's own: `AdwApplicationWindow`, with `AdwTabBar` tabs
  that reorder, drag out into a new window or into another window, and have
  a context menu.
- Shells start with Rio's shell integration (`shell-integration`): the
  working directory follows `cd` (OSC 7), and zsh and fish mark prompts
  and commands (OSC 133), which `notify-on-command-finish` uses.
- Images show as in Rio: kitty graphics (with virtual placements),
  sixel and iTerm2 inline images.
- Settings come from Rio's config file and apply as soon as it is saved:
  fonts, colors (with `adaptive-colors` following the desktop's light or
  dark style, or `force-theme`), cursor, shell, working directory,
  padding, scrollback, window size, mode, decorations and opacity, and
  `navigation.hide-if-single` for the tab bar. The window's own style
  follows the terminal's background.

## Building

Needs GTK 4.6+ and libadwaita 1.1+ with their development files:

```sh
# Debian / Ubuntu
sudo apt install libgtk-4-dev libadwaita-1-dev
# Fedora
sudo dnf install gtk4-devel libadwaita-devel
# macOS (for development)
brew install gtk4 libadwaita
```

It isn't a default workspace member, so plain `cargo build` skips it:

```sh
cargo run -p rioterm-gtk
```

## Shortcuts

| | |
|---|---|
| New tab / window | Ctrl+Shift+T / Ctrl+Shift+N |
| Close split or tab | Ctrl+Shift+W |
| Split right / down | Ctrl+Shift+R / Ctrl+Shift+D |
| Next / previous split (or tab without splits) | Ctrl+Shift+] / Ctrl+Shift+[ |
| Move a divider | Ctrl+Shift+Alt+Arrows |
| Next / previous tab | Ctrl+Tab, Ctrl+PageDown / Ctrl+Shift+Tab, Ctrl+PageUp |
| Move tab left / right | Ctrl+Shift+PageUp / Ctrl+Shift+PageDown |
| Tab 1–8 / last tab | Alt+1–8 / Alt+9 |
| Copy / paste | Ctrl+Shift+C / Ctrl+Shift+V |
| Paste the primary selection | Shift+Insert, middle click |
| Select all | Ctrl+Shift+A |
| Fullscreen | F11 |
| Find (Enter / Shift+Enter for older / newer matches) | Ctrl+Shift+F |
| Scroll a page, to the top or bottom | Shift+PageUp / PageDown / Home / End |
| Font size bigger / smaller / reset | Ctrl+= / Ctrl+- / Ctrl+0 |

Selecting text copies it to the primary selection (and to the clipboard
with `copy-on-select`). Pastes that could run commands are confirmed
first, as in Rio (`paste-protection`). New tabs open in the current
tab's directory (`navigation.current-working-directory`).

Links work as in Rio's hints (`[hints]`): with a rule's modifiers held,
Alt by default, the URL, path or OSC 8 link under the pointer is
underlined, and clicking it runs the rule's action (open, copy, paste,
select or a command).
