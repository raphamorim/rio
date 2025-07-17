RIO(1)

# NAME

Rio - A cross-platform, GPU-accelerated terminal emulator.

# SYNOPSIS

Rio is a modern terminal emulator built with Rust and WebGPU that provides high performance rendering with support for modern terminal features including sixel graphics, hyperlinks, and custom shaders.

# FLAGS

*-h, --help*

	Prints help information.

*-V, --version*

	Prints version information.

# OPTIONS

*-e, --command* _<COMMAND>..._

	Command and args to execute (must be last argument).

*-w, --working-dir* _<WORKING_DIR>_

	Start the shell in the specified working directory.

*--write-config* _[<PATH>]_

	Writes the config to a given path or the default location.

	Rio doesn't create the config file for you, but it looks for one in the
	following locations on UNIX systems:

		. _$RIO_CONFIG_HOME/config.toml_
		. _$XDG_CONFIG_HOME/rio/config.toml_
		. _$HOME/.config/rio/config.toml_

	On Windows, the config file will be looked for in:

		. _%USERPROFILE%\\AppData\\Local\\rio\\config.toml_

*--enable-log-file*

	Writes the logs to a file inside the config directory.

*--title-placeholder* _<TITLE>_

	Start window with specified title.

# CONFIGURATION

Rio uses TOML format for configuration. The configuration file is automatically
reloaded when changed. See *rio*(5) for detailed configuration documentation.

# FEATURES

Rio supports many modern terminal features:

- GPU-accelerated rendering with WebGPU
- Sixel graphics protocol
- iTerm2 image protocol
- Kitty keyboard protocol
- Hyperlinks (OSC 8)
- Vi mode for navigation
- Split panels
- Custom shaders (RetroArch compatible)
- Adaptive themes
- Font ligatures
- Wide color gamut support

# ENVIRONMENT

*RIO_CONFIG_HOME*

	Custom path for Rio configuration directory. When set, Rio will look for
	configuration files in this directory instead of the default locations.

# SEE ALSO

*rio*(5)

# BUGS

Found a bug? Please report it at _https://github.com/raphamorim/rio/issues_.

# MAINTAINERS

- Raphael Amorim <rapha@raphamorim.io>
