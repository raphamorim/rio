// glyph module code along with comments was originally retired from glyph-brush
// https://github.com/alexheretic/glyph-brush
// glyph-brush was originally written Alex Butler (https://github.com/alexheretic)
// and licensed under Apache-2.0 license.

use crate::components::text::glyph::FontId;
use ab_glyph::*;
use std::f32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionGeometry {
    /// Position on screen to render text, in pixels from top-left. Defaults to (0, 0).
    pub screen_position: (f32, f32),
    /// Max (width, height) bounds, in pixels from top-left. Defaults to unbounded.
    pub bounds: (f32, f32),
}

impl Default for SectionGeometry {
    #[inline]
    fn default() -> Self {
        Self {
            screen_position: (0.0, 0.0),
            bounds: (f32::INFINITY, f32::INFINITY),
        }
    }
}

/// Text to layout together using a font & scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionText<'a> {
    /// Text to render
    pub text: &'a str,
    /// Pixel scale of text. Defaults to 16.
    pub scale: PxScale,
    /// Font id to use for this section.
    ///
    /// It must be a valid id in the `FontMap` used for layout calls.
    /// The default `FontId(0)` should always be valid.
    pub font_id: FontId,
}

impl Default for SectionText<'static> {
    #[inline]
    fn default() -> Self {
        Self {
            text: "",
            scale: PxScale::from(16.0),
            font_id: FontId::default(),
        }
    }
}

pub trait ToSectionText {
    fn to_section_text(&self) -> SectionText<'_>;
}

impl ToSectionText for SectionText<'_> {
    #[inline]
    fn to_section_text(&self) -> SectionText<'_> {
        *self
    }
}

impl ToSectionText for &SectionText<'_> {
    #[inline]
    fn to_section_text(&self) -> SectionText<'_> {
        **self
    }
}

/// A positioned glyph with info relating to the `SectionText` from which it was derived.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct SectionGlyph {
    /// The `SectionText` index.
    pub section_index: usize,
    /// The character byte index from the `SectionText` text.
    pub byte_index: usize,
    /// A positioned glyph.
    pub glyph: Glyph,
    /// Font id.
    pub font_id: FontId,
}
