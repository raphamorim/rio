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

#[allow(clippy::derivable_impls)]
impl Default for Decorations {
    fn default() -> Decorations {
        #[cfg(target_os = "macos")]
        {
            Decorations::Transparent
        }

        #[cfg(not(target_os = "macos"))]
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
    #[serde(default = "bool::default")]
    pub blur: bool,
    #[serde(rename = "background-image", skip_serializing)]
    pub background_image: Option<ImageProperties>,
    #[serde(default = "Decorations::default")]
    pub decorations: Decorations,
    #[serde(default = "bool::default", rename = "macos-use-unified-titlebar")]
    pub macos_use_unified_titlebar: bool,
    #[serde(rename = "macos-use-shadow", default = "default_bool_true")]
    pub macos_use_shadow: bool,
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
}

impl Default for Window {
    fn default() -> Window {
        Window {
            width: default_window_width(),
            height: default_window_height(),
            mode: WindowMode::default(),
            opacity: default_opacity(),
            background_image: None,
            decorations: Decorations::default(),
            blur: false,
            macos_use_unified_titlebar: false,
            macos_use_shadow: true,
            initial_title: None,
            windows_use_undecorated_shadow: None,
            windows_use_no_redirection_bitmap: None,
            windows_corner_preference: None,
        }
    }
}

impl Window {
    pub fn is_fullscreen(&self) -> bool {
        self.mode == WindowMode::Fullscreen
    }
}
