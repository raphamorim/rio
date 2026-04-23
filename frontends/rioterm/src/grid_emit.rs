// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Translates terminal `Square` cells into `CellBg` / `CellText`
//! instances for the grid GPU renderer.
//!
//! `build_row_bg` is one CellBg per cell; `build_row_fg` does
//! **run-level shaping** so ligatures (`=>`, `!=`, `fi`) form
//! correctly — a contiguous run of cells sharing `(font_id,
//! style_flags)` is shaped in one call, and one `CellText` is emitted
//! per resulting glyph (not per input cell).
//!
//! Shape + rasterize backends split by platform:
//! - **macOS**: CoreText via `font::macos::shape_text` /
//!   `rasterize_glyph`.
//! - **non-macOS**: swash `ShapeContext` + `ScaleContext`.
//!
//! Both populate the same `ShapedGlyph` shape and route into the same
//! `GridRenderer` atlases via the same emit loop.
//!
//! Mirrors Ghostty's `font::shaper::run::RunIterator` (`run.zig`).

use rio_backend::config::colors::term::TermColors;
use rio_backend::crosswords::grid::row::Row;
use rio_backend::crosswords::pos::Column;
use rio_backend::crosswords::square::{ContentTag, Square};
use rio_backend::crosswords::style::{StyleFlags, StyleSet};
use rustc_hash::FxHashMap;

use crate::renderer::Renderer;

use rio_backend::sugarloaf::font::FontLibrary;
use rio_backend::sugarloaf::grid::{
    AtlasSlot, CellBg, CellText, GlyphKey, GridRenderer, RasterizedGlyph,
};

//  Bg + shared helpers

pub fn cell_fg(
    sq: Square,
    style_set: &StyleSet,
    renderer: &Renderer,
    term_colors: &TermColors,
) -> [u8; 4] {
    if sq.is_bg_only() {
        return normalized_to_u8(renderer.named_colors.foreground);
    }
    let style = style_set.get(sq.style_id());
    let color = renderer.compute_color(&style.fg, style.flags, term_colors);
    normalized_to_u8(color)
}

//  Decoration sprites (underlines, strikethrough)
//
// Ghostty pre-rasterizes underline/strikethrough sprites into the
// grayscale atlas and emits them as regular `CellText` entries
// (`ghostty/src/font/sprite/draw/special.zig`,
// `ghostty/src/renderer/generic.zig:3074`). We do the same: one sprite
// per (style, cell_w, thickness) cached in the grid atlas. Z-order is
// enforced by emit order — underlines before glyphs (draws under),
// strikethrough after (draws on top).

#[derive(Clone, Copy, Debug)]
#[repr(u32)]
enum DecorationStyle {
    Underline = 0,
    DoubleUnderline = 1,
    DottedUnderline = 2,
    DashedUnderline = 3,
    CurlyUnderline = 4,
    Strikethrough = 5,
}

/// Sentinel font_id base for decoration sprites. Real font_ids come
/// from sugarloaf's font library which packs into usize indices
/// starting at 0; 0xFFFF_FF00+ is far outside that range. Matches
/// Ghostty's `font.sprite_index` idea (`font/sprite.zig:17`).
const DECORATION_FONT_ID_BASE: u32 = 0xFFFF_FF00;

/// Underline thickness in physical pixels. Matches Ghostty's fallback
/// (15% of ex-height, min 1px) when the font doesn't expose
/// `underline_thickness` — we don't thread per-font metrics through to
/// decorations because runs can mix fonts inside a row. 0.075 * size_px
/// approximates 15% of ex-height at typical terminal fonts.
#[inline]
fn decoration_thickness(size_px: f32) -> u32 {
    (size_px * 0.075).round().max(1.0) as u32
}

/// Offset (in pixels, from cell bottom) at which the BOTTOM of an
/// underline sits. Small gap so underlines don't merge with the row
/// below. Mirrors the spirit of Ghostty's `underline_position` but
/// simplified — we don't have per-font metrics here.
#[inline]
fn underline_gap_below(cell_h: u32) -> u32 {
    (cell_h / 20).max(1)
}

