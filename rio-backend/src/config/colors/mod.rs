pub mod defaults;
pub mod term;

use defaults::*;
use regex::Regex;
use serde::Serialize;
use serde::{de, Deserialize};
use std::num::ParseIntError;

// `ColorWGPU` is the legacy name; `sugarloaf::Color` is the actual
// type now (mirrors `wgpu::Color`'s shape, but doesn't drag wgpu
// into the dep tree on Linux/macOS native builds).
pub type ColorWGPU = sugarloaf::Color;
pub type ColorComposition = (ColorArray, ColorWGPU);

// The terminal color value model now lives in `rio-core`. Re-export it so all
// existing `config::colors::{ColorRgb, AnsiColor, NamedColor, ColorArray, ...}`
// paths keep resolving unchanged. The frontend-only conversions (`to_wgpu`,
// `to_composition`) stay here, exposed via [`ColorRgbExt`].
pub use rio_core::color::{AnsiColor, ColorArray, ColorRgb, NamedColor, DIM_FACTOR};

/// Frontend-only conversions for [`ColorRgb`]: turning a plain RGB value into
/// the renderer's `Color` (gamma/format aware via [`ColorBuilder`]). These
/// deliberately do NOT live in `rio-core`, which is renderer-agnostic.
pub trait ColorRgbExt {
    fn to_wgpu(&self) -> ColorWGPU;
    fn to_composition(&self) -> ColorComposition;
}

impl ColorRgbExt for ColorRgb {
    fn to_wgpu(&self) -> ColorWGPU {
        ColorBuilder::from_rgb(*self, Format::SRGB0_1).to_wgpu()
    }

