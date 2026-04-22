// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Translates terminal `Square` cells into `CellBg` / `CellText`
//! instances for the grid GPU renderer.
//!
//! The bg path is unconditional and platform-agnostic: one `CellBg`
//! per cell. The fg path is macOS-only today and does **run-level
//! shaping** via CoreText — a contiguous run of cells with the same
//! `(font_id, style_flags)` is shaped in one `shape_text` call, and
//! one `CellText` is emitted per resulting `ShapedGlyph` (not per
//! input cell). That's what lets ligatures like `=>` / `!=` / `fi`
//! collapse into a single oversized glyph spanning multiple cells —
//! CoreText can't form a ligature from a one-codepoint shape call.
//!
//! Mirrors Ghostty's `font::shaper::run::RunIterator` in
//! `ghostty/src/font/shaper/run.zig`. Not a copy — run-break rules
//! are a subset for now (see `build_row_fg` for the list).

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

/// Resolve a cell's foreground color. Bg-only cells have no fg so we
/// return the palette default.
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

/// Resolve a cell's background color to premultiplied RGBA8.
///
/// Three code paths based on the cell's content tag:
/// - `BgRgb` — RGB already packed in the cell.
/// - `BgPalette` — palette index packed in the cell.
/// - `Codepoint` — defer to `Renderer::compute_bg_color` (handles
///   dim/bold/light-bold overrides via the looked-up `Style`).
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

/// Populate `bg_scratch` with one `CellBg` per cell in `row`.
///
/// Platform-agnostic — the bg pass doesn't need shaping.
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

/// Bits of `StyleFlags` that change shaping and font-variant
/// selection. Bold/italic pick different font files (`BOLD` /
/// `ITALIC` bits). Color, decoration, dim etc. don't participate in
/// shaping so they don't need to break runs.
#[cfg(target_os = "macos")]
const SHAPING_FLAG_MASK: u16 = StyleFlags::BOLD.bits() | StyleFlags::ITALIC.bits();

/// Number of buckets in the run-shape cache. Power of two so that
/// `hash & (N - 1)` folds cleanly. Paired with `RUN_BUCKET_SIZE` to
/// cap total entries at `256 * 8 = 2048`, matching Ghostty's
/// `CellCacheTable` (`ghostty/src/font/shaper/Cache.zig:32`).
#[cfg(target_os = "macos")]
const RUN_BUCKET_COUNT: usize = 256;

/// Items per bucket before eviction. Scans are linear so keep this
/// small.
#[cfg(target_os = "macos")]
const RUN_BUCKET_SIZE: usize = 8;

#[cfg(target_os = "macos")]
struct RunCacheEntry {
    /// Stored for collision verification — FxHasher 64-bit collisions
    /// are astronomically rare, but on mismatch we just re-shape.
    hash: u64,
    run_str: String,
    glyphs: Vec<rio_backend::sugarloaf::font::macos::ShapedGlyph>,
}

/// Per-panel glyph rasterizer + caches.
///
/// - `font_resolve`: `(char, style_flags) → (font_id, is_emoji)`. Keyed
///   on style because bold/italic pick different fonts; a `char` can
///   resolve to different font_ids depending on weight/slant.
/// - `handle_cache`: `font_id → FontHandle`. Saves a read-lock + clone
///   on every shape call.
/// - `ascent_cache`: `(font_id, size_bucket) → ascent_px`. Feeds the
///   `bearing_y = cell_h - ascent + top` formula; computed per glyph.
/// - `run_cache`: 256 × 8 bucketed LRU of shaped runs. Keyed by
///   `run_hash` (font + size + style + text); position-independent
///   so identical text at different rows reuses the entry.
///   Mirrors `ghostty/src/font/shaper/Cache.zig`'s `CellCacheTable`.
/// - `run_str_scratch`: reusable buffer for building a run's string
///   without allocating each call.
pub struct GridGlyphRasterizer {
    font_resolve: FxHashMap<(char, u8), (u32, bool)>,
    #[cfg(target_os = "macos")]
    ascent_cache: FxHashMap<(u32, u16), i16>,
    #[cfg(target_os = "macos")]
    handle_cache: FxHashMap<u32, rio_backend::sugarloaf::font::macos::FontHandle>,
    /// Per-`font_id` `(should_embolden, should_italicize)`. Mirrors
    /// sugarloaf's rich-text path (`glyph.rs:208-209`): if the font
    /// library's best-match lookup landed on an entry whose load-time
    /// attrs differ from what was requested, those bools are `true`
    /// and the rasterizer synthesises via CGTextFillStroke / skew.
    /// If the library actually has a native bold/italic face, the
    /// entry has `should_embolden=false` / `should_italicize=false`
    /// so no double-synthesis.
    #[cfg(target_os = "macos")]
    synthesis_cache: FxHashMap<u32, (bool, bool)>,
    #[cfg(target_os = "macos")]
    run_cache: Vec<Vec<RunCacheEntry>>,
    #[cfg(target_os = "macos")]
    run_str_scratch: String,
}