fn rasterize_decoration(
    style: DecorationStyle,
    cell_w: u32,
    cell_h: u32,
    thickness: u32,
) -> (Vec<u8>, u32, u32, i16) {
    // Returns (pixels, width, height, bearing_y). `pixels` is R8
    // row-major, treated as alpha by the grayscale fragment branch.
    // `bearing_y` is cell-bottom → sprite-top distance (Rio's
    // grid-renderer convention).
    match style {
        DecorationStyle::Underline => {
            let bytes = vec![0xFFu8; (cell_w * thickness) as usize];
            let bearing_y = (thickness + underline_gap_below(cell_h)) as i16;
            (bytes, cell_w, thickness, bearing_y)
        }
        DecorationStyle::DoubleUnderline => {
            // Two strips with a `thickness` gap.
            let gap = thickness;
            let h = thickness * 2 + gap;
            let mut bytes = vec![0u8; (cell_w * h) as usize];
            let row_w = cell_w as usize;
            // Top strip: rows [0, thickness)
            for row in 0..thickness as usize {
                let start = row * row_w;
                bytes[start..start + row_w].fill(0xFF);
            }
            // Bottom strip: rows [thickness + gap, h)
            for row in (thickness + gap) as usize..h as usize {
                let start = row * row_w;
                bytes[start..start + row_w].fill(0xFF);
            }
            let bearing_y = (h + underline_gap_below(cell_h)) as i16;
            (bytes, cell_w, h, bearing_y)
        }
        DecorationStyle::DottedUnderline => {
            // Dots of diameter=thickness, period=2*thickness.
            let h = thickness;
            let diameter = thickness.max(1);
            let period = diameter * 2;
            let mut bytes = vec![0u8; (cell_w * h) as usize];
            let row_w = cell_w as usize;
            let mut x = 0u32;
            while x < cell_w {
                let end = (x + diameter).min(cell_w);
                for row in 0..h as usize {
                    let start = row * row_w + x as usize;
                    bytes[start..start + (end - x) as usize].fill(0xFF);
                }
                x += period;
            }
            let bearing_y = (h + underline_gap_below(cell_h)) as i16;
            (bytes, cell_w, h, bearing_y)
        }
        DecorationStyle::DashedUnderline => {
            // Two dashes per cell arranged as DASH-GAP-DASH-GAP on
            // quarter boundaries. Each cell ends on a GAP and starts
            // on a DASH, so adjacent cell sprites tile into one
            // continuous periodic pattern (dash | gap | dash | gap | ...)
            // across the row. For cell widths not divisible by 4,
            // segment widths differ by a single pixel inside the
            // cell but the cell-to-cell rhythm stays regular.
            //
            // Ghostty uses 3 segments per cell (`special.zig:135`)
            // which meets DASH-to-DASH at every cell boundary — we
            // prefer the 4-segment layout because it stays periodic
            // under tiling.
            let h = thickness;
            let b1 = cell_w / 4;
            let b2 = cell_w / 2;
            let b3 = (cell_w * 3) / 4;
            let mut bytes = vec![0u8; (cell_w * h) as usize];
            let row_w = cell_w as usize;
            for (x_lo, x_hi) in [(0u32, b1), (b2, b3)] {
                if x_hi <= x_lo {
                    continue;
                }
                for row in 0..h as usize {
                    let start = row * row_w + x_lo as usize;
                    let end = row * row_w + x_hi as usize;
                    bytes[start..end].fill(0xFF);
                }
            }
            let bearing_y = (h + underline_gap_below(cell_h)) as i16;
            (bytes, cell_w, h, bearing_y)
        }
        DecorationStyle::CurlyUnderline => {
            // One arch per cell: baseline → peak-at-center → baseline,
            // with horizontal tangents at cell edges so tiled sprites
            // join smoothly. Matches Ghostty's two-cubic-Bezier shape
            // (`ghostty/src/font/sprite/draw/special.zig:167`):
            //   amplitude = cell_w / π
            //   stroke width = thickness, round caps
            // We approximate the Bezier with a raised cosine, which
            // has the same endpoints, same peak, and the same
            // horizontal tangent at the edges. The two curves differ
            // by a fraction of a pixel at the shoulders — invisible
            // at terminal cell sizes.
            use core::f32::consts::PI;
            let amp = (cell_w as f32 / PI).max(thickness as f32);
            let amp_i = amp.ceil() as u32;
            let h = amp_i + thickness + 1;
            let mut bytes = vec![0u8; (cell_w * h) as usize];
            let row_w = cell_w as usize;
            let half_t = thickness as f32 * 0.5;
            // Baseline (bottom of arch) near sprite bottom; peak
            // (top of arch) near sprite top, both inset by half a
            // stroke + 0.5px so the stroke doesn't clip at edges.
            let baseline = h as f32 - half_t - 0.5;
            for col in 0..cell_w {
                let x_norm = (col as f32 + 0.5) / cell_w as f32;
                // Raised cosine: 0 at endpoints, 1 at midpoint. Zero
                // derivative at both endpoints = smooth tiling.
                let s = 0.5 * (1.0 - (x_norm * 2.0 * PI).cos());
                let y_center = baseline - s * amp;
                let y_lo = (y_center - half_t).floor().max(0.0) as u32;
                let y_hi = ((y_center + half_t).ceil() as u32).min(h);
                for row in y_lo..y_hi {
                    bytes[row as usize * row_w + col as usize] = 0xFF;
                }
            }
            let bearing_y = (h + underline_gap_below(cell_h)) as i16;
            (bytes, cell_w, h, bearing_y)
        }
        DecorationStyle::Strikethrough => {
            // Single strip through vertical middle of the cell.
            let bytes = vec![0xFFu8; (cell_w * thickness) as usize];
            // Top of strike sits at cell_h/2 + thickness/2 above cell
            // bottom (i.e., strike is centered at cell_h/2).
            let center_from_bottom = cell_h / 2;
            let bearing_y =
                center_from_bottom as i16 + (thickness as i16 + 1) / 2;
            (bytes, cell_w, thickness, bearing_y)
        }
    }
}

