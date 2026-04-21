// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// text.rs was originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE
//
// Eventually the file had updates to support other features like background-color,
// text color, underline color and etc.

use crate::font_introspector::{GlyphId, NormalizedCoord};
use crate::layout::SpanStyleDecoration;
use crate::sugarloaf::primitives::{DrawableChar, SugarCursor};

/// Properties for a text run.
#[derive(Copy, Clone)]
pub struct TextRunStyle<'a> {
    /// Normalized variation coordinates for the font.
    pub font_coords: &'a [NormalizedCoord],
    /// Font size.
    pub font_size: f32,
    /// Color of the text.
    pub color: [f32; 4],
    /// Background of the text.
    pub background_color: Option<[f32; 4]>,
    /// Baseline of the run.
    pub baseline: f32,
    /// Topline of the run (basically y axis).
    pub topline: f32,
    /// Absolute line height of the run.
    pub line_height: f32,
    /// Padding y
    pub padding_y: f32,
    /// Absolute line height of the run without mod.
    pub line_height_without_mod: f32,
    /// Total advance of the run.
    pub advance: f32,
    /// Underline style.
    pub decoration: Option<SpanStyleDecoration>,
    /// Underline style.
    pub decoration_color: Option<[f32; 4]>,
    /// Cursor style.
    pub cursor: Option<SugarCursor>,
    pub drawable_char: Option<DrawableChar>,
    /// Font metrics for proper underline/strikethrough positioning
    pub underline_offset: f32,
    pub strikeout_offset: f32,
    pub underline_thickness: f32,
    pub x_height: f32,
    /// Font ascent and descent for cursor positioning
    pub ascent: f32,
    pub descent: f32,
    /// Scale constraint: (cell_width_px, constraint_cells).
    /// None for normal glyphs. When set, the glyph is scaled to fit
    /// constraint_cells * cell_width_px without affecting positioning.
    pub scale_constraint: Option<(f32, u8)>,
    /// Per-glyph Nerd Font constraint from ghostty's patcher table.
    /// When set, overrides the compositor's cell-centered fit with
    /// ghostty's constrain() math — size + alignment + padding. Applies
    /// only to PUA glyphs that have a hand-tuned entry in the table.
    pub nerd_font_constraint: Option<crate::font::nerd_font_attributes::Constraint>,
    /// Run is composed of Glyph Protocol custom glyphs. Custom glyphs
    /// live at PUA codepoints with no Nerd Font patcher entry, so the
    /// compositor's generic "natural position" fallback would leave
    /// them uncentered in the cell. This flag restores the fit +
    /// slot-center path for those runs only.
    pub is_custom_glyph_run: bool,
}

/// Positioned glyph in a text run.
#[derive(Copy, Clone)]
pub struct Glyph {
    /// Glyph identifier.
    pub id: GlyphId,
    /// X offset of the glyph.
    pub x: f32,
    /// Y offset of the glyph.
    pub y: f32,
    /// Horizontal advance. Used by the compositor to fit bitmap glyphs
    /// (emoji) into their per-glyph cell slot.
    pub advance: f32,
}
