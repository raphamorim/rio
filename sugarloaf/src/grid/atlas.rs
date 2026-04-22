// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Shared atlas types used by both the Metal and wgpu grid backends.
//!
//! The atlas texture itself is backend-specific (Metal `Texture` vs
//! wgpu `Texture`), so each backend owns its own atlas struct. These
//! types are the common vocabulary: how callers identify a glyph
//! (`GlyphKey`), where it landed in the atlas (`AtlasSlot`), and the
//! caller-supplied rasterized pixels (`RasterizedGlyph`).

/// Identifier for a rasterized glyph. `(font_id, glyph_id)` is
/// enough when a grid renders at one font size; `size_bucket` lets
/// us share the atlas across minor size changes (e.g. during a
/// resize animation) without re-rasterizing. Quantize to 1/4 of a
/// physical pixel to keep the cache hit rate high:
/// `size_bucket = (scaled_px * 4.0).round() as u16`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct GlyphKey {
    pub font_id: u32,
    pub glyph_id: u32,
    pub size_bucket: u16,
}

/// Atlas position + glyph metrics for one rasterized glyph. Exactly
/// the fields the `grid_text_vertex` shader reads via `CellText`:
/// `glyph_pos`, `glyph_size`, `bearings`.
#[derive(Clone, Copy, Debug, Default)]
pub struct AtlasSlot {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    pub bearing_x: i16,
    pub bearing_y: i16,
}

/// Raw rasterized glyph bitmap, caller-supplied. The atlas doesn't
/// rasterize itself — that stays in whatever shaping / scaling path
/// the caller uses (sugarloaf's swash-backed `ScaleContext`).
pub struct RasterizedGlyph<'a> {
    pub width: u16,
    pub height: u16,
    pub bearing_x: i16,
    pub bearing_y: i16,
    /// R8 pixels, row-major, length `width * height`. No row stride —
    /// the atlas upload uses `bytes_per_row = width`.
    pub bytes: &'a [u8],
}
