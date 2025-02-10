use crate::config::colors::NamedColor;
use crate::config::colors::{ColorArray, ColorBuilder, ColorRgb, Format};
use crate::config::Colors;
use std::ops::{Index, IndexMut};

/// Number of terminal colors.
pub const COUNT: usize = 269;

/// Factor for automatic computation of dim colors.
pub const DIM_FACTOR: f32 = 0.66;

/// > The 256 color table and its partitioning
///
/// The color range of a 256 color terminal consists of 4 parts,
/// often 5, in which case you actually get 258 colors:
///
/// Color numbers 0 to 7 are the default terminal colors, the actual RGB
/// value of which is not standardized and can often be configured.
///
/// Color numbers 8 to 15 are the "bright" colors. Most of the time these are a
/// lighter shade of the color with index - 8. They are also not standardized and
/// can often be configured. Depending on terminal and shell, they are often used instead of or in conjunction with bold font faces.
///
/// Color numbers 16 to 231 are RGB colors.
/// These 216 colors are defined by 6 values on each of the three RGB axes.
/// That is, instead of values 0 - 255, each color only ranges from 0 - 5.
///
/// The color number is then calculated like this:
/// number = 16 + 36 * r + 6 * g + b
/// with r, g and b in the range 0 - 5.
///
/// The color numbers 232 to 255 are grayscale with 24 shades
/// of gray from dark to light.
///
/// The default colors for foreground and background.
/// In many terminals they can be configured independently from the
/// 256 indexed colors, giving an additional two configurable colors.
/// You get them when not setting any other color or disabling other colors
/// (i.e. print '\e[m').

#[derive(Copy, Debug, Clone, PartialEq)]
pub struct TermColors([Option<ColorArray>; COUNT]);

impl Default for TermColors {
    fn default() -> Self {
        Self([None; COUNT])
    }
}

