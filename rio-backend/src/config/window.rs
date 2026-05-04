use crate::config::defaults::*;
use serde::{Deserialize, Serialize};
use sugarloaf::ImageProperties;

#[derive(Default, Clone, Serialize, Deserialize, Copy, Debug, PartialEq)]
pub enum WindowMode {
    #[serde(alias = "maximized")]
    Maximized,
    #[serde(alias = "fullscreen")]
    Fullscreen,
    // Windowed will use width and height definition
    #[default]
    #[serde(alias = "windowed")]
    Windowed,
}

#[derive(Clone, Serialize, Deserialize, Copy, Debug, PartialEq)]
pub enum Colorspace {
    #[serde(alias = "srgb")]
    Srgb,
    #[serde(alias = "display-p3")]
    DisplayP3,
    #[serde(alias = "rec2020")]
    Rec2020,
}

#[allow(clippy::derivable_impls)]
impl Default for Colorspace {
    fn default() -> Colorspace {
        // `[window] colorspace` = how to interpret hex / ANSI color values
        // (matches ghostty's `window-colorspace` semantics). The surface
        // itself is always wide-gamut on macOS; the config picks which
        // primaries the input bytes are assumed to be in. Default `srgb`
        // keeps `#ff0000` looking like the sRGB standard red most apps use.
        Colorspace::Srgb
    }
}

#[derive(Clone, Serialize, Deserialize, Copy, Debug, PartialEq)]
pub enum Decorations {
    #[serde(alias = "enabled")]
    Enabled,
    #[serde(alias = "disabled")]
    Disabled,
    #[serde(alias = "transparent")]
    Transparent,
    #[serde(alias = "buttonless")]
    Buttonless,
}

#[cfg(target_os = "macos")]
#[allow(clippy::derivable_impls)]
impl Default for Decorations {
    fn default() -> Decorations {
        Decorations::Transparent
    }
}

#[cfg(not(target_os = "macos"))]
#[allow(clippy::derivable_impls)]
impl Default for Decorations {
    fn default() -> Decorations {
        Decorations::Enabled
    }
}

#[derive(PartialEq, Serialize, Deserialize, Clone, Debug)]
pub enum WindowsCornerPreference {
    #[serde(alias = "default")]
    Default = 0,
    #[serde(alias = "donotround")]
    DoNotRound = 1,
    #[serde(alias = "round")]
    Round = 2,
    #[serde(alias = "roundsmall")]
    RoundSmall = 3,
}

/// Background blur / liquid-glass behaviour for the window.
///
/// Accepted in TOML as either a bool or one of the macOS glass-effect
/// strings, mirroring the established `window.blur = true` legacy
/// config:
///
/// ```toml
/// blur = false                  # off
/// blur = true                   # standard system blur (CGS / KWin / DWM)
/// blur = "macos-glass-regular"  # macOS 26+ liquid glass, regular opacity
/// blur = "macos-glass-clear"    # macOS 26+ liquid glass, highly transparent
/// ```
///
/// On platforms where the requested style isn't available (e.g. glass
/// values on macOS < 26 or on Linux/Windows), the windowing layer
/// degrades to `System` and emits a `tracing::warn` so the user finds
/// out without a hard failure. Glass values imply a translucent
/// window — they flip the layer/window's opaque flag the same way
/// `window.opacity < 1` does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowBlur {
    #[default]
    Off,
    System,
    MacosGlassRegular,
    MacosGlassClear,
}

impl WindowBlur {
    /// True for any non-`Off` variant. Use this in places that just
    /// care "is some kind of background effect on?" without needing to
    /// distinguish the style.
    #[inline]
    pub fn is_enabled(self) -> bool {
        !matches!(self, WindowBlur::Off)
    }

    /// True for the macOS liquid-glass variants. They imply a
    /// translucent window the same way `window.opacity < 1` does.
    #[inline]
    pub fn is_glass(self) -> bool {
        matches!(
            self,
            WindowBlur::MacosGlassRegular | WindowBlur::MacosGlassClear
        )
    }
}

impl From<WindowBlur> for rio_window::window::BlurStyle {
    fn from(b: WindowBlur) -> Self {
        match b {
            WindowBlur::Off => rio_window::window::BlurStyle::Off,
            WindowBlur::System => rio_window::window::BlurStyle::System,
            WindowBlur::MacosGlassRegular => {
                rio_window::window::BlurStyle::MacosGlassRegular
            }
            WindowBlur::MacosGlassClear => rio_window::window::BlurStyle::MacosGlassClear,
        }
    }
}

impl Serialize for WindowBlur {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            // Round-trip the legacy / common path as a bool so existing
            // config files stay byte-identical after a save.
            WindowBlur::Off => s.serialize_bool(false),
            WindowBlur::System => s.serialize_bool(true),
            WindowBlur::MacosGlassRegular => s.serialize_str("macos-glass-regular"),
            WindowBlur::MacosGlassClear => s.serialize_str("macos-glass-clear"),
        }
    }
}

