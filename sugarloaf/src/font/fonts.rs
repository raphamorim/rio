use crate::font::{DEFAULT_FONT_FAMILY, DEFAULT_FONT_FAMILY_VARIANT};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct SugarloafFont {
    #[serde(default = "default_font_family")]
    pub family: String,
    pub weight: Option<u16>,
    pub style: Option<String>,
}

impl Default for SugarloafFont {
    fn default() -> Self {
        Self {
            family: default_font_family(),
            weight: None,
            style: None,
        }
    }
}

impl SugarloafFont {
    #[inline]
    pub fn is_default_family(&self) -> bool {
        let current = self.family.replace(' ', "").trim().to_lowercase();
        current == default_font_family() || current == default_font_family_variant()
    }
}

#[inline]
pub fn default_font_size() -> f32 {
    14.
}

fn default_font_family() -> String {
    DEFAULT_FONT_FAMILY.to_string()
}
fn default_font_family_variant() -> String {
    DEFAULT_FONT_FAMILY_VARIANT.to_string()
}

pub fn default_font_regular() -> SugarloafFont {
    SugarloafFont {
        family: default_font_family(),
        weight: Some(400),
        style: Some(String::from("normal")),
    }
}

pub fn default_font_bold() -> SugarloafFont {
    SugarloafFont {
        family: default_font_family(),
        weight: Some(800),
        style: Some(String::from("normal")),
    }
}

pub fn default_font_italic() -> SugarloafFont {
    SugarloafFont {
        family: default_font_family(),
        weight: Some(400),
        style: Some(String::from("italic")),
    }
}

pub fn default_font_bold_italic() -> SugarloafFont {
    SugarloafFont {
        family: default_font_family(),
        weight: Some(800),
        style: Some(String::from("italic")),
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct SugarloafFonts {
    #[serde(default = "default_font_size")]
    pub size: f32,
    #[serde(default = "Option::default")]
    pub family: Option<String>,
    #[serde(default = "default_font_regular")]
    pub regular: SugarloafFont,
    #[serde(default = "default_font_bold")]
    pub bold: SugarloafFont,
    #[serde(default = "default_font_bold_italic", rename = "bold-italic")]
    pub bold_italic: SugarloafFont,
    #[serde(default = "default_font_italic")]
    pub italic: SugarloafFont,
    #[serde(default = "Vec::default")]
    pub extras: Vec<SugarloafFont>,
}

impl Default for SugarloafFonts {
    fn default() -> SugarloafFonts {
        SugarloafFonts {
            size: default_font_size(),
            family: None,
            regular: default_font_regular(),
            bold: default_font_bold(),
            bold_italic: default_font_bold_italic(),
            italic: default_font_italic(),
            extras: vec![],
        }
    }
}
