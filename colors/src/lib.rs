use regex::Regex;
use serde::{de, Deserialize};
use std::num::ParseIntError;

#[derive(Debug, PartialEq, Deserialize, Clone, Copy)]
pub struct Rgba {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub alpha: f32,
}

impl Rgba {
    #[allow(dead_code)]
    fn new(red: f32, green: f32, blue: f32, alpha: f32) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    pub fn from_hex(mut hex: String) -> Result<Self, String> {
        let mut alpha: f32 = 1.0;
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
            alpha = (alpha_from_hex / 255) as f32;
            // hex = hex.split_at(1).0.to_string();
        }

        // if hex.len() == 4 {
        //  alpha_from_hex = Number.parseInt(hex.slice(3, 4).repeat(2), 16) / 255;
        //  hex = hex.slice(0, 3);
        // }

        // if hex.len() == 3 {
        //  hex = hex[0] + hex[0] + hex[1] + hex[1] + hex[2] + hex[2];
        // }

        let rgb = decode_hex(&hex).unwrap_or_default();
        if rgb.is_empty() || rgb.len() > 4 {
            return Err(String::from("Error: Invalid string, not able to convert"));
        }

        // let number = hex.parse::<i32>().unwrap();
        // let red = number >> 16;
        // let green = (number >> 8) & 255;
        // let blue = number & 255;
        // let alpha = typeof options.alpha === "number" ? options.alpha : alpha_from_hex;

        Ok(Self {
            red: (rgb[0] as f32) / 1000.0,
            green: (rgb[1] as f32) / 1000.0,
            blue: (rgb[2] as f32) / 1000.0,
            alpha,
        })
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

impl Default for Rgba {
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

impl std::fmt::Display for Rgba {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        std::fmt::Display::fmt(&self.format_string(), f)
    }
}

pub fn deserialize_hex_string<'de, D>(deserializer: D) -> Result<Rgba, D::Error>
where
    D: de::Deserializer<'de>,
{
    let s: &str = de::Deserialize::deserialize(deserializer)?;
    match Rgba::from_hex(s.to_string()) {
        Ok(color) => Ok(color),
        Err(e) => Err(serde::de::Error::custom(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversion_from_hex_invalid_character() {
        let invalid_character_color = match Rgba::from_hex(String::from("#invalid-color"))
        {
            Ok(d) => d.to_string(),
            Err(e) => e,
        };

        assert_eq!(invalid_character_color, "Error: Character is not valid");
    }

    #[test]
    fn test_default_color_as_black() {
        let default_color: Rgba = Rgba::default();

        assert_eq!(
            Rgba {
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
        let invalid_invalid_size = match Rgba::from_hex(String::from("abc")) {
            Ok(d) => d.to_string(),
            Err(e) => e,
        };

        assert_eq!(invalid_invalid_size, "Error: Hex String size is not valid");
    }

    #[test]
    fn test_conversion_from_hex() {
        let color = Rgba::from_hex(String::from("#151515")).unwrap();
        assert_eq!(
            color,
            Rgba {
                red: 0.021,
                green: 0.021,
                blue: 0.021,
                alpha: 1.0
            }
        );

        let color = Rgba::from_hex(String::from("#000000")).unwrap();
        assert_eq!(
            color,
            Rgba {
                red: 0.0,
                green: 0.0,
                blue: 0.0,
                alpha: 1.0
            }
        );
    }
}
