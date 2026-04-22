// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Helpers that translate terminal `Square` cells into the
//! `CellBg` / `CellText` format the grid GPU renderer expects.
//!
//! Phase 2.2 scope: bg-only resolution. Glyph rasterization + fg
//! emission lands in a follow-up — it requires a shaper +
//! rasterizer bridge that hasn't been written yet.

use rio_backend::config::colors::term::TermColors;
use rio_backend::crosswords::square::{ContentTag, Square};
use rio_backend::crosswords::style::{Style, StyleFlags, StyleSet};
use rustc_hash::FxHashMap;

use crate::renderer::Renderer;

use rio_backend::sugarloaf::font::FontLibrary;
use rio_backend::sugarloaf::grid::{
    AtlasSlot, CellText, GlyphKey, GridRenderer, RasterizedGlyph,
};

/// Resolve a single cell's foreground color. Mirrors `cell_bg` but
/// calls `compute_color` (which honors `dim`/`bold`) on the style's
/// fg. Bg-only cells have no fg so we return the palette default.
pub fn cell_fg(
    sq: Square,
    style_set: &StyleSet,
    renderer: &Renderer,
    term_colors: &TermColors,
) -> [u8; 4] {
    if sq.is_bg_only() {
        // No glyph to draw; color is irrelevant. Return default fg.
        return normalized_to_u8(renderer.named_colors.foreground);
    }
    let style = style_set.get(sq.style_id());
    let color = renderer.compute_color(&style.fg, style.flags, term_colors);
    normalized_to_u8(color)
}

/// Resolve a single cell's background color to premultiplied RGBA8,
/// suitable for direct `CellBg` upload.
///
/// Three code paths based on the cell's content tag:
///
/// - `BgRgb`    — RGB already packed in the cell, no style lookup.
/// - `BgPalette` — palette index packed in the cell; resolve via
///                 the renderer's color list + live `TermColors`.
/// - `Codepoint` (the common case) — dereference the cell's
///                 `style_id` into `StyleSet`, then let the renderer's
///                 existing `compute_bg_color` do the heavy lifting
///                 (handles `dim` / `bold` / light-bold overrides).
///
/// Alpha is always 255 — the grid draws cells opaquely. Inverse
/// handling (fg/bg swap) and selection/search tints are applied by
/// the caller *after* this returns; they're workflow concerns, not
/// per-cell style ones.
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

