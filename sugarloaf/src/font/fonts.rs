use crate::font::DEFAULT_FONT_FAMILY;
use serde::Deserialize;

/* Example:

[fonts]
size = 18
regular = {
    family = "Menlo",
    style = "normal",
    weight = 300
}

bold = {
    family = "Menlo",
    style = "bold"
    weight = 600
}

italic = {
    family = "Menlo",
    style = "italic"
    weight = 600
}
*/

#[derive(Debug, Deserialize, PartialEq, Clone)]
pub struct Font {
    pub weight: Option<u32>,
    pub style: String,
}

fn default_font_size() -> f32 {
    18.
}

fn default_font_family() -> String {
    DEFAULT_FONT_FAMILY.to_string()
}

fn default_font_regular() -> Font {
    Font {
        weight: Some(300),
        style: String::from("regular"),
    }
}

fn default_font_bold() -> Font {
    Font {
        weight: Some(600),
        style: String::from("bold"),
    }
}

fn default_font_italic() -> Font {
    Font {
        weight: Some(400),
        style: String::from("italic"),
    }
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
pub struct Fonts {
    #[serde(default = "default_font_size")]
    pub size: f32,
    #[serde(default = "default_font_family")]
    pub family: String,
    #[serde(default = "default_font_regular")]
    pub regular: Font,
    #[serde(default = "default_font_bold")]
    pub bold: Font,
    #[serde(default = "default_font_italic")]
    pub italic: Font,
}

impl Default for Fonts {
    fn default() -> Fonts {
        Fonts {
            size: default_font_size(),
            family: DEFAULT_FONT_FAMILY.to_string(),
            regular: Font {
                weight: Some(500),
                style: String::from("regular"),
            },
            bold: Font {
                weight: Some(600),
                style: String::from("bold"),
            },
            italic: Font {
                weight: Some(400),
                style: String::from("italic"),
            },
        }
    }
}

impl Fonts {
    pub fn is_not_default(&self) -> bool {
        self != &Fonts::default()
    }
}