/// Look up or insert a decoration sprite into the grid atlas. Key is
/// (decoration font_id sentinel, cell_w as glyph_id, thickness as
/// size_bucket) — the same cache that backs regular glyphs, so
/// decorations ride the grid's glyph-eviction policy for free.
fn ensure_decoration_slot(
    grid: &mut GridRenderer,
    style: DecorationStyle,
    cell_w: u32,
    cell_h: u32,
    thickness: u32,
) -> Option<AtlasSlot> {
    let key = GlyphKey {
        font_id: DECORATION_FONT_ID_BASE + style as u32,
        glyph_id: cell_w,
        size_bucket: thickness as u16,
    };
    if let Some(slot) = grid.lookup_glyph(key) {
        return Some(slot);
    }
    let (bytes, w, h, bearing_y) = rasterize_decoration(style, cell_w, cell_h, thickness);
    grid.insert_glyph(
        key,
        RasterizedGlyph {
            width: w.min(u16::MAX as u32) as u16,
            height: h.min(u16::MAX as u32) as u16,
            bearing_x: 0,
            bearing_y,
            bytes: &bytes,
        },
    )
}

/// Pick the decoration enum value for a cell's `StyleFlags`, or `None`
/// if the cell has no underline. Bit-test order matches StyleFlags
/// ordering in `rio-backend/src/crosswords/style.rs`.
#[inline]
fn underline_style_from_flags(flags: StyleFlags) -> Option<DecorationStyle> {
    if flags.contains(StyleFlags::UNDERLINE) {
        Some(DecorationStyle::Underline)
    } else if flags.contains(StyleFlags::DOUBLE_UNDERLINE) {
        Some(DecorationStyle::DoubleUnderline)
    } else if flags.contains(StyleFlags::UNDERCURL) {
        Some(DecorationStyle::CurlyUnderline)
    } else if flags.contains(StyleFlags::DOTTED_UNDERLINE) {
        Some(DecorationStyle::DottedUnderline)
    } else if flags.contains(StyleFlags::DASHED_UNDERLINE) {
        Some(DecorationStyle::DashedUnderline)
    } else {
        None
    }
}

/// Decoration color: SGR 58 `underline_color` if set, else the cell's
/// computed fg. Matches Ghostty `generic.zig:2968`.
#[inline]
fn decoration_color(
    sq: Square,
    style: &rio_backend::crosswords::style::Style,
    style_set: &StyleSet,
    renderer: &Renderer,
    term_colors: &TermColors,
) -> [u8; 4] {
    if let Some(uc) = style.underline_color {
        normalized_to_u8(renderer.compute_color(&uc, style.flags, term_colors))
    } else {
        cell_fg(sq, style_set, renderer, term_colors)
    }
}

pub fn cell_bg(
    sq: Square,
    style_set: &StyleSet,
    renderer: &Renderer,
    term_colors: &TermColors,
) -> [u8; 4] {
    let color = match sq.content_tag() {
        ContentTag::BgRgb => {
            let (r, g, b) = sq.bg_rgb();
            return [r, g, b, 255];
        }
        ContentTag::BgPalette => {
            let idx = sq.bg_palette_index() as usize;
            renderer.color(idx, term_colors)
        }
        ContentTag::Codepoint => {
            let style = style_set.get(sq.style_id());
            renderer.compute_bg_color(&style, term_colors)
        }
    };
    normalized_to_u8(color)
}

#[inline]
fn normalized_to_u8(c: [f32; 4]) -> [u8; 4] {
    [
        (c[0].clamp(0.0, 1.0) * 255.0) as u8,
        (c[1].clamp(0.0, 1.0) * 255.0) as u8,
        (c[2].clamp(0.0, 1.0) * 255.0) as u8,
        (c[3].clamp(0.0, 1.0) * 255.0) as u8,
    ]
}

pub fn build_row_bg(
    row: &Row<Square>,
    cols: usize,
    style_set: &StyleSet,
    renderer: &Renderer,
    term_colors: &TermColors,
    bg_scratch: &mut Vec<CellBg>,
) {
    bg_scratch.clear();
    for x in 0..cols {
        let sq = row[Column(x)];
        let rgba = cell_bg(sq, style_set, renderer, term_colors);
        bg_scratch.push(CellBg { rgba });
    }
}

//  Run-shaping infrastructure (platform-agnostic types)

/// Bits of `StyleFlags` that change shaping / font selection. Bold +
/// italic pick different font files. Color / decoration / dim don't
/// affect shaping so they don't break runs.
const SHAPING_FLAG_MASK: u16 = StyleFlags::BOLD.bits() | StyleFlags::ITALIC.bits();

/// 256 × 8 bucketed LRU cache — matches Ghostty's CellCacheTable.
const RUN_BUCKET_COUNT: usize = 256;
const RUN_BUCKET_SIZE: usize = 8;

