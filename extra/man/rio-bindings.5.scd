RIO-BINDINGS(5)

# NAME

Rio-bindings - Default key bindings for Rio terminal emulator.

# DESCRIPTION

This page documents the default key bindings for Rio terminal emulator. Key bindings can be customized in the configuration file under the *[bindings]* section.

# SYNTAX

Key bindings use the following syntax in the configuration file:

```
[[bindings.keys]]
key = "<key>"
with = "<modifiers>"
action = "<action>"
```

Where:
- *key* is the key name (e.g., "q", "Enter", "F1")
- *with* specifies modifier keys: "super", "alt", "ctrl", "shift"
- *action* is the action to perform

Multiple modifiers can be combined with "|", for example: "ctrl|shift".

# DEFAULT BINDINGS

## Application Control

*Super+Q* (macOS) / *Ctrl+Shift+Q* (Linux/Windows)
	Quit Rio.

*Super+N* (macOS) / *Ctrl+Shift+N* (Linux/Windows)
	Create new window.

*Super+W* (macOS) / *Ctrl+Shift+W* (Linux/Windows)
	Close current window.

*Super+T* (macOS) / *Ctrl+Shift+T* (Linux/Windows)
	Create new tab.

*Super+Comma* (macOS) / *Ctrl+Comma* (Linux/Windows)
	Open configuration editor.

## Text Operations

*Super+C* (macOS) / *Ctrl+Shift+C* (Linux/Windows)
	Copy selected text to clipboard.

*Super+V* (macOS) / *Ctrl+Shift+V* (Linux/Windows)
	Paste from clipboard.

*Super+A* (macOS) / *Ctrl+Shift+A* (Linux/Windows)
	Select all text.

## Font Size

*Super+Plus* (macOS) / *Ctrl+Plus* (Linux/Windows)
	Increase font size.

*Super+Minus* (macOS) / *Ctrl+Minus* (Linux/Windows)
	Decrease font size.

*Super+0* (macOS) / *Ctrl+0* (Linux/Windows)
	Reset font size to default.

## Scrolling

*Page Up*
	Scroll up one page.

*Page Down*
	Scroll down one page.

*Shift+Page Up*
	Scroll up half page.

*Shift+Page Down*
	Scroll down half page.

*Home*
	Scroll to top of buffer.

*End*
	Scroll to bottom of buffer.

## Navigation

*Super+1-9* (macOS) / *Ctrl+1-9* (Linux/Windows)
	Switch to tab 1-9.

*Super+Left* (macOS) / *Ctrl+Shift+Left* (Linux/Windows)
	Switch to previous tab.

*Super+Right* (macOS) / *Ctrl+Shift+Right* (Linux/Windows)
	Switch to next tab.

## Vi Mode

*Ctrl+Shift+Space*
	Toggle Vi mode for keyboard navigation.

*h*, *j*, *k*, *l*
	Move cursor left, down, up, right.

*w*
	Move to start of next word.

*b*
	Move to start of previous word.

*e*
	Move to end of current word.

*0*
	Move to beginning of line.

*$*
	Move to end of line.

*g*, *g*
	Move to top of buffer.

*G*
	Move to bottom of buffer.

*/*
	Start forward search.

*?*
	Start backward search.

*n*
	Jump to next search match.

*N*
	Jump to previous search match.

*v*
	Start visual selection.

*V*
	Start line selection.

*Ctrl+V*
	Start block selection.

*y*
	Copy selection.

*Escape*
	Exit Vi mode.

## Search

*Super+F* (macOS) / *Ctrl+Shift+F* (Linux/Windows)
	Start search.

*Enter*
	Confirm search.

*Escape*
	Cancel search.

*Ctrl+G* / *F3*
	Find next match.

*Ctrl+Shift+G* / *Shift+F3*
	Find previous match.

## Split Panels

*Super+D* (macOS) / *Ctrl+Shift+D* (Linux/Windows)
	Split panel horizontally.

*Super+Shift+D* (macOS) / *Ctrl+Shift+Alt+D* (Linux/Windows)
	Split panel vertically.

*Super+Shift+W* (macOS) / *Ctrl+Shift+Alt+W* (Linux/Windows)
	Close current split panel.

*Super+H/J/K/L* (macOS) / *Ctrl+Shift+H/J/K/L* (Linux/Windows)
	Navigate between split panels.

## Developer

*F12*
	Toggle developer tools (when available).

*Ctrl+Shift+I*
	Open inspector (when available).

# CUSTOMIZATION

Key bindings can be customized by adding entries to the *[bindings]* section in the configuration file:

```
[[bindings.keys]]
key = "q"
with = "super"
action = "Quit"

[[bindings.keys]]
key = "Enter"
with = "ctrl|shift"
action = "CreateWindow"
```

# AVAILABLE ACTIONS

*Quit*
	Quit the application.

*CreateWindow*
	Create a new window.

*CloseWindow*
	Close the current window.

*CreateTab*
	Create a new tab.

*CloseTab*
	Close the current tab.

*SelectTab1* through *SelectTab9*
	Switch to specific tab.

*SelectPreviousTab*
	Switch to previous tab.

*SelectNextTab*
	Switch to next tab.

*Copy*
	Copy selected text.

*Paste*
	Paste from clipboard.

*SelectAll*
	Select all text.

*IncreaseFontSize*
	Increase font size.

*DecreaseFontSize*
	Decrease font size.

*ResetFontSize*
	Reset font size to default.

*ScrollUp*
	Scroll up.

*ScrollDown*
	Scroll down.

*ScrollPageUp*
	Scroll up one page.

*ScrollPageDown*
	Scroll down one page.

*ScrollHalfPageUp*
	Scroll up half page.

*ScrollHalfPageDown*
	Scroll down half page.

*ScrollToTop*
	Scroll to top of buffer.

*ScrollToBottom*
	Scroll to bottom of buffer.

*ToggleViMode*
	Toggle Vi mode.

*Search*
	Start search.

*OpenConfigEditor*
	Open configuration file in editor.

*SplitHorizontally*
	Split panel horizontally.

*SplitVertically*
	Split panel vertically.

*ClosePanel*
	Close current split panel.

*FocusNextPanel*
	Focus next split panel.

*FocusPreviousPanel*
	Focus previous split panel.

# SEE ALSO

*rio*(1), *rio*(5)

# BUGS

Found a bug? Please report it at _https://github.com/raphamorim/rio/issues_.

# MAINTAINERS

- Raphael Amorim <rapha850@gmail.com>
