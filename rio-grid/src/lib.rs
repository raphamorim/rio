// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! `rio-grid`: shared grid-emit crate.
//!
//! Translates terminal `Square` cells into `CellBg` / `CellText`
//! instances for the grid GPU renderer. Decoupled from any frontend
//! via the [`GridPalette`] trait, so both rioterm and libsugarloaf can
//! drive it.
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
//! `font::shaper::run::RunIterator`.

use core::hash::Hasher;
use rio_backend::config::colors::term::TermColors;
use rio_backend::config::colors::{AnsiColor, NamedColor};
use rio_backend::crosswords::grid::row::Row;
use rio_backend::crosswords::pos::{Column, Line, Pos};
use rio_backend::crosswords::search::Match;
use rio_backend::crosswords::square::{ContentTag, Extras, Square, Wide};
use rio_backend::crosswords::style::{Style, StyleFlags, UnderlineKind};
use rio_backend::selection::SelectionRange;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

/// Snapshot's per-frame extras map. Keyed by the cell's `extras_id`,
/// populated by `Crosswords::snapshot_visible` from a walk of visible
/// cells. The renderer reads via `extras.get(&id)`.
pub type ExtrasMap = FxHashMap<u16, Extras>;

pub mod preedit;
use preedit::{PreeditCaret, PreeditCell, PreeditLine};

/// The IME composition threaded into a row build. Only handed to the
/// emit passes for the single row the composition lives on.
pub struct PreeditRow<'a> {
    pub line: &'a PreeditLine,
    /// The cursor color the frame resolved once (OSC 12 wins, then the
    /// theme): the same value the cursor-block uniforms use, threaded
    /// here so the block fill, the PastEnd beam, and the cursor can
    /// never diverge.
    pub block_bg: [u8; 4],
}

impl PreeditRow<'_> {
    #[inline]
    fn cell(&self, col: usize) -> Option<PreeditCell> {
        self.line.cell(col)
    }

    /// Whether ink drawn at `col` spanning `span` cells would land on
    /// any composition cell. Used where the span is dynamic (a custom
    /// glyph's render span); per-cell emitters use [`Self::suppresses`].
    #[inline]
    fn covers_ink(&self, col: usize, span: usize) -> bool {
        (col..col.saturating_add(span)).any(|c| self.cell(c).is_some())
    }

    /// THE suppression policy for per-cell fg emitters (glyphs and
    /// their decorations) while composing: drop the cell when it is
    /// under the block, when it is a wide base whose spacer is, or
    /// when it is a spacer whose base is. Half-covered wide glyphs
    /// vanish whole (the rule the grid applies when half of a wide
    /// char is overwritten), and neither half may leave a floating
    /// decoration behind.
    #[inline]
    fn suppresses(&self, sq: Square, col: usize) -> bool {
        if self.cell(col).is_some() {
            return true;
        }
        match sq.wide() {
            Wide::Wide => self.cell(col + 1).is_some(),
            Wide::Spacer => col > 0 && self.cell(col - 1).is_some(),
            _ => false,
        }
    }
}

/// Color/palette operations the emit code needs from its host
/// renderer. Implemented by the frontend (e.g. rioterm's `Renderer`)
/// and passed by generic reference into the per-cell hot path, so the
/// calls monomorphize with no vtable cost.
pub trait GridPalette {
    fn named_colors(&self) -> &rio_backend::config::colors::Colors;
    fn compute_color(
        &self,
        color: &AnsiColor,
        flags: StyleFlags,
        term_colors: &TermColors,
    ) -> rio_backend::config::colors::ColorArray;
    fn compute_bg_color(
        &self,
        cell_style: &Style,
        term_colors: &TermColors,
    ) -> rio_backend::config::colors::ColorArray;
    fn color(
        &self,
        idx: usize,
        term_colors: &TermColors,
    ) -> rio_backend::config::colors::ColorArray;
    fn use_drawable_chars(&self) -> bool;
    fn opacity_cells(&self) -> bool;
    fn cell_bg_alpha(&self) -> u8;
    fn ignore_selection_fg_color(&self) -> bool;
}

/// A single hint-mode label overlaid on a cell (leader-key jump
/// target). Mirrors the frontend's own `HintLabel`; the frontend
/// copies its fields into this at the call sites.
pub struct HintLabel {
    pub position: Pos,
    pub label: char,
    pub is_first: bool,
}

/// The pre-resolved style for cell `x`: bg-only cells already hold the
/// default here (their color travels inline in the cell).
#[inline(always)]
pub fn resolve_style(row_styles: &[Style], x: usize) -> Style {
    row_styles.get(x).copied().unwrap_or_default()
}

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

/// Search-hint category at a cell. `HighlightTag`
/// — we use the same two-way
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
    Label,
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
/// as (`generic.zig:1330-1353`: "The order below matters.
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

/// The two hint-label badge styles: (first-char, following-chars).
pub fn hint_label_styles(
    hint_foreground: rio_backend::config::colors::ColorArray,
    hint_background: rio_backend::config::colors::ColorArray,
) -> (Style, Style) {
    use rio_backend::config::colors::ColorRgb;
    let fg = AnsiColor::Spec(ColorRgb::from_color_arr(hint_foreground));
    let first = Style {
        fg,
        bg: AnsiColor::Spec(ColorRgb::from_color_arr(hint_background)),
        underline_color: None,
        flags: StyleFlags::BOLD,
    };
    let dimmed = [
        hint_background[0] * 0.8,
        hint_background[1] * 0.8,
        hint_background[2] * 0.8,
        hint_background[3],
    ];
    let rest = Style {
        fg,
        bg: AnsiColor::Spec(ColorRgb::from_color_arr(dimmed)),
        underline_color: None,
        flags: StyleFlags::BOLD,
    };
    (first, rest)
}

/// Style id stamped on overlaid hint-label cells. Never produced by
/// interning (the id cap stops before this index), so the shaping-run
/// id comparison always breaks at label boundaries even though the
/// badge style itself lives in the resolved row styles.
const HINT_LABEL_STYLE_ID: u16 = u16::MAX;

pub fn overlay_hint_labels(
    row: &Row<Square>,
    row_styles: &[Style],
    labels: &[HintLabel],
    y: usize,
    display_offset: i32,
    label_styles: (Style, Style),
    row_hints: &mut Vec<RowHint>,
) -> Option<(Row<Square>, Vec<Style>)> {
    let line = Line((y as i32) - display_offset);
    let mut out: Option<(Row<Square>, Vec<Style>)> = None;
    let (first_style, rest_style) = label_styles;
    for label in labels {
        if label.position.row != line {
            continue;
        }
        let col = label.position.col.0;
        if col >= row.len() {
            continue;
        }
        let (target, styles) = out.get_or_insert_with(|| {
            let mut styles = row_styles.to_vec();
            styles.resize(row.len(), Style::default());
            (row.clone(), styles)
        });
        let mut sq = Square::from_char(label.label);
        sq.set_style_id(HINT_LABEL_STYLE_ID);
        target[Column(col)] = sq;
        styles[col] = if label.is_first {
            first_style
        } else {
            rest_style
        };
        row_hints.insert(
            0,
            RowHint {
                lo: col as u16,
                hi: col as u16,
                tag: HintTag::Label,
            },
        );
    }
    out
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
    row_hints
        .iter()
        .any(|rh| rh.tag == HintTag::HyperlinkHover && col >= rh.lo && col <= rh.hi)
}

/// Foreground for a hint-matched cell. Mirrors `cell_fg_selected` but
/// uses the configured `search_match_foreground` /
/// `search_focused_match_foreground` from
/// `colors::Colors` (`rio-backend/src/config/colors/mod.rs:287,299`).
#[inline]
fn cell_fg_hinted<P: GridPalette>(tag: HintTag, palette: &P) -> [u8; 4] {
    match tag {
        HintTag::Focused => {
            normalized_to_u8(palette.named_colors().search_focused_match_foreground)
        }
        HintTag::Match => {
            normalized_to_u8(palette.named_colors().search_match_foreground)
        }
        HintTag::Label => normalized_to_u8(palette.named_colors().hint_foreground),
        // Hover doesn't change fg color; defensive — `cell_in_row_hints`
        // already filters this tag out, so this arm shouldn't fire.
        HintTag::HyperlinkHover => [0, 0, 0, 0],
    }
}

use rio_backend::sugarloaf::font::FontLibrary;
use rio_backend::sugarloaf::grid::{
    AtlasSlot, CellBg, CellText, GlyphKey, GridRenderer, RasterizedGlyph,
};

// Bg + shared helpers

pub fn cell_fg<P: GridPalette>(
    sq: Square,
    style: Style,
    palette: &P,
    term_colors: &TermColors,
) -> [u8; 4] {
    if sq.is_bg_only() {
        return normalized_to_u8(palette.named_colors().foreground);
    }
    let mut style = style;
    if style.flags.contains(StyleFlags::INVERSE) {
        std::mem::swap(&mut style.fg, &mut style.bg);
    }
    let color = palette.compute_color(&style.fg, style.flags, term_colors);
    normalized_to_u8(color)
}

/// Foreground for a selected cell. selection-fg
/// rule: use the configured `selection-foreground`
/// unless the user asked to keep the cell's own fg (Rio's
/// `ignore-selection-foreground-color`). falls back to
/// `state.colors.background` when no color is configured; Rio always
/// has a default selection_foreground populated in its theme, so we
/// use it directly.
#[inline]
pub fn cell_fg_selected<P: GridPalette>(
    sq: Square,
    style: Style,
    palette: &P,
    term_colors: &TermColors,
) -> [u8; 4] {
    if palette.ignore_selection_fg_color() {
        cell_fg(sq, style, palette, term_colors)
    } else {
        normalized_to_u8(palette.named_colors().selection_foreground)
    }
}

// Decoration sprites (underlines, strikethrough)
//
// Underline/strikethrough sprites are pre-rasterized into the
// grayscale atlas and emitted as regular `CellText` entries: one sprite
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
    /// Thick underline marking the IME caret on a composition cell.
    /// Drawn in the terminal background color so it reads against the
    /// cursor-colored block — a beam there would be cursor-on-cursor
    /// and invisible.
    ImeCaretUnderline = 6,
    /// Vertical beam marking the IME caret one cell past the
    /// composition, where there is no block behind it; drawn in the
    /// cursor color against the normal background.
    ImeCaretBeam = 7,
}

/// Sentinel font_id base for decoration sprites. Real font_ids come
/// from sugarloaf's font library which packs into usize indices
/// starting at 0; 0xFFFF_FF00+ is far outside that range. Matches
/// `font.sprite_index` idea.
const DECORATION_FONT_ID_BASE: u32 = 0xFFFF_FF00;

/// Sentinel font_id for Glyph Protocol registrations. Pulled directly
/// in u32 form from `sugarloaf::font::glyph_registry`; lands above the
/// cursor/decoration ranges and never collides with a real font index.
/// The atlas `glyph_id` for a registered cell is
/// `pack_atlas_glyph_id(codepoint, version)`.
use rio_backend::sugarloaf::font::glyph_registry::CUSTOM_GLYPH_FONT_ID_U32;

/// Sentinel font_id base for cursor sprites. Distinct from the
/// decoration range so the two never collide in the atlas
/// hash-key space.
const CURSOR_FONT_ID_BASE: u32 = 0xFFFF_FE00;

/// Sentinel font_id for built-in drawable sprites (box-drawing, blocks,
/// braille, …). A single id suffices — the codepoint itself goes in the
/// atlas `glyph_id`. Sits below the cursor range and far above any real
/// font index, so it never collides.
const DRAWABLE_FONT_ID: u32 = 0xFFFF_FD00;

/// Cursor sprite styles. `font.Sprite::cursor_*`
///. Each variant maps to a distinct
/// rasterized bitmap stored in the grid's grayscale atlas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
enum CursorSpriteStyle {
    /// Full-cell filled rectangle. Drawn UNDER text via slot 0 so
    /// inverted text composites on top.
    Block = 0,
    /// Outlined rectangle (focused-cell border for inactive panes).
    Hollow = 1,
    /// Vertical bar, `thickness` px wide, centered on the LEFT edge
    /// of the cursor cell (straddles the cell boundary).
    Bar = 2,
    /// Horizontal bar at the underline position, `thickness` px tall.
    Underline = 3,
}

impl CursorSpriteStyle {
    /// Block cursors land in `fg_rows[0]` so glyphs draw on top
    /// (the text shader's fg-swap handles the inverted character).
    /// Everything else lands in the non-block slot to overlay text.
    #[inline]
    fn is_block_slot(self) -> bool {
        matches!(self, CursorSpriteStyle::Block)
    }
}

