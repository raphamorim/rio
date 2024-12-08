use crate::font::DEFAULT_FONT_FAMILY;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone)]
pub enum SugarloafFontStyle {
    #[default]
    Normal,
    Italic,
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone)]
pub enum SugarloafFontWidth {
    UltraCondensed,
    ExtraCondensed,
    Condensed,
    SemiCondensed,
    #[default]
    Normal,
    SemiExpanded,
    Expanded,
    ExtraExpanded,
    UltraExpanded,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct SugarloafFont {
    #[serde(default = "default_font_family")]
    pub family: String,
    #[serde(default = "Option::default")]
    pub weight: Option<u16>,
    #[serde(default = "SugarloafFontStyle::default")]
    pub style: SugarloafFontStyle,
    #[serde(default = "Option::default")]
    pub width: Option<SugarloafFontWidth>,
}

impl Default for SugarloafFont {
    fn default() -> Self {
        Self {
            family: default_font_family(),
            weight: None,
            style: SugarloafFontStyle::Normal,
            width: None,
        }
    }
}

impl SugarloafFont {
    #[inline]
    pub fn is_default_family(&self) -> bool {
        let current = self.family.replace(' ', "").trim().to_lowercase();
        current == default_font_family()
    }
}

#[inline]
pub fn default_font_size() -> f32 {
    14.
}

#[inline]
pub fn default_font_hinting() -> bool {
    true
}

fn default_font_family() -> String {
    DEFAULT_FONT_FAMILY.to_string()
}

pub fn default_font_regular() -> SugarloafFont {
    SugarloafFont {
        family: default_font_family(),
        weight: Some(400),
        style: SugarloafFontStyle::Normal,
        width: None,
    }
}

pub fn default_font_bold() -> SugarloafFont {
    SugarloafFont {
        family: default_font_family(),
        weight: Some(800),
        style: SugarloafFontStyle::Normal,
        width: None,
    }
}

pub fn default_font_italic() -> SugarloafFont {
    SugarloafFont {
        family: default_font_family(),
        weight: Some(300),
        style: SugarloafFontStyle::Italic,
        width: None,
    }
}

pub fn default_font_bold_italic() -> SugarloafFont {
    SugarloafFont {
        family: default_font_family(),
        weight: Some(800),
        style: SugarloafFontStyle::Italic,
        width: None,
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct SugarloafFonts {
    #[serde(default = "default_font_size")]
    pub size: f32,
    #[serde(default = "default_font_hinting")]
    pub hinting: bool,
    #[serde(default = "Option::default")]
    pub features: Option<Vec<String>>,
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
    #[serde(default = "Option::default")]
    pub ui: Option<SugarloafFont>,
    #[serde(default = "Option::default")]
    pub emoji: Option<SugarloafFont>,
    #[serde(default = "Vec::default")]
    pub extras: Vec<SugarloafFont>,
}

impl Default for SugarloafFonts {
    fn default() -> SugarloafFonts {
        SugarloafFonts {
            features: None,
            hinting: true,
            size: default_font_size(),
            family: None,
            emoji: None,
            ui: None,
            regular: default_font_regular(),
            bold: default_font_bold(),
            bold_italic: default_font_bold_italic(),
            italic: default_font_italic(),
            extras: vec![],
        }
    }
}