/// One shaped glyph. Same shape from both CoreText (macOS) and swash
/// (non-macOS). `cluster` is a UTF-8 byte offset into the run string.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // `x` / `y` / `advance` kept for future kerning-aware layout
struct ShapedGlyph {
    id: u16,
    x: f32,
    y: f32,
    advance: f32,
    cluster: u32,
}

struct RunCacheEntry {
    /// Stored so we can verify on lookup. FxHasher 64-bit collisions
    /// are astronomically rare but not zero; on mismatch we re-shape.
    hash: u64,
    run_str: String,
    glyphs: Vec<ShapedGlyph>,
}

pub struct GridGlyphRasterizer {
    font_resolve: FxHashMap<(char, u8), (u32, bool)>,
    ascent_cache: FxHashMap<(u32, u16), i16>,
    /// `(should_embolden, should_italicize)` per font_id. Read from
    /// `FontData` synthesis flags; matches the rich-text rasterizer's
    /// convention.
    synthesis_cache: FxHashMap<u32, (bool, bool)>,
    run_cache: Vec<Vec<RunCacheEntry>>,
    run_str_scratch: String,

    // macOS: cached CoreText handles per font_id.
    #[cfg(target_os = "macos")]
    handle_cache: FxHashMap<u32, rio_backend::sugarloaf::font::macos::FontHandle>,

    // non-macOS: swash contexts + cached font bytes per font_id.
    #[cfg(not(target_os = "macos"))]
    shape_ctx: rio_backend::sugarloaf::swash::shape::ShapeContext,
    #[cfg(not(target_os = "macos"))]
    scale_ctx: rio_backend::sugarloaf::swash::scale::ScaleContext,
    #[cfg(not(target_os = "macos"))]
    font_data_cache: FxHashMap<
        u32,
        (
            rio_backend::sugarloaf::font::SharedData,
            u32,
            rio_backend::sugarloaf::swash::CacheKey,
        ),
    >,
}

impl Default for GridGlyphRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

impl GridGlyphRasterizer {
    pub fn new() -> Self {
        Self {
            font_resolve: FxHashMap::default(),
            ascent_cache: FxHashMap::default(),
            synthesis_cache: FxHashMap::default(),
            run_cache: (0..RUN_BUCKET_COUNT)
                .map(|_| Vec::with_capacity(RUN_BUCKET_SIZE))
                .collect(),
            run_str_scratch: String::new(),
            #[cfg(target_os = "macos")]
            handle_cache: FxHashMap::default(),
            #[cfg(not(target_os = "macos"))]
            shape_ctx: rio_backend::sugarloaf::swash::shape::ShapeContext::new(),
            #[cfg(not(target_os = "macos"))]
            scale_ctx: rio_backend::sugarloaf::swash::scale::ScaleContext::new(),
            #[cfg(not(target_os = "macos"))]
            font_data_cache: FxHashMap::default(),
        }
    }

    #[inline]
    fn resolve_font(
        &mut self,
        ch: char,
        style_flags: u8,
        font_library: &FontLibrary,
    ) -> (u32, bool) {
        *self
            .font_resolve
            .entry((ch, style_flags))
            .or_insert_with(|| {
                let span_style = span_style_for_flags(style_flags);
                #[cfg(target_os = "macos")]
                let (id, emoji) = font_library.resolve_font_for_char(ch, &span_style);
                #[cfg(not(target_os = "macos"))]
                let (id, emoji) = {
                    let lib = font_library.inner.read();
                    lib.find_best_font_match(ch, &span_style).unwrap_or((0, false))
                }
                (id as u32, emoji)
            })
    }

    #[inline]
    fn get_synthesis(
        &mut self,
        font_id: u32,
        font_library: &FontLibrary,
    ) -> (bool, bool) {
        *self.synthesis_cache.entry(font_id).or_insert_with(|| {
            let lib = font_library.inner.read();
            let fd = lib.get(&(font_id as usize));
            (fd.should_embolden, fd.should_italicize)
        })
    }
}

#[inline]
fn span_style_for_flags(style_flags: u8) -> rio_backend::sugarloaf::SpanStyle {
    use rio_backend::sugarloaf::{Attributes, Stretch, Style as FontStyle, Weight};
    let mut s = rio_backend::sugarloaf::SpanStyle::default();
    let bold = (style_flags & StyleFlags::BOLD.bits() as u8) != 0;
    let italic = (style_flags & StyleFlags::ITALIC.bits() as u8) != 0;
    let weight = if bold { Weight::BOLD } else { Weight::NORMAL };
    let fstyle = if italic {
        FontStyle::Italic
    } else {
        FontStyle::Normal
    };
    s.font_attrs = Attributes::new(Stretch::NORMAL, weight, fstyle);
    s
}

#[inline]
fn run_hash(font_id: u32, size_bucket: u16, style_flags: u8, run: &str) -> u64 {
    use core::hash::Hasher;
    use rustc_hash::FxHasher;
    let mut h = FxHasher::default();
    h.write_u32(font_id);
    h.write_u16(size_bucket);
    h.write_u8(style_flags);
    h.write(run.as_bytes());
    h.finish()
}