/// Top-level cursor render decision.
/// `renderer::cursor::Style` enum — a
/// superset of the terminal's cursor shapes that adds the
/// inactive-pane variant. Lock isn't implemented yet; password-input
/// detection would need DEC mode 2004 plumbing in the parser.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorRenderStyle {
    /// Active focused block, painted via uniforms (text inverts).
    /// Also emits a `cursor_rect` sprite into slot 0 for parity with
    /// .
    Block,
    /// Outlined rectangle for inactive split panels.
    BlockHollow,
    /// Vertical bar (`beam` in rio's config; calls it `bar`).
    Bar,
    /// Underscore at the cell baseline.
    Underline,
}

/// Inputs to the cursor-style decision.
/// `renderer::cursor::StyleOptions`.
pub struct CursorRenderInputs {
    /// `false` when DECTCEM hides the cursor.
    pub visible: bool,
    /// `true` when this panel currently has focus.
    pub focused: bool,
    /// `true` for the visible half of a blink cycle. Pass `true`
    /// when blink is disabled.
    pub blink_visible: bool,
    /// `true` when the cursor is blinking (DEC blinking shape, or
    /// SGR cursor blink).
    pub blinking: bool,
    /// `true` while an IME pre-edit string is active. Forces block
    /// regardless of the configured shape so the user can tell IME
    /// is taking input.
    pub preedit: bool,
    /// The terminal-side configured cursor shape (block / underline /
    /// beam / hidden).
    pub shape: rio_backend::ansi::CursorShape,
}

/// Decide which cursor variant to render this frame, or `None` to
/// skip emission entirely (hidden cursor / blink-off half-frame).
/// Strict priority order mirrors :
/// preedit > visibility > focused > blink > terminal shape.
pub fn cursor_render_style(opts: CursorRenderInputs) -> Option<CursorRenderStyle> {
    use rio_backend::ansi::CursorShape;
    if opts.preedit {
        return Some(CursorRenderStyle::Block);
    }
    if !opts.visible || opts.shape == CursorShape::Hidden {
        return None;
    }
    if !opts.focused {
        return Some(CursorRenderStyle::BlockHollow);
    }
    if opts.blinking && !opts.blink_visible {
        return None;
    }
    Some(match opts.shape {
        CursorShape::Block => CursorRenderStyle::Block,
        CursorShape::Underline => CursorRenderStyle::Underline,
        CursorShape::Beam => CursorRenderStyle::Bar,
        // Hidden was filtered out by the visibility check above.
        CursorShape::Hidden => unreachable!("hidden shape is filtered above"),
    })
}

impl CursorRenderStyle {
    #[inline]
    fn sprite(self) -> CursorSpriteStyle {
        match self {
            CursorRenderStyle::Block => CursorSpriteStyle::Block,
            CursorRenderStyle::BlockHollow => CursorSpriteStyle::Hollow,
            CursorRenderStyle::Bar => CursorSpriteStyle::Bar,
            CursorRenderStyle::Underline => CursorSpriteStyle::Underline,
        }
    }
}

/// Cursor stroke thickness in physical px. pulls this from
/// font metrics (`metrics.cursor_thickness`); we approximate from
/// cell height. Capped at 2 px so deeply-zoomed cells don't get a
/// chunky frame / fat bar instead of a cursor hint.
#[inline]
pub fn cursor_thickness(cell_h: u32) -> u32 {
    (cell_h / 16).clamp(1, 2)
}

/// Per-style sprite bitmap + bearings. Top-of-sprite bearing is
/// `cell_h` for vertical-fill sprites (block / hollow / bar) so the
/// sprite's top edge aligns with the cell top; underline uses a
/// smaller bearing so the sprite sits near the cell baseline.
fn rasterize_cursor(
    style: CursorSpriteStyle,
    cell_w: u32,
    cell_h: u32,
    thickness: u32,
) -> (Vec<u8>, u16, u16, i16, i16) {
    let t = thickness.max(1);
    match style {
        CursorSpriteStyle::Block => {
            // Full-cell fill.
            let bytes = vec![0xFFu8; (cell_w * cell_h) as usize];
            (
                bytes,
                cell_w.min(u16::MAX as u32) as u16,
                cell_h.min(u16::MAX as u32) as u16,
                0,
                cell_h.min(i16::MAX as u32) as i16,
            )
        }
        CursorSpriteStyle::Hollow => {
            // Filled rect minus inset rect (= border ring). Same as
            // `cursor_hollow_rect`.
            let row_w = cell_w as usize;
            let h = cell_h as usize;
            let mut bytes = vec![0u8; row_w * h];
            let ti = (t as usize).max(1);
            for row in 0..ti.min(h) {
                let s = row * row_w;
                bytes[s..s + row_w].fill(0xFF);
            }
            for row in h.saturating_sub(ti)..h {
                let s = row * row_w;
                bytes[s..s + row_w].fill(0xFF);
            }
            for row in ti..h.saturating_sub(ti) {
                let s = row * row_w;
                for col in 0..ti.min(row_w) {
                    bytes[s + col] = 0xFF;
                }
                for col in row_w.saturating_sub(ti)..row_w {
                    bytes[s + col] = 0xFF;
                }
            }
            (
                bytes,
                cell_w.min(u16::MAX as u32) as u16,
                cell_h.min(u16::MAX as u32) as u16,
                0,
                cell_h.min(i16::MAX as u32) as i16,
            )
        }
        CursorSpriteStyle::Bar => {
            // Vertical bar `t` px wide, full cell height. Negative
            // bearing_x straddles the cell boundary so a bar between
            // cells `n-1` and `n` looks right. uses
            // `x = -(thickness + 1) / 2`.
            let bytes = vec![0xFFu8; (t * cell_h) as usize];
            let bearing_x = -((t as i16 + 1) / 2);
            (
                bytes,
                t.min(u16::MAX as u32) as u16,
                cell_h.min(u16::MAX as u32) as u16,
                bearing_x,
                cell_h.min(i16::MAX as u32) as i16,
            )
        }
        CursorSpriteStyle::Underline => {
            // Horizontal bar at the underline position. Reuse the
            // SGR-underline gap formula so the cursor underline sits
            // at the same baseline as a regular underline.
            let bytes = vec![0xFFu8; (cell_w * t) as usize];
            // The text shader's `glyph_y = cell_pos.y + cell_h -
            // bearing_y` puts the sprite top at
            // `cell_h - (t + gap)`, leaving a `gap` of empty rows
            // below the underline.
            let bearing_y = (t + underline_gap_below(cell_h)) as i16;
            (
                bytes,
                cell_w.min(u16::MAX as u32) as u16,
                t.min(u16::MAX as u32) as u16,
                0,
                bearing_y,
            )
        }
    }
}

/// Lookup or insert a cursor sprite. `size_bucket` packs `(thickness,
/// cell_h)` so a font-size or DPI change invalidates the cached
/// sprite. `cell_w` is the glyph_id so wide-cell sprites (CJK
/// double-width) get their own slot.
fn ensure_cursor_sprite_slot(
    grid: &mut GridRenderer,
    style: CursorSpriteStyle,
    cell_w: u32,
    cell_h: u32,
    thickness: u32,
) -> Option<AtlasSlot> {
    let key = GlyphKey {
        font_id: CURSOR_FONT_ID_BASE + style as u32,
        glyph_id: cell_w,
        size_bucket: ((thickness as u16 & 0xF) << 12) | (cell_h.min(0xFFF) as u16),
    };
    if let Some(slot) = grid.lookup_glyph(key) {
        return Some(slot);
    }
    let (bytes, w, h, bearing_x, bearing_y) =
        rasterize_cursor(style, cell_w, cell_h, thickness);
    grid.insert_glyph(
        key,
        RasterizedGlyph {
            width: w,
            height: h,
            bearing_x,
            bearing_y,
            bytes: &bytes,
        },
    )
}

/// Look up or rasterize a built-in drawable sprite (box-drawing, …) into
/// the grid atlas. Keyed by codepoint + cell height, so a font-size / DPI
/// change re-rasterizes; rasterized once then served from the atlas like
/// any glyph. Modeled on `ensure_cursor_sprite_slot`. Returns `None` when
/// the codepoint isn't a sprite we draw, so the caller falls back to the
/// font.
fn ensure_drawable_sprite(
    grid: &mut GridRenderer,
    cp: u32,
    cell_w: u32,
    cell_h: u32,
) -> Option<AtlasSlot> {
    let key = GlyphKey {
        font_id: DRAWABLE_FONT_ID,
        glyph_id: cp,
        // One cell size per pane atlas, so `cell_h` alone identifies the
        // rasterization (`cell_w` is a function of the font size). Sprites
        // are always single-cell; wide (CJK-ambiguous) cells aren't
        // special-cased.
        size_bucket: cell_h.min(u16::MAX as u32) as u16,
    };
    if let Some(slot) = grid.lookup_glyph(key) {
        return Some(slot);
    }
    let sprite = rio_backend::sugarloaf::sprite::rasterize(cp, cell_w, cell_h)?;
    grid.insert_glyph(
        key,
        RasterizedGlyph {
            width: sprite.width,
            height: sprite.height,
            // Full-cell sprite anchored to the cell box (same convention
            // as the block cursor): top-left at the cell origin.
            bearing_x: 0,
            bearing_y: cell_h.min(i16::MAX as u32) as i16,
            bytes: &sprite.bytes,
        },
    )
}

pub fn cursor_sprite_cell(
    grid: &mut GridRenderer,
    style: CursorRenderStyle,
    col: u16,
    row: u16,
    color: [u8; 4],
    cell_w: u32,
    cell_h: u32,
) -> Option<(bool, CellText)> {
    let sprite = style.sprite();
    let thickness = cursor_thickness(cell_h);
    let slot = ensure_cursor_sprite_slot(grid, sprite, cell_w, cell_h, thickness)?;
    if slot.w == 0 || slot.h == 0 {
        return None;
    }
    let cursor_cell = CellText {
        glyph_pos: [slot.x as u32, slot.y as u32],
        glyph_size: [slot.w as u32, slot.h as u32],
        bearings: [slot.bearing_x, slot.bearing_y],
        grid_pos: [col, row],
        color,
        atlas: CellText::ATLAS_GRAYSCALE,
        // Marks this as "the cursor itself" so the text shader's
        // fg-swap skips it (the sprite paints in `color` directly,
        // not in `cursor_color` from the uniforms).
        bools: CellText::BOOL_IS_CURSOR_GLYPH,
        page: slot.page,
        _pad: 0,
    };
    Some((sprite.is_block_slot(), cursor_cell))
}

/// Underline thickness in physical pixels. fallback
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
/// below. Mirrors the spirit of `underline_position` but
/// simplified — we don't have per-font metrics here.
#[inline]
pub fn underline_gap_below(cell_h: u32) -> u32 {
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
            // uses 3 segments per cell
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
            // join smoothly. two-cubic-Bezier shape
            //:
            // amplitude = cell_w / π
            // stroke width = thickness, round caps
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
        DecorationStyle::ImeCaretUnderline => {
            // The cursor's underline sprite at doubled thickness: one
            // rasterizer for both, so a change to the underline
            // position can't leave the IME caret misaligned with the
            // underline cursor.
            let t2 = (thickness * 2).max(2).min(cell_h);
            let (bytes, w, h, _bearing_x, bearing_y) =
                rasterize_cursor(CursorSpriteStyle::Underline, cell_w, cell_h, t2);
            (bytes, w as u32, h as u32, bearing_y)
        }
        DecorationStyle::ImeCaretBeam => {
            // Full-height beam pinned to the cell's left edge, in the
            // same visual weight as the underline decorations.
            let w = thickness.max(1).min(cell_w);
            let bytes = vec![0xFFu8; (w * cell_h) as usize];
            (bytes, w, cell_h, cell_h as i16)
        }
    }
}

/// Look up or insert a decoration sprite into the grid atlas. Keyed by
/// (decoration font_id sentinel, cell_w as glyph_id, thickness+cell_h
/// as size_bucket) — the same cache that backs regular glyphs, so
/// decorations ride the grid's glyph-eviction policy for free. Every
/// decoration's bearing (and the IME beam's height) depends on
/// `cell_h`, so it must key the sprite or a line-height-only config
/// reload serves stale-height sprites until eviction.
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
        size_bucket: ((thickness as u16 & 0xF) << 12) | (cell_h.min(0xFFF) as u16),
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
/// if the cell has no underline.
#[inline]
fn underline_style_from_flags(flags: StyleFlags) -> Option<DecorationStyle> {
    flags.underline_kind().map(|kind| match kind {
        UnderlineKind::Single => DecorationStyle::Underline,
        UnderlineKind::Double => DecorationStyle::DoubleUnderline,
        UnderlineKind::Curly => DecorationStyle::CurlyUnderline,
        UnderlineKind::Dotted => DecorationStyle::DottedUnderline,
        UnderlineKind::Dashed => DecorationStyle::DashedUnderline,
    })
}

