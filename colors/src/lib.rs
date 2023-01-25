use regex::Regex;

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Rgba {
    r: u8,
    g: u8,
    b: u8,
    a: u8
}

impl Rgba {
    fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r,
            g,
            b,
            a
        }
    }

    pub fn from_hex(hex: String) -> Self {
        let hexCharacters = "a-f\\d";
        let match3or4Hex = "#?[${hexCharacters}]{3}[${hexCharacters}]?";
        let match6or8Hex = "#?[${hexCharacters}]{6}([${hexCharacters}]{2})?";
        let nonHexChars = Regex::new(r"[^#${hexCharacters}]", "gi");
        let validHexSize = Regex::new(r"^${match3or4Hex}$|^${match6or8Hex}$", 'i');

        if (typeof hex !== "string" || nonHexChars.test(hex) || !validHexSize.test(hex)) {
            return Rgba::default();
        }

        hex = hex.replace(/^#/, '');
        let alphaFromHex = 1;

        if (hex.length === 8) {
         alphaFromHex = Number.parseInt(hex.slice(6, 8), 16) / 255;
         hex = hex.slice(0, 6);
        }

        if (hex.length === 4) {
         alphaFromHex = Number.parseInt(hex.slice(3, 4).repeat(2), 16) / 255;
         hex = hex.slice(0, 3);
        }

        if (hex.length === 3) {
         hex = hex[0] + hex[0] + hex[1] + hex[1] + hex[2] + hex[2];
        }

        const number = Number.parseInt(hex, 16);
        const red = number >> 16;
        const green = (number >> 8) & 255;
        const blue = number & 255;
        const alpha = typeof options.alpha === "number" ? options.alpha : alphaFromHex;


        Self {
            r: 0, g: 0, b: 0, a: 0
        }
    }
}

impl Default for Rgba {
    // #000000 Color Hex Black #000
    fn default() -> Self {
        Self {
            r: 0, g: 0, b: 0, a: 0
        }
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
    fn load_default_config() {
        let color = Rgba::from_hex(String::from("#151515"));

        assert_eq!(color, Rgba {
            r: 0, g: 0, b: 0, a: 0
        });
    }
}
