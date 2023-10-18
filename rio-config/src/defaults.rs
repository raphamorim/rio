#[inline]
pub fn default_env_vars() -> Vec<String> {
    vec![]
}

#[inline]
pub fn default_padding_x() -> f32 {
    #[cfg(not(target_os = "macos"))]
    {
        0.
    }

    #[cfg(target_os = "macos")]
    {
        10.
    }
}

#[inline]
pub fn default_use_kitty_keyboard_protocol() -> bool {
    false
}

#[inline]
pub fn default_line_height() -> f32 {
    1.0
}

#[inline]
pub fn default_shell() -> crate::Shell {
    #[cfg(not(target_os = "windows"))]
    {
        crate::Shell {
            program: String::from(""),
            args: vec![String::from("--login")],
        }
    }

    #[cfg(target_os = "windows")]
    {
        crate::Shell {
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
pub fn default_background_opacity() -> f32 {
    1.0
}

#[inline]
pub fn default_option_as_alt() -> String {
    String::from("None")
}

#[inline]
pub fn default_log_level() -> String {
    String::from("OFF")
}

#[inline]
pub fn default_cursor() -> char {
    '▇'
}

#[inline]
pub fn default_theme() -> String {
    String::from("")
}

#[inline]
pub fn default_window_width() -> i32 {
    600
}

#[inline]
pub fn default_window_height() -> i32 {
    400
}

#[inline]
pub fn default_scroll_multiplier() -> f64 {
    3.0
}

pub fn default_config_file_content() -> String {
    r#"
# Cursor
#
# Default cursor is Block
# Other available options are: '_' and '|'
#
cursor = '▇'

# Blinking Cursor
#
# Default is false
#
blinking-cursor = false

# Scroll Speed Multiplier
#
# You can change how many lines are scrolled each time by setting this option.
# Defaul is 3.0.
# Example:
# scroll-multiplier = 3.0

# Ignore theme selection foreground color
#
# Default is false
#
# Example:
# ignore-selection-foreground-color = false

# Enable Kitty Keyboard protocol
#
# Default is false
#
# Example:
# use-kitty-keyboard-protocol = false

# Performance
#
# Set WGPU rendering performance
# High: Adapter that has the highest performance. This is often a discrete GPU.
# Low: Adapter that uses the least possible power. This is often an integrated GPU.
#
performance = "High"

# Theme
#
# It makes Rio look for the specified theme in the themes folder
# (macos and linux: ~/.config/rio/themes/dracula.toml)
# (windows: C:\Users\USER\AppData\Local\rio\themes\dracula.toml)
#
# Example:
# theme = "dracula"

# Padding-x
#
# define x axis padding (default is 10)
#
# Example:
# padding-x = 10

# Option as Alt
#
# This config only works on MacOs.
# Possible choices: 'both', 'left' and 'right'.
#
# Example:
# option-as-alt = 'left'

# Window configuration
#
# • width - define the intial window width.
#   Default: 600
#
# • height - define the inital window height.
#   Default: 400
#
# • mode - define how the window will be created
#     - "Windowed" (default) is based on width and height
#     - "Maximized" window is created with maximized
#     - "Fullscreen" window is created with fullscreen
#
# Example:
# [window]
# width = 600
# height = 400
# mode = "Windowed"

# Background configuration
#
# • opacity - changes the background transparency state
#   Default: 1.0
#
# • mode - defines background mode bewteen "Color" and "Image"
#
# • image - Set an image as background
#   Default: None
#
# Example:
# [background]
# mode = "Image"
# opacity = 1.0
#
# [background.image]
# path = "/Users/rapha/Desktop/eastward.jpg"
# width = 200.0
# height = 200.0
# x = 0.0
# y = 0.0

# Window Height
#
# window-height changes the inital window height.
#   Default: 400
#
# Example:
# window-height = 400

# Fonts
#
# Configure fonts used by the terminal
#
# Note: You can set different font families but Rio terminal
# will always look for regular font bounds whene
#
# You can also set family on root to overwritte all fonts
# [fonts]
#   family = "cascadiamono"
#
# You can also specify extra fonts to load
# [fonts]
# extras = [{ family = "Microsoft JhengHei" }]
#
#
# Example:
# [fonts]
# size = 18
#
# [fonts.regular]
# family = "cascadiamono"
# style = "normal"
# weight = 400
#
# [fonts.bold]
# family = "cascadiamono"
# style = "normal"
# weight = 800
#
# [fonts.italic]
# family = "cascadiamono"
# style = "italic"
# weight = 400
#
# [fonts.bold-italic]
# family = "cascadiamono"
# style = "italic"
# weight = 800

# Navigation
#
# "mode" - Define navigation mode
#   • NativeTab (MacOs only)
#   • CollapsedTab
#   • BottomTab
#   • TopTab
#   • Breadcrumb
#   • Plain
#
# "clickable" - Enable click on tabs to switch.
# "use-current-path" - Use same path whenever a new tab is created.
# "color-automation" - Set a specific color for the tab whenever a specific program is running.
# "macos-hide-window-buttons" - (MacOS only) Hide window buttons
#
# Example:
# [navigation]
# mode = "CollapsedTab"
# clickable = false
# use-current-path = false
# color-automation = []
# macos-hide-window-buttons = false

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
# shell = { program = "/bin/fish", args = ["--login"] }
#
# Example 2 for Windows using powershell
#
# shell = { program = "pwsh", args = [] }
#
# Example 3 for Windows using powershell with login
#
# shell = { program = "pwsh", args = ["-l"] }

# Startup directory
#
# Directory the shell is started in. If this is unset the working
# directory of the parent process will be used.
#
# This configuration only has effect if use-fork is disabled
#
# Example:
# working-dir = "/Users/raphael/Documents/"

# Environment variables
#
# The example below sets fish as the default SHELL using env vars
# please do not copy this if you do not need
#
# Example:
# env-vars = []

# Disable render when unfocused
#
# This property disable renderer processes while Rio is unfocused.
#
# Example:
# disable-renderer-when-unfocused = false

# Use fork
#
# Defaults for POSIX-based systems (Windows is not configurable):
# MacOS: spawn processes
# Linux/BSD: fork processes
#
# Example:
# use-fork = false

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
# More information in: raphamorim.io/rio/docs/custom-key-bindings
#
# Example:
# [bindings]
# keys = [
#   { key = "q", with = "super", action = "Quit" },
#   # Bytes[27, 91, 53, 126] is equivalent to "\x1b[5~"
#   { key = "home", with = "super | shift", bytes = [27, 91, 53, 126] }
# ]

# Log level
#
# This property enables log level filter. Default is "OFF".
#
# Example:
# [developer]
# log-level = "OFF"
"#.to_string()
}
