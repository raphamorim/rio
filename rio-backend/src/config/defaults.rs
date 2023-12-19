#[inline]
pub fn default_env_vars() -> Vec<String> {
    vec![]
}

#[inline]
pub fn default_padding_x() -> f32 {
    0.
}

#[inline]
pub fn default_bool_true() -> bool {
    true
}

#[inline]
pub fn default_line_height() -> f32 {
    1.0
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
pub fn default_editor() -> String {
    String::from("vi")
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

pub fn default_config_file_content() -> String {
    r#"
# Editor
#
# Default editor is "vi".
#
# Whenever the key binding `OpenConfigEditor` is triggered it will
# use the value of the editor along with the rio configuration path.
# editor = 'vi'

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

# Ignore theme selection foreground color
#
# Default is false
#
# Example:
# ignore-selection-foreground-color = false

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
# define x axis padding (default is 0)
#
# Example:
# padding-x = 0

# Option as Alt
#
# This config only works on MacOs.
# Possible choices: 'both', 'left' and 'right'.
#
# Example:
# option-as-alt = 'left'

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

# Use fork
#
# Defaults for POSIX-based systems (Windows is not configurable):
# MacOS: spawn processes
# Linux/BSD: fork processes
#
# Example:
# use-fork = false

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
# • background-opacity - Set background opacity
#
# • foreground-opacity - Set foreground opacity
#
# • blur - Set blur on the window background. Changing this config requires restarting Rio to take effect.
#
# • decorations - Set window decorations, options: "Enabled", "Disabled", "Transparent", "Buttonless"
#
# Example:
# [window]
# width = 600
# height = 400
# mode = "Windowed"
# foreground-opacity = 1.0
# background-opacity = 1.0
# blur = false
# decorations = "Enabled"

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
#   - DX11: Supported on Windows 7+
#   - Metal: Supported on macOS/iOS
#
# • disable-renderer-when-unfocused: This property disable renderer processes while Rio is unfocused.
#
# Example:
# [renderer]
# performance = "High"
# backend = "Automatic"
# disable-renderer-when-unfocused = false

# Keyboard
#
# use-kitty-keyboard-protocol - Enable Kitty Keyboard protocol
#
# disable-ctlseqs-alt - Disable ctlseqs with ALT keys
#   - For example: Terminal.app does not deal with ctlseqs with ALT keys
#
# Example:
# [keyboard]
# use-kitty-keyboard-protocol = false
# disable-ctlseqs-alt = false

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
#   • NativeTab (MacOs only)
#   • CollapsedTab
#   • BottomTab
#   • TopTab
#   • Breadcrumb
#   • Plain
#
# "clickable" - Enable click on tabs to switch.
# "use-current-path" - Use same path whenever a new tab is created.
# "color-automation" - Set a specific color for the tab whenever a specific program is running, or in a specific directory.
#
# Example:
# [navigation]
# mode = "CollapsedTab"
# clickable = false
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
