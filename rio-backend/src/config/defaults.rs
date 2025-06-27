use crate::{ansi::CursorShape, config::Shell};

#[inline]
pub fn default_bool_true() -> bool {
    true
}

#[inline]
pub fn default_line_height() -> f32 {
    1.0
}

#[inline]
pub fn default_cursor_interval() -> u64 {
    800
}

#[inline]
pub fn default_title_placeholder() -> Option<String> {
    Some(String::from("▲"))
}

#[inline]
pub fn default_title_content() -> String {
    String::from("{{ TITLE || PROGRAM }}")
}

#[inline]
pub fn default_padding_y() -> [f32; 2] {
    [0., 0.]
}

#[inline]
pub fn default_shell() -> crate::config::Shell {
    #[cfg(not(target_os = "windows"))]
    {
        crate::config::Shell {
            program: String::from(""),
            args: vec![String::from("--login")],
        }
    }

    #[cfg(target_os = "windows")]
    {
        crate::config::Shell {
            program: String::from("powershell"),
            args: vec![],
        }
    }
}

#[inline]
pub fn default_use_fork() -> bool {
    #[cfg(target_os = "macos")]
    {
        false
    }

    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

#[inline]
pub fn default_working_dir() -> Option<String> {
    None
}

#[inline]
pub fn default_opacity() -> f32 {
    1.0
}

#[inline]
pub fn default_option_as_alt() -> String {
    String::from("none")
}

#[inline]
pub fn default_log_level() -> String {
    String::from("OFF")
}

#[inline]
pub fn default_cursor() -> CursorShape {
    CursorShape::default()
}

#[inline]
pub fn default_theme() -> String {
    String::from("")
}

#[inline]
pub fn default_editor() -> Shell {
    #[cfg(not(target_os = "windows"))]
    {
        Shell {
            program: String::from("vi"),
            args: vec![],
        }
    }

    #[cfg(target_os = "windows")]
    {
        Shell {
            program: String::from("notepad"),
            args: vec![],
        }
    }
}

#[inline]
pub fn default_window_width() -> i32 {
    800
}

#[inline]
pub fn default_window_height() -> i32 {
    490
}

#[inline]
pub fn default_disable_ctlseqs_alt() -> bool {
    #[cfg(target_os = "macos")]
    {
        true
    }

    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[inline]
pub fn default_ime_cursor_positioning() -> bool {
    true
}

pub fn default_config_file_content() -> String {
    r#"
# Hide the cursor while typing
#
# Default is `false`
#
# hide-cursor-when-typing = false

# Ignore theme selection foreground color
#
# Default is false
#
# Example:
# ignore-selection-foreground-color = false

# Theme
#
# It makes Rio look for the specified theme in the themes folder
# (macos: ~/.config/rio/themes/dracula.toml)
# (linux: $XDG_HOME_CONFIG/rio/themes/dracula.toml or ~/.config/rio/themes/dracula.toml)
# (windows: C:\Users\USER\AppData\Local\rio\themes\dracula.toml)
#
# Example:
# theme = "dracula"

# Padding-x
#
# define x axis padding (default is 0)
#
# Example:
# padding-x = 10

# Padding-y
#
# define y axis padding based on a format [top, bottom]
# (default is [0, 0])
#
# Example:
# padding-y = [30, 10]

# Option as Alt
#
# This config only works on MacOS.
# Possible choices: 'both', 'left' and 'right'.
#
# Example:
# option-as-alt = 'left'

# Line height
#
# This option will apply an modifier to line-height
# Default is `1.0`
#
# Example:
# line-height = 1.2

# Startup directory
#
# Directory the shell is started in. If this is unset the working
# directory of the parent process will be used.
#
# This configuration only has effect if use-fork is disabled.
#
# Example:
# working-dir = "/Users/raphael/Documents/"

# Environment variables
#
# Example:
# env-vars = []

# Use fork
#
# Defaults for POSIX-based systems (Windows is not configurable):
# MacOS: spawn processes
# Linux/BSD: fork processes
#
# Example:
# use-fork = false

# Confirm before exiting Rio
# Default is `true`
#
# confirm-before-quit = false

# Cursor
#
# shape - Default cursor shape is 'block'
# Other available options are: 'underline', 'beam' or 'hidden'
#
# blinking - Whether the cursor blinks. The default is false
#
# blinking-interval - Cursor update on milliseconds interval
#
# [cursor]
# shape = 'block'
# blinking = false
# blinking-interval = 800

# Editor
#
# Default editor on Linux and MacOS is "vi",
# on Windows it is "notepad".
#
# Whenever the key binding `OpenConfigEditor` is triggered it will
# use the value of the editor along with the rio configuration path.
# [editor]
# program = "vi"
# args = []

# Window configuration
#
# • width - define the initial window width.
#   Default: 600
#
# • height - define the initial window height.
#   Default: 400
#
# • mode - define how the window will be created
#     - "Windowed" (default) is based on width and height
#     - "Maximized" window is created with maximized
#     - "Fullscreen" window is created with fullscreen
#
# • opacity - Set window opacity
#
# • blur - Set blur on the window background. Changing this config requires restarting Rio to take effect.
#
# • decorations - Set window decorations, options: "Enabled", "Disabled", "Transparent", "Buttonless"
#
# • colorspace - Set the color space for the window
#     - "srgb" (default on non-macOS)
#     - "display-p3" (default on macOS)
#     - "rec2020"
#
# Example:
# [window]
# width = 600
# height = 400
# mode = "windowed"
# opacity = 1.0
# blur = false
# decorations = "enabled"
# colorspace = "display-p3"

# Renderer
#
# • Performance: Set WGPU rendering performance
#   - High: Adapter that has the highest performance. This is often a discrete GPU.
#   - Low: Adapter that uses the least possible power. This is often an integrated GPU.
#
# • Backend: Set WGPU rendering backend
#   - Automatic: Leave Sugarloaf/WGPU to decide
#   - GL: Supported on Linux/Android, and Windows and macOS/iOS via ANGLE
#   - Vulkan: Supported on Windows, Linux/Android
#   - DX12: Supported on Windows 10
#   - Metal: Supported on macOS/iOS
#
# • disable-unfocused-render: This property disable renderer processes while Rio is unfocused.
#
# • level: Configure renderer level
#   - Available options: 0 and 1.
#       Higher the level more rendering features and computations
#       will be done like enable font ligatures or emoji support.
#       For more information please check the docs.
#
# • filters: A list of paths to RetroArch slang shaders. Might not work with OpenGL.
#
# Example:
# [renderer]
# performance = "high"
# backend = "automatic"
# disable-unfocused-render = false
# level = 1
# filters = []

# Keyboard
#
# use-kitty-keyboard-protocol - Enable Kitty Keyboard protocol
#
# disable-ctlseqs-alt - Disable ctlseqs with ALT keys
#   - For example: Terminal.app does not deal with ctlseqs with ALT keys
#
# ime-cursor-positioning - Enable IME cursor positioning
#   - When enabled, the IME input popup will appear at the cursor position
#   - Default is true
#
# Example:
# [keyboard]
# use-kitty-keyboard-protocol = false
# disable-ctlseqs-alt = false
# ime-cursor-positioning = true

# Fonts
#
# Configure fonts used by the terminal
#
# Note: You can set different font families but Rio terminal
# will always look for regular font bounds whene
#
# You can also set family on root to overwrite all fonts.
# [fonts]
# family = "cascadiamono"
#
# You can also specify extra fonts to load
# [fonts]
# extras = [{ family = "Microsoft JhengHei" }]
#
# In case you want to specify any font feature:
# [fonts]
# features = ["ss02", "ss03", "ss05", "ss19"]
#
# Note: Font features do not have support to live reload on configuration,
# so to reflect your changes, you will need to close and reopen Rio.
#
# You can also disable font hinting. Font hinting is enabled by default.
# [fonts]
# hinting = false
#
# You can also map the specified Unicode codepoints to a particular font.
# [fonts]
# symbol-map = [
#   { start = "2297", end = "2299", font-family = "Cascadia Code NF" }
# ]
#
# Simple example:
# [fonts]
# size = 18
#
# [fonts.regular]
# family = "cascadiamono"
# style = "Normal"
# weight = 400
#
# [fonts.bold]
# family = "cascadiamono"
# style = "Normal"
# weight = 800
#
# [fonts.italic]
# family = "cascadiamono"
# style = "Italic"
# weight = 400
#
# [fonts.bold-italic]
# family = "cascadiamono"
# style = "Italic"
# weight = 800

# Scroll
#
# You can change how many lines are scrolled each time by setting this option.
#
# Scroll calculation for canonical mode will be based on `lines = (accumulated scroll * multiplier / divider)`,
# If you want a quicker scroll, keep increasing the multiplier.
# If you want to reduce scroll speed you will need to increase the divider.
# You can use both properties also to find the best scroll for you.
#
# Multiplier default is 3.0.
# Divider default is 1.0.
# Example:
# [scroll]
# multiplier = 3.0
# divider = 1.0

# Navigation
#
# "mode" - Define navigation mode
#   • NativeTab (MacOS only)
#   • Bookmark
#   • BottomTab
#   • TopTab
#   • Plain
#
# "hide-if-single" - Hide navigation UI if is single.
# "clickable" - Enable click on tabs to switch.
# "use-current-path" - Use same path whenever a new tab is created (Note: requires `use-fork` to be set to false).
# "color-automation" - Set a specific color for the tab whenever a specific program is running, or in a specific directory.
#
# Example:
# [navigation]
# mode = "bookmark"
# clickable = false
# hide-if-single = true
# use-current-path = false
# color-automation = []

# Shell
#
# You can set `shell.program` to the path of your favorite shell, e.g. `/bin/fish`.
# Entries in `shell.args` are passed unmodified as arguments to the shell.
#
# Default:
#   - (macOS) user login shell
#   - (Linux/BSD) user login shell
#   - (Windows) powershell
#
# Example 1 using fish shell from bin path:
#
# [shell]
# program = "/bin/fish"
# args = ["--login"]
#
# Example 2 for Windows using powershell
#
# [shell]
# program = "pwsh"
# args = []
#
# Example 3 for Windows using powershell with login
#
# [shell]
# program = "pwsh"
# args = ["-l"]
#
# Example 4 for MacOS with tmux installed by homebrew
#
# [shell]
# program = "/opt/homebrew/bin/tmux"
# args = ["new-session", "-c", "/var/www"]

# Colors
#
# Colors definition will overwrite any property in theme
# (considering if theme folder does exists and is being used)
#
# Example:
# [colors]
# background = '#0F0D0E'
# foreground = '#F9F4DA'
# cursor = '#F38BA3'
# tabs = '#443d40'
# tabs-active = '#F38BA3'
# green = '#0BA95B'
# red = '#ED203D'
# blue = '#12B5E5'
# yellow = '#FCBA28'

# Bindings
#
# Create custom Key bindings for Rio terminal
# More information in: https://raphamorim.io/rio/docs/key-bindings
#
# Example:
# [bindings]
# keys = [
#   { key = "q", with = "super", action = "Quit" },
#   # Bytes[27, 91, 53, 126] is equivalent to "\x1b[5~"
#   { key = "home", with = "super | shift", bytes = [27, 91, 53, 126] }
# ]

# Platform
#
# Rio now allows you to have different configurations per OS
# You can write ovewrite properties like `Shell`, `Navigation`
# and `Window`.
#
# Example:
# [shell]
# # default (in this case will be used only on MacOS)
# program = "/bin/fish"
# args = ["--login"]
#
# [platform]
# # Microsoft Windows overwrite
# windows.shell.program = "pwsh"
# windows.shell.args = ["-l"]
#
# # Linux overwrite
# linux.shell.program = "tmux"
# linux.shell.args = ["new-session", "-c", "/var/www"]

# Log level
#
# This property enables log level filter and file. The default level is "OFF" and the logs are not logged to a file as default.
#
# Example:
# [developer]
# log-level = "OFF"
# enable-log-file = false
"#.to_string()
}