/// Decoration color: SGR 58 `underline_color` if set, else the cell's
/// computed fg. `generic.zig:2968`.
#[inline]
fn decoration_color<P: GridPalette>(
    sq: Square,
    style: &rio_backend::crosswords::style::Style,
    palette: &P,
    term_colors: &TermColors,
) -> [u8; 4] {
    if let Some(uc) = style.underline_color {
        normalized_to_u8(palette.compute_color(&uc, style.flags, term_colors))
    } else {
        cell_fg(sq, *style, palette, term_colors)
    }
}

/// Per-cell background color including the alpha-routing logic that
/// makes window opacity actually visible:
///
/// - Cells with an **explicit** bg (BgRgb / BgPalette inline encoding,
///   or a non-default `style.bg`) → `alpha = 255`. Stays opaque so
///   syntax-highlighted regions, TUI panels, etc. don't bleed through.
///   With `window.opacity-cells = true`, the per-frame opacity gets
///   applied here too — for users who want their Neovim / tmux UI to
///   share the window translucency.
/// - Cells with the **terminal default** bg (`style.bg ==
///   AnsiColor::Named(NamedColor::Background)` and no INVERSE) →
///   `alpha = 0`. The grid bg pass blends premultiplied-over so writing
///   `(0,0,0,0)` is a no-op — the drawable's clear color (which
///   carries `window.opacity` via `dynamic_background.1.a`) shows
///   through. This is what gives the user a translucent window.
/// - INVERSE flag → fg/bg swap promotes the cell to "has explicit bg"
///   so it stays opaque. INVERSE bypasses `opacity-cells` to keep
///   cursor / inverted-text readable.
///
/// Selection / hint highlights are applied at the `build_row_bg` slow
/// path with their own (always opaque) bg colors, so they don't go
/// through this function.
pub fn cell_bg<P: GridPalette>(
    sq: Square,
    style: Style,
    palette: &P,
    term_colors: &TermColors,
) -> [u8; 4] {
    // Alpha for cells that paint an explicit bg. Default = fully
    // opaque (keeps TUI contrast). With `window.opacity-cells = true`
    // and a transparent window, we multiply by the window opacity so
    // the explicit-bg cells stay proportionally translucent. INVERSE
    // always uses 255.
    let explicit_bg_alpha = if palette.opacity_cells() {
        palette.cell_bg_alpha()
    } else {
        255
    };

    match sq.content_tag() {
        ContentTag::BgRgb => {
            let (r, g, b) = sq.bg_rgb();
            [r, g, b, explicit_bg_alpha]
        }
        ContentTag::BgPalette => {
            let idx = sq.bg_palette_index() as usize;
            let color = palette.color(idx, term_colors);
            let [r, g, b, _] = normalized_to_u8(color);
            [r, g, b, explicit_bg_alpha]
        }
        ContentTag::Codepoint => {
            let inverse = style.flags.contains(StyleFlags::INVERSE);
            // "Default bg": the cell carries the terminal-default bg
            // sentinel with no SGR override. INVERSE flips fg/bg,
            // which always produces a non-default effective bg, so
            // treat as explicit.
            let has_default_bg =
                !inverse && matches!(style.bg, AnsiColor::Named(NamedColor::Background));
            if has_default_bg {
                // Skip painting → the translucent clear (or opaque
                // global bg, for non-transparent windows) shows
                // through unchanged. Premultiplied blending makes
                // (0,0,0,0) a no-op.
                return [0, 0, 0, 0];
            }
            // Resolve the explicit bg (with INVERSE swap applied).
            let mut resolved = style;
            if inverse {
                std::mem::swap(&mut resolved.fg, &mut resolved.bg);
            }
            let color = palette.compute_bg_color(&resolved, term_colors);
            let [r, g, b, _] = normalized_to_u8(color);
            // INVERSE always opaque — keep cursor / inverted text
            // readable regardless of the opacity-cells flag.
            let alpha = if inverse { 255 } else { explicit_bg_alpha };
            [r, g, b, alpha]
        }
    }
}

#[inline]
pub fn normalized_to_u8(c: [f32; 4]) -> [u8; 4] {
    [
        (c[0].clamp(0.0, 1.0) * 255.0) as u8,
        (c[1].clamp(0.0, 1.0) * 255.0) as u8,
        (c[2].clamp(0.0, 1.0) * 255.0) as u8,
        (c[3].clamp(0.0, 1.0) * 255.0) as u8,
    ]
}

#[allow(clippy::too_many_arguments)]
pub fn build_row_bg<P: GridPalette>(
    row: &Row<Square>,
    cols: usize,
    row_styles: &[Style],
    palette: &P,
    term_colors: &TermColors,
    row_sel: Option<RowSelection>,
    row_hints: &[RowHint],
    preedit: Option<&PreeditRow<'_>>,
    bg_scratch: &mut Vec<CellBg>,
) {
    bg_scratch.clear();

    // Block fill behind every composition cell: the frame's resolved
    // cursor color, so the block and the forced block cursor on the
    // first composition cell can never be two different colors.
    let preedit_block_bg = preedit.map(|p| p.block_bg);

    // Fast path: row has no selection, no color-changing hints, and no
    // composition. (HyperlinkHover only contributes an underline,
    // never bg.) The overwhelming majority of rows in idle terminals
    // hit this path — strip the per-cell `cell_in_row_sel` /
    // `cell_in_row_hints` checks and just walk cells.
    let has_sel = row_sel.is_some();
    let has_color_hints = row_hints.iter().any(|rh| rh.tag != HintTag::HyperlinkHover);
    if !has_sel && !has_color_hints && preedit.is_none() {
        bg_scratch.reserve(cols);
        for x in 0..cols {
            let sq = row[Column(x)];
            bg_scratch.push(CellBg {
                rgba: cell_bg(sq, resolve_style(row_styles, x), palette, term_colors),
            });
        }
        return;
    }

    // Slow path: selection and/or hint highlighting present.
    let sel_bg = if has_sel {
        Some(normalized_to_u8(
            palette.named_colors().selection_background,
        ))
    } else {
        None
    };
    let (match_bg, focused_bg) = if has_color_hints {
        (
            Some(normalized_to_u8(
                palette.named_colors().search_match_background,
            )),
            Some(normalized_to_u8(
                palette.named_colors().search_focused_match_background,
            )),
        )
    } else {
        (None, None)
    };
    for x in 0..cols {
        let sq = row[Column(x)];
        let style = resolve_style(row_styles, x);
        let col = x as u16;
        // The composition wins over selection / hint backgrounds: the
        // user is actively typing here, that signal reads first. Both
        // Start and Continuation cells take the fill so wide clusters
        // span one continuous block.
        let preedit_here = match (preedit, preedit_block_bg) {
            (Some(p), Some(bg)) if p.cell(x).is_some() => Some(bg),
            _ => None,
        };
        let rgba =
            if let Some(bg) = preedit_here {
                bg
            } else if cell_in_row_sel(row_sel, col) {
                // Selection bg wins over hint bg and the cell's own bg,
                // matching `generic.zig:2775-2800` (selection check
                // runs before highlight check).
                sel_bg.unwrap_or_else(|| cell_bg(sq, style, palette, term_colors))
            } else if let Some(tag) = cell_in_row_hints(row_hints, col) {
                match tag {
                    HintTag::Focused => focused_bg
                        .unwrap_or_else(|| cell_bg(sq, style, palette, term_colors)),
                    HintTag::Match => match_bg
                        .unwrap_or_else(|| cell_bg(sq, style, palette, term_colors)),
                    HintTag::Label => cell_bg(sq, style, palette, term_colors),
                    // `cell_in_row_hints` filters HyperlinkHover out, but
                    // make the match exhaustive so a future caller can't
                    // accidentally hit a panic.
                    HintTag::HyperlinkHover => cell_bg(sq, style, palette, term_colors),
                }
            } else {
                cell_bg(sq, style, palette, term_colors)
            };
        bg_scratch.push(CellBg { rgba });
    }
}

// Run-shaping infrastructure (platform-agnostic types)

/// Bits of `StyleFlags` that change shaping / font selection. Bold +
/// italic pick different font files. Color / decoration / dim don't
/// affect shaping so they don't break runs.
const SHAPING_FLAG_MASK: u16 = StyleFlags::BOLD.bits() | StyleFlags::ITALIC.bits();

/// 256 × 8 bucketed LRU cache — CellCacheTable.
const RUN_BUCKET_COUNT: usize = 256;
const RUN_BUCKET_SIZE: usize = 8;

/// One shaped glyph. Same shape from both CoreText (macOS) and swash
/// (non-macOS). `cluster` is the shaping-buffer offset of the source
/// cell: UTF-16 code units on macOS (CoreText string indices), UTF-8
/// bytes elsewhere (swash `cluster.source.start`).
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
    /// Summed glyph advance, computed once at insert so per-frame
    /// consumers (the preedit overflow check) never re-walk glyphs.
    advance: f32,
    /// 64-bit rapidhash of (font_id, size_bucket, style_flags, run bytes).
    /// We key on the hash alone — no stored run string, no equality
    /// check on lookup. `CellCacheTable` pattern
    ///: rapidhash / wyhash pass
    /// SMHasher, so a random collision costs a wrong-glyph frame
    /// until the next row rebuild but never corrupts state. Birthday
    /// bound at N=10k concurrent cache entries ≈ 2.7×10⁻¹².
    hash: u64,
    glyphs: Vec<ShapedGlyph>,
}

pub struct GridGlyphRasterizer {
    /// Cache of `(char, style_flags, route_id) → (font_id, is_emoji)`
    /// resolutions. The route_id is part of the key because Glyph
    /// Protocol registrations are per-pane: the same PUA codepoint
    /// can resolve to `CUSTOM_GLYPH_FONT_ID` in one pane and to a
    /// system font in another. Non-PUA characters resolve identically
    /// across panes; the duplication is cheap (a few bytes per
    /// (char, route) pair) compared to the cost of mis-rendering.
    font_resolve: FxHashMap<(char, u8, usize), (u32, bool)>,
    ascent_cache: FxHashMap<(u32, u16), i16>,
    /// `(should_embolden, should_italicize)` per font_id. Read from
    /// `FontData` synthesis flags; matches the rich-text rasterizer's
    /// convention.
    synthesis_cache: FxHashMap<u32, (bool, bool)>,
    run_cache: Vec<Vec<RunCacheEntry>>,

    /// Per-run hasher rebuilt at every run-start. Hashed incrementally
    /// across the run-extension loop with `(codepoint, cluster)` pairs
    /// per cell, then finalized after the loop with `(cell_count,
    /// font_id, size_bucket, style_flags)`. Position-independent within
    /// a row: identical run text at different starting columns shares
    /// the same hash.
    run_hasher: rapidhash::fast::RapidHasher<'static>,

    // macOS: stage the run in UTF-16 (what CoreText wants natively)
    // so the shaper call can hand the buffer straight to
    // `CFStringCreateWithCharactersNoCopy` with no encoding
    // conversion. `coretext.zig:88-104` — UTF-16
    // `unichars` + a parallel cell-start table for the cluster →
    // cell mapping.
    #[cfg(target_os = "macos")]
    run_utf16_scratch: Vec<u16>,
    /// `run_cell_starts[i]` is the offset where cell `i` of the run
    /// begins inside the platform shaping buffer — UTF-16 code units
    /// into `run_utf16_scratch` on macOS, UTF-8 bytes into
    /// `run_str_scratch` elsewhere. Length = cells in the run. Used to
    /// walk shaped glyphs back to the cell they belong to; explicit
    /// because a cell can contribute several codepoints (its attached
    /// combining marks shape together with the base).
    run_cell_starts: Vec<u32>,
    /// `run_cell_columns[i]` is the absolute grid column for the
    /// `i`-th appended cell in the run. Decouples the cell-index-
    /// within-run from the grid column so wide-char spacer cells can
    /// be skipped (not appended to scratch / hash / column array)
    /// while still letting the glyph→column mapping recover the right
    /// cell for each shaped glyph.
    run_cell_columns: Vec<u16>,
    /// Cached CoreText handles per font_id.
    #[cfg(target_os = "macos")]
    handle_cache: FxHashMap<u32, rio_backend::sugarloaf::font::macos::FontHandle>,

