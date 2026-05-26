use crate::config::colors::Colors;
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct AdaptiveColors {
    #[serde(default = "Option::default", skip_serializing)]
    pub dark: Option<Colors>,
    #[serde(default = "Option::default", skip_serializing)]
    pub light: Option<Colors>,
}

#[derive(Default, Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct AdaptiveTheme {
    pub dark: String,
    pub light: String,
}

#[derive(Debug, Default, Clone, Deserialize, PartialEq)]
pub struct Theme {
    #[serde(default = "Colors::default")]
    pub colors: Colors,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppearanceTheme {
    Dark,
    Light,
}

impl AppearanceTheme {
    pub fn to_window_theme(self) -> rio_window::window::Theme {
        match self {
            AppearanceTheme::Dark => rio_window::window::Theme::Dark,
            AppearanceTheme::Light => rio_window::window::Theme::Light,
        }
    }

    pub fn from_window_theme(theme: rio_window::window::Theme) -> Self {
        match theme {
            rio_window::window::Theme::Light => AppearanceTheme::Light,
            rio_window::window::Theme::Dark => AppearanceTheme::Dark,
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            AppearanceTheme::Dark => AppearanceTheme::Light,
            AppearanceTheme::Light => AppearanceTheme::Dark,
        }
    }
}
