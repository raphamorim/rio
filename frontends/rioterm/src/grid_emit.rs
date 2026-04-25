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
use rio_backend::crosswords::pos::{Column, Line, Pos};
use rio_backend::crosswords::search::Match;
use rio_backend::crosswords::square::{ContentTag, Square};
use rio_backend::crosswords::style::{StyleFlags, StyleSet};
use rio_backend::selection::SelectionRange;
use rustc_hash::FxHashMap;

use crate::renderer::Renderer;

/// Per-row selection interval, in column indices. `None` = row is
/// outside the selection. Block selections reduce to the same
/// `[lo, hi]` on every row; linear selections expand middle rows to
/// the full width.
#[derive(Clone, Copy)]
pub struct RowSelection {
    pub lo: u16,
    pub hi: u16,
}

/// Compute the selection interval (if any) for visible row `y`.
/// `display_offset` translates visible-row index → absolute `Line`.
pub fn row_selection_for(
    sel: Option<SelectionRange>,
    y: usize,
    cols: usize,
    display_offset: i32,
) -> Option<RowSelection> {
    let sel = sel?;
    if cols == 0 {
        return None;
    }
    let line = Line((y as i32) - display_offset);
    if line < sel.start.row || line > sel.end.row {
        return None;
    }
    let cols_max = cols.saturating_sub(1);
    // Block selections: every row inside the band uses the same span.
    if sel.is_block {
        let lo = sel.start.col.0.min(cols_max);
        let hi = sel.end.col.0.min(cols_max);
        return Some(RowSelection {
            lo: lo as u16,
            hi: hi as u16,
        });
    }
    let lo = if line == sel.start.row {
        sel.start.col.0
    } else {
        0
    };
    let hi = if line == sel.end.row {
        sel.end.col.0
    } else {
        cols_max
    };
    Some(RowSelection {
        lo: lo.min(cols_max) as u16,
        hi: hi.min(cols_max) as u16,
    })
}

#[inline]
fn cell_in_row_sel(row_sel: Option<RowSelection>, col: u16) -> bool {
    match row_sel {
        Some(s) => col >= s.lo && col <= s.hi,
        None => false,
    }
}

/// Search-hint category at a cell. Matches Ghostty's `HighlightTag`
/// (`ghostty/src/renderer/generic.zig:240`) — we use the same two-way
/// split so `search_focused_match_background` can override the regular
/// match color on the currently-focused hit.
///
/// `HyperlinkHover` is rio-specific: same row-interval shape but the
/// only visual is a forced underline (no bg/fg color change), used for
/// the OSC 8 / regex-hint-on-hover affordance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HintTag {
    Match,
    Focused,
    HyperlinkHover,
}

/// Per-row hint interval, closed on both ends. Several `RowHint`s may
/// exist on one row (when the row contains multiple matches).
#[derive(Clone, Copy, Debug)]
pub struct RowHint {
    pub lo: u16,
    pub hi: u16,
    pub tag: HintTag,
}