/// Per-panel rasterization bridge from codepoint → grid `AtlasSlot`.
///
/// Phase 2.2c scope: macOS only, grayscale only (no color emoji),
/// regular weight/style only (no bold/italic synthesis). The body is
/// a direct adaptation of sugarloaf's `GlyphCache::get_at_size`
/// (`sugarloaf/src/renderer/image_cache/glyph.rs:178`) — same
/// shaping + rasterization calls, different destination atlas.
///
/// Font resolution result is cached so hot chars skip
/// `FontLibrary::resolve_font_for_char` (which may do a CoreText
/// fallback walk on cache miss).
pub struct GridGlyphRasterizer {
    font_resolve: FxHashMap<char, (u32, bool)>,
    /// `(font_id, size_bucket)` → rounded ascent in px. Cached
    /// because bearings.y needs it per-glyph; CT queries involve a
    /// clone-with-size of the base font.
    ///
    /// Cell-bottom-relative `bearings.y` is derived as
    /// `cell_h - ascent + top`, so whatever extra space sits at the
    /// top of the cell (leading, `line_height > 1`) is absorbed
    /// automatically rather than leaking as a visible gap.
    #[cfg(target_os = "macos")]
    ascent_cache: FxHashMap<(u32, u16), i16>,
    /// `font_id` → `FontHandle`. Avoids a `FontLibrary::ct_font`
    /// read-lock acquisition + FontHandle clone on every cell —
    /// `spf`-style full-screen scrolls hit the hot path ~5k times
    /// per frame, so amortising this to one lookup per font per
    /// *session* moves ~250k reads/sec off the critical path.
    #[cfg(target_os = "macos")]
    handle_cache: FxHashMap<u32, rio_backend::sugarloaf::font::macos::FontHandle>,
    /// `(font_id, char, size_bucket)` → glyph_id. Avoids calling
    /// `shape_text` (CoreText `CTLineCreateWithAttributedString`)
    /// on every cell — that's the hot loop's dominant cost.
    #[cfg(target_os = "macos")]
    shape_cache: FxHashMap<(u32, char, u16), u16>,
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
            shape_cache: FxHashMap::default(),
        }
    }

    /// Ensure `ch` is rasterized at `size_px` in `grid`'s atlas and
    /// return its slot. Returns `None` for zero-width / missing
    /// glyphs — the caller should skip emitting a `CellText` in that
    /// case.
    /// Result of `ensure_glyph`: the atlas slot plus which atlas
    /// (grayscale mask vs color bitmap) the slot lives in. Callers
    /// (`build_cell_text`) tag the emitted `CellText.atlas` from this
    /// so the fragment shader picks the right texture.
    #[cfg(target_os = "macos")]
    pub fn ensure_glyph(
        &mut self,
        grid: &mut GridRenderer,
        ch: char,
        size_px: f32,
        cell_h: f32,
        flags: StyleFlags,
        font_library: &FontLibrary,
    ) -> Option<(GlyphKey, AtlasSlot, bool /* is_color */)> {
        // Phase 2.2c: skip synthesis for now — treat every lookup as
        // upright regular. The renderer already marks bold/italic in
        // `StyleFlags`; the final implementation should pass them
        // into `rasterize_glyph` as synthetic_bold / synthetic_italic
        // and into `resolve_font_for_char` via SpanStyle.font_attrs.
        let (font_id, is_emoji) = *self.font_resolve.entry(ch).or_insert_with(|| {
            let style = rio_backend::sugarloaf::SpanStyle::default();
            let (id, emoji) = font_library.resolve_font_for_char(ch, &style);
            (id as u32, emoji)
        });

        // NOTE: do not early-return on `is_emoji` — that flag is
        // per-font (e.g. "Apple Color Emoji") and the monochrome
        // outline path handles color fonts too. Ghostty defers the
        // decision to per-glyph `isColorGlyph()` at rasterize time
        // (`ghostty/src/renderer/generic.zig:3272`); we use macOS
        // rasterizer's per-glyph `is_color` below instead.
        let _ = is_emoji;

        // Cache the FontHandle per font_id. `ct_font` takes a
        // read-lock + clones the handle; doing that on every cell is
        // avoidable. Handles are CF-refcounted so cloning is cheap
        // AT CALL, but the lock acquisition isn't free across
        // thousands of cells per frame.
        let handle = match self.handle_cache.entry(font_id) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut().clone(),
            std::collections::hash_map::Entry::Vacant(e) => {
                let h = font_library.ct_font(font_id as usize)?;
                e.insert(h.clone());
                h
            }
        };

        // Quantize the size to 1/4-pixel buckets so minor scale drift
        // doesn't thrash the atlas.
        let size_bucket = (size_px * 4.0).round().clamp(0.0, u16::MAX as f32) as u16;
        // Pixel size for shaping/rasterization — integer avoids subpixel
        // cache explosions. Matches GlyphCache's `size as u16` quantization.
        let size_u16 = size_px.round().clamp(1.0, u16::MAX as f32) as u16;

        // Shape cache keyed by (font_id, char, size). First visit
        // runs a full CoreText shape; subsequent visits to the same
        // (font, char, size) are a single hashmap lookup. This is
        // the dominant perf win — `shape_text` is ~10–100µs per
        // call, and a 100×40 grid on a fullscreen scroll hits it
        // 4000× per frame without caching.
        let glyph_id = match self.shape_cache.entry((font_id, ch, size_bucket)) {
            std::collections::hash_map::Entry::Occupied(e) => *e.get(),
            std::collections::hash_map::Entry::Vacant(e) => {
                let mut buf = [0u8; 4];
                let s = ch.encode_utf8(&mut buf);
                let shaped = rio_backend::sugarloaf::font::macos::shape_text(
                    &handle,
                    s,
                    size_u16 as f32,
                );
                let id = shaped.first()?.id;
                *e.insert(id)
            }
        };

        let key = GlyphKey {
            font_id,
            glyph_id: glyph_id as u32,
            size_bucket,
        };
        // Check both atlases — a glyph might be in either depending
        // on its color status, which was decided at first-rasterize
        // time. `font_id` is part of the key so a color-emoji font
        // and a text font can't collide.
        if let Some(slot) = grid.lookup_glyph(key) {
            return Some((key, slot, false));
        }
        if let Some(slot) = grid.lookup_glyph_color(key) {
            return Some((key, slot, true));
        }

        // Cell-bottom-relative bearings conversion. macOS rasterizer
        // returns `top` baseline-relative (distance from baseline
        // up to bitmap top). The shader expects `bearings.y` =
        // distance from cell bottom to bitmap top. For a cell
        // laid out as `| leading | ascent | descent |` (top to
        // bottom), baseline sits at `cell_top + leading + ascent`
        // = `cell_bottom - descent`. Bitmap top = baseline - top.
        // Distance from cell bottom to bitmap top =
        //   cell_h - (bitmap_top - cell_top)
        //   = cell_h - (leading + ascent - top)
        //   = cell_h - ascent + top  (if we fold leading into cell_h,
        //                             which sugarloaf already does).
        //
        // Caching ascent (rather than descent+leading) makes this
        // formula robust to any extra space the cell picked up from
        // `line_height > 1.0` too — it automatically sits on top
        // of the glyph instead of pushing the glyph down.
        let ascent_px = *self
            .ascent_cache
            .entry((font_id, size_bucket))
            .or_insert_with(|| {
                let m = rio_backend::sugarloaf::font::macos::font_metrics(
                    &handle,
                    size_u16 as f32,
                );
                m.ascent.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16
            });

        // Phase 2.2c simplification: no synthetic bold/italic even if
        // the style flags set them. `sugarloaf::font::macos::rasterize_glyph`
        // handles that path; wire it in when bold/italic start to
        // matter visually.
        let _ = flags;
        // `is_emoji = true` enables CoreText's color-rasterization
        // path when the font (e.g. Apple Color Emoji) carries color
        // tables. Monochrome fonts ignore this and still render
        // grayscale. Per-glyph color status comes back as
        // `raw.is_color` — that's the authoritative signal for
        // which atlas to use.
        let raw = rio_backend::sugarloaf::font::macos::rasterize_glyph(
            &handle,
            glyph_id,
            size_u16 as f32,
            is_emoji,
            /* synthetic_italic: */ false,
            /* synthetic_bold: */ false,
        )?;

        let is_color = raw.is_color;

        let raster = RasterizedGlyph {
            width: raw.width.min(u16::MAX as u32) as u16,
            height: raw.height.min(u16::MAX as u32) as u16,
            // `left`: x of bitmap-left from pen. Shader expects
            // `bearings.x` (distance from cell-left to glyph-left),
            // which is the same when advances equal cell width.
            bearing_x: raw.left.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
            // `bearings.y = cell_h - ascent + top`. See the long
            // comment at `ascent_cache` for the derivation. The
            // cell_h-dependent part gets baked into the cached
            // slot's bearing_y, so if the panel's cell_h changes
            // (resize) the cached slot is still positionally wrong
            // until re-inserted — acceptable because resize clears
            // needs_full_rebuild which triggers a full rewrite.
            bearing_y: {
                let top_i16 =
                    raw.top.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
                let cell_h_i16 =
                    cell_h.round().clamp(0.0, i16::MAX as f32) as i16;
                cell_h_i16
                    .saturating_sub(ascent_px)
                    .saturating_add(top_i16)
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
}

/// Build a `CellText` instance for a cell. Returns `None` for
/// bg-only / space / zero-width cells where no glyph should be
/// drawn.
#[cfg(target_os = "macos")]
pub fn build_cell_text(
    sq: Square,
    col: u16,
    row: u16,
    style_set: &StyleSet,
    renderer: &Renderer,
    term_colors: &TermColors,
    rasterizer: &mut GridGlyphRasterizer,
    grid: &mut GridRenderer,
    size_px: f32,
    cell_h: f32,
    font_library: &FontLibrary,
) -> Option<CellText> {
    if sq.is_bg_only() {
        return None;
    }
    let ch = sq.c();
    if ch == '\0' || ch == ' ' {
        return None;
    }

    let style: Style = style_set.get(sq.style_id());
    let (_key, slot, is_color) = rasterizer.ensure_glyph(
        grid,
        ch,
        size_px,
        cell_h,
        style.flags,
        font_library,
    )?;
    if slot.w == 0 || slot.h == 0 {
        return None;
    }

    // For color glyphs (emoji), the fragment shader returns the
    // atlas sample directly and ignores `color`. For grayscale
    // glyphs, `color` is multiplied by the alpha mask — that's where
    // the SGR foreground lands.
    let (atlas, color) = if is_color {
        (CellText::ATLAS_COLOR, [255, 255, 255, 255])
    } else {
        (
            CellText::ATLAS_GRAYSCALE,
            cell_fg(sq, style_set, renderer, term_colors),
        )
    };
    Some(CellText {
        glyph_pos: [slot.x as u32, slot.y as u32],
        glyph_size: [slot.w as u32, slot.h as u32],
        bearings: [slot.bearing_x, slot.bearing_y],
        grid_pos: [col, row],
        color,
        atlas,
        bools: 0,
        _pad: [0, 0],
    })
}
