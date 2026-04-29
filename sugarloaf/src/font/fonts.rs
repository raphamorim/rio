use crate::font::DEFAULT_FONT_FAMILY;
use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};
use std::fmt;

/// Per-slot font style override. Mirrors Ghostty's `FontStyle` enum:
///   - `Default`: let font discovery pick the face implied by the slot
///     (regular / bold / italic / bold+italic traits).
///   - `Disabled`: skip this slot entirely; the regular face is reused
///     when the terminal asks for this style. Spelled `false` in TOML.
///   - `Named(String)`: match a specific style name from the family,
///     e.g. `"Light"`, `"Medium"`, `"Heavy"`. CoreText / fontconfig
///     resolves this against the face's style/PostScript name.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum FontStyle {
    #[default]
    Default,
    Disabled,
    Named(String),
}

impl FontStyle {
    #[inline]
    pub fn name(&self) -> Option<&str> {
        match self {
            FontStyle::Named(s) => Some(s.as_str()),
            _ => None,
        }
    }

    #[inline]
    pub fn is_disabled(&self) -> bool {
        matches!(self, FontStyle::Disabled)
    }
}

impl Serialize for FontStyle {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        match self {
            FontStyle::Default => ser.serialize_str("default"),
            FontStyle::Disabled => ser.serialize_bool(false),
            FontStyle::Named(s) => ser.serialize_str(s),
        }
    }
}

impl<'de> Deserialize<'de> for FontStyle {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = FontStyle;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("\"default\", false, or a font style name string")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<FontStyle, E> {
                if v {
                    Err(E::custom(
                        "font style cannot be `true`; use \"default\" or a name",
                    ))
                } else {
                    Ok(FontStyle::Disabled)
                }
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<FontStyle, E> {
                self.visit_string(v.to_string())
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<FontStyle, E> {
                Ok(match v.as_str() {
                    "default" => FontStyle::Default,
                    "false" => FontStyle::Disabled,
                    _ => FontStyle::Named(v),
                })
            }
        }
        de.deserialize_any(V)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct SugarloafFont {
    #[serde(default = "default_font_family")]
    pub family: String,
    #[serde(default)]
    pub style: FontStyle,
}

impl Default for SugarloafFont {
    fn default() -> Self {
        Self {
            family: default_font_family(),
            style: FontStyle::Default,
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
pub fn default_bool_true() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SymbolMap {
    pub start: String,
    pub end: String,
    #[serde(rename = "font-family")]
    pub font_family: String,
}

fn default_font_family() -> String {
    DEFAULT_FONT_FAMILY.to_string()
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct SugarloafFonts {
    #[serde(default = "default_font_size")]
    pub size: f32,
    #[serde(default = "default_bool_true")]
    pub hinting: bool,
    #[serde(default = "Option::default")]
    pub features: Option<Vec<String>>,
    #[serde(default = "Option::default")]
    pub family: Option<String>,
    #[serde(default)]
    pub regular: SugarloafFont,
    #[serde(default)]
    pub bold: SugarloafFont,
    #[serde(default, rename = "bold-italic")]
    pub bold_italic: SugarloafFont,
    #[serde(default)]
    pub italic: SugarloafFont,
    #[serde(default = "default_bool_true", rename = "use-drawable-chars")]
    pub use_drawable_chars: bool,
    #[serde(default = "Option::default", rename = "symbol-map")]
    pub symbol_map: Option<Vec<SymbolMap>>,
    #[serde(default = "bool::default", rename = "disable-warnings-not-found")]
    pub disable_warnings_not_found: bool,
    #[serde(default = "Option::default", rename = "additional-dirs")]
    pub additional_dirs: Option<Vec<String>>,
}

pub fn parse_unicode(input: &str) -> Option<char> {
    if let Ok(unicode) = u32::from_str_radix(input, 16) {
        if let Some(result) = char::from_u32(unicode) {
            return Some(result);
        }
    }

    None
}

impl Default for SugarloafFonts {
    fn default() -> SugarloafFonts {
        SugarloafFonts {
            features: None,
            hinting: true,
            size: default_font_size(),
            family: None,
            regular: SugarloafFont::default(),
            bold: SugarloafFont::default(),
            bold_italic: SugarloafFont::default(),
            italic: SugarloafFont::default(),
            use_drawable_chars: true,
            symbol_map: None,
            disable_warnings_not_found: false,
            additional_dirs: None,
        }
    }
}