/// Compute the hint-match intervals (if any) for visible row `y`.
/// Linear-selection semantics: a match can span multiple rows; first
/// / last rows clip to the match's column bounds; interior rows cover
/// the full width. Mirrors `row_selection_for`.
///
/// `focused_match` is pushed first so it wins `cell_in_row_hints`
/// iteration order when it overlaps another match — same precedence
/// as Ghostty (`generic.zig:1330-1353`: "The order below matters.
/// Highlights added earlier will take priority").
pub fn row_hints_for(
    hint_matches: Option<&[Match]>,
    focused_match: Option<&Match>,
    hover_hyperlink: Option<(Pos, Pos)>,
    y: usize,
    cols: usize,
    display_offset: i32,
    out: &mut Vec<RowHint>,
) {
    out.clear();
    if cols == 0 {
        return;
    }
    let line = Line((y as i32) - display_offset);
    let cols_max = cols.saturating_sub(1) as u16;

    let pos_pair_to_row_hint = |start: Pos, end: Pos, tag: HintTag| -> Option<RowHint> {
        if line < start.row || line > end.row {
            return None;
        }
        let lo = if line == start.row {
            start.col.0 as u16
        } else {
            0
        };
        let hi = if line == end.row {
            end.col.0 as u16
        } else {
            cols_max
        };
        Some(RowHint {
            lo: lo.min(cols_max),
            hi: hi.min(cols_max),
            tag,
        })
    };

    let to_row_hint =
        |m: &Match, tag: HintTag| pos_pair_to_row_hint(*m.start(), *m.end(), tag);

    let is_same_match = |a: &Match, b: &Match| -> bool {
        let (a_start, a_end) = (*a.start(), *a.end());
        let (b_start, b_end) = (*b.start(), *b.end());
        pos_eq(a_start, b_start) && pos_eq(a_end, b_end)
    };

    // Hyperlink hover sits in front of search matches in the priority
    // order — a hovered cell shows the underline regardless of whether
    // it also overlaps a search hit. The bg / fg paths skip this tag
    // (see `build_row_bg` / `cell_fg_hinted`) so the search-tag visual
    // still wins for color, but the underline is always emitted.
    if let Some((start, end)) = hover_hyperlink {
        if let Some(rh) = pos_pair_to_row_hint(start, end, HintTag::HyperlinkHover) {
            out.push(rh);
        }
    }

    let Some(matches) = hint_matches else {
        return;
    };

    if let Some(fm) = focused_match {
        if let Some(rh) = to_row_hint(fm, HintTag::Focused) {
            out.push(rh);
        }
    }
    for m in matches {
        if let Some(fm) = focused_match {
            if is_same_match(m, fm) {
                continue;
            }
        }
        if let Some(rh) = to_row_hint(m, HintTag::Match) {
            out.push(rh);
        }
    }
}

#[inline]
fn pos_eq(a: Pos, b: Pos) -> bool {
    a.row == b.row && a.col == b.col
}

#[inline]
fn cell_in_row_hints(row_hints: &[RowHint], col: u16) -> Option<HintTag> {
    // Skip HyperlinkHover for the color paths — it only contributes
    // an underline (handled separately in `emit_underlines`).
    for rh in row_hints {
        if rh.tag == HintTag::HyperlinkHover {
            continue;
        }
        if col >= rh.lo && col <= rh.hi {
            return Some(rh.tag);
        }
    }
    None
}

/// Whether a cell should receive a forced underline from a hovered
/// hyperlink / hint, regardless of its own SGR style flags.
#[inline]
fn cell_in_hover_underline(row_hints: &[RowHint], col: u16) -> bool {
    row_hints.iter().any(|rh| {
        rh.tag == HintTag::HyperlinkHover && col >= rh.lo && col <= rh.hi
    })
}

/// Foreground for a hint-matched cell. Mirrors `cell_fg_selected` but
/// uses the configured `search_match_foreground` /
/// `search_focused_match_foreground` from
/// `colors::Colors` (`rio-backend/src/config/colors/mod.rs:287,299`).
#[inline]
fn cell_fg_hinted(tag: HintTag, renderer: &Renderer) -> [u8; 4] {
    match tag {
        HintTag::Focused => {
            normalized_to_u8(renderer.named_colors.search_focused_match_foreground)
        }
        HintTag::Match => normalized_to_u8(renderer.named_colors.search_match_foreground),
        // Hover doesn't change fg color; defensive — `cell_in_row_hints`
        // already filters this tag out, so this arm shouldn't fire.
        HintTag::HyperlinkHover => [0, 0, 0, 0],
    }
}

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