#[inline]
fn is_run_breaker(sq: Square) -> bool {
    if sq.is_bg_only() {
        return true;
    }
    let ch = sq.c();
    ch == '\0' || ch == ' '
}

/// Lookup. Hash → bucket; scan from most-recent; rotate on hit.
fn run_cache_get<'a>(
    buckets: &'a mut [Vec<RunCacheEntry>],
    hash: u64,
    run_str: &str,
) -> Option<&'a [ShapedGlyph]> {
    let idx = (hash as usize) & (RUN_BUCKET_COUNT - 1);
    let bucket = &mut buckets[idx];
    let last = bucket.len().checked_sub(1)?;
    for i in (0..bucket.len()).rev() {
        if bucket[i].hash == hash && bucket[i].run_str == run_str {
            if i != last {
                bucket[i..=last].rotate_left(1);
            }
            return Some(&bucket[last].glyphs);
        }
    }
    None
}

/// Insert. Bucket full → evict oldest (front).
fn run_cache_put(buckets: &mut [Vec<RunCacheEntry>], entry: RunCacheEntry) {
    let idx = (entry.hash as usize) & (RUN_BUCKET_COUNT - 1);
    let bucket = &mut buckets[idx];
    if bucket.len() >= RUN_BUCKET_SIZE {
        bucket.remove(0);
    }
    bucket.push(entry);
}

//  Platform-specific shape + ascent helpers

/// Shape a single run on macOS via CoreText and populate
/// `out.ascent_px` as a side effect via the rasterizer's cache.
/// Returns the glyph list if the handle is available.
#[cfg(target_os = "macos")]
fn shape_run_ct(
    rasterizer: &mut GridGlyphRasterizer,
    font_id: u32,
    size_u16: u16,
    size_bucket: u16,
    font_library: &FontLibrary,
) -> Option<(Vec<ShapedGlyph>, i16)> {
    let handle = match rasterizer.handle_cache.entry(font_id) {
        std::collections::hash_map::Entry::Occupied(e) => e.into_mut().clone(),
        std::collections::hash_map::Entry::Vacant(e) => {
            let h = font_library.ct_font(font_id as usize)?;
            e.insert(h.clone());
            h
        }
    };
    let ascent_px = *rasterizer
        .ascent_cache
        .entry((font_id, size_bucket))
        .or_insert_with(|| {
            let m = rio_backend::sugarloaf::font::macos::font_metrics(
                &handle,
                size_u16 as f32,
            );
            m.ascent.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16
        });
    let ct_glyphs = rio_backend::sugarloaf::font::macos::shape_text(
        &handle,
        &rasterizer.run_str_scratch,
        size_u16 as f32,
    );
    let glyphs: Vec<ShapedGlyph> = ct_glyphs
        .iter()
        .map(|g| ShapedGlyph {
            id: g.id,
            x: g.x,
            y: g.y,
            advance: g.advance,
            cluster: g.cluster,
        })
        .collect();
    Some((glyphs, ascent_px))
}

/// Shape a single run on non-macOS via swash. Populates
/// `rasterizer.ascent_cache` + `rasterizer.font_data_cache` as a side
/// effect.
#[cfg(not(target_os = "macos"))]
fn shape_run_swash(
    rasterizer: &mut GridGlyphRasterizer,
    font_id: u32,
    size_u16: u16,
    size_bucket: u16,
    font_library: &FontLibrary,
) -> Option<(Vec<ShapedGlyph>, i16)> {
    use rio_backend::sugarloaf::swash::FontRef;

    let font_entry = rasterizer
        .font_data_cache
        .entry(font_id)
        .or_insert_with(|| {
            let lib = font_library.inner.read();
            lib.get_data(&(font_id as usize))
                .expect("font id resolved but get_data returned None")
        });
    let font_ref = FontRef {
        data: font_entry.0.as_ref(),
        offset: font_entry.1,
        key: font_entry.2,
    };

    let ascent_px = *rasterizer
        .ascent_cache
        .entry((font_id, size_bucket))
        .or_insert_with(|| {
            let m = font_ref.metrics(&[]).scale(size_u16 as f32);
            m.ascent.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16
        });

    let mut shaper = rasterizer
        .shape_ctx
        .builder(font_ref)
        .size(size_u16 as f32)
        .build();
    shaper.add_str(&rasterizer.run_str_scratch);
    let mut glyphs: Vec<ShapedGlyph> = Vec::new();
    shaper.shape_with(|cluster| {
        let byte_offset = cluster.source.start;
        for g in cluster.glyphs {
            glyphs.push(ShapedGlyph {
                id: g.id,
                x: g.x,
                y: g.y,
                advance: g.advance,
                cluster: byte_offset,
            });
        }
    });
    Some((glyphs, ascent_px))
}

//  Emission

