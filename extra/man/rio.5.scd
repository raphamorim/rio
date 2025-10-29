RIO(5)

# NAME

Rio - TOML configuration file format.

# SYNTAX

Rio's configuration file uses the TOML format. The format's specification
can be found at _https://toml.io/en/v1.0.0_.

# LOCATION

Rio doesn't create the config file for you, but it looks for one in the
following locations on UNIX systems:

. _$RIO_CONFIG_HOME/config.toml_
. _$XDG_CONFIG_HOME/rio/config.toml_
. _$HOME/.config/rio/config.toml_

On Windows, the config file will be looked for in:

. _%USERPROFILE%\\AppData\\Local\\rio\\config.toml_

# GENERAL

*theme* = _"<string>"_

	Theme name to use. When set, colors defined in the configuration file will be ignored.

	Default: _None_

*confirm-before-quit* = _true_ | _false_

	Require confirmation before quitting.

	Default: _true_

*draw-bold-text-with-light-colors* = _true_ | _false_

	When _true_, bold text is drawn using the bright color variants.

	Default: _false_

*env-vars* = [_"<string>"_,]

	Environment variables to set for spawned processes.

	Example:
		env-vars = [_"TERM=xterm-256color"_, _"COLORTERM=truecolor"_]

# WINDOW

This section documents the *[window]* table of the configuration file.

*width* = _<integer>_

	Window width in pixels.

	Default: _600_

*height* = _<integer>_

	Window height in pixels.

	Default: _400_

*mode* = _"Windowed"_ | _"Maximized"_ | _"Fullscreen"_

	Window startup mode.

	Default: _"Windowed"_

*opacity* = _<float>_

	Background opacity as a floating point number from _0.0_ to _1.0_.

	Default: _1.0_

*blur* = _true_ | _false_

	Request compositor to blur content behind transparent windows.

	Default: _false_

*decorations* = _"Enabled"_ | _"Disabled"_ | _"Transparent"_ | _"Buttonless"_

	Window decorations.

	*Enabled*
		Normal window decorations.
	*Disabled*
		No window decorations.
	*Transparent* _(macOS only)_
		Transparent title bar.
	*Buttonless* _(macOS only)_
		Title bar without buttons.

	Default: _"Enabled"_

*background-image* = { path = _"<string>"_, opacity = _<float>_, width = _<float>_, height = _<float>_, x = _<float>_, y = _<float>_ }

	Background image configuration.

	*path*
		Path to the background image file.
	*opacity*
		Image opacity from _0.0_ to _1.0_.
	*width*, *height*
		Image dimensions as percentage of window size.
	*x*, *y*
		Image position as percentage of window size.

# FONTS

This section documents the *[fonts]* table of the configuration file.

*size* = _<float>_

	Font size in points.

	Default: _18.0_

*family* = _"<string>"_

	Font family name. When set, overrides all font variant families.

	Default: _"cascadiacode"_

*features* = [_"<string>"_,]

	OpenType font features to enable.

	Example:
		features = [_"ss02"_, _"ss03"_, _"liga"_]

*use-drawable-chars* = _true_ | _false_

	Use built-in drawable characters for box drawing.

	Default: _true_

*disable-warnings-not-found* = _true_ | _false_

	Disable warnings when fonts are not found.

	Default: _false_

*additional-dirs* = [_"<string>"_,]

	Additional directories to search for fonts.

*regular* = { family = _"<string>"_, style = _"<string>"_, width = _"<string>"_, weight = _<integer>_ }

	Regular font configuration.

	Default: { family = _"cascadiacode"_, style = _"Normal"_, width = _"Normal"_, weight = _400_ }

*bold* = { family = _"<string>"_, style = _"<string>"_, width = _"<string>"_, weight = _<integer>_ }

	Bold font configuration.

	Default: { family = _"cascadiacode"_, style = _"Normal"_, width = _"Normal"_, weight = _800_ }

*italic* = { family = _"<string>"_, style = _"<string>"_, width = _"<string>"_, weight = _<integer>_ }

	Italic font configuration.

	Default: { family = _"cascadiacode"_, style = _"Italic"_, width = _"Normal"_, weight = _400_ }

*bold-italic* = { family = _"<string>"_, style = _"<string>"_, width = _"<string>"_, weight = _<integer>_ }

	Bold italic font configuration.

	Default: { family = _"cascadiacode"_, style = _"Italic"_, width = _"Normal"_, weight = _800_ }

*extras* = [{ family = _"<string>"_ },]

	Additional fonts to load.

*emoji* = { family = _"<string>"_ }

	Emoji font configuration.

	Default: Built-in Twemoji font

*hinting* = _true_ | _false_

	Enable font hinting.

	Default: _true_

*symbol-map* = [{ chars = _"<string>"_, family = _"<string>"_ },]

	Map specific character ranges to fonts.

# COLORS

This section documents the *[colors]* table of the configuration file.

Colors are specified using their hexadecimal values with a _#_ prefix: _#RRGGBB_ or _#RRGGBBAA_.

*background* = _"<string>"_

	Default background color.

*foreground* = _"<string>"_

	Default foreground color.

*cursor* = _"<string>"_

	Cursor color.

