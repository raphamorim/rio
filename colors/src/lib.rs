// Produces WGPU Color based on ColorBuilder

use regex::Regex;
use serde::{de, Deserialize};
use std::num::ParseIntError;

#[derive(Debug, Clone, Copy)]
pub enum Format {
    SRGB0_255,
    SRGB0_1,
}

#[derive(Debug, PartialEq, Deserialize, Clone, Copy)]
pub struct ColorBuilder {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub alpha: f64,
}

pub type Color = wgpu::Color;
pub type ColorArray = [f32; 4];

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
        let _match3or4_hex = "#?[a-f\\d]{3}[a-f\\d]?";
        let _match6or8_hex = "#?[a-f\\d]{6}([a-f\\d]{2})?";
        let non_hex_chars = Regex::new(r"(?i)[^#a-f\\0-9]").unwrap();

        // ^#?[a-f\\d]{3}[a-f\\d]?$|^#?[a-f\\d]{6}([a-f\\d]{2})?$ , "i"
        let valid_hex_size =
            Regex::new(r"(?i)^#?[a-f\\0-9]{6}([a-f]\\0-9]{2})?$").unwrap();

        if non_hex_chars.is_match(&hex) {
            return Err(String::from("Error: Character is not valid"));
        }

        if !valid_hex_size.is_match(&hex) {
            return Err(String::from("Error: Hex String size is not valid"));
        }

        hex = hex.replace('#', "");

        if hex.len() == 8 {
            // split_at(6, 8)
            let items = hex.split_at(4);
            let alpha_from_hex = items.1.to_string().parse::<i32>().unwrap();
            hex = items.0.to_string();
            alpha = (alpha_from_hex / 255) as f64;
            // hex = hex.split_at(1).0.to_string();
        }

        let rgb = decode_hex(&hex).unwrap_or_default();
        if rgb.is_empty() || rgb.len() > 4 {
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

    pub fn to_wgpu(&self) -> wgpu::Color {
        wgpu::Color {
            r: self.red,
            g: self.green,
            b: self.blue,
            a: self.alpha,
        }
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

pub fn deserialize_to_wpgu<'de, D>(deserializer: D) -> Result<Color, D::Error>
where
    D: de::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    match ColorBuilder::from_hex(s, Format::SRGB0_1) {
        Ok(color) => Ok(color.to_wgpu()),
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
    fn test_default_color_as_black() {
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
        let color: wgpu::Color =
            ColorBuilder::from_hex(String::from("#151515"), Format::SRGB0_1)
                .unwrap()
                .to_wgpu();
        assert_eq!(
            color,
            Color {
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
        let color: wgpu::Color =
            ColorBuilder::from_hex(String::from("#151515"), Format::SRGB0_255)
                .unwrap()
                .to_wgpu();
        assert_eq!(
            color,
            Color {
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
}
