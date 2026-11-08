// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Glyph Protocol sizing and placement values (spec §8.5). Plain data,
//! outside the `glyph` feature, because the wire parser needs them even
//! in builds without the registry or outline decoder.

/// `size` scale policy (spec §8.5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GlyphSize {
    Height,
    Advance,
    #[default]
    Contain,
    Cover,
    Stretch,
}

/// Horizontal half of `align` (spec §8.5.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GlyphAlignH {
    Start,
    #[default]
    Center,
    End,
}

/// Vertical half of `align` (spec §8.5.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GlyphAlignV {
    Start,
    #[default]
    Center,
    End,
    Baseline,
}

/// Explicit sizing and placement for a registration (spec §8.5).
/// Defaults are the protocol's own.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GlyphLayout {
    /// Authored advance width in upm units.
    pub aw: Option<u16>,
    /// Authored line height in upm units.
    pub lh: Option<u16>,
    /// Bottom of the authored extent in upm units. If any of `aw`, `lh`
    /// or `ymin` is set the extent is declared and the rest default to
    /// `upm`, `upm` and `0`; otherwise it is the outline's bounds.
    pub ymin: Option<i16>,
    pub size: GlyphSize,
    pub align_h: GlyphAlignH,
    pub align_v: GlyphAlignV,
    /// Insets as `[top, right, bottom, left]` fractions. Never
    /// degenerate: the parser normalises those to zero.
    pub pad: [f32; 4],
}