impl Index<usize> for TermColors {
    type Output = Option<ColorArray>;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl IndexMut<usize> for TermColors {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

impl Index<NamedColor> for TermColors {
    type Output = Option<ColorArray>;

    #[inline]
    fn index(&self, index: NamedColor) -> &Self::Output {
        &self.0[index as usize]
    }
}

impl IndexMut<NamedColor> for TermColors {
    #[inline]
    fn index_mut(&mut self, index: NamedColor) -> &mut Self::Output {
        &mut self.0[index as usize]
    }
}

#[derive(Copy, Debug, Clone)]
pub struct List([ColorArray; COUNT]);

impl From<&Colors> for List {
    fn from(colors: &Colors) -> List {
        // Type inference fails without this annotation.
        let mut list = List([ColorArray::default(); COUNT]);

        list.fill_named(colors);
        list.fill_cube();
        list.fill_gray_ramp();

        list
    }
}

impl List {
    pub fn fill_named(&mut self, colors: &Colors) {
        self[NamedColor::Black] = colors.black;
        self[NamedColor::Red] = colors.red;
        self[NamedColor::Green] = colors.green;
        self[NamedColor::Yellow] = colors.yellow;
        self[NamedColor::Blue] = colors.blue;
        self[NamedColor::Magenta] = colors.magenta;
        self[NamedColor::Cyan] = colors.cyan;
        self[NamedColor::White] = colors.white;

        // Lights.
        self[NamedColor::LightBlack] = colors.light_black;
        self[NamedColor::LightRed] = colors.light_red;
        self[NamedColor::LightGreen] = colors.light_green;
        self[NamedColor::LightYellow] = colors.light_yellow;
        self[NamedColor::LightBlue] = colors.light_blue;
        self[NamedColor::LightMagenta] = colors.light_magenta;
        self[NamedColor::LightCyan] = colors.light_cyan;
        self[NamedColor::LightWhite] = colors.light_white;

        if let Some(color) = colors.light_foreground {
            self[NamedColor::LightForeground] = color;
        } else {
            self[NamedColor::LightForeground] =
                (ColorRgb::from_color_arr(colors.foreground)).to_arr();
        }

        // Foreground and background.
        self[NamedColor::Foreground] = colors.foreground;
        self[NamedColor::Background] = colors.background.0;

        // Dims.
        if let Some(color) = colors.dim_foreground {
            self[NamedColor::DimForeground] = color;
        } else {
            self[NamedColor::DimForeground] =
                (ColorRgb::from_color_arr(colors.foreground) * DIM_FACTOR).to_arr();
        }

        if let Some(color) = colors.dim_black {
            self[NamedColor::DimBlack] = color;
        } else {
            self[NamedColor::DimBlack] =
                (ColorRgb::from_color_arr(colors.black) * DIM_FACTOR).to_arr();
        }

        if let Some(color) = colors.dim_red {
            self[NamedColor::DimRed] = color;
        } else {
            self[NamedColor::DimRed] =
                (ColorRgb::from_color_arr(colors.red) * DIM_FACTOR).to_arr();
        }

        if let Some(color) = colors.dim_green {
            self[NamedColor::DimGreen] = color;
        } else {
            self[NamedColor::DimGreen] =
                (ColorRgb::from_color_arr(colors.green) * DIM_FACTOR).to_arr();
        }

        if let Some(color) = colors.dim_yellow {
            self[NamedColor::DimYellow] = color;
        } else {
            self[NamedColor::DimYellow] =
                (ColorRgb::from_color_arr(colors.yellow) * DIM_FACTOR).to_arr();
        }

        if let Some(color) = colors.dim_blue {
            self[NamedColor::DimBlue] = color;
        } else {
            self[NamedColor::DimBlue] =
                (ColorRgb::from_color_arr(colors.blue) * DIM_FACTOR).to_arr();
        }

        if let Some(color) = colors.dim_magenta {
            self[NamedColor::DimMagenta] = color;
        } else {
            self[NamedColor::DimMagenta] =
                (ColorRgb::from_color_arr(colors.magenta) * DIM_FACTOR).to_arr();
        }

        if let Some(color) = colors.dim_cyan {
            self[NamedColor::DimCyan] = color;
        } else {
            self[NamedColor::DimCyan] =
                (ColorRgb::from_color_arr(colors.cyan) * DIM_FACTOR).to_arr();
        }

        if let Some(color) = colors.dim_white {
            self[NamedColor::DimWhite] = color;
        } else {
            self[NamedColor::DimWhite] =
                (ColorRgb::from_color_arr(colors.white) * DIM_FACTOR).to_arr();
        }
    }

    pub fn fill_cube(&mut self) {
        let mut index: usize = 16;
        // Build colors.
        for r in 0..6 {
            for g in 0..6 {
                for b in 0..6 {
                    let rgb = ColorRgb {
                        r: if r == 0 { 0 } else { r * 40 + 55 },
                        b: if b == 0 { 0 } else { b * 40 + 55 },
                        g: if g == 0 { 0 } else { g * 40 + 55 },
                    };

                    let arr = ColorBuilder::from_rgb(rgb, Format::SRGB0_1).to_arr();
                    self[index] = arr;
                    index += 1;
                }
            }
        }

        debug_assert!(index == 232);
    }

    pub fn fill_gray_ramp(&mut self) {
        let mut index: usize = 232;

        for i in 0..24 {
            let value = i * 10 + 8;
            let rgb = ColorRgb {
                r: value,
                g: value,
                b: value,
            };
            let arr = ColorBuilder::from_rgb(rgb, Format::SRGB0_1).to_arr();
            self[index] = arr;
            index += 1;
        }

        debug_assert!(index == 256);
    }
}

impl Index<usize> for List {
    type Output = ColorArray;

    #[inline]
    fn index(&self, idx: usize) -> &Self::Output {
        &self.0[idx]
    }
}

impl IndexMut<usize> for List {
    #[inline]
    fn index_mut(&mut self, idx: usize) -> &mut Self::Output {
        &mut self.0[idx]
    }
}

impl Index<NamedColor> for List {
    type Output = ColorArray;

    #[inline]
    fn index(&self, idx: NamedColor) -> &Self::Output {
        &self.0[idx as usize]
    }
}

impl IndexMut<NamedColor> for List {
    #[inline]
    fn index_mut(&mut self, idx: NamedColor) -> &mut Self::Output {
        &mut self.0[idx as usize]
    }
}