impl GridGlyphRasterizer {
    pub fn new() -> Self {
        Self {
            font_resolve: FxHashMap::default(),
            #[cfg(target_os = "macos")]
            ascent_cache: FxHashMap::default(),
            #[cfg(target_os = "macos")]
            handle_cache: FxHashMap::default(),
            #[cfg(target_os = "macos")]
            synthesis_cache: FxHashMap::default(),
            #[cfg(target_os = "macos")]
            run_cache: (0..RUN_BUCKET_COUNT)
                .map(|_| Vec::with_capacity(RUN_BUCKET_SIZE))
                .collect(),
            #[cfg(target_os = "macos")]
            run_str_scratch: String::new(),
        }
    }

    /// Font resolution for `ch` under a given style. Result is cached
    /// per `(char, style_flags)` so hot chars skip the CoreText
    /// fallback walk.
    ///
    /// `style_flags` is the masked `StyleFlags` (`SHAPING_FLAG_MASK`),
    /// not the raw bits — color/decoration don't affect resolution.
    #[cfg(target_os = "macos")]
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
                let (id, emoji) = font_library.resolve_font_for_char(ch, &span_style);
                (id as u32, emoji)
            })
    }

    #[cfg(target_os = "macos")]
    fn get_handle(
        &mut self,
        font_id: u32,
        font_library: &FontLibrary,
    ) -> Option<rio_backend::sugarloaf::font::macos::FontHandle> {
        match self.handle_cache.entry(font_id) {
            std::collections::hash_map::Entry::Occupied(e) => Some(e.into_mut().clone()),
            std::collections::hash_map::Entry::Vacant(e) => {
                let h = font_library.ct_font(font_id as usize)?;
                e.insert(h.clone());
                Some(h)
            }
        }
    }

    /// `(should_embolden, should_italicize)` for `font_id`. Read once
    /// from `FontLibraryData` behind the `inner` RwLock, then cached.
    /// Same values the rich-text rasterize path reads — so if the
    /// library's best-match picked a native bold face these are
    /// `false` (no synthesis on top of it); if it fell back to the
    /// regular face under a bold request, `should_embolden` is
    /// `true` and we tell CoreGraphics to stroke-over-fill.
    #[cfg(target_os = "macos")]
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

    #[cfg(target_os = "macos")]
    #[inline]
    fn get_ascent(
        &mut self,
        font_id: u32,
        size_bucket: u16,
        handle: &rio_backend::sugarloaf::font::macos::FontHandle,
        size_u16: u16,
    ) -> i16 {
        *self
            .ascent_cache
            .entry((font_id, size_bucket))
            .or_insert_with(|| {
                let m = rio_backend::sugarloaf::font::macos::font_metrics(
                    handle,
                    size_u16 as f32,
                );
                m.ascent.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16
            })
    }
}

// Free functions (not methods) so callers can borrow
// `rasterizer.run_cache` mutably while holding a `&str` into
// `rasterizer.run_str_scratch` — disjoint-field borrow split doesn't
// work through `&mut self` method syntax.