/// Run-level fg emission. Shapes once per run, emits one CellText per
/// shaped glyph. Works on both macOS (CoreText) and non-macOS (swash).
///
/// Emits in three ordered phases so decoration z-order matches
/// Ghostty's: underlines first (drawn under glyphs), glyphs, then
/// strikethroughs (drawn on top).
#[allow(clippy::too_many_arguments)]
pub fn build_row_fg(
    row: &Row<Square>,
    cols: usize,
    y: u16,
    style_set: &StyleSet,
    renderer: &Renderer,
    term_colors: &TermColors,
    rasterizer: &mut GridGlyphRasterizer,
    grid: &mut GridRenderer,
    size_px: f32,
    cell_w: f32,
    cell_h: f32,
    font_library: &FontLibrary,
    fg_scratch: &mut Vec<CellText>,
) {
    fg_scratch.clear();

    let size_bucket = (size_px * 4.0).round().clamp(0.0, u16::MAX as f32) as u16;
    let size_u16 = size_px.round().clamp(1.0, u16::MAX as f32) as u16;

    let cell_w_u32 = cell_w.round().clamp(1.0, u32::MAX as f32) as u32;
    let cell_h_u32 = cell_h.round().clamp(1.0, u32::MAX as f32) as u32;
    let thickness = decoration_thickness(size_px);

    // Phase 1: underline pass. Emit before glyphs so grayscale quads
    // draw under the characters.
    emit_underlines(
        row,
        cols,
        y,
        style_set,
        renderer,
        term_colors,
        grid,
        cell_w_u32,
        cell_h_u32,
        thickness,
        fg_scratch,
    );

    let mut x: usize = 0;
    while x < cols {
        let sq = row[Column(x)];
        if is_run_breaker(sq) {
            x += 1;
            continue;
        }

        // Open a run at x.
        let ch = sq.c();
        let run_style_flags =
            (style_set.get(sq.style_id()).flags.bits() & SHAPING_FLAG_MASK) as u8;
        let (font_id, is_emoji) =
            rasterizer.resolve_font(ch, run_style_flags, font_library);
        let run_start = x;

        rasterizer.run_str_scratch.clear();
        rasterizer.run_str_scratch.push(ch);

        // Extend the run while (font_id, style_flags) match.
        let mut end = x + 1;
        while end < cols {
            let sq2 = row[Column(end)];
            if is_run_breaker(sq2) {
                break;
            }
            let ch2 = sq2.c();
            let style2_flags =
                (style_set.get(sq2.style_id()).flags.bits() & SHAPING_FLAG_MASK) as u8;
            if style2_flags != run_style_flags {
                break;
            }
            let (font_id2, _) = rasterizer.resolve_font(ch2, style2_flags, font_library);
            if font_id2 != font_id {
                break;
            }
            rasterizer.run_str_scratch.push(ch2);
            end += 1;
        }

        let hash = run_hash(
            font_id,
            size_bucket,
            run_style_flags,
            &rasterizer.run_str_scratch,
        );

        // Shape (cached) and capture ascent for this (font_id, size).
        let ascent_px = if run_cache_get(
            &mut rasterizer.run_cache,
            hash,
            &rasterizer.run_str_scratch,
        )
        .is_some()
        {
            // Cache hit — ascent already stored.
            rasterizer
                .ascent_cache
                .get(&(font_id, size_bucket))
                .copied()
                .unwrap_or(0)
        } else {
            #[cfg(target_os = "macos")]
            let shaped_opt =
                shape_run_ct(rasterizer, font_id, size_u16, size_bucket, font_library);
            #[cfg(not(target_os = "macos"))]
            let shaped_opt = shape_run_swash(
                rasterizer,
                font_id,
                size_u16,
                size_bucket,
                font_library,
            );
            let Some((glyphs, ascent_px)) = shaped_opt else {
                x = end;
                continue;
            };
            run_cache_put(
                &mut rasterizer.run_cache,
                RunCacheEntry {
                    hash,
                    run_str: rasterizer.run_str_scratch.clone(),
                    glyphs,
                },
            );
            ascent_px
        };

        let (synthetic_bold, synthetic_italic) =
            rasterizer.get_synthesis(font_id, font_library);

        // Collect (glyph_id, cell_offset) pairs by walking the shape
        // result alongside a monotonic cluster → cell-offset cursor.
        // Done up-front so we can release borrows on `rasterizer`
        // before the emit loop (which takes `&mut rasterizer` for the
        // rasterize + atlas-insert step).
        let glyph_emits: Vec<(u16, u16)> = {
            let glyphs = run_cache_get(
                &mut rasterizer.run_cache,
                hash,
                &rasterizer.run_str_scratch,
            )
            .expect("just inserted");
            let mut char_cursor =
                rasterizer.run_str_scratch.char_indices().peekable();
            let mut cell_idx_in_run: u16 = 0;
            let mut out = Vec::with_capacity(glyphs.len());
            for g in glyphs {
                while let Some(&(byte_offset, _)) = char_cursor.peek() {
                    if (byte_offset as u32) >= g.cluster {
                        break;
                    }
                    char_cursor.next();
                    cell_idx_in_run = cell_idx_in_run.saturating_add(1);
                }
                out.push((g.id, cell_idx_in_run));
            }
            out
        };

        for (glyph_id, cell_idx_in_run) in glyph_emits {
            let grid_col = (run_start as u16).saturating_add(cell_idx_in_run);
            if (grid_col as usize) >= cols {
                continue;
            }

            let Some((_, slot, is_color)) = ensure_glyph_by_id(
                rasterizer,
                grid,
                font_id,
                glyph_id,
                size_bucket,
                size_u16,
                cell_h,
                ascent_px,
                is_emoji,
                synthetic_italic,
                synthetic_bold,
            ) else {
                continue;
            };
            if slot.w == 0 || slot.h == 0 {
                continue;
            }

            // Pull fg from the cluster's first cell. Non-ligature runs
            // end up with one cluster per cell (per-cell colour);
            // ligatures take the first cluster cell's colour.
            let src_col =
                (run_start + cell_idx_in_run as usize).min(cols.saturating_sub(1));
            let src_sq = row[Column(src_col)];
            let (atlas, color) = if is_color {
                (CellText::ATLAS_COLOR, [255, 255, 255, 255])
            } else {
                (
                    CellText::ATLAS_GRAYSCALE,
                    cell_fg(src_sq, style_set, renderer, term_colors),
                )
            };

            fg_scratch.push(CellText {
                glyph_pos: [slot.x as u32, slot.y as u32],
                glyph_size: [slot.w as u32, slot.h as u32],
                bearings: [slot.bearing_x, slot.bearing_y],
                grid_pos: [grid_col, y],
                color,
                atlas,
                bools: 0,
                _pad: [0, 0],
            });
        }

        x = end;
    }

    // Phase 3: strikethrough pass. Emitted last so the strike overlays
    // the glyph.
    emit_strikethroughs(
        row,
        cols,
        y,
        style_set,
        renderer,
        term_colors,
        grid,
        cell_w_u32,
        cell_h_u32,
        thickness,
        fg_scratch,
    );
}

