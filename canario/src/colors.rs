//! The engine-facing terminal color table.
//!
//! `TermColors` is the live, per-terminal palette the engine mutates in
//! response to OSC color sequences (OSC 4/10/11/12/…): a fixed-size array of
//! optional [`ColorArray`] slots indexed by raw slot number or by
//! [`NamedColor`]. It carries no config/serde/`ColorBuilder` logic — that
//! lives in the frontend, which seeds and reads this table at the render
//! boundary (see `rio-backend::config::colors::term::List`). Only
//! [`rio_core::color`] types are referenced here, so the engine stays
//! config-agnostic.

use rio_core::color::{ColorArray, NamedColor};
use std::ops::{Index, IndexMut};

/// Number of terminal color slots.
///
/// The color range of a 256 color terminal consists of 4 parts (often 5, in
/// which case you actually get 258 colors):
///
/// - Color numbers 0 to 7 are the default terminal colors, whose actual RGB
///   value is not standardized and can often be configured.
/// - Color numbers 8 to 15 are the "bright" colors, usually a lighter shade
///   of `index - 8`; also not standardized and often configurable.
/// - Color numbers 16 to 231 are RGB colors: 216 colors defined by 6 values
///   on each of the three RGB axes (`number = 16 + 36*r + 6*g + b`, with
///   `r,g,b` in `0..=5`).
/// - Color numbers 232 to 255 are grayscale, 24 shades from dark to light.
///
/// Rio extends this with the foreground/background and the dim/light/UI slots
/// past 255, for a total of [`COUNT`].
pub const COUNT: usize = 269;

/// The live, per-terminal palette: one optional [`ColorArray`] per slot. A
/// `None` slot means "use the configured default"; the engine writes `Some`
/// when a program overrides a slot via OSC.
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