    fn to_composition(&self) -> ColorComposition {
        (self.to_arr(), self.to_wgpu())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Format {
    SRGB0_255,
    SRGB0_1,
}

#[derive(Debug, Copy, Deserialize, PartialEq, Clone)]
pub struct Colors {
    #[serde(
        deserialize_with = "deserialize_to_composition",
        default = "defaults::background"
    )]
    /// Background is a special color type called ColorComposition
    /// ColorComposition type is (ColorArray, ColorWGPU)
    /// See more in colors definition
    pub background: ColorComposition,
    #[serde(
        deserialize_with = "deserialize_to_arr",
        default = "defaults::foreground"
    )]
    pub foreground: ColorArray,
    #[serde(deserialize_with = "deserialize_to_arr", default = "defaults::blue")]
    pub blue: ColorArray,
    #[serde(deserialize_with = "deserialize_to_arr", default = "defaults::green")]
    pub green: ColorArray,
    #[serde(deserialize_with = "deserialize_to_arr", default = "defaults::red")]
    pub red: ColorArray,
    #[serde(deserialize_with = "deserialize_to_arr", default = "defaults::yellow")]
    pub yellow: ColorArray,
    #[serde(
        deserialize_with = "deserialize_to_arr",
        default = "defaults::tabs_active",
        rename = "tabs-active"
    )]
    pub tabs_active: ColorArray,
    #[serde(default = "defaults::cursor", deserialize_with = "deserialize_to_arr")]
    pub cursor: ColorArray,
    #[serde(
        default = "defaults::vi_cursor",
        rename = "vi-cursor",
        deserialize_with = "deserialize_to_arr"
    )]
    pub vi_cursor: ColorArray,
    #[serde(default = "defaults::black", deserialize_with = "deserialize_to_arr")]
    pub black: ColorArray,
    #[serde(default = "defaults::cyan", deserialize_with = "deserialize_to_arr")]
    pub cyan: ColorArray,
    #[serde(default = "defaults::magenta", deserialize_with = "deserialize_to_arr")]
    pub magenta: ColorArray,
    #[serde(default = "defaults::tabs", deserialize_with = "deserialize_to_arr")]
    pub tabs: ColorArray,
    #[serde(
        default = "defaults::tab_border",
        rename = "tab-border",
        deserialize_with = "deserialize_to_arr"
    )]
    pub tab_border: ColorArray,
    #[serde(default = "defaults::bar", deserialize_with = "deserialize_to_arr")]
    pub bar: ColorArray,
    #[serde(default = "defaults::white", deserialize_with = "deserialize_to_arr")]
    pub white: ColorArray,
    #[serde(
        default = "Option::default",
        deserialize_with = "deserialize_to_arr_opt",
        rename = "dim-black"
    )]
    pub dim_black: Option<ColorArray>,
    #[serde(
        default = "Option::default",
        deserialize_with = "deserialize_to_arr_opt",
        rename = "dim-blue"
    )]
    pub dim_blue: Option<ColorArray>,
    #[serde(
        default = "Option::default",
        deserialize_with = "deserialize_to_arr_opt",
        rename = "dim-cyan"
    )]
    pub dim_cyan: Option<ColorArray>,
    #[serde(
        default = "Option::default",
        deserialize_with = "deserialize_to_arr_opt",
        rename = "dim-foreground"
    )]
    pub dim_foreground: Option<ColorArray>,
    #[serde(
        default = "Option::default",
        deserialize_with = "deserialize_to_arr_opt",
        rename = "dim-green"
    )]
    pub dim_green: Option<ColorArray>,
    #[serde(
        default = "Option::default",
        deserialize_with = "deserialize_to_arr_opt",
        rename = "dim-magenta"
    )]
    pub dim_magenta: Option<ColorArray>,
    #[serde(
        default = "Option::default",
        deserialize_with = "deserialize_to_arr_opt",
        rename = "dim-red"
    )]
    pub dim_red: Option<ColorArray>,
    #[serde(
        default = "Option::default",
        deserialize_with = "deserialize_to_arr_opt",
        rename = "dim-white"
    )]
    pub dim_white: Option<ColorArray>,
    #[serde(
        default = "Option::default",
        deserialize_with = "deserialize_to_arr_opt",
        rename = "dim-yellow"
    )]
    pub dim_yellow: Option<ColorArray>,
    #[serde(
        default = "default_light_black",
        deserialize_with = "deserialize_to_arr",
        rename = "light-black"
    )]
    pub light_black: ColorArray,
    #[serde(
        default = "default_light_blue",
        deserialize_with = "deserialize_to_arr",
        rename = "light-blue"
    )]
    pub light_blue: ColorArray,
    #[serde(
        default = "default_light_cyan",
        deserialize_with = "deserialize_to_arr",
        rename = "light-cyan"
    )]
    pub light_cyan: ColorArray,
    #[serde(
        default = "Option::default",
        deserialize_with = "deserialize_to_arr_opt",
        rename = "light-foreground"
    )]
    pub light_foreground: Option<ColorArray>,
    #[serde(
        default = "default_light_green",
        deserialize_with = "deserialize_to_arr",
        rename = "light-green"
    )]
    pub light_green: ColorArray,
    #[serde(
        default = "default_light_magenta",
        deserialize_with = "deserialize_to_arr",
        rename = "light-magenta"
    )]
    pub light_magenta: ColorArray,
    #[serde(
        default = "default_light_red",
        deserialize_with = "deserialize_to_arr",
        rename = "light-red"
    )]
    pub light_red: ColorArray,
    #[serde(
        default = "default_light_white",
        deserialize_with = "deserialize_to_arr",
        rename = "light-white"
    )]
    pub light_white: ColorArray,
    #[serde(
        default = "default_light_yellow",
        deserialize_with = "deserialize_to_arr",
        rename = "light-yellow"
    )]
    pub light_yellow: ColorArray,
    #[serde(
        default = "defaults::selection_background",
        deserialize_with = "deserialize_to_arr",
        rename = "selection-background"
    )]
    pub selection_background: ColorArray,
    #[serde(
        default = "defaults::selection_foreground",
        deserialize_with = "deserialize_to_arr",
        rename = "selection-foreground"
    )]
    pub selection_foreground: ColorArray,
    #[serde(default = "defaults::split", deserialize_with = "deserialize_to_arr")]
    pub split: ColorArray,
    #[serde(
        default = "defaults::split_active",
        deserialize_with = "deserialize_to_arr",
        rename = "split-active"
    )]
    pub split_active: ColorArray,
    #[serde(
        default = "defaults::search_match_background",
        deserialize_with = "deserialize_to_arr",
        rename = "search-match-background"
    )]
    pub search_match_background: ColorArray,
    #[serde(
        default = "defaults::search_match_foreground",
        deserialize_with = "deserialize_to_arr",
        rename = "search-match-foreground"
    )]
    pub search_match_foreground: ColorArray,
    #[serde(
        default = "defaults::search_focused_match_background",
        deserialize_with = "deserialize_to_arr",
        rename = "search-focused-match-background"
    )]
    pub search_focused_match_background: ColorArray,
    #[serde(
        default = "defaults::search_focused_match_foreground",
        deserialize_with = "deserialize_to_arr",
        rename = "search-focused-match-foreground"
    )]
    pub search_focused_match_foreground: ColorArray,
    #[serde(
        default = "defaults::hint_foreground",
        deserialize_with = "deserialize_to_arr",
        rename = "hint-foreground"
    )]
    pub hint_foreground: ColorArray,
    #[serde(
        default = "defaults::hint_background",
        deserialize_with = "deserialize_to_arr",
        rename = "hint-background"
    )]
    pub hint_background: ColorArray,
}