/// Bucketed LRU lookup. Hash → bucket; scan from most-recent end; on
/// hit, rotate-left to promote the entry back to the end. `run_str`
/// comparison guards against hash collisions.
#[cfg(target_os = "macos")]
fn run_cache_get<'a>(
    buckets: &'a mut [Vec<RunCacheEntry>],
    hash: u64,
    run_str: &str,
) -> Option<&'a [rio_backend::sugarloaf::font::macos::ShapedGlyph]> {
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

/// Bucketed LRU insert. If the bucket is full, evict the oldest
/// (front) entry — same policy as Ghostty's `CacheTable.put`.
#[cfg(target_os = "macos")]
fn run_cache_put(buckets: &mut [Vec<RunCacheEntry>], entry: RunCacheEntry) {
    let idx = (entry.hash as usize) & (RUN_BUCKET_COUNT - 1);
    let bucket = &mut buckets[idx];
    if bucket.len() >= RUN_BUCKET_SIZE {
        // Remove the oldest (front). O(BUCKET_SIZE); bucket is small
        // so this is trivial vs. `VecDeque`.
        bucket.remove(0);
    }
    bucket.push(entry);
}

/// Build a `SpanStyle` with bold/italic toggled per `style_flags`.
/// Rio's font library honours `font_attrs` to pick the matching
/// bold/italic font variant (see `FontLibraryData::find_best_font_match`
/// in `sugarloaf/src/font/mod.rs:418`).
#[cfg(target_os = "macos")]
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

/// Hash `(font_id, size_bucket, style_flags, run_str)` → u64. Shared
/// by the cache insert + lookup paths so a run's hash is stable.
#[cfg(target_os = "macos")]
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

/// True if the cell contributes a glyph to the fg pass. Bg-only cells
/// and literal '\0' / ' ' break runs: there's nothing to shape, and
/// a space inside a run would still get no glyph emission so the run
/// might as well end.
#[cfg(target_os = "macos")]
#[inline]
fn is_run_breaker(sq: Square) -> bool {
    if sq.is_bg_only() {
        return true;
    }
    let ch = sq.c();
    ch == '\0' || ch == ' '
}