*vi-cursor* = _"<string>"_

	Vi mode cursor color.

*black*, *red*, *green*, *yellow*, *blue*, *magenta*, *cyan*, *white* = _"<string>"_

	Standard ANSI colors.

*light-black*, *light-red*, *light-green*, *light-yellow*, *light-blue*, *light-magenta*, *light-cyan*, *light-white* = _"<string>"_

	Bright ANSI colors.

*dim-black*, *dim-red*, *dim-green*, *dim-yellow*, *dim-blue*, *dim-magenta*, *dim-cyan*, *dim-white* = _"<string>"_

	Dim ANSI colors.

*tabs* = _"<string>"_

	Tab bar background color.

*tabs-foreground* = _"<string>"_

	Tab bar text color.

*tabs-active* = _"<string>"_

	Active tab background color.

*tabs-active-foreground* = _"<string>"_

	Active tab text color.

*tabs-active-highlight* = _"<string>"_

	Active tab highlight color.

*bar* = _"<string>"_

	Navigation bar background color.

*split* = _"<string>"_

	Split separator color.

*search-match-background* = _"<string>"_

	Search match background color.

*search-match-foreground* = _"<string>"_

	Search match text color.

*search-focused-match-background* = _"<string>"_

	Focused search match background color.

*search-focused-match-foreground* = _"<string>"_

	Focused search match text color.

*selection-background* = _"<string>"_

	Selection background color.

*selection-foreground* = _"<string>"_

	Selection text color.

# CURSOR

This section documents the *[cursor]* table of the configuration file.

*shape* = _"block"_ | _"underline"_ | _"beam"_

	Cursor shape.

	Default: _"block"_

*blinking* = _true_ | _false_

	Enable cursor blinking.

	Default: _false_

*blinking-interval* = _<integer>_

	Cursor blinking interval in milliseconds. Range: 350-1200.

	Default: _800_

# SCROLL

This section documents the *[scroll]* table of the configuration file.

*multiplier* = _<float>_

	Scroll speed multiplier.

	Default: _3.0_

*divider* = _<float>_

	Scroll divider for fine-tuning.

	Default: _1.0_

# NAVIGATION

This section documents the *[navigation]* table of the configuration file.

*mode* = _"NativeTab"_ | _"TopTab"_ | _"BottomTab"_ | _"Breadcrumb"_ | _"Plain"_

	Navigation display mode.

	Default: _"NativeTab"_

*clickable* = _true_ | _false_

	Enable clickable navigation elements.

	Default: _false_

*use-current-path* = _true_ | _false_

	Show current working directory in navigation.

	Default: _false_

*color-automation* = [{ program = _"<string>"_, path = _"<string>"_, color = _"<string>"_ },]

	Automatic color assignment based on program or path.

# SHELL

This section documents the *[shell]* table of the configuration file.

*program* = _"<string>"_

	Shell program to execute.

	Default: System default shell

*args* = [_"<string>"_,]

	Arguments to pass to the shell.

# EDITOR

This section documents the *[editor]* table of the configuration file.

*program* = _"<string>"_

	Editor program for opening configuration files.

	Default: _"vi"_

*args* = [_"<string>"_,]

	Arguments to pass to the editor.

# BINDINGS

This section documents the *[bindings]* table of the configuration file.

*keys* = [{ key = _"<string>"_, with = _"<string>"_, action = _"<string>"_ },]

	Key bindings configuration.

	*key*
		Key name (e.g., _"q"_, _"Enter"_, _"F1"_).

	*with*
		Modifier keys: _"super"_, _"alt"_, _"ctrl"_, _"shift"_.
		Multiple modifiers can be combined with _|_.

	*action*
		Action to perform. Available actions include:
		_"Quit"_, _"CreateWindow"_, _"CloseWindow"_, _"Copy"_, _"Paste"_,
		_"ScrollUp"_, _"ScrollDown"_, _"ToggleViMode"_, _"OpenConfigEditor"_,
		and many others.

# RENDERER

This section documents the *[renderer]* table of the configuration file.

*performance* = _"High"_ | _"Low"_

	Rendering performance mode.

	Default: _"High"_

*backend* = _"Automatic"_ | _"GL"_ | _"Vulkan"_ | _"DX12"_ | _"Metal"_

	Graphics backend to use.

	Default: _"Automatic"_

*disable-renderer-when-unfocused* = _true_ | _false_

	Disable rendering when window is unfocused to save power.

	Default: _false_

*level* = _<integer>_

	Rendering level (0-3).

	Default: _3_

# DEVELOPER

This section documents the *[developer]* table of the configuration file.

*log-level* = _"OFF"_ | _"ERROR"_ | _"WARN"_ | _"INFO"_ | _"DEBUG"_ | _"TRACE"_

	Log level for debugging.

	Default: _"OFF"_

*enable-log-file* = _true_ | _false_

	Write logs to file.

	Default: _false_

*enable-fps-counter* = _true_ | _false_

	Display FPS counter.

	Default: _false_

# SEE ALSO

*rio*(1)

# BUGS

Found a bug? Please report it at _https://github.com/raphamorim/rio/issues_.

# MAINTAINERS

- Raphael Amorim <rapha850@gmail.com>