impl<'de> Deserialize<'de> for WindowBlur {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // `untagged` lets a single field accept either a TOML bool
        // (`blur = true`) or a TOML string (`blur = "macos-glass-clear"`)
        // without forcing the caller to wrap it in a tagged form.
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Bool(bool),
            Str(String),
        }

        match Raw::deserialize(d)? {
            Raw::Bool(false) => Ok(WindowBlur::Off),
            Raw::Bool(true) => Ok(WindowBlur::System),
            Raw::Str(s) => match s.as_str() {
                "macos-glass-regular" => Ok(WindowBlur::MacosGlassRegular),
                "macos-glass-clear" => Ok(WindowBlur::MacosGlassClear),
                other => Err(serde::de::Error::custom(format!(
                    "unknown window.blur value `{other}`; expected a bool or one of \
                     \"macos-glass-regular\", \"macos-glass-clear\""
                ))),
            },
        }
    }
}

#[derive(PartialEq, Serialize, Deserialize, Clone, Debug)]
pub struct Window {
    #[serde(default = "default_window_width")]
    pub width: i32,
    #[serde(default = "default_window_height")]
    pub height: i32,
    #[serde(default = "WindowMode::default")]
    pub mode: WindowMode,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    /// Apply `window.opacity` to cells that paint an explicit
    /// background color too, not just to the window's default
    /// background. Off by default — cells with an SGR-set background
    /// stay fully opaque so syntax-highlighted regions and status
    /// lines painted by tmux/Neovim keep their contrast. Flip to
    /// `true` to make TUIs see-through too.
    ///
    /// On the wire: kebab-case `opacity-cells` under `[window]`.
    #[serde(rename = "opacity-cells", default = "bool::default")]
    pub opacity_cells: bool,
    #[serde(default)]
    pub blur: WindowBlur,
    #[serde(rename = "background-image", skip_serializing)]
    pub background_image: Option<ImageProperties>,
    #[serde(default = "Decorations::default")]
    pub decorations: Decorations,
    #[serde(default = "bool::default", rename = "macos-use-unified-titlebar")]
    pub macos_use_unified_titlebar: bool,
    #[serde(rename = "macos-use-shadow", default = "default_bool_true")]
    pub macos_use_shadow: bool,
    #[serde(rename = "macos-traffic-light-position-x", default = "Option::default")]
    pub macos_traffic_light_position_x: Option<f64>,
    #[serde(rename = "macos-traffic-light-position-y", default = "Option::default")]
    pub macos_traffic_light_position_y: Option<f64>,
    #[serde(rename = "initial-title", skip_serializing)]
    pub initial_title: Option<String>,
    #[serde(rename = "windows-use-undecorated-shadow", default = "Option::default")]
    pub windows_use_undecorated_shadow: Option<bool>,
    #[serde(
        rename = "windows-use-no-redirection-bitmap",
        default = "Option::default"
    )]
    pub windows_use_no_redirection_bitmap: Option<bool>,
    #[serde(rename = "windows-corner-preference", default = "Option::default")]
    pub windows_corner_preference: Option<WindowsCornerPreference>,
    #[serde(default = "Colorspace::default")]
    pub colorspace: Colorspace,
    #[serde(default = "Option::default")]
    pub columns: Option<u16>,
    #[serde(default = "Option::default")]
    pub rows: Option<u16>,
}

impl Default for Window {
    fn default() -> Window {
        Window {
            width: default_window_width(),
            height: default_window_height(),
            mode: WindowMode::default(),
            opacity: default_opacity(),
            opacity_cells: false,
            background_image: None,
            decorations: Decorations::default(),
            blur: WindowBlur::default(),
            macos_use_unified_titlebar: false,
            macos_use_shadow: true,
            macos_traffic_light_position_x: None,
            macos_traffic_light_position_y: None,
            initial_title: None,
            windows_use_undecorated_shadow: None,
            windows_use_no_redirection_bitmap: None,
            windows_corner_preference: None,
            colorspace: Colorspace::default(),
            columns: None,
            rows: None,
        }
    }
}

impl Colorspace {
    pub fn to_sugarloaf_colorspace(&self) -> sugarloaf::Colorspace {
        match self {
            Colorspace::Srgb => sugarloaf::Colorspace::Srgb,
            Colorspace::DisplayP3 => sugarloaf::Colorspace::DisplayP3,
            Colorspace::Rec2020 => sugarloaf::Colorspace::Rec2020,
        }
    }

    #[cfg(target_os = "macos")]
    pub fn to_rio_window_colorspace(&self) -> rio_window::platform::macos::Colorspace {
        match self {
            Colorspace::Srgb => rio_window::platform::macos::Colorspace::Srgb,
            Colorspace::DisplayP3 => rio_window::platform::macos::Colorspace::DisplayP3,
            Colorspace::Rec2020 => rio_window::platform::macos::Colorspace::Rec2020,
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub fn to_rio_window_colorspace(&self) {
        // No-op for non-macOS platforms
    }
}

impl Window {
    pub fn is_fullscreen(&self) -> bool {
        self.mode == WindowMode::Fullscreen
    }
}
