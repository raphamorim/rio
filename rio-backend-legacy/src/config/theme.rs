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