/// Foreground for a selected cell. Mirrors Ghostty's selection-fg
/// rule (`generic.zig:2867`): use the configured `selection-foreground`
/// unless the user asked to keep the cell's own fg (Rio's
/// `ignore-selection-foreground-color`). Ghostty falls back to
/// `state.colors.background` when no color is configured; Rio always
/// has a default selection_foreground populated in its theme, so we
/// use it directly.
#[inline]
pub fn cell_fg_selected(
    sq: Square,
    style_set: &StyleSet,
    renderer: &Renderer,
    term_colors: &TermColors,
) -> [u8; 4] {
    if renderer.ignore_selection_fg_color {
        cell_fg(sq, style_set, renderer, term_colors)
    } else {
        normalized_to_u8(renderer.named_colors.selection_foreground)
    }
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
            let bearing_y = center_from_bottom as i16 + (thickness as i16 + 1) / 2;
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
    row_sel: Option<RowSelection>,
    row_hints: &[RowHint],
    bg_scratch: &mut Vec<CellBg>,
) {
    bg_scratch.clear();

    // Fast path: row has no selection and no color-changing hints
    // (HyperlinkHover only contributes an underline, never bg). The
    // overwhelming majority of rows in idle terminals hit this path —
    // strip the per-cell `cell_in_row_sel` / `cell_in_row_hints`
    // checks and just walk cells.
    let has_sel = row_sel.is_some();
    let has_color_hints = row_hints
        .iter()
        .any(|rh| rh.tag != HintTag::HyperlinkHover);
    if !has_sel && !has_color_hints {
        bg_scratch.reserve(cols);
        for x in 0..cols {
            let sq = row[Column(x)];
            bg_scratch.push(CellBg {
                rgba: cell_bg(sq, style_set, renderer, term_colors),
            });
        }
        return;
    }

    // Slow path: selection and/or hint highlighting present.
    let sel_bg = if has_sel {
        Some(normalized_to_u8(renderer.named_colors.selection_background))
    } else {
        None
    };
    let (match_bg, focused_bg) = if has_color_hints {
        (
            Some(normalized_to_u8(
                renderer.named_colors.search_match_background,
            )),
            Some(normalized_to_u8(
                renderer.named_colors.search_focused_match_background,
            )),
        )
    } else {
        (None, None)
    };
    for x in 0..cols {
        let sq = row[Column(x)];
        let col = x as u16;
        let rgba = if cell_in_row_sel(row_sel, col) {
            // Selection bg wins over hint bg and the cell's own bg,
            // matching Ghostty `generic.zig:2775-2800` (selection check
            // runs before highlight check).
            sel_bg.unwrap_or_else(|| cell_bg(sq, style_set, renderer, term_colors))
        } else if let Some(tag) = cell_in_row_hints(row_hints, col) {
            match tag {
                HintTag::Focused => focused_bg
                    .unwrap_or_else(|| cell_bg(sq, style_set, renderer, term_colors)),
                HintTag::Match => match_bg
                    .unwrap_or_else(|| cell_bg(sq, style_set, renderer, term_colors)),
                // `cell_in_row_hints` filters HyperlinkHover out, but
                // make the match exhaustive so a future caller can't
                // accidentally hit a panic.
                HintTag::HyperlinkHover => cell_bg(sq, style_set, renderer, term_colors),
            }
        } else {
            cell_bg(sq, style_set, renderer, term_colors)
        };
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
    /// 64-bit rapidhash of (font_id, size_bucket, style_flags, run bytes).
    /// We key on the hash alone — no stored run string, no equality
    /// check on lookup. Matches Ghostty's `CellCacheTable` pattern
    /// (`font/shaper/Cache.zig:20-30`): rapidhash / wyhash pass
    /// SMHasher, so a random collision costs a wrong-glyph frame
    /// until the next row rebuild but never corrupts state. Birthday
    /// bound at N=10k concurrent cache entries ≈ 2.7×10⁻¹².
    hash: u64,
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

    // macOS: stage the run in UTF-16 (what CoreText wants natively)
    // so the shaper call can hand the buffer straight to
    // `CFStringCreateWithCharactersNoCopy` with no encoding
    // conversion. Matches Ghostty's `coretext.zig:88-104` — UTF-16
    // `unichars` + a parallel cell-start table for the cluster →
    // cell mapping.
    #[cfg(target_os = "macos")]
    run_utf16_scratch: Vec<u16>,
    /// On macOS, `run_cell_starts[i]` is the offset (in UTF-16 code
    /// units) where cell `i` of the run begins inside
    /// `run_utf16_scratch`. Length = cells in the run. Used to walk
    /// shaped glyphs back to the cell they belong to.
    #[cfg(target_os = "macos")]
    run_cell_starts: Vec<u32>,
    /// Cached CoreText handles per font_id.
    #[cfg(target_os = "macos")]
    handle_cache: FxHashMap<u32, rio_backend::sugarloaf::font::macos::FontHandle>,

    // non-macOS: swash wants UTF-8, so keep a `String` scratch.
    #[cfg(not(target_os = "macos"))]
    run_str_scratch: String,
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
            #[cfg(target_os = "macos")]
            run_utf16_scratch: Vec::new(),
            #[cfg(target_os = "macos")]
            run_cell_starts: Vec::new(),
            #[cfg(not(target_os = "macos"))]
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
        // ASCII printable + regular style → always primary font, never
        // emoji. Skips the FxHashMap lookup that dominates this fn's
        // cost on terminal-typical content. Mirrors Ghostty's
        // `font/Group.zig` indexForCodepoint ASCII fast path.
        //
        // Bold / italic ASCII still goes through the cache because
        // the bold and italic font IDs are dynamic (depend on which
        // faces the user loaded), and non-ASCII can hit fallback.
        if style_flags == 0 && (' '..='~').contains(&ch) {
            return (
                rio_backend::sugarloaf::font::FONT_ID_REGULAR as u32,
                false,
            );
        }

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
                    lib.find_best_font_match(ch, &span_style)
                        .unwrap_or((0, false))
                };
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

/// Rapidhash-based run key. Rapidhash is the official successor to
/// wyhash (Ghostty's choice at `font/shaper/run.zig:8`) — same
/// quality, passes SMHasher, near-ideal collision probability. We use
/// the streaming `Hasher` API so we don't have to glue the inputs
/// into a single byte slice.
#[inline]
fn run_hash(font_id: u32, size_bucket: u16, style_flags: u8, run_bytes: &[u8]) -> u64 {
    use core::hash::Hasher;
    // `fast` flavour = the standard rapidhash algorithm tuned for
    // throughput. Quality is still SMHasher-passing (near-ideal
    // collision rate). `quality` is overkill for in-memory cache
    // keys where we don't need DoS resistance.
    let mut h = rapidhash::fast::RapidHasher::default();
    h.write_u32(font_id);
    h.write_u16(size_bucket);
    h.write_u8(style_flags);
    h.write(run_bytes);
    h.finish()
}

// Force inline — called once per cell during run extension on the hot
// path; body is two field reads + two compares so a real call is pure
// overhead.
#[inline(always)]
fn is_run_breaker(sq: Square) -> bool {
    if sq.is_bg_only() {
        return true;
    }
    let ch = sq.c();
    ch == '\0' || ch == ' '
}

/// Lookup. Hash → bucket; scan from most-recent; rotate on hit. No
/// secondary comparison — we trust the 64-bit rapidhash to be
/// collision-free across realistic workloads. Matches Ghostty
/// (`font/shaper/Cache.zig:27`).
fn run_cache_get(
    buckets: &mut [Vec<RunCacheEntry>],
    hash: u64,
) -> Option<&[ShapedGlyph]> {
    let idx = (hash as usize) & (RUN_BUCKET_COUNT - 1);
    let bucket = &mut buckets[idx];
    let last = bucket.len().checked_sub(1)?;
    for i in (0..bucket.len()).rev() {
        if bucket[i].hash == hash {
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
    let ct_glyphs = rio_backend::sugarloaf::font::macos::shape_text_utf16(
        &handle,
        &rasterizer.run_utf16_scratch,
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
    row_sel: Option<RowSelection>,
    row_hints: &[RowHint],
    font_library: &FontLibrary,
    fg_scratch: &mut Vec<CellText>,
) {
    fg_scratch.clear();

    let size_bucket = (size_px * 4.0).round().clamp(0.0, u16::MAX as f32) as u16;
    let size_u16 = size_px.round().clamp(1.0, u16::MAX as f32) as u16;

    let cell_w_u32 = cell_w.round().clamp(1.0, u32::MAX as f32) as u32;
    let cell_h_u32 = cell_h.round().clamp(1.0, u32::MAX as f32) as u32;
    let thickness = decoration_thickness(size_px);

    // Row-level state hoisted out of the per-glyph emit loop. Same
    // optimisation as `build_row_bg`'s fast path — avoids the
    // `cell_in_row_sel` + `cell_in_row_hints` calls per glyph when
    // the row has no selection / no color-changing hints.
    let has_sel = row_sel.is_some();
    let has_color_hints = row_hints
        .iter()
        .any(|rh| rh.tag != HintTag::HyperlinkHover);
    let needs_per_cell_check = has_sel || has_color_hints;

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
        row_sel,
        row_hints,
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

        #[cfg(target_os = "macos")]
        {
            rasterizer.run_utf16_scratch.clear();
            rasterizer.run_cell_starts.clear();
            rasterizer
                .run_cell_starts
                .push(rasterizer.run_utf16_scratch.len() as u32);
            let mut buf = [0u16; 2];
            rasterizer
                .run_utf16_scratch
                .extend_from_slice(ch.encode_utf16(&mut buf));
        }
        #[cfg(not(target_os = "macos"))]
        {
            rasterizer.run_str_scratch.clear();
            rasterizer.run_str_scratch.push(ch);
        }

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
            #[cfg(target_os = "macos")]
            {
                rasterizer
                    .run_cell_starts
                    .push(rasterizer.run_utf16_scratch.len() as u32);
                let mut buf = [0u16; 2];
                rasterizer
                    .run_utf16_scratch
                    .extend_from_slice(ch2.encode_utf16(&mut buf));
            }
            #[cfg(not(target_os = "macos"))]
            {
                rasterizer.run_str_scratch.push(ch2);
            }
            end += 1;
        }

        #[cfg(target_os = "macos")]
        let run_bytes: &[u8] = {
            // Reinterpret the u16 scratch as bytes for the hasher —
            // same alignment rule as `slice::align_to`, but we know
            // u16 → u8 is always well-aligned so this is a trivial
            // cast. Only the byte pattern matters for the hash.
            let s = &rasterizer.run_utf16_scratch;
            // Safety: `u16` has stricter alignment than `u8`; the
            // resulting byte slice aliases `s` read-only for the
            // lifetime of this borrow.
            unsafe {
                core::slice::from_raw_parts(
                    s.as_ptr() as *const u8,
                    s.len() * core::mem::size_of::<u16>(),
                )
            }
        };
        #[cfg(not(target_os = "macos"))]
        let run_bytes: &[u8] = rasterizer.run_str_scratch.as_bytes();
        let hash = run_hash(font_id, size_bucket, run_style_flags, run_bytes);

        // Shape (cached) and capture ascent for this (font_id, size).
        let ascent_px = if run_cache_get(&mut rasterizer.run_cache, hash).is_some() {
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
            let shaped_opt =
                shape_run_swash(rasterizer, font_id, size_u16, size_bucket, font_library);
            let Some((glyphs, ascent_px)) = shaped_opt else {
                x = end;
                continue;
            };
            run_cache_put(&mut rasterizer.run_cache, RunCacheEntry { hash, glyphs });
            ascent_px
        };

        let (synthetic_bold, synthetic_italic) =
            rasterizer.get_synthesis(font_id, font_library);

        // Collect (glyph_id, cell_offset) pairs by walking the shape
        // result alongside a monotonic cluster → cell-offset cursor.
        // Done up-front so we can release borrows on `rasterizer`
        // before the emit loop (which takes `&mut rasterizer` for the
        // rasterize + atlas-insert step).
        //
        // Cluster space differs by platform: macOS CoreText reports
        // UTF-16 code-unit offsets, swash reports UTF-8 byte offsets.
        // Each backend walks its own cell-position table.
        let glyph_emits: Vec<(u16, u16)> = {
            let glyphs =
                run_cache_get(&mut rasterizer.run_cache, hash).expect("just inserted");
            let mut cell_idx_in_run: u16 = 0;
            let mut out = Vec::with_capacity(glyphs.len());
            #[cfg(target_os = "macos")]
            {
                let cell_starts = &rasterizer.run_cell_starts;
                for g in glyphs {
                    while (cell_idx_in_run as usize + 1) < cell_starts.len()
                        && cell_starts[cell_idx_in_run as usize + 1] <= g.cluster
                    {
                        cell_idx_in_run = cell_idx_in_run.saturating_add(1);
                    }
                    out.push((g.id, cell_idx_in_run));
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                let mut char_cursor =
                    rasterizer.run_str_scratch.char_indices().peekable();
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
                // Colour glyphs (emoji) don't take the selection-fg /
                // hint-fg swap — matches Ghostty's behaviour for
                // bitmap/COLR atlas entries.
                (CellText::ATLAS_COLOR, [255, 255, 255, 255])
            } else if !needs_per_cell_check {
                // Fast path — no selection / color-changing hints on
                // this row.
                (
                    CellText::ATLAS_GRAYSCALE,
                    cell_fg(src_sq, style_set, renderer, term_colors),
                )
            } else {
                let is_sel = cell_in_row_sel(row_sel, src_col as u16);
                let hint_tag = if is_sel {
                    None
                } else {
                    cell_in_row_hints(row_hints, src_col as u16)
                };
                if is_sel {
                    (
                        CellText::ATLAS_GRAYSCALE,
                        cell_fg_selected(src_sq, style_set, renderer, term_colors),
                    )
                } else if let Some(tag) = hint_tag {
                    // Hint-fg wins over the cell's own fg, matching
                    // Ghostty's `.search` / `.search_selected` branches at
                    // `generic.zig:2829-2833` (the fg picker mirrors bg).
                    (CellText::ATLAS_GRAYSCALE, cell_fg_hinted(tag, renderer))
                } else {
                    (
                        CellText::ATLAS_GRAYSCALE,
                        cell_fg(src_sq, style_set, renderer, term_colors),
                    )
                }
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
        row_sel,
        row_hints,
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
    row_sel: Option<RowSelection>,
    row_hints: &[RowHint],
    fg_scratch: &mut Vec<CellText>,
) {
    for x in 0..cols {
        let sq = row[Column(x)];
        let style = style_set.get(sq.style_id());
        let col = x as u16;
        // SGR underline (UNDER, double, curly, …) wins over the
        // hover-only forced underline. When the cell has no SGR
        // decoration but is inside a hovered hyperlink, emit a plain
        // single-line underline using the cell fg color — same shape
        // as Ghostty's hyperlink-hover affordance.
        let (deco, hover_force) = match underline_style_from_flags(style.flags) {
            Some(d) => (d, false),
            None if cell_in_hover_underline(row_hints, col) => {
                (DecorationStyle::Underline, true)
            }
            None => continue,
        };
        let Some(slot) = ensure_decoration_slot(grid, deco, cell_w, cell_h, thickness)
        else {
            continue;
        };
        if slot.w == 0 || slot.h == 0 {
            continue;
        }
        let color = if cell_in_row_sel(row_sel, col) {
            // Inside selection: underline follows the selection fg so
            // it stays visible against the selection bg. SGR 58 is
            // suppressed here — a theme's selection_foreground
            // overrides per-cell decoration color.
            cell_fg_selected(sq, style_set, renderer, term_colors)
        } else if let Some(tag) = cell_in_row_hints(row_hints, col) {
            // Same reasoning as selection: underline inside a hint
            // should stay legible on the hint bg.
            cell_fg_hinted(tag, renderer)
        } else if hover_force {
            // Hover-only forced underline: use the cell fg so the
            // underline tracks the hyperlink text color (matches
            // Ghostty's hyperlink hover affordance).
            cell_fg(sq, style_set, renderer, term_colors)
        } else {
            decoration_color(sq, &style, style_set, renderer, term_colors)
        };
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
    row_sel: Option<RowSelection>,
    row_hints: &[RowHint],
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
        let col = x as u16;
        // Strikethrough always uses the cell fg (there's no SGR for
        // a separate strike color, matching Ghostty).
        let color = if cell_in_row_sel(row_sel, col) {
            cell_fg_selected(sq, style_set, renderer, term_colors)
        } else if let Some(tag) = cell_in_row_hints(row_hints, col) {
            cell_fg_hinted(tag, renderer)
        } else {
            cell_fg(sq, style_set, renderer, term_colors)
        };
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