/// Emit run-level shaped glyphs for one row into `fg_scratch`.
///
/// Walks cells, accumulates runs of cells sharing
/// `(font_id, style_flags & SHAPING_FLAG_MASK)`, breaks at
/// run-breakers (bg-only / null / space). Each run is shaped once
/// (cached) via `shape_text`. One `CellText` is emitted per
/// `ShapedGlyph`, placed at the glyph's cluster cell — so a 2-cell
/// ligature contributes one wide-glyph `CellText` at the first cell,
/// and the second cell contributes no fg (the oversized quad paints
/// over it).
#[cfg(target_os = "macos")]
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
    cell_h: f32,
    font_library: &FontLibrary,
    fg_scratch: &mut Vec<CellText>,
) {
    fg_scratch.clear();

    let size_bucket = (size_px * 4.0).round().clamp(0.0, u16::MAX as f32) as u16;
    let size_u16 = size_px.round().clamp(1.0, u16::MAX as f32) as u16;

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

        // Extend the run.
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

        // Shape the run (cached). Structure gymnastics to avoid holding
        // a borrow across the cache insert.
        let Some(handle) = rasterizer.get_handle(font_id, font_library) else {
            x = end;
            continue;
        };
        let hash = run_hash(
            font_id,
            size_bucket,
            run_style_flags,
            &rasterizer.run_str_scratch,
        );
        let has_hit =
            run_cache_get(&mut rasterizer.run_cache, hash, &rasterizer.run_str_scratch)
                .is_some();
        if !has_hit {
            let shaped = rio_backend::sugarloaf::font::macos::shape_text(
                &handle,
                &rasterizer.run_str_scratch,
                size_u16 as f32,
            );
            run_cache_put(
                &mut rasterizer.run_cache,
                RunCacheEntry {
                    hash,
                    run_str: rasterizer.run_str_scratch.clone(),
                    glyphs: shaped,
                },
            );
        }

        let ascent_px = rasterizer.get_ascent(font_id, size_bucket, &handle, size_u16);
        let (synthetic_bold, synthetic_italic) =
            rasterizer.get_synthesis(font_id, font_library);

        // Copy shaped glyph metadata out of the cache so we can release
        // the cache borrow before rasterizing (which mutates `grid`).
        //
        // Each glyph is 6 bytes (id: u16, cluster: u32); copy is cheap.
        let glyphs: Vec<(u16, u32)> =
            run_cache_get(&mut rasterizer.run_cache, hash, &rasterizer.run_str_scratch)
                .expect("just inserted")
                .iter()
                .map(|g| (g.id, g.cluster))
                .collect();

        // Monotonic cursor over `run_str_scratch`: CT returns shaped
        // glyphs in cluster-ascending order (LTR), so the cell-offset
        // for cluster i is monotonically non-decreasing. Tracking a
        // cursor avoids the O(M·K) rescan that `char_indices` on every
        // glyph would do.
        let mut char_cursor = rasterizer.run_str_scratch.char_indices().peekable();
        let mut cell_idx_in_run: u16 = 0;

        for (glyph_id, cluster) in glyphs {
            // Advance cursor to the char at `cluster`.
            while let Some(&(byte_offset, _)) = char_cursor.peek() {
                if (byte_offset as u32) >= cluster {
                    break;
                }
                char_cursor.next();
                cell_idx_in_run = cell_idx_in_run.saturating_add(1);
            }
            let grid_col = (run_start as u16).saturating_add(cell_idx_in_run);
            if (grid_col as usize) >= cols {
                continue;
            }

            let Some((_, slot, is_color)) = ensure_glyph_by_id(
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
                &handle,
            ) else {
                continue;
            };
            if slot.w == 0 || slot.h == 0 {
                continue;
            }

            // Color emoji: shader samples atlas_color directly, ignores
            // `color`. Grayscale: `color` multiplies the alpha mask —
            // the SGR fg lands there. Pull fg from the cell at
            // `run_start + cell_idx_in_run` so that non-ligature runs
            // get per-cell colors correctly; ligatures take the first
            // cluster cell's fg (cosmetic when a ligature straddles a
            // fg-color change — Ghostty breaks runs at selection bound
            // for the same reason).
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
}

/// Look up (or rasterize + insert) a glyph in the grid atlas by
/// `glyph_id`, skipping the shape step.
///
/// `synthetic_italic` / `synthetic_bold` come from the resolved
/// `FontData`'s load-time `should_italicize` / `should_embolden`
/// flags — i.e. they're `true` only when the font library's
/// best-match lookup landed on an entry that *doesn't natively*
/// have the requested attribute. A native bold face returns
/// `should_embolden=false`, so no stroke-over-fill gets layered on
/// top of an already-bold outline (the "huge bold" regression).
/// Matches sugarloaf's rich-text path at
/// `sugarloaf/src/renderer/image_cache/glyph.rs:208`.
#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn ensure_glyph_by_id(
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
    handle: &rio_backend::sugarloaf::font::macos::FontHandle,
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

    let raw = rio_backend::sugarloaf::font::macos::rasterize_glyph(
        handle,
        glyph_id,
        size_u16 as f32,
        is_emoji,
        synthetic_italic,
        synthetic_bold,
    )?;
    let is_color = raw.is_color;

    let raster = RasterizedGlyph {
        width: raw.width.min(u16::MAX as u32) as u16,
        height: raw.height.min(u16::MAX as u32) as u16,
        bearing_x: raw.left.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
        bearing_y: {
            let top_i16 = raw.top.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            let cell_h_i16 = cell_h.round().clamp(0.0, i16::MAX as f32) as i16;
            cell_h_i16.saturating_sub(ascent_px).saturating_add(top_i16)
        },
        bytes: &raw.bytes,
    };

    let slot = if is_color {
        grid.insert_glyph_color(key, raster)?
    } else {
        grid.insert_glyph(key, raster)?
    };
    Some((key, slot, is_color))
}
