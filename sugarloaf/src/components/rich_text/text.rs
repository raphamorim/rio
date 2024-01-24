use crate::components::rich_text::color::Color;
use swash::{FontRef, GlyphId, NormalizedCoord};

/// Properties for a text run.
#[derive(Copy, Clone)]
pub struct TextRunStyle<'a> {
    /// Font for the run.
    pub font: FontRef<'a>,
    /// Normalized variation coordinates for the font.
    pub font_coords: &'a [NormalizedCoord],
    /// Font size.
    pub font_size: f32,
    /// Color of the text.
    pub color: [f32; 4],
    /// Baseline of the run.
    pub baseline: f32,
    /// Total advance of the run.
    pub advance: f32,
    /// Underline style.
    pub underline: Option<UnderlineStyle>,
}

/// Underline decoration style.
#[derive(Copy, Clone)]
pub struct UnderlineStyle {
    /// Offset of the underline stroke.
    pub offset: f32,
    /// Thickness of the underline stroke.
    pub size: f32,
    /// Color of the underline.
    pub color: [f32; 4],
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
}
