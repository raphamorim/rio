use crate::defaults::*;
use serde::Deserialize;

#[derive(Default, Clone, Deserialize, Copy, Debug, PartialEq)]
pub enum WindowMode {
    Maximized,
    Fullscreen,
    // Regular will use width and height definition
    #[default]
    Regular,
}

#[derive(PartialEq, Deserialize, Clone, Copy, Debug)]
pub struct Window {
    #[serde(default = "default_window_opacity")]
    pub opacity: f32,
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
            opacity: default_window_opacity(),
            mode: WindowMode::default(),
        }
    }
}
