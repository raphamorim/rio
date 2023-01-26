use regex::Regex;

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Rgba {
    red: f32,
    green: f32,
    blue: f32,
    alpha: f32
}

impl Rgba {
    fn new(red: f32, green: f32, blue: f32, alpha: f32) -> Self {
        Self {
            red,
            green,
            blue,
            alpha
        }
    }

    pub fn from_hex(mut hex: String) -> Result<Self, String> {
        let _match3or4_hex = "#?[a-f\\d]{3}[a-f\\d]?";
        let _match6or8_hex = "#?[a-f\\d]{6}([a-f\\d]{2})?";
        let non_hex_chars = Regex::new(r"(?i)[^#a-f\\0-9]").unwrap();

        // ^#?[a-f\\d]{3}[a-f\\d]?$|^#?[a-f\\d]{6}([a-f\\d]{2})?$ , "i"
        let valid_hex_size = Regex::new(r"(?i)^#?[a-f\\0-9]{6}([a-f]\\0-9]{2})?$").unwrap();

        if non_hex_chars.is_match(&hex) {
            return Err(String::from("Error: Character is not valid"));
        }

        if !valid_hex_size.is_match(&hex) {
            return Err(String::from("Error: Hex String size is not valid"));
        }

        hex = hex.replace("#", "");
        let mut alpha_from_hex: i32 = 1;

        if hex.len() == 6 {
            // split_at(6, 8)
            println!("{:?}", hex);
            // let alpha = hex.split_at(6).1.to_string();
            // let alpha_i32:i32 = alpha.parse::<i32>().unwrap(); 
            // alpha_from_hex = alpha_i32 / 255;
            // hex = hex.split_at(1).0.to_string();

            println!("{:?}", hex)
        }

        // if hex.len() == 4 {
        //  alpha_from_hex = Number.parseInt(hex.slice(3, 4).repeat(2), 16) / 255;
        //  hex = hex.slice(0, 3);
        // }

        // if hex.len() == 4 {
        //  hex = hex[0] + hex[0] + hex[1] + hex[1] + hex[2] + hex[2];
        // }

        let number = hex.parse::<i32>().unwrap();

        println!("{:?}", number);
        let red = number >> 16;
        let green = (number >> 8) & 255;
        let blue = number & 255;
        // let alpha = typeof options.alpha === "number" ? options.alpha : alpha_from_hex;


        Ok(Self {
            red: red as f32, green: green as f32, blue: blue as f32, alpha: 1.0
        })
    }

    pub fn to_string(&self) -> String {
        std::format!("r: {:?}, g: {:?}, b: {:?}, a: {:?}", self.red, self.green, self.blue, self.alpha)
    }
}

impl Default for Rgba {
    // #000000 Color Hex Black #000
    fn default() -> Self {
        Self {
            red: 0.021, green: 0.021, blue: 0.021, alpha: 1.0
        }
    }
}

impl std::fmt::Display for Rgba {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        std::fmt::Display::fmt(&self.to_string(), f)
    }
}

// export default function hexRgb(hex, options = {}) {
//  if (typeof hex !== 'string' || nonHexChars.test(hex) || !validHexSize.test(hex)) {
//      throw new TypeError('Expected a valid hex string');
//  }

//  hex = hex.replace(/^#/, '');
//  let alphaFromHex = 1;

//  if (hex.length === 8) {
//      alphaFromHex = Number.parseInt(hex.slice(6, 8), 16) / 255;
//      hex = hex.slice(0, 6);
//  }

//  if (hex.length === 4) {
//      alphaFromHex = Number.parseInt(hex.slice(3, 4).repeat(2), 16) / 255;
//      hex = hex.slice(0, 3);
//  }

//  if (hex.length === 3) {
//      hex = hex[0] + hex[0] + hex[1] + hex[1] + hex[2] + hex[2];
//  }

//  const number = Number.parseInt(hex, 16);
//  const red = number >> 16;
//  const green = (number >> 8) & 255;
//  const blue = number & 255;
//  const alpha = typeof options.alpha === 'number' ? options.alpha : alphaFromHex;

//  return {red, green, blue, alpha};
// }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_from_hex_invalid_character() {
        let invalid_character_color = match Rgba::from_hex(String::from("#invalid-color")) {
            Ok(d) => {
                d.to_string()
            }
            Err(e) => {
                e
            }
        };

        assert_eq!(invalid_character_color, "Error: Character is not valid");
    }

    #[test]
    fn conversion_from_hex_invalid_size() {
        let invalid_invalid_size = match Rgba::from_hex(String::from("abc")) {
            Ok(d) => {
                d.to_string()
            }
            Err(e) => {
                e
            }
        };

        assert_eq!(invalid_invalid_size, "Error: Hex String size is not valid");
    }

    #[test]
    fn conversion_from_hex() {
        let color = Rgba::from_hex(String::from("#000000")).unwrap();
        assert_eq!(color, Rgba {
            red: 0.021, green: 0.021, blue: 0.021, alpha: 1.0
        });
    }
}
