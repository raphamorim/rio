use colors::Colors;
use serde::Deserialize;

#[derive(Default, Debug, Deserialize, PartialEq, Clone)]
pub struct AdaptiveColors {
    #[serde(default = "Option::default")]
    pub dark: Option<Colors>,
    #[serde(default = "Option::default")]
    pub light: Option<Colors>,
}

#[derive(Default, Debug, Deserialize, PartialEq, Clone)]
pub struct AdaptiveTheme {
    pub dark: String,
    pub light: String,
}

#[derive(Debug, Default, Clone, Deserialize, PartialEq)]
pub struct Theme {
    #[serde(default = "Colors::default")]
    pub colors: Colors,
}
