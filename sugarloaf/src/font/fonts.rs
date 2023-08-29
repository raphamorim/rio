use crate::font::DEFAULT_FONT_FAMILY;
use serde::Deserialize;

/* Example:

[fonts]
size = 18

[fonts.regular]
family = "CascadiaMono"
style = "normal"
weight = 300

[fonts.bold]
family = "CascadiaMono"
style = "normal"
weight = 600

[fonts.italic]
family = "CascadiaMono"
style = "italic"
weight = 400
*/

#[derive(Debug, Deserialize, PartialEq, Clone)]
pub struct SugarloafFont {
    #[serde(default = "default_font_family")]
    pub family: String,
    pub weight: Option<u32>,
    pub style: Option<String>,
}

impl SugarloafFont {
    pub fn is_default_family(&self) -> bool {
        self.family == default_font_family()
    }
}

fn default_font_size() -> f32 {
    18.
}

fn default_font_family() -> String {
    DEFAULT_FONT_FAMILY.to_string()
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
        style: Some(String::from("normal")),
    }
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
pub struct SugarloafFonts {
    #[serde(default = "default_font_size")]
    pub size: f32,
    #[serde(default = "default_font_regular")]
    pub regular: SugarloafFont,
    #[serde(default = "default_font_bold")]
    pub bold: SugarloafFont,
    #[serde(default = "default_font_bold_italic")]
    pub bold_italic: SugarloafFont,
    #[serde(default = "default_font_italic")]
    pub italic: SugarloafFont,
}

impl Default for SugarloafFonts {
    fn default() -> SugarloafFonts {
        SugarloafFonts {
            size: default_font_size(),
            regular: default_font_regular(),
            bold: default_font_bold(),
            bold_italic: default_font_bold_italic(),
            italic: default_font_italic(),
        }
    }
}
