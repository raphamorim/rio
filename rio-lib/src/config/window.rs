use crate::config::defaults::*;
use serde::{Deserialize, Serialize};
use sugarloaf::core::ImageProperties;

#[derive(Default, Clone, Serialize, Deserialize, Copy, Debug, PartialEq)]
pub enum WindowMode {
    Maximized,
    Fullscreen,
    // Windowed will use width and height definition
    #[default]
    Windowed,
}

#[derive(PartialEq, Serialize, Deserialize, Clone, Debug)]
pub struct Window {
    #[serde(default = "default_window_width")]
    pub width: i32,
    #[serde(default = "default_window_height")]
    pub height: i32,
    #[serde(default = "WindowMode::default")]
    pub mode: WindowMode,
}

impl Default for Window {
    fn default() -> Window {
        Window {
            width: default_window_width(),
            height: default_window_height(),
            mode: WindowMode::default(),
        }
    }
}

#[derive(Default, Clone, Serialize, Deserialize, Copy, Debug, PartialEq)]
pub enum BackgroundMode {
    #[default]
    Color,
    Image,
}

impl BackgroundMode {
    pub fn is_image(self) -> bool {
        self == BackgroundMode::Image
    }
}

#[derive(PartialEq, Serialize, Deserialize, Clone, Debug)]
pub struct Background {
    #[serde(default = "default_background_opacity", skip_serializing)]
    pub opacity: f32,
    #[serde(default = "BackgroundMode::default", skip_serializing)]
    pub mode: BackgroundMode,
    #[serde(default = "Option::default", skip_serializing)]
    pub image: Option<ImageProperties>,
}

impl Default for Background {
    fn default() -> Background {
        Background {
            opacity: default_background_opacity(),
            image: None,
            mode: BackgroundMode::Color,
        }
    }
}