    /// Library-wide `(hinting, features)` snapshot, refreshed lazily
    /// after `clear_font_caches` so shaping and rasterization don't
    /// take the library lock per run.
    lib_settings: Option<(
        bool,
        std::sync::Arc<Vec<rio_backend::sugarloaf::swash::Setting<u16>>>,
    )>,
    /// Per-font `wght` axis pin, mirrored from `FontData.wght_variation`.
    #[cfg(not(target_os = "macos"))]
    wght_cache: FxHashMap<u32, Option<f32>>,

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
            run_hasher: rapidhash::fast::RapidHasher::default(),
            #[cfg(target_os = "macos")]
            run_utf16_scratch: Vec::new(),
            run_cell_starts: Vec::new(),
            run_cell_columns: Vec::new(),
            #[cfg(not(target_os = "macos"))]
            run_str_scratch: String::new(),
            #[cfg(target_os = "macos")]
            handle_cache: FxHashMap::default(),
            lib_settings: None,
            #[cfg(not(target_os = "macos"))]
            wght_cache: FxHashMap::default(),
            #[cfg(not(target_os = "macos"))]
            shape_ctx: rio_backend::sugarloaf::swash::shape::ShapeContext::new(),
            #[cfg(not(target_os = "macos"))]
            scale_ctx: rio_backend::sugarloaf::swash::scale::ScaleContext::new(),
            #[cfg(not(target_os = "macos"))]
            font_data_cache: FxHashMap::default(),
        }
    }

    /// Drop every cache keyed by `font_id`; a swapped font library
    /// reuses the same ids.
    pub fn clear_font_caches(&mut self) {
        self.font_resolve.clear();
        self.ascent_cache.clear();
        self.synthesis_cache.clear();
        for bucket in &mut self.run_cache {
            bucket.clear();
        }
        self.lib_settings = None;
        #[cfg(target_os = "macos")]
        self.handle_cache.clear();
        #[cfg(not(target_os = "macos"))]
        {
            self.wght_cache.clear();
            self.font_data_cache.clear();
        }
    }

    /// Library-wide `(hinting, features)`, cached until the next
    /// `clear_font_caches`.
    fn library_settings(
        &mut self,
        font_library: &FontLibrary,
    ) -> (
        bool,
        std::sync::Arc<Vec<rio_backend::sugarloaf::swash::Setting<u16>>>,
    ) {
        if self.lib_settings.is_none() {
            let lib = font_library.inner.read();
            self.lib_settings = Some((lib.hinting, lib.features.clone()));
        }
        self.lib_settings.clone().unwrap()
    }

    #[inline]
    fn resolve_font(
        &mut self,
        ch: char,
        style_flags: u8,
        font_library: &FontLibrary,
        route_id: usize,
    ) -> (u32, bool) {
        // Kitty Unicode placeholder cells (U+10EEEE) are rendered as
        // image-overlay slices, not text. Resolve them to the primary
        // font as if they were a space, so the run shapes them as an
        // invisible space glyph instead of falling back to a notdef
        // tofu box.
        if ch == rio_backend::ansi::kitty_virtual::PLACEHOLDER {
            return (rio_backend::sugarloaf::font::FONT_ID_REGULAR as u32, false);
        }

        // ASCII printable + regular style → always primary font, never
        // emoji. Skips the FxHashMap lookup that dominates this fn's
        // cost on terminal-typical content. ASCII codepoints can never
        // be Glyph-Protocol-registered (PUA-only restriction), so the
        // route_id is irrelevant on this fast path.
        //
        // Bold / italic ASCII still goes through the cache because
        // the bold and italic font IDs are dynamic (depend on which
        // faces the user loaded), and non-ASCII can hit fallback.
        if style_flags == 0 && (' '..='~').contains(&ch) {
            return (rio_backend::sugarloaf::font::FONT_ID_REGULAR as u32, false);
        }

        *self
            .font_resolve
            .entry((ch, style_flags, route_id))
            .or_insert_with(|| {
                let span_style = span_style_for_flags(style_flags);
                let (id, emoji) =
                    font_library.resolve_font_for_char(ch, &span_style, Some(route_id));
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

/// Hash the cell's zero-width combining codepoints into the per-run
/// hasher. Each combining codepoint is stamped as `(cp, cluster)` with
/// the same cluster as the base cell. Variation Selectors (VS-15 /
/// VS-16) only steer presentation form, not glyph identity, so they're
/// skipped to keep the cache key stable across presentation toggles.
#[inline]
fn hash_combining(
    rasterizer: &mut GridGlyphRasterizer,
    extras_table: &ExtrasMap,
    sq: Square,
    cluster: u32,
) {
    if !sq.has_grapheme() {
        return;
    }
    let Some(id) = sq.extras_id() else {
        return;
    };
    let Some(extras) = extras_table.get(&id) else {
        return;
    };
    for &cp in &extras.zerowidth {
        if cp == '\u{FE0E}' || cp == '\u{FE0F}' {
            continue;
        }
        rasterizer.run_hasher.write_u32(cp as u32);
        rasterizer.run_hasher.write_u32(cluster);
    }
}

/// Append a cell's attached combining codepoints to the shaping
/// buffer, so the shaper composes them with the base glyph (`e` +
/// U+0301 renders as `é` instead of a bare `e`). Mirrors
/// [`hash_combining`]: VS15/VS16 are skipped — presentation is
/// already resolved into `font_id`, and feeding a selector to a text
/// font's shaper can only produce a notdef.
fn push_cluster_text(
    rasterizer: &mut GridGlyphRasterizer,
    extras_table: &ExtrasMap,
    sq: Square,
) {
    if !sq.has_grapheme() {
        return;
    }
    let Some(id) = sq.extras_id() else {
        return;
    };
    let Some(extras) = extras_table.get(&id) else {
        return;
    };
    push_cluster_chars(rasterizer, &extras.zerowidth);
}

/// Platform-specific tail of [`push_cluster_text`], split out so the
/// buffer layout is unit-testable without building a `Square`.
fn push_cluster_chars(rasterizer: &mut GridGlyphRasterizer, marks: &[char]) {
    for &cp in marks {
        if cp == '\u{FE0E}' || cp == '\u{FE0F}' {
            continue;
        }
        #[cfg(target_os = "macos")]
        {
            let mut buf = [0u16; 2];
            rasterizer
                .run_utf16_scratch
                .extend_from_slice(cp.encode_utf16(&mut buf));
        }
        #[cfg(not(target_os = "macos"))]
        rasterizer.run_str_scratch.push(cp);
    }
}

// Force inline — called once per cell during run extension on the hot
// path; body is two field reads + two compares so a real call is pure
// overhead.
//
// Wide-char spacer cells (`Wide::Spacer` / `Wide::LeadingSpacer`) carry
// `' '` as their codepoint but represent the right half / left padding
// of a multi-cell glyph rather than an independent space character —
// they're handled separately via `is_skipped_spacer` at the run-start
// and run-extend sites instead of being treated as run breakers, so a
// wide-char run can extend past its own spacer to the next glyph.
#[inline(always)]
fn is_run_breaker(sq: Square) -> bool {
    if sq.is_bg_only() {
        return true;
    }
    sq.c() == '\0'
}

/// Wide-char spacer cells contain a synthetic `' '` to occupy the
/// second column of a wide character (or the trailing column before a
/// soft-wrap). They aren't independent glyphs — the shaper emits the
/// wide glyph at the base cell and we want spacers skipped from the
/// run text + hash + cluster mapping.
#[inline(always)]
fn is_skipped_spacer(sq: Square) -> bool {
    matches!(sq.wide(), Wide::Spacer | Wide::LeadingSpacer)
}

/// Lookup. Hash → bucket; scan from most-recent; rotate on hit. No
/// secondary comparison — we trust the 64-bit rapidhash to be
/// collision-free across realistic workloads. Matches
///.
fn run_cache_get(
    buckets: &mut [Vec<RunCacheEntry>],
    hash: u64,
) -> Option<&RunCacheEntry> {
    let idx = (hash as usize) & (RUN_BUCKET_COUNT - 1);
    let bucket = &mut buckets[idx];
    let last = bucket.len().checked_sub(1)?;
    for i in (0..bucket.len()).rev() {
        if bucket[i].hash == hash {
            if i != last {
                bucket[i..=last].rotate_left(1);
            }
            return Some(&bucket[last]);
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

// Platform-specific shape + ascent helpers

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
    let (_, features) = rasterizer.library_settings(font_library);
    let handle = match rasterizer.handle_cache.entry(font_id) {
        std::collections::hash_map::Entry::Occupied(e) => e.into_mut().clone(),
        std::collections::hash_map::Entry::Vacant(e) => {
            let h = font_library.ct_font(font_id as usize)?;
            // Configured OpenType features are baked into the cached
            // CTFont so both shaping and rasterization honor them.
            let h = if features.is_empty() {
                h
            } else {
                let pairs: Vec<(u32, u16)> =
                    features.iter().map(|s| (s.tag, s.value)).collect();
                h.clone().with_features(&pairs).unwrap_or(h)
            };
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
    use rio_backend::sugarloaf::swash::{FontRef, Setting};

    let (_, features) = rasterizer.library_settings(font_library);
    let wght = *rasterizer.wght_cache.entry(font_id).or_insert_with(|| {
        let lib = font_library.inner.read();
        lib.try_get(&(font_id as usize))
            .and_then(|f| f.wght_variation)
    });

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

    const WGHT_TAG: u32 = u32::from_be_bytes(*b"wght");
    let wght_var = wght.map(|v| Setting {
        tag: WGHT_TAG,
        value: v,
    });
    let mut shaper = rasterizer
        .shape_ctx
        .builder(font_ref)
        .size(size_u16 as f32)
        .features(features.iter().copied())
        .variations(wght_var.iter().copied())
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

// Emission

/// Shape the rasterizer's current run scratch, keyed in the run cache
/// by `hash`, and return `(ascent, summed advance)` for
/// `(font_id, size_bucket)`; on a miss the shaped glyphs are stored
/// under `hash` with their advance. `None` means shaping failed (no
/// font handle). The one shaping-cache protocol, shared by the grid
/// run path and the preedit path.
fn shape_cached(
    rasterizer: &mut GridGlyphRasterizer,
    hash: u64,
    font_id: u32,
    size_u16: u16,
    size_bucket: u16,
    font_library: &FontLibrary,
) -> Option<(i16, f32)> {
    if let Some(entry) = run_cache_get(&mut rasterizer.run_cache, hash) {
        // Cache hit: advance stored, ascent in its own cache.
        let advance = entry.advance;
        return Some((
            rasterizer
                .ascent_cache
                .get(&(font_id, size_bucket))
                .copied()
                .unwrap_or(0),
            advance,
        ));
    }
    #[cfg(target_os = "macos")]
    let shaped_opt =
        shape_run_ct(rasterizer, font_id, size_u16, size_bucket, font_library);
    #[cfg(not(target_os = "macos"))]
    let shaped_opt =
        shape_run_swash(rasterizer, font_id, size_u16, size_bucket, font_library);
    let (glyphs, ascent_px) = shaped_opt?;
    let advance: f32 = glyphs.iter().map(|g| g.advance).sum();
    run_cache_put(
        &mut rasterizer.run_cache,
        RunCacheEntry {
            hash,
            glyphs,
            advance,
        },
    );
    Some((ascent_px, advance))
}

/// Run-level fg emission. Shapes once per run, emits one CellText per
/// shaped glyph. Works on both macOS (CoreText) and non-macOS (swash).
///
/// Emits in three ordered phases so decoration z-order is correct:
/// underlines first (drawn under glyphs), glyphs, then strikethroughs
/// (drawn on top).
#[allow(clippy::too_many_arguments)]
pub fn build_row_fg<P: GridPalette>(
    row: &Row<Square>,
    cols: usize,
    y: u16,
    row_styles: &[Style],
    extras_table: &ExtrasMap,
    palette: &P,
    term_colors: &TermColors,
    rasterizer: &mut GridGlyphRasterizer,
    grid: &mut GridRenderer,
    size_px: f32,
    cell_w: f32,
    cell_h: f32,
    row_sel: Option<RowSelection>,
    row_hints: &[RowHint],
    preedit: Option<&PreeditRow<'_>>,
    font_library: &FontLibrary,
    route_id: usize,
    // Column of the cursor on this row, or `None` if the cursor isn't
    // on this row (different row, or hidden). When `Some`, the
    // run-extension loop breaks the run around the cursor cell so
    // partial ligature lookahead (e.g. `grap` waiting for `h` to form
    // `graph`) can't make pre-cursor cells visually disappear while
    // the user is mid-typing.
    cursor_col_for_row: Option<u16>,
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
    let has_color_hints = row_hints.iter().any(|rh| rh.tag != HintTag::HyperlinkHover);
    let needs_per_cell_check = has_sel || has_color_hints;
    // Consulted in the per-cell sprite hook and the run-extension break.
    let use_drawable_chars = palette.use_drawable_chars();

    // Glyph Protocol registry for *this pane*. One Arc clone per row;
    // the per-cell custom-glyph helper then uses the local handle so
    // it never re-acquires the FontLibrary read lock. Arc clone is
    // cheap; `None` when no program in this pane's session has used
    // the protocol. With multiple panes, each pane consults its own
    // registry by route_id, so two panes can register conflicting
    // glyphs at the same codepoint without interfering.
    let glyph_registry = font_library.glyph_registry_for(route_id);

    // Phase 1: underline pass. Emit before glyphs so grayscale quads
    // draw under the characters.
    emit_underlines(
        row,
        cols,
        y,
        row_styles,
        palette,
        term_colors,
        grid,
        cell_w_u32,
        cell_h_u32,
        thickness,
        row_sel,
        row_hints,
        preedit,
        glyph_registry.as_ref(),
        fg_scratch,
    );

    // Trim the row from the right: walk back to the last non-breaker
    // cell so the outer loop doesn't iterate the (typically large)
    // trailing-blank tail of a partially-filled row.
    let max = (0..cols)
        .rev()
        .find(|&i| !is_run_breaker(row[Column(i)]))
        .map(|i| i + 1)
        .unwrap_or(0);

    let mut x: usize = 0;
    while x < max {
        let sq = row[Column(x)];
        // Composition cells emit in their own pass below (shaped per
        // grapheme cluster so they can't ligate with terminal text),
        // and a wide glyph half-covered by the block is dropped whole.
        if preedit.is_some_and(|p| p.suppresses(sq, x)) {
            x += 1;
            continue;
        }
        if is_run_breaker(sq) {
            x += 1;
            continue;
        }
        // Wide-char spacers shouldn't be a run-start either — the
        // wide glyph lives in the preceding `Wide` cell, this cell is
        // pure padding. Advance past it.
        if is_skipped_spacer(sq) {
            x += 1;
            continue;
        }

        // Open a run at x.
        let ch = sq.c();
        let run_start_style_id = sq.style_id();
        let run_style_flags =
            (resolve_style(row_styles, x).flags.bits() & SHAPING_FLAG_MASK) as u8;
        let (font_id, is_emoji) =
            rasterizer.resolve_font(ch, run_style_flags, font_library, route_id);

        // Glyph Protocol short-circuit: registered codepoints render
        // directly from the registry without shaping, run-extension,
        // or per-platform shaper plumbing. Each registered cell is
        // its own one-cell run.
        if font_id == CUSTOM_GLYPH_FONT_ID_U32 {
            // The font cascade reported a custom glyph but the row
            // already cloned `glyph_registry` as None — registry was
            // detached between font resolution and this branch (rare,
            // but harmless: render nothing).
            let Some(registry) = glyph_registry.as_ref() else {
                x += 1;
                continue;
            };

            // fg colour, mirroring the regular emit loop's
            // selection / hint precedence.
            let style = resolve_style(row_styles, x);
            let color = if !needs_per_cell_check {
                cell_fg(sq, style, palette, term_colors)
            } else {
                let is_sel = cell_in_row_sel(row_sel, x as u16);
                let hint_tag = if is_sel {
                    None
                } else {
                    cell_in_row_hints(row_hints, x as u16)
                };
                if is_sel {
                    cell_fg_selected(sq, style, palette, term_colors)
                } else if let Some(tag) = hint_tag {
                    cell_fg_hinted(tag, palette)
                } else {
                    cell_fg(sq, style, palette, term_colors)
                }
            };

            // The render span comes from the registration's declared
            // `width` (a render hint), NOT the cell layout: a width=2
            // glyph overflows rightward into the following cell(s) in
            // pixels while the grid still treats this codepoint as one
            // logical column. The author is expected to leave that next
            // cell blank (a trailing space) so the overflow lands on
            // empty space rather than real content.
            if let Some((_, slot, is_color, span)) = ensure_custom_glyph_by_codepoint(
                grid, registry, ch as u32, cell_w_u32, cell_h, color,
            ) {
                // A registered glyph's render span can overflow into
                // the next cell: like a wide glyph, drop it while the
                // composition block covers any cell its ink spans.
                if preedit.is_some_and(|p| p.covers_ink(x, span as usize)) {
                    x += 1;
                    continue;
                }
                if slot.w != 0 && slot.h != 0 {
                    // Center the rasterised glyph in its render-span box
                    // (`span × cell_w` wide, `cell_h` tall). The raster
                    // is already contained within that box, so centering
                    // keeps it inside the span regardless of the
                    // outline's own bearings. `bearings.x` is the offset
                    // from the cell's left edge; `bearings.y` is measured
                    // from the cell *bottom* (the shader flips it), so a
                    // vertically-centred glyph's top sits at
                    // `(cell_h + glyph_h) / 2`.
                    let span_w_px =
                        (cell_w_u32 * span as u32).min(i16::MAX as u32) as i16;
                    let cell_h_i16 = cell_h.round().clamp(0.0, i16::MAX as f32) as i16;
                    let glyph_w = slot.w.min(i16::MAX as u16) as i16;
                    let glyph_h = slot.h.min(i16::MAX as u16) as i16;
                    let bearing_x = (span_w_px - glyph_w) / 2;
                    let bearing_y = (cell_h_i16 + glyph_h) / 2;

                    // Colour atlas entries are pre-painted (palette
                    // applied during COLR rasterisation), so the
                    // shader multiplies by white. Mono entries take
                    // the per-cell fg colour the run loop computed
                    // above.
                    let (atlas, color) = if is_color {
                        (CellText::ATLAS_COLOR, [255, 255, 255, 255])
                    } else {
                        (CellText::ATLAS_GRAYSCALE, color)
                    };
                    fg_scratch.push(CellText {
                        glyph_pos: [slot.x as u32, slot.y as u32],
                        glyph_size: [slot.w as u32, slot.h as u32],
                        bearings: [bearing_x, bearing_y],
                        grid_pos: [x as u16, y],
                        color,
                        atlas,
                        bools: 0,
                        page: slot.page,
                        _pad: 0,
                    });
                }
            }
            x += 1;
            continue;
        }

        // Built-in drawable sprite short-circuit: box-drawing and other
        // procedurally-rendered codepoints render as a one-cell atlas
        // sprite (crisp + seamless in any font) instead of the font
        // glyph. Glyph-protocol customs (handled above) still win. If the
        // sprite can't be produced we fall through to normal shaping.
        if use_drawable_chars && rio_backend::sugarloaf::sprite::is_drawable(ch as u32) {
            if let Some(slot) =
                ensure_drawable_sprite(grid, ch as u32, cell_w_u32, cell_h_u32)
            {
                if slot.w != 0 && slot.h != 0 {
                    // fg colour, mirroring the regular emit loop's
                    // selection / hint precedence.
                    let style = resolve_style(row_styles, x);
                    let color = if !needs_per_cell_check {
                        cell_fg(sq, style, palette, term_colors)
                    } else {
                        let is_sel = cell_in_row_sel(row_sel, x as u16);
                        let hint_tag = if is_sel {
                            None
                        } else {
                            cell_in_row_hints(row_hints, x as u16)
                        };
                        if is_sel {
                            cell_fg_selected(sq, style, palette, term_colors)
                        } else if let Some(tag) = hint_tag {
                            cell_fg_hinted(tag, palette)
                        } else {
                            cell_fg(sq, style, palette, term_colors)
                        }
                    };
                    fg_scratch.push(CellText {
                        glyph_pos: [slot.x as u32, slot.y as u32],
                        glyph_size: [slot.w as u32, slot.h as u32],
                        bearings: [slot.bearing_x, slot.bearing_y],
                        grid_pos: [x as u16, y],
                        color,
                        atlas: CellText::ATLAS_GRAYSCALE,
                        bools: 0,
                        page: slot.page,
                        _pad: 0,
                    });
                }
                x += 1;
                continue;
            }
        }

        let run_start = x;
        // Sticky style_id: equal ids imply equal resolved styles (the
        // per-row styles are resolved from these very ids, and overlay
        // label cells carry the reserved HINT_LABEL_STYLE_ID), so the
        // flags comparison only runs on id changes.
        let mut prev_style_id = run_start_style_id;

        // Kitty Unicode placeholder shapes as a space — the cell
        // joins the run, the shaper emits an invisible space glyph
        // (no notdef tofu), and the kitty image overlay is drawn on
        // top to fill the cell.
        let shape_ch = if ch == rio_backend::ansi::kitty_virtual::PLACEHOLDER {
            ' '
        } else {
            ch
        };

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
                .extend_from_slice(shape_ch.encode_utf16(&mut buf));
        }
        #[cfg(not(target_os = "macos"))]
        {
            rasterizer.run_str_scratch.clear();
            rasterizer.run_cell_starts.clear();
            rasterizer.run_cell_starts.push(0);
            rasterizer.run_str_scratch.push(shape_ch);
        }
        // Attached combining marks shape together with their base so
        // `e` + U+0301 composes; kitty placeholder cells are excluded —
        // their `zerowidth` encodes image-slice coordinates, not text.
        if ch != rio_backend::ansi::kitty_virtual::PLACEHOLDER {
            push_cluster_text(rasterizer, extras_table, sq);
        }
        rasterizer.run_cell_columns.clear();
        rasterizer.run_cell_columns.push(x as u16);
        // Reset the per-run hasher and stamp the run-start cell as
        // `(codepoint, cluster=0)`. Subsequent cells append themselves
        // in the run-extension loop below.
        rasterizer.run_hasher = rapidhash::fast::RapidHasher::default();
        rasterizer.run_hasher.write_u32(shape_ch as u32);
        rasterizer.run_hasher.write_u32(0);
        // Hash the cell's zero-width combining codepoints too — without
        // this, `(e, U+0301)` and `(e, U+0302)` would alias in the run
        // cache. Variation Selectors (VS-15 / VS-16) don't change the
        // glyph identity, so skip them.
        hash_combining(rasterizer, extras_table, sq, 0);

        // Extend the run while (font_id, style_flags) match.
        let mut end = x + 1;
        while end < cols {
            let sq2 = row[Column(end)];
            // Stop before composition cells (taken over by the preedit
            // pass) and before a wide glyph the block half-covers,
            // whose shaping would bleed into it (see the run-start
            // guard).
            if preedit.is_some_and(|p| p.suppresses(sq2, end)) {
                break;
            }
            if is_run_breaker(sq2) {
                break;
            }
            // Wide-char spacer: advance past without appending to scratch
            // / hash / column array. The shaper treated the preceding
            // `Wide` cell as the glyph; this cell is just padding.
            if is_skipped_spacer(sq2) {
                end += 1;
                continue;
            }
            // Built-in drawable sprites are emitted one cell at a time by
            // the per-cell hook above; they must not be swallowed into a
            // shaped font run, or only the run's first cell would get a
            // sprite and the rest would fall back to the font. Break so the
            // next outer-loop iteration handles this cell as a sprite.
            if use_drawable_chars
                && rio_backend::sugarloaf::sprite::is_drawable(sq2.c() as u32)
            {
                break;
            }
            // Selection-boundary break: keep selection start / end
            // exactly aligned to a run boundary so per-cell selection
            // re-coloring never lands mid-ligature glyph. `lo` is the
            // first selected column and `hi` is the last (inclusive),
            // so we break when stepping onto `lo` or one past `hi`.
            if let Some(sel) = row_sel {
                let end_u16 = end as u16;
                if end_u16 == sel.lo || end_u16 == sel.hi.saturating_add(1) {
                    break;
                }
            }
            // Hard-break before known-bad Latin ligatures (`fl`, `fi`,
            // `st`). In monospace these typically render with a single
            // ligature glyph that visually breaks the cell grid even
            // when the per-cell font otherwise lines up.
            if !sq2.has_grapheme() {
                let prev = row[Column(end - 1)];
                if !prev.has_grapheme() {
                    let prev_cp = prev.c();
                    let cp = sq2.c();
                    if (prev_cp == 'f' && (cp == 'l' || cp == 'i'))
                        || (prev_cp == 's' && cp == 't')
                    {
                        break;
                    }
                }
            }
            // Cursor break: keep the cursor cell in its own one-cell
            // run so OpenType lookahead can't leave a pre-cursor span
            // visually empty while waiting for substitution candidates
            // to resolve (e.g. typing `paragraph` mid-row used to make
            // `grap` blank until `h` arrived). Skipped on grapheme
            // cells so emoji-ZWJ / combining-mark clusters stay whole
            // when the cursor lands on them.
            if !sq2.has_grapheme() {
                if let Some(cursor_x) = cursor_col_for_row {
                    let cursor_x = cursor_x as usize;
                    if run_start == cursor_x && end == run_start + 1 {
                        break;
                    }
                    if run_start < cursor_x && end == cursor_x {
                        break;
                    }
                }
            }
            let style2_id = sq2.style_id();
            if style2_id != prev_style_id {
                let f = (resolve_style(row_styles, end).flags.bits() & SHAPING_FLAG_MASK)
                    as u8;
                if f != run_style_flags {
                    break;
                }
                prev_style_id = style2_id;
            }
            let ch2 = sq2.c();
            let (font_id2, _) =
                rasterizer.resolve_font(ch2, run_style_flags, font_library, route_id);
            if font_id2 != font_id {
                break;
            }
            // Same placeholder→space substitution as the run-start
            // path above.
            let shape_ch2 = if ch2 == rio_backend::ansi::kitty_virtual::PLACEHOLDER {
                ' '
            } else {
                ch2
            };
            #[cfg(target_os = "macos")]
            {
                rasterizer
                    .run_cell_starts
                    .push(rasterizer.run_utf16_scratch.len() as u32);
                let mut buf = [0u16; 2];
                rasterizer
                    .run_utf16_scratch
                    .extend_from_slice(shape_ch2.encode_utf16(&mut buf));
            }
            #[cfg(not(target_os = "macos"))]
            {
                rasterizer
                    .run_cell_starts
                    .push(rasterizer.run_str_scratch.len() as u32);
                rasterizer.run_str_scratch.push(shape_ch2);
            }
            if ch2 != rio_backend::ansi::kitty_virtual::PLACEHOLDER {
                push_cluster_text(rasterizer, extras_table, sq2);
            }
            // Stamp the cell into the per-run hasher with its relative
            // cluster offset (`end - run_start`, captured *before* the
            // increment below).
            let cluster = (end - run_start) as u32;
            rasterizer.run_hasher.write_u32(shape_ch2 as u32);
            rasterizer.run_hasher.write_u32(cluster);
            hash_combining(rasterizer, extras_table, sq2, cluster);
            rasterizer.run_cell_columns.push(end as u16);
            end += 1;
        }

        // Finalize the per-run hash: append `(cell_count, font_id,
        // size_bucket)`. `style_flags` are not included separately
        // because `font_id` already varies with style (`resolve_font`
        // factors style_flags into the resolution key); adding them
        // a second time would just be redundant work.
        let cell_count = (end - run_start) as u32;
        rasterizer.run_hasher.write_u32(cell_count);
        rasterizer.run_hasher.write_u32(font_id);
        rasterizer.run_hasher.write_u16(size_bucket);
        let hash = rasterizer.run_hasher.finish();

        // Shape (cached) and capture ascent for this (font_id, size).
        let Some((ascent_px, _)) = shape_cached(
            rasterizer,
            hash,
            font_id,
            size_u16,
            size_bucket,
            font_library,
        ) else {
            x = end;
            continue;
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
        //
        // SmallVec inline capacity 64 covers terminal-typical runs
        // (ASCII identifiers, short bursts of non-ligature text)
        // entirely on the stack — no heap touch. Ligature-heavy or
        // shaped emoji runs that outgrow 64 slots spill to heap once.
        let mut glyph_emits: SmallVec<[(u16, u16); 64]> = SmallVec::new();
        {
            let glyphs = &run_cache_get(&mut rasterizer.run_cache, hash)
                .expect("just inserted")
                .glyphs;
            let mut cell_idx_in_run: u16 = 0;
            // Both platforms record explicit per-cell starts into the
            // shaping buffer (UTF-16 units on macOS, UTF-8 bytes on
            // swash), so one walk serves both. A per-char cursor would
            // miscount: cells with combining marks contribute several
            // chars each.
            let cell_starts = &rasterizer.run_cell_starts;
            for g in glyphs {
                while (cell_idx_in_run as usize + 1) < cell_starts.len()
                    && cell_starts[cell_idx_in_run as usize + 1] <= g.cluster
                {
                    cell_idx_in_run = cell_idx_in_run.saturating_add(1);
                }
                glyph_emits.push((g.id, cell_idx_in_run));
            }
        }

        for &(glyph_id, cell_idx_in_run) in &glyph_emits {
            // Map the appended-cell index back to its actual grid
            // column. Spacer cells were skipped from the run text so
            // `cell_idx_in_run` no longer equals `column - run_start`;
            // the parallel `run_cell_columns` table records the source
            // column for each appended cell.
            let grid_col = rasterizer
                .run_cell_columns
                .get(cell_idx_in_run as usize)
                .copied()
                .unwrap_or((run_start as u16).saturating_add(cell_idx_in_run));
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
            // ligatures take the first cluster cell's colour. Mapped
            // through `run_cell_columns` for the same reason as
            // `grid_col` above.
            let src_col = (grid_col as usize).min(cols.saturating_sub(1));
            let src_sq = row[Column(src_col)];
            let src_style = resolve_style(row_styles, src_col);
            let (atlas, color) = if is_color {
                // Colour glyphs (emoji) don't take the selection-fg /
                // hint-fg swap — behaviour for
                // bitmap/COLR atlas entries.
                (CellText::ATLAS_COLOR, [255, 255, 255, 255])
            } else if !needs_per_cell_check {
                // Fast path — no selection / color-changing hints on
                // this row.
                (
                    CellText::ATLAS_GRAYSCALE,
                    cell_fg(src_sq, src_style, palette, term_colors),
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
                        cell_fg_selected(src_sq, src_style, palette, term_colors),
                    )
                } else if let Some(tag) = hint_tag {
                    // Hint-fg wins over the cell's own fg, matching
                    // `.search` / `.search_selected` branches at
                    // `generic.zig:2829-2833` (the fg picker mirrors bg).
                    (CellText::ATLAS_GRAYSCALE, cell_fg_hinted(tag, palette))
                } else {
                    (
                        CellText::ATLAS_GRAYSCALE,
                        cell_fg(src_sq, src_style, palette, term_colors),
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
                page: slot.page,
                _pad: 0,
            });
        }

        x = end;
    }

    // Phase 2.5: composition pass. Every grapheme cluster shapes as
    // its own run — never ligating with the surrounding terminal text
    // — with the foreground forced to the terminal background and
    // BOOL_IS_CURSOR_GLYPH set, so the glyph reads inverted against
    // the cursor-colored block painted in `build_row_bg`.
    if let Some(pre) = preedit {
        let text_fg = normalized_to_u8(palette.named_colors().background.0);
        for col in pre.line.start_col..pre.line.end_col().min(cols) {
            let Some(PreeditCell::Start(cluster)) = pre.cell(col) else {
                continue;
            };
            let reserved_cells = if pre.cell(col + 1) == Some(PreeditCell::Continuation) {
                2
            } else {
                1
            };
            emit_preedit_cluster(
                pre.line.cluster(cluster),
                col as u16,
                y,
                reserved_cells,
                rasterizer,
                grid,
                font_library,
                route_id,
                size_u16,
                size_bucket,
                cell_w,
                cell_h,
                text_fg,
                fg_scratch,
            );
        }
    }

    // Phase 3: strikethrough pass. Emitted last so the strike overlays
    // the glyph.
    emit_strikethroughs(
        row,
        cols,
        y,
        row_styles,
        palette,
        term_colors,
        grid,
        cell_w_u32,
        cell_h_u32,
        thickness,
        row_sel,
        row_hints,
        preedit,
        glyph_registry.as_ref(),
        fg_scratch,
    );

    // Phase 4: the IME caret, topmost element of the composition.
    // On a composition cell it must not be a beam — cursor color on
    // the cursor-colored block is invisible — so it renders as a
    // thick underline in the text color instead; past the end of the
    // composition there is no block, so a beam in the cursor color
    // reads correctly there.
    if let Some(pre) = preedit {
        let caret = match pre.line.caret {
            PreeditCaret::OnCell(col) => Some((
                col,
                DecorationStyle::ImeCaretUnderline,
                normalized_to_u8(palette.named_colors().background.0),
            )),
            PreeditCaret::PastEnd(col) => {
                Some((col, DecorationStyle::ImeCaretBeam, pre.block_bg))
            }
            // The IME asked for no caret (candidate paging).
            PreeditCaret::Hidden => None,
        };
        if let Some((col, style, color)) = caret {
            if col < cols {
                emit_preedit_caret(
                    col as u16, y, grid, cell_w_u32, cell_h_u32, thickness, style, color,
                    fg_scratch,
                );
            }
            // A caret underline on a wide cluster covers both of its
            // cells; one cell would underline half the kanji.
            if matches!(style, DecorationStyle::ImeCaretUnderline)
                && pre.cell(col + 1) == Some(PreeditCell::Continuation)
                && col + 1 < cols
            {
                emit_preedit_caret(
                    (col + 1) as u16,
                    y,
                    grid,
                    cell_w_u32,
                    cell_h_u32,
                    thickness,
                    style,
                    color,
                    fg_scratch,
                );
            }
        }
    }
}

/// Fill the run scratch with `text`, hash it into the composition
/// cache namespace (the leading "PREE" sentinel keeps it disjoint from
/// the grid's per-cell keys), shape it (cached), and return
/// `(hash, ascent, summed advance)`. `None` means no shaping handle.
fn shape_preedit_text(
    rasterizer: &mut GridGlyphRasterizer,
    text: &str,
    font_id: u32,
    size_u16: u16,
    size_bucket: u16,
    font_library: &FontLibrary,
) -> Option<(u64, i16, f32)> {
    #[cfg(target_os = "macos")]
    {
        rasterizer.run_utf16_scratch.clear();
        rasterizer.run_cell_starts.clear();
        rasterizer
            .run_cell_starts
            .push(rasterizer.run_utf16_scratch.len() as u32);
        let mut buf = [0u16; 2];
        for ch in text.chars() {
            rasterizer
                .run_utf16_scratch
                .extend_from_slice(ch.encode_utf16(&mut buf));
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        rasterizer.run_str_scratch.clear();
        rasterizer.run_str_scratch.push_str(text);
    }

    rasterizer.run_hasher = rapidhash::fast::RapidHasher::default();
    rasterizer.run_hasher.write_u32(0x5052_4545); // "PREE"
    for (i, ch) in text.chars().enumerate() {
        rasterizer.run_hasher.write_u32(ch as u32);
        rasterizer.run_hasher.write_u32(i as u32);
    }
    rasterizer.run_hasher.write_u32(font_id);
    rasterizer.run_hasher.write_u16(size_bucket);
    let hash = rasterizer.run_hasher.finish();

    let (ascent_px, advance) = shape_cached(
        rasterizer,
        hash,
        font_id,
        size_u16,
        size_bucket,
        font_library,
    )?;
    Some((hash, ascent_px, advance))
}

/// Shape one grapheme cluster as a standalone run and emit its glyphs
/// at `(grid_col, y)` with a forced foreground color. Composition
/// cells always shape in the plain style: composing text shouldn't
/// inherit bold/italic from whatever prompt segment sat under the
/// cursor.
#[allow(clippy::too_many_arguments)]
fn emit_preedit_cluster(
    cluster: &str,
    grid_col: u16,
    y: u16,
    reserved_cells: usize,
    rasterizer: &mut GridGlyphRasterizer,
    grid: &mut GridRenderer,
    font_library: &FontLibrary,
    route_id: usize,
    size_u16: u16,
    size_bucket: u16,
    cell_w: f32,
    cell_h: f32,
    text_fg: [u8; 4],
    fg_scratch: &mut Vec<CellText>,
) {
    let Some(base) = cluster.chars().next() else {
        return;
    };
    let run_style_flags = 0u8;
    let (font_id, is_emoji) =
        rasterizer.resolve_font(base, run_style_flags, font_library, route_id);

    // Layout reserves 1 or 2 cells per cluster; shaping draws natural
    // width. Per-char width sums misjudge both directions (a ZWJ emoji
    // sums to 6 cells yet shapes to ~2, a conjunct sums to 2 yet can
    // ink 3), so the overflow decision uses the SHAPED advance: a full
    // cluster overflowing its reserved cells retries as the base char.
    // A base (or single) char that still overflows draws anyway:
    // hiding the character the user is actively composing is a worse
    // artifact than its transient spill next to the block. The
    // half-cell slack absorbs color-font advance quirks.
    let max_advance = reserved_cells as f32 * cell_w + cell_w * 0.5;
    let base_str = &cluster[..base.len_utf8()];
    let Some((hash, ascent_px, advance)) = shape_preedit_text(
        rasterizer,
        cluster,
        font_id,
        size_u16,
        size_bucket,
        font_library,
    ) else {
        return;
    };
    let (hash, ascent_px) = if advance <= max_advance || base_str.len() == cluster.len() {
        (hash, ascent_px)
    } else {
        let Some((hash, ascent_px, _)) = shape_preedit_text(
            rasterizer,
            base_str,
            font_id,
            size_u16,
            size_bucket,
            font_library,
        ) else {
            return;
        };
        (hash, ascent_px)
    };

    let (synthetic_bold, synthetic_italic) =
        rasterizer.get_synthesis(font_id, font_library);

    let mut glyph_ids: SmallVec<[u16; 4]> = SmallVec::new();
    {
        let glyphs = &run_cache_get(&mut rasterizer.run_cache, hash)
            .expect("just inserted")
            .glyphs;
        for g in glyphs {
            glyph_ids.push(g.id);
        }
    }

    for glyph_id in glyph_ids {
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
        let (atlas, color) = if is_color {
            (CellText::ATLAS_COLOR, [255, 255, 255, 255])
        } else {
            (CellText::ATLAS_GRAYSCALE, text_fg)
        };
        fg_scratch.push(CellText {
            glyph_pos: [slot.x as u32, slot.y as u32],
            glyph_size: [slot.w as u32, slot.h as u32],
            bearings: [slot.bearing_x, slot.bearing_y],
            grid_pos: [grid_col, y],
            color,
            atlas,
            // We computed the inverse foreground ourselves; the shader
            // must not swap it again on the forced-cursor cell.
            bools: CellText::BOOL_IS_CURSOR_GLYPH,
            page: slot.page,
            _pad: 0,
        });
    }
}

/// Emit the IME caret decoration sprite at `(col, y)`.
#[allow(clippy::too_many_arguments)]
fn emit_preedit_caret(
    col: u16,
    y: u16,
    grid: &mut GridRenderer,
    cell_w: u32,
    cell_h: u32,
    thickness: u32,
    style: DecorationStyle,
    color: [u8; 4],
    fg_scratch: &mut Vec<CellText>,
) {
    let Some(slot) = ensure_decoration_slot(grid, style, cell_w, cell_h, thickness)
    else {
        return;
    };
    if slot.w == 0 || slot.h == 0 {
        return;
    }
    fg_scratch.push(CellText {
        glyph_pos: [slot.x as u32, slot.y as u32],
        glyph_size: [slot.w as u32, slot.h as u32],
        bearings: [slot.bearing_x, slot.bearing_y],
        grid_pos: [col, y],
        color,
        atlas: CellText::ATLAS_GRAYSCALE,
        bools: CellText::BOOL_IS_CURSOR_GLYPH,
        page: slot.page,
        _pad: 0,
    });
}

/// Whether a registered custom glyph's render span reaches the
/// composition block from `col`: the fg pass drops such a glyph, so
/// its decorations must vanish with it. One predicate for both
/// decoration emitters; the span rule mirrors
/// `ensure_custom_glyph_by_codepoint`'s clamp to the protocol's 1..=2.
/// The `covers_ink(col, 2)` prefilter keeps the registry (an RwLock)
/// out of cells that are not next to the block, and callers run this
/// only after the cell is known to carry a decoration.
fn custom_glyph_ink_covered(
    pre: &PreeditRow<'_>,
    registry: Option<&rio_backend::sugarloaf::font::glyph_registry::GlyphRegistry>,
    sq: Square,
    col: usize,
) -> bool {
    let Some(registry) = registry else {
        return false;
    };
    pre.covers_ink(col, 2)
        && registry.get(sq.c() as u32).is_some_and(|entry| {
            pre.covers_ink(col, (entry.width as u16).clamp(1, 2) as usize)
        })
}

#[allow(clippy::too_many_arguments)]
fn emit_underlines<P: GridPalette>(
    row: &Row<Square>,
    cols: usize,
    y: u16,
    row_styles: &[Style],
    palette: &P,
    term_colors: &TermColors,
    grid: &mut GridRenderer,
    cell_w: u32,
    cell_h: u32,
    thickness: u32,
    row_sel: Option<RowSelection>,
    row_hints: &[RowHint],
    preedit: Option<&PreeditRow<'_>>,
    glyph_registry: Option<&rio_backend::sugarloaf::font::glyph_registry::GlyphRegistry>,
    fg_scratch: &mut Vec<CellText>,
) {
    for x in 0..cols {
        let sq = row[Column(x)];
        // Composing text takes no decorations from whatever sat under
        // it, and either half of a wide glyph the fg pass dropped must
        // not leave a floating decoration next to the block.
        if preedit.is_some_and(|p| p.suppresses(sq, x)) {
            continue;
        }
        let style = resolve_style(row_styles, x);
        let col = x as u16;
        // SGR underline (UNDER, double, curly, …) wins over the
        // hover-only forced underline. When the cell has no SGR
        // decoration but is inside a hovered hyperlink, emit a plain
        // single-line underline using the cell fg color — same shape
        // as hyperlink-hover affordance.
        let (deco, hover_force) = match underline_style_from_flags(style.flags) {
            Some(d) => (d, false),
            None if cell_in_hover_underline(row_hints, col) => {
                (DecorationStyle::Underline, true)
            }
            None => continue,
        };
        if preedit.is_some_and(|p| custom_glyph_ink_covered(p, glyph_registry, sq, x)) {
            continue;
        }
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
            cell_fg_selected(sq, style, palette, term_colors)
        } else if let Some(tag) = cell_in_row_hints(row_hints, col) {
            // Same reasoning as selection: underline inside a hint
            // should stay legible on the hint bg.
            cell_fg_hinted(tag, palette)
        } else if hover_force {
            // Hover-only forced underline: use the cell fg so the
            // underline tracks the hyperlink text color (matches
            // hyperlink hover affordance).
            cell_fg(sq, style, palette, term_colors)
        } else {
            decoration_color(sq, &style, palette, term_colors)
        };
        fg_scratch.push(CellText {
            glyph_pos: [slot.x as u32, slot.y as u32],
            glyph_size: [slot.w as u32, slot.h as u32],
            bearings: [slot.bearing_x, slot.bearing_y],
            grid_pos: [x as u16, y],
            color,
            atlas: CellText::ATLAS_GRAYSCALE,
            bools: 0,
            page: slot.page,
            _pad: 0,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_strikethroughs<P: GridPalette>(
    row: &Row<Square>,
    cols: usize,
    y: u16,
    row_styles: &[Style],
    palette: &P,
    term_colors: &TermColors,
    grid: &mut GridRenderer,
    cell_w: u32,
    cell_h: u32,
    thickness: u32,
    row_sel: Option<RowSelection>,
    row_hints: &[RowHint],
    preedit: Option<&PreeditRow<'_>>,
    glyph_registry: Option<&rio_backend::sugarloaf::font::glyph_registry::GlyphRegistry>,
    fg_scratch: &mut Vec<CellText>,
) {
    for x in 0..cols {
        let sq = row[Column(x)];
        // Composing text takes no decorations from whatever sat under
        // it, and either half of a wide glyph the fg pass dropped must
        // not leave a floating decoration next to the block.
        if preedit.is_some_and(|p| p.suppresses(sq, x)) {
            continue;
        }
        let style = resolve_style(row_styles, x);
        if !style.flags.contains(StyleFlags::STRIKEOUT) {
            continue;
        }
        if preedit.is_some_and(|p| custom_glyph_ink_covered(p, glyph_registry, sq, x)) {
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
        // a separate strike color, matching ).
        let color = if cell_in_row_sel(row_sel, col) {
            cell_fg_selected(sq, style, palette, term_colors)
        } else if let Some(tag) = cell_in_row_hints(row_hints, col) {
            cell_fg_hinted(tag, palette)
        } else {
            cell_fg(sq, style, palette, term_colors)
        };
        fg_scratch.push(CellText {
            glyph_pos: [slot.x as u32, slot.y as u32],
            glyph_size: [slot.w as u32, slot.h as u32],
            bearings: [slot.bearing_x, slot.bearing_y],
            grid_pos: [x as u16, y],
            color,
            atlas: CellText::ATLAS_GRAYSCALE,
            bools: 0,
            page: slot.page,
            _pad: 0,
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

/// Look up or rasterise a Glyph Protocol registration into the grid
/// atlas. The atlas key combines the codepoint with the registration's
/// `version` (bumped on every register/clear) so re-registering the
/// same codepoint never serves a stale rasterisation. Each unique
/// (codepoint × version × pixel size) combination owns one atlas slot;
/// previous-version slots become unreachable and the atlas LRU evicts
/// them in due course.
///
/// `ascent_px` matches the primary font's ascent at the same size
/// bucket — Glyph Protocol payloads have no font-of-their-own, so we
/// align registered glyphs to the surrounding text baseline. A more
/// faithful rendering would walk the registered outline's bbox to
/// compute per-glyph bearings, but for icon-style PUA glyphs the
/// primary-font baseline produces the expected appearance.
///
/// `registry` is the active terminal's glyph registry, cloned once
/// per row by `build_row_fg`. Passing it in (instead of going through
/// the `FontLibrary` write lock) keeps the per-cell hot loop allocation
/// and lock free.
///
/// Returns `None` when the registration was cleared between font
/// resolution and render, or when rasterisation produces no pixels
/// (zero-area outline, malformed COLR, etc.). On success the 4th tuple
/// element is the declared render span in cells (1 or 2) so the caller
/// can center the glyph across its overflow box.
#[allow(clippy::too_many_arguments)]
fn ensure_custom_glyph_by_codepoint(
    grid: &mut GridRenderer,
    registry: &rio_backend::sugarloaf::font::glyph_registry::GlyphRegistry,
    codepoint: u32,
    cell_w_u32: u32,
    cell_h: f32,
    foreground_rgba: [u8; 4],
) -> Option<(GlyphKey, AtlasSlot, bool, u16)> {
    use rio_backend::sugarloaf::font::glyph_registry::pack_atlas_glyph_id;

    // Fetch first so we know the registration's version + declared
    // width. The lookup happens under the registry's RwLock read; the
    // entry's payload is cloned out so the lock drops before tiny-skia.
    let entry = registry.get(codepoint)?;

    // The declared `width` is a render hint: the glyph rasterises into a
    // `span × cell_w` box and overflows rightward in pixels. The grid
    // still treats the codepoint as one logical column (no cell was
    // reserved), so this is purely visual. Clamp to the protocol's 1..=2.
    let span = (entry.width as u16).clamp(1, 2);
    let span_w_px = (cell_w_u32 * span as u32).min(u16::MAX as u32) as u16;
    let cell_h_px = cell_h.round().clamp(0.0, u16::MAX as f32) as u16;
    let cell_w_px = cell_w_u32.min(u16::MAX as u32) as u16;
    // The cached slot is now a pure function of (cp, version, upm,
    // scale) — the bitmap and its raster-intrinsic bearings depend only
    // on the scale `min(span_w, cell_h)/upm`, and `upm`/version ride in
    // `glyph_id`. So the key only has to capture the geometry that feeds
    // the scale: cell_w, cell_h and the 1-vs-2-cell span. Pack them the
    // same way the cursor sprite key does (span in bit 15, low cell_w
    // bits in 12..15, cell_h in the low 12) so a line-height-only change
    // re-keys instead of serving a stale raster. (Within a font family
    // cell_h pins cell_w, so the 3 low cell_w bits are ample.) The font
    // ascent is no longer part of the slot — it's applied per-emit — so
    // it can't alias here.
    let key = GlyphKey {
        font_id: CUSTOM_GLYPH_FONT_ID_U32,
        glyph_id: pack_atlas_glyph_id(codepoint, entry.version),
        size_bucket: (((span >= 2) as u16) << 15)
            | ((cell_w_px & 0x7) << 12)
            | (cell_h_px & 0xFFF),
    };
    if let Some(slot) = grid.lookup_glyph(key) {
        return Some((key, slot, false, span));
    }
    if let Some(slot) = grid.lookup_glyph_color(key) {
        return Some((key, slot, true, span));
    }

    let raster = rio_backend::sugarloaf::glyph_protocol::rasterize_payload(
        &entry.payload,
        entry.upm,
        span_w_px,
        cell_h_px,
        foreground_rgba,
    )?;

    // Bearings are NOT baked into the cached slot: the caller centers
    // the glyph in its cell box from `slot.w/h` + the cell metrics at
    // emit time, so placement always tracks the current geometry and
    // the slot stays a pure (cp, version, scale) → bitmap mapping.
    let raster_in = RasterizedGlyph {
        width: raster.width,
        height: raster.height,
        bearing_x: 0,
        bearing_y: 0,
        bytes: &raster.data,
    };

    let slot = if raster.is_color {
        grid.insert_glyph_color(key, raster_in)?
    } else {
        grid.insert_glyph(key, raster_in)?
    };
    Some((key, slot, raster.is_color, span))
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
            Render, Source,
        },
        zeno::{Angle, Format, Transform},
        FontRef, Setting,
    };

    let font_entry = rasterizer.font_data_cache.get(&font_id)?.clone();
    let font_ref = FontRef {
        data: font_entry.0.as_ref(),
        offset: font_entry.1,
        key: font_entry.2,
    };

    // Shaping runs before rasterization, so `lib_settings` and
    // `wght_cache` are already populated for this font.
    let hinting = rasterizer
        .lib_settings
        .as_ref()
        .map(|s| s.0)
        .unwrap_or(true);
    const WGHT_TAG: u32 = u32::from_be_bytes(*b"wght");
    let wght_var = rasterizer
        .wght_cache
        .get(&font_id)
        .copied()
        .flatten()
        .map(|v| Setting {
            tag: WGHT_TAG,
            value: v,
        });
    let mut scaler = rasterizer
        .scale_ctx
        .builder(font_ref)
        .hint(hinting)
        .size(size_u16 as f32)
        .variations(wght_var.iter().copied())
        .build();

    let sources: &[Source] = match rio_backend::sugarloaf::font::select_color_bitmap(
        font_ref,
        glyph_id,
        size_u16 as f32,
    ) {
        Some(bitmap) => &[Source::ColorOutline(0), bitmap, Source::Outline],
        None => &[Source::ColorOutline(0), Source::Outline],
    };
    let mut image = GlyphImage::new();
    let embolden_amount = if synthetic_bold {
        (size_u16 as f32 / 14.0).max(1.0)
    } else {
        0.0
    };
    let ok = Render::new(sources)
        .format(Format::Alpha)
        .embolden(embolden_amount)
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
    rio_backend::sugarloaf::font::normalize_color_bitmap(&mut image);
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

#[cfg(test)]
mod hint_label_tests {
    use super::*;

    fn label(row: i32, col: usize, ch: char, is_first: bool) -> HintLabel {
        HintLabel {
            position: Pos::new(Line(row), Column(col)),
            label: ch,
            is_first,
        }
    }

    #[test]
    fn hint_label_styles_are_bold_badges() {
        use rio_backend::config::colors::ColorRgb;
        let fg = [0.1, 0.1, 0.1, 1.0];
        let bg = [1.0, 0.5, 0.0, 1.0];
        let (first, rest) = hint_label_styles(fg, bg);
        assert!(first.flags.contains(StyleFlags::BOLD));
        assert!(rest.flags.contains(StyleFlags::BOLD));
        assert_eq!(first.bg, AnsiColor::Spec(ColorRgb::from_color_arr(bg)));
        assert_eq!(
            rest.bg,
            AnsiColor::Spec(ColorRgb::from_color_arr([0.8, 0.4, 0.0, 1.0]))
        );
    }

    #[test]
    fn overlay_substitutes_label_squares_on_matching_row_only() {
        let fg = [0.1, 0.1, 0.1, 1.0];
        let bg = [1.0, 0.5, 0.0, 1.0];
        let (first_style, rest_style) = hint_label_styles(fg, bg);
        let pair = (first_style, rest_style);
        let row: Row<Square> = Row::new(10);
        let row_styles = vec![Style::default(); 10];
        let labels = [label(3, 2, 'j', true), label(3, 3, 'f', false)];
        let mut hints = Vec::new();

        assert!(
            overlay_hint_labels(&row, &row_styles, &labels, 2, 0, pair, &mut hints)
                .is_none()
        );
        assert!(hints.is_empty());

        let (overlaid, styles) =
            overlay_hint_labels(&row, &row_styles, &labels, 3, 0, pair, &mut hints)
                .unwrap();
        assert_eq!(overlaid[Column(2)].c(), 'j');
        assert_eq!(styles[2], first_style);
        assert_eq!(overlaid[Column(3)].c(), 'f');
        assert_eq!(styles[3], rest_style);
        assert_eq!(overlaid[Column(4)].c(), row[Column(4)].c());
        assert_eq!(styles[4], Style::default());
        assert_eq!(hints.len(), 2);
        assert!(hints.iter().all(|h| h.tag == HintTag::Label));
        assert_eq!(cell_in_row_hints(&hints, 2), Some(HintTag::Label));
        assert_eq!(cell_in_row_hints(&hints, 3), Some(HintTag::Label));

        let mut hints = vec![RowHint {
            lo: 0,
            hi: 9,
            tag: HintTag::Match,
        }];
        overlay_hint_labels(&row, &row_styles, &labels, 3, 0, pair, &mut hints).unwrap();
        assert_eq!(cell_in_row_hints(&hints, 2), Some(HintTag::Label));
        assert_eq!(cell_in_row_hints(&hints, 5), Some(HintTag::Match));

        let mut hints = Vec::new();
        assert!(
            overlay_hint_labels(&row, &row_styles, &labels, 3, 2, pair, &mut hints)
                .is_none()
        );
        assert!(
            overlay_hint_labels(&row, &row_styles, &labels, 5, 2, pair, &mut hints)
                .is_some()
        );

        let oob = [label(3, 99, 'x', true)];
        let mut hints = Vec::new();
        assert!(
            overlay_hint_labels(&row, &row_styles, &oob, 3, 0, pair, &mut hints)
                .is_none()
        );
        assert!(hints.is_empty());
    }
}

#[cfg(test)]
mod preedit_suppression_tests {
    use super::*;
    use preedit::{PreeditCursor, PreeditLine};

    /// Shape `text` through the REAL production path
    /// (`shape_preedit_text`, the fn `emit_preedit_cluster` calls) and
    /// return the summed advance: exactly the quantity the composed-
    /// cluster overflow fallback keys on, from the same code.
    fn shaped_advance(
        r: &mut GridGlyphRasterizer,
        lib: &FontLibrary,
        text: &str,
        size: u16,
    ) -> Option<f32> {
        let base = text.chars().next()?;
        let (font_id, _) = r.resolve_font(base, 0, lib, 0);
        shape_preedit_text(r, text, font_id, size, size, lib)
            .map(|(_, _, advance)| advance)
    }

    /// A ZWJ emoji must fit its 2 reserved cells under the shaped-
    /// advance overflow rule, or IME candidate selection would degrade
    /// it to the base person glyph. Per-char width sums say 6 cells;
    /// the shaped advance is the truth this pins.
    #[test]
    fn zwj_emoji_shaped_advance_fits_reserved_cells() {
        let font_library = FontLibrary::default();
        let mut r = GridGlyphRasterizer::new();
        let size: u16 = 28;
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
        let Some(family_advance) = shaped_advance(&mut r, &font_library, family, size)
        else {
            // No shaping handle for this font in the environment:
            // nothing to measure.
            return;
        };
        assert!(family_advance > 0.0);

        // In a monospace font every narrow advance IS the cell width.
        // Skip like the emoji guard above when the primary font has no
        // shaping handle in this environment.
        let Some(cell_w) = shaped_advance(&mut r, &font_library, "m", size) else {
            return;
        };
        // Reserved 2 cells + the fallback's half-cell slack.
        let max_advance = 2.0 * cell_w + cell_w * 0.5;
        // Strict only where CI ships a real emoji font (Apple Color
        // Emoji); a fontless Linux container may shape to notdef with
        // arbitrary metrics.
        #[cfg(target_os = "macos")]
        assert!(
            family_advance <= max_advance,
            "family emoji advance {family_advance} exceeds {max_advance}: \
             the IME composition would degrade it to the base glyph"
        );
        #[cfg(not(target_os = "macos"))]
        let _ = max_advance;
    }

    /// `covers_ink` is the one predicate every fg emitter consults to
    /// drop ink that would land on the composition block (wide glyphs
    /// whose spacer sits under it, custom glyphs whose render span
    /// reaches into it).
    #[test]
    fn covers_ink_spans() {
        // Block occupies columns 4..8 ("日本" at cursor col 4).
        let line = PreeditLine::new("日本", PreeditCursor::Byte(6), 0, 4, 80).unwrap();
        let pre = PreeditRow {
            line: &line,
            block_bg: [0; 4],
        };
        // One-cell ink left of the block never triggers.
        assert!(!pre.covers_ink(3, 1));
        // Two-cell ink at column 3 reaches the block's first cell.
        assert!(pre.covers_ink(3, 2));
        // Inside the block.
        assert!(pre.covers_ink(5, 1));
        // Ink starting at the block's end is clear of it.
        assert!(!pre.covers_ink(8, 2));
    }

    /// A wide pair half-covered by the block suppresses BOTH halves,
    /// whichever half the block touches: glyphs and decorations vanish
    /// together, never a floating underline under an empty half-cell.
    #[test]
    fn suppresses_covers_half_covered_wide_pairs() {
        // Block occupies columns 4..8 ("日本" at cursor col 4).
        let line = PreeditLine::new("日本", PreeditCursor::Byte(6), 0, 4, 80).unwrap();
        let pre = PreeditRow {
            line: &line,
            block_bg: [0; 4],
        };
        let narrow = Square::default();
        let mut wide = Square::default();
        wide.set_wide(Wide::Wide);
        let mut spacer = Square::default();
        spacer.set_wide(Wide::Spacer);

        // Plain cells: only covered columns suppress.
        assert!(!pre.suppresses(narrow, 3));
        assert!(pre.suppresses(narrow, 4));
        // Wide base at 3: its spacer at 4 is under the block.
        assert!(pre.suppresses(wide, 3));
        assert!(!pre.suppresses(wide, 1));
        // Spacer at 8: its base at 7 is under the block.
        assert!(pre.suppresses(spacer, 8));
        assert!(!pre.suppresses(spacer, 9));
        // Column 0 spacer never underflows.
        assert!(!pre.suppresses(spacer, 0));
    }
}

#[cfg(test)]
mod cluster_text_tests {
    use super::*;

    /// The shaping buffer must receive attached combining marks (so
    /// the shaper can compose them) but never variation selectors,
    /// whose effect is already resolved into the font choice.
    #[test]
    fn push_cluster_chars_appends_marks_and_skips_selectors() {
        let mut r = GridGlyphRasterizer::new();

        #[cfg(target_os = "macos")]
        {
            let mut buf = [0u16; 2];
            r.run_utf16_scratch
                .extend_from_slice('e'.encode_utf16(&mut buf));
            push_cluster_chars(&mut r, &['\u{301}', '\u{FE0F}', '\u{302}']);
            let s = String::from_utf16(&r.run_utf16_scratch).unwrap();
            assert_eq!(s, "e\u{301}\u{302}");
        }
        #[cfg(not(target_os = "macos"))]
        {
            r.run_str_scratch.push('e');
            push_cluster_chars(&mut r, &['\u{301}', '\u{FE0F}', '\u{302}']);
            assert_eq!(r.run_str_scratch, "e\u{301}\u{302}");
        }
    }

    /// Cell-start bookkeeping: with marks in the buffer, per-cell
    /// starts (not a per-char cursor) must map shaped clusters back to
    /// cells. Simulates a run of `e`+U+0301 then `x`: a glyph whose
    /// cluster points at the mark still belongs to cell 0, and `x`'s
    /// glyph to cell 1.
    #[test]
    fn cell_starts_attribute_marked_cells_correctly() {
        // Buffer layout (UTF-8 bytes): e=0, U+0301=1..3, x=3.
        let cell_starts: Vec<u32> = vec![0, 3];
        let clusters = [0u32, 1, 3];
        let mut cell_idx: u16 = 0;
        let mut out = Vec::new();
        for g in clusters {
            while (cell_idx as usize + 1) < cell_starts.len()
                && cell_starts[cell_idx as usize + 1] <= g
            {
                cell_idx = cell_idx.saturating_add(1);
            }
            out.push(cell_idx);
        }
        assert_eq!(out, [0, 0, 1]);
    }
}