impl Default for Colors {
    fn default() -> Colors {
        Colors {
            background: defaults::background(),
            foreground: defaults::foreground(),
            blue: defaults::blue(),
            green: defaults::green(),
            red: defaults::red(),
            yellow: defaults::yellow(),
            bar: defaults::bar(),
            tabs: defaults::tabs(),
            tabs_active: defaults::tabs_active(),
            tab_border: defaults::tab_border(),
            cursor: defaults::cursor(),
            split: defaults::split(),
            split_active: defaults::split_active(),
            vi_cursor: defaults::vi_cursor(),
            black: defaults::black(),
            cyan: defaults::cyan(),
            magenta: defaults::magenta(),
            white: defaults::white(),
            dim_black: None,
            dim_blue: None,
            dim_cyan: None,
            dim_foreground: None,
            dim_green: None,
            dim_magenta: None,
            dim_red: None,
            dim_white: None,
            dim_yellow: None,
            light_black: default_light_black(),
            light_blue: default_light_blue(),
            light_cyan: default_light_cyan(),
            light_foreground: None,
            light_green: default_light_green(),
            light_magenta: default_light_magenta(),
            light_red: default_light_red(),
            light_white: default_light_white(),
            light_yellow: default_light_yellow(),
            selection_background: defaults::selection_background(),
            selection_foreground: defaults::selection_foreground(),
            search_match_background: defaults::search_match_background(),
            search_match_foreground: defaults::search_match_foreground(),
            search_focused_match_background: defaults::search_focused_match_background(),
            search_focused_match_foreground: defaults::search_focused_match_foreground(),
            hint_foreground: defaults::hint_foreground(),
            hint_background: defaults::hint_background(),
        }
    }
}

pub fn hex_to_color_arr(s: &str) -> ColorArray {
    ColorBuilder::from_hex(s.to_string(), Format::SRGB0_1)
        .unwrap_or_default()
        .to_arr()
}

pub fn hex_to_color_wgpu(s: &str) -> ColorWGPU {
    ColorBuilder::from_hex(s.to_string(), Format::SRGB0_1)
        .unwrap_or_default()
        .to_wgpu()
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone, Copy)]
pub struct ColorBuilder {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub alpha: f64,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone, Copy)]
pub struct ColorBuilder8Bits {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl ColorBuilder8Bits {
    pub fn transform_to_color_arr(red: u8, green: u8, blue: u8, alpha: u8) -> ColorArray {
        [red as f32, green as f32, blue as f32, alpha as f32]
    }
}

impl ColorBuilder {
    #[allow(dead_code)]
    fn new(red: f64, green: f64, blue: f64, alpha: f64) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    pub fn from_hex(mut hex: String, conversion_type: Format) -> Result<Self, String> {
        let mut alpha: f64 = 1.0;
        let non_hex_chars = Regex::new(r"(?i)[^#a-f\d]").unwrap();

        // match valid 6 or 8 hex characters
        let valid_hex_size = Regex::new(r"(?i)^#?[a-f\d]{6}([a-f\d]{2})?$").unwrap();

        if non_hex_chars.is_match(&hex) {
            return Err(String::from("Error: Character is not valid"));
        }

        if !valid_hex_size.is_match(&hex) {
            return Err(String::from("Error: Hex String size is not valid"));
        }

        hex = hex.replace('#', "");

        if hex.len() == 8 {
            let (rgb_part, alpha_part) = hex.split_at(6);
            let alpha_from_hex = i32::from_str_radix(alpha_part, 16).unwrap();
            hex = rgb_part.to_string();
            alpha = (alpha_from_hex as f64) / 255.0;
        }

        let rgb = decode_hex(&hex).unwrap_or_default();
        if rgb.is_empty() || (rgb.len() != 3 && rgb.len() != 4) {
            return Err(String::from("Error: Invalid string, not able to convert"));
        }

        match conversion_type {
            Format::SRGB0_1 => Ok(Self {
                red: (rgb[0] as f64) / 255.0,
                green: (rgb[1] as f64) / 255.0,
                blue: (rgb[2] as f64) / 255.0,
                alpha,
            }),
            Format::SRGB0_255 => Ok(Self {
                red: (rgb[0] as f64),
                green: (rgb[1] as f64),
                blue: (rgb[2] as f64),
                alpha,
            }),
        }
    }

