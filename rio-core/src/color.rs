//! The terminal color model — relocated from `rio-backend::config::colors`.
//!
//! These are terminal-engine concepts (an SGR color is `Named`, a 256-color
//! `Indexed`, or a true-color `Spec`) that merely happened to live under
//! `config`. The frontend-only conversions (`to_wgpu`, `to_composition`,
//! gamma/`ColorBuilder` formatting) deliberately do NOT move here — they
//! stay in the frontend, which converts from these types at the render
//! boundary. See `canario/DESIGN.md` §5 Severance 1.

use std::ops::Mul;

/// A normalized RGBA color as consumed by a renderer (`[r, g, b, a]`, each
/// in `0.0..=1.0`). The engine indexes a palette of these; how they are
/// produced from [`ColorRgb`] (gamma, color space) is a frontend concern.
pub type ColorArray = [f32; 4];

/// Multiplier applied to a color to produce its "dim" (SGR 2) variant.
pub const DIM_FACTOR: f32 = 0.66;

/// An 8-bit-per-channel RGB color. Plain data — no GPU/format logic.
#[derive(Debug, Default, Copy, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ColorRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl ColorRgb {
    #[inline]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// The dim (SGR 2) variant of this color.
    #[inline]
    pub fn dim(self) -> Self {
        self * DIM_FACTOR
    }

    /// Normalize this color to a linear `[r/255, g/255, b/255, 1.0]` array.
    pub fn to_arr(&self) -> ColorArray {
        [
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
            1.0,
        ]
    }

    /// Like [`to_arr`](Self::to_arr), but each channel is first dimmed by
    /// [`DIM_FACTOR`] (truncated to `u8`) — the SGR 2 faint variant.
    pub fn to_arr_with_dim(&self) -> ColorArray {
        let d = ColorRgb {
            r: (self.r as f32 * 0.66) as u8,
            g: (self.g as f32 * 0.66) as u8,
            b: (self.b as f32 * 0.66) as u8,
        };
        d.to_arr()
    }

    /// Build a [`ColorRgb`] from a normalized `[r, g, b, _]` array
    /// (the alpha channel is ignored).
    pub fn from_color_arr(arr: ColorArray) -> ColorRgb {
        ColorRgb {
            r: (arr[0] * 255.0) as u8,
            g: (arr[1] * 255.0) as u8,
            b: (arr[2] * 255.0) as u8,
        }
    }
}

impl From<&ColorRgb> for ColorArray {
    fn from(c: &ColorRgb) -> ColorArray {
        c.to_arr()
    }
}

impl Mul<f32> for ColorRgb {
    type Output = ColorRgb;

    fn mul(self, rhs: f32) -> ColorRgb {
        ColorRgb {
            r: (f32::from(self.r) * rhs).clamp(0.0, 255.0) as u8,
            g: (f32::from(self.g) * rhs).clamp(0.0, 255.0) as u8,
            b: (f32::from(self.b) * rhs).clamp(0.0, 255.0) as u8,
        }
    }
}

/// A color as it appears in an SGR sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AnsiColor {
    /// One of the named palette slots (see [`NamedColor`]).
    Named(NamedColor),
    /// A true-color (24-bit) specification.
    Spec(ColorRgb),
    /// An index into the 256-color palette.
    Indexed(u8),
}

/// The named palette slots. Discriminants are stable: `0..=15` are the
/// standard + bright ANSI colors and `256..` are the special UI slots
/// (foreground/background/cursor) and the derived dim/light variants.
#[derive(Debug, Copy, Clone, Eq, PartialEq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum NamedColor {
    Black = 0,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    LightBlack,
    LightRed,
    LightGreen,
    LightYellow,
    LightBlue,
    LightMagenta,
    LightCyan,
    LightWhite,
    /// The foreground color.
    Foreground = 256,
    /// The background color.
    Background,
    /// Color for the cursor itself.
    Cursor,
    DimBlack,
    DimRed,
    DimGreen,
    DimYellow,
    DimBlue,
    DimMagenta,
    DimCyan,
    DimWhite,
    /// The bright foreground color.
    LightForeground,
    /// Dim foreground.
    DimForeground,
}

impl NamedColor {
    /// The "bright" counterpart of this color (SGR bold-as-bright).
    #[must_use]
    pub fn to_light(self) -> Self {
        match self {
            NamedColor::Foreground => NamedColor::LightForeground,
            NamedColor::Black => NamedColor::LightBlack,
            NamedColor::Red => NamedColor::LightRed,
            NamedColor::Green => NamedColor::LightGreen,
            NamedColor::Yellow => NamedColor::LightYellow,
            NamedColor::Blue => NamedColor::LightBlue,
            NamedColor::Magenta => NamedColor::LightMagenta,
            NamedColor::Cyan => NamedColor::LightCyan,
            NamedColor::White => NamedColor::LightWhite,
            NamedColor::DimForeground => NamedColor::Foreground,
            NamedColor::DimBlack => NamedColor::Black,
            NamedColor::DimRed => NamedColor::Red,
            NamedColor::DimGreen => NamedColor::Green,
            NamedColor::DimYellow => NamedColor::Yellow,
            NamedColor::DimBlue => NamedColor::Blue,
            NamedColor::DimMagenta => NamedColor::Magenta,
            NamedColor::DimCyan => NamedColor::Cyan,
            NamedColor::DimWhite => NamedColor::White,
            other => other,
        }
    }

    /// The "dim" counterpart of this color (SGR 2 faint).
    #[must_use]
    pub fn to_dim(self) -> Self {
        match self {
            NamedColor::Foreground => NamedColor::DimForeground,
            NamedColor::Black => NamedColor::DimBlack,
            NamedColor::Red => NamedColor::DimRed,
            NamedColor::Green => NamedColor::DimGreen,
            NamedColor::Yellow => NamedColor::DimYellow,
            NamedColor::Blue => NamedColor::DimBlue,
            NamedColor::Magenta => NamedColor::DimMagenta,
            NamedColor::Cyan => NamedColor::DimCyan,
            NamedColor::White => NamedColor::DimWhite,
            NamedColor::LightForeground => NamedColor::Foreground,
            NamedColor::LightBlack => NamedColor::Black,
            NamedColor::LightRed => NamedColor::Red,
            NamedColor::LightGreen => NamedColor::Green,
            NamedColor::LightYellow => NamedColor::Yellow,
            NamedColor::LightBlue => NamedColor::Blue,
            NamedColor::LightMagenta => NamedColor::Magenta,
            NamedColor::LightCyan => NamedColor::Cyan,
            NamedColor::LightWhite => NamedColor::White,
            other => other,
        }
    }
}
