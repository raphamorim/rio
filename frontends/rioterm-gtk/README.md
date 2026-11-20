# rioterm-gtk

Rio as a GTK 4 / libadwaita app, an alternative to `rioterm` on Linux.

- The terminal is [librio](../../librio): PTY, parser, state and input
  encoding, the same core Rio runs.
- It is drawn by Sugarloaf's CPU renderer, through
  [libsugarloaf](../../libsugarloaf), into a frame the terminal widget hands
  GTK as a texture, so text renders exactly as in Rio.
- The window is GNOME's own: `AdwApplicationWindow`, with `AdwTabBar` tabs
  that reorder, drag out into a new window or into another window, and have
  a context menu.
- Settings come from Rio's config file (fonts, colors, shell, working
  directory, padding, scrollback).

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
| Close tab | Ctrl+Shift+W |
| Next / previous tab | Ctrl+PageDown / Ctrl+PageUp |
| Move tab left / right | Ctrl+Shift+PageUp / Ctrl+Shift+PageDown |
| Tab 1–8 / last tab | Alt+1–8 / Alt+9 |
| Copy / paste | Ctrl+Shift+C / Ctrl+Shift+V |

Selecting text copies it to the primary selection; middle-click pastes it.