    pub fn from_rgb(rgb: ColorRgb, conversion_type: Format) -> Self {
        match conversion_type {
            Format::SRGB0_1 => Self {
                red: (rgb.r as f64) / 255.0,
                green: (rgb.g as f64) / 255.0,
                blue: (rgb.b as f64) / 255.0,
                alpha: 1.0,
            },
            Format::SRGB0_255 => Self {
                red: (rgb.r as f64),
                green: (rgb.g as f64),
                blue: (rgb.b as f64),
                alpha: 1.0,
            },
        }
    }

    pub fn to_wgpu(&self) -> sugarloaf::Color {
        sugarloaf::Color {
            r: self.red,
            g: self.green,
            b: self.blue,
            a: self.alpha,
        }
    }

    pub fn sub_alpha(&mut self, alpha: f64) -> &mut Self {
        self.alpha -= alpha;
        self
    }

    pub fn to_arr(&self) -> ColorArray {
        [
            self.red as f32,
            self.green as f32,
            self.blue as f32,
            self.alpha as f32,
        ]
    }

    pub fn format_string(&self) -> String {
        std::format!(
            "r: {:?}, g: {:?}, b: {:?}, a: {:?}",
            self.red,
            self.green,
            self.blue,
            self.alpha
        )
    }
}

fn decode_hex(s: &str) -> Result<Vec<u8>, ParseIntError> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect()
}

impl Default for ColorBuilder {
    // #000000 Color Hex Black #000
    fn default() -> Self {
        Self {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            alpha: 1.0,
        }
    }
}

impl std::fmt::Display for ColorBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        std::fmt::Display::fmt(&self.format_string(), f)
    }
}

pub fn deserialize_to_wgpu<'de, D>(deserializer: D) -> Result<ColorWGPU, D::Error>
where
    D: de::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    match ColorBuilder::from_hex(s, Format::SRGB0_1) {
        Ok(color) => Ok(color.to_wgpu()),
        Err(e) => Err(serde::de::Error::custom(e)),
    }
}

pub fn deserialize_to_composition<'de, D>(
    deserializer: D,
) -> Result<ColorComposition, D::Error>
where
    D: de::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    match ColorBuilder::from_hex(s, Format::SRGB0_1) {
        Ok(color) => Ok((color.to_arr(), color.to_wgpu())),
        Err(e) => Err(serde::de::Error::custom(e)),
    }
}

pub fn deserialize_to_arr<'de, D>(deserializer: D) -> Result<ColorArray, D::Error>
where
    D: de::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    match ColorBuilder::from_hex(s, Format::SRGB0_1) {
        Ok(color) => Ok(color.to_arr()),
        Err(e) => Err(serde::de::Error::custom(e)),
    }
}