#[allow(clippy::too_many_arguments)]
fn emit_underlines(
    row: &Row<Square>,
    cols: usize,
    y: u16,
    style_set: &StyleSet,
    renderer: &Renderer,
    term_colors: &TermColors,
    grid: &mut GridRenderer,
    cell_w: u32,
    cell_h: u32,
    thickness: u32,
    fg_scratch: &mut Vec<CellText>,
) {
    for x in 0..cols {
        let sq = row[Column(x)];
        let style = style_set.get(sq.style_id());
        let Some(deco) = underline_style_from_flags(style.flags) else {
            continue;
        };
        let Some(slot) = ensure_decoration_slot(grid, deco, cell_w, cell_h, thickness)
        else {
            continue;
        };
        if slot.w == 0 || slot.h == 0 {
            continue;
        }
        let color = decoration_color(sq, &style, style_set, renderer, term_colors);
        fg_scratch.push(CellText {
            glyph_pos: [slot.x as u32, slot.y as u32],
            glyph_size: [slot.w as u32, slot.h as u32],
            bearings: [slot.bearing_x, slot.bearing_y],
            grid_pos: [x as u16, y],
            color,
            atlas: CellText::ATLAS_GRAYSCALE,
            bools: 0,
            _pad: [0, 0],
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_strikethroughs(
    row: &Row<Square>,
    cols: usize,
    y: u16,
    style_set: &StyleSet,
    renderer: &Renderer,
    term_colors: &TermColors,
    grid: &mut GridRenderer,
    cell_w: u32,
    cell_h: u32,
    thickness: u32,
    fg_scratch: &mut Vec<CellText>,
) {
    for x in 0..cols {
        let sq = row[Column(x)];
        let style = style_set.get(sq.style_id());
        if !style.flags.contains(StyleFlags::STRIKEOUT) {
            continue;
        }
        let Some(slot) = ensure_decoration_slot(
            grid,
            DecorationStyle::Strikethrough,
            cell_w,
            cell_h,
            thickness,
        ) else {
            continue;
        };
        if slot.w == 0 || slot.h == 0 {
            continue;
        }
        // Strikethrough always uses the cell fg (there's no SGR for
        // a separate strike color, matching Ghostty).
        let color = cell_fg(sq, style_set, renderer, term_colors);
        fg_scratch.push(CellText {
            glyph_pos: [slot.x as u32, slot.y as u32],
            glyph_size: [slot.w as u32, slot.h as u32],
            bearings: [slot.bearing_x, slot.bearing_y],
            grid_pos: [x as u16, y],
            color,
            atlas: CellText::ATLAS_GRAYSCALE,
            bools: 0,
            _pad: [0, 0],
        });
    }
}

/// Look up or rasterize-and-insert a glyph into the grid atlas by
/// `glyph_id`. Platform-agnostic entry point; cfg branches inside to
/// call the CT or swash rasterizer.
#[allow(clippy::too_many_arguments)]
fn ensure_glyph_by_id(
    rasterizer: &mut GridGlyphRasterizer,
    grid: &mut GridRenderer,
    font_id: u32,
    glyph_id: u16,
    size_bucket: u16,
    size_u16: u16,
    cell_h: f32,
    ascent_px: i16,
    is_emoji: bool,
    synthetic_italic: bool,
    synthetic_bold: bool,
) -> Option<(GlyphKey, AtlasSlot, bool)> {
    let key = GlyphKey {
        font_id,
        glyph_id: glyph_id as u32,
        size_bucket,
    };
    if let Some(slot) = grid.lookup_glyph(key) {
        return Some((key, slot, false));
    }
    if let Some(slot) = grid.lookup_glyph_color(key) {
        return Some((key, slot, true));
    }

    // Rasterize via the platform-native backend.
    let raw = rasterize_glyph_native(
        rasterizer,
        font_id,
        glyph_id,
        size_u16,
        is_emoji,
        synthetic_bold,
        synthetic_italic,
    )?;
    let is_color = raw.is_color;

    // Convert CG-convention `left`/`top` into grid-convention
    // `bearing_y` = `cell_h - ascent + top`. See the long comment in
    // the original macOS rasterizer for the geometry.
    let bearing_y = {
        let top_i16 = raw.top.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        let cell_h_i16 = cell_h.round().clamp(0.0, i16::MAX as f32) as i16;
        cell_h_i16.saturating_sub(ascent_px).saturating_add(top_i16)
    };
    let raster = RasterizedGlyph {
        width: raw.width.min(u16::MAX as u32) as u16,
        height: raw.height.min(u16::MAX as u32) as u16,
        bearing_x: raw.left.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
        bearing_y,
        bytes: &raw.bytes,
    };

    let slot = if is_color {
        grid.insert_glyph_color(key, raster)?
    } else {
        grid.insert_glyph(key, raster)?
    };
    Some((key, slot, is_color))
}

/// Platform-agnostic raw-glyph struct. Both backends populate this
/// shape and let the caller convert bearings to the grid's
/// cell-bottom-relative convention.
struct RawGlyph {
    width: u32,
    height: u32,
    left: i32,
    top: i32,
    is_color: bool,
    bytes: Vec<u8>,
}

#[cfg(target_os = "macos")]
fn rasterize_glyph_native(
    rasterizer: &mut GridGlyphRasterizer,
    font_id: u32,
    glyph_id: u16,
    size_u16: u16,
    is_emoji: bool,
    synthetic_bold: bool,
    synthetic_italic: bool,
) -> Option<RawGlyph> {
    let handle = rasterizer.handle_cache.get(&font_id)?.clone();
    let raw = rio_backend::sugarloaf::font::macos::rasterize_glyph(
        &handle,
        glyph_id,
        size_u16 as f32,
        is_emoji,
        synthetic_italic,
        synthetic_bold,
    )?;
    Some(RawGlyph {
        width: raw.width,
        height: raw.height,
        left: raw.left,
        top: raw.top,
        is_color: raw.is_color,
        bytes: raw.bytes,
    })
}

#[cfg(not(target_os = "macos"))]
fn rasterize_glyph_native(
    rasterizer: &mut GridGlyphRasterizer,
    font_id: u32,
    glyph_id: u16,
    size_u16: u16,
    _is_emoji: bool,
    synthetic_bold: bool,
    synthetic_italic: bool,
) -> Option<RawGlyph> {
    use rio_backend::sugarloaf::swash::{
        scale::{
            image::{Content, Image as GlyphImage},
            Render, Source, StrikeWith,
        },
        zeno::{Angle, Format, Transform},
        FontRef,
    };

    let font_entry = rasterizer.font_data_cache.get(&font_id)?.clone();
    let font_ref = FontRef {
        data: font_entry.0.as_ref(),
        offset: font_entry.1,
        key: font_entry.2,
    };

    let hinting = font_library_hinting(rasterizer);
    let mut scaler = rasterizer
        .scale_ctx
        .builder(font_ref)
        .hint(hinting)
        .size(size_u16 as f32)
        .build();

    let sources: &[Source] = &[
        Source::ColorOutline(0),
        Source::ColorBitmap(StrikeWith::BestFit),
        Source::Outline,
    ];
    let mut image = GlyphImage::new();
    let ok = Render::new(sources)
        .format(Format::Alpha)
        .embolden(if synthetic_bold { 0.5 } else { 0.0 })
        .transform(if synthetic_italic {
            Some(Transform::skew(
                Angle::from_degrees(14.0),
                Angle::from_degrees(0.0),
            ))
        } else {
            None
        })
        .render_into(&mut scaler, glyph_id, &mut image);
    if !ok {
        return None;
    }
    let is_color = image.content == Content::Color;
    Some(RawGlyph {
        width: image.placement.width,
        height: image.placement.height,
        left: image.placement.left,
        top: image.placement.top,
        is_color,
        bytes: image.data,
    })
}

/// Hinting is a library-wide setting. Read once per rasterize; the
/// RwLock read is cheap. (Caching it locally would require reset
/// plumbing on config reload.)
#[cfg(not(target_os = "macos"))]
#[inline]
fn font_library_hinting(_r: &GridGlyphRasterizer) -> bool {
    // TODO: thread through from a cache to avoid the lock per glyph.
    // For now the lock on swash rasterize is a small fraction of
    // render time; optimise if profiling flags it.
    true
}
