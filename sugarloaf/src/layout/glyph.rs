// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// layout_data.rs was originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE

use crate::font_introspector::shape::cluster::Glyph as ShapedGlyph;
use crate::font_introspector::GlyphId;
use crate::layout::FragmentStyle;

pub const GLYPH_DETAILED: u32 = 0x80000000;

#[derive(Copy, Debug, Clone)]
pub struct GlyphData {
    pub data: u32,
    pub size: usize,
}

impl GlyphData {
    pub fn simple(id: u16, advance: f32, size: usize) -> Self {
        let advance = (advance * 64.).max(0.) as u32;
        Self {
            data: id as u32 | ((advance & 0x7FFF) << 16),
            size,
        }
    }

    pub fn is_simple(self) -> bool {
        self.data & GLYPH_DETAILED == 0
    }

    pub fn simple_data(self) -> (u16, f32) {
        ((self.data & 0xFFFF) as u16, (self.data >> 16) as f32 / 64.)
    }

    pub fn detail_index(self) -> usize {
        (self.data & !GLYPH_DETAILED) as usize
    }

    pub fn add_spacing(&mut self, spacing: f32) {
        let (id, advance) = self.simple_data();
        *self = Self::simple(id, (advance + spacing).max(0.), self.size);
    }

    pub fn clear_advance(&mut self) {
        let (id, _advance) = self.simple_data();
        *self = Self::simple(id, 0., self.size);
    }
}

#[derive(Debug, Clone)]
pub struct RunData {
    pub span: FragmentStyle,
    pub line: u32,
    pub size: f32,
    pub glyphs: Vec<GlyphData>,
    pub detailed_glyphs: Vec<Glyph>,
    pub ascent: f32,
    pub descent: f32,
    pub leading: f32,
    pub underline_offset: f32,
    pub strikeout_offset: f32,
    pub strikeout_size: f32,
    pub x_height: f32,
    pub advance: f32,
}

/// Shaped glyph in a paragraph.
#[derive(Copy, Debug, Clone)]
pub struct Glyph {
    /// Glyph identifier.
    pub id: GlyphId,
    /// Horizontal offset.
    pub x: f32,
    /// Vertical offset.
    pub y: f32,
    /// Advance width or height.
    pub advance: f32,
    /// Span that generated the glyph.
    pub span: usize,
}

impl Glyph {
    pub fn new(g: &ShapedGlyph) -> Self {
        Self {
            id: g.id,
            x: g.x,
            y: g.y,
            advance: g.advance,
            span: g.data as usize,
        }
    }
}