pub fn deserialize_to_arr_opt<'de, D>(
    deserializer: D,
) -> Result<Option<ColorArray>, D::Error>
where
    D: de::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    match ColorBuilder::from_hex(s, Format::SRGB0_1) {
        Ok(color) => Ok(Some(color.to_arr())),
        Err(e) => Err(serde::de::Error::custom(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversion_from_hex_invalid_character() {
        let invalid_character_color = match ColorBuilder::from_hex(
            String::from("#invalid-color"),
            Format::SRGB0_255,
        ) {
            Ok(d) => d.to_string(),
            Err(e) => e,
        };

        assert_eq!(invalid_character_color, "Error: Character is not valid");
    }

    #[test]
    fn test_default_as_black() {
        let default_color: ColorBuilder = ColorBuilder::default();

        assert_eq!(
            ColorBuilder {
                red: 0.0,
                green: 0.0,
                blue: 0.0,
                alpha: 1.0,
            },
            default_color
        );
    }

    #[test]
    fn test_conversion_from_hex_invalid_size() {
        let invalid_invalid_size =
            match ColorBuilder::from_hex(String::from("abc"), Format::SRGB0_255) {
                Ok(d) => d.to_string(),
                Err(e) => e,
            };

        assert_eq!(invalid_invalid_size, "Error: Hex String size is not valid");
    }

    #[test]
    fn test_conversion_from_hex_sgb_255() {
        let color: sugarloaf::Color =
            ColorBuilder::from_hex(String::from("#151515"), Format::SRGB0_1)
                .unwrap()
                .to_wgpu();
        assert_eq!(
            color,
            ColorWGPU {
                r: 0.08235294117647059,
                g: 0.08235294117647059,
                b: 0.08235294117647059,
                a: 1.0
            }
        );

        let color =
            ColorBuilder::from_hex(String::from("#FFFFFF"), Format::SRGB0_1).unwrap();
        assert_eq!(
            color,
            ColorBuilder {
                red: 1.0,
                green: 1.0,
                blue: 1.0,
                alpha: 1.0
            }
        );
    }

    #[test]
    fn test_conversion_from_hex_sgb_1() {
        let color: sugarloaf::Color =
            ColorBuilder::from_hex(String::from("#151515"), Format::SRGB0_255)
                .unwrap()
                .to_wgpu();
        assert_eq!(
            color,
            ColorWGPU {
                r: 21.0,
                g: 21.0,
                b: 21.0,
                a: 1.0
            }
        );

        let color =
            ColorBuilder::from_hex(String::from("#FFFFFF"), Format::SRGB0_255).unwrap();
        assert_eq!(
            color,
            ColorBuilder {
                red: 255.0,
                green: 255.0,
                blue: 255.0,
                alpha: 1.0
            }
        );
    }

    #[test]
    fn test_conversion_from_gray_hex_with_alpha() {
        let color_with_alpha =
            ColorBuilder::from_hex(String::from("#15151580"), Format::SRGB0_255).unwrap();
        assert_eq!(
            color_with_alpha,
            ColorBuilder {
                red: 21.0,
                green: 21.0,
                blue: 21.0,
                alpha: 128.0 / 255.0
            }
        );

        let color_with_alpha_srgb0_1 =
            ColorBuilder::from_hex(String::from("#15151580"), Format::SRGB0_1).unwrap();
        assert_eq!(
            color_with_alpha_srgb0_1,
            ColorBuilder {
                red: 21.0 / 255.0,
                green: 21.0 / 255.0,
                blue: 21.0 / 255.0,
                alpha: 128.0 / 255.0
            }
        );
    }

    #[test]
    fn test_conversion_from_teal_hex_with_alpha() {
        let color_with_alpha =
            ColorBuilder::from_hex(String::from("#06a49b99"), Format::SRGB0_255).unwrap();
        assert_eq!(
            color_with_alpha,
            ColorBuilder {
                red: 6.0,
                green: 164.0,
                blue: 155.0,
                alpha: 153.0 / 255.0
            }
        );

        let color_with_alpha_srgb0_1 =
            ColorBuilder::from_hex(String::from("#06a49b99"), Format::SRGB0_1).unwrap();
        assert_eq!(
            color_with_alpha_srgb0_1,
            ColorBuilder {
                red: 6.0 / 255.0,
                green: 164.0 / 255.0,
                blue: 155.0 / 255.0,
                alpha: 153.0 / 255.0
            }
        );
    }
}
