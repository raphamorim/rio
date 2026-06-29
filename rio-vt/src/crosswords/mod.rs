/*
    Crosswords -> Rio's grid manager

    |----------------------------------|
    |-$-bash:-echo-1-------------------|
    |-1--------------------------------|
    |----------------------------------|
    |----------------------------------|
    |----------------------------------|
    |----------------------------------|
    |----------------------------------|

// Crosswords (mod.rs) was originally taken from https://github.com/alacritty/alacritty/blob/e35e5ad14fce8456afdd89f2b392b9924bb27471/alacritty_terminal/src/term/mod.rs
// which is licensed under Apache 2.0 license.
*/

pub mod attr;
pub mod formatter;
pub mod grid;
pub mod pos;
pub mod search;
pub mod square;
pub mod style;
pub mod vi_mode;

use crate::ansi::graphics::Graphics;
use crate::ansi::graphics::KittyPlacement;
use crate::ansi::graphics::UpdateQueues;
use crate::ansi::mode::NamedMode;
use crate::ansi::mode::NamedPrivateMode;
use crate::ansi::mode::PrivateMode;
use crate::ansi::sixel;
use crate::ansi::{
    mode::Mode as AnsiMode, ClearMode, CursorShape, KeyboardModes,
    KeyboardModesApplyBehavior, LineClearMode, TabulationClearMode,
};
use crate::clipboard::ClipboardType;
use crate::config::colors::{self, ColorRgb};
use crate::crosswords::colors::term::TermColors;
use crate::crosswords::grid::{Dimensions, Grid, GridSquare, Scroll};
use crate::crosswords::square::{CellFlags, Wide};
use crate::event::WindowId;
use crate::event::{EventListener, RioEvent, TerminalDamage};
use crate::performer::handler::Handler;
use crate::performer::parser::Params;
use crate::selection::{Selection, SelectionRange, SelectionType};
use crate::simd_utf8;
use attr::*;
use base64::{engine::general_purpose, Engine as _};
use bitflags::bitflags;
use grid::row::Row;
use pos::{
    Boundary, CharsetIndex, Column, Cursor, CursorState, Direction, Line, Pos, Side,
};
use rio_graphics::{GraphicData, MAX_GRAPHIC_DIMENSIONS};
use square::{Hyperlink, LineLength, Square};
use std::mem;
use std::ops::{Index, IndexMut, Range};
use std::option::Option;
use std::ptr;
use std::sync::Arc;
use tracing::{debug, info, trace, warn};
use vi_mode::{ViModeCursor, ViMotion};

pub type NamedColor = colors::NamedColor;

pub const MIN_COLUMNS: usize = 2;
pub const MIN_LINES: usize = 1;

// Max. number of graphics stored in a single cell.
// const MAX_GRAPHICS_PER_CELL: usize = 20;

bitflags! {
     #[derive(Debug, Copy, Clone)]
     pub struct Mode: u32 {
        const NONE                    = 0;
        const SHOW_CURSOR             = 1;
        const APP_CURSOR              = 1 << 1;
        const APP_KEYPAD              = 1 << 2;
        const MOUSE_REPORT_CLICK      = 1 << 3;
        const BRACKETED_PASTE         = 1 << 4;
        const SGR_MOUSE               = 1 << 5;
        const MOUSE_MOTION            = 1 << 6;
        const LINE_WRAP               = 1 << 7;
        const LINE_FEED_NEW_LINE      = 1 << 8;
        const ORIGIN                  = 1 << 9;
        const INSERT                  = 1 << 10;
        const FOCUS_IN_OUT            = 1 << 11;
        const ALT_SCREEN              = 1 << 12;
        const MOUSE_DRAG              = 1 << 13;
        const UTF8_MOUSE              = 1 << 14;
        const ALTERNATE_SCROLL        = 1 << 15;
        const VI                      = 1 << 16;
        const URGENCY_HINTS           = 1 << 17;
        const DISAMBIGUATE_ESC_CODES  = 1 << 18;
        const REPORT_EVENT_TYPES      = 1 << 19;
        const REPORT_ALTERNATE_KEYS   = 1 << 20;
        const REPORT_ALL_KEYS_AS_ESC  = 1 << 21;
        const REPORT_ASSOCIATED_TEXT  = 1 << 22;
        const COLOR_SCHEME_UPDATES    = 1 << 23;
        /// DEC private mode 2027: grapheme cluster processing. Shared
        /// across main/alt screens (the mode lives on the terminal,
        /// not the grid), matching contour.
        const GRAPHEME_CLUSTER        = 1 << 24;
        const MOUSE_REPORT_X10        = 1 << 25;
        const MOUSE_MODE = Self::MOUSE_REPORT_CLICK.bits() | Self::MOUSE_MOTION.bits() | Self::MOUSE_DRAG.bits() | Self::MOUSE_REPORT_X10.bits();
        const KITTY_KEYBOARD_PROTOCOL = Self::DISAMBIGUATE_ESC_CODES.bits()
                                      | Self::REPORT_EVENT_TYPES.bits()
                                      | Self::REPORT_ALTERNATE_KEYS.bits()
                                      | Self::REPORT_ALL_KEYS_AS_ESC.bits()
                                      | Self::REPORT_ASSOCIATED_TEXT.bits();
        const ANY                    = u32::MAX;

        const SIXEL_DISPLAY             = 1 << 28;
        const SIXEL_PRIV_PALETTE        = 1 << 29;
        const SIXEL_CURSOR_TO_THE_RIGHT = 1 << 31;
    }
}

/// The state of the [`Mode`] and [`PrivateMode`].
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
enum ModeState {
    /// The mode is not supported.
    NotSupported = 0,
    /// The mode is currently set.
    Set = 1,
    /// The mode is currently not set.
    Reset = 2,
}

impl From<bool> for ModeState {
    fn from(value: bool) -> Self {
        if value {
            Self::Set
        } else {
            Self::Reset
        }
    }
}

impl Default for Mode {
    fn default() -> Mode {
        Mode::SHOW_CURSOR
            | Mode::LINE_WRAP
            | Mode::ALTERNATE_SCROLL
            | Mode::URGENCY_HINTS
            | Mode::SIXEL_PRIV_PALETTE
    }
}

impl From<KeyboardModes> for Mode {
    fn from(value: KeyboardModes) -> Self {
        let mut mode = Self::empty();
        mode.set(
            Mode::DISAMBIGUATE_ESC_CODES,
            value.contains(KeyboardModes::DISAMBIGUATE_ESC_CODES),
        );
        mode.set(
            Mode::REPORT_EVENT_TYPES,
            value.contains(KeyboardModes::REPORT_EVENT_TYPES),
        );
        mode.set(
            Mode::REPORT_ALTERNATE_KEYS,
            value.contains(KeyboardModes::REPORT_ALTERNATE_KEYS),
        );
        mode.set(
            Mode::REPORT_ALL_KEYS_AS_ESC,
            value.contains(KeyboardModes::REPORT_ALL_KEYS_AS_ESC),
        );
        mode.set(
            Mode::REPORT_ASSOCIATED_TEXT,
            value.contains(KeyboardModes::REPORT_ASSOCIATED_TEXT),
        );
        mode
    }
}

/// Terminal damage information collected since the last [`Term::reset_damage`] call.
#[derive(Debug)]
pub enum TermDamage<'a> {
    /// The entire terminal is damaged.
    Full,

    /// Iterator over damaged lines in the terminal.
    Partial(TermDamageIterator<'a>),
}

/// Iterator over the terminal's viewport damaged lines.
#[derive(Clone, Debug)]
pub struct TermDamageIterator<'a> {
    line_damage: std::slice::Iter<'a, LineDamage>,
    display_offset: usize,
}

impl<'a> TermDamageIterator<'a> {
    pub fn new(line_damage: &'a [LineDamage], display_offset: usize) -> Self {
        let num_lines = line_damage.len();
        // Filter out invisible damage.
        let line_damage = &line_damage[..num_lines.saturating_sub(display_offset)];
        Self {
            display_offset,
            line_damage: line_damage.iter(),
        }
    }
}

impl Iterator for TermDamageIterator<'_> {
    type Item = LineDamage;

    fn next(&mut self) -> Option<Self::Item> {
        self.line_damage.find_map(|line| {
            line.is_damaged()
                .then_some(LineDamage::new(line.line + self.display_offset, true))
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LineDamage {
    /// Line number.
    pub line: usize,
    /// Whether this line is damaged.
    pub damaged: bool,
}

impl LineDamage {
    #[inline]
    pub fn new(line: usize, damaged: bool) -> Self {
        Self { line, damaged }
    }

    #[inline]
    pub fn undamaged(line: usize) -> Self {
        Self {
            line,
            damaged: false,
        }
    }

    #[inline]
    pub fn reset(&mut self) {
        self.damaged = false;
    }

    #[inline]
    pub fn is_damaged(&self) -> bool {
        self.damaged
    }

    #[inline]
    pub fn mark_damaged(&mut self) {
        self.damaged = true;
    }
}

#[derive(Debug, Clone)]
struct TermDamageState {
    /// Hint whether terminal should be damaged entirely regardless of the actual damage changes.
    full: bool,

    /// Information about damage on terminal lines.
    lines: Vec<LineDamage>,

    /// Old terminal cursor point.
    last_cursor: Pos,

    /// Last Vi cursor point.
    last_vi_cursor_point: Option<Pos>,
    // Old selection range.
    last_selection: Option<SelectionRange>,
}

impl TermDamageState {
    fn new(num_lines: usize) -> Self {
        let lines = (0..num_lines).map(LineDamage::undamaged).collect();

        Self {
            full: true,
            lines,
            last_cursor: Default::default(),
            last_vi_cursor_point: Default::default(),
            last_selection: Default::default(),
        }
    }

    #[inline]
    fn resize(&mut self, num_lines: usize) {
        // Reset point, so old cursor won't end up outside of the viewport.
        self.last_cursor = Default::default();
        self.last_vi_cursor_point = None;
        self.last_selection = None;
        self.full = true;

        self.lines.clear();
        self.lines.reserve(num_lines);
        for line in 0..num_lines {
            self.lines.push(LineDamage::undamaged(line));
        }
    }

    /// Damage a line
    #[inline]
    fn damage_line(&mut self, line: usize) {
        self.lines[line].mark_damaged();
    }

    fn damage_selection(&mut self, selection: SelectionRange, display_offset: usize) {
        let display_offset = display_offset as i32;
        let last_visible_line = self.lines.len() as i32 - 1;

        // Don't damage invisible selection.
        if selection.end.row.0 + display_offset < 0
            || selection.start.row.0.abs() < display_offset - last_visible_line
        {
            return;
        };

        let start = std::cmp::max(selection.start.row.0 + display_offset, 0);
        let end = (selection.end.row.0 + display_offset).clamp(0, last_visible_line);
        for line in start as usize..=end as usize {
            self.damage_line(line);
        }
    }

    /// Reset information about terminal damage.
    fn reset(&mut self) {
        self.full = false;
        self.lines.iter_mut().for_each(|line| line.reset());
    }
}

#[derive(Debug, Clone)]
struct TabStops {
    tabs: Vec<bool>,
}

/// Default tab interval, corresponding to terminfo `it` value.
const INITIAL_TABSTOPS: usize = 8;

impl TabStops {
    #[inline]
    fn new(columns: usize) -> TabStops {
        TabStops {
            tabs: (0..columns).map(|i| i % INITIAL_TABSTOPS == 0).collect(),
        }
    }

    /// Remove all tabstops.
    #[inline]
    fn clear_all(&mut self) {
        unsafe {
            ptr::write_bytes(self.tabs.as_mut_ptr(), 0, self.tabs.len());
        }
    }

    /// Increase tabstop capacity.
    #[inline]
    fn resize(&mut self, columns: usize) {
        let mut index = self.tabs.len();
        self.tabs.resize_with(columns, || {
            let is_tabstop = index.is_multiple_of(INITIAL_TABSTOPS);
            index += 1;
            is_tabstop
        });
    }
}

impl Index<Column> for TabStops {
    type Output = bool;

    fn index(&self, index: Column) -> &bool {
        &self.tabs[index.0]
    }
}

impl IndexMut<Column> for TabStops {
    fn index_mut(&mut self, index: Column) -> &mut bool {
        self.tabs.index_mut(index.0)
    }
}

/// Terminal version for escape sequence reports.
///
/// This returns the current terminal version as a unique number based on rio's
/// semver version. The different versions are padded to ensure that a higher semver version will
/// always report a higher version number.
fn version_number(mut version: &str) -> usize {
    if let Some(separator) = version.rfind('-') {
        version = &version[..separator];
    }

    let mut version_number = 0;

    let semver_versions = version.split('.');
    for (i, semver_version) in semver_versions.rev().enumerate() {
        let semver_number = semver_version.parse::<usize>().unwrap_or(0);
        version_number += usize::pow(100, i as u32) * semver_number;
    }

    version_number
}

/// True when (`base`, `vs`) appears in Unicode's `emoji-variation-sequences.txt`,
/// i.e. `vs` (U+FE0F or U+FE0E) is defined to have an effect on this base.
/// Equivalent to kitty's `is_emoji_presentation_base` guard — the
/// actual widen/narrow decision is then
/// gated on the current cell's `Wide` state by the callers.
pub(crate) fn vs_is_valid_base(base: char, vs: char) -> bool {
    use rio_grapheme_width::emoji::Presentation;
    let mut buf = [0u8; 8];
    let n1 = base.encode_utf8(&mut buf).len();
    let n2 = vs.encode_utf8(&mut buf[n1..]).len();
    // SAFETY: two valid chars written consecutively remain valid UTF-8.
    let s = unsafe { core::str::from_utf8_unchecked(&buf[..n1 + n2]) };
    // `for_grapheme` returns `Some(explicit)` iff the sequence is in
    // VARIATION_MAP (forked from wezterm's emoji-variation-sequences.txt).
    Presentation::for_grapheme(s).1.is_some()
}

// Max size of the window title stack.
const TITLE_STACK_MAX_DEPTH: usize = 4096;

// Max size of the keyboard modes.
const KEYBOARD_MODE_STACK_MAX_DEPTH: usize = 8;

#[derive(Debug)]
pub struct Crosswords<U>
where
    U: EventListener,
{
    active_charset: CharsetIndex,
    mode: Mode,
    pub vi_mode_cursor: ViModeCursor,
    semantic_escape_chars: String,
    pub grid: Grid<Square>,
    inactive_grid: Grid<Square>,
    scroll_region: Range<Line>,
    tabs: TabStops,
    event_proxy: U,
    pub selection: Option<Selection>,
    pub colors: TermColors,
    pub title: String,
    damage: TermDamageState,
    pub graphics: Graphics,
    /// Per-session registry of glyphs registered over Glyph Protocol
    /// (APC `25a1`). `None` until a program in this session actually
    /// uses the protocol, so terminals that never see a Glyph
    /// Protocol message pay zero cost — no Arc allocation, no
    /// per-frame attach call. Lazily initialised by `glyph_register`.
    /// Two tabs can register conflicting glyphs for the same
    /// codepoint; each `Crosswords` owns its own registry once
    /// initialised.
    #[cfg(feature = "graphics")]
    pub glyph_registry: Option<rio_graphics::glyph::glyph_registry::GlyphRegistry>,
    pub cursor_shape: CursorShape,
    pub default_cursor_shape: CursorShape,
    pub blinking_cursor: bool,
    pub window_id: WindowId,
    pub route_id: usize,
    title_stack: Vec<String>,
    pub current_directory: Option<std::path::PathBuf>,
    /// Shell state from `OSC 1337 ; SetUserVar` (iTerm2 style).
    pub user_vars: rustc_hash::FxHashMap<String, String>,

    /// Whether a `TerminalDamaged` event is already in flight to the renderer.
    /// Set by PTY thread before sending; cleared by renderer after extracting damage.
    pub damage_event_in_flight: bool,

    /// What DECRQM callers see after a full reset: the
    /// consumer-configured default for grapheme clustering (mode
    /// 2027). On by default; embedders serving legacy wcwidth
    /// consumers flip it
    /// via [`set_grapheme_clustering`](Self::set_grapheme_clustering).
    grapheme_clustering_default: bool,

    // The stack for the keyboard modes.
    modify_other_keys: u8,
    keyboard_mode_stack: [u8; KEYBOARD_MODE_STACK_MAX_DEPTH],
    keyboard_mode_idx: usize,
    inactive_keyboard_mode_stack: [u8; KEYBOARD_MODE_STACK_MAX_DEPTH],
    inactive_keyboard_mode_idx: usize,

    /// Last-known color scheme polarity, kept current by the frontend on
    /// every config update. Answers the DSR `CSI ? 996 n` query.
    color_scheme_is_dark: bool,
}

impl<U: EventListener> Crosswords<U> {
    pub fn new<D: Dimensions>(
        dimensions: D,
        cursor_shape: CursorShape,
        event_proxy: U,
        window_id: WindowId,
        route_id: usize,
        scrollback_history_limit: usize,
    ) -> Crosswords<U> {
        let cols = dimensions.columns();
        let rows = dimensions.screen_lines();
        let grid = Grid::new(rows, cols, scrollback_history_limit);
        let alt = Grid::new(rows, cols, 0);

        let scroll_region = Line(0)..Line(rows as i32);
        let semantic_escape_chars = String::from(",│`|:\"' ()[]{}<>\t\0");
        let term_colors = TermColors::default();
        // Regex used for the default URL hint.
        let _url_regex: &str = "(ipfs:|ipns:|magnet:|mailto:|gemini://|gopher://|https://|http://|news:|file:|git://|ssh:|ftp://)\
                         [^\u{0000}-\u{001F}\u{007F}-\u{009F}<>\"\\s{-}\\^⟨⟩`\\\\]+";

        Crosswords {
            vi_mode_cursor: ViModeCursor::new(grid.cursor.pos),
            semantic_escape_chars,
            selection: None,
            grid,
            inactive_grid: alt,
            active_charset: CharsetIndex::default(),
            scroll_region,
            event_proxy,
            colors: term_colors,
            title: String::from(""),
            tabs: TabStops::new(cols),
            mode: Mode::SHOW_CURSOR
                | Mode::LINE_WRAP
                | Mode::ALTERNATE_SCROLL
                | Mode::URGENCY_HINTS
                | Mode::GRAPHEME_CLUSTER,
            grapheme_clustering_default: true,
            damage: TermDamageState::new(rows),
            graphics: Graphics::new(&dimensions),
            #[cfg(feature = "graphics")]
            glyph_registry: None,
            default_cursor_shape: cursor_shape,
            cursor_shape,
            blinking_cursor: false,
            window_id,
            route_id,
            title_stack: Default::default(),
            current_directory: None,
            user_vars: rustc_hash::FxHashMap::default(),
            damage_event_in_flight: false,
            modify_other_keys: 0,
            keyboard_mode_stack: Default::default(),
            keyboard_mode_idx: 0,
            inactive_keyboard_mode_stack: Default::default(),
            inactive_keyboard_mode_idx: 0,
            color_scheme_is_dark: true,
        }
    }

    /// Configure the default for grapheme cluster processing (DEC
    /// private mode 2027): applied immediately and restored by RIS. A
    /// program's DECSET/DECRST still wins at runtime. Defaults to
    /// enabled; consumers whose renderers or downstream protocols
    /// assume legacy wcwidth cell layout can turn it off.
    pub fn set_grapheme_clustering(&mut self, enabled: bool) {
        self.grapheme_clustering_default = enabled;
        self.mode.set(Mode::GRAPHEME_CLUSTER, enabled);
    }

    pub fn mark_fully_damaged(&mut self) {
        // Only emit event if we weren't already fully damaged
        let was_damaged = self.damage.full;
        self.damage.full = true;

        // Request a render to display the damage
        if !was_damaged {
            self.event_proxy
                .send_event(RioEvent::RenderRoute(self.route_id), self.window_id);
        }
    }

    #[inline]
    pub fn is_fully_damaged(&self) -> bool {
        self.damage.full
    }

    /// Update selection damage tracking.
    /// This should be called when the selection changes to damage only the affected lines.
    pub fn update_selection_damage(
        &mut self,
        new_selection: Option<SelectionRange>,
        display_offset: usize,
    ) {
        // Damage old selection lines if they exist
        if let Some(old_selection) = self.damage.last_selection {
            self.damage.damage_selection(old_selection, display_offset);
        }

        // Damage new selection lines if they exist
        if let Some(new_selection) = new_selection {
            self.damage.damage_selection(new_selection, display_offset);
        }

        // Update the stored selection
        self.damage.last_selection = new_selection;
    }
    #[must_use]
    pub fn damage(&mut self) -> TermDamage<'_> {
        // Ensure the entire terminal is damaged after entering insert mode.
        // Leaving is handled in the ansi handler.
        if self.mode.contains(Mode::INSERT) {
            self.mark_fully_damaged();
        }

        let previous_cursor =
            mem::replace(&mut self.damage.last_cursor, self.grid.cursor.pos);

        if self.damage.full {
            return TermDamage::Full;
        }

        // Add information about old cursor position and new one if they are not the same, so we
        // cover everything that was produced by `Term::input`.
        if self.damage.last_cursor != previous_cursor {
            // Damage the entire line where the previous cursor was
            let previous_line = previous_cursor.row.0 as usize;
            self.damage.damage_line(previous_line);
        }

        // Always damage current cursor.
        // self.damage_cursor();

        // NOTE: damage which changes all the content when the display offset is non-zero (e.g.
        // scrolling) is handled via full damage.
        let display_offset = self.grid.display_offset();
        TermDamage::Partial(TermDamageIterator::new(&self.damage.lines, display_offset))
    }

    /// Peek damage event based on current damage state
    pub fn peek_damage_event(&self) -> Option<TerminalDamage> {
        if self.damage.full {
            Some(TerminalDamage::Full)
        } else if self.damage.lines.iter().any(|line| line.is_damaged()) {
            Some(TerminalDamage::Partial)
        } else if self.damage.last_cursor != self.grid.cursor.pos {
            Some(TerminalDamage::CursorOnly)
        } else {
            None
        }
    }

    #[inline]
    pub fn reset_damage(&mut self) {
        self.damage.reset();
        // The consumer snapshots the cursor along with the rows, so the
        // position is painted once this frame is consumed. Without this,
        // `peek_damage_event` reports CursorOnly forever after the first
        // cursor movement (`last_cursor` was only maintained by the
        // legacy `damage()` path), so every PTY drain fires a redraw
        // event even when nothing changed.
        self.damage.last_cursor = self.grid.cursor.pos;
    }

    #[inline]
    pub fn display_offset(&self) -> usize {
        self.grid.display_offset()
    }

    #[inline]
    pub fn clear_saved_history(&mut self) {
        self.clear_screen(ClearMode::Saved);
    }

    /// Scroll so the previous (`forward = false`) or next prompt row
    /// starts at the top of the viewport. Prompts come from OSC 133
    /// marks; a run of consecutive prompt rows counts as one prompt.
    pub fn scroll_to_prompt(&mut self, forward: bool) {
        use crate::crosswords::grid::row::SemanticPrompt;

        let display_offset = self.grid.display_offset() as i32;
        let history = self.grid.history_size() as i32;
        let screen_lines = self.grid.screen_lines() as i32;
        let top = -display_offset;

        let is_marked =
            |line: i32| self.grid[Line(line)].semantic_prompt != SemanticPrompt::None;
        // First row of a prompt run: marked, with an unmarked row (or
        // the top of history) above it.
        let is_prompt_start =
            |line: i32| is_marked(line) && (line == -history || !is_marked(line - 1));

        let target = if forward {
            (top + 1..screen_lines).find(|line| is_prompt_start(*line))
        } else {
            (-history..top).rev().find(|line| is_prompt_start(*line))
        };

        if let Some(line) = target {
            let new_offset = (-line).max(0);
            self.scroll_display(Scroll::Delta(new_offset - display_offset));
        }
    }

    #[inline]
    pub fn scroll_display(&mut self, scroll: Scroll) {
        let old_display_offset = self.grid.display_offset();
        self.grid.scroll_display(scroll);
        self.event_proxy
            .send_event(RioEvent::MouseCursorDirty, self.window_id);

        // Clamp vi mode cursor to the viewport.
        let viewport_start = -(self.grid.display_offset() as i32);
        let viewport_end = viewport_start + self.grid.bottommost_line().0;
        let vi_cursor_line = &mut self.vi_mode_cursor.pos.row.0;
        *vi_cursor_line =
            std::cmp::min(viewport_end, std::cmp::max(viewport_start, *vi_cursor_line));
        self.vi_mode_recompute_selection();

        // Damage everything if display offset changed.
        if old_display_offset != self.grid.display_offset() {
            self.mark_fully_damaged();
            // Scrolling changes image positions on screen
            if !self.graphics.kitty_placements.is_empty() {
                self.graphics.kitty_graphics_dirty = true;
            }
        }
    }

    /// Lines ever evicted off the scrollback ring (see
    /// `Grid::lines_evicted`).
    #[inline]
    pub fn lines_evicted(&self) -> u64 {
        self.grid.lines_evicted()
    }

    #[inline]
    pub fn bottommost_line(&self) -> Line {
        self.grid.bottommost_line()
    }

    #[inline]
    pub fn colors(&self) -> &TermColors {
        &self.colors
    }

    /// Get queues to update graphic data. If both queues are empty, it returns
    /// `None`.
    #[inline]
    pub fn graphics_take_queues(&mut self) -> Option<UpdateQueues> {
        self.graphics.take_queues()
    }

    #[inline]
    pub fn send_graphics_updates(&mut self) {
        if self.graphics.has_pending_updates() {
            if let Some(queues) = self.graphics.take_queues() {
                self.event_proxy.send_event(
                    RioEvent::UpdateGraphics {
                        route_id: self.route_id,
                        queues,
                    },
                    self.window_id,
                );
            }
        }
    }

    #[inline]
    pub fn exit(&mut self)
    where
        U: EventListener,
    {
        self.event_proxy
            .send_event(RioEvent::CloseTerminal(self.route_id), self.window_id);
    }

    pub fn resize<S: Dimensions>(&mut self, size: S) {
        let old_cols = self.grid.columns();
        let old_lines = self.grid.screen_lines();
        let num_cols = size.columns();
        let num_lines = size.screen_lines();

        if old_cols == num_cols && old_lines == num_lines {
            // Same grid, but the cell size may still have changed (a
            // font or DPI change that kept cols/rows constant). Keep
            // the graphics cell metrics and placements in sync so
            // cell-sized images keep tracking the grid.
            if self.graphics.cell_width != size.square_width()
                || self.graphics.cell_height != size.square_height()
            {
                self.graphics.resize(&size);
                let cell_w = self.graphics.cell_width.round() as usize;
                let cell_h = self.graphics.cell_height.round() as usize;
                if cell_w > 0 && cell_h > 0 {
                    for p in self.graphics.kitty_placements.values_mut() {
                        let (iw, ih) = self
                            .graphics
                            .kitty_images
                            .get(&p.image_id)
                            .map(|s| (s.data.width, s.data.height))
                            .unwrap_or((0, 0));
                        p.rescale(iw, ih, cell_w, cell_h);
                    }
                    for p in self
                        .graphics
                        .kitty_inactive_screen
                        .kitty_placements
                        .values_mut()
                    {
                        let (iw, ih) = self
                            .graphics
                            .kitty_inactive_screen
                            .kitty_images
                            .get(&p.image_id)
                            .map(|s| (s.data.width, s.data.height))
                            .unwrap_or((0, 0));
                        p.rescale(iw, ih, cell_w, cell_h);
                    }
                    if !self.graphics.kitty_placements.is_empty()
                        || !self
                            .graphics
                            .kitty_inactive_screen
                            .kitty_placements
                            .is_empty()
                    {
                        self.graphics.kitty_graphics_dirty = true;
                    }
                }
            }
            info!("Crosswords::resize dimensions unchanged");
            return;
        }
        // Move vi mode cursor with the content.
        let history_size = self.history_size();
        let mut delta = num_lines as i32 - old_lines as i32;
        let min_delta =
            std::cmp::min(0, num_lines as i32 - self.grid.cursor.pos.row.0 - 1);

        delta = std::cmp::min(std::cmp::max(delta, min_delta), history_size as i32);
        self.vi_mode_cursor.pos.row += delta;

        // Image placements (kitty and sixel/iTerm2) anchor at absolute
        // rows. A column reflow moves content to different rows, so ask
        // each grid to record an exact old-row to new-row mapping while
        // it reflows, but only when that grid's screen actually has
        // placements; tracking costs a Vec sized to the ring. Vertical
        // resizes never move content in absolute row space and produce
        // no remap.
        let active_has_placements = !self.graphics.kitty_placements.is_empty()
            || !self.graphics.atlas_placements.is_empty();
        let inactive_has_placements = !self
            .graphics
            .kitty_inactive_screen
            .kitty_placements
            .is_empty()
            || !self
                .graphics
                .kitty_inactive_screen
                .atlas_placements
                .is_empty();

        let is_alt = self.mode.contains(Mode::ALT_SCREEN);
        self.grid.track_reflow_remap = active_has_placements;
        self.inactive_grid.track_reflow_remap = inactive_has_placements;
        self.grid.resize(!is_alt, num_lines, num_cols);
        self.inactive_grid.resize(is_alt, num_lines, num_cols);
        self.grid.track_reflow_remap = false;
        self.inactive_grid.track_reflow_remap = false;
        let active_remap = self.grid.reflow_remap.take();
        let inactive_remap = self.inactive_grid.reflow_remap.take();

        // Invalidate selection and tabs only when necessary.
        if old_cols != num_cols {
            self.selection = None;

            // Recreate tabs list.
            self.tabs.resize(num_cols);
        } else if let Some(selection) = self.selection.take() {
            let max_lines = std::cmp::max(num_lines, old_lines) as i32;
            let range = Line(0)..Line(max_lines);
            self.selection = selection.rotate(&self.grid, &range, -delta);
        }

        // Clamp vi cursor to viewport.
        let vi_pos = self.vi_mode_cursor;
        let viewport_top = Line(-(self.grid.display_offset() as i32));
        let viewport_bottom = viewport_top + self.bottommost_line();
        self.vi_mode_cursor.pos.row =
            std::cmp::max(std::cmp::min(vi_pos.pos.row, viewport_bottom), viewport_top);
        self.vi_mode_cursor.pos.col =
            std::cmp::min(vi_pos.pos.col, self.grid.last_column());

        // Reset scrolling region.
        self.scroll_region = Line(0)..Line(self.grid.screen_lines() as i32);

        // Resize damage information.
        self.damage.resize(num_lines);

        // Update size information for graphics.
        self.graphics.resize(&size);

        // Rescale overlay placements for the new cell size (cell-sized
        // placements track the grid; native-size ones keep their pixel
        // dimensions). Active and inactive screens both get the
        // treatment so alt-screen images aren't stale on swap-back.
        let cell_w = self.graphics.cell_width.round() as usize;
        let cell_h = self.graphics.cell_height.round() as usize;
        if cell_w > 0 && cell_h > 0 {
            for p in self.graphics.kitty_placements.values_mut() {
                let (iw, ih) = self
                    .graphics
                    .kitty_images
                    .get(&p.image_id)
                    .map(|s| (s.data.width, s.data.height))
                    .unwrap_or((0, 0));
                p.rescale(iw, ih, cell_w, cell_h);
            }
            for p in self
                .graphics
                .kitty_inactive_screen
                .kitty_placements
                .values_mut()
            {
                let (iw, ih) = self
                    .graphics
                    .kitty_inactive_screen
                    .kitty_images
                    .get(&p.image_id)
                    .map(|s| (s.data.width, s.data.height))
                    .unwrap_or((0, 0));
                p.rescale(iw, ih, cell_w, cell_h);
            }
        }

        // Re-anchor placements through the exact reflow row mapping:
        // each one moves to wherever its anchor row's content landed.
        // Kitty placements on a dropped row stay put (they float and
        // get culled off screen); sixel/iTerm2 placements are grid
        // content and are destroyed, along with any that a truncation
        // pushed past the ring end.
        if let Some(remap) = &active_remap {
            let max_abs =
                self.grid.lines_evicted() as i64 + self.grid.total_lines() as i64;
            for p in self.graphics.kitty_placements.values_mut() {
                if let Some(abs) = remap.remap_abs(p.dest_row) {
                    p.dest_row = abs;
                }
            }
            let before = self.graphics.atlas_placements.len();
            self.graphics.atlas_placements.retain_mut(|p| {
                match remap.remap_abs(p.abs_row) {
                    Some(abs) if abs < max_abs => {
                        p.abs_row = abs;
                        true
                    }
                    _ => false,
                }
            });
            if self.graphics.atlas_placements.len() != before {
                self.graphics.recount_atlas_keys();
                self.send_graphics_updates();
            }
        }
        if let Some(remap) = &inactive_remap {
            let max_abs = self.inactive_grid.lines_evicted() as i64
                + self.inactive_grid.total_lines() as i64;
            let inactive = &mut self.graphics.kitty_inactive_screen;
            for p in inactive.kitty_placements.values_mut() {
                if let Some(abs) = remap.remap_abs(p.dest_row) {
                    p.dest_row = abs;
                }
            }
            let before = inactive.atlas_placements.len();
            inactive
                .atlas_placements
                .retain_mut(|p| match remap.remap_abs(p.abs_row) {
                    Some(abs) if abs < max_abs => {
                        p.abs_row = abs;
                        true
                    }
                    _ => false,
                });
            if inactive.atlas_placements.len() != before {
                self.graphics.recount_inactive_atlas_keys();
                self.send_graphics_updates();
            }
        }

        // The renderer only refreshes its placement snapshot when it
        // sees the dirty flag, and resize invalidates positions even
        // when no placement field changed (history and viewport moved).
        if active_has_placements || inactive_has_placements {
            self.graphics.kitty_graphics_dirty = true;
        }
        self.expire_atlas_placements();
    }

    /// Toggle the vi mode.
    #[inline]
    pub fn toggle_vi_mode(&mut self)
    where
        U: EventListener,
    {
        self.mode ^= Mode::VI;

        if self.mode.contains(Mode::VI) {
            let display_offset = self.grid.display_offset() as i32;
            if self.grid.cursor.pos.row > self.grid.bottommost_line() - display_offset {
                // Move cursor to top-left if terminal cursor is not visible.
                let pos = Pos::new(Line(-display_offset), Column(0));
                self.vi_mode_cursor.pos = pos;
            } else {
                // Reset vi mode cursor position to match primary cursor.
                self.vi_mode_cursor.pos = self.grid.cursor.pos;
            }
        }

        // Update UI about cursor blinking state changes.
        self.event_proxy
            .send_event(RioEvent::CursorBlinkingChange, self.window_id);
    }

    /// Update the active selection to match the vi mode cursor position.
    #[inline]
    fn vi_mode_recompute_selection(&mut self) {
        // Require vi mode to be active.
        if !self.mode.contains(Mode::VI) {
            return;
        }

        // Update only if non-empty selection is present.
        if let Some(selection) = self.selection.as_mut().filter(|s| !s.is_empty()) {
            selection.update(self.vi_mode_cursor.pos, Side::Left);
            selection.include_all();
        }
    }

    #[inline]
    pub fn vi_motion(&mut self, motion: ViMotion)
    where
        U: EventListener,
    {
        // Require vi mode to be active.
        if !self.mode.contains(Mode::VI) {
            return;
        }

        // Move cursor.
        self.vi_mode_cursor = self.vi_mode_cursor.motion(self, motion);
        self.vi_mode_recompute_selection();
    }

    /// Move vi cursor to a point in the grid.
    #[inline]
    pub fn vi_goto_pos(&mut self, pos: Pos)
    where
        U: EventListener,
    {
        // Move viewport to make pos visible.
        self.scroll_to_pos(pos);

        // Move vi cursor to the pos.
        self.vi_mode_cursor.pos = pos;

        self.vi_mode_recompute_selection();
    }

    /// Scroll display to point if it is outside of viewport.
    #[inline]
    pub fn scroll_to_pos(&mut self, pos: Pos)
    where
        U: EventListener,
    {
        let display_offset = self.grid.display_offset() as i32;
        let screen_lines = self.grid.screen_lines() as i32;

        if pos.row < -display_offset {
            let lines = pos.row + display_offset;
            self.scroll_display(Scroll::Delta(-lines.0));
        } else if pos.row >= (screen_lines - display_offset) {
            let lines = pos.row + display_offset - screen_lines + 1i32;
            self.scroll_display(Scroll::Delta(-lines.0));
        }
    }

    /// Jump to the end of a wide cell.
    pub fn expand_wide(&self, mut pos: Pos, direction: Direction) -> Pos {
        use crate::crosswords::square::Wide;
        let wide = self.grid[pos.row][pos.col].wide();

        match direction {
            Direction::Right if matches!(wide, Wide::LeadingSpacer) => {
                pos.col = Column(1);
                pos.row += 1;
            }
            Direction::Right if matches!(wide, Wide::Wide) => {
                pos.col = std::cmp::min(pos.col + 1, self.grid.last_column());
            }
            Direction::Left if matches!(wide, Wide::Wide | Wide::Spacer) => {
                if matches!(wide, Wide::Spacer) {
                    pos.col -= 1;
                }

                let prev = pos.sub(&self.grid, Boundary::Grid, 1);
                if matches!(self.grid[prev].wide(), Wide::LeadingSpacer) {
                    pos = prev;
                }
            }
            _ => (),
        }

        pos
    }

    #[inline]
    pub fn semantic_escape_chars(&self) -> &str {
        &self.semantic_escape_chars
    }

    #[inline]
    pub fn wrapline(&mut self) {
        if !self.mode.contains(Mode::LINE_WRAP) {
            return;
        }

        self.grid.cursor_cell().set_wrapline(true);

        if self.grid.cursor.pos.row + 1 >= self.scroll_region.end {
            self.linefeed();
        } else {
            self.damage_cursor();
            self.grid.cursor.pos.row += 1;
        }

        self.grid.cursor.pos.col = Column(0);
        self.grid.cursor.should_wrap = false;
        self.damage_cursor();
    }

    pub fn history_size(&self) -> usize {
        self.grid
            .total_lines()
            .saturating_sub(self.grid.screen_lines())
    }

    /// Damage the entire line at the cursor position
    #[inline]
    pub fn damage_cursor_line(&mut self) {
        let cursor_line = self.grid.cursor.pos.row.0 as usize;
        self.damage_line(cursor_line);
    }

    /// Damage an entire line
    #[inline]
    pub fn damage_line(&mut self, line: usize) {
        self.damage.damage_line(line);
    }

    #[inline]
    pub fn damage_cursor(&mut self) {
        self.damage_cursor_line();

        // self.event_proxy.send_event(
        // RioEvent::TerminalDamaged {
        // route_id: self.route_id,
        // damage: TerminalDamage::CursorOnly(self.grid.cursor.pos.line, None),
        // },
        // self.window_id,
        // );
    }

    #[inline]
    pub fn damage_cursor_blink(&mut self) {
        // Only damage cursor for blink if cursor is visible and blinking is enabled
        let cursor_state = self.cursor();
        if cursor_state.is_visible() {
            // Use line-based damage for cursor blinking
            self.damage_cursor_line();

            // self.event_proxy.send_event(
            // RioEvent::TerminalDamaged {
            // route_id: self.route_id,
            // damage: TerminalDamage::CursorOnly,
            // },
            // self.window_id,
            // );
        }
    }

    /// Check if any rendering is actually needed
    #[inline]
    pub fn needs_render(&self) -> bool {
        // Always render if fully damaged
        if self.is_fully_damaged() {
            return true;
        }

        // Check if there's any partial damage
        if self.damage.lines.iter().any(|line| line.is_damaged()) {
            return true;
        }

        // No rendering needed if no damage
        false
    }

    #[inline]
    fn scroll_down_relative(&mut self, origin: Line, mut lines: usize) {
        debug!(
            "Scrolling down relative: origin={}, lines={}",
            origin, lines
        );

        lines = std::cmp::min(
            lines,
            (self.scroll_region.end - self.scroll_region.start).0 as usize,
        );
        lines = std::cmp::min(lines, (self.scroll_region.end - origin).0 as usize);

        let region = origin..self.scroll_region.end;

        // Scroll selection.
        self.selection = self
            .selection
            .take()
            .and_then(|s| s.rotate(&self.grid, &region, -(lines as i32)));

        // Scroll vi mode cursor.
        let line = &mut self.vi_mode_cursor.pos.row;
        if region.start <= *line && region.end > *line {
            *line = std::cmp::min(*line + lines, region.end - 1);
        }

        // Scroll between origin and bottom
        self.grid.sync_template_style();
        self.grid.scroll_down(&region, lines);
        self.mark_fully_damaged();
        // Partial-region scrolls move grid-plane images with their
        // content (kitty placements float and are not adjusted).
        self.shift_atlas_placements_in_region(&region, lines as i64);
        if !self.graphics.kitty_placements.is_empty() {
            self.graphics.kitty_graphics_dirty = true;
        }
    }

    #[inline]
    pub fn scroll_up_relative(&mut self, origin: Line, mut lines: usize) {
        lines = std::cmp::min(
            lines,
            (self.scroll_region.end - self.scroll_region.start).0 as usize,
        );

        let region = origin..self.scroll_region.end;

        // Scroll selection.
        if self.selection.is_some() {
            self.selection = self
                .selection
                .take()
                .and_then(|s| s.rotate(&self.grid, &region, lines as i32));
        }

        self.grid.sync_template_style();
        let pinned_offset = self.grid.display_offset();
        self.grid.scroll_up(&region, lines);

        // A viewport pinned in scrollback normally absorbs the scroll:
        // the offset grows and the same absolute rows stay on screen.
        // At the history cap the offset clamps, the view slides, and
        // every visible row changes while the region damage below only
        // covers active-area lines. Nothing less than full damage is
        // correct then.
        if region.start == 0
            && pinned_offset != 0
            && self.grid.display_offset() != pinned_offset + lines
        {
            self.mark_fully_damaged();
        }

        // Scroll vi mode cursor.
        let viewport_top = Line(-(self.grid.display_offset() as i32));
        let top = if region.start == 0 {
            viewport_top
        } else {
            region.start
        };
        let line = &mut self.vi_mode_cursor.pos.row;
        if (top <= *line) && region.end > *line {
            *line = std::cmp::max(*line - lines, top);
        }
        // Mark the scroll region as damaged; a full-screen region
        // coalesces into full damage, like scroll_down_relative.
        if region.start.0 == 0 && region.end.0 as usize == self.grid.screen_lines() {
            self.mark_fully_damaged();
        } else {
            for line in region.start.0..region.end.0 {
                self.damage.damage_line(line as usize);
            }
        }
        if !self.graphics.kitty_placements.is_empty() {
            // Placements whose rows all scrolled off the ring expire,
            // like kitty: the image data survives for future
            // placements, the placement itself dies with its content.
            let base = self.grid.lines_evicted() as i64;
            self.graphics
                .kitty_placements
                .retain(|_, p| p.dest_row + p.rows as i64 > base);
            self.graphics.kitty_graphics_dirty = true;
        }
        // Grid-plane images follow their content through region
        // scrolls; kitty placements float and are not adjusted.
        if region.start.0 == 0 {
            if (region.end.0 as usize) < self.grid.screen_lines() {
                // History grew under the fixed bottom rows.
                self.shift_atlas_placements_below(region.end, lines as i64);
            }
            // In-region content keeps its absolute rows (they moved
            // into history), so nothing else to do.
        } else {
            self.shift_atlas_placements_in_region(&region, -(lines as i64));
        }
        self.expire_atlas_placements();
    }

    /// Drop sixel/iTerm2 placements whose rows all scrolled off the
    /// ring; the key recount frees images that lost their last
    /// placement (pixel store and GPU texture).
    fn expire_atlas_placements(&mut self) {
        if self.graphics.atlas_placements.is_empty() {
            return;
        }
        let base = self.grid.lines_evicted() as i64;
        let before = self.graphics.atlas_placements.len();
        self.graphics
            .atlas_placements
            .retain(|p| p.abs_row + p.rows as i64 > base);
        if self.graphics.atlas_placements.len() != before {
            self.graphics.recount_atlas_keys();
            self.graphics.kitty_graphics_dirty = true;
            self.send_graphics_updates();
        }
    }

    /// Clip sixel/iTerm2 placements against a cell-aligned hole in
    /// visible screen coordinates (DEC semantics: text and erasure
    /// remove exactly the covered image cells; the surviving pieces
    /// keep referencing the same texture with adjusted crops).
    pub fn clip_atlas_placements(
        &mut self,
        screen_row_start: i32,
        screen_row_end: i32,
        col_start: usize,
        col_end: usize,
    ) {
        if self.graphics.atlas_placements.is_empty() {
            return;
        }
        let base = self.grid.lines_evicted() as i64 + self.history_size() as i64;
        let hr0 = base + screen_row_start as i64;
        let hr1 = base + screen_row_end as i64;

        let old = std::mem::take(&mut self.graphics.atlas_placements);
        let mut next: Vec<crate::ansi::graphics::AtlasPlacement> =
            Vec::with_capacity(old.len());
        let mut changed = false;
        for placement in old {
            match placement.subtract_rect(hr0, hr1, col_start, col_end, &mut next) {
                None => next.push(placement),
                Some(()) => changed = true,
            }
        }
        self.graphics.atlas_placements = next;
        if changed {
            self.graphics.recount_atlas_keys();
            self.graphics.kitty_graphics_dirty = true;
            self.send_graphics_updates();
        }
    }

    /// Shift sixel/iTerm2 placements with a partial scroll region
    /// (DECSTBM): content inside the region moves by `delta` rows and
    /// clips at the region boundary, exactly like text; content
    /// outside is untouched. Full-screen scrolls never come here;
    /// absolute anchoring already handles them.
    fn shift_atlas_placements_in_region(
        &mut self,
        region: &std::ops::Range<Line>,
        delta: i64,
    ) {
        if self.graphics.atlas_placements.is_empty() {
            return;
        }
        let base = self.grid.lines_evicted() as i64 + self.history_size() as i64;
        let r0 = base + region.start.0 as i64;
        let r1 = base + region.end.0 as i64;

        let old = std::mem::take(&mut self.graphics.atlas_placements);
        let mut next: Vec<crate::ansi::graphics::AtlasPlacement> =
            Vec::with_capacity(old.len());
        let mut changed = false;
        for placement in old {
            let p_r0 = placement.abs_row;
            let p_r1 = placement.abs_row + placement.rows as i64;
            if p_r1 <= r0 || p_r0 >= r1 {
                next.push(placement);
                continue;
            }
            changed = true;
            // Pieces outside the region stay put.
            placement.subtract_rect(r0, r1, 0, usize::MAX, &mut next);
            // The piece inside shifts, then clips at the region bounds.
            let ir0 = p_r0.max(r0);
            let ir1 = p_r1.min(r1);
            let inside = placement.slice(
                ir0,
                ir1,
                placement.col,
                placement.col + placement.columns,
            );
            let shifted_r0 = (inside.abs_row + delta).max(r0);
            let shifted_r1 = (inside.abs_row + delta + inside.rows as i64).min(r1);
            if shifted_r1 > shifted_r0 {
                // Clip in the pre-shift frame, then translate.
                let mut clipped = inside.slice(
                    shifted_r0 - delta,
                    shifted_r1 - delta,
                    inside.col,
                    inside.col + inside.columns,
                );
                clipped.abs_row += delta;
                next.push(clipped);
            }
        }
        self.graphics.atlas_placements = next;
        if changed {
            self.graphics.recount_atlas_keys();
            self.graphics.kitty_graphics_dirty = true;
            self.send_graphics_updates();
        }
    }

    /// A top-anchored partial scroll region grows history while the
    /// rows below the region stay visually fixed, which advances
    /// their absolute coordinates. Shift placements on those rows
    /// (splitting any placement straddling the boundary) so they stay
    /// glued to their fixed content.
    fn shift_atlas_placements_below(&mut self, boundary: Line, delta: i64) {
        if self.graphics.atlas_placements.is_empty() {
            return;
        }
        // The boundary in the space placements were anchored in,
        // BEFORE this scroll grew history by `delta`.
        let base = self.grid.lines_evicted() as i64 + self.history_size() as i64 - delta;
        let b = base + boundary.0 as i64;

        let old = std::mem::take(&mut self.graphics.atlas_placements);
        let mut next: Vec<crate::ansi::graphics::AtlasPlacement> =
            Vec::with_capacity(old.len());
        let mut changed = false;
        for placement in old {
            let p_r0 = placement.abs_row;
            let p_r1 = placement.abs_row + placement.rows as i64;
            if p_r1 <= b {
                next.push(placement);
                continue;
            }
            changed = true;
            if p_r0 < b {
                next.push(placement.slice(
                    p_r0,
                    b,
                    placement.col,
                    placement.col + placement.columns,
                ));
            }
            let mut below = placement.slice(
                p_r0.max(b),
                p_r1,
                placement.col,
                placement.col + placement.columns,
            );
            below.abs_row += delta;
            next.push(below);
        }
        self.graphics.atlas_placements = next;
        if changed {
            self.graphics.recount_atlas_keys();
            self.graphics.kitty_graphics_dirty = true;
        }
    }

    #[inline(always)]
    pub fn write_at_cursor(&mut self, c: char) {
        let c = self.grid.cursor.charsets[self.active_charset].map(c);
        self.grid.sync_template_style();
        let template = self.cell_template();
        self.write_cell(c, template);
    }

    /// Write an already-charset-mapped `c` into the cursor cell, taking a
    /// prebuilt `template` (a `Square` carrying the cursor's style / extras /
    /// flags with a zero codepoint). Split out of `write_at_cursor` so a
    /// print run (`input_str`) hoists the charset map and template build out
    /// of its per-cell loop; the write is then a single packed store, and the
    /// cursor cell is resolved once (not twice) in the common narrow case.
    #[inline]
    /// Clear the wide pair that a cell boundary before `col` would
    /// split, plus the cross-row links a mutation at that boundary
    /// severs: each
    /// range mutation names the exact seams it cuts, O(1) per seam,
    /// instead of sweeping the row afterwards.
    ///
    /// `col` may equal `columns`, meaning the boundary to the right of
    /// the final cell — that clears stale pre-wrap padding
    /// (`LeadingSpacer`) so a shift can't drag it into the interior.
    ///
    /// A cleared cell takes `blank` but keeps WRAPLINE: the flag marks
    /// the logical line for reflow and lives on the row's last cell,
    /// which the mutation never targeted.
    fn split_wide_seam(
        &mut self,
        line: Line,
        col: usize,
        blank: crate::crosswords::square::Square,
    ) {
        use crate::crosswords::square::Wide;
        let columns = self.grid.columns();

        // Boundary right of the final cell.
        if col >= columns {
            let last = Column(columns - 1);
            if matches!(self.grid[line][last].wide(), Wide::LeadingSpacer) {
                self.grid[line][last].set_wide(Wide::Narrow);
            }
            return;
        }

        // A wide char at column 0 pairs with a `LeadingSpacer` closing
        // the previous row; a mutation reaching columns 0/1 severs
        // that link. Mirrors `write_cell`'s revert (including its
        // history-row reach when line 0 scrolled something out).
        if col <= 1
            && matches!(self.grid[line][Column(0)].wide(), Wide::Wide)
            && line != self.grid.topmost_line()
        {
            let last = self.grid.last_column();
            let prev = &mut self.grid[line - 1i32][last];
            if matches!(prev.wide(), Wide::LeadingSpacer) {
                prev.set_wide(Wide::Narrow);
                if line.0 > 0 {
                    self.damage.damage_line((line.0 - 1) as usize);
                }
            }
        }
        if col == 0 {
            return;
        }

        // Interior boundary: splitting between col-1 and col cuts a
        // pair whose lead sits at col-1. Erasing half a wide char
        // erases the whole char (the xterm convention).
        if matches!(self.grid[line][Column(col - 1)].wide(), Wide::Wide) {
            self.clear_pair_preserving_wrapline(line, col - 1, blank);
        }
    }

    /// Clear both halves of the wide pair with its lead at `col`,
    /// keeping each cell's WRAPLINE bit.
    fn clear_pair_preserving_wrapline(
        &mut self,
        line: Line,
        col: usize,
        blank: crate::crosswords::square::Square,
    ) {
        let columns = self.grid.columns();
        for c in col..(col + 2).min(columns) {
            let cell = &mut self.grid[line][Column(c)];
            let wrapline = cell.wrapline();
            *cell = blank;
            if wrapline {
                cell.set_wrapline(true);
            }
        }
    }

    /// Append `c` to the cluster of the cell most recently written at
    /// the cursor (legacy zero-width semantics: `saturating_sub`
    /// column math, so a mark at column 0 targets column 0).
    fn attach_to_prev_cell(&mut self, c: char) {
        let mut column = self.grid.cursor.pos.col;
        if !self.grid.cursor.should_wrap {
            column.0 = column.saturating_sub(1);
        }

        let row = self.grid.cursor.pos.row;
        if matches!(self.grid[row][column].wide(), Wide::Spacer) {
            column.0 = column.saturating_sub(1);
        }

        // A bg-only cell reuses the extras-id bits for its color
        // channel: reading them as an id would clone an unrelated
        // slot onto this cell, and writing one back would corrupt
        // the color. A combining mark with no base character has
        // nothing to attach to, so drop it.
        if !matches!(
            self.grid[row][column].content_tag(),
            crate::crosswords::square::ContentTag::Codepoint
        ) {
            return;
        }
        // Copy-on-write: slots are interned and shared; every
        // cell written under one OSC 8 template references the
        // same slot, so pushing into it in place would attach the
        // mark to the whole hyperlink span. Clone, extend, and
        // re-intern instead; the marked cell gets its own
        // (marks + hyperlink) slot and its neighbors keep theirs.
        let existing_id = self.grid[row][column].extras_id();
        let mut extras = existing_id
            .and_then(|id| self.grid.extras_table.get(id).cloned())
            .unwrap_or_default();
        extras.zerowidth.push(c);
        let id = self.grid.alloc_extras(extras);
        if id != 0 {
            let cell = &mut self.grid[row][column];
            cell.set_extras_id(Some(id));
            cell.insert_cell_flag(CellFlags::GRAPHEME);
            self.grid[row].has_extras = true;
            // The attach mutated an already-painted cell and the input
            // paths return without touching the write-path damage
            // bookkeeping: record it here or the renderer never
            // repaints the row (stale glyphs until a full redraw).
            self.damage.damage_line(row.0 as usize);
        }
        // id == 0: slot space exhausted. Keep the cell's existing
        // extras (hyperlink, earlier marks) rather than erasing
        // them; only the new mark is lost.
    }

    /// Mode-2027 cluster continuation. `true` means `c` was consumed:
    /// appended to the previous cell's cluster (possibly widening it),
    /// or deliberately ignored (an invalid variation
    /// selector). `false` means a grapheme break:
    /// the caller writes `c` through the normal paths.
    ///
    /// There is no cross-call segmentation state. Any no-break
    /// sequence accumulates into a single cell, so everything the
    /// break rules can look behind at, an emoji ZWJ run (GB11),
    /// regional-indicator parity (GB12/13), an Indic conjunct chain
    /// (GB9c), is exactly the previous cell's contents, and the
    /// `BreakState` is rebuilt from them (a handful of codepoints).
    /// Cursor movement, clears, scrolling, and screen switches
    /// invalidate a cluster automatically: whatever cell precedes the
    /// cursor *is* the truth, with no reset hooks to forget.
    fn try_cluster_append(&mut self, c: char, width: usize) -> bool {
        use crate::grapheme_lut::{class_of, is_break_lut, start_state};

        let row = self.grid.cursor.pos.row;
        let Some(base_col) = self.prev_cell_col() else {
            return false;
        };
        let cell = self.grid[row][Column(base_col)];
        if !matches!(
            cell.content_tag(),
            crate::crosswords::square::ContentTag::Codepoint
        ) {
            return false;
        }
        let base = cell.c();
        if base == '\0' {
            // Never-written cell: nothing to continue.
            return false;
        }

        // Reconstruct the segmentation state from the cluster itself.
        // Flat-table hops: one class lookup per
        // codepoint and one transition index per step, no rule chain.
        let mut prev = class_of(base);
        let mut state = start_state(prev);
        let mut last_cp = base;
        if let Some(id) = cell.extras_id() {
            if let Some(extras) = self.grid.extras_table.get(id) {
                for &attached in &extras.zerowidth {
                    let class = class_of(attached);
                    let _ = is_break_lut(prev, class, &mut state);
                    prev = class;
                    last_cp = attached;
                }
            }
        }
        if is_break_lut(prev, class_of(c), &mut state) {
            return false;
        }

        // Joined. Decide the width effect: variation selectors flip a valid
        // base's width and are otherwise ignored outright; any other
        // width-bearing continuation makes the whole cluster wide
        // (the base already contributed one column); zero-width
        // continuations change nothing.
        match c {
            '\u{FE0F}' => {
                // A selector modifies the codepoint immediately before
                // it, so validity is judged against the *last* codepoint
                // of the cluster, not its base. The width helpers are
                // no-ops when the cell is already in the target state.
                if vs_is_valid_base(last_cp, c) {
                    self.widen_prev_cell();
                    self.attach_to_prev_cell(c);
                }
                // Invalid selector: ignore. The cell is untouched, so
                // the reconstructed state next time is identical.
            }
            '\u{FE0E}' => {
                if vs_is_valid_base(last_cp, c) {
                    self.narrow_prev_cell();
                    self.attach_to_prev_cell(c);
                }
            }
            _ if width == 0 => self.attach_to_prev_cell(c),
            _ => {
                self.widen_prev_cell();
                self.attach_to_prev_cell(c);
            }
        }
        true
    }

    fn write_cell(&mut self, c: char, template: crate::crosswords::square::Square) {
        use crate::crosswords::square::{Square, Wide};

        // DEC semantics: printing into a cell covered by a sixel or iTerm2
        // image clips exactly that cell out of the placement. One branch
        // when no images exist.
        if !self.graphics.atlas_placements.is_empty() {
            let pos = self.grid.cursor.pos;
            self.clip_atlas_placements(
                pos.row.0,
                pos.row.0 + 1,
                pos.col.0,
                pos.col.0 + 1,
            );
        }

        let has_extras = template.extras_id().is_some();
        let styled = template.carries_style();
        let cursor_square = self.grid.cursor_square();

        // Fast path: overwriting a narrow cell. One resolve, one packed store.
        if !matches!(cursor_square.wide(), Wide::Wide | Wide::Spacer) {
            *cursor_square = Square::from_template(template, c);
            if has_extras || styled {
                let row = self.grid.cursor.pos.row;
                self.grid[row].has_extras |= has_extras;
                self.grid[row].has_styles |= styled;
            }
            return;
        }

        // Slow path: overwriting a wide char / spacer needs neighbour cleanup.
        let wide = matches!(cursor_square.wide(), Wide::Wide);
        let point = self.grid.cursor.pos;
        if wide && point.col < self.grid.last_column() {
            self.grid[point.row][point.col + 1].set_wide(Wide::Narrow);
        } else if point.col > 0 {
            self.grid[point.row][point.col - 1].clear();
        }
        // Remove leading spacers.
        if point.col <= 1 && point.row != self.grid.topmost_line() {
            let column = self.grid.last_column();
            let prev = &mut self.grid[point.row - 1i32][column];
            if matches!(prev.wide(), Wide::LeadingSpacer) {
                prev.set_wide(Wide::Narrow);
            }
        }

        *self.grid.cursor_cell() = Square::from_template(template, c);
        if has_extras || styled {
            let row = self.grid.cursor.pos.row;
            self.grid[row].has_extras |= has_extras;
            self.grid[row].has_styles |= styled;
        }
    }

    /// Write one non-zero-width codepoint at the cursor with scalar wrap,
    /// placeholder and wide-pair handling; fallback for the run writers.
    fn write_codepoint_cell(&mut self, cp: u32, c: char, width: u8) {
        if self.grid.cursor.should_wrap {
            self.wrapline();
        }

        let columns = self.grid.columns();

        // Kitty placeholder bookkeeping: cp can't be ASCII here (parser
        // routes ASCII through `input_str`) but the placeholder lives at
        // U+10EEEE, so the check is still needed.
        if cp == crate::ansi::kitty_virtual::PLACEHOLDER as u32 {
            let row = self.grid.cursor.pos.row;
            self.grid[row].kitty_virtual_placeholder = true;
        }

        // A one-column grid can never hold a wide pair, and the wrap-for-room
        // branch below cannot create room on it: wrapping lands back on
        // column 0, which is still the last column. Drop the glyph and leave
        // a blank narrow cell, rather
        // than running the Spacer off the end of the row. Embedders are
        // entitled to such a grid: drag-resize and tiling layouts produce
        // them transiently.
        let (c, width) = if width == 2 && columns < 2 {
            (' ', 1)
        } else {
            (c, width)
        };

        if width == 2 {
            if self.grid.cursor.pos.col + 1 >= columns {
                if self.mode.contains(Mode::LINE_WRAP) {
                    self.write_at_cursor(' ');
                    self.grid
                        .cursor_cell()
                        .set_wide(crate::crosswords::square::Wide::LeadingSpacer);
                    self.wrapline();
                } else {
                    self.grid.cursor.should_wrap = true;
                    return;
                }
            }

            self.write_at_cursor(c);
            self.grid
                .cursor_cell()
                .set_wide(crate::crosswords::square::Wide::Wide);
            self.grid.cursor.pos.col += 1;
            self.write_at_cursor(' ');
            self.grid
                .cursor_cell()
                .set_wide(crate::crosswords::square::Wide::Spacer);
        } else {
            self.write_at_cursor(c);
        }

        let cursor_line = self.grid.cursor.pos.row.0 as usize;
        self.damage.damage_line(cursor_line);

        if self.grid.cursor.pos.col + 1 < columns {
            self.grid.cursor.pos.col += 1;
        } else {
            self.grid.cursor.should_wrap = true;
        }
    }

    /// Bulk-write a run of width-1 codepoints, mirroring the ASCII bulk
    /// path in `input_ascii_str`: one destination scan, packed stores,
    /// cursor and damage updated per row chunk instead of per codepoint.
    fn write_narrow_run(
        &mut self,
        template: crate::crosswords::square::Square,
        template_has_extras: bool,
        cps: &[u32],
    ) {
        let mut idx = 0;
        while idx < cps.len() {
            if self.grid.cursor.should_wrap {
                self.wrapline();
            }

            let columns = self.grid.columns();
            let cursor_col = self.grid.cursor.pos.col.0;
            let remaining = columns.saturating_sub(cursor_col);
            if remaining == 0 {
                for &cp in &cps[idx..] {
                    let c = char::from_u32(cp).unwrap_or('\u{FFFD}');
                    self.write_codepoint_cell(cp, c, 1);
                }
                return;
            }

            let take = (cps.len() - idx).min(remaining);
            let mut wrote_bulk = false;
            {
                let line = self.grid.cursor.pos.row;
                let row = &mut self.grid[line];
                let cells = &mut row[Column(cursor_col)..Column(cursor_col + take)];
                let needs_cleanup = cells
                    .iter()
                    .fold(false, |acc, c| acc | c.needs_wide_cleanup());
                if !needs_cleanup {
                    for (cell, &cp) in cells.iter_mut().zip(&cps[idx..idx + take]) {
                        let c = char::from_u32(cp).unwrap_or('\u{FFFD}');
                        *cell =
                            crate::crosswords::square::Square::from_template(template, c);
                    }
                    if template_has_extras {
                        row.has_extras = true;
                    }
                    if template.carries_style() {
                        row.has_styles = true;
                    }
                    wrote_bulk = true;
                }
            }

            if wrote_bulk {
                let end = cursor_col + take;
                if end < columns {
                    self.grid.cursor.pos.col = Column(end);
                } else {
                    self.grid.cursor.pos.col = Column(columns - 1);
                    self.grid.cursor.should_wrap = true;
                }
                let row = self.grid.cursor.pos.row;
                self.damage.damage_line(row.0 as usize);
            } else {
                for &cp in &cps[idx..idx + take] {
                    let c = char::from_u32(cp).unwrap_or('\u{FFFD}');
                    self.write_codepoint_cell(cp, c, 1);
                }
            }
            idx += take;
        }
    }

    /// Bulk-write a run of width-2 codepoints as (wide, spacer) cell pairs.
    /// Pairs never split across rows; the end-of-row leading-spacer case
    /// defers to the scalar writer.
    fn write_wide_run(
        &mut self,
        template: crate::crosswords::square::Square,
        template_has_extras: bool,
        cps: &[u32],
    ) {
        use crate::crosswords::square::{Square, Wide};

        let mut spacer = Square::from_template(template, ' ');
        spacer.set_wide(Wide::Spacer);

        let mut idx = 0;
        while idx < cps.len() {
            if self.grid.cursor.should_wrap {
                self.wrapline();
            }

            let columns = self.grid.columns();
            let cursor_col = self.grid.cursor.pos.col.0;
            let remaining = columns.saturating_sub(cursor_col);
            if remaining < 2 {
                let cp = cps[idx];
                let c = char::from_u32(cp).unwrap_or('\u{FFFD}');
                self.write_codepoint_cell(cp, c, 2);
                idx += 1;
                continue;
            }

            let take = (cps.len() - idx).min(remaining / 2);
            let mut wrote_bulk = false;
            {
                let line = self.grid.cursor.pos.row;
                let row = &mut self.grid[line];
                let cells = &mut row[Column(cursor_col)..Column(cursor_col + take * 2)];
                let needs_cleanup = cells
                    .iter()
                    .fold(false, |acc, c| acc | c.needs_wide_cleanup());
                if !needs_cleanup {
                    for (pair, &cp) in
                        cells.chunks_exact_mut(2).zip(&cps[idx..idx + take])
                    {
                        let c = char::from_u32(cp).unwrap_or('\u{FFFD}');
                        let mut wide = Square::from_template(template, c);
                        wide.set_wide(Wide::Wide);
                        pair[0] = wide;
                        pair[1] = spacer;
                    }
                    if template_has_extras {
                        row.has_extras = true;
                    }
                    if template.carries_style() {
                        row.has_styles = true;
                    }
                    wrote_bulk = true;
                }
            }

            if wrote_bulk {
                let end = cursor_col + take * 2;
                if end < columns {
                    self.grid.cursor.pos.col = Column(end);
                } else {
                    self.grid.cursor.pos.col = Column(columns - 1);
                    self.grid.cursor.should_wrap = true;
                }
                let row = self.grid.cursor.pos.row;
                self.damage.damage_line(row.0 as usize);
                idx += take;
            } else {
                for &cp in &cps[idx..idx + take] {
                    let c = char::from_u32(cp).unwrap_or('\u{FFFD}');
                    self.write_codepoint_cell(cp, c, 2);
                }
                idx += take;
            }
        }
    }

    /// Build the cursor's current style/extras/flags into a zero-codepoint
    /// `Square` template for `write_cell`.
    #[inline]
    fn cell_template(&self) -> crate::crosswords::square::Square {
        let mut t = crate::crosswords::square::Square::default();
        t.set_style_id(self.grid.cursor.template.style_id());
        t.set_extras_id(self.grid.cursor.template.extras_id());
        t.set_cell_flags(self.grid.cursor.template.cell_flags());
        t
    }

    /// The column of the cell most recently written at the cursor: the
    /// cursor column itself while a pending wrap holds the cursor past
    /// the edge, otherwise the column before it, stepping over a wide
    /// pair's spacer to its lead. `None` when no such cell exists.
    fn prev_cell_col(&self) -> Option<usize> {
        let col = self.grid.cursor.pos.col.0;
        let col = if self.grid.cursor.should_wrap {
            col
        } else {
            col.checked_sub(1)?
        };
        let row = self.grid.cursor.pos.row;
        Some(match self.grid[row][Column(col)].wide() {
            Wide::Spacer => col.checked_sub(1)?,
            _ => col,
        })
    }

    /// Widen the narrow cell preceding the cursor into a wide pair,
    /// handling the row-edge wrap. Shared by VS16 promotion and
    /// mode-2027 cluster continuation (a width-bearing codepoint
    /// joining a narrow cluster makes the whole cluster wide).
    #[inline(never)]
    fn widen_prev_cell(&mut self) {
        let columns = self.grid.columns();
        // No wide pair fits on one column, so leave the base narrow: the
        // wrap branch below would place the trailing Spacer at column 1 of
        // the new row, off the end of it.
        if columns < 2 {
            return;
        }
        let row = self.grid.cursor.pos.row;
        let Some(base_col) = self.prev_cell_col() else {
            return;
        };
        if !matches!(self.grid[row][Column(base_col)].wide(), Wide::Narrow) {
            return;
        }

        let spacer_col = base_col + 1;
        if spacer_col >= columns {
            // Base is at the final column → no room for a Spacer on this
            // row. Mirror kitty's `move_widened_char_past_multiline_chars`
            // (screen.c): turn
            // the trailing cell into a `LeadingSpacer` (signals "wide char
            // continues on next line"), wrap, and re-place the wide base
            // on the new row, preserving the original cell's style and
            // any extras (zerowidth combining chars attached before VS16).
            if !self.mode.contains(Mode::LINE_WRAP) {
                return;
            }

            // Snapshot the base cell — `write_at_cursor` below replaces
            // it with a fresh `Square` and would otherwise lose the
            // codepoint, style, and extras_id we want to move.
            let base_snapshot = self.grid[row][Column(base_col)];

            self.grid.cursor.pos.col = Column(base_col);
            self.grid.cursor.should_wrap = false;
            self.write_at_cursor(' ');
            self.grid.cursor_cell().set_wide(Wide::LeadingSpacer);

            self.wrapline();

            let new_row = self.grid.cursor.pos.row;
            let mut moved = base_snapshot;
            moved.set_wide(Wide::Wide);
            self.grid[new_row][Column(0)] = moved;
            // IndexMut doesn't maintain the row hints; without them,
            // reclaim would sweep this cell's live slots.
            if moved.extras_id().is_some() {
                self.grid[new_row].has_extras = true;
            }
            if moved.carries_style() {
                self.grid[new_row].has_styles = true;
            }

            self.grid.cursor.pos.col = Column(1);
            self.write_at_cursor(' ');
            self.grid.cursor_cell().set_wide(Wide::Spacer);

            if 2 < columns {
                self.grid.cursor.pos.col = Column(2);
            } else {
                self.grid.cursor.should_wrap = true;
            }

            self.damage.damage_line(row.0 as usize);
            self.damage.damage_line(new_row.0 as usize);
            return;
        }

        self.grid[row][Column(base_col)].set_wide(Wide::Wide);

        self.grid.cursor.pos.col = Column(spacer_col);
        self.grid.cursor.should_wrap = false;
        self.write_at_cursor(' ');
        self.grid.cursor_cell().set_wide(Wide::Spacer);

        if spacer_col + 1 < columns {
            self.grid.cursor.pos.col = Column(spacer_col + 1);
        } else {
            self.grid.cursor.should_wrap = true;
        }

        self.damage.damage_line(row.0 as usize);
    }

    /// Shrink the wide pair preceding the cursor back to a narrow cell,
    /// clearing its spacer and retracting the cursor. No-op unless a
    /// wide pair is there. Validity-free counterpart of
    /// [`widen_prev_cell`]: VS15 demotion (via `apply_emoji_vs15`,
    /// which gates on the base) and mode-2027 cluster continuation
    /// (which gates on the cluster's last codepoint) share it.
    fn narrow_prev_cell(&mut self) {
        let row = self.grid.cursor.pos.row;
        let cursor_col = self.grid.cursor.pos.col.0;
        let should_wrap = self.grid.cursor.should_wrap;

        let (base_col, spacer_col) = if should_wrap {
            if cursor_col == 0 {
                return;
            }
            (cursor_col - 1, cursor_col)
        } else {
            if cursor_col < 2 {
                return;
            }
            (cursor_col - 2, cursor_col - 1)
        };

        if !matches!(self.grid[row][Column(base_col)].wide(), Wide::Wide) {
            return;
        }

        self.grid[row][Column(base_col)].set_wide(Wide::Narrow);
        self.grid[row][Column(spacer_col)] = Square::default();

        self.grid.cursor.pos.col = Column(spacer_col);
        self.grid.cursor.should_wrap = false;

        self.damage.damage_line(row.0 as usize);
    }

    /// Read the hyperlink (if any) for the cell at `(line, col)`.
    /// Looks up the cell's `extras_id` in the per-grid extras table.
    /// Used by hint matching (`find_hyperlink_matches`) to locate
    /// clickable OSC 8 link spans on screen.
    #[inline]
    pub fn cell_hyperlink(&self, line: Line, col: Column) -> Option<Hyperlink> {
        let cell = &self.grid[line][col];
        if !cell.has_hyperlink() {
            return None;
        }
        let extras_id = cell.extras_id()?;
        self.grid
            .extras_table
            .get(extras_id)
            .and_then(|e| e.hyperlink.clone())
    }

    /// Read the cell's `extras_id` if it carries a hyperlink. Cheaper
    /// than `cell_hyperlink` for span scans because it returns just the
    /// 16-bit id; matching consecutive cells is then a u16 compare.
    #[inline]
    pub fn cell_hyperlink_id(&self, line: Line, col: Column) -> Option<u16> {
        let cell = &self.grid[line][col];
        if !cell.has_hyperlink() {
            return None;
        }
        cell.extras_id()
    }

    #[inline]
    pub fn visible_rows(&self) -> Vec<Row<Square>> {
        let mut buf = Vec::with_capacity(self.grid.screen_lines());
        self.fill_visible_rows(&mut buf);
        buf
    }

    pub fn snapshot_visible(
        &mut self,
        damage: &crate::event::TerminalDamage,
        dst: &mut Vec<Row<Square>>,
        row_styles: &mut Vec<Vec<crate::crosswords::style::Style>>,
        extras: &mut rustc_hash::FxHashMap<u16, crate::crosswords::square::Extras>,
    ) {
        use crate::event::TerminalDamage;
        let (start, end) = self.visible_line_bounds();
        let count = (end - start) as usize;

        let needs_full = matches!(damage, TerminalDamage::Full)
            || dst.len() != count
            || row_styles.len() != count;

        // Styles are resolved to values per copied row, so the snapshot
        // holds no style ids at all: sweeps reusing ids, table growth,
        // and grid swaps can't affect rows copied on earlier frames.
        if needs_full {
            dst.clear();
            dst.reserve(count);
            row_styles.resize_with(count, Vec::new);
            extras.clear();
            for (i, row_idx) in (start..end).enumerate() {
                let src = &self.grid[Line(row_idx)];
                let mut copied = src.clone();
                copied.dirty = true;
                dst.push(copied);
                self.grid.resolve_row_styles(src, &mut row_styles[i]);
                self.refresh_row_extras(src, extras);
            }
            for row_idx in start..end {
                self.grid[Line(row_idx)].dirty = false;
            }
            return;
        }

        // A Noop/CursorOnly frame normally has nothing to copy, but rows
        // written after the damage event was consumed (e.g. a graphics
        // insert racing a redraw) still carry their dirty bit. Fall
        // through when any row is dirty so the snapshot can't go stale.
        if matches!(damage, TerminalDamage::Noop | TerminalDamage::CursorOnly) {
            let any_dirty = (0..count as i32).any(|y| self.grid[Line(start + y)].dirty);
            if !any_dirty {
                return;
            }
        }

        #[allow(clippy::needless_range_loop)]
        for y in 0..count {
            let row_idx = start + y as i32;
            if !self.grid[Line(row_idx)].dirty {
                continue;
            }
            let src = &self.grid[Line(row_idx)];
            dst[y].copy_from(src);
            dst[y].dirty = true;
            self.grid.resolve_row_styles(src, &mut row_styles[y]);
            self.refresh_row_extras(src, extras);
        }
        for row_idx in start..end {
            self.grid[Line(row_idx)].dirty = false;
        }
    }

    /// Insert (overwriting) every extras id referenced by `row`'s
    /// cells into `extras`. Overwrite semantics are deliberate — when
    /// Live slots are immutable under interning (writers copy-on-
    // write into fresh ids), so an overwrite here only ever swaps
    // which immutable content a row's ids resolve to.
    /// zerowidth combining codepoint appended to an existing cell),
    /// the cell's row is marked dirty by the surrounding `IndexMut`
    /// write, so any other rows referencing the same id pick up the
    /// fresh data on this row's refresh.
    fn refresh_row_extras(
        &self,
        row: &Row<Square>,
        extras: &mut rustc_hash::FxHashMap<u16, crate::crosswords::square::Extras>,
    ) {
        if !row.has_extras {
            return;
        }
        for sq in &row.inner {
            if let Some(id) = sq.extras_id_checked() {
                if let Some(live) = self.grid.extras_table.get(id) {
                    extras.insert(id, live.clone());
                }
            }
        }
    }

    /// Half-open grid line range currently on screen, as `(start, end)`.
    ///
    /// This is the full screen height offset by the scrollback position, and
    /// deliberately has nothing to do with `scroll_region`: DECSTBM bounds
    /// where scrolling happens, not what is displayed. Both were once read
    /// from `scroll_region`, so an app that narrowed it (vim, less, htop,
    /// tmux, man) lost rows off the snapshot.
    #[inline]
    fn visible_line_bounds(&self) -> (i32, i32) {
        let scroll = self.display_offset() as i32;
        (-scroll, self.grid.screen_lines() as i32 - scroll)
    }

    /// Copy the visible viewport into `dst` in place. Reuses both the
    /// outer `Vec`'s capacity and each inner `Row`'s `Vec<Square>`
    /// allocation (via [`Row::copy_from`]) so the renderer's frame
    /// buffer doesn't reallocate every frame at steady state.
    #[inline]
    pub fn fill_visible_rows(&self, dst: &mut Vec<Row<Square>>) {
        let (start, end) = self.visible_line_bounds();
        let count = (end - start) as usize;

        // Copy each visible row into the matching slot. Excess slots
        // get truncated; missing ones get pushed (with a fresh `Row`
        // each — first-frame allocation only).
        for (i, row_idx) in (start..end).enumerate() {
            let src = &self.grid[Line(row_idx)];
            if let Some(dst_row) = dst.get_mut(i) {
                dst_row.copy_from(src);
            } else {
                dst.push(src.clone());
            }
        }
        if dst.len() > count {
            dst.truncate(count);
        }
    }

    /// Get terminal dimensions
    #[inline]
    pub fn columns(&self) -> usize {
        self.grid.columns()
    }

    /// Get terminal screen lines
    #[inline]
    pub fn screen_lines(&self) -> usize {
        self.grid.screen_lines()
    }

    fn deccolm(&mut self)
    where
        U: EventListener,
    {
        // Setting 132 column font makes no sense, but run the other side effects.
        // Clear scrolling region.
        self.set_scrolling_region(1, None);

        // Clear grid.
        self.grid.sync_template_style();
        self.grid.reset_region(..);
        self.mark_fully_damaged();
    }

    /// The active xterm `modifyOtherKeys` level, or `None` when disabled.
    ///
    /// Set by `CSI > 4 ; n m`. An embedder that encodes key events needs this
    /// to know whether the application wants keys like ctrl+enter reported as
    /// `CSI 27 ; …` rather than as their legacy control bytes.
    #[inline]
    pub fn modify_other_keys(&self) -> Option<u8> {
        (self.modify_other_keys > 0).then_some(self.modify_other_keys)
    }

    /// The kitty keyboard protocol flags currently in effect, i.e. the top of
    /// the mode stack. This is the same value the terminal reports back to the
    /// application, so an embedder encoding key events itself does not have to
    /// rebuild the flag byte out of [`Mode`] bits and keep that bit order in
    /// sync by hand.
    #[inline]
    pub fn keyboard_mode(&self) -> KeyboardModes {
        KeyboardModes::from_bits_truncate(
            self.keyboard_mode_stack[self.keyboard_mode_idx],
        )
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    #[inline]
    pub fn cursor(&self) -> CursorState {
        let mut content = self.cursor_shape;
        let vi_mode = self.mode.contains(Mode::VI);
        let scroll = self.display_offset() as i32;
        let mut pos = if vi_mode {
            let mut vi_cursor_pos = self.vi_mode_cursor.pos;
            if scroll > 0 {
                vi_cursor_pos.row += scroll;
            }
            vi_cursor_pos
        } else {
            if scroll != 0 {
                content = CursorShape::Hidden;
            }
            self.grid.cursor.pos
        };
        if matches!(self.grid[pos].wide(), Wide::Spacer) {
            pos.col -= 1;
        }

        // If the cursor is hidden then set content as hidden
        if !vi_mode && !self.mode.contains(Mode::SHOW_CURSOR) {
            content = CursorShape::Hidden;
        }

        CursorState { pos, content }
    }

    pub fn swap_alt(&mut self) {
        if !self.mode.contains(Mode::ALT_SCREEN) {
            // Set alt screen cursor to the current primary screen cursor.
            self.inactive_grid.cursor = self.grid.cursor.clone();

            // Style ids are local to a grid's table too: the cloned
            // template carries a primary-table id, so force the
            // `sync_template_style` below to re-intern `pending_style`
            // (the id-independent value) into the alt table.
            self.inactive_grid.cursor.style_dirty = true;

            // Extras ids are local to a grid's table. If the template
            // carries a hyperlink (OSC 8 open across the screen
            // switch), re-intern it into the alt table; a foreign id
            // would resolve against the wrong table on every cell the
            // alt screen writes.
            if let Some(id) = self.inactive_grid.cursor.template.extras_id() {
                let hyperlink = self
                    .grid
                    .extras_table
                    .get(id)
                    .and_then(|extras| extras.hyperlink.clone());
                let new_id = hyperlink.map(|hyperlink| {
                    self.inactive_grid
                        .alloc_extras(crate::crosswords::square::Extras {
                            hyperlink: Some(hyperlink),
                            ..Default::default()
                        })
                });
                self.inactive_grid
                    .cursor
                    .template
                    .set_extras_id(new_id.filter(|&id| id != 0));
            }

            // Drop information about the primary screens saved cursor.
            self.grid.saved_cursor = self.grid.cursor.clone();

            // Reset alternate screen contents.
            self.inactive_grid.sync_template_style();
            self.inactive_grid.reset_region(..);
            // The alt screen starts blank: sixel/iTerm2 placements
            // stashed from a previous alt session die with its
            // contents (DEC grid-plane semantics; kitty state
            // intentionally persists per screen).
            let stale = &mut self.graphics.kitty_inactive_screen;
            if !stale.atlas_placements.is_empty() {
                let keys: Vec<u64> = stale.atlas_key_refs.keys().copied().collect();
                {
                    let mut removals = self.graphics.texture_operations.lock();
                    removals.extend(keys.iter().copied());
                }
                stale.atlas_placements.clear();
                stale.atlas_key_refs.clear();
                // The bytes go back to the budget with the textures.
                self.graphics.untrack_atlas_keys(&keys);
                self.send_graphics_updates();
            }
        } else {
            // Leaving alt screen: the full-screen app that set any OSC
            // color overrides is exiting, so drop them.
            self.reset_all_colors();
        }

        mem::swap(
            &mut self.keyboard_mode_stack,
            &mut self.inactive_keyboard_mode_stack,
        );
        mem::swap(
            &mut self.keyboard_mode_idx,
            &mut self.inactive_keyboard_mode_idx,
        );

        mem::swap(&mut self.grid, &mut self.inactive_grid);
        self.mode ^= Mode::ALT_SCREEN;
        self.selection = None;

        // Swap kitty graphics state per screen so each screen owns its
        // own image cache, placements, number map, and virtual placements.
        // (Marks the overlay layer dirty as a side effect so the renderer
        // rebuilds against the new active screen.)
        if self.graphics.swap_kitty_screen_state() {
            self.send_graphics_updates();
        }
        self.mark_fully_damaged();
    }

    /// Drop every per-pane OSC color override. The bg reset is pushed
    /// to the frontend so the window background (derived from OSC 11)
    /// follows. Caller is responsible for damage.
    fn reset_all_colors(&mut self) {
        let bg = NamedColor::Background as usize;
        let had_bg_override = self.colors[bg].is_some();

        self.colors = TermColors::default();

        if had_bg_override {
            self.event_proxy.send_event(
                RioEvent::ColorChange(self.route_id, bg, None),
                self.window_id,
            );
        }
    }

    #[inline]
    pub fn mark_line_damaged(&mut self, line: Line) {
        let line_idx = line.0 as usize;
        self.damage.damage_line(line_idx);
    }

    /// Record the current color scheme so a later `CSI ? 996 n` query
    /// gets the right answer. Does not notify the app.
    pub fn set_color_scheme(&mut self, is_dark: bool) {
        self.color_scheme_is_dark = is_dark;
    }

    /// If an app subscribed via DECSET 2031, push an unsolicited
    /// scheme-change DSR so it re-queries colors without a restart.
    pub fn report_color_scheme(&self, is_dark: bool) {
        if !self.mode.contains(Mode::COLOR_SCHEME_UPDATES) {
            return;
        }
        self.send_color_scheme_dsr(is_dark);
    }

    /// Reply to the DSR query `CSI ? 996 n` with the current scheme.
    /// Not gated by DECSET 2031 — it's a direct request.
    fn reply_color_scheme_query(&self) {
        self.send_color_scheme_dsr(self.color_scheme_is_dark);
    }

    fn send_color_scheme_dsr(&self, is_dark: bool) {
        let polarity = if is_dark { 1 } else { 2 };
        self.event_proxy.send_event(
            RioEvent::PtyWrite(self.route_id, format!("\x1b[?997;{polarity}n")),
            self.window_id,
        );
    }

    pub fn selection_to_string(&self) -> Option<String> {
        let selection_range = self.selection.as_ref().and_then(|s| s.to_range(self))?;
        let SelectionRange { start, end, .. } = selection_range;

        let mut res = String::new();

        match self.selection.as_ref() {
            Some(Selection {
                ty: SelectionType::Block,
                ..
            }) => {
                for line in (start.row.0..end.row.0).map(Line::from) {
                    res +=
                        &self.line_to_string(line, start.col..end.col, start.col.0 != 0);
                    res += "\n";
                }

                res += &self.line_to_string(end.row, start.col..end.col, true);
            }
            Some(Selection {
                ty: SelectionType::Lines,
                ..
            }) => {
                res = self.bounds_to_string(start, end) + "\n";
            }
            _ => {
                res = self.bounds_to_string(start, end);
            }
        }

        Some(res)
    }

    pub fn bounds_to_string(&self, start: Pos, end: Pos) -> String {
        let mut text = String::new();
        let mut blank_rows: usize = 0;
        let mut blank_cells: usize = 0;
        let last_col = self.grid.last_column();

        for line in (start.row.0..=end.row.0).map(Line::from) {
            let start_col = if line == start.row {
                start.col
            } else {
                Column(0)
            };
            let end_col = if line == end.row { end.col } else { last_col };

            // Carry buffered blank cells across wrap continuations only.
            // Without this, `aaa \n aaa"` (where row N wraps into N+1) would
            // collapse the cross-row gap from two spaces to one.
            let is_wrap_continuation =
                line.0 > start.row.0 && self.grid[line - 1i32][last_col].wrapline();
            if !is_wrap_continuation {
                blank_cells = 0;
            }

            let mut row_text = String::new();
            let had_content = self.append_cells(
                &mut row_text,
                line,
                start_col..end_col,
                line == end.row,
                &mut blank_cells,
            );

            if !had_content {
                // Defer entirely-blank rows; trailing blank rows get dropped.
                blank_rows += 1;
                continue;
            }

            for _ in 0..blank_rows {
                text.push('\n');
            }
            blank_rows = 0;

            text.push_str(&row_text);

            let cur_wraps = self.grid[line][last_col].wrapline();
            if end_col >= last_col && !cur_wraps {
                text.push('\n');
                blank_cells = 0;
            }
        }

        text.strip_suffix('\n').map(str::to_owned).unwrap_or(text)
    }

    /// Convert a single line in the grid to a String. Used by Block selection;
    /// trailing blank cells are dropped. No trailing newline is appended —
    /// the caller controls row separation.
    fn line_to_string(
        &self,
        line: Line,
        cols: Range<Column>,
        include_wrapped_wide: bool,
    ) -> String {
        let mut text = String::new();
        let mut blank_cells = 0;
        self.append_cells(
            &mut text,
            line,
            cols,
            include_wrapped_wide,
            &mut blank_cells,
        );
        text
    }

    /// Append cells from a single line to `text`, buffering blank cells
    /// (`\0` and trailing spaces) so that:
    /// - `\0` cells inside a run of content become real spaces
    /// - trailing blanks at end of the run are dropped (caller decides
    ///   whether to flush them via the `blank_cells` accumulator)
    ///
    /// Returns true if the line emitted any non-blank content.
    fn append_cells(
        &self,
        text: &mut String,
        line: Line,
        mut cols: Range<Column>,
        include_wrapped_wide: bool,
        blank_cells: &mut usize,
    ) -> bool {
        let mut had_content = false;
        let grid_line = &self.grid[line];
        let line_length = std::cmp::min(grid_line.line_length(), cols.end + 1);

        // Include wide char when trailing spacer is selected.
        if cols.start < self.grid.columns()
            && matches!(grid_line[cols.start].wide(), Wide::Spacer)
        {
            cols.start -= 1;
        }

        let mut tab_mode = false;
        for column in (cols.start.0..line_length.0).map(Column::from) {
            let cell = &grid_line[column];

            // Skip over cells until next tab-stop once a tab was found.
            if tab_mode {
                if self.tabs[column] || cell.c() != '\0' {
                    tab_mode = false;
                } else {
                    continue;
                }
            }

            if cell.c() == '\t' {
                tab_mode = true;
            }

            if matches!(cell.wide(), Wide::Spacer | Wide::LeadingSpacer) {
                continue;
            }

            let c = cell.c();
            let has_extras = cell.has_extras();

            // Buffer blank cells. They only get emitted as real spaces if a
            // non-blank cell follows (on this row or a wrap continuation).
            if !has_extras && (c == '\0' || c == ' ') {
                *blank_cells += 1;
                continue;
            }

            for _ in 0..*blank_cells {
                text.push(' ');
            }
            *blank_cells = 0;

            text.push(c);
            if let Some(extras_id) = cell.extras_id_checked() {
                if let Some(extras) = self.grid.extras_table.get(extras_id) {
                    for c in &extras.zerowidth {
                        text.push(*c);
                    }
                }
            }
            had_content = true;
        }

        // If wide char is not part of the selection, but leading spacer is, include it.
        if line_length == self.grid.columns()
            && line_length.0 >= 2
            && matches!(grid_line[line_length - 1].wide(), Wide::LeadingSpacer)
            && include_wrapped_wide
        {
            for _ in 0..*blank_cells {
                text.push(' ');
            }
            *blank_cells = 0;
            text.push(self.grid[line - 1i32][Column(0)].c());
            had_content = true;
        }

        had_content
    }

    #[inline]
    fn set_keyboard_mode(&mut self, mode: u8, apply: KeyboardModesApplyBehavior) {
        // println!("{:?}", mode);
        let active_mode = self.keyboard_mode_stack[self.keyboard_mode_idx];
        let new_mode = match apply {
            KeyboardModesApplyBehavior::Replace => mode,
            KeyboardModesApplyBehavior::Union => active_mode | mode,
            KeyboardModesApplyBehavior::Difference => active_mode & !mode,
        };
        info!("Setting keyboard mode to {new_mode:?}");
        self.keyboard_mode_stack[self.keyboard_mode_idx] = new_mode;

        // Sync self.mode with keyboard_mode_stack
        self.mode &= !Mode::KITTY_KEYBOARD_PROTOCOL;
        self.mode |= Mode::from(KeyboardModes::from_bits_truncate(new_mode));
    }

    /// Find the beginning of the current line across linewraps.
    pub fn row_search_left(&self, mut point: Pos) -> Pos {
        while point.row > self.grid.topmost_line()
            && self.grid[point.row - 1i32][self.grid.last_column()].wrapline()
        {
            point.row -= 1;
        }

        point.col = Column(0);

        point
    }

    /// Find the end of the current line across linewraps.
    pub fn row_search_right(&self, mut point: Pos) -> Pos {
        while point.row + 1 < self.grid.screen_lines()
            && self.grid[point.row][self.grid.last_column()].wrapline()
        {
            point.row += 1;
        }

        point.col = self.grid.last_column();

        point
    }
}

impl<U: EventListener> Crosswords<U> {
    /// Ids of graphics still displayed (atlas placements + kitty).
    fn collect_used_graphic_ids(&mut self) -> std::collections::HashSet<u64> {
        self.graphics.collect_active_graphic_ids()
    }

    fn cleanup_unused_kitty_images(&mut self) {
        // Collect all currently used graphic IDs from the grid
        let used_ids = self.collect_used_graphic_ids();

        // Convert to u32 for kitty_images map
        let used_kitty_ids: std::collections::HashSet<u32> =
            used_ids.iter().map(|&id| id as u32).collect();

        // Delete images not in use
        self.graphics
            .delete_kitty_images(|id, _| !used_kitty_ids.contains(id));
    }
}

impl<U: EventListener> Handler for Crosswords<U> {
    #[inline]
    fn set_mode(&mut self, mode: AnsiMode) {
        let mode = match mode {
            AnsiMode::Named(mode) => mode,
            AnsiMode::Unknown(mode) => {
                debug!("Ignoring unknown mode {} in set_mode", mode);
                return;
            }
        };

        trace!("Setting public mode: {:?}", mode);
        match mode {
            NamedMode::Insert => self.mode.insert(Mode::INSERT),
            NamedMode::LineFeedNewLine => self.mode.insert(Mode::LINE_FEED_NEW_LINE),
        }
    }

    #[inline]
    fn unset_mode(&mut self, mode: AnsiMode) {
        let mode = match mode {
            AnsiMode::Named(mode) => mode,
            AnsiMode::Unknown(mode) => {
                debug!("Ignoring unknown mode {} in unset_mode", mode);
                return;
            }
        };

        trace!("Setting public mode: {:?}", mode);
        match mode {
            NamedMode::Insert => {
                self.mode.remove(Mode::INSERT);
                self.mark_fully_damaged();
            }
            NamedMode::LineFeedNewLine => self.mode.remove(Mode::LINE_FEED_NEW_LINE),
        }
    }

    #[inline]
    fn report_mode(&mut self, mode: AnsiMode) {
        trace!("Reporting mode {mode:?}");
        let state = match mode {
            AnsiMode::Named(mode) => match mode {
                NamedMode::Insert => self.mode.contains(Mode::INSERT).into(),
                NamedMode::LineFeedNewLine => {
                    self.mode.contains(Mode::LINE_FEED_NEW_LINE).into()
                }
            },
            AnsiMode::Unknown(_) => ModeState::NotSupported,
        };

        self.event_proxy.send_event(
            RioEvent::PtyWrite(
                self.route_id,
                format!("\x1b[{};{}$y", mode.raw(), state as u8,),
            ),
            self.window_id,
        );
    }

    #[inline]
    fn set_private_mode(&mut self, mode: PrivateMode) {
        let mode = match mode {
            PrivateMode::Named(mode) => mode,

            // SixelDisplay
            PrivateMode::Unknown(80) => {
                self.mode.insert(Mode::SIXEL_DISPLAY);
                return;
            }

            // SixelPrivateColorRegisters
            PrivateMode::Unknown(1070) => {
                self.mode.insert(Mode::SIXEL_PRIV_PALETTE);
                return;
            }

            // SixelCursorToTheRight
            PrivateMode::Unknown(8452) => {
                self.mode.insert(Mode::SIXEL_CURSOR_TO_THE_RIGHT);
                return;
            }

            PrivateMode::Unknown(mode) => {
                debug!("Ignoring unknown mode {} in set_private_mode", mode);
                return;
            }
        };

        trace!("Setting private mode: {:?}", mode);
        match mode {
            NamedPrivateMode::UrgencyHints => self.mode.insert(Mode::URGENCY_HINTS),
            NamedPrivateMode::SwapScreenAndSetRestoreCursor => {
                if !self.mode.contains(Mode::ALT_SCREEN) {
                    self.swap_alt();
                }
            }
            NamedPrivateMode::ShowCursor => self.mode.insert(Mode::SHOW_CURSOR),
            NamedPrivateMode::CursorKeys => self.mode.insert(Mode::APP_CURSOR),
            // Mouse protocols are mutually exclusive.
            NamedPrivateMode::ReportX10MouseClicks => {
                self.mode.remove(Mode::MOUSE_MODE);
                self.mode.insert(Mode::MOUSE_REPORT_X10);
                self.event_proxy
                    .send_event(RioEvent::MouseCursorDirty, self.window_id);
            }
            NamedPrivateMode::ReportMouseClicks => {
                self.mode.remove(Mode::MOUSE_MODE);
                self.mode.insert(Mode::MOUSE_REPORT_CLICK);
                self.event_proxy
                    .send_event(RioEvent::MouseCursorDirty, self.window_id);
            }
            NamedPrivateMode::ReportCellMouseMotion => {
                self.mode.remove(Mode::MOUSE_MODE);
                self.mode.insert(Mode::MOUSE_DRAG);
                self.event_proxy
                    .send_event(RioEvent::MouseCursorDirty, self.window_id);
            }
            NamedPrivateMode::ReportAllMouseMotion => {
                self.mode.remove(Mode::MOUSE_MODE);
                self.mode.insert(Mode::MOUSE_MOTION);
                self.event_proxy
                    .send_event(RioEvent::MouseCursorDirty, self.window_id);
            }
            NamedPrivateMode::ReportFocusInOut => self.mode.insert(Mode::FOCUS_IN_OUT),
            NamedPrivateMode::BracketedPaste => self.mode.insert(Mode::BRACKETED_PASTE),
            // Mouse encodings are mutually exclusive.
            NamedPrivateMode::SgrMouse => {
                self.mode.remove(Mode::UTF8_MOUSE);
                self.mode.insert(Mode::SGR_MOUSE);
            }
            NamedPrivateMode::Utf8Mouse => {
                self.mode.remove(Mode::SGR_MOUSE);
                self.mode.insert(Mode::UTF8_MOUSE);
            }
            NamedPrivateMode::AlternateScroll => self.mode.insert(Mode::ALTERNATE_SCROLL),
            NamedPrivateMode::LineWrap => self.mode.insert(Mode::LINE_WRAP),
            NamedPrivateMode::Origin => self.mode.insert(Mode::ORIGIN),
            NamedPrivateMode::ColumnMode => self.deccolm(),
            NamedPrivateMode::BlinkingCursor => {
                self.blinking_cursor = true;
                self.event_proxy
                    .send_event(RioEvent::CursorBlinkingChange, self.window_id);
            }
            NamedPrivateMode::GraphemeCluster => self.mode.insert(Mode::GRAPHEME_CLUSTER),
            NamedPrivateMode::ColorSchemeUpdates => {
                self.mode.insert(Mode::COLOR_SCHEME_UPDATES)
            }
            NamedPrivateMode::SyncUpdate => (),
        }
    }

    #[inline]
    fn unset_private_mode(&mut self, mode: PrivateMode) {
        let mode = match mode {
            PrivateMode::Named(mode) => mode,

            // SixelDisplay
            PrivateMode::Unknown(80) => {
                self.mode.remove(Mode::SIXEL_DISPLAY);
                return;
            }

            // SixelPrivateColorRegisters
            PrivateMode::Unknown(1070) => {
                self.graphics.sixel_shared_palette = None;
                self.mode.remove(Mode::SIXEL_PRIV_PALETTE);
                return;
            }

            // SixelCursorToTheRight
            PrivateMode::Unknown(8452) => {
                self.mode.remove(Mode::SIXEL_CURSOR_TO_THE_RIGHT);
                return;
            }

            PrivateMode::Unknown(mode) => {
                debug!("Ignoring unknown mode {} in unset_private_mode", mode);
                return;
            }
        };

        trace!("Unsetting private mode: {:?}", mode);
        match mode {
            NamedPrivateMode::UrgencyHints => self.mode.remove(Mode::URGENCY_HINTS),
            NamedPrivateMode::SwapScreenAndSetRestoreCursor => {
                if self.mode.contains(Mode::ALT_SCREEN) {
                    self.swap_alt();
                }
            }
            NamedPrivateMode::ShowCursor => self.mode.remove(Mode::SHOW_CURSOR),
            NamedPrivateMode::CursorKeys => self.mode.remove(Mode::APP_CURSOR),
            // The mouse protocols are one setting, so resetting any of them
            // turns reporting off. There is no protocol underneath to fall
            // back to: setting one already cleared the others.
            NamedPrivateMode::ReportX10MouseClicks
            | NamedPrivateMode::ReportMouseClicks
            | NamedPrivateMode::ReportCellMouseMotion
            | NamedPrivateMode::ReportAllMouseMotion => {
                self.mode.remove(Mode::MOUSE_MODE);
                self.event_proxy
                    .send_event(RioEvent::MouseCursorDirty, self.window_id);
            }
            NamedPrivateMode::ReportFocusInOut => self.mode.remove(Mode::FOCUS_IN_OUT),
            NamedPrivateMode::BracketedPaste => self.mode.remove(Mode::BRACKETED_PASTE),
            NamedPrivateMode::SgrMouse => self.mode.remove(Mode::SGR_MOUSE),
            NamedPrivateMode::Utf8Mouse => self.mode.remove(Mode::UTF8_MOUSE),
            NamedPrivateMode::AlternateScroll => self.mode.remove(Mode::ALTERNATE_SCROLL),
            NamedPrivateMode::LineWrap => self.mode.remove(Mode::LINE_WRAP),
            NamedPrivateMode::Origin => self.mode.remove(Mode::ORIGIN),
            NamedPrivateMode::ColumnMode => self.deccolm(),
            NamedPrivateMode::BlinkingCursor => {
                self.blinking_cursor = false;
                self.event_proxy
                    .send_event(RioEvent::CursorBlinkingChange, self.window_id);
            }
            NamedPrivateMode::GraphemeCluster => self.mode.remove(Mode::GRAPHEME_CLUSTER),
            NamedPrivateMode::ColorSchemeUpdates => {
                self.mode.remove(Mode::COLOR_SCHEME_UPDATES)
            }
            NamedPrivateMode::SyncUpdate => (),
        }
    }

    #[inline]
    fn report_private_mode(&mut self, mode: PrivateMode) {
        info!("Reporting private mode {mode:?}");
        let state = match mode {
            PrivateMode::Named(mode) => match mode {
                NamedPrivateMode::CursorKeys => {
                    self.mode.contains(Mode::APP_CURSOR).into()
                }
                NamedPrivateMode::Origin => self.mode.contains(Mode::ORIGIN).into(),
                NamedPrivateMode::LineWrap => self.mode.contains(Mode::LINE_WRAP).into(),
                NamedPrivateMode::BlinkingCursor => self.blinking_cursor.into(),
                NamedPrivateMode::ShowCursor => {
                    self.mode.contains(Mode::SHOW_CURSOR).into()
                }
                NamedPrivateMode::ReportX10MouseClicks => {
                    self.mode.contains(Mode::MOUSE_REPORT_X10).into()
                }
                NamedPrivateMode::ReportMouseClicks => {
                    self.mode.contains(Mode::MOUSE_REPORT_CLICK).into()
                }
                NamedPrivateMode::ReportCellMouseMotion => {
                    self.mode.contains(Mode::MOUSE_DRAG).into()
                }
                NamedPrivateMode::ReportAllMouseMotion => {
                    self.mode.contains(Mode::MOUSE_MOTION).into()
                }
                NamedPrivateMode::ReportFocusInOut => {
                    self.mode.contains(Mode::FOCUS_IN_OUT).into()
                }
                NamedPrivateMode::Utf8Mouse => {
                    self.mode.contains(Mode::UTF8_MOUSE).into()
                }
                NamedPrivateMode::SgrMouse => self.mode.contains(Mode::SGR_MOUSE).into(),
                NamedPrivateMode::AlternateScroll => {
                    self.mode.contains(Mode::ALTERNATE_SCROLL).into()
                }
                NamedPrivateMode::UrgencyHints => {
                    self.mode.contains(Mode::URGENCY_HINTS).into()
                }
                NamedPrivateMode::SwapScreenAndSetRestoreCursor => {
                    self.mode.contains(Mode::ALT_SCREEN).into()
                }
                NamedPrivateMode::BracketedPaste => {
                    self.mode.contains(Mode::BRACKETED_PASTE).into()
                }
                NamedPrivateMode::GraphemeCluster => {
                    if self.mode.contains(Mode::GRAPHEME_CLUSTER) {
                        ModeState::Set
                    } else {
                        ModeState::Reset
                    }
                }
                NamedPrivateMode::ColorSchemeUpdates => {
                    self.mode.contains(Mode::COLOR_SCHEME_UPDATES).into()
                }
                NamedPrivateMode::SyncUpdate => ModeState::Reset,
                NamedPrivateMode::ColumnMode => ModeState::NotSupported,
            },
            PrivateMode::Unknown(_) => ModeState::NotSupported,
        };

        self.event_proxy.send_event(
            RioEvent::PtyWrite(
                self.route_id,
                format!("\x1b[?{};{}$y", mode.raw(), state as u8,),
            ),
            self.window_id,
        );
    }

    #[inline]
    fn dynamic_color_sequence(&mut self, prefix: String, index: usize, terminator: &str) {
        debug!(
            "Requested write of escape sequence for color code {}: color[{}]",
            prefix, index
        );

        let terminator = terminator.to_owned();
        self.event_proxy.send_event(
            RioEvent::ColorRequest(
                self.route_id,
                index,
                Arc::new(move |color| {
                    format!(
                        "\x1b]{};rgb:{1:02x}{1:02x}/{2:02x}{2:02x}/{3:02x}{3:02x}{4}",
                        prefix, color.r, color.g, color.b, terminator
                    )
                }),
            ),
            self.window_id,
        );
    }

    #[inline]
    fn goto(&mut self, line: Line, col: Column) {
        trace!("Going to: line={}, col={}", line, col);
        let (y_offset, max_y) = if self.mode.contains(Mode::ORIGIN) {
            (self.scroll_region.start, self.scroll_region.end - 1)
        } else {
            (Line(0), self.grid.bottommost_line())
        };

        self.damage_cursor();
        self.grid.cursor.pos.row =
            std::cmp::max(std::cmp::min(line + y_offset, max_y), Line(0));
        self.grid.cursor.pos.col = std::cmp::min(col, self.grid.last_column());
        self.damage_cursor();
        self.grid.cursor.should_wrap = false;
    }

    #[inline]
    fn set_active_charset(&mut self, index: CharsetIndex) {
        self.active_charset = index;
    }

    #[inline]
    fn move_forward(&mut self, cols: Column) {
        let last_column =
            std::cmp::min(self.grid.cursor.pos.col + cols, self.grid.last_column());

        let cursor_line = self.grid.cursor.pos.row.0 as usize;
        self.damage.damage_line(cursor_line);

        self.grid.cursor.pos.col = last_column;
        self.grid.cursor.should_wrap = false;
    }

    #[inline]
    fn move_backward(&mut self, cols: Column) {
        let column = self.grid.cursor.pos.col.saturating_sub(cols.0);

        let cursor_line = self.grid.cursor.pos.row.0 as usize;
        self.damage.damage_line(cursor_line);

        self.grid.cursor.pos.col = Column(column);
        self.grid.cursor.should_wrap = false;
    }

    #[inline]
    fn move_backward_tabs(&mut self, count: u16) {
        trace!("Moving backward {} tabs", count);

        for _ in 0..count {
            let mut col = self.grid.cursor.pos.col;

            if col == 0 {
                break;
            }

            for i in (0..(col.0)).rev() {
                if self.tabs[Column(i)] {
                    col = Column(i);
                    break;
                }
            }
            self.grid.cursor.pos.col = col;
        }

        let line = self.grid.cursor.pos.row.0 as usize;
        self.damage.damage_line(line);
    }

    #[inline]
    fn goto_line(&mut self, line: Line) {
        self.goto(line, self.grid.cursor.pos.col)
    }

    #[inline]
    fn goto_col(&mut self, col: Column) {
        self.goto(self.grid.cursor.pos.row, col)
    }

    #[inline]
    fn decaln(&mut self) {
        for line in (0..self.grid.screen_lines()).map(Line::from) {
            for column in 0..self.grid.columns() {
                let cell = &mut self.grid[line][Column(column)];
                *cell = Square::default();
                cell.set_c('E');
            }
        }

        self.mark_fully_damaged();
    }

    #[inline]
    fn move_up(&mut self, rows: usize) {
        self.goto(self.grid.cursor.pos.row - rows, self.grid.cursor.pos.col)
    }

    #[inline]
    fn move_down(&mut self, rows: usize) {
        self.goto(self.grid.cursor.pos.row + rows, self.grid.cursor.pos.col)
    }

    #[inline]
    fn move_down_and_cr(&mut self, rows: usize) {
        self.goto(self.grid.cursor.pos.row + rows, Column(0))
    }

    #[inline]
    fn move_up_and_cr(&mut self, lines: usize) {
        self.goto(self.grid.cursor.pos.row - lines, Column(0))
    }

    #[inline]
    fn scroll_up(&mut self, lines: usize) {
        let origin = self.scroll_region.start;
        self.scroll_up_relative(origin, lines);
    }

    #[inline]
    fn delete_lines(&mut self, lines: usize) {
        let origin = self.grid.cursor.pos.row;
        let lines = std::cmp::min(self.grid.screen_lines() - origin.0 as usize, lines);

        if lines > 0 && self.scroll_region.contains(&origin) {
            self.scroll_up_relative(origin, lines);
        }
    }

    #[inline]
    fn push_title(&mut self) {
        trace!("Pushing '{:?}' onto title stack", self.title);

        if self.title_stack.len() >= TITLE_STACK_MAX_DEPTH {
            let removed = self.title_stack.remove(0);
            trace!(
                "Removing '{:?}' from bottom of title stack that exceeds its maximum depth",
                removed
            );
        }

        self.title_stack.push(self.title.clone());
    }

    #[inline]
    fn pop_title(&mut self) {
        trace!("Attempting to pop title from stack...");

        if let Some(popped) = self.title_stack.pop() {
            trace!("Title '{:?}' popped from stack", popped);
            self.set_title(Some(popped));
        }
    }

    #[inline]
    fn erase_chars(&mut self, count: Column) {
        let start = self.grid.cursor.pos.col;
        let end = std::cmp::min(start + count, Column(self.grid.columns()));

        // Cleared cells have current background color set.
        let bg = self.grid.template_style().bg;
        let blank = self.grid.blank_with_bg(bg);
        let line = self.grid.cursor.pos.row;
        self.damage.damage_line(line.0 as usize);
        self.split_wide_seam(line, start.0, blank);
        self.split_wide_seam(line, end.0, blank);
        let row = &mut self.grid[line];
        row.has_styles |= blank.carries_style();
        for cell in &mut row[start..end] {
            *cell = blank;
        }
        self.clip_atlas_placements(line.0, line.0 + 1, start.0, end.0);
        if !self.graphics.kitty_placements.is_empty() {
            self.graphics.kitty_graphics_dirty = true;
        }
    }

    #[inline]
    fn delete_chars(&mut self, count: usize) {
        let columns = self.grid.columns();
        let bg = self.grid.template_style().bg;
        let blank = self.grid.blank_with_bg(bg);

        // Ensure deleting within terminal bounds.
        let count = std::cmp::min(count, columns);

        let start = self.grid.cursor.pos.col.0;
        let end = std::cmp::min(start + count, columns - 1);
        let num_cells = columns - end;

        let line = self.grid.cursor.pos.row;
        self.damage.damage_line(line.0 as usize);
        self.split_wide_seam(line, start, blank);
        self.split_wide_seam(line, (start + count).min(columns), blank);
        self.split_wide_seam(line, columns, blank);
        self.grid[line].has_styles |= blank.carries_style();
        let row = &mut self.grid[line][..];

        for offset in 0..num_cells {
            row.swap(start + offset, end + offset);
        }

        // Clear last `count` cells in the row. If deleting 1 char, need to delete
        // 1 cell.
        let end = columns - count;
        for cell in &mut row[end..] {
            *cell = blank;
        }
        // Image cells in the shifted tail stop showing their slices
        // (placement-model approximation, like other placement-based
        // terminals).
        self.clip_atlas_placements(line.0, line.0 + 1, start, columns);
    }

    #[inline]
    fn scroll_down(&mut self, lines: usize) {
        let origin = self.scroll_region.start;
        self.scroll_down_relative(origin, lines);
    }

    #[inline]
    fn insert_blank_lines(&mut self, lines: usize) {
        let origin = self.grid.cursor.pos.row;
        if self.scroll_region.contains(&origin) {
            self.scroll_down_relative(origin, lines);
        }
    }

    #[inline]
    fn insert_blank(&mut self, count: usize) {
        let bg = self.grid.template_style().bg;
        let blank = self.grid.blank_with_bg(bg);

        // Ensure inserting within terminal bounds
        let count =
            std::cmp::min(count, self.grid.columns() - self.grid.cursor.pos.col.0);

        let source = self.grid.cursor.pos.col;
        let destination = self.grid.cursor.pos.col.0 + count;
        let num_cells = self.grid.columns() - destination;

        let line = self.grid.cursor.pos.row;
        self.damage.damage_line(line.0 as usize);
        self.split_wide_seam(line, source.0, blank);
        self.split_wide_seam(line, self.grid.columns(), blank);
        // The last shifted cell lands on the row's final column; if it
        // is a wide lead its spacer will not survive the shift, so
        // clear the pair up front.
        if num_cells > 0 {
            let last_src = source.0 + num_cells - 1;
            if matches!(
                self.grid[line][Column(last_src)].wide(),
                crate::crosswords::square::Wide::Wide
            ) {
                self.clear_pair_preserving_wrapline(line, last_src, blank);
            }
        }

        self.grid[line].has_styles |= blank.carries_style();
        let row = &mut self.grid[line][..];

        for offset in (0..num_cells).rev() {
            row.swap(destination + offset, source.0 + offset);
        }

        // Squares were just moved out toward the end of the line;
        // fill in between source and dest with blanks.
        for cell in &mut row[source.0..destination] {
            *cell = blank;
        }
        // Placement-model approximation: image cells from the insert
        // point onward stop showing their slices.
        let columns = self.grid.columns();
        self.clip_atlas_placements(line.0, line.0 + 1, source.0, columns);
    }

    #[inline]
    fn reverse_index(&mut self) {
        // If cursor is at the top.
        if self.grid.cursor.pos.row == self.scroll_region.start {
            self.scroll_down(1);
        } else {
            self.damage_cursor();
            self.grid.cursor.pos.row =
                std::cmp::max(self.grid.cursor.pos.row - 1, Line(0));
            self.damage_cursor();
        }
    }

    #[inline]
    fn reset_state(&mut self) {
        if self.mode.contains(Mode::ALT_SCREEN) {
            std::mem::swap(&mut self.grid, &mut self.inactive_grid);
        }
        self.active_charset = Default::default();
        self.cursor_shape = self.default_cursor_shape;
        self.grid.reset();
        self.inactive_grid.reset();
        self.scroll_region = Line(0)..Line(self.grid.screen_lines() as i32);
        self.tabs = TabStops::new(self.grid.columns());
        self.title_stack = Vec::new();
        self.modify_other_keys = 0;
        self.keyboard_mode_stack = [0; KEYBOARD_MODE_STACK_MAX_DEPTH];
        self.inactive_keyboard_mode_stack = [0; KEYBOARD_MODE_STACK_MAX_DEPTH];
        self.keyboard_mode_idx = 0;
        self.inactive_keyboard_mode_idx = 0;
        self.title = String::from("");
        self.selection = None;
        self.vi_mode_cursor = Default::default();
        self.keyboard_mode_stack = Default::default();
        self.inactive_keyboard_mode_stack = Default::default();

        // Clear all graphics on full reset (both screens, kitty and
        // sixel/iTerm2) and dispatch the queued texture removals.
        self.graphics.clear_all_kitty_state();
        self.send_graphics_updates();

        // Preserve vi mode across resets.
        self.mode &= Mode::VI;
        self.mode.insert(Mode::default());
        self.mode
            .set(Mode::GRAPHEME_CLUSTER, self.grapheme_clustering_default);

        self.event_proxy
            .send_event(RioEvent::CursorBlinkingChange, self.window_id);
        self.mark_fully_damaged();
    }

    #[inline]
    fn terminal_attribute(&mut self, attr: Attr) {
        trace!("Setting attribute: {:?}", attr);
        use crate::crosswords::style::StyleFlags;
        match attr {
            Attr::Foreground(color) => self.grid.update_template_style(|s| s.fg = color),
            Attr::Background(color) => self.grid.update_template_style(|s| s.bg = color),
            Attr::UnderlineColor(color) => self
                .grid
                .update_template_style(|s| s.underline_color = color),
            Attr::Reset => self
                .grid
                .set_template_style(crate::crosswords::style::Style::default()),
            Attr::Reverse => self
                .grid
                .update_template_style(|s| s.flags.insert(StyleFlags::INVERSE)),
            Attr::CancelReverse => self
                .grid
                .update_template_style(|s| s.flags.remove(StyleFlags::INVERSE)),
            Attr::Bold => self
                .grid
                .update_template_style(|s| s.flags.insert(StyleFlags::BOLD)),
            Attr::CancelBold => self
                .grid
                .update_template_style(|s| s.flags.remove(StyleFlags::BOLD)),
            Attr::Dim => self
                .grid
                .update_template_style(|s| s.flags.insert(StyleFlags::DIM)),
            Attr::CancelBoldDim => self.grid.update_template_style(|s| {
                s.flags.remove(StyleFlags::BOLD | StyleFlags::DIM)
            }),
            Attr::Italic => self
                .grid
                .update_template_style(|s| s.flags.insert(StyleFlags::ITALIC)),
            Attr::CancelItalic => self
                .grid
                .update_template_style(|s| s.flags.remove(StyleFlags::ITALIC)),
            Attr::Underline => self.grid.update_template_style(|s| {
                s.flags.remove(StyleFlags::ALL_UNDERLINES);
                s.flags.insert(StyleFlags::UNDERLINE);
            }),
            Attr::DoubleUnderline => self.grid.update_template_style(|s| {
                s.flags.remove(StyleFlags::ALL_UNDERLINES);
                s.flags.insert(StyleFlags::DOUBLE_UNDERLINE);
            }),
            Attr::Undercurl => self.grid.update_template_style(|s| {
                s.flags.remove(StyleFlags::ALL_UNDERLINES);
                s.flags.insert(StyleFlags::UNDERCURL);
            }),
            Attr::DottedUnderline => self.grid.update_template_style(|s| {
                s.flags.remove(StyleFlags::ALL_UNDERLINES);
                s.flags.insert(StyleFlags::DOTTED_UNDERLINE);
            }),
            Attr::DashedUnderline => self.grid.update_template_style(|s| {
                s.flags.remove(StyleFlags::ALL_UNDERLINES);
                s.flags.insert(StyleFlags::DASHED_UNDERLINE);
            }),
            Attr::BlinkSlow | Attr::BlinkFast | Attr::CancelBlink => {
                info!("Term got unhandled attr: {:?}", attr);
            }
            Attr::CancelUnderline => self
                .grid
                .update_template_style(|s| s.flags.remove(StyleFlags::ALL_UNDERLINES)),
            Attr::Hidden => self
                .grid
                .update_template_style(|s| s.flags.insert(StyleFlags::HIDDEN)),
            Attr::CancelHidden => self
                .grid
                .update_template_style(|s| s.flags.remove(StyleFlags::HIDDEN)),
            Attr::Strike => self
                .grid
                .update_template_style(|s| s.flags.insert(StyleFlags::STRIKEOUT)),
            Attr::CancelStrike => self
                .grid
                .update_template_style(|s| s.flags.remove(StyleFlags::STRIKEOUT)),
            // _ => {
            // warn!("Term got unhandled attr: {:?}", attr);
            // }
        }
    }

    fn set_title(&mut self, title: Option<String>) {
        let title = title.unwrap_or_default();
        if title != self.title {
            self.title = title;
            self.event_proxy
                .send_event(RioEvent::Title(self.title.clone()), self.window_id);
        }
    }

    fn set_progress_report(&mut self, report: crate::event::ProgressReport) {
        self.event_proxy
            .send_event(RioEvent::ProgressReport(report), self.window_id);
    }

    fn set_current_directory(&mut self, path: std::path::PathBuf) {
        trace!("Setting working directory {:?}", path);
        self.current_directory = Some(path);
    }

    fn set_semantic_prompt(
        &mut self,
        mark: crate::crosswords::grid::row::SemanticPrompt,
    ) {
        let row = self.grid.cursor.pos.row;
        self.grid[row].semantic_prompt = mark;
    }

    fn set_user_var(&mut self, name: String, value: String) {
        self.user_vars.insert(name, value);
    }

    #[inline]
    fn set_cursor_style(&mut self, style: Option<CursorShape>, blinking: bool) {
        if let Some(cursor_shape) = style {
            self.cursor_shape = cursor_shape;
        } else {
            self.cursor_shape = self.default_cursor_shape;
        }

        self.blinking_cursor = blinking;
        self.event_proxy
            .send_event(RioEvent::CursorBlinkingChange, self.window_id);
    }

    #[inline]
    fn set_cursor_shape(&mut self, shape: CursorShape) {
        self.cursor_shape = shape;
    }

    #[inline]
    fn set_keypad_application_mode(&mut self) {
        trace!("Setting keypad application mode");
        self.mode.insert(Mode::APP_KEYPAD);
    }

    #[inline]
    fn unset_keypad_application_mode(&mut self) {
        trace!("Unsetting keypad application mode");
        self.mode.remove(Mode::APP_KEYPAD);
    }

    /// Store data into clipboard.
    #[inline]
    fn clipboard_store(&mut self, clipboard: u8, base64: &[u8]) {
        let clipboard_type = match clipboard {
            b'c' => ClipboardType::Clipboard,
            b'p' | b's' => ClipboardType::Selection,
            _ => return,
        };

        if let Some(bytes) = crate::simd_base64::decode(base64) {
            if let Ok(text) = simd_utf8::from_utf8_to_string(&bytes) {
                self.event_proxy.send_event(
                    RioEvent::ClipboardStore(clipboard_type, text),
                    self.window_id,
                );
            }
        }
    }

    #[inline]
    fn configure_charset(
        &mut self,
        index: pos::CharsetIndex,
        charset: pos::StandardCharset,
    ) {
        trace!("Configuring charset {:?} as {:?}", index, charset);
        self.grid.cursor.charsets[index] = charset;
    }

    #[inline(never)]
    fn input(&mut self, c: char) {
        // A Glyph Protocol registration does NOT change a codepoint's
        // logical width: the cell layout stays consistent with system
        // `wcwidth` so the cursor / selection / copy never desync with a
        // width-unaware consumer (a shell line editor, etc.). The
        // declared `width` is honoured purely at render time, where the
        // glyph overflows into the following cell(s) in pixels.
        let width = match crate::codepoint_width::codepoint_width(c as u32) {
            Some(w) => w as usize,
            None => return,
        };

        // Mode 2027: try to continue the previous cell's grapheme
        // cluster first. On a break (or nothing to continue) fall
        // through to the legacy paths: zero-width codepoints attach
        // wcwidth-style, everything else writes a fresh cell.
        //
        // Codepoints <= 0xFF never continue a cluster (the only
        // UAX29 rule this waives is Prepend x
        // Latin-1, which the bulk ASCII writer already waives; this
        // keeps scalar and bulk agreeing regardless of how the parser
        // chunks the stream) and are the common case, so that check
        // goes first.
        if c > '\u{FF}'
            && self.mode.contains(Mode::GRAPHEME_CLUSTER)
            && self.try_cluster_append(c, width)
        {
            return;
        }

        // Handle zero-width characters.
        if width == 0 {
            // Mode 2027: the cluster path above is the only legitimate
            // attach. Reaching here means there was no base to join:
            // a column-0 orphan, an empty or bg-only previous cell.
            // Attaching wcwidth-style would contradict the boundary
            // the mode just computed, so drop the codepoint.
            if self.mode.contains(Mode::GRAPHEME_CLUSTER) {
                return;
            }
            // Legacy (clustering opted out or reset by the program):
            // attach the zero-width codepoint without touching the
            // base cell's width, exactly wcwidth semantics. Variation
            // selector width effects live only in the mode-2027
            // cluster path: a width the application cannot predict
            // desyncs every wcwidth-based redraw (tmux positions its
            // cursor from its own per-codepoint math and leaves stray
            // cells behind whenever the terminal disagrees).
            //
            // Selectors only attach after an emoji base; anywhere else
            // they are presentation noise and would pollute the cell's
            // extras (copy, serialize, round-trip) for no render effect.
            if matches!(c, '\u{FE0F}' | '\u{FE0E}') {
                let emoji_base = self.prev_cell_col().is_some_and(|col| {
                    let cell = self.grid[self.grid.cursor.pos.row][Column(col)];
                    matches!(
                        cell.content_tag(),
                        crate::crosswords::square::ContentTag::Codepoint
                    ) && crate::grapheme_lut::class_of(cell.c())
                        == rio_unicode::grapheme::GraphemeClass::ExtPic as u8
                });
                if !emoji_base {
                    return;
                }
            }
            self.attach_to_prev_cell(c);
            return;
        }

        if self.grid.cursor.should_wrap {
            self.wrapline();
        }

        let columns = self.grid.columns();
        if self.mode.contains(Mode::INSERT) && self.grid.cursor.pos.col + width < columns
        {
            let line = self.grid.cursor.pos.row;
            let col = self.grid.cursor.pos.col;
            let row = &mut self.grid[line][..];

            for col in (col.0..(columns - width)).rev() {
                row.swap(col + width, col);
            }
        }

        // Set the per-row kitty placeholder flag so the renderer can
        // skip the U+10EEEE scan on rows that don't have any.
        if c == crate::ansi::kitty_virtual::PLACEHOLDER {
            let line = self.grid.cursor.pos.row;
            self.grid[line].kitty_virtual_placeholder = true;
        }

        // A one-column grid can never hold a wide pair, and the
        // wrap-for-room branch below cannot create room on it: wrapping
        // lands back on column 0, still the last column, and the Spacer
        // write runs off the end of the row. Drop the glyph for a blank
        // narrow cell, matching `write_codepoint_cell` on the same
        // degenerate size.
        let (c, width) = if width == 2 && columns < 2 {
            (' ', 1)
        } else {
            (c, width)
        };

        if width == 1 {
            self.write_at_cursor(c);
        } else {
            if self.grid.cursor.pos.col + 1 >= columns {
                if self.mode.contains(Mode::LINE_WRAP) {
                    // Insert placeholder before wide char if glyph does not
                    // fit in this row. Mark it as LeadingSpacer post-write.
                    self.write_at_cursor(' ');
                    self.grid.cursor_cell().set_wide(Wide::LeadingSpacer);
                    self.wrapline();
                } else {
                    // Prevent out of bounds crash when linewrapping is disabled.
                    self.grid.cursor.should_wrap = true;
                    return;
                }
            }

            // Wide character itself.
            self.write_at_cursor(c);
            self.grid.cursor_cell().set_wide(Wide::Wide);

            // Spacer cell after it.
            self.grid.cursor.pos.col += 1;
            self.write_at_cursor(' ');
            self.grid.cursor_cell().set_wide(Wide::Spacer);
        }

        // Mark cursor line as damaged for partial rendering
        let cursor_line = self.grid.cursor.pos.row.0 as usize;
        self.damage.damage_line(cursor_line);

        if self.grid.cursor.pos.col + 1 < columns {
            self.grid.cursor.pos.col += 1;
        } else {
            self.grid.cursor.should_wrap = true;
        }
    }

    fn input_codepoints(&mut self, codepoints: &[u32]) {
        // Insert mode falls back: cell-rotation is per-char and the cost of
        // bulk-handling it correctly outweighs the win. Non-ASCII charsets
        // fall back too: decode runs can carry ASCII that needs mapping.
        let active = self.grid.cursor.charsets[self.active_charset];
        if self.mode.contains(Mode::INSERT)
            || active != crate::crosswords::pos::StandardCharset::Ascii
            // Grapheme clustering decides cell layout per codepoint
            // against the previous cell; the bulk writers place whole
            // width-runs at once and would split clusters. ASCII bulk
            // paths stay fast: ASCII never continues a cluster, and
            // the cluster check anchors on the previous cell, so it
            // self-invalidates across a bulk run.
            || self.mode.contains(Mode::GRAPHEME_CLUSTER)
        {
            for &cp in codepoints {
                let c = char::from_u32(cp).unwrap_or('\u{FFFD}');
                self.input(c);
            }
            return;
        }

        // Registered glyphs keep their system `wcwidth` here (see the
        // note in `input`); the declared width is a render-time hint
        // only, applied as pixel overflow in the renderer.
        let table = crate::codepoint_width::width_table();

        // Image placements can clip cells, which the scalar writer owns.
        if !self.graphics.atlas_placements.is_empty() {
            for &cp in codepoints {
                let width = match crate::codepoint_width::width_in(table, cp) {
                    Some(w) => w,
                    None => continue,
                };
                let c = char::from_u32(cp).unwrap_or('\u{FFFD}');
                if width == 0 {
                    self.input(c);
                } else {
                    self.write_codepoint_cell(cp, c, width);
                }
            }
            return;
        }

        self.grid.sync_template_style();
        let template = self.cell_template();
        let template_has_extras = template.extras_id().is_some();

        let mut i = 0;
        let n = codepoints.len();
        while i < n {
            let cp = codepoints[i];
            let width = match crate::codepoint_width::width_in(table, cp) {
                Some(w) => w,
                None => {
                    i += 1;
                    continue;
                }
            };

            if width == 0 {
                // Combining marks, VS15/VS16. Defer to scalar `input` which
                // owns the emoji-presentation flip and grapheme-extension
                // logic (attaches to preceding cell rather than writing a
                // new one).
                let c = char::from_u32(cp).unwrap_or('\u{FFFD}');
                self.input(c);
                i += 1;
                continue;
            }

            if cp == crate::ansi::kitty_virtual::PLACEHOLDER as u32 {
                let c = char::from_u32(cp).unwrap_or('\u{FFFD}');
                self.write_codepoint_cell(cp, c, width);
                i += 1;
                continue;
            }

            // Maximal run of same-width codepoints, written in bulk.
            let mut j = i + 1;
            while j < n {
                let next = codepoints[j];
                if next == crate::ansi::kitty_virtual::PLACEHOLDER as u32
                    || crate::codepoint_width::width_in(table, next) != Some(width)
                {
                    break;
                }
                j += 1;
            }

            if width == 1 {
                self.write_narrow_run(template, template_has_extras, &codepoints[i..j]);
            } else {
                self.write_wide_run(template, template_has_extras, &codepoints[i..j]);
            }
            i = j;
        }
    }

    fn input_str(&mut self, s: &str) {
        if !s.is_ascii() {
            for c in s.chars() {
                self.input(c);
            }
            return;
        }
        self.input_ascii_str(s);
    }

    fn input_ascii_str(&mut self, s: &str) {
        // Fast path: ASCII printable runs are the common case (vim redraws,
        // log tails, prompt rendering). Side-step the per-char `input()`
        // dispatch which does width lookup, wide-char/zero-width checks,
        // kitty placeholder bookkeeping, and per-byte wrap branching.
        // The caller guarantees `s` is ASCII; charset mapping and insert
        // mode still defer to the scalar path.
        debug_assert!(s.is_ascii());
        let active = self.grid.cursor.charsets[self.active_charset];
        if active != crate::crosswords::pos::StandardCharset::Ascii
            || self.mode.contains(Mode::INSERT)
        {
            for c in s.chars() {
                self.input(c);
            }
            return;
        }

        // The cursor template (colors + flags) is constant across a printable
        // run, and ASCII needs no charset map — build the cell template once
        // and write each cell as a single packed store.
        self.grid.sync_template_style();
        let template = self.cell_template();
        // Two per-run invariants, hoisted out of the cell loop below: whether
        // any image placement could clip a cell, and whether the template
        // carries extras (so `has_extras` is set once for the whole run).
        let has_atlas = !self.graphics.atlas_placements.is_empty();
        let template_has_extras = template.extras_id().is_some();

        let bytes = s.as_bytes();
        let mut idx = 0;
        while idx < bytes.len() {
            if self.grid.cursor.should_wrap {
                if !self.mode.contains(Mode::LINE_WRAP) {
                    // LINE_WRAP off: cursor is parked on the last column and
                    // each new char overwrites that cell. Defer to scalar
                    // `input()` for those exact semantics.
                    for c in s[idx..].chars() {
                        self.input(c);
                    }
                    return;
                }
                self.wrapline();
            }

            let columns = self.grid.columns();
            let cursor_col = self.grid.cursor.pos.col.0;
            let remaining_in_row = columns.saturating_sub(cursor_col);
            if remaining_in_row == 0 {
                for c in s[idx..].chars() {
                    self.input(c);
                }
                return;
            }

            let take = (bytes.len() - idx).min(remaining_in_row);

            // Bulk fill: when no image can clip this run, classify the target
            // cells and, if none needs wide-char cleanup, store the whole run
            // as `template | codepoint`. Splitting the scan from the fill lets
            // the compiler vectorize both, versus the branchy per-cell path.
            let mut wrote_bulk = false;
            if !has_atlas {
                let line = self.grid.cursor.pos.row;
                let row = &mut self.grid[line];
                let cells = &mut row[Column(cursor_col)..Column(cursor_col + take)];
                // Branchless fold; an early-exit any() does not vectorize.
                let needs_cleanup = cells
                    .iter()
                    .fold(false, |acc, c| acc | c.needs_wide_cleanup());
                if !needs_cleanup {
                    let src = &bytes[idx..idx + take];
                    for (cell, &b) in cells.iter_mut().zip(src) {
                        *cell = crate::crosswords::square::Square::from_template(
                            template, b as char,
                        );
                    }
                    if template_has_extras {
                        row.has_extras = true;
                    }
                    if template.carries_style() {
                        row.has_styles = true;
                    }
                    wrote_bulk = true;
                }
            }

            if wrote_bulk {
                // The run wrote columns `cursor_col ..= cursor_col + take - 1`.
                // A full row means park on the last column with a pending wrap;
                // otherwise advance past the last written cell.
                let end = cursor_col + take;
                if end < columns {
                    self.grid.cursor.pos.col = Column(end);
                } else {
                    self.grid.cursor.pos.col = Column(columns - 1);
                    self.grid.cursor.should_wrap = true;
                }
            } else {
                for i in 0..take {
                    let c = bytes[idx + i] as char;
                    self.write_cell(c, template);
                    if self.grid.cursor.pos.col + 1 < columns {
                        self.grid.cursor.pos.col += 1;
                    } else {
                        self.grid.cursor.should_wrap = true;
                        break;
                    }
                }
            }

            let row = self.grid.cursor.pos.row;
            self.damage.damage_line(row.0 as usize);
            idx += take;
        }
    }

    #[inline]
    fn identify_terminal(&mut self, intermediate: Option<char>) {
        match intermediate {
            None => {
                trace!("Reporting primary device attributes");
                // 62: VT220, 4: sixel, 6: selective erase, 22: ANSI
                // color, 52: clipboard access via OSC 52 (probed by
                // tcell and others before enabling clipboard writes).
                let text = String::from("\x1b[?62;4;6;22;52c");
                self.event_proxy
                    .send_event(RioEvent::PtyWrite(self.route_id, text), self.window_id);
            }
            Some('>') => {
                trace!("Reporting secondary device attributes");
                let version = version_number(env!("CARGO_PKG_VERSION"));
                let text = format!("\x1b[>0;{version};1c");
                self.event_proxy
                    .send_event(RioEvent::PtyWrite(self.route_id, text), self.window_id);
            }
            _ => debug!("Unsupported device attributes intermediate"),
        }
    }

    #[inline]
    fn report_version(&mut self) {
        trace!("Reporting terminal version (XTVERSION)");
        let version = env!("CARGO_PKG_VERSION");
        let text = format!("\x1bP>|Rio {version}\x1b\\");
        self.event_proxy
            .send_event(RioEvent::PtyWrite(self.route_id, text), self.window_id);
    }

    #[inline]
    fn set_modify_other_keys(&mut self, level: u8) {
        self.modify_other_keys = level;
    }

    #[inline]
    fn report_keyboard_mode(&mut self) {
        let current_mode = self.keyboard_mode_stack[self.keyboard_mode_idx];
        let text = format!("\x1b[?{current_mode}u");
        self.event_proxy
            .send_event(RioEvent::PtyWrite(self.route_id, text), self.window_id);
    }

    #[inline]
    fn push_keyboard_mode(&mut self, mode: KeyboardModes) {
        self.keyboard_mode_idx = self.keyboard_mode_idx.wrapping_add(1);
        if self.keyboard_mode_idx >= KEYBOARD_MODE_STACK_MAX_DEPTH {
            self.keyboard_mode_idx %= KEYBOARD_MODE_STACK_MAX_DEPTH;
        }
        self.keyboard_mode_stack[self.keyboard_mode_idx] = mode.bits();

        // Sync self.mode with keyboard_mode_stack
        self.mode &= !Mode::KITTY_KEYBOARD_PROTOCOL;
        self.mode |= Mode::from(mode);
    }

    #[inline]
    fn pop_keyboard_modes(&mut self, to_pop: u16) {
        // If popping more modes than we have, just clear the stack.
        if usize::from(to_pop) >= KEYBOARD_MODE_STACK_MAX_DEPTH {
            self.keyboard_mode_stack.fill(KeyboardModes::NO_MODE.bits());
            self.keyboard_mode_idx = 0;
            self.mode &= !Mode::KITTY_KEYBOARD_PROTOCOL;
            return;
        }
        for _ in 0..to_pop {
            self.keyboard_mode_stack[self.keyboard_mode_idx] =
                KeyboardModes::NO_MODE.bits();
            self.keyboard_mode_idx = self.keyboard_mode_idx.wrapping_sub(1);
            if self.keyboard_mode_idx >= KEYBOARD_MODE_STACK_MAX_DEPTH {
                self.keyboard_mode_idx %= KEYBOARD_MODE_STACK_MAX_DEPTH;
            }
        }

        // Sync self.mode with keyboard_mode_stack
        let current_mode = self.keyboard_mode_stack[self.keyboard_mode_idx];
        self.mode &= !Mode::KITTY_KEYBOARD_PROTOCOL;
        self.mode |= Mode::from(KeyboardModes::from_bits_truncate(current_mode));
    }

    #[inline]
    fn set_keyboard_mode(
        &mut self,
        mode: KeyboardModes,
        apply: KeyboardModesApplyBehavior,
    ) {
        self.set_keyboard_mode(mode.bits(), apply);
    }

    #[inline]
    fn device_status(&mut self, arg: usize) {
        trace!("Reporting device status: {}", arg);
        match arg {
            5 => {
                let text = String::from("\x1b[0n");
                self.event_proxy
                    .send_event(RioEvent::PtyWrite(self.route_id, text), self.window_id);
            }
            6 => {
                let pos = self.grid.cursor.pos;
                let text = format!("\x1b[{};{}R", pos.row + 1, pos.col + 1);
                self.event_proxy
                    .send_event(RioEvent::PtyWrite(self.route_id, text), self.window_id);
            }
            _ => debug!("unknown device status query: {}", arg),
        };
    }

    #[inline]
    fn report_color_scheme_query(&mut self) {
        self.reply_color_scheme_query();
    }

    #[inline]
    fn newline(&mut self) {
        self.linefeed();

        if self.mode.contains(Mode::LINE_FEED_NEW_LINE) {
            self.carriage_return();
        }
    }

    #[inline]
    fn backspace(&mut self) {
        if self.grid.cursor.pos.col > Column(0) {
            let line = self.grid.cursor.pos.row.0 as usize;
            self.grid.cursor.pos.col -= 1;
            self.grid.cursor.should_wrap = false;
            self.damage.damage_line(line);
        }
    }

    #[inline]
    fn clear_screen(&mut self, mode: ClearMode) {
        self.grid.sync_template_style();
        let bg = self.grid.template_style().bg;
        let blank = self.grid.blank_with_bg(bg);

        let screen_lines = self.grid.screen_lines();

        match mode {
            ClearMode::Above => {
                let cursor = self.grid.cursor.pos;

                // If clearing more than one line.
                if cursor.row > 1 {
                    // Fully clear all lines before the current line.
                    self.grid.reset_region(..cursor.row);
                }

                // Clear up to the current column in the current line.
                let end = std::cmp::min(cursor.col + 1, Column(self.grid.columns()));
                self.split_wide_seam(cursor.row, 0, blank);
                self.split_wide_seam(cursor.row, end.0, blank);
                self.grid[cursor.row].has_styles |= blank.carries_style();
                for cell in &mut self.grid[cursor.row][..end] {
                    *cell = blank;
                }

                let range = Line(0)..=cursor.row;
                self.selection =
                    self.selection.take().filter(|s| !s.intersects_range(range));

                let columns = self.grid.columns();
                self.clip_atlas_placements(0, cursor.row.0, 0, columns);
                self.clip_atlas_placements(cursor.row.0, cursor.row.0 + 1, 0, end.0);
            }
            ClearMode::Below => {
                let cursor = self.grid.cursor.pos;
                self.split_wide_seam(cursor.row, cursor.col.0, blank);
                self.grid[cursor.row].has_styles |= blank.carries_style();
                for cell in &mut self.grid[cursor.row][cursor.col..] {
                    *cell = blank;
                }

                if (cursor.row.0 as usize) < screen_lines - 1 {
                    self.grid.reset_region((cursor.row + 1)..);
                }

                let range = cursor.row..Line(screen_lines as i32);
                self.selection =
                    self.selection.take().filter(|s| !s.intersects_range(range));

                let columns = self.grid.columns();
                self.clip_atlas_placements(
                    cursor.row.0,
                    cursor.row.0 + 1,
                    cursor.col.0,
                    columns,
                );
                self.clip_atlas_placements(
                    cursor.row.0 + 1,
                    screen_lines as i32,
                    0,
                    columns,
                );
            }
            ClearMode::All => {
                if self.mode.contains(Mode::ALT_SCREEN) {
                    self.grid.reset_region(..);
                    let columns = self.grid.columns();
                    self.clip_atlas_placements(0, screen_lines as i32, 0, columns);
                } else {
                    // The viewport scrolls into history below; atlas
                    // placements are absolutely anchored and follow
                    // their content into scrollback untouched.
                    let old_offset = self.grid.display_offset();

                    self.grid.clear_viewport();

                    // Compute number of lines scrolled by clearing the viewport.
                    let lines = self.grid.display_offset().saturating_sub(old_offset);

                    self.vi_mode_cursor.pos.row = (self.vi_mode_cursor.pos.row - lines)
                        .grid_clamp(&self.grid, Boundary::Grid);
                }

                self.selection = None;
            }
            ClearMode::Saved if self.history_size() > 0 => {
                self.grid.clear_history();
                self.expire_atlas_placements();

                self.vi_mode_cursor.pos.row = self
                    .vi_mode_cursor
                    .pos
                    .row
                    .grid_clamp(&self.grid, Boundary::Cursor);

                self.selection = self
                    .selection
                    .take()
                    .filter(|s| !s.intersects_range(..Line(0)));
            }
            // We have no history to clear.
            ClearMode::Saved => (),
        }

        // Mark affected lines as damaged based on clear mode
        match mode {
            ClearMode::Above => {
                let cursor_row = self.grid.cursor.pos.row.0 as usize;
                for line in 0..=cursor_row {
                    self.damage.damage_line(line);
                }
            }
            ClearMode::Below => {
                let cursor_row = self.grid.cursor.pos.row.0 as usize;
                for line in cursor_row..screen_lines {
                    self.damage.damage_line(line);
                }
            }
            ClearMode::All | ClearMode::Saved => {
                self.mark_fully_damaged();
            }
        }
        if !self.graphics.kitty_placements.is_empty() {
            self.graphics.kitty_graphics_dirty = true;
        }
    }

    #[inline]
    fn clear_tabs(&mut self, mode: TabulationClearMode) {
        match mode {
            TabulationClearMode::Current => {
                self.tabs[self.grid.cursor.pos.col] = false;
            }
            TabulationClearMode::All => {
                self.tabs.clear_all();
            }
        }
    }

    #[inline]
    fn linefeed(&mut self) {
        let next = self.grid.cursor.pos.row + 1;
        if next == self.scroll_region.end {
            self.scroll_up_relative(self.scroll_region.start, 1);
        } else if next < self.grid.screen_lines() {
            self.damage_cursor();
            self.grid.cursor.pos.row += 1;
            self.damage_cursor();
        }
    }

    #[inline]
    fn set_horizontal_tabstop(&mut self) {
        self.tabs[self.grid.cursor.pos.col] = true;
    }

    #[inline]
    fn set_hyperlink(&mut self, hyperlink: Option<Hyperlink>) {
        // OSC 8 hyperlinks live in `Grid::extras_table` after the cell
        // repack. Setting one allocates a side-table slot holding the
        // Hyperlink and stores its id on the cursor template, so every
        // subsequent cell write picks up the id via
        // `write_at_cursor`'s `template_extras_id` propagation.
        //
        // Extras slots are not reference-counted by design; a slot
        // lives until the cadence-driven `reclaim_extras` mark-and-sweep
        // finds no cell referencing it (bounded drift, see
        // `ExtrasTable::should_reclaim`). Ref-counting would free slots
        // sooner but requires Clone/Drop hooks on `Square`, taxing row
        // drops and template copies even when no extras exist; the
        // sweep costs nothing unless extras are actually allocated.
        match hyperlink {
            Some(hl) => {
                let id = self.grid.alloc_extras(crate::crosswords::square::Extras {
                    hyperlink: Some(hl),
                    ..Default::default()
                });
                self.grid.cursor.template.set_extras_id(Some(id));
                self.grid
                    .cursor
                    .template
                    .insert_cell_flag(crate::crosswords::square::CellFlags::HYPERLINK);
            }
            None => {
                self.grid.cursor.template.set_extras_id(None);
                self.grid
                    .cursor
                    .template
                    .remove_cell_flag(crate::crosswords::square::CellFlags::HYPERLINK);
            }
        }
    }

    /// Set the indexed color value.
    #[inline]
    fn set_color(&mut self, index: usize, color: ColorRgb) {
        // Damage terminal if the color changed and it's not the cursor.
        let color_arr = color.to_arr();

        if index != NamedColor::Cursor as usize && self.colors[index] != Some(color_arr) {
            self.mark_fully_damaged();
        }

        self.colors[index] = Some(color_arr);
        self.event_proxy.send_event(
            RioEvent::ColorChange(self.route_id, index, Some(color)),
            self.window_id,
        );
    }

    #[inline]
    fn reset_color(&mut self, index: usize) {
        // Damage terminal if the color changed and it's not the cursor.
        if index != NamedColor::Cursor as usize && self.colors[index].is_some() {
            self.mark_fully_damaged();
        }

        self.colors[index] = None;
        self.event_proxy.send_event(
            RioEvent::ColorChange(self.route_id, index, None),
            self.window_id,
        );
    }

    #[inline]
    fn bell(&mut self) {
        self.event_proxy.send_event(RioEvent::Bell, self.window_id);
    }

    #[inline]
    fn desktop_notification(&mut self, title: String, body: String) {
        self.event_proxy.send_event(
            RioEvent::DesktopNotification { title, body },
            self.window_id,
        );
    }

    #[inline]
    fn substitute(&mut self) {
        warn!("[unimplemented] Substitute");
    }

    #[inline]
    fn clipboard_load(&mut self, clipboard: u8, terminator: &str) {
        let clipboard_type = match clipboard {
            b'c' => ClipboardType::Clipboard,
            b'p' | b's' => ClipboardType::Selection,
            _ => return,
        };

        let terminator = terminator.to_owned();

        self.event_proxy.send_event(
            RioEvent::ClipboardLoad(
                self.route_id,
                clipboard_type,
                Arc::new(move |text| {
                    let base64 = general_purpose::STANDARD.encode(text);
                    format!("\x1b]52;{};{}{}", clipboard as char, base64, terminator)
                }),
            ),
            self.window_id,
        );
    }

    #[inline]
    fn put_tab(&mut self, mut count: u16) {
        // A tab after the last column is the same as a linebreak.
        if self.grid.cursor.should_wrap {
            self.wrapline();
            return;
        }

        while self.grid.cursor.pos.col < self.grid.columns() && count != 0 {
            count -= 1;

            let c = self.grid.cursor.charsets[self.active_charset].map('\t');
            let cell = self.grid.cursor_square();
            if cell.c() == '\0' {
                cell.set_c(c);
            }

            loop {
                if (self.grid.cursor.pos.col + 1) == self.grid.columns() {
                    break;
                }

                self.grid.cursor.pos.col += 1;

                if self.tabs[self.grid.cursor.pos.col] {
                    break;
                }
            }
        }
    }

    #[inline]
    fn carriage_return(&mut self) {
        let new_col = 0;
        let row = self.grid.cursor.pos.row.0 as usize;
        self.damage.damage_line(row);
        self.grid.cursor.pos.col = Column(new_col);
        self.grid.cursor.should_wrap = false;
    }

    #[inline]
    fn move_forward_tabs(&mut self, count: u16) {
        trace!("Moving forward {} tabs", count);
        let num_cols = self.columns();
        for _ in 0..count {
            let mut col = self.grid.cursor.pos.col;

            if col == num_cols - 1 {
                break;
            }

            for i in col.0 + 1..num_cols {
                col = Column(i);
                if self.tabs[col] {
                    break;
                }
            }

            self.grid.cursor.pos.col = col;
        }

        let line = self.grid.cursor.pos.row.0 as usize;
        self.damage.damage_line(line);
    }

    #[inline]
    fn save_cursor_position(&mut self) {
        self.grid.saved_cursor = self.grid.cursor.clone();
    }

    #[inline]
    fn restore_cursor_position(&mut self) {
        trace!("Restoring cursor position");

        self.damage_cursor();
        self.grid.cursor = self.grid.saved_cursor.clone();
        self.damage_cursor();
    }

    #[inline]
    fn clear_line(&mut self, mode: LineClearMode) {
        let bg = self.grid.template_style().bg;
        let blank = self.grid.blank_with_bg(bg);
        let point = self.grid.cursor.pos;
        let should_wrap = self.grid.cursor.should_wrap;

        let (left, right) = match mode {
            LineClearMode::Right if should_wrap => return,
            LineClearMode::Right => (point.col, Column(self.grid.columns())),
            LineClearMode::Left => (Column(0), point.col + 1),
            LineClearMode::All => (Column(0), Column(self.grid.columns())),
        };

        self.damage.damage_line(point.row.0 as usize);

        self.split_wide_seam(point.row, left.0, blank);
        self.split_wide_seam(point.row, right.0, blank);
        let row = &mut self.grid[point.row];
        row.has_styles |= blank.carries_style();
        for cell in &mut row[left..right] {
            *cell = blank;
        }
        let range = self.grid.cursor.pos.row..=self.grid.cursor.pos.row;
        self.selection = self.selection.take().filter(|s| !s.intersects_range(range));
        self.clip_atlas_placements(point.row.0, point.row.0 + 1, left.0, right.0);
        if !self.graphics.kitty_placements.is_empty() {
            self.graphics.kitty_graphics_dirty = true;
        }
    }

    #[inline]
    fn set_scrolling_region(&mut self, top: usize, bottom: Option<usize>) {
        // Fallback to the last line as default.
        let bottom = bottom.unwrap_or_else(|| self.grid.screen_lines());

        if top >= bottom {
            warn!("Invalid scrolling region: ({};{})", top, bottom);
            return;
        }

        // Bottom should be included in the range, but range end is not
        // usually included. One option would be to use an inclusive
        // range, but instead we just let the open range end be 1
        // higher.
        let start = Line(top as i32 - 1);
        let end = Line(bottom as i32);

        debug!("Setting scrolling region: ({};{})", start, end);

        let screen_lines = Line(self.grid.screen_lines() as i32);
        self.scroll_region.start = std::cmp::min(start, screen_lines);
        self.scroll_region.end = std::cmp::min(end, screen_lines);
        self.goto(Line(0), Column(0));
    }

    #[inline]
    fn text_area_size_pixels(&mut self) {
        debug!("text_area_size_pixels");
        self.event_proxy.send_event(
            RioEvent::TextAreaSizeRequest(
                self.route_id,
                Arc::new(move |window_size| {
                    let height = window_size.height;
                    let width = window_size.width;
                    format!("\x1b[4;{height};{width}t")
                }),
            ),
            self.window_id,
        );
    }

    #[inline]
    fn cells_size_pixels(&mut self) {
        // https://terminalguide.namepad.de/seq/csi_st-16/
        let text = format!(
            "\x1b[6;{};{}t",
            self.graphics.cell_height, self.graphics.cell_width
        );
        debug!("cells_size_pixels {:?}", text);
        self.event_proxy
            .send_event(RioEvent::PtyWrite(self.route_id, text), self.window_id);
    }

    #[inline]
    fn text_area_size_chars(&mut self) {
        let text = format!(
            "\x1b[8;{};{}t",
            self.grid.screen_lines(),
            self.grid.columns()
        );
        debug!("text_area_size_chars {:?}", text);
        self.event_proxy
            .send_event(RioEvent::PtyWrite(self.route_id, text), self.window_id);
    }

    #[inline]
    fn graphics_attribute(&mut self, pi: u16, pa: u16) {
        // From Xterm documentation:
        //
        // CSI ? Pi ; Pa ; Pv S
        //
        // Pi = 1 -> item is number of color registers.
        // Pi = 2 -> item is Sixel graphics geometry (in pixels).
        // Pi = 3 -> item is ReGIS graphics geometry (in pixels).
        //
        // Pa = 1 -> read attribute.
        // Pa = 2 -> reset to default.
        // Pa = 3 -> set to value in Pv.
        // Pa = 4 -> read the maximum allowed value.
        //
        // Pv is ignored by xterm except when setting (Pa == 3).
        // Pv = n <- A single integer is used for color registers.
        // Pv = width ; height <- Two integers for graphics geometry.
        //
        // xterm replies with a control sequence of the same form:
        //
        // CSI ? Pi ; Ps ; Pv S
        //
        // where Ps is the status:
        // Ps = 0 <- success.
        // Ps = 1 <- error in Pi.
        // Ps = 2 <- error in Pa.
        // Ps = 3 <- failure.
        //
        // On success, Pv represents the value read or set.

        fn generate_response(pi: u16, ps: u16, pv: &[usize]) -> String {
            use std::fmt::Write;
            let mut text = format!("\x1b[?{pi};{ps}");
            for item in pv {
                let _ = write!(&mut text, ";{item}");
            }
            text.push('S');
            text
        }

        let (ps, pv) = match pi {
            1 => {
                match pa {
                    1 => (0, &[sixel::MAX_COLOR_REGISTERS][..]), // current value is always the
                    // maximum
                    2 => (3, &[][..]), // Report unsupported
                    3 => (3, &[][..]), // Report unsupported
                    4 => (0, &[sixel::MAX_COLOR_REGISTERS][..]),
                    _ => (2, &[][..]), // Report error in Pa
                }
            }

            2 => {
                match pa {
                    1 => {
                        self.event_proxy.send_event(
                            RioEvent::TextAreaSizeRequest(
                                self.route_id,
                                Arc::new(move |window_size| {
                                    let width = window_size.width;
                                    let height = window_size.height;
                                    let graphic_dimensions = [
                                        std::cmp::min(
                                            width as usize,
                                            MAX_GRAPHIC_DIMENSIONS[0],
                                        ),
                                        std::cmp::min(
                                            height as usize,
                                            MAX_GRAPHIC_DIMENSIONS[1],
                                        ),
                                    ];

                                    let (ps, pv) = (0, &graphic_dimensions[..]);
                                    generate_response(pi, ps, pv)
                                }),
                            ),
                            self.window_id,
                        );
                        return;
                    }
                    2 => (3, &[][..]), // Report unsupported
                    3 => (3, &[][..]), // Report unsupported
                    4 => (0, &MAX_GRAPHIC_DIMENSIONS[..]),
                    _ => (2, &[][..]), // Report error in Pa
                }
            }

            3 => {
                (1, &[][..]) // Report error in Pi (ReGIS unknown)
            }

            _ => {
                (1, &[][..]) // Report error in Pi
            }
        };

        self.event_proxy.send_event(
            RioEvent::PtyWrite(self.route_id, generate_response(pi, ps, pv)),
            self.window_id,
        );
    }

    #[inline]
    fn sixel_graphic_start(&mut self, params: &Params) {
        let palette = self.graphics.sixel_shared_palette.take();
        self.graphics.sixel_parser = Some(Box::new(sixel::Parser::new(params, palette)));
    }

    #[inline]
    fn is_sixel_graphic_active(&self) -> bool {
        self.graphics.sixel_parser.is_some()
    }

    #[inline]
    fn sixel_graphic_put(&mut self, byte: u8) -> Result<(), sixel::Error> {
        if let Some(parser) = &mut self.graphics.sixel_parser {
            parser.put(byte)
        } else {
            self.sixel_graphic_reset();
            Err(sixel::Error::NonExistentParser)
        }
    }

    #[inline]
    fn sixel_graphic_reset(&mut self) {
        self.graphics.sixel_parser = None;
    }

    #[inline]
    fn sixel_graphic_finish(&mut self) {
        let parser = self.graphics.sixel_parser.take();
        if let Some(parser) = parser {
            match parser.finish() {
                // Sixel uses None to indicate traditional Sixel cursor behavior
                Ok((graphic, palette)) => {
                    self.insert_graphic(graphic, Some(palette), None)
                }
                Err(err) => warn!("Failed to parse Sixel data: {}", err),
            }
        } else {
            warn!("Failed to sixel_graphic_finish");
        }
    }

    #[inline]
    fn insert_graphic(
        &mut self,
        graphic: GraphicData,
        palette: Option<Vec<ColorRgb>>,
        cursor_movement: Option<u8>,
    ) {
        debug!(
            "insert_graphic called: id={}, {}x{}, format={:?}, cursor_movement={:?}",
            graphic.id.get(),
            graphic.width,
            graphic.height,
            graphic.color_type,
            cursor_movement
        );
        let cell_width = self.graphics.cell_width as usize;
        let cell_height = self.graphics.cell_height as usize;

        // Headless embedders never report cell dimensions; without this
        // guard the row-fill loop below would `step_by(0)` and panic.
        if cell_width == 0 || cell_height == 0 {
            return;
        }

        // Store last palette if we receive a new one, and it is shared.
        if let Some(palette) = palette {
            if !self.mode.contains(Mode::SIXEL_PRIV_PALETTE) {
                self.graphics.sixel_shared_palette = Some(palette);
            }
        }

        // Compute display dimensions without CPU pixel resampling.
        // The GPU will scale the original texture to these dimensions.
        let view_width = cell_width * self.grid.columns();
        let view_height = cell_height * self.grid.screen_lines();
        let (display_w, display_h) = graphic.compute_display_dimensions(
            cell_width,
            cell_height,
            view_width,
            view_height,
        );

        if display_w > MAX_GRAPHIC_DIMENSIONS[0] || display_h > MAX_GRAPHIC_DIMENSIONS[1]
        {
            debug!(
                "insert_graphic: display dimensions too large {}x{}, max is {:?}",
                display_w, display_h, MAX_GRAPHIC_DIMENSIONS
            );
            return;
        }

        let width = display_w as u16;
        let height = display_h as u16;

        if width == 0 || height == 0 {
            debug!("insert_graphic: zero width or height, aborting");
            return;
        }

        let mut graphic = graphic;
        graphic.display_width = Some(display_w);
        graphic.display_height = Some(display_h);

        // Calculate bytes for this graphic
        let graphic_bytes = graphic.pixels.len();

        debug!(
            "insert_graphic: image needs {} bytes, current total: {}/{}",
            graphic_bytes, self.graphics.total_bytes, self.graphics.total_limit
        );

        // Only scan the grid for used IDs when eviction is actually needed
        if self.graphics.total_bytes + graphic_bytes > self.graphics.total_limit {
            let used_ids = self.collect_used_graphic_ids();
            debug!(
                "insert_graphic: {} images currently in use in grid, need eviction",
                used_ids.len()
            );

            if !self.graphics.evict_images(graphic_bytes, &used_ids) {
                warn!(
                    "Failed to evict enough images for {} bytes, image may not display",
                    graphic_bytes
                );
                // Continue anyway - let it fail gracefully rather than silently dropping
            }
        }

        let graphic_id = self.graphics.next_id();

        debug!("insert_graphic: assigned new id {}", graphic_id.0);

        // Track this graphic's memory usage
        self.graphics.track_graphic(graphic_id, graphic_bytes);

        // If SIXEL_DISPLAY is disabled, the start of the graphic is the
        // cursor position, and the grid can be scrolled if the graphic is
        // larger than the screen. The cursor is moved to the next line
        // after the graphic.
        //
        // If it is disabled, the graphic starts at (0, 0), the grid is never
        // scrolled, and the cursor position is unmodified.

        let scrolling = !self.mode.contains(Mode::SIXEL_DISPLAY);

        // For "don't move the cursor" placements, the fill loop below
        // still linefeeds through the image rows; the position is
        // restored afterwards.
        let saved_cursor = self.grid.cursor.pos;

        let leftmost = if scrolling {
            self.grid.cursor.pos.col.0
        } else {
            0
        };

        // Anchor of the image's top row in the stable absolute space,
        // captured before the fill loop scrolls anything. DECSDM
        // (no scrolling) draws from the screen's top-left instead.
        let anchor_abs_row = self.grid.lines_evicted() as i64
            + self.history_size() as i64
            + if scrolling {
                self.grid.cursor.pos.row.0 as i64
            } else {
                0
            };

        // Advance through the image's rows: damage each covered row and
        // scroll as the image grows past the bottom margin. The
        // placement below carries the pixels; cells stay untouched
        // (DEC semantics are enforced by placement clipping instead).
        for (top, offset_y) in (0..).zip((0..height).step_by(cell_height)) {
            let line = if scrolling {
                self.grid.cursor.pos.row
            } else {
                // Check if the image is beyond the screen limit.
                if top >= self.grid.screen_lines() as i32 {
                    break;
                }

                Line(top)
            };

            self.mark_line_damaged(line);

            if scrolling && offset_y < height.saturating_sub(cell_height as u16) {
                self.linefeed();
            }
        }

        // Handle cursor movement based on cursor_movement parameter:
        // - None: Sixel (traditional behavior - move to next line after image)
        // - Some(0): Kitty C=0 (cursor stays on last row of image)
        // - Some(1): Kitty C=1 (cursor doesn't move at all)
        // Display width in cells, from the size actually drawn (the
        // requested display size, not the source pixel size).
        let graphic_columns = (width as usize).div_ceil(cell_width);

        match cursor_movement {
            None => {
                // Sixel: the fill loop left the cursor on the image's
                // last row; the column follows DEC STD 070 (image start
                // column), or mode 8452 (first column right of the
                // image). This matches foot, xterm and contour.
                if scrolling {
                    let col = if self.mode.contains(Mode::SIXEL_CURSOR_TO_THE_RIGHT) {
                        (leftmost + graphic_columns).min(self.grid.columns() - 1)
                    } else {
                        leftmost
                    };
                    self.grid.cursor.pos.col = Column(col);
                    self.grid.cursor.should_wrap = false;
                }
            }
            Some(1) => {
                // Don't move the cursor at all (kitty C=1, and the
                // OSC 1337 doNotMoveCursor=1 extension). The fill
                // loop advanced through the image rows; put the
                // cursor back where the image was requested.
                self.grid.cursor.pos = saved_cursor;
            }
            Some(2) => {
                // OSC 1337 inline image: iTerm2 leaves the cursor on
                // the image's last row, in the first column after the
                // image.
                if scrolling {
                    let col = (leftmost + graphic_columns).min(self.grid.columns() - 1);
                    self.grid.cursor.pos.col = Column(col);
                    self.grid.cursor.should_wrap = false;
                }
            }
            Some(_) => {
                // Legacy/unknown values: last row, image start column.
                if scrolling {
                    self.grid.cursor.pos.col = Column(leftmost);
                    self.grid.cursor.should_wrap = false;
                }
            }
        }

        // Record the placement: the single source of truth for where
        // this image lives on the grid. Cell fills above remain only
        // until the extras-based path is removed.
        {
            let key = rio_graphics::atlas_image_key(graphic_id.get());
            let placement_columns = (width as usize).div_ceil(cell_width);
            let placement_rows = (height as usize).div_ceil(cell_height);

            // A new image overwrites what it covers, like text does:
            // clip existing placements against its rect so a fully
            // covered predecessor drops (the recount below frees its
            // texture).
            if !self.graphics.atlas_placements.is_empty() {
                let old = std::mem::take(&mut self.graphics.atlas_placements);
                let mut next = Vec::with_capacity(old.len());
                for placement in old {
                    if placement
                        .subtract_rect(
                            anchor_abs_row,
                            anchor_abs_row + placement_rows as i64,
                            leftmost,
                            leftmost + placement_columns,
                            &mut next,
                        )
                        .is_none()
                    {
                        next.push(placement);
                    }
                }
                self.graphics.atlas_placements = next;
            }

            self.graphics
                .atlas_placements
                .push(crate::ansi::graphics::AtlasPlacement {
                    image_key: key,
                    abs_row: anchor_abs_row,
                    col: leftmost,
                    columns: placement_columns,
                    rows: placement_rows,
                    src_x: 0,
                    src_y: 0,
                    src_width: width as u32,
                    src_height: height as u32,
                    total_width: width as u32,
                    total_height: height as u32,
                    insert_cell_w: cell_width as u16,
                    insert_cell_h: cell_height as u16,
                });
            self.graphics.recount_atlas_keys();
            self.graphics.kitty_graphics_dirty = true;
        }

        // Add the graphic data to the pending queue.
        debug!(
            "insert_graphic: adding to pending queue, graphic_id={}, final size={}x{}",
            graphic_id.0, width, height
        );
        self.graphics.pending.push(GraphicData {
            id: graphic_id,
            ..graphic
        });

        // Send graphics update event
        debug!("insert_graphic: sending graphics updates");
        self.send_graphics_updates();
    }

    #[inline]
    fn store_graphic(&mut self, graphic: GraphicData) {
        // a=t: Store graphic without displaying.
        // No GPU upload here — pixel data is sent to GPU when a=p placement arrives.
        let image_id = graphic.id.get() as u32;
        debug!(
            "Storing kitty graphic: id={}, {}x{}",
            image_id, graphic.width, graphic.height
        );
        // Retransmission of an id with live placements: refresh their
        // grid footprint against the new dimensions and push the new
        // pixels so the display updates without a re-place.
        let has_placements = self
            .graphics
            .kitty_placements
            .keys()
            .any(|(id, _)| *id == image_id)
            || self
                .graphics
                .kitty_virtual_placements
                .keys()
                .any(|(id, _)| *id == image_id);
        let image_width = graphic.width;
        let image_height = graphic.height;
        self.graphics.store_kitty_image(image_id, None, graphic);

        if has_placements {
            self.refresh_placements_for_image(image_id, image_width, image_height);
            self.ensure_kitty_upload(image_id);
        }
        self.send_graphics_updates();
    }

    fn kitty_transmit_and_display(
        &mut self,
        graphic_data: GraphicData,
        placement: crate::ansi::kitty_graphics_protocol::PlacementRequest,
    ) {
        let image_id = graphic_data.id.get() as u32;
        debug!(
            "Kitty transmit+display: id={}, {}x{}",
            image_id, graphic_data.width, graphic_data.height
        );

        // `a=T` + `U=1` is what `kitten icat --unicode-placeholder`
        // emits: combined transmit+place where the placement is virtual.
        // Route to the virtual-placement path so only metadata is
        // registered (the application emits U+10EEEE cells itself).
        // Like the a=t path, a retransmission with live placements of
        // this id must refresh them against the new dimensions.
        let image_width = graphic_data.width;
        let image_height = graphic_data.height;

        if placement.virtual_placement {
            self.graphics
                .store_kitty_image(image_id, None, graphic_data);
            self.refresh_placements_for_image(image_id, image_width, image_height);
            self.ensure_kitty_upload(image_id);
            self.place_virtual_graphic(placement);
            return;
        }

        // Store takes ownership and sets transmit_time.
        self.graphics
            .store_kitty_image(image_id, None, graphic_data);
        self.refresh_placements_for_image(image_id, image_width, image_height);

        // Place as overlay — handles GPU upload internally.
        self.place_kitty_overlay(image_id, &placement);
    }

    #[inline]
    fn place_graphic(
        &mut self,
        placement: crate::ansi::kitty_graphics_protocol::PlacementRequest,
    ) -> bool {
        debug!(
            "Kitty graphics placement: image_id={}, x={}, y={}, columns={}, rows={}, virtual={}",
            placement.image_id,
            placement.x,
            placement.y,
            placement.columns,
            placement.rows,
            placement.virtual_placement,
        );

        // `a=p` always references a previously transmitted image; per
        // the kitty spec placing an unknown id is ENOENT, which the
        // dispatcher reports from this return value. This covers
        // virtual placements too — registering placeholder metadata
        // for an image that does not exist would render nothing while
        // telling the client everything worked.
        let image_id = placement.image_id;
        if self.graphics.get_kitty_image(image_id).is_none() {
            warn!(
                "Attempted to place non-existent kitty graphic: id={}",
                placement.image_id
            );
            return false;
        }

        // `U=1` → virtual placement: store metadata, the application
        // emits U+10EEEE placeholder cells itself. The renderer scans
        // visible cells and composites the image at those positions.
        if placement.virtual_placement {
            // `a=t` defers the GPU upload to the placement; without it
            // the placeholder cells stay blank (the `a=t` + `a=p,U=1`
            // sequence snacks.image and yazi emit).
            self.ensure_kitty_upload(image_id);
            self.place_virtual_graphic(placement);
            return true;
        }

        // Direct placement: use overlay path
        self.place_kitty_overlay(image_id, &placement);
        true
    }

    #[inline]
    fn delete_graphics(
        &mut self,
        delete: crate::ansi::kitty_graphics_protocol::DeleteRequest,
    ) {
        debug!(
            "Kitty graphics delete: action={}, image_id={}, x={}, y={}, z_index={}",
            delete.action as char, delete.image_id, delete.x, delete.y, delete.z_index
        );

        let mut overlay_changed = false;

        match delete.action {
            b'a' | b'A' => {
                // "All placements visible on screen": virtual (U=1)
                // placements are not visible by themselves and survive,
                // as do the images they keep alive (kitty's
                // clear_filter_func skips virtual refs).
                overlay_changed = !self.graphics.kitty_placements.is_empty();
                self.graphics.kitty_placements.clear();

                if delete.delete_data {
                    self.cleanup_unused_kitty_images();
                }
            }
            b'i' | b'I' => {
                let image_id_to_match = delete.image_id;
                // Delete overlay and virtual placements for this image
                let before = self.graphics.kitty_placements.len()
                    + self.graphics.kitty_virtual_placements.len();
                if delete.placement_id != 0 {
                    self.graphics
                        .kitty_placements
                        .remove(&(image_id_to_match, delete.placement_id));
                    self.graphics
                        .kitty_virtual_placements
                        .remove(&(image_id_to_match, delete.placement_id));
                } else {
                    self.graphics
                        .kitty_placements
                        .retain(|k, _| k.0 != image_id_to_match);
                    self.graphics
                        .kitty_virtual_placements
                        .retain(|k, _| k.0 != image_id_to_match);
                }
                overlay_changed = self.graphics.kitty_placements.len()
                    + self.graphics.kitty_virtual_placements.len()
                    != before;

                if delete.delete_data {
                    self.graphics
                        .delete_kitty_images(|id, _| *id == image_id_to_match);
                }
            }
            b'c' | b'C' => {
                let cursor_pos = self.grid.cursor.pos;
                // Delete overlays intersecting cursor
                let col = cursor_pos.col.0;
                let abs_row = self.grid.lines_evicted() as i64
                    + self.history_size() as i64
                    + cursor_pos.row.0 as i64;
                let before = self.graphics.kitty_placements.len();
                self.graphics.kitty_placements.retain(|_, p| {
                    !(p.dest_col <= col
                        && col < p.dest_col + p.columns as usize
                        && p.dest_row <= abs_row
                        && abs_row < p.dest_row + p.rows as i64)
                });
                overlay_changed = self.graphics.kitty_placements.len() != before;

                if delete.delete_data {
                    self.cleanup_unused_kitty_images();
                }
            }
            b'p' | b'P' => {
                if delete.x > 0 && delete.y > 0 {
                    let col = Column((delete.x - 1) as usize);
                    let row = Line((delete.y - 1) as i32);
                    // Delete overlays at position
                    let abs_row = self.grid.lines_evicted() as i64
                        + self.history_size() as i64
                        + row.0 as i64;
                    let c = col.0;
                    let before = self.graphics.kitty_placements.len();
                    self.graphics.kitty_placements.retain(|_, p| {
                        !(p.dest_col <= c
                            && c < p.dest_col + p.columns as usize
                            && p.dest_row <= abs_row
                            && abs_row < p.dest_row + p.rows as i64)
                    });
                    overlay_changed = self.graphics.kitty_placements.len() != before;
                }

                if delete.delete_data {
                    self.cleanup_unused_kitty_images();
                }
            }
            b'x' | b'X' => {
                if delete.x > 0 {
                    let col = Column((delete.x - 1) as usize);
                    let c = col.0;
                    let before = self.graphics.kitty_placements.len();
                    self.graphics.kitty_placements.retain(|_, p| {
                        !(p.dest_col <= c && c < p.dest_col + p.columns as usize)
                    });
                    overlay_changed = self.graphics.kitty_placements.len() != before;
                }

                if delete.delete_data {
                    self.cleanup_unused_kitty_images();
                }
            }
            b'y' | b'Y' => {
                if delete.y > 0 {
                    let row = Line((delete.y - 1) as i32);
                    let abs_row = self.grid.lines_evicted() as i64
                        + self.history_size() as i64
                        + row.0 as i64;
                    let before = self.graphics.kitty_placements.len();
                    self.graphics.kitty_placements.retain(|_, p| {
                        !(p.dest_row <= abs_row && abs_row < p.dest_row + p.rows as i64)
                    });
                    overlay_changed = self.graphics.kitty_placements.len() != before;
                }

                if delete.delete_data {
                    self.cleanup_unused_kitty_images();
                }
            }
            b'z' | b'Z' => {
                let z = delete.z_index;
                let before = self.graphics.kitty_placements.len();
                self.graphics.kitty_placements.retain(|_, p| p.z_index != z);
                overlay_changed = self.graphics.kitty_placements.len() != before;

                if delete.delete_data {
                    self.cleanup_unused_kitty_images();
                }
            }
            b'n' | b'N' => {
                // Delete by image number — look up image_id from the
                // number map. Prefer the canonical `I=` channel
                // (`delete.image_number`), but fall back to `image_id`
                // for older clients that shove the number into `i=`.
                let lookup_number = if delete.image_number > 0 {
                    delete.image_number
                } else {
                    delete.image_id
                };
                if let Some(&image_id) =
                    self.graphics.kitty_image_numbers.get(&lookup_number)
                {
                    let before = self.graphics.kitty_placements.len()
                        + self.graphics.kitty_virtual_placements.len();
                    if delete.placement_id != 0 {
                        self.graphics
                            .kitty_placements
                            .remove(&(image_id, delete.placement_id));
                        self.graphics
                            .kitty_virtual_placements
                            .remove(&(image_id, delete.placement_id));
                    } else {
                        self.graphics
                            .kitty_placements
                            .retain(|k, _| k.0 != image_id);
                        self.graphics
                            .kitty_virtual_placements
                            .retain(|k, _| k.0 != image_id);
                    }
                    overlay_changed = self.graphics.kitty_placements.len()
                        + self.graphics.kitty_virtual_placements.len()
                        != before;

                    if delete.delete_data {
                        self.graphics.delete_kitty_images(|id, _| *id == image_id);
                    }
                }
            }
            b'q' | b'Q' => {
                // Delete at cell position with z-index filter
                if delete.x > 0 && delete.y > 0 {
                    let col = Column((delete.x - 1) as usize);
                    let row = Line((delete.y - 1) as i32);
                    // Delete overlays at position with z-index filter
                    let z = delete.z_index;
                    let abs_row = self.grid.lines_evicted() as i64
                        + self.history_size() as i64
                        + row.0 as i64;
                    let c = col.0;
                    let before = self.graphics.kitty_placements.len();
                    self.graphics.kitty_placements.retain(|_, p| {
                        !(p.z_index == z
                            && p.dest_col <= c
                            && c < p.dest_col + p.columns as usize
                            && p.dest_row <= abs_row
                            && abs_row < p.dest_row + p.rows as i64)
                    });
                    overlay_changed = self.graphics.kitty_placements.len() != before;
                }

                if delete.delete_data {
                    self.cleanup_unused_kitty_images();
                }
            }
            b'r' | b'R' => {
                // Delete by image ID range [x..y]
                let range_start = delete.x;
                let range_end = delete.y;
                if range_start > 0 && range_end >= range_start {
                    let before = self.graphics.kitty_placements.len()
                        + self.graphics.kitty_virtual_placements.len();
                    self.graphics
                        .kitty_placements
                        .retain(|k, _| k.0 < range_start || k.0 > range_end);
                    self.graphics
                        .kitty_virtual_placements
                        .retain(|k, _| k.0 < range_start || k.0 > range_end);
                    overlay_changed = self.graphics.kitty_placements.len()
                        + self.graphics.kitty_virtual_placements.len()
                        != before;

                    if delete.delete_data {
                        self.graphics.delete_kitty_images(|id, _| {
                            *id >= range_start && *id <= range_end
                        });
                    }
                }
            }
            _ => {
                debug!(
                    "Kitty graphics delete mode '{}' not implemented",
                    delete.action as char
                );
            }
        }

        if overlay_changed {
            self.graphics.kitty_graphics_dirty = true;
            // Placement-only deletes produce no cell damage, and the
            // damage event is what drives a repaint; without this a
            // deleted image stays visible until unrelated output.
            self.mark_fully_damaged();
        }
        self.send_graphics_updates();
    }

    #[inline]
    fn kitty_graphics_response(&mut self, response: String) {
        // Send response back to the terminal
        self.event_proxy
            .send_event(RioEvent::PtyWrite(self.route_id, response), self.window_id);
    }

    #[inline]
    fn xtgettcap_response(&mut self, response: String) {
        self.event_proxy
            .send_event(RioEvent::PtyWrite(self.route_id, response), self.window_id);
    }

    #[inline]
    fn glyph_protocol_response(&mut self, response: String) {
        self.event_proxy
            .send_event(RioEvent::PtyWrite(self.route_id, response), self.window_id);
    }

    #[cfg(feature = "graphics")]
    #[cfg(feature = "graphics")]
    fn glyph_register(
        &mut self,
        cp: u32,
        payload: crate::ansi::glyph_protocol::GlyphPayload,
        width: u8,
    ) -> Result<(), crate::ansi::glyph_protocol::RegisterError> {
        use crate::ansi::glyph_protocol::{is_pua, GlyphPayload, RegisterError};
        use rio_graphics::glyph::glyf_decode;
        use rio_graphics::glyph::glyph_registry::{
            GlyphRegistry, RegisterRejection, StoredPayload,
        };

        // PUA check first — callers that bypass the wire parser (direct
        // API, tests) still get the rejection without accidentally
        // allocating the registry on a doomed request.
        if !is_pua(cp) {
            return Err(RegisterError::OutOfNamespace);
        }

        // Translate a glyf_decode error into the protocol's defined
        // `reason=` codes.
        fn translate(err: glyf_decode::DecodeError) -> RegisterError {
            match err {
                glyf_decode::DecodeError::Composite => {
                    RegisterError::CompositeUnsupported
                }
                glyf_decode::DecodeError::Hinted => RegisterError::HintingUnsupported,
                glyf_decode::DecodeError::Malformed => RegisterError::MalformedPayload,
                glyf_decode::DecodeError::TooLarge => RegisterError::OutlineTooLarge,
            }
        }

        // §8.2: a container's outlines must sum to at most
        // MAX_DECODED_POINTS. Peeks each record header, no decoding.
        fn container_budget(glyphs: &[Vec<u8>]) -> Result<(), RegisterError> {
            let mut total = 0usize;
            for glyf in glyphs {
                total += glyf_decode::peek_point_count(glyf).map_err(translate)?;
                if total > glyf_decode::MAX_DECODED_POINTS {
                    return Err(RegisterError::OutlineTooLarge);
                }
            }
            Ok(())
        }

        // Validate the monochrome `glyf` payload at register time so a
        // bad outline produces a clear error response. For COLR
        // containers, structural validation stays render-time only:
        // re-decoding every carried outline (up to 1024 per
        // registration) on the hot register path costs more than it
        // saves, and the parser already catches the common-case
        // malformation. A structurally-broken COLR payload manifests
        // as tofu at first render rather than a register-time error;
        // that's the accepted trade-off. The one register-time check
        // containers do get is the §8.2 decoded-size budget, because
        // it exists precisely to stop hostile payloads from reaching
        // the decoder's allocations.
        let (stored, upm) = match payload {
            GlyphPayload::Glyf { glyf, upm } => {
                glyf_decode::decode(&glyf).map_err(translate)?;
                (StoredPayload::Glyf { glyf }, upm)
            }
            GlyphPayload::ColrV0 { container, upm } => {
                container_budget(&container.glyphs)?;
                (
                    StoredPayload::ColrV0 {
                        glyphs: container.glyphs,
                        colr: container.colr,
                        cpal: container.cpal,
                    },
                    upm,
                )
            }
            GlyphPayload::ColrV1 { container, upm } => {
                container_budget(&container.glyphs)?;
                (
                    StoredPayload::ColrV1 {
                        glyphs: container.glyphs,
                        colr: container.colr,
                        cpal: container.cpal,
                    },
                    upm,
                )
            }
        };

        // Lazily allocate the registry — idle terminals that never see
        // Glyph Protocol traffic stay at `None` and pay nothing per
        // frame. The first allocation fires `GlyphProtocolInstalled`
        // so the frontend can wire the registry into the font library
        // exactly once per session.
        let was_uninitialised = self.glyph_registry.is_none();
        let registry = self.glyph_registry.get_or_insert_with(GlyphRegistry::new);

        let result = match registry.register_with_width(cp, stored, upm, width) {
            Ok(_evicted) => Ok(()),
            Err(RegisterRejection::OutOfNamespace) => {
                // Unreachable given the is_pua check above, but the
                // registry double-checks so the defence exists.
                Err(RegisterError::OutOfNamespace)
            }
        };

        if was_uninitialised && result.is_ok() {
            let registry = registry.clone();
            self.event_proxy.send_event(
                RioEvent::GlyphProtocolInstalled {
                    route_id: self.route_id,
                    registry,
                },
                self.window_id,
            );
        }

        // §7.3: re-registering a codepoint must repaint cells already
        // showing it — partial damage can't know which rows hold the
        // codepoint, so a register is a full-screen invalidation.
        if result.is_ok() {
            self.mark_fully_damaged();
        }

        result
    }

    #[cfg(feature = "graphics")]
    #[cfg(feature = "graphics")]
    fn glyph_clear(&mut self, cp: Option<u32>) {
        // Nothing to clear if nothing was ever registered.
        let Some(registry) = self.glyph_registry.as_ref() else {
            return;
        };
        match cp {
            None => registry.clear_all(),
            Some(cp) => registry.clear_one(cp),
        }
        // §7.3: cleared codepoints fall back to font rendering on the
        // next frame — repaint everything that might show them.
        self.mark_fully_damaged();
    }

    fn glyph_query(&mut self, cp: u32) {
        // Defer to the frontend: only it has access to both the per-
        // route registry and the FontLibrary, so only it can compute
        // the System / Glossary / Both bits accurately. The frontend
        // formats the reply and writes it back to this pane's PTY
        // asynchronously.
        self.event_proxy.send_event(
            RioEvent::GlyphProtocolQuery {
                route_id: self.route_id,
                cp,
            },
            self.window_id,
        );
    }

    #[inline]
    fn kitty_chunking_state_mut(
        &mut self,
    ) -> Option<&mut crate::ansi::kitty_graphics_protocol::KittyGraphicsState> {
        Some(&mut self.graphics.kitty_chunking_state)
    }
}

pub struct CrosswordsSize {
    pub columns: usize,
    pub screen_lines: usize,
    pub width: u32,
    pub height: u32,
    pub square_width: u32,
    pub square_height: u32,
}

impl CrosswordsSize {
    pub fn new(columns: usize, screen_lines: usize) -> Self {
        Self {
            columns,
            screen_lines,
            width: 0,
            height: 0,
            square_width: 0,
            square_height: 0,
        }
    }

    pub fn new_with_dimensions(
        columns: usize,
        screen_lines: usize,
        width: u32,
        height: u32,
        square_width: u32,
        square_height: u32,
    ) -> Self {
        Self {
            columns,
            screen_lines,
            width,
            height,
            square_width,
            square_height,
        }
    }
}

impl Dimensions for CrosswordsSize {
    fn total_lines(&self) -> usize {
        self.screen_lines()
    }

    fn screen_lines(&self) -> usize {
        self.screen_lines
    }

    fn columns(&self) -> usize {
        self.columns
    }

    fn square_width(&self) -> f32 {
        self.square_width as f32
    }

    fn square_height(&self) -> f32 {
        self.square_height as f32
    }
}

impl<T: EventListener> Dimensions for Crosswords<T> {
    #[inline]
    fn columns(&self) -> usize {
        self.grid.columns()
    }

    #[inline]
    fn screen_lines(&self) -> usize {
        self.grid.screen_lines()
    }

    #[inline]
    fn total_lines(&self) -> usize {
        self.grid.total_lines()
    }

    fn square_width(&self) -> f32 {
        self.graphics.cell_width
    }

    fn square_height(&self) -> f32 {
        self.graphics.cell_height
    }
}

// Additional Crosswords methods (not part of Handler trait)
impl<U: EventListener> Crosswords<U> {
    /// Place a kitty image as an overlay (not in grid cells).
    /// Used for a=T (transmit+display) and a=p (place stored image).
    fn place_kitty_overlay(
        &mut self,
        image_id: u32,
        placement: &crate::ansi::kitty_graphics_protocol::PlacementRequest,
    ) {
        let (image_width, image_height, transmit_time) =
            match self.graphics.get_kitty_image(image_id) {
                Some(s) => (s.data.width, s.data.height, s.transmission_time),
                None => {
                    warn!("place_kitty_overlay: image {} not found", image_id);
                    return;
                }
            };
        if image_width == 0 || image_height == 0 {
            return;
        }

        let cell_width = self.graphics.cell_width.round() as usize;
        let cell_height = self.graphics.cell_height.round() as usize;

        if cell_width == 0 || cell_height == 0 {
            return;
        }

        // Resolve the source rectangle (kitty `x=`/`y=`/`w=`/`h=`)
        // against the image. The crop is what gets displayed; it
        // never affects where the placement lands. Both crop and
        // display size are re-resolved at render time, so these
        // values only drive the grid footprint (spans and cursor
        // movement). A crop fully outside the image still stores the
        // placement — it renders nothing and occupies no cells, and
        // can become visible after a retransmission with larger
        // dimensions.
        let (display_w, display_h) = match crate::ansi::graphics::resolve_source_rect(
            placement.x,
            placement.y,
            placement.width,
            placement.height,
            image_width,
            image_height,
        ) {
            Some((_, _, source_width, source_height)) => {
                crate::ansi::graphics::kitty_display_size(
                    source_width,
                    source_height,
                    placement.columns,
                    placement.rows,
                    cell_width,
                    cell_height,
                )
            }
            None => (0, 0),
        };

        if display_w > MAX_GRAPHIC_DIMENSIONS[0] || display_h > MAX_GRAPHIC_DIMENSIONS[1]
        {
            return;
        }

        // Memory is managed in store_kitty_image (eviction happens there)

        // Per the kitty spec a placement always renders at the cursor
        // position; `x=`/`y=` select the source rectangle within the
        // image, never the destination cell.
        let dest_col = self.grid.cursor.pos.col.0;
        let cursor_row = self.grid.cursor.pos.row.0;
        // Absolute row in the stable space: lines ever evicted off the
        // ring + current history + screen-relative row. Stays glued to
        // content even after scrollback saturates.
        let dest_row = self.grid.lines_evicted() as i64
            + self.history_size() as i64
            + cursor_row as i64;

        // kitty spec the `X=`/`Y=` sub-cell offset must be smaller
        // than the cell size. The stored value stays raw (a later cell
        // size change re-clamps at read time without losing the
        // original); these clamped copies only drive span derivation.
        let cell_x_offset = (placement.cell_x_offset as usize).min(cell_width - 1);
        let cell_y_offset = (placement.cell_y_offset as usize).min(cell_height - 1);

        // Compute cell-based size.
        //
        // the trick is the sub-cell offset shifts the image
        // within its first cell, so it can spill into one extra
        // row/column. Include it so cursor movement and row occupation
        // cover the full image. An invisible placement (degenerate
        // crop) occupies no cells at all.
        let (columns, rows) = if display_w == 0 || display_h == 0 {
            (0, 0)
        } else {
            let columns = if placement.columns > 0 {
                placement.columns
            } else {
                (display_w + cell_x_offset).div_ceil(cell_width) as u32
            };
            let rows = if placement.rows > 0 {
                placement.rows
            } else {
                (display_h + cell_y_offset).div_ceil(cell_height) as u32
            };
            (columns, rows)
        };

        // Create overlay placement.
        //
        // Per kitty spec, when the client doesn't supply `p=` (or sends
        // p=0) the terminal must allocate a unique internal placement_id
        // so multiple placements of the same image don't collide. Without
        // this, two `kitten icat` invocations referencing the same
        // image_id would both store at key (image_id, 0) and only the
        // last one would survive.
        let placement_id = if placement.placement_id == 0 {
            self.graphics.allocate_internal_placement_id()
        } else {
            placement.placement_id
        };
        let kitty_placement = KittyPlacement {
            image_id,
            placement_id,
            source_x: placement.x,
            source_y: placement.y,
            source_width: placement.width,
            source_height: placement.height,
            dest_col,
            dest_row,
            columns,
            rows,
            requested_columns: placement.columns,
            requested_rows: placement.rows,
            pixel_width: display_w as u32,
            pixel_height: display_h as u32,
            cell_x_offset: placement.cell_x_offset,
            cell_y_offset: placement.cell_y_offset,
            z_index: placement.z_index,
            transmit_time,
        };

        self.graphics
            .kitty_placements
            .insert((image_id, placement_id), kitty_placement);
        self.graphics.kitty_graphics_dirty = true;

        self.graphics.queue_kitty_upload(image_id);
        self.send_graphics_updates();

        // Handle cursor movement per kitty spec
        match placement.cursor_movement {
            0 => {
                // C=0: Move cursor to after the image
                self.advance_cursor_past_placement(dest_col, columns, rows);
            }
            1 => {
                // C=1: Don't move cursor
            }
            _ => {
                // Default: treat as C=0
                self.advance_cursor_past_placement(dest_col, columns, rows);
            }
        }
    }

    /// Refresh the grid footprint of every live placement of an image
    /// after its pixel data changed dimensions.
    pub fn refresh_placements_for_image(
        &mut self,
        image_id: u32,
        image_width: usize,
        image_height: usize,
    ) {
        let cell_w = self.graphics.cell_width.round() as usize;
        let cell_h = self.graphics.cell_height.round() as usize;
        for ((id, _), p) in self.graphics.kitty_placements.iter_mut() {
            if *id == image_id {
                p.rescale(image_width, image_height, cell_w, cell_h);
            }
        }
        // A virtual placement keeps its source rect; if the new pixels
        // no longer cover it, fall back to the whole image rather than
        // rendering nothing at the placeholder cells.
        for ((id, _), vp) in self.graphics.kitty_virtual_placements.iter_mut() {
            if *id == image_id
                && (vp.x as usize >= image_width || vp.y as usize >= image_height)
            {
                vp.x = 0;
                vp.y = 0;
                vp.width = 0;
                vp.height = 0;
            }
        }
    }

    /// Queue the stored pixels of `image_id` for the GPU unless the
    /// texture already holds them. Returns whether an upload was queued.
    fn ensure_kitty_upload(&mut self, image_id: u32) -> bool {
        if self.graphics.queue_kitty_upload(image_id) {
            self.graphics.kitty_graphics_dirty = true;
            self.send_graphics_updates();
            true
        } else {
            false
        }
    }

    /// Move the cursor past a placement (`C=0`): linefeed once per
    /// occupied row so the whole image scrolls into view, then land on
    /// the image's last row at the first column after it, clamped to
    /// the grid edge. An invisible placement (zero span) leaves the
    /// cursor untouched.
    fn advance_cursor_past_placement(
        &mut self,
        dest_col: usize,
        columns: u32,
        rows: u32,
    ) {
        if rows == 0 {
            return;
        }
        for _ in 0..rows {
            self.linefeed();
        }
        if self.grid.cursor.pos.row.0 > 0 {
            self.grid.cursor.pos.row -= 1;
        }
        let col = (dest_col + columns as usize).min(self.grid.columns() - 1);
        self.grid.cursor.pos.col = Column(col);
        // Any cursor repositioning discards a pending wrap, otherwise
        // the next printed character wraps a row below the intended
        // caption position.
        self.grid.cursor.should_wrap = false;
    }

    /// Register a virtual placement (kitty graphics `a=p,U=1`).
    ///
    /// Per the spec, this command only declares metadata — it does NOT
    /// write any cells. The application (e.g. `kitten icat
    /// --unicode-placeholder`, see kitty's `kittens/icat/transmit.go:221`)
    /// emits the U+10EEEE placeholder cells itself as ordinary text right
    /// after this APC. The renderer scans visible cells for U+10EEEE,
    /// decodes the image_id from the foreground color and the row/col
    /// indices from the combining-mark diacritics (kitty_virtual::*), and
    /// looks up the metadata stored here to know which image to composite.
    fn place_virtual_graphic(
        &mut self,
        placement: crate::ansi::kitty_graphics_protocol::PlacementRequest,
    ) {
        use crate::ansi::graphics::VirtualPlacement;

        debug!(
            "Virtual placement: image_id={}, placement_id={}, columns={}, rows={}",
            placement.image_id, placement.placement_id, placement.columns, placement.rows
        );

        let vp = VirtualPlacement {
            image_id: placement.image_id,
            placement_id: placement.placement_id,
            columns: placement.columns,
            rows: placement.rows,
            x: placement.x,
            y: placement.y,
            width: placement.width,
            height: placement.height,
        };
        self.graphics
            .kitty_virtual_placements
            .insert((placement.image_id, placement.placement_id), vp);
        self.graphics.kitty_graphics_dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crosswords::pos::{Column, Line, Pos, Side};
    use crate::crosswords::CrosswordsSize;
    use crate::event::VoidListener;

    fn make_crosswords() -> Crosswords<VoidListener> {
        let size = CrosswordsSize::new(4, 4);
        let window_id = crate::event::WindowId::from(0);
        Crosswords::new(size, CursorShape::Block, VoidListener {}, window_id, 0, 10)
    }

    #[test]
    fn swap_alt_reinterns_cursor_style_into_alt_table() {
        use crate::config::colors::{AnsiColor, ColorRgb};
        let mut cw = make_crosswords();

        // Pad the primary table so the carried style's id differs from
        // what a fresh alt-table intern will assign; a foreign id would
        // then resolve to the default fallback instead of the carried
        // style.
        for r in 1..4 {
            cw.terminal_attribute(Attr::Background(AnsiColor::Spec(ColorRgb {
                r,
                g: 0,
                b: 0,
            })));
            cw.input('x');
        }
        let red = AnsiColor::Spec(ColorRgb {
            r: 200,
            g: 10,
            b: 10,
        });
        cw.terminal_attribute(Attr::Background(red));
        cw.input('x');

        cw.swap_alt();

        // The alt cursor template must resolve, in the ALT table, to the
        // style value carried over from the primary screen.
        let style = cw.grid.style_of(&cw.grid.cursor.template);
        assert_eq!(style.bg, red);
        // And the cleared alt cells were stamped with a local id too.
        let sq = cw.grid[Line(0)][Column(0)];
        assert_eq!(cw.grid.style_of(&sq).bg, red);
    }

    #[test]
    fn snapshot_resolves_styles_per_row() {
        use crate::config::colors::{AnsiColor, ColorRgb};
        use crate::crosswords::style::Style;
        use crate::event::TerminalDamage;
        let mut cw = make_crosswords();
        let mut rows = Vec::new();
        let mut row_styles = Vec::new();
        let mut extras = rustc_hash::FxHashMap::default();

        let red = AnsiColor::Spec(ColorRgb { r: 250, g: 0, b: 0 });
        cw.terminal_attribute(Attr::Background(red));
        cw.input('x');
        let damage = cw.peek_damage_event().unwrap();
        cw.snapshot_visible(&damage, &mut rows, &mut row_styles, &mut extras);
        cw.reset_damage();
        assert_eq!(row_styles.len(), rows.len());
        assert_eq!(row_styles[0][0].bg, red);
        assert_eq!(row_styles[0][1].bg, Style::default().bg);

        // Rows copied on earlier frames hold resolved values, so table
        // mutations between frames cannot retint them: only the row the
        // new style landed on changes.
        let blue = AnsiColor::Spec(ColorRgb { r: 0, g: 0, b: 250 });
        cw.goto(Line(1), Column(0));
        cw.terminal_attribute(Attr::Background(blue));
        cw.input('z');
        let damage = cw.peek_damage_event().unwrap();
        cw.snapshot_visible(&damage, &mut rows, &mut row_styles, &mut extras);
        cw.reset_damage();
        assert_eq!(row_styles[0][0].bg, red);
        assert_eq!(row_styles[1][0].bg, blue);

        // An instance switch arrives as Full damage and re-resolves
        // everything against the newly active grid's table.
        cw.swap_alt();
        let damage = cw.peek_damage_event().unwrap();
        assert!(matches!(damage, TerminalDamage::Full));
        cw.snapshot_visible(&damage, &mut rows, &mut row_styles, &mut extras);
        cw.reset_damage();
        // The alt screen was cleared with the carried blue bg template.
        assert_eq!(row_styles[0][0].bg, blue);
    }

    #[test]
    fn style_sweep_bounds_table_under_unique_style_churn() {
        use crate::config::colors::{AnsiColor, ColorRgb};
        let mut cw = make_crosswords();

        // Overwrite one cell with tens of thousands of distinct styles;
        // only a handful stay referenced, so the sweep must keep the
        // table well below the number of styles ever interned.
        let total = 40_000u32;
        for i in 0..total {
            cw.goto(Line(0), Column(0));
            cw.terminal_attribute(Attr::Background(AnsiColor::Spec(ColorRgb {
                r: (i & 0xFF) as u8,
                g: ((i >> 8) & 0xFF) as u8,
                b: ((i >> 16) & 0xFF) as u8,
            })));
            cw.input('x');
        }
        assert!(
            cw.grid.styles().len() < 35_000,
            "sweep never reclaimed ids: table has {} entries after {} interns",
            cw.grid.styles().len(),
            total,
        );

        // The last style written is still live and resolves correctly.
        let last = total - 1;
        let sq = cw.grid[Line(0)][Column(0)];
        assert_eq!(
            cw.grid.style_of(&sq).bg,
            AnsiColor::Spec(ColorRgb {
                r: (last & 0xFF) as u8,
                g: ((last >> 8) & 0xFF) as u8,
                b: ((last >> 16) & 0xFF) as u8,
            }),
        );
    }

    #[test]
    fn styled_write_sets_row_hint() {
        use crate::config::colors::{AnsiColor, ColorRgb};
        let mut cw = make_crosswords();
        cw.input('a');
        assert!(!cw.grid[Line(0)].has_styles);

        cw.terminal_attribute(Attr::Background(AnsiColor::Spec(ColorRgb {
            r: 1,
            g: 2,
            b: 3,
        })));
        cw.input('b');
        assert!(cw.grid[Line(0)].has_styles);
        assert!(!cw.grid[Line(1)].has_styles);
    }

    #[test]
    fn wide_wrap_carries_row_hint_to_next_row() {
        use crate::config::colors::{AnsiColor, ColorRgb};
        let mut cw = make_crosswords();
        cw.terminal_attribute(Attr::Background(AnsiColor::Spec(ColorRgb {
            r: 9,
            g: 9,
            b: 9,
        })));
        // Styled wide char at the last column wraps the base onto the
        // next row; the hint must travel with it.
        cw.goto(Line(0), Column(3));
        cw.input('你');
        assert!(cw.grid[Line(1)].has_styles);
    }

    #[test]
    fn styled_blank_fills_flag_rows_and_bg_only_fills_do_not() {
        use crate::config::colors::{AnsiColor, NamedColor};
        let mut cw = make_crosswords();

        // Truecolor / palette backgrounds erase via bg-only cells, which
        // carry no style id: the hint must stay clear.
        cw.terminal_attribute(Attr::Background(AnsiColor::Indexed(42)));
        cw.erase_chars(Column(2));
        assert!(!cw.grid[Line(0)].has_styles);

        // Special named backgrounds fall back to an interned style, so
        // the fill must flag the row.
        cw.terminal_attribute(Attr::Background(AnsiColor::Named(NamedColor::DimRed)));
        cw.goto(Line(1), Column(0));
        cw.erase_chars(Column(2));
        assert!(cw.grid[Line(1)].has_styles);
    }

    #[test]
    fn scrolled_styled_rows_keep_hint_in_history() {
        use crate::config::colors::{AnsiColor, ColorRgb};
        let mut cw = make_crosswords();
        let teal = AnsiColor::Spec(ColorRgb { r: 0, g: 90, b: 90 });
        cw.terminal_attribute(Attr::Background(teal));
        cw.input('x');
        cw.terminal_attribute(Attr::Reset);

        // Scroll the styled row into history; the flag must travel with
        // it so the sweep keeps its id live.
        for _ in 0..8 {
            cw.goto(Line(3), Column(0));
            cw.linefeed();
        }
        cw.grid.reclaim_styles();
        assert!(cw.grid.styles().iter().any(|s| s.bg == teal));
    }

    #[test]
    fn unstyled_session_never_flags_rows() {
        let mut cw = make_crosswords();
        for _ in 0..3 {
            cw.input('a');
            cw.linefeed();
        }
        let styled_rows = cw.grid.raw.rows().filter(|r| r.has_styles).count();
        assert_eq!(styled_rows, 0);
        // And the sweep over such a grid frees nothing and trips no
        // skip-contract assertion.
        let len = cw.grid.styles().len();
        cw.grid.reclaim_styles();
        assert_eq!(cw.grid.styles().len(), len);
    }

    #[test]
    fn sweep_keeps_styles_through_column_reflow() {
        use crate::config::colors::{AnsiColor, ColorRgb};
        let mut cw = make_crosswords();
        let purple = AnsiColor::Spec(ColorRgb {
            r: 120,
            g: 0,
            b: 200,
        });
        cw.terminal_attribute(Attr::Background(purple));
        for _ in 0..4 {
            cw.input('x');
        }
        cw.terminal_attribute(Attr::Reset);

        // Reflow to narrower and back: styled cells splice across rows,
        // and the conservative row hints must keep their ids live
        // through both sweeps.
        cw.resize(CrosswordsSize::new(2, 4));
        cw.grid.reclaim_styles();
        assert!(cw.grid.styles().iter().any(|s| s.bg == purple));

        cw.resize(CrosswordsSize::new(4, 4));
        cw.grid.reclaim_styles();
        let found = cw.grid.raw.rows().any(|row| {
            row.inner
                .iter()
                .any(|sq| !sq.is_bg_only() && cw.grid.style_of(sq).bg == purple)
        });
        assert!(found, "reflowed styled cells lost their style after sweeps");
    }

    #[test]
    fn sweep_keeps_styles_of_rows_cached_by_a_shrink() {
        use crate::config::colors::{AnsiColor, ColorRgb};
        let mut cw = make_crosswords();

        // Style the bottom row, then move the cursor to the top and
        // reset the SGR state so no cursor template keeps the style
        // alive on its own.
        let green = AnsiColor::Spec(ColorRgb {
            r: 0,
            g: 200,
            b: 50,
        });
        cw.goto(Line(3), Column(0));
        cw.terminal_attribute(Attr::Background(green));
        cw.input('x');
        cw.terminal_attribute(Attr::Reset);
        cw.goto(Line(0), Column(0));
        cw.input('y');

        // Shrinking the window drops the bottom rows into Storage's
        // hidden cache (content intact, re-exposed verbatim by a later
        // grow), so the sweep must keep their style ids live.
        cw.resize(CrosswordsSize::new(4, 2));
        cw.grid.reclaim_styles();
        assert!(
            cw.grid.styles().iter().any(|s| s.bg == green),
            "sweep freed a style still referenced by a cached row",
        );

        // Growing back keeps the row (visible or in history) with its
        // style still resolving against the swept table.
        cw.resize(CrosswordsSize::new(4, 4));
        let found = cw.grid.raw.rows().any(|row| {
            row.inner
                .iter()
                .any(|sq| !sq.is_bg_only() && cw.grid.style_of(sq).bg == green)
        });
        assert!(found, "cached row lost its style across shrink/grow");
    }

    // Minimum-valid simple glyph: one contour, one on-curve point.
    #[cfg(feature = "graphics")]
    fn minimal_glyf_bytes() -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&1i16.to_be_bytes()); // numberOfContours
        v.extend_from_slice(&[0u8; 8]); // bounding box
        v.extend_from_slice(&0u16.to_be_bytes()); // endPtsOfContours[0]
        v.extend_from_slice(&0u16.to_be_bytes()); // instructionLength
        v.push(0x01); // flags: on-curve, no shorts
        v.extend_from_slice(&0i16.to_be_bytes()); // x delta
        v.extend_from_slice(&0i16.to_be_bytes()); // y delta
        v
    }

    #[cfg(feature = "graphics")]
    #[cfg(feature = "graphics")]
    fn glyf_payload(
        bytes: Vec<u8>,
        upm: u16,
    ) -> crate::ansi::glyph_protocol::GlyphPayload {
        crate::ansi::glyph_protocol::GlyphPayload::Glyf { glyf: bytes, upm }
    }

    #[cfg(feature = "graphics")]
    #[cfg(feature = "graphics")]
    fn registry_contains(cw: &Crosswords<VoidListener>, cp: u32) -> bool {
        cw.glyph_registry.as_ref().is_some_and(|r| r.contains(cp))
    }

    #[cfg(feature = "graphics")]
    #[cfg(feature = "graphics")]
    fn registry_len(cw: &Crosswords<VoidListener>) -> usize {
        cw.glyph_registry.as_ref().map_or(0, |r| r.len())
    }

    #[test]
    fn semantic_prompt_marks_and_navigation() {
        use crate::crosswords::grid::row::SemanticPrompt;

        let mut cw = make_crosswords();
        // Three prompts, each followed by two lines of output. The
        // 4-row screen pushes earlier prompts into scrollback.
        for _ in 0..3 {
            cw.set_semantic_prompt(SemanticPrompt::Prompt);
            cw.linefeed();
            cw.linefeed();
            cw.linefeed();
        }
        assert_eq!(cw.grid.history_size(), 6);
        assert_eq!(cw.grid[Line(-6)].semantic_prompt, SemanticPrompt::Prompt);
        assert_eq!(cw.grid[Line(-3)].semantic_prompt, SemanticPrompt::Prompt);
        assert_eq!(cw.grid[Line(0)].semantic_prompt, SemanticPrompt::Prompt);

        cw.scroll_to_prompt(false);
        assert_eq!(cw.display_offset(), 3);
        cw.scroll_to_prompt(false);
        assert_eq!(cw.display_offset(), 6);
        // No prompt further up: stays put.
        cw.scroll_to_prompt(false);
        assert_eq!(cw.display_offset(), 6);

        cw.scroll_to_prompt(true);
        assert_eq!(cw.display_offset(), 3);
        cw.scroll_to_prompt(true);
        assert_eq!(cw.display_offset(), 0);
        cw.scroll_to_prompt(true);
        assert_eq!(cw.display_offset(), 0);
    }

    #[test]
    fn semantic_prompt_run_counts_as_one() {
        use crate::crosswords::grid::row::SemanticPrompt;

        let mut cw = make_crosswords();
        for _ in 0..6 {
            cw.linefeed();
        }
        // A two-row prompt: continuation directly below the start.
        cw.grid[Line(-3)].semantic_prompt = SemanticPrompt::Prompt;
        cw.grid[Line(-2)].semantic_prompt = SemanticPrompt::PromptContinuation;

        cw.scroll_to_prompt(false);
        assert_eq!(cw.display_offset(), 3);
        cw.scroll_to_prompt(false);
        assert_eq!(cw.display_offset(), 3);
    }

    #[test]
    fn shell_integration_oscs_dispatch_from_raw_bytes() {
        use crate::crosswords::grid::row::SemanticPrompt;
        use crate::performer::handler::Processor;

        let size = CrosswordsSize::new(40, 5);
        let window_id = crate::event::WindowId::from(0);
        let mut cw = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );
        let mut processor = Processor::default();

        // A prompt mark, a full A/B/C/D cycle with options, and a
        // user var ("hello" in base64), bell-terminated like shells
        // emit them.
        let bytes = b"]133;A;aid=1prompt]133;Bcmd
]133;Cout
]133;D;0]1337;SetUserVar=foo=aGVsbG8=";
        processor.advance(&mut cw, bytes);

        assert_eq!(cw.grid[Line(0)].semantic_prompt, SemanticPrompt::Prompt);
        assert_eq!(cw.grid[Line(1)].semantic_prompt, SemanticPrompt::None);
        assert_eq!(cw.user_vars.get("foo").map(String::as_str), Some("hello"));
    }

    #[test]
    fn grid_iterator_survives_narrow_rows() {
        let mut cw = make_crosswords();
        for _ in 0..6 {
            cw.linefeed();
        }
        // History rows keep their old length across column growth;
        // simulate one narrower than the grid (#1713).
        cw.grid[Line(-2)].inner.truncate(2);
        let start = Pos::new(Line(-3), Column(0));
        // Must not panic; may end early at the narrow row.
        let _ = cw.grid.iter_from(start).count();

        // A stale position beyond the grid's history must end the
        // iteration instead of panicking.
        let stale = Pos::new(Line(-40), Column(0));
        assert_eq!(cw.grid.iter_from(stale).count(), 0);
    }

    #[test]
    fn user_vars_are_stored() {
        let mut cw = make_crosswords();
        assert!(cw.user_vars.is_empty());
        cw.set_user_var("k".to_string(), "v".to_string());
        cw.set_user_var("k".to_string(), "v2".to_string());
        assert_eq!(cw.user_vars.get("k").map(String::as_str), Some("v2"));
    }

    #[cfg(feature = "graphics")]
    #[cfg(feature = "graphics")]
    #[test]
    fn glyph_registry_is_none_until_first_register() {
        let cw = make_crosswords();
        assert!(cw.glyph_registry.is_none());
    }

    #[cfg(feature = "graphics")]
    #[cfg(feature = "graphics")]
    #[test]
    fn glyph_protocol_register_populates_registry() {
        let mut cw = make_crosswords();

        let glyf = minimal_glyf_bytes();
        // E0A0 is the Powerline branch codepoint — in basic PUA.
        let res = Handler::glyph_register(&mut cw, 0xE0A0, glyf_payload(glyf, 1000), 1);
        assert!(res.is_ok());
        assert!(cw.glyph_registry.is_some());
        assert!(registry_contains(&cw, 0xE0A0));
    }

    #[cfg(feature = "graphics")]
    #[cfg(feature = "graphics")]
    #[test]
    fn glyph_protocol_register_rejects_non_pua() {
        use crate::ansi::glyph_protocol::RegisterError;
        let mut cw = make_crosswords();

        // 0x61 is 'a' — not in PUA. Registry must refuse.
        let res = Handler::glyph_register(
            &mut cw,
            0x61,
            glyf_payload(minimal_glyf_bytes(), 1000),
            1,
        );
        assert_eq!(res, Err(RegisterError::OutOfNamespace));
        assert!(cw.glyph_registry.is_none());
    }

    #[cfg(feature = "graphics")]
    #[cfg(feature = "graphics")]
    #[test]
    fn glyph_protocol_register_rejects_hinted_payload() {
        use crate::ansi::glyph_protocol::RegisterError;
        let mut cw = make_crosswords();

        let mut v = Vec::new();
        v.extend_from_slice(&1i16.to_be_bytes());
        v.extend_from_slice(&[0u8; 8]);
        v.extend_from_slice(&0u16.to_be_bytes()); // endPts[0]
        v.extend_from_slice(&1u16.to_be_bytes()); // instructionLength = 1
        v.push(0x00); // the instruction
        v.push(0x01); // on-curve flag
        v.extend_from_slice(&0i16.to_be_bytes());
        v.extend_from_slice(&0i16.to_be_bytes());

        let res = Handler::glyph_register(&mut cw, 0xE0A0, glyf_payload(v, 1000), 1);
        assert_eq!(res, Err(RegisterError::HintingUnsupported));
        // Decode failed before the registry was touched, so it stays
        // uninitialised.
        assert!(cw.glyph_registry.is_none());
    }

    // A simple glyph declaring `n` points in almost no wire bytes:
    // the §8.2 decode-amplification payload.
    #[cfg(feature = "graphics")]
    fn dense_glyf_bytes(n: usize) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&1i16.to_be_bytes()); // numberOfContours
        v.extend_from_slice(&[0u8; 8]); // bounding box
        v.extend_from_slice(&((n - 1) as u16).to_be_bytes()); // endPts
        v.extend_from_slice(&0u16.to_be_bytes()); // instructionLength
        let flag = 0x01 | 0x08 | 0x10 | 0x20; // on-curve|repeat|x-same|y-same
        let mut left = n;
        while left > 0 {
            let run = left.min(255);
            v.push(flag);
            v.push((run - 1) as u8);
            left -= run;
        }
        v
    }

    #[cfg(feature = "graphics")]
    #[test]
    fn glyph_protocol_register_rejects_oversized_outline() {
        use crate::ansi::glyph_protocol::RegisterError;
        use rio_graphics::glyph::glyf_decode::MAX_DECODED_POINTS;
        let mut cw = make_crosswords();

        let v = dense_glyf_bytes(MAX_DECODED_POINTS + 1);
        let res = Handler::glyph_register(&mut cw, 0xE0A0, glyf_payload(v, 1000), 1);
        assert_eq!(res, Err(RegisterError::OutlineTooLarge));
        assert!(cw.glyph_registry.is_none());
    }

    #[cfg(feature = "graphics")]
    #[test]
    fn glyph_protocol_register_rejects_container_over_budget() {
        use crate::ansi::glyph_protocol::{ColrContainer, GlyphPayload, RegisterError};
        use rio_graphics::glyph::glyf_decode::MAX_DECODED_POINTS;
        let mut cw = make_crosswords();

        // Each outline is under the per-outline budget, but together
        // they blow the per-registration budget. The budget check runs
        // before COLR validation, so junk table bytes are fine here.
        let half = MAX_DECODED_POINTS / 2 + 1;
        let container = ColrContainer {
            glyphs: vec![dense_glyf_bytes(half), dense_glyf_bytes(half)],
            colr: vec![0u8; 4],
            cpal: Vec::new(),
        };
        let res = Handler::glyph_register(
            &mut cw,
            0xE0A0,
            GlyphPayload::ColrV0 {
                container,
                upm: 1000,
            },
            1,
        );
        assert_eq!(res, Err(RegisterError::OutlineTooLarge));
        assert!(cw.glyph_registry.is_none());

        // The same total spread across outlines that stay within the
        // budget registers fine.
        let container = ColrContainer {
            glyphs: vec![
                dense_glyf_bytes(MAX_DECODED_POINTS / 2),
                dense_glyf_bytes(MAX_DECODED_POINTS / 2),
            ],
            colr: vec![0u8; 4],
            cpal: Vec::new(),
        };
        let res = Handler::glyph_register(
            &mut cw,
            0xE0A0,
            GlyphPayload::ColrV0 {
                container,
                upm: 1000,
            },
            1,
        );
        assert!(res.is_ok());
        assert!(registry_contains(&cw, 0xE0A0));
    }

    #[cfg(feature = "graphics")]
    #[cfg(feature = "graphics")]
    #[test]
    fn glyph_protocol_clear_before_any_register_is_noop() {
        let mut cw = make_crosswords();
        Handler::glyph_clear(&mut cw, None);
        // No panic, registry still absent.
        assert!(cw.glyph_registry.is_none());
    }

    #[cfg(feature = "graphics")]
    #[cfg(feature = "graphics")]
    #[test]
    fn glyph_protocol_clear_all_wipes_registry() {
        let mut cw = make_crosswords();
        Handler::glyph_register(
            &mut cw,
            0xE0A0,
            glyf_payload(minimal_glyf_bytes(), 1000),
            1,
        )
        .unwrap();
        Handler::glyph_register(
            &mut cw,
            0xE0A1,
            glyf_payload(minimal_glyf_bytes(), 1000),
            1,
        )
        .unwrap();
        assert_eq!(registry_len(&cw), 2);

        Handler::glyph_clear(&mut cw, None);
        // Clear-all empties the registry but leaves the Arc in place,
        // since a program that cleared once will likely register again.
        assert_eq!(registry_len(&cw), 0);
        assert!(cw.glyph_registry.is_some());
    }

    #[cfg(feature = "graphics")]
    #[cfg(feature = "graphics")]
    #[test]
    fn glyph_protocol_width_does_not_change_layout() {
        use crate::crosswords::square::Wide;
        let mut cw = make_crosswords();

        // A width=2 registration must NOT reserve two cells: the
        // declared width is a render-time hint only, so the logical
        // layout stays consistent with system wcwidth (PUA = 1). This
        // keeps the cursor / selection / copy in sync with width-unaware
        // consumers; the glyph overflows into the next cell in pixels at
        // render time.
        Handler::glyph_register(
            &mut cw,
            0xE0A0,
            glyf_payload(minimal_glyf_bytes(), 1000),
            2,
        )
        .unwrap();
        Handler::input(&mut cw, '\u{E0A0}');

        let row = cw.grid.cursor.pos.row;
        assert!(matches!(cw.grid[row][Column(0)].wide(), Wide::Narrow));
        assert_eq!(
            cw.grid.cursor.pos.col,
            Column(1),
            "registered glyph advances the cursor by one cell"
        );
    }

    #[cfg(feature = "graphics")]
    #[test]
    fn glyph_protocol_register_and_clear_mark_full_damage() {
        let mut cw = make_crosswords();
        cw.reset_damage();
        assert!(!cw.is_fully_damaged());

        Handler::glyph_register(
            &mut cw,
            0xE0A0,
            glyf_payload(minimal_glyf_bytes(), 1000),
            1,
        )
        .unwrap();
        assert!(cw.is_fully_damaged(), "register must repaint (§7.3)");

        cw.reset_damage();
        Handler::glyph_clear(&mut cw, Some(0xE0A0));
        assert!(cw.is_fully_damaged(), "clear must repaint (§7.3)");
    }

    #[cfg(feature = "graphics")]
    #[test]
    fn glyph_protocol_clear_one_leaves_others_intact() {
        let mut cw = make_crosswords();
        Handler::glyph_register(
            &mut cw,
            0xE0A0,
            glyf_payload(minimal_glyf_bytes(), 1000),
            1,
        )
        .unwrap();
        Handler::glyph_register(
            &mut cw,
            0xE0A1,
            glyf_payload(minimal_glyf_bytes(), 1000),
            1,
        )
        .unwrap();

        Handler::glyph_clear(&mut cw, Some(0xE0A0));
        assert!(!registry_contains(&cw, 0xE0A0));
        assert!(registry_contains(&cw, 0xE0A1));
    }

    #[test]
    fn scroll_up() {
        let size = CrosswordsSize::new(1, 10);
        let window_id = crate::event::WindowId::from(0);
        let mut cw = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );
        for i in 0..10 {
            cw.grid[Line(i)][Column(0)].set_c(i as u8 as char);
        }

        cw.grid.scroll_up(&(Line(0)..Line(10)), 2);

        assert_eq!(cw.grid[Line(0)][Column(0)].c(), '\u{2}');
        assert_eq!(cw.grid[Line(0)].occ, 1);
        assert_eq!(cw.grid[Line(1)][Column(0)].c(), '\u{3}');
        assert_eq!(cw.grid[Line(1)].occ, 1);
        assert_eq!(cw.grid[Line(2)][Column(0)].c(), '\u{4}');
        assert_eq!(cw.grid[Line(2)].occ, 1);
        assert_eq!(cw.grid[Line(3)][Column(0)].c(), '\u{5}');
        assert_eq!(cw.grid[Line(3)].occ, 1);
        assert_eq!(cw.grid[Line(4)][Column(0)].c(), '\u{6}');
        assert_eq!(cw.grid[Line(4)].occ, 1);
        assert_eq!(cw.grid[Line(5)][Column(0)].c(), '\u{7}');
        assert_eq!(cw.grid[Line(5)].occ, 1);
        assert_eq!(cw.grid[Line(6)][Column(0)].c(), '\u{8}');
        assert_eq!(cw.grid[Line(6)].occ, 1);
        assert_eq!(cw.grid[Line(7)][Column(0)].c(), '\u{9}');
        assert_eq!(cw.grid[Line(7)].occ, 1);
        assert_eq!(cw.grid[Line(8)][Column(0)].c(), '\0'); // was 0.
        assert_eq!(cw.grid[Line(8)].occ, 0);
        assert_eq!(cw.grid[Line(9)][Column(0)].c(), '\0'); // was 1.
        assert_eq!(cw.grid[Line(9)].occ, 0);
    }

    #[test]
    fn test_linefeed() {
        let size = CrosswordsSize::new(1, 1);
        let window_id = crate::event::WindowId::from(0);

        let mut cw = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );
        assert_eq!(cw.grid.total_lines(), 1);

        cw.linefeed();
        assert_eq!(cw.grid.total_lines(), 2);
    }

    #[test]
    fn test_linefeed_moving_cursor() {
        let size = CrosswordsSize::new(1, 3);

        let window_id = crate::event::WindowId::from(0);

        let mut cw = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );
        let cursor = cw.cursor();
        assert_eq!(cursor.pos.col, 0);
        assert_eq!(cursor.pos.row, 0);

        cw.linefeed();
        let cursor = cw.cursor();
        assert_eq!(cursor.pos.col, 0);
        assert_eq!(cursor.pos.row, 1);

        // Keep adding lines but keep cursor at max row
        for _ in 0..20 {
            cw.linefeed();
        }
        let cursor = cw.cursor();
        assert_eq!(cursor.pos.col, 0);
        assert_eq!(cursor.pos.row, 2);
        assert_eq!(cw.grid.total_lines(), 22);
    }

    #[test]
    fn test_input() {
        let size = CrosswordsSize::new(5, 10);
        let window_id = crate::event::WindowId::from(0);

        let mut cw = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );
        for i in 0..4 {
            cw.grid[Line(0)][Column(i)].set_c(i as u8 as char);
        }
        cw.grid[Line(1)][Column(3)].set_c('b');

        assert_eq!(cw.grid[Line(0)][Column(0)].c(), '\u{0}');
        assert_eq!(cw.grid[Line(0)][Column(1)].c(), '\u{1}');
        assert_eq!(cw.grid[Line(0)][Column(2)].c(), '\u{2}');
        assert_eq!(cw.grid[Line(0)][Column(3)].c(), '\u{3}');
        assert_eq!(cw.grid[Line(0)][Column(4)].c(), '\0');
        assert_eq!(cw.grid[Line(1)][Column(2)].c(), '\0');
        assert_eq!(cw.grid[Line(1)][Column(3)].c(), 'b');
        assert_eq!(cw.grid[Line(0)][Column(4)].c(), '\0');
    }

    /// Drive the parser with an OSC 8 hyperlink and assert that every
    /// cell in the link span carries the same `extras_id`, that the
    /// extras table holds the URI, and that cells outside the span
    /// have no hyperlink. Locks in the per-instance ExtrasTable wiring
    /// added in 2026-04-12 so future regressions in the cell repack
    /// path get caught at test time.
    #[test]
    fn osc8_hyperlink_basic() {
        use crate::performer::handler::Processor;

        let size = CrosswordsSize::new(40, 5);
        let window_id = crate::event::WindowId::from(0);
        let mut cw = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );
        let mut processor = Processor::default();

        // Plain text, then "click" wrapped in an OSC 8, then plain "."
        // The bell-terminated form is what most shells emit.
        let bytes = b"go \x1b]8;;https://example.com\x07click\x1b]8;;\x07.";
        processor.advance(&mut cw, bytes);

        // "go " — three cells, no hyperlink.
        for col in 0..3 {
            assert!(
                cw.cell_hyperlink(Line(0), Column(col)).is_none(),
                "col {} should have no hyperlink",
                col,
            );
            assert!(cw.cell_hyperlink_id(Line(0), Column(col)).is_none());
        }

        // "click" — five cells, all sharing the same extras_id and URI.
        let id = cw
            .cell_hyperlink_id(Line(0), Column(3))
            .expect("expected hyperlink id at col 3");
        for col in 3..8 {
            assert_eq!(
                cw.cell_hyperlink_id(Line(0), Column(col)),
                Some(id),
                "col {} should share the link's extras_id",
                col,
            );
            let hl = cw.cell_hyperlink(Line(0), Column(col)).expect("hyperlink");
            assert_eq!(hl.uri(), "https://example.com");
        }

        // "." — one cell after the OSC 8 reset, no hyperlink.
        assert!(cw.cell_hyperlink(Line(0), Column(8)).is_none());
        assert!(cw.cell_hyperlink_id(Line(0), Column(8)).is_none());
    }

    /// Two distinct hyperlinks back-to-back must use *different*
    /// `extras_id`s so that consumers walking cell-by-cell can detect
    /// the boundary by id comparison.
    #[test]
    fn osc8_hyperlink_two_distinct_links_use_distinct_ids() {
        use crate::performer::handler::Processor;

        let size = CrosswordsSize::new(40, 5);
        let window_id = crate::event::WindowId::from(0);
        let mut cw = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );
        let mut processor = Processor::default();

        let bytes = b"\x1b]8;;https://a.example\x07A\x1b]8;;\x07\
                      \x1b]8;;https://b.example\x07B\x1b]8;;\x07";
        processor.advance(&mut cw, bytes);

        let id_a = cw.cell_hyperlink_id(Line(0), Column(0));
        let id_b = cw.cell_hyperlink_id(Line(0), Column(1));
        assert!(id_a.is_some());
        assert!(id_b.is_some());
        assert_ne!(
            id_a, id_b,
            "two distinct hyperlinks must allocate distinct extras_ids",
        );

        let hl_a = cw.cell_hyperlink(Line(0), Column(0)).unwrap();
        let hl_b = cw.cell_hyperlink(Line(0), Column(1)).unwrap();
        assert_eq!(hl_a.uri(), "https://a.example");
        assert_eq!(hl_b.uri(), "https://b.example");
    }

    /// OSC 8 with no params clears the active hyperlink. Cells written
    /// after the reset must not inherit the previous link.
    #[test]
    fn osc8_hyperlink_reset_clears_template() {
        use crate::performer::handler::Processor;

        let size = CrosswordsSize::new(40, 5);
        let window_id = crate::event::WindowId::from(0);
        let mut cw = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );
        let mut processor = Processor::default();

        // Open a hyperlink, write 'X', close it, then write 'Y'.
        processor.advance(&mut cw, b"\x1b]8;;https://example.com\x07X\x1b]8;;\x07Y");

        // X has the link.
        assert!(cw.cell_hyperlink(Line(0), Column(0)).is_some());
        // Y does not.
        assert!(
            cw.cell_hyperlink(Line(0), Column(1)).is_none(),
            "cells after the OSC 8 reset must not inherit the previous link",
        );
    }

    /// OSC 8 hyperlinks span multiple lines when the parser writes a
    /// linefeed in the middle of the link. The cells on both lines
    /// should carry the same `extras_id` (modulo the cells before /
    /// after the link on those lines).
    #[test]
    fn osc8_hyperlink_spans_linefeed() {
        use crate::performer::handler::Processor;

        let size = CrosswordsSize::new(40, 5);
        let window_id = crate::event::WindowId::from(0);
        let mut cw = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );
        let mut processor = Processor::default();

        // "AB\nCD" with the whole thing inside one OSC 8 link.
        processor.advance(
            &mut cw,
            b"\x1b]8;;https://example.com\x07AB\r\nCD\x1b]8;;\x07",
        );

        let id_a = cw.cell_hyperlink_id(Line(0), Column(0)).unwrap();
        let id_b = cw.cell_hyperlink_id(Line(0), Column(1)).unwrap();
        let id_c = cw.cell_hyperlink_id(Line(1), Column(0)).unwrap();
        let id_d = cw.cell_hyperlink_id(Line(1), Column(1)).unwrap();

        assert_eq!(id_a, id_b);
        assert_eq!(id_b, id_c);
        assert_eq!(id_c, id_d);

        let hl = cw.cell_hyperlink(Line(1), Column(1)).unwrap();
        assert_eq!(hl.uri(), "https://example.com");
    }

    #[test]
    fn test_damage_tracking_after_control_c() {
        let size = CrosswordsSize::new(80, 24);
        let window_id = crate::event::WindowId::from(0);
        let mut cw = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );

        // Simulate fzf-like scenario: write some text
        let test_text = "fzf> search term";
        for ch in test_text.chars() {
            cw.input(ch);
        }

        // Check that input caused damage
        assert!(
            cw.peek_damage_event().is_some(),
            "Input should cause damage"
        );

        // Reset damage to simulate a render cycle completing
        cw.reset_damage();

        // Update last cursor position to match current (simulating that cursor position was rendered)
        cw.damage.last_cursor = cw.grid.cursor.pos;

        // Verify damage was cleared
        assert!(
            cw.peek_damage_event().is_none(),
            "Should have no damage after reset with cursor sync"
        );

        // Simulate Control+C (ETX) - this should clear the line and damage it
        // In fzf, Control+C typically clears the current line and returns to prompt
        cw.carriage_return();
        cw.clear_line(LineClearMode::Right);

        // Check that damage was registered from the clear operation
        let damage = cw.peek_damage_event();
        assert!(
            damage.is_some(),
            "Clear line operation should register damage"
        );

        // Specifically check that it's not just cursor-only damage
        match damage {
            Some(TerminalDamage::Partial) | Some(TerminalDamage::Full) => {
                // Good - line damage was registered
            }
            Some(TerminalDamage::CursorOnly) | Some(TerminalDamage::Noop) => {
                panic!(
                    "Clear line should register line damage, not just cursor movement"
                );
            }
            None => {
                panic!("Clear line should register damage");
            }
        }

        // Verify the line was actually cleared
        let cursor_line = cw.grid.cursor.pos.row;
        for col in 0..test_text.len() {
            assert_eq!(
                cw.grid[cursor_line][Column(col)].c(),
                '\0',
                "Line should be cleared after Control+C"
            );
        }
    }

    #[test]
    fn test_damage_tracking_cursor_movement() {
        let size = CrosswordsSize::new(80, 24);
        let window_id = crate::event::WindowId::from(0);
        let mut cw = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );

        // Write text on multiple lines
        cw.input('A');
        cw.linefeed();
        cw.input('B');
        cw.linefeed();
        cw.input('C');

        // Reset damage
        cw.reset_damage();

        // Move cursor up - should damage both old and new cursor lines
        let old_line = cw.grid.cursor.pos.row;
        cw.move_up(1);
        let new_line = cw.grid.cursor.pos.row;

        // Check that damage was registered
        let damage = cw.peek_damage_event();
        assert!(damage.is_some(), "Cursor movement should register damage");

        // Verify both lines are marked as damaged
        assert_ne!(old_line, new_line, "Cursor should have moved");
    }

    #[test]
    fn test_damage_tracking_clear_operations() {
        let size = CrosswordsSize::new(80, 24);
        let window_id = crate::event::WindowId::from(0);
        let mut cw = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );

        // Fill some lines with content
        for line in 0..5 {
            for col in 0..10 {
                cw.grid[Line(line)][Column(col)].set_c('X');
            }
        }

        // Reset damage
        cw.reset_damage();

        // Clear from cursor to end of line
        cw.grid.cursor.pos = Pos::new(Line(2), Column(5));
        cw.clear_line(LineClearMode::Right);

        // Check damage is registered
        let damage = cw.peek_damage_event();
        assert!(damage.is_some(), "Clear line should register damage");

        // Verify the clear operation
        for col in 5..10 {
            assert_eq!(
                cw.grid[Line(2)][Column(col)].c(),
                '\0',
                "Characters from cursor to end should be cleared"
            );
        }

        // Characters before cursor should remain
        for col in 0..5 {
            assert_eq!(
                cw.grid[Line(2)][Column(col)].c(),
                'X',
                "Characters before cursor should remain"
            );
        }
    }

    #[test]
    fn test_damage_tracking_prompt_redraw() {
        let size = CrosswordsSize::new(80, 24);
        let window_id = crate::event::WindowId::from(0);
        let mut cw = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );

        // Simulate a shell prompt scenario
        let prompt = "$ ";
        for ch in prompt.chars() {
            cw.input(ch);
        }

        // User types a command
        let command = "ls -la";
        for ch in command.chars() {
            cw.input(ch);
        }

        // Reset damage (simulating a render)
        cw.reset_damage();

        // Simulate Control+C: clear line and redraw prompt
        cw.carriage_return();
        cw.clear_line(LineClearMode::Right);

        // Damage should be registered for the cleared line
        assert!(cw.peek_damage_event().is_some(), "Line clear should damage");

        // Write new prompt
        for ch in prompt.chars() {
            cw.input(ch);
        }

        // Verify prompt is displayed correctly
        assert_eq!(cw.grid[cw.grid.cursor.pos.row][Column(0)].c(), '$');
        assert_eq!(cw.grid[cw.grid.cursor.pos.row][Column(1)].c(), ' ');

        // Verify old command is cleared
        for col in 2..8 {
            assert_eq!(
                cw.grid[cw.grid.cursor.pos.row][Column(col)].c(),
                '\0',
                "Old command should be cleared"
            );
        }
    }

    #[test]
    fn simple_selection_works() {
        let size = CrosswordsSize::new(5, 5);
        let window_id = crate::event::WindowId::from(0);

        let mut term = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );
        let grid = &mut term.grid;
        for i in 0..4 {
            if i == 1 {
                continue;
            }

            grid[Line(i)][Column(0)].set_c('"');

            for j in 1..4 {
                grid[Line(i)][Column(j)].set_c('a');
            }

            grid[Line(i)][Column(4)].set_c('"');
        }
        grid[Line(2)][Column(0)].set_c(' ');
        grid[Line(2)][Column(4)].set_c(' ');
        grid[Line(2)][Column(4)].set_wrapline(true);
        grid[Line(3)][Column(0)].set_c(' ');

        // Multiple lines contain an empty line.
        term.selection = Some(Selection::new(
            SelectionType::Simple,
            Pos {
                row: Line(0),
                col: Column(0),
            },
            Side::Left,
        ));
        if let Some(s) = term.selection.as_mut() {
            s.update(
                Pos {
                    row: Line(2),
                    col: Column(4),
                },
                Side::Right,
            );
        }
        // Trailing space on the wrapped row is preserved as a buffered blank
        // and only flushed if a non-blank cell follows on the continuation
        // row. Here the selection ends mid-wrap so the trailing space is dropped.
        assert_eq!(
            term.selection_to_string(),
            Some(String::from("\"aaa\"\n\n aaa"))
        );

        // A wrapline.
        term.selection = Some(Selection::new(
            SelectionType::Simple,
            Pos {
                row: Line(2),
                col: Column(0),
            },
            Side::Left,
        ));
        if let Some(s) = term.selection.as_mut() {
            s.update(
                Pos {
                    row: Line(3),
                    col: Column(4),
                },
                Side::Right,
            );
        }
        assert_eq!(
            term.selection_to_string(),
            Some(String::from(" aaa  aaa\""))
        );
    }

    #[test]
    fn line_selection_works() {
        let size = CrosswordsSize::new(5, 1);
        let window_id = crate::event::WindowId::from(0);

        let mut term = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );
        let mut grid: Grid<Square> = Grid::new(1, 5, 0);
        for i in 0..5 {
            grid[Line(0)][Column(i)].set_c('a');
        }
        grid[Line(0)][Column(0)].set_c('"');
        grid[Line(0)][Column(3)].set_c('"');

        mem::swap(&mut term.grid, &mut grid);

        term.selection = Some(Selection::new(
            SelectionType::Lines,
            Pos {
                row: Line(0),
                col: Column(3),
            },
            Side::Left,
        ));
        assert_eq!(term.selection_to_string(), Some(String::from("\"aa\"a\n")));
    }

    #[test]
    fn block_selection_works() {
        let size = CrosswordsSize::new(5, 5);
        let window_id = crate::event::WindowId::from(0);

        let mut term = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );
        let grid = &mut term.grid;
        for i in 1..4 {
            grid[Line(i)][Column(0)].set_c('"');

            for j in 1..4 {
                grid[Line(i)][Column(j)].set_c('a');
            }

            grid[Line(i)][Column(4)].set_c('"');
        }
        grid[Line(2)][Column(2)].set_c(' ');
        grid[Line(2)][Column(4)].set_wrapline(true);
        grid[Line(3)][Column(4)].set_c(' ');

        term.selection = Some(Selection::new(
            SelectionType::Block,
            Pos {
                row: Line(0),
                col: Column(3),
            },
            Side::Left,
        ));

        // The same column.
        if let Some(s) = term.selection.as_mut() {
            s.update(
                Pos {
                    row: Line(3),
                    col: Column(3),
                },
                Side::Right,
            );
        }
        assert_eq!(term.selection_to_string(), Some(String::from("\na\na\na")));

        // The first column.
        if let Some(s) = term.selection.as_mut() {
            s.update(
                Pos {
                    row: Line(3),
                    col: Column(0),
                },
                Side::Left,
            );
        }
        assert_eq!(
            term.selection_to_string(),
            Some(String::from("\n\"aa\n\"a\n\"aa"))
        );

        // The last column.
        if let Some(s) = term.selection.as_mut() {
            s.update(
                Pos {
                    row: Line(3),
                    col: Column(4),
                },
                Side::Right,
            );
        }
        assert_eq!(
            term.selection_to_string(),
            Some(String::from("\na\"\na\"\na"))
        );
    }

    fn make_term_for_selection(rows: usize, cols: usize) -> Crosswords<VoidListener> {
        let size = CrosswordsSize::new(cols, rows);
        let window_id = crate::event::WindowId::from(0);
        Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        )
    }

    fn select_simple(
        term: &mut Crosswords<VoidListener>,
        start: (i32, usize),
        end: (i32, usize),
    ) {
        term.selection = Some(Selection::new(
            SelectionType::Simple,
            Pos {
                row: Line(start.0),
                col: Column(start.1),
            },
            Side::Left,
        ));
        if let Some(s) = term.selection.as_mut() {
            s.update(
                Pos {
                    row: Line(end.0),
                    col: Column(end.1),
                },
                Side::Right,
            );
        }
    }

    /// `\0` cells in the middle of a run of content must be emitted as ASCII
    /// spaces, not raw NULs. This is the "TUI redrew its UI and left holes"
    /// case (e.g. fullscreen apps that paint with cursor positioning).
    #[test]
    fn null_cells_inside_run_become_spaces() {
        let mut term = make_term_for_selection(1, 7);
        let grid = &mut term.grid;
        // Row layout: a, a, \0, \0, \0, b, b
        grid[Line(0)][Column(0)].set_c('a');
        grid[Line(0)][Column(1)].set_c('a');
        grid[Line(0)][Column(5)].set_c('b');
        grid[Line(0)][Column(6)].set_c('b');

        select_simple(&mut term, (0, 0), (0, 6));
        let s = term.selection_to_string().unwrap();
        assert_eq!(s, "aa   bb");
        assert!(!s.contains('\0'), "selection must not contain raw NULs");
    }

    /// Trailing `\0` and trailing spaces on a non-wrapped row must be dropped
    /// rather than padding the copy out to column width.
    #[test]
    fn trailing_blanks_on_non_wrapped_row_are_dropped() {
        let mut term = make_term_for_selection(1, 8);
        let grid = &mut term.grid;
        grid[Line(0)][Column(0)].set_c('h');
        grid[Line(0)][Column(1)].set_c('i');
        grid[Line(0)][Column(2)].set_c(' ');
        grid[Line(0)][Column(3)].set_c(' ');
        // cols 4..=7 stay as \0

        select_simple(&mut term, (0, 0), (0, 7));
        assert_eq!(term.selection_to_string(), Some(String::from("hi")));
    }

    /// Trailing blank rows in a multi-row selection must be dropped, not
    /// emitted as a run of `\n`s.
    #[test]
    fn trailing_blank_rows_are_dropped() {
        let mut term = make_term_for_selection(5, 5);
        let grid = &mut term.grid;
        grid[Line(0)][Column(0)].set_c('x');
        grid[Line(0)][Column(1)].set_c('y');
        // Rows 1..=4 are entirely \0.

        select_simple(&mut term, (0, 0), (4, 4));
        assert_eq!(term.selection_to_string(), Some(String::from("xy")));
    }

    /// Blank rows between non-blank rows must still be emitted as `\n`s, so
    /// real visual gaps in the selection are preserved.
    #[test]
    fn blank_rows_between_content_are_preserved() {
        let mut term = make_term_for_selection(5, 5);
        let grid = &mut term.grid;
        grid[Line(0)][Column(0)].set_c('a');
        // Rows 1, 2 entirely \0.
        grid[Line(3)][Column(0)].set_c('b');
        // Row 4 entirely \0.

        select_simple(&mut term, (0, 0), (4, 4));
        assert_eq!(term.selection_to_string(), Some(String::from("a\n\n\nb")));
    }

    /// When a row wraps into the next, the trailing-space buffer must carry
    /// across so the visual gap survives the round-trip through the clipboard.
    #[test]
    fn trailing_space_carries_across_wrap_continuation() {
        let mut term = make_term_for_selection(2, 5);
        let grid = &mut term.grid;
        // Row 0: "ab " with a wrap into row 1.
        grid[Line(0)][Column(0)].set_c('a');
        grid[Line(0)][Column(1)].set_c('b');
        grid[Line(0)][Column(2)].set_c(' ');
        grid[Line(0)][Column(3)].set_c(' ');
        grid[Line(0)][Column(4)].set_c(' ');
        grid[Line(0)][Column(4)].set_wrapline(true);
        // Row 1: " cd "
        grid[Line(1)][Column(0)].set_c(' ');
        grid[Line(1)][Column(1)].set_c('c');
        grid[Line(1)][Column(2)].set_c('d');
        grid[Line(1)][Column(3)].set_c(' ');
        grid[Line(1)][Column(4)].set_c(' ');

        // Trailing spaces on row 1 dropped (no further continuation), but the
        // 4 spaces between `b` and `c` must survive across the wrap.
        select_simple(&mut term, (0, 0), (1, 4));
        assert_eq!(term.selection_to_string(), Some(String::from("ab    cd")));
    }

    #[test]
    fn parse_cargo_version() {
        assert_eq!(version_number("0.0.1-nightly"), 1);
        assert_eq!(version_number("0.1.2-nightly"), 1_02);
        assert_eq!(version_number("1.2.3-nightly"), 1_02_03);
        assert_eq!(version_number("999.99.99"), 9_99_99_99);
    }

    #[test]
    fn test_cursor_damage_after_clear() {
        use crate::ansi::CursorShape;
        use crate::crosswords::CrosswordsSize;
        use crate::event::{VoidListener, WindowId};
        use crate::performer::handler::Handler;

        let size = CrosswordsSize::new(10, 10);
        let window_id = WindowId::from(0);
        let mut term = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );

        // Move cursor to position (1, 5) and type some text
        term.goto(Line(1), Column(5));
        for c in "hello".chars() {
            term.input(c);
        }

        // Get initial damage and reset
        {
            let _initial_damage = term.damage();
        }
        term.reset_damage();

        // Simulate `clear` command: clear screen and move cursor to home
        term.clear_screen(ClearMode::All);
        term.goto(Line(0), Column(0));

        // Verify cursor is at origin
        assert_eq!(term.grid.cursor.pos.row, Line(0));
        assert_eq!(term.grid.cursor.pos.col, Column(0));

        // Reset damage after clear
        {
            let _clear_damage = term.damage();
        }
        term.reset_damage();

        // Now type "aa" - both characters should trigger line damage
        term.input('a');

        // Check damage after first 'a' - should damage entire line 0
        let has_damage_first = {
            let damage_after_first_a = term.damage();
            match damage_after_first_a {
                TermDamage::Partial(iter) => {
                    let damaged_lines: Vec<_> = iter.collect();
                    !damaged_lines.is_empty()
                        && damaged_lines.iter().any(|line| line.line == 0)
                }
                TermDamage::Full => true,
            }
        };
        assert!(has_damage_first, "First 'a' should cause line damage");
        term.reset_damage();

        term.input('a');

        // Check damage after second 'a' - should also damage entire line 0
        let has_damage_second = {
            let damage_after_second_a = term.damage();
            match damage_after_second_a {
                TermDamage::Partial(iter) => {
                    let damaged_lines: Vec<_> = iter.collect();
                    !damaged_lines.is_empty()
                        && damaged_lines.iter().any(|line| line.line == 0)
                }
                TermDamage::Full => true,
            }
        };
        assert!(has_damage_second, "Second 'a' should cause line damage");
        term.reset_damage();

        // Verify final cursor position
        assert_eq!(term.grid.cursor.pos.row, Line(0));
        assert_eq!(term.grid.cursor.pos.col, Column(2)); // After typing "aa"
    }

    #[test]
    fn test_line_damage_approach() {
        use crate::ansi::CursorShape;
        use crate::crosswords::CrosswordsSize;
        use crate::event::{VoidListener, WindowId};

        let size = CrosswordsSize::new(10, 10);
        let window_id = WindowId::from(0);
        let mut term = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );

        // Reset damage to start clean
        term.reset_damage();

        // Move cursor to line 2 and damage that line
        term.goto(Line(2), Column(3));
        term.damage_cursor_line();

        let damage_result = {
            let damage = term.damage();
            match damage {
                TermDamage::Partial(iter) => {
                    let damaged_lines: Vec<_> = iter.collect();
                    damaged_lines
                        .iter()
                        .find(|line| line.line == 2)
                        .map(|line| line.damaged)
                }
                TermDamage::Full => Some(true), // Full damage
            }
        };

        // Should damage line 2
        assert_eq!(damage_result, Some(true), "Should damage line 2");
        term.reset_damage();

        // Test the general damage_line method
        term.damage_line(5);

        let damage_result_2 = {
            let damage = term.damage();
            match damage {
                TermDamage::Partial(iter) => {
                    let damaged_lines: Vec<_> = iter.collect();
                    damaged_lines
                        .iter()
                        .find(|line| line.line == 5)
                        .map(|line| line.damaged)
                }
                TermDamage::Full => Some(true),
            }
        };

        // Should damage line 5
        assert_eq!(damage_result_2, Some(true), "Should damage line 5");
    }

    /// Unit tests for keyboard mode stack functionality
    /// These tests verify the push, pop, and set operations for the keyboard mode stack
    /// which was refactored in commit 7cfd5f73a1934f641174ed3fe335b6f37cb75316
    ///
    /// Test coverage:
    /// - test_keyboard_mode_push_pop: Basic push and pop operations
    /// - test_keyboard_mode_stack_wraparound: Stack overflow protection and wraparound
    /// - test_keyboard_mode_pop_excessive: Handling of excessive pop operations
    /// - test_keyboard_mode_set_replace: Replace behavior for keyboard modes
    /// - test_keyboard_mode_set_union: Union behavior for keyboard modes
    /// - test_keyboard_mode_set_difference: Difference behavior for keyboard modes
    /// - test_keyboard_mode_report: Current mode reporting functionality
    /// - test_keyboard_mode_reset: Terminal reset behavior on keyboard stack
    /// - test_keyboard_mode_stack_underflow_protection: Stack underflow protection

    #[test]
    fn test_keyboard_mode_push_pop() {
        let size = CrosswordsSize::new(10, 10);
        let window_id = WindowId::from(0);
        let mut term = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );

        // Initial state: stack should be empty with NO_MODE
        assert_eq!(
            term.keyboard_mode_stack[term.keyboard_mode_idx],
            KeyboardModes::NO_MODE.bits()
        );
        assert_eq!(term.keyboard_mode_idx, 0);

        // Push first mode using Handler trait
        Handler::push_keyboard_mode(&mut term, KeyboardModes::DISAMBIGUATE_ESC_CODES);
        assert_eq!(term.keyboard_mode_idx, 1);
        assert_eq!(
            term.keyboard_mode_stack[1],
            KeyboardModes::DISAMBIGUATE_ESC_CODES.bits()
        );

        // Push second mode
        Handler::push_keyboard_mode(&mut term, KeyboardModes::REPORT_EVENT_TYPES);
        assert_eq!(term.keyboard_mode_idx, 2);
        assert_eq!(
            term.keyboard_mode_stack[2],
            KeyboardModes::REPORT_EVENT_TYPES.bits()
        );

        // Pop one mode using Handler trait
        Handler::pop_keyboard_modes(&mut term, 1);
        assert_eq!(term.keyboard_mode_idx, 1);
        assert_eq!(term.keyboard_mode_stack[2], KeyboardModes::NO_MODE.bits()); // Should be cleared

        // Pop remaining mode
        Handler::pop_keyboard_modes(&mut term, 1);
        assert_eq!(term.keyboard_mode_idx, 0);
        assert_eq!(term.keyboard_mode_stack[1], KeyboardModes::NO_MODE.bits()); // Should be cleared
    }

    #[test]
    fn test_keyboard_mode_stack_wraparound() {
        let size = CrosswordsSize::new(10, 10);
        let window_id = WindowId::from(0);
        let mut term = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );

        // Fill the stack to maximum depth using Handler trait
        for i in 0..KEYBOARD_MODE_STACK_MAX_DEPTH {
            Handler::push_keyboard_mode(&mut term, KeyboardModes::DISAMBIGUATE_ESC_CODES);
            assert_eq!(
                term.keyboard_mode_idx,
                (i + 1) % KEYBOARD_MODE_STACK_MAX_DEPTH
            );
        }

        // Push one more - should wrap around
        Handler::push_keyboard_mode(&mut term, KeyboardModes::REPORT_EVENT_TYPES);
        assert_eq!(term.keyboard_mode_idx, 1); // Should wrap to 1
        assert_eq!(
            term.keyboard_mode_stack[1],
            KeyboardModes::REPORT_EVENT_TYPES.bits()
        );
    }

    #[test]
    fn test_keyboard_mode_pop_excessive() {
        let size = CrosswordsSize::new(10, 10);
        let window_id = WindowId::from(0);
        let mut term = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );

        // Push a few modes using Handler trait
        Handler::push_keyboard_mode(&mut term, KeyboardModes::DISAMBIGUATE_ESC_CODES);
        Handler::push_keyboard_mode(&mut term, KeyboardModes::REPORT_EVENT_TYPES);
        Handler::push_keyboard_mode(&mut term, KeyboardModes::REPORT_ALTERNATE_KEYS);

        // Pop more modes than exist - should clear everything
        Handler::pop_keyboard_modes(&mut term, KEYBOARD_MODE_STACK_MAX_DEPTH as u16);

        assert_eq!(term.keyboard_mode_idx, 0);
        // All modes should be cleared to NO_MODE
        for i in 0..KEYBOARD_MODE_STACK_MAX_DEPTH {
            assert_eq!(term.keyboard_mode_stack[i], KeyboardModes::NO_MODE.bits());
        }
    }

    #[test]
    fn test_keyboard_mode_set_replace() {
        let size = CrosswordsSize::new(10, 10);
        let window_id = WindowId::from(0);
        let mut term = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );

        // Set initial mode using Handler trait method
        Handler::set_keyboard_mode(
            &mut term,
            KeyboardModes::DISAMBIGUATE_ESC_CODES,
            KeyboardModesApplyBehavior::Replace,
        );
        assert_eq!(
            term.keyboard_mode_stack[term.keyboard_mode_idx],
            KeyboardModes::DISAMBIGUATE_ESC_CODES.bits()
        );

        // Replace with different mode
        Handler::set_keyboard_mode(
            &mut term,
            KeyboardModes::REPORT_EVENT_TYPES,
            KeyboardModesApplyBehavior::Replace,
        );
        assert_eq!(
            term.keyboard_mode_stack[term.keyboard_mode_idx],
            KeyboardModes::REPORT_EVENT_TYPES.bits()
        );
    }

    #[test]
    fn test_keyboard_mode_set_union() {
        let size = CrosswordsSize::new(10, 10);
        let window_id = WindowId::from(0);
        let mut term = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );

        // Set initial mode using Handler trait method
        Handler::set_keyboard_mode(
            &mut term,
            KeyboardModes::DISAMBIGUATE_ESC_CODES,
            KeyboardModesApplyBehavior::Replace,
        );

        // Add another mode using union
        Handler::set_keyboard_mode(
            &mut term,
            KeyboardModes::REPORT_EVENT_TYPES,
            KeyboardModesApplyBehavior::Union,
        );

        let expected = KeyboardModes::DISAMBIGUATE_ESC_CODES.bits()
            | KeyboardModes::REPORT_EVENT_TYPES.bits();
        assert_eq!(term.keyboard_mode_stack[term.keyboard_mode_idx], expected);
    }

    #[test]
    fn test_keyboard_mode_set_difference() {
        let size = CrosswordsSize::new(10, 10);
        let window_id = WindowId::from(0);
        let mut term = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );

        // Set combined mode using Handler trait method
        let combined_mode =
            KeyboardModes::DISAMBIGUATE_ESC_CODES | KeyboardModes::REPORT_EVENT_TYPES;
        Handler::set_keyboard_mode(
            &mut term,
            combined_mode,
            KeyboardModesApplyBehavior::Replace,
        );

        // Remove one mode using difference
        Handler::set_keyboard_mode(
            &mut term,
            KeyboardModes::REPORT_EVENT_TYPES,
            KeyboardModesApplyBehavior::Difference,
        );

        assert_eq!(
            term.keyboard_mode_stack[term.keyboard_mode_idx],
            KeyboardModes::DISAMBIGUATE_ESC_CODES.bits()
        );
    }

    #[test]
    fn test_keyboard_mode_report() {
        let size = CrosswordsSize::new(10, 10);
        let window_id = WindowId::from(0);
        let listener = VoidListener {};
        let mut term =
            Crosswords::new(size, CursorShape::Block, listener, window_id, 0, 10_000);

        // Push a mode and test reporting using Handler trait
        Handler::push_keyboard_mode(&mut term, KeyboardModes::DISAMBIGUATE_ESC_CODES);

        // The report_keyboard_mode function sends an event through event_proxy
        // We can't easily test the exact output without mocking the event system,
        // but we can verify the current mode is correctly retrieved
        let current_mode = term.keyboard_mode_stack[term.keyboard_mode_idx];
        assert_eq!(current_mode, KeyboardModes::DISAMBIGUATE_ESC_CODES.bits());
    }

    #[test]
    fn test_keyboard_mode_reset() {
        let size = CrosswordsSize::new(10, 10);
        let window_id = WindowId::from(0);
        let mut term = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );

        // Push several modes
        Handler::push_keyboard_mode(&mut term, KeyboardModes::DISAMBIGUATE_ESC_CODES);
        Handler::push_keyboard_mode(&mut term, KeyboardModes::REPORT_EVENT_TYPES);
        Handler::push_keyboard_mode(&mut term, KeyboardModes::REPORT_ALTERNATE_KEYS);

        // Reset terminal using Handler trait - use reset_state method
        term.reset_state();

        // Verify stack is reset
        assert_eq!(term.keyboard_mode_idx, 0);
        assert_eq!(term.inactive_keyboard_mode_idx, 0);
        for i in 0..KEYBOARD_MODE_STACK_MAX_DEPTH {
            assert_eq!(term.keyboard_mode_stack[i], 0);
            assert_eq!(term.inactive_keyboard_mode_stack[i], 0);
        }
    }

    #[test]
    fn test_keyboard_mode_stack_underflow_protection() {
        let size = CrosswordsSize::new(10, 10);
        let window_id = WindowId::from(0);
        let mut term = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );

        // Start at index 0, try to pop using Handler trait - should wrap correctly
        assert_eq!(term.keyboard_mode_idx, 0);

        Handler::pop_keyboard_modes(&mut term, 1);

        // With wraparound logic, index should wrap to max-1
        let expected_idx = (0_usize.wrapping_sub(1)) % KEYBOARD_MODE_STACK_MAX_DEPTH;
        assert_eq!(term.keyboard_mode_idx, expected_idx);
        assert_eq!(term.keyboard_mode_stack[0], KeyboardModes::NO_MODE.bits()); // Should be cleared
    }

    #[test]
    fn test_xtversion_report() {
        use std::cell::RefCell;
        use std::rc::Rc;

        // Create a custom event listener that captures PtyWrite events
        #[derive(Clone)]
        struct TestListener {
            events: Rc<RefCell<Vec<RioEvent>>>,
        }

        impl EventListener for TestListener {
            fn send_event(&self, event: RioEvent, _id: WindowId) {
                self.events.borrow_mut().push(event);
            }
        }

        let size = CrosswordsSize::new(10, 10);
        let window_id = WindowId::from(0);
        let events = Rc::new(RefCell::new(Vec::new()));
        let listener = TestListener {
            events: events.clone(),
        };
        let mut term =
            Crosswords::new(size, CursorShape::Block, listener, window_id, 0, 10_000);

        // Call report_version using Handler trait
        Handler::report_version(&mut term);

        // Verify that a PtyWrite event was sent
        let captured_events = events.borrow();
        assert_eq!(captured_events.len(), 1, "Should have sent one event");

        // Verify the event is PtyWrite with the correct format
        match &captured_events[0] {
            RioEvent::PtyWrite(_route_id, text) => {
                // Expected format: DCS > | Rio {version} ST
                // DCS = \x1bP, ST = \x1b\\
                assert!(
                    text.starts_with("\x1bP>|Rio "),
                    "Should start with DCS>|Rio"
                );
                assert!(text.ends_with("\x1b\\"), "Should end with ST");

                // Extract version from the response
                let version = env!("CARGO_PKG_VERSION");
                let expected = format!("\x1bP>|Rio {}\x1b\\", version);
                assert_eq!(
                    text, &expected,
                    "XTVERSION response should match expected format"
                );
            }
            other => panic!("Expected PtyWrite event, got {:?}", other),
        }
    }

    #[test]
    fn set_title_emits_title_event() {
        use std::cell::RefCell;
        use std::rc::Rc;

        #[derive(Clone)]
        struct TestListener {
            events: Rc<RefCell<Vec<RioEvent>>>,
        }

        impl EventListener for TestListener {
            fn send_event(&self, event: RioEvent, _id: WindowId) {
                self.events.borrow_mut().push(event);
            }
        }

        let events = Rc::new(RefCell::new(Vec::new()));
        let mut term = Crosswords::new(
            CrosswordsSize::new(10, 10),
            CursorShape::Block,
            TestListener {
                events: events.clone(),
            },
            WindowId::from(0),
            0,
            10,
        );

        Handler::set_title(&mut term, Some("my title".to_string()));

        assert_eq!(term.title, "my title");
        assert!(
            events
                .borrow()
                .iter()
                .any(|e| matches!(e, RioEvent::Title(t) if t == "my title")),
            "set_title should emit RioEvent::Title"
        );
    }

    #[test]
    fn test_keyboard_mode_syncs_with_mode() {
        let size = CrosswordsSize::new(10, 10);
        let window_id = WindowId::from(0);
        let mut term = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );

        // Initially, no keyboard mode should be set
        assert!(!term.mode().contains(Mode::DISAMBIGUATE_ESC_CODES));
        assert!(!term.mode().contains(Mode::REPORT_ALL_KEYS_AS_ESC));

        // Push DISAMBIGUATE_ESC_CODES
        Handler::push_keyboard_mode(&mut term, KeyboardModes::DISAMBIGUATE_ESC_CODES);
        assert!(
            term.mode().contains(Mode::DISAMBIGUATE_ESC_CODES),
            "mode() should contain DISAMBIGUATE_ESC_CODES after push"
        );
        assert!(!term.mode().contains(Mode::REPORT_ALL_KEYS_AS_ESC));

        // Push REPORT_ALL_KEYS_AS_ESC (replaces previous mode at this stack level)
        Handler::push_keyboard_mode(&mut term, KeyboardModes::REPORT_ALL_KEYS_AS_ESC);
        assert!(
            term.mode().contains(Mode::REPORT_ALL_KEYS_AS_ESC),
            "mode() should contain REPORT_ALL_KEYS_AS_ESC after push"
        );
        assert!(!term.mode().contains(Mode::DISAMBIGUATE_ESC_CODES),
            "mode() should not contain DISAMBIGUATE_ESC_CODES after pushing different mode"
        );

        // Pop back to previous level
        Handler::pop_keyboard_modes(&mut term, 1);
        assert!(
            term.mode().contains(Mode::DISAMBIGUATE_ESC_CODES),
            "mode() should contain DISAMBIGUATE_ESC_CODES after pop"
        );
        assert!(
            !term.mode().contains(Mode::REPORT_ALL_KEYS_AS_ESC),
            "mode() should not contain REPORT_ALL_KEYS_AS_ESC after pop"
        );

        // Test set_keyboard_mode with Union
        Handler::set_keyboard_mode(
            &mut term,
            KeyboardModes::REPORT_EVENT_TYPES,
            KeyboardModesApplyBehavior::Union,
        );
        assert!(
            term.mode().contains(Mode::DISAMBIGUATE_ESC_CODES),
            "mode() should still contain DISAMBIGUATE_ESC_CODES after union"
        );
        assert!(
            term.mode().contains(Mode::REPORT_EVENT_TYPES),
            "mode() should contain REPORT_EVENT_TYPES after union"
        );

        // Test set_keyboard_mode with Replace
        Handler::set_keyboard_mode(
            &mut term,
            KeyboardModes::REPORT_ALTERNATE_KEYS,
            KeyboardModesApplyBehavior::Replace,
        );
        assert!(
            term.mode().contains(Mode::REPORT_ALTERNATE_KEYS),
            "mode() should contain REPORT_ALTERNATE_KEYS after replace"
        );
        assert!(
            !term.mode().contains(Mode::DISAMBIGUATE_ESC_CODES),
            "mode() should not contain DISAMBIGUATE_ESC_CODES after replace"
        );
        assert!(
            !term.mode().contains(Mode::REPORT_EVENT_TYPES),
            "mode() should not contain REPORT_EVENT_TYPES after replace"
        );
    }

    /// Insert a small sixel graphic and verify that every cell in the
    /// image span carries the GRAPHICS flag with a valid extras_id
    /// pointing to a GraphicCell in the extras table.
    #[test]
    fn sixel_stores_placement_spanning_cells() {
        let size = CrosswordsSize::new(20, 10);
        let window_id = crate::event::WindowId::from(0);
        let mut cw = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );
        cw.graphics.cell_width = 10.0;
        cw.graphics.cell_height = 10.0;

        let graphic = GraphicData {
            id: rio_graphics::GraphicId::new(0),
            width: 20,
            height: 20,
            pixels: vec![0u8; 20 * 20 * 4],
            color_type: rio_graphics::ColorType::Rgba,
            is_opaque: true,
            display_width: None,
            display_height: None,
            resize: None,
            transmit_time: crate::time::Instant::now(),
        };

        cw.insert_graphic(graphic, None, None);

        // One placement spanning rows 0..2 x cols 0..2; cells stay
        // untouched (placements are the single source of truth).
        assert_eq!(cw.graphics.atlas_placements.len(), 1);
        let p = &cw.graphics.atlas_placements[0];
        assert_eq!((p.abs_row, p.col, p.columns, p.rows), (0, 0, 2, 2));
        assert!(cw.grid[Line(0)][Column(0)].extras_id().is_none());
        // The placement holds the image key alive.
        assert_eq!(cw.graphics.atlas_key_refs.len(), 1);
    }

    /// Verify that `cell_graphic()` reads the first GraphicCell back.
    #[test]
    fn insert_graphic_creates_and_replaces_placements() {
        let size = CrosswordsSize::new(20, 10);
        let window_id = crate::event::WindowId::from(0);
        let mut cw = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );
        cw.graphics.cell_width = 10.0;
        cw.graphics.cell_height = 10.0;

        let graphic = GraphicData {
            id: rio_graphics::GraphicId::new(0),
            width: 10,
            height: 10,
            pixels: vec![0u8; 10 * 10 * 4],
            color_type: rio_graphics::ColorType::Rgba,
            is_opaque: true,
            display_width: None,
            display_height: None,
            resize: None,
            transmit_time: crate::time::Instant::now(),
        };
        cw.insert_graphic(graphic.clone(), None, Some(1));

        assert_eq!(cw.graphics.atlas_placements.len(), 1);
        let p = &cw.graphics.atlas_placements[0];
        assert_eq!((p.abs_row, p.col, p.columns, p.rows), (0, 0, 1, 1));

        // A second image fully covering the first replaces it.
        cw.grid.cursor.pos = Pos::new(Line(0), Column(0));
        cw.insert_graphic(graphic, None, Some(1));
        assert_eq!(cw.graphics.atlas_placements.len(), 1);
        assert_eq!(cw.graphics.atlas_key_refs.len(), 1);
    }

    /// Headless embedders never set `graphics.cell_width`/`cell_height`;
    /// inserting a graphic then must be a no-op instead of panicking in the
    /// row-fill loop (`step_by(0)`).
    #[test]
    fn insert_graphic_without_cell_dimensions_is_noop() {
        let size = CrosswordsSize::new(20, 10);
        let window_id = crate::event::WindowId::from(0);
        let mut cw = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );

        let graphic = GraphicData {
            id: rio_graphics::GraphicId::new(0),
            width: 10,
            height: 10,
            pixels: vec![0u8; 10 * 10 * 4],
            color_type: rio_graphics::ColorType::Rgba,
            is_opaque: true,
            display_width: None,
            display_height: None,
            resize: None,
            transmit_time: crate::time::Instant::now(),
        };
        cw.insert_graphic(graphic, None, Some(1));
        assert!(cw.graphics.atlas_placements.is_empty());
    }

    // ------------------------------------------------------------------
    // Emoji presentation variation selectors (VS15 / VS16).
    // See `input()` + `apply_emoji_vs16` / `apply_emoji_vs15`.
    // ------------------------------------------------------------------

    fn new_term(cols: usize, rows: usize) -> Crosswords<VoidListener> {
        let size = CrosswordsSize::new(cols, rows);
        let window_id = crate::event::WindowId::from(0);
        Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        )
    }

    #[test]
    fn vs16_widens_text_presentation_emoji() {
        use crate::performer::handler::Handler;
        let mut cw = new_term(10, 3);
        // 🎟 (U+1F39F, EAW=N, default text presentation) then VS16.
        cw.input('\u{1F39F}');
        cw.input('\u{FE0F}');

        let row = Line(0);
        assert_eq!(cw.grid[row][Column(0)].c(), '\u{1F39F}');
        assert_eq!(cw.grid[row][Column(0)].wide(), Wide::Wide);
        assert_eq!(cw.grid[row][Column(1)].wide(), Wide::Spacer);
        assert_eq!(cw.grid.cursor.pos.col, Column(2));
        assert!(!cw.grid.cursor.should_wrap);
        // VS16 still attached as combining mark to the base cell.
        let extras_id = cw.grid[row][Column(0)].extras_id();
        assert!(extras_id.is_some());
    }

    #[test]
    fn vs16_on_non_emoji_base_leaves_cell_narrow() {
        use crate::performer::handler::Handler;
        let mut cw = new_term(10, 3);
        cw.input('a');
        cw.input('\u{FE0F}');

        let row = Line(0);
        assert_eq!(cw.grid[row][Column(0)].c(), 'a');
        assert_eq!(cw.grid[row][Column(0)].wide(), Wide::Narrow);
        assert_eq!(cw.grid[row][Column(1)].wide(), Wide::Narrow);
        assert_eq!(cw.grid.cursor.pos.col, Column(1));
    }

    #[test]
    fn vs16_on_already_wide_emoji_is_noop() {
        use crate::performer::handler::Handler;
        let mut cw = new_term(10, 3);
        // 🤝 (U+1F91D, EAW=W, already wide).
        cw.input('\u{1F91D}');
        cw.input('\u{FE0F}');

        let row = Line(0);
        assert_eq!(cw.grid[row][Column(0)].wide(), Wide::Wide);
        assert_eq!(cw.grid[row][Column(1)].wide(), Wide::Spacer);
        assert_eq!(cw.grid.cursor.pos.col, Column(2));
    }

    #[test]
    fn vs15_narrows_default_emoji() {
        use crate::performer::handler::Handler;
        let mut cw = new_term(10, 3);
        // 👍 (U+1F44D THUMBS UP) defaults to emoji presentation. It is
        // listed in emoji-variation-sequences.txt with a VS15 narrowing
        // sequence (unlike e.g. 🤝 U+1F91D, which has no VS entry at all).
        cw.input('\u{1F44D}');
        cw.input('\u{FE0E}');

        let row = Line(0);
        assert_eq!(cw.grid[row][Column(0)].c(), '\u{1F44D}');
        assert_eq!(cw.grid[row][Column(0)].wide(), Wide::Narrow);
        assert_eq!(cw.grid[row][Column(1)].wide(), Wide::Narrow);
        assert_eq!(cw.grid.cursor.pos.col, Column(1));
        assert!(!cw.grid.cursor.should_wrap);
    }

    #[test]
    fn vs15_on_non_listed_emoji_is_noop() {
        use crate::performer::handler::Handler;
        let mut cw = new_term(10, 3);
        // 🤝 (U+1F91D) is default-emoji but has no VS entry, so VS15
        // cannot narrow it — the grid must keep the Wide+Spacer pair.
        cw.input('\u{1F91D}');
        cw.input('\u{FE0E}');

        let row = Line(0);
        assert_eq!(cw.grid[row][Column(0)].wide(), Wide::Wide);
        assert_eq!(cw.grid[row][Column(1)].wide(), Wide::Spacer);
        assert_eq!(cw.grid.cursor.pos.col, Column(2));
    }

    #[test]
    fn vs16_at_column_zero_noop() {
        use crate::performer::handler::Handler;
        let mut cw = new_term(10, 3);
        // VS16 with no preceding base must be ignored.
        cw.input('\u{FE0F}');
        let row = Line(0);
        assert_eq!(cw.grid[row][Column(0)].wide(), Wide::Narrow);
        assert_eq!(cw.grid.cursor.pos.col, Column(0));
    }

    /// Repairing a stranded spacer at the row's last column must keep
    /// WRAPLINE: the flag marks the logical line for reflow, and the
    /// op never targeted that cell.
    #[test]
    fn seam_repair_preserves_wrapline() {
        use crate::performer::handler::Handler;
        let mut cw = new_term(4, 3);
        cw.input('a');
        cw.input('a');
        cw.input('\u{5BBD}'); // Wide at 2, Spacer at 3
        cw.input('b'); // wraps; WRAPLINE lands on the spacer at (0,3)
        assert!(cw.grid[Line(0)][Column(3)].wrapline());
        cw.goto(Line(0), Column(2));
        cw.erase_chars(Column(1)); // blank the lead; sweep repairs the spacer
        assert_eq!(cw.grid[Line(0)][Column(3)].wide(), Wide::Narrow);
        assert!(cw.grid[Line(0)][Column(3)].wrapline());
    }

    /// ED partial-row clears split pairs at the cursor boundary too.
    #[test]
    fn clear_screen_partial_rows_repair_seams() {
        use crate::performer::handler::Handler;
        // Above: cursor on the lead → clears [..=0], strands the spacer.
        let mut cw = new_term(6, 3);
        cw.input('\u{5BBD}'); // Wide at 0, Spacer at 1
        cw.goto(Line(0), Column(0));
        cw.clear_screen(ClearMode::Above);
        assert_eq!(cw.grid[Line(0)][Column(1)].wide(), Wide::Narrow);

        // Below: cursor on the spacer → clears [2..], strands the lead.
        let mut cw = new_term(6, 3);
        cw.input('a');
        cw.input('\u{5BBD}'); // Wide at 1, Spacer at 2
        cw.goto(Line(0), Column(2));
        cw.clear_screen(ClearMode::Below);
        assert_eq!(cw.grid[Line(0)][Column(1)].wide(), Wide::Narrow);
        assert_eq!(cw.grid[Line(0)][Column(1)].c(), '\0');
    }

    /// DCH dragging a row-final LeadingSpacer into the interior leaves
    /// meaningless padding; the sweep reverts it to narrow.
    #[test]
    fn delete_chars_normalizes_interior_leading_spacer() {
        use crate::performer::handler::Handler;
        let mut cw = new_term(4, 3);
        cw.input('a');
        cw.input('a');
        cw.input('a');
        cw.input('\u{5BBD}'); // no room: LeadingSpacer at (0,3), pair wraps
        assert_eq!(cw.grid[Line(0)][Column(3)].wide(), Wide::LeadingSpacer);
        cw.goto(Line(0), Column(0));
        cw.delete_chars(1); // LeadingSpacer shifts to col 2
        assert_eq!(cw.grid[Line(0)][Column(2)].wide(), Wide::Narrow);
    }

    /// ECH erasing only the spacer half of a wide pair must clear the
    /// stranded lead too, mirroring `write_cell`'s overwrite cleanup.
    #[test]
    fn erase_chars_repairs_split_wide_pair() {
        use crate::performer::handler::Handler;
        let mut cw = new_term(6, 2);
        cw.input('\u{5BBD}'); // 宽: Wide at 0, Spacer at 1
        cw.goto(Line(0), Column(1));
        cw.erase_chars(Column(1)); // erase just the spacer
        assert_eq!(cw.grid[Line(0)][Column(0)].wide(), Wide::Narrow);
        assert_eq!(cw.grid[Line(0)][Column(0)].c(), '\0');
        assert_eq!(cw.grid[Line(0)][Column(1)].wide(), Wide::Narrow);

        // And the mirror case: erase through the lead, strand the spacer.
        let mut cw = new_term(6, 2);
        cw.input('a');
        cw.input('\u{5BBD}'); // Wide at 1, Spacer at 2
        cw.goto(Line(0), Column(0));
        cw.erase_chars(Column(2)); // erases 'a' + the lead
        assert_eq!(cw.grid[Line(0)][Column(2)].wide(), Wide::Narrow);
        assert_eq!(cw.grid[Line(0)][Column(2)].c(), '\0');
    }

    /// DCH shifting a spacer into a position whose lead was deleted
    /// must not leave the orphan behind.
    #[test]
    fn delete_chars_repairs_orphaned_spacer() {
        use crate::performer::handler::Handler;
        let mut cw = new_term(6, 2);
        cw.input('a');
        cw.input('\u{5BBD}'); // a at 0, Wide at 1, Spacer at 2
        cw.goto(Line(0), Column(1));
        cw.delete_chars(1); // deletes the lead; spacer shifts to col 1
        assert_eq!(cw.grid[Line(0)][Column(1)].wide(), Wide::Narrow);
        assert_eq!(cw.grid[Line(0)][Column(1)].c(), '\0');
    }

    /// ICH pushing a wide pair so its lead lands on the last column
    /// (spacer pushed off the row) must clear the stranded lead.
    #[test]
    fn insert_blank_repairs_pair_pushed_off_the_edge() {
        use crate::performer::handler::Handler;
        let mut cw = new_term(4, 2);
        cw.goto(Line(0), Column(2));
        cw.input('\u{5BBD}'); // Wide at 2, Spacer at 3
        cw.goto(Line(0), Column(0));
        cw.insert_blank(1); // lead lands at 3, spacer falls off
        assert_eq!(cw.grid[Line(0)][Column(3)].wide(), Wide::Narrow);
        assert_eq!(cw.grid[Line(0)][Column(3)].c(), '\0');
    }

    /// EL to the left of the cursor erases the lead of a pair the
    /// cursor sits on; the spacer past the cleared range must go too.
    #[test]
    fn clear_line_left_repairs_stranded_spacer() {
        use crate::performer::handler::Handler;
        let mut cw = new_term(6, 2);
        cw.input('\u{5BBD}'); // Wide at 0, Spacer at 1
        cw.goto(Line(0), Column(0));
        cw.clear_line(LineClearMode::Left); // clears only col 0
        assert_eq!(cw.grid[Line(0)][Column(1)].wide(), Wide::Narrow);
        assert_eq!(cw.grid[Line(0)][Column(1)].c(), '\0');
    }

    /// Clearing the wide char at column 0 of a wrapped row must revert
    /// the `LeadingSpacer` closing the previous row.
    #[test]
    fn clear_line_reverts_stale_leading_spacer() {
        use crate::performer::handler::Handler;
        let mut cw = new_term(3, 3);
        cw.input('a');
        cw.input('a');
        cw.input('\u{5BBD}'); // no room: LeadingSpacer at (0,2), pair on row 1
        assert_eq!(cw.grid[Line(0)][Column(2)].wide(), Wide::LeadingSpacer);
        cw.goto(Line(1), Column(0));
        cw.clear_line(LineClearMode::All);
        assert_eq!(cw.grid[Line(0)][Column(2)].wide(), Wide::Narrow);
    }

    #[test]
    fn vs16_at_last_column_wraps_base_to_next_row() {
        use crate::performer::handler::Handler;
        // Width 3 so that a 1-cell base at col 2 has no room for a spacer.
        let mut cw = new_term(3, 3);
        cw.input('a');
        cw.input('a');
        cw.input('\u{1F39F}');
        // Cursor at col 2 with should_wrap=true after writing base in last col.
        assert!(cw.grid.cursor.should_wrap);
        cw.input('\u{FE0F}');

        // Old row's last cell is now a LeadingSpacer signalling that a wide
        // glyph continues on the wrapped line. The base char itself moves to
        // (1, 0) marked Wide, with a Spacer at (1, 1).
        assert_eq!(cw.grid[Line(0)][Column(0)].c(), 'a');
        assert_eq!(cw.grid[Line(0)][Column(1)].c(), 'a');
        assert_eq!(cw.grid[Line(0)][Column(2)].wide(), Wide::LeadingSpacer);

        assert_eq!(cw.grid[Line(1)][Column(0)].c(), '\u{1F39F}');
        assert_eq!(cw.grid[Line(1)][Column(0)].wide(), Wide::Wide);
        assert_eq!(cw.grid[Line(1)][Column(1)].wide(), Wide::Spacer);

        assert_eq!(cw.grid.cursor.pos.row, Line(1));
        assert_eq!(cw.grid.cursor.pos.col, Column(2));
        assert!(!cw.grid.cursor.should_wrap);
    }

    /// A combining mark aimed at a bg-only cell must be dropped: those
    /// cells store color where the extras id lives, so reading an id
    /// there would clone an unrelated slot (someone's hyperlink) onto
    /// the cell, and writing one back would corrupt the color.
    #[test]
    fn combining_mark_on_bg_only_cell_is_dropped() {
        use crate::performer::handler::Handler;
        let mut cw = new_term(6, 3);
        // Keep an early slot id live so a garbage id would hit it.
        let link = Hyperlink::new(None::<&str>, "https://x.example");
        cw.set_hyperlink(Some(link));
        cw.input('a');
        cw.set_hyperlink(None);
        // Colored bg-only blanks.
        cw.terminal_attribute(Attr::Background(crate::config::colors::AnsiColor::Spec(
            ColorRgb { r: 0, g: 0, b: 200 },
        )));
        cw.goto(Line(1), Column(0));
        cw.erase_chars(Column(3));
        let before = cw.grid[Line(1)][Column(0)];
        assert!(!matches!(
            before.content_tag(),
            crate::crosswords::square::ContentTag::Codepoint
        ));
        // Mark lands after the blank at column 0.
        cw.goto(Line(1), Column(1));
        cw.input('\u{301}');
        let after = cw.grid[Line(1)][Column(0)];
        assert_eq!(after, before, "bg-only cell must be untouched");
        assert!(!after.has_grapheme());
    }

    /// An OSC 8 hyperlink left open across an alt-screen switch must be
    /// re-interned into the alt grid's table: extras ids are
    /// table-local, and the seeded cursor template would otherwise
    /// stamp a foreign id onto every alt-screen cell.
    fn term_2027(cols: usize, rows: usize) -> Crosswords<VoidListener> {
        use crate::ansi::mode::PrivateMode;
        use crate::performer::handler::Handler;
        let mut cw = new_term(cols, rows);
        cw.set_private_mode(PrivateMode::new(2027));
        cw
    }

    fn extras_of(cw: &Crosswords<VoidListener>, line: i32, col: usize) -> Vec<char> {
        cw.grid[Line(line)][Column(col)]
            .extras_id()
            .and_then(|id| cw.grid.extras_table.get(id))
            .map(|e| e.zerowidth.clone())
            .unwrap_or_default()
    }

    /// ZWJ emoji occupy one two-cell slot under mode 2027 (GB11),
    /// and shatter into per-emoji cells without it.
    #[test]
    fn mode_2027_zwj_emoji_is_one_cluster() {
        use crate::performer::handler::Handler;
        let farmer = ['\u{1F9D1}', '\u{200D}', '\u{1F33E}'];

        let mut cw = term_2027(10, 2);
        for c in farmer {
            cw.input(c);
        }
        assert_eq!(cw.grid[Line(0)][Column(0)].c(), '\u{1F9D1}');
        assert_eq!(cw.grid[Line(0)][Column(0)].wide(), Wide::Wide);
        assert_eq!(cw.grid[Line(0)][Column(1)].wide(), Wide::Spacer);
        assert_eq!(extras_of(&cw, 0, 0), ['\u{200D}', '\u{1F33E}']);
        assert_eq!(cw.grid.cursor.pos.col, Column(2));

        // Legacy (consumer opt-out): the trailing emoji starts its
        // own pair.
        let mut cw = new_term(10, 2);
        cw.set_grapheme_clustering(false);
        for c in farmer {
            cw.input(c);
        }
        assert_eq!(cw.grid[Line(0)][Column(2)].c(), '\u{1F33E}');
        assert_eq!(cw.grid.cursor.pos.col, Column(4));
    }

    /// Regional indicators pair up (GB12/13): two RIs form one wide
    /// cluster, the third starts a new cell.
    #[test]
    fn mode_2027_regional_indicators_pair() {
        use crate::performer::handler::Handler;
        let mut cw = term_2027(10, 2);
        let ri = ['\u{1F1E7}', '\u{1F1F7}', '\u{1F1E6}'];
        for c in ri {
            cw.input(c);
        }
        // First pair: one wide cluster.
        assert_eq!(cw.grid[Line(0)][Column(0)].wide(), Wide::Wide);
        assert_eq!(extras_of(&cw, 0, 0), ['\u{1F1F7}']);
        // Third RI: fresh narrow cell (parity even at the boundary).
        assert_eq!(cw.grid[Line(0)][Column(2)].c(), '\u{1F1E6}');
        assert_eq!(cw.grid[Line(0)][Column(2)].wide(), Wide::Narrow);
    }

    /// Indic conjuncts join across the linker (GB9c) and the cluster
    /// goes wide the moment a second width-bearing codepoint joins.
    #[test]
    fn mode_2027_indic_conjunct_joins() {
        use crate::performer::handler::Handler;
        let mut cw = term_2027(10, 2);
        for c in ['\u{915}', '\u{94D}', '\u{937}'] {
            cw.input(c);
        }
        assert_eq!(cw.grid[Line(0)][Column(0)].c(), '\u{915}');
        assert_eq!(cw.grid[Line(0)][Column(0)].wide(), Wide::Wide);
        assert_eq!(extras_of(&cw, 0, 0), ['\u{94D}', '\u{937}']);
        assert_eq!(cw.grid.cursor.pos.col, Column(2));
    }

    /// A valid emoji variation sequence widens within the cluster; an
    /// invalid selector is ignored outright rather
    /// than attached like the legacy path does.
    #[test]
    fn mode_2027_variation_selectors() {
        use crate::performer::handler::Handler;
        let mut cw = term_2027(10, 2);
        cw.input('\u{1F39F}'); // text-presentation default, narrow
        assert_eq!(cw.grid[Line(0)][Column(0)].wide(), Wide::Narrow);
        cw.input('\u{FE0F}');
        assert_eq!(cw.grid[Line(0)][Column(0)].wide(), Wide::Wide);
        assert_eq!(extras_of(&cw, 0, 0), ['\u{FE0F}']);

        let mut cw = term_2027(10, 2);
        cw.input('a');
        cw.input('\u{FE0F}'); // 'a' is no emoji base: dropped entirely
        assert_eq!(extras_of(&cw, 0, 0), Vec::<char>::new());
        assert!(!cw.grid[Line(0)][Column(0)].has_grapheme());
        assert_eq!(cw.grid.cursor.pos.col, Column(1));
    }

    /// A skin-tone modifier (width-bearing Extend) joins its emoji
    /// without growing the already-wide cluster.
    #[test]
    fn mode_2027_skin_tone_joins() {
        use crate::performer::handler::Handler;
        let mut cw = term_2027(10, 2);
        cw.input('\u{1F44B}');
        cw.input('\u{1F3FB}');
        assert_eq!(cw.grid[Line(0)][Column(0)].wide(), Wide::Wide);
        assert_eq!(extras_of(&cw, 0, 0), ['\u{1F3FB}']);
        assert_eq!(cw.grid.cursor.pos.col, Column(2));
    }

    /// Plain combining marks behave as before: joined, still narrow.
    /// Plain text never joins anything.
    #[test]
    fn mode_2027_narrow_cases() {
        use crate::performer::handler::Handler;
        let mut cw = term_2027(10, 2);
        cw.input('e');
        cw.input('\u{301}');
        cw.input('x');
        assert_eq!(cw.grid[Line(0)][Column(0)].wide(), Wide::Narrow);
        assert_eq!(extras_of(&cw, 0, 0), ['\u{301}']);
        assert_eq!(cw.grid[Line(0)][Column(1)].c(), 'x');
    }

    /// A width-bearing continuation arriving when the narrow cluster
    /// sits at the last column widens it through the wrap dance:
    /// LeadingSpacer stays behind, the cluster moves to the next row.
    #[test]
    fn mode_2027_cluster_widens_across_row_edge() {
        use crate::performer::handler::Handler;
        let mut cw = term_2027(3, 3);
        cw.input('a');
        cw.input('a');
        cw.input('\u{1F1E7}'); // narrow RI at the last column
        cw.input('\u{1F1F7}'); // pairs: cluster must go wide → wraps
        assert_eq!(cw.grid[Line(0)][Column(2)].wide(), Wide::LeadingSpacer);
        assert_eq!(cw.grid[Line(1)][Column(0)].c(), '\u{1F1E7}');
        assert_eq!(cw.grid[Line(1)][Column(0)].wide(), Wide::Wide);
        assert_eq!(extras_of(&cw, 1, 0), ['\u{1F1F7}']);
    }

    /// The bulk decode path must route through the cluster machine:
    /// a ZWJ sequence arriving as one codepoint batch clusters the
    /// same as scalar input.
    #[test]
    fn mode_2027_bulk_codepoints_cluster() {
        use crate::performer::handler::Handler;
        let mut cw = term_2027(10, 2);
        cw.input_codepoints(&[0x1F9D1, 0x200D, 0x1F33E, 0x61]);
        assert_eq!(cw.grid[Line(0)][Column(0)].wide(), Wide::Wide);
        assert_eq!(extras_of(&cw, 0, 0), ['\u{200D}', '\u{1F33E}']);
        assert_eq!(cw.grid[Line(0)][Column(2)].c(), 'a');
        assert_eq!(cw.grid.cursor.pos.col, Column(3));
    }

    /// Reflow moves a cluster atomically: the wide pair and its
    /// attached codepoints survive shrink-rewrap and grow-unwrap.
    #[test]
    fn mode_2027_cluster_survives_reflow() {
        use crate::performer::handler::Handler;
        let mut cw = term_2027(6, 3);
        cw.input('a');
        cw.input('b');
        for c in ['\u{1F9D1}', '\u{200D}', '\u{1F33E}'] {
            cw.input(c);
        }

        // Shrink: the cluster no longer fits after "ab" and rewraps.
        cw.resize(CrosswordsSize::new(3, 3));
        let mut found = None;
        for line in cw.grid.topmost_line().0..3 {
            for col in 0..3 {
                let cell = cw.grid[Line(line)][Column(col)];
                if cell.c() == '\u{1F9D1}' {
                    found = Some((line, col));
                }
            }
        }
        let (line, col) = found.expect("cluster base survives shrink");
        assert_eq!(cw.grid[Line(line)][Column(col)].wide(), Wide::Wide);
        assert_eq!(extras_of(&cw, line, col), ['\u{200D}', '\u{1F33E}']);

        // Grow back: still one intact cluster.
        cw.resize(CrosswordsSize::new(6, 3));
        let mut found = None;
        for line in cw.grid.topmost_line().0..3 {
            for col in 0..6 {
                let cell = cw.grid[Line(line)][Column(col)];
                if cell.c() == '\u{1F9D1}' {
                    found = Some((line, col));
                }
            }
        }
        let (line, col) = found.expect("cluster base survives grow");
        assert_eq!(cw.grid[Line(line)][Column(col)].wide(), Wide::Wide);
        assert_eq!(extras_of(&cw, line, col), ['\u{200D}', '\u{1F33E}']);

        // The moved cells' rows must keep the extras hint, or reclaim
        // would sweep the live slot out from under them.
        cw.grid.reclaim_extras();
        assert_eq!(extras_of(&cw, line, col), ['\u{200D}', '\u{1F33E}']);
    }

    /// Copy/serialize emits the full cluster text, and replaying that
    /// text into a fresh mode-2027 terminal reproduces the same cells.
    #[test]
    fn mode_2027_cluster_round_trips_through_text() {
        use crate::performer::handler::Handler;
        let mut cw = term_2027(10, 2);
        cw.input('a');
        for c in ['\u{1F9D1}', '\u{200D}', '\u{1F33E}'] {
            cw.input(c);
        }
        cw.input('b');

        let text = cw
            .bounds_to_string(Pos::new(Line(0), Column(0)), Pos::new(Line(0), Column(4)));
        assert_eq!(text, "a\u{1F9D1}\u{200D}\u{1F33E}b");

        let mut replay = term_2027(10, 2);
        for c in text.chars() {
            replay.input(c);
        }
        assert_eq!(replay.grid[Line(0)][Column(1)].wide(), Wide::Wide);
        assert_eq!(extras_of(&replay, 0, 1), ['\u{200D}', '\u{1F33E}']);
        assert_eq!(replay.grid[Line(0)][Column(3)].c(), 'b');
    }

    #[test]
    fn hyperlink_crosses_alt_screen_into_the_alt_table() {
        use crate::performer::handler::Handler;
        let mut cw = new_term(6, 3);
        let link = Hyperlink::new(None::<&str>, "https://alt.example");
        cw.set_hyperlink(Some(link.clone()));
        cw.swap_alt();
        cw.input('x');
        assert_eq!(cw.cell_hyperlink(Line(0), Column(0)), Some(link));
        cw.swap_alt();
    }

    fn partial_damage_lines(cw: &mut Crosswords<VoidListener>) -> Vec<usize> {
        match cw.damage() {
            TermDamage::Full => panic!("expected partial damage"),
            TermDamage::Partial(iter) => {
                let mut lines: Vec<usize> = iter.map(|d| d.line).collect();
                lines.sort_unstable();
                lines
            }
        }
    }

    /// A combining mark arriving in its own damage window (its base was
    /// painted and the damage consumed) must damage the base's row: the
    /// attach mutates a painted cell. This is the mechanism behind the
    /// 0.5.20 "stale glyphs until you scroll" report.
    #[test]
    fn cluster_attach_records_damage() {
        use crate::performer::handler::Handler;
        let mut cw = term_2027(8, 3);
        cw.input('e');
        cw.reset_damage(); // renderer consumed the frame with the base

        cw.input('\u{0301}');
        assert_eq!(extras_of(&cw, 0, 0), ['\u{0301}']);
        assert_eq!(partial_damage_lines(&mut cw), [0]);
    }

    /// Same contract on the legacy wcwidth path (clustering opted out):
    /// the zero-width attach predates mode 2027 and had the same gap.
    #[test]
    fn legacy_zero_width_attach_records_damage() {
        use crate::performer::handler::Handler;
        let mut cw = new_term(8, 3);
        cw.set_grapheme_clustering(false);
        cw.input('e');
        cw.reset_damage();

        cw.input('\u{0301}');
        assert_eq!(extras_of(&cw, 0, 0), ['\u{0301}']);
        assert_eq!(partial_damage_lines(&mut cw), [0]);
    }

    /// A VS16 that widens the cluster damages the row through both the
    /// widen and the attach; the selector must also land in the extras.
    #[test]
    fn vs16_widen_records_damage() {
        use crate::performer::handler::Handler;
        let mut cw = term_2027(8, 3);
        cw.input('\u{2764}'); // text-default heart, narrow
        cw.reset_damage();

        cw.input('\u{FE0F}');
        assert_eq!(cw.grid[Line(0)][Column(0)].wide(), Wide::Wide);
        assert_eq!(extras_of(&cw, 0, 0), ['\u{FE0F}']);
        assert!(partial_damage_lines(&mut cw).contains(&0));
    }

    /// Damage soundness: every row whose visible content changed since
    /// the last reset must be covered by the damage the terminal
    /// reports. Feeds randomized VT streams (clusters, wide chars,
    /// erases, scrolls, alt screen) and diffs full row snapshots. Bare
    /// continuation codepoints are separate chunks deliberately: an
    /// attach that lands in its own damage window is exactly the
    /// "stale glyphs until you scroll" class of bug.
    #[test]
    fn damage_covers_every_content_change() {
        use crate::performer::handler::Processor;

        const COLS: usize = 40;
        const ROWS: usize = 12;

        fn row_snapshot(
            cw: &Crosswords<VoidListener>,
        ) -> Vec<Vec<(char, u8, Vec<char>)>> {
            (0..ROWS)
                .map(|r| {
                    (0..COLS)
                        .map(|c| {
                            let sq = cw.grid[Line(r as i32)][Column(c)];
                            let zw = sq
                                .extras_id()
                                .and_then(|id| cw.grid.extras_table.get(id))
                                .map(|e| e.zerowidth.clone())
                                .unwrap_or_default();
                            (sq.c(), sq.wide() as u8, zw)
                        })
                        .collect()
                })
                .collect()
        }

        fn damaged_rows(cw: &mut Crosswords<VoidListener>) -> Vec<bool> {
            match cw.damage() {
                TermDamage::Full => vec![true; ROWS],
                TermDamage::Partial(iter) => {
                    let mut out = vec![false; ROWS];
                    for d in iter {
                        if d.line < ROWS {
                            out[d.line] = true;
                        }
                    }
                    out
                }
            }
        }

        // Deterministic LCG so failures reproduce.
        let mut state: u64 = 0x2027_5EED;
        let mut rng = move |n: usize| {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((state >> 33) as usize) % n
        };

        let chunks: &[&[u8]] = &[
            b"hello world 12345",
            b"\x1b[H",
            b"\x1b[3;7H5",
            b"\r\n",
            "你好世界".as_bytes(),
            "🧑\u{200D}🌾".as_bytes(),
            "e\u{301}".as_bytes(),
            "❤\u{FE0F}".as_bytes(),
            "👍🏻x".as_bytes(),
            // Bare continuations: attach to whatever the previous step
            // left at the cursor, inside a fresh damage window.
            "\u{301}".as_bytes(),
            "\u{200D}🌾".as_bytes(),
            "\u{FE0F}".as_bytes(),
            "🏻".as_bytes(),
            b"\x1b[K",
            b"\x1b[2K",
            b"\x1b[J",
            b"\x1b[2J",
            b"\x1b[4X",
            b"\x1b[3@",
            b"\x1b[2P",
            b"\x1b[2L",
            b"\x1b[2M",
            b"\x1b[3;9r\x1b[2S",
            b"\x1b[2T\x1b[r",
            b"\x1b[?1049h",
            b"\x1b[?1049l",
            b"\x1b[3b",
            b"\x1b[10;20Hmid",
        ];

        let mut cw: Crosswords<VoidListener> = Crosswords::new(
            CrosswordsSize::new(COLS, ROWS),
            CursorShape::Block,
            VoidListener,
            WindowId::from(0),
            0,
            50,
        );
        let mut parser = Processor::default();
        cw.reset_damage();
        let mut prev = row_snapshot(&cw);

        for step in 0..800 {
            for _ in 0..(1 + rng(3)) {
                let chunk = chunks[rng(chunks.len())];
                parser.advance(&mut cw, chunk);
            }
            let damaged = damaged_rows(&mut cw);
            cw.reset_damage();
            let now = row_snapshot(&cw);
            for r in 0..ROWS {
                if now[r] != prev[r] {
                    assert!(
                        damaged[r],
                        "step {step}: row {r} content changed without damage"
                    );
                }
            }
            prev = now;
        }
    }

    fn scrolled_back_term() -> Crosswords<VoidListener> {
        use crate::crosswords::grid::Scroll;
        use crate::performer::handler::Handler;
        // Small scrollback cap so the tests can sit at the boundary.
        let mut cw = Crosswords::new(
            CrosswordsSize::new(10, 6),
            CursorShape::Block,
            VoidListener {},
            crate::event::WindowId::from(0),
            0,
            40,
        );
        // Overfill scrollback so history sits at its cap.
        for i in 0..300 {
            cw.input((b'a' + (i % 26) as u8) as char);
            cw.linefeed();
            cw.carriage_return();
        }
        cw.scroll_display(Scroll::Top);
        assert!(cw.display_offset() > 0);
        cw.reset_damage();
        cw
    }

    /// A sub-region scroll (IL/DL, scroll region not starting at the
    /// top) adds nothing to history, so it must not move a viewport
    /// pinned into scrollback. The drifted offset made
    /// `compute_index` resolve visible lines to wrong storage slots:
    /// arbitrary rows painted at arbitrary positions.
    #[test]
    fn sub_region_scroll_keeps_display_offset() {
        use crate::performer::handler::Handler;
        let mut cw = scrolled_back_term();
        let offset = cw.display_offset();

        // DL and IL at a mid-screen cursor line.
        cw.grid.cursor.pos = Pos::new(Line(2), Column(0));
        cw.delete_lines(2);
        assert_eq!(cw.display_offset(), offset, "DL moved the offset");
        cw.insert_blank_lines(2);
        assert_eq!(cw.display_offset(), offset, "IL moved the offset");
        assert!(
            cw.display_offset() <= cw.history_size(),
            "offset past available history"
        );
    }

    /// A top-anchored region scroll grows history; a viewport pinned
    /// in scrollback absorbs it (same absolute rows stay visible), so
    /// only the region lines are damaged, not the whole screen.
    #[test]
    fn pinned_viewport_absorbs_history_scroll() {
        use crate::performer::handler::Handler;
        let mut cw = new_term(10, 6);
        for _ in 0..20 {
            cw.linefeed();
        }
        cw.scroll_display(crate::crosswords::grid::Scroll::Delta(5));
        assert_eq!(cw.display_offset(), 5);
        cw.reset_damage();

        // Region 0..5 with a fixed bottom line: rows rotate into
        // history and offset grows in lockstep while under the cap.
        cw.set_scrolling_region(1, Some(5));
        cw.grid.cursor.pos = Pos::new(Line(4), Column(0));
        cw.linefeed();
        assert_eq!(cw.display_offset(), 6, "pin did not absorb the scroll");
        assert!(cw.display_offset() <= cw.history_size());
        assert!(
            !matches!(cw.peek_damage_event(), Some(TerminalDamage::Full)),
            "absorbed scroll needs no full repaint"
        );
    }

    /// At the history cap the pinned offset clamps and the whole
    /// scrolled-back view slides one row: every visible row changes
    /// while region damage only covers active-area lines. That frame
    /// must be full damage or the view keeps stale rows.
    #[test]
    fn pinned_viewport_slide_at_cap_is_full_damage() {
        use crate::performer::handler::Handler;
        let mut cw = scrolled_back_term();
        let cap = cw.history_size();
        assert_eq!(cw.display_offset(), cap, "expected offset pinned at cap");

        cw.set_scrolling_region(1, Some(5));
        cw.grid.cursor.pos = Pos::new(Line(4), Column(0));
        cw.linefeed();
        assert_eq!(cw.history_size(), cap, "history can no longer grow");
        assert_eq!(cw.display_offset(), cap, "offset must clamp at the cap");
        assert!(
            matches!(cw.peek_damage_event(), Some(TerminalDamage::Full)),
            "slid view must be fully damaged"
        );
    }

    /// Growing the viewport pulls rows from history; the offset must
    /// track the rows actually pulled and never exceed what remains.
    #[test]
    fn resize_clamps_pinned_display_offset() {
        let mut cw = scrolled_back_term();
        cw.resize(CrosswordsSize::new(10, 9));
        assert!(
            cw.display_offset() <= cw.history_size(),
            "grow_lines left offset {} past history {}",
            cw.display_offset(),
            cw.history_size()
        );
        cw.resize(CrosswordsSize::new(10, 4));
        assert!(cw.display_offset() <= cw.history_size());
        cw.resize(CrosswordsSize::new(7, 8));
        assert!(cw.display_offset() <= cw.history_size());
    }

    /// Consuming a frame records the cursor position it carried:
    /// without that, every subsequent peek reports CursorOnly and
    /// each PTY drain fires a redraw event forever.
    #[test]
    fn reset_damage_quiesces_cursor_events() {
        use crate::performer::handler::Handler;
        let mut cw = new_term(10, 4);
        cw.reset_damage();
        assert!(cw.peek_damage_event().is_none());

        cw.input('x');
        assert!(cw.peek_damage_event().is_some());
        cw.reset_damage();
        assert!(
            cw.peek_damage_event().is_none(),
            "consumed frame keeps re-reporting the cursor"
        );
    }

    /// Mode 2027: a zero-width codepoint with no base to join (orphan
    /// at column 0) is dropped, never legacy-attached: the mode just
    /// computed a boundary and wcwidth-attaching would contradict it.
    #[test]
    fn mode_2027_orphan_zero_width_dropped() {
        use crate::ansi::mode::PrivateMode;
        use crate::performer::handler::Handler;
        let mut cw = new_term(6, 3);
        cw.set_private_mode(PrivateMode::new(2027));
        cw.input('\u{0301}');
        let cell = cw.grid[Line(0)][Column(0)];
        assert_eq!(cell.c(), '\0');
        assert!(cell.extras_id().is_none());
        assert_eq!(cw.grid.cursor.pos.col, Column(0));
    }

    /// Mode 2027: variation-selector validity is judged against the
    /// cluster's *last* codepoint, not its base: a selector modifies
    /// the character immediately before it. U+261D is a valid VS16
    /// base but the
    /// skin tone that joined after it is not, so the trailing VS16 is
    /// ignored outright.
    #[test]
    fn mode_2027_vs_validated_against_last_codepoint() {
        use crate::ansi::mode::PrivateMode;
        use crate::performer::handler::Handler;
        let mut cw = new_term(8, 3);
        cw.set_private_mode(PrivateMode::new(2027));
        cw.input('\u{261D}'); // ☝ narrow, valid VS16 base
        cw.input('\u{1F3FB}'); // skin tone: Extend, width 2 → cluster goes wide
        let col_after_modifier = cw.grid.cursor.pos.col;
        cw.input('\u{FE0F}'); // last cp is the skin tone → invalid → ignored

        let cell = cw.grid[Line(0)][Column(0)];
        assert_eq!(cell.c(), '\u{261D}');
        assert_eq!(cell.wide(), Wide::Wide);
        let extras = cw.grid.extras_table.get(cell.extras_id().unwrap()).unwrap();
        assert_eq!(extras.zerowidth, vec!['\u{1F3FB}']);
        assert_eq!(cw.grid.cursor.pos.col, col_after_modifier);
    }

    /// DEC private mode 2027 (grapheme cluster processing): set,
    /// reset, DECRQM visibility, RIS, and cross-screen sharing. The
    /// mode is plumbing-complete here; segmentation behavior arrives
    /// with the cluster input path.
    #[test]
    fn mode_2027_plumbing() {
        use crate::ansi::mode::{NamedPrivateMode, PrivateMode};
        use crate::performer::handler::Handler;
        let mut cw = new_term(6, 3);

        // Default: set (the consumer knob below configures it).
        assert!(cw.mode().contains(Mode::GRAPHEME_CLUSTER));

        // DECRST / DECSET round-trip: the program wins at runtime.
        cw.unset_private_mode(PrivateMode::new(2027));
        assert!(!cw.mode().contains(Mode::GRAPHEME_CLUSTER));
        cw.set_private_mode(PrivateMode::new(2027));
        assert!(cw.mode().contains(Mode::GRAPHEME_CLUSTER));

        // 2027 resolves to the named mode, not Unknown.
        assert!(matches!(
            PrivateMode::new(2027),
            PrivateMode::Named(NamedPrivateMode::GraphemeCluster)
        ));

        // Shared across screens: the mode lives on the terminal.
        cw.set_private_mode(PrivateMode::new(2027));
        cw.swap_alt();
        assert!(cw.mode().contains(Mode::GRAPHEME_CLUSTER));
        cw.swap_alt();

        // RIS restores the configured default: on out of the box,
        // whatever a program had DECRST'd...
        cw.unset_private_mode(PrivateMode::new(2027));
        cw.reset_state();
        assert!(cw.mode().contains(Mode::GRAPHEME_CLUSTER));

        // ...and off for a consumer that opted out, surviving both a
        // program's DECSET and the RIS after it.
        cw.set_grapheme_clustering(false);
        assert!(!cw.mode().contains(Mode::GRAPHEME_CLUSTER));
        cw.set_private_mode(PrivateMode::new(2027));
        assert!(cw.mode().contains(Mode::GRAPHEME_CLUSTER));
        cw.reset_state();
        assert!(!cw.mode().contains(Mode::GRAPHEME_CLUSTER));
    }

    /// Identical extras content interns into one slot: the u16 id
    /// space stops being a per-occurrence budget.
    #[test]
    fn extras_intern_by_content() {
        use crate::performer::handler::Handler;
        let mut cw = new_term(10, 2);
        cw.input('e');
        cw.input('\u{301}');
        cw.input('e');
        cw.input('\u{301}');
        let a = cw.grid[Line(0)][Column(0)].extras_id().unwrap();
        let b = cw.grid[Line(0)][Column(1)].extras_id().unwrap();
        assert_eq!(a, b);
        // Different content gets its own slot.
        cw.input('e');
        cw.input('\u{302}');
        let c = cw.grid[Line(0)][Column(2)].extras_id().unwrap();
        assert_ne!(a, c);
    }

    /// A combining mark typed inside an OSC 8 hyperlink must attach to
    /// its own cell only. The old in-place mutation edited the shared
    /// template slot, leaking the mark into every cell of the span —
    /// and the marked cell must keep the hyperlink.
    #[test]
    fn combining_mark_inside_hyperlink_stays_on_one_cell() {
        use crate::performer::handler::Handler;
        let mut cw = new_term(10, 2);
        let link = Hyperlink::new(None::<&str>, "https://example.com");
        cw.set_hyperlink(Some(link.clone()));
        cw.input('a');
        cw.input('\u{301}');
        cw.input('b');
        cw.set_hyperlink(None);

        // The marked cell carries mark + hyperlink.
        let a_id = cw.grid[Line(0)][Column(0)].extras_id().unwrap();
        let a = cw.grid.extras_table.get(a_id).unwrap();
        assert_eq!(a.zerowidth, vec!['\u{301}']);
        assert_eq!(a.hyperlink.as_ref(), Some(&link));

        // Its neighbor keeps the pristine hyperlink-only slot.
        let b_id = cw.grid[Line(0)][Column(1)].extras_id().unwrap();
        let b = cw.grid.extras_table.get(b_id).unwrap();
        assert!(b.zerowidth.is_empty());
        assert_eq!(b.hyperlink.as_ref(), Some(&link));
        assert_ne!(a_id, b_id);
    }

    /// The interning lookup must be purged when slots are swept, or a
    /// re-alloc of old content would return a dead slot id.
    #[test]
    fn reclaim_purges_interning_lookup() {
        use crate::performer::handler::Handler;
        let mut cw = new_term(10, 2);
        cw.input('e');
        cw.input('\u{301}');
        let content = cw
            .grid
            .extras_table
            .get(cw.grid[Line(0)][Column(0)].extras_id().unwrap())
            .unwrap()
            .clone();
        // Drop the only reference and sweep.
        cw.clear_screen(ClearMode::All);
        cw.grid.reclaim_extras();
        // Re-alloc of the same content must yield a live slot.
        let id = cw.grid.alloc_extras(content.clone());
        assert_ne!(id, 0);
        assert_eq!(cw.grid.extras_table.get(id), Some(&content));
    }

    /// A width-bearing continuation joining a cluster whose base sits
    /// at the last column widens through the row edge: the base moves
    /// to the next row as a wide pair and its earlier extras move with
    /// it.
    #[test]
    fn cluster_widen_at_last_column_preserves_base_extras() {
        use crate::performer::handler::Handler;
        let mut cw = term_2027(3, 3);
        cw.input('a');
        cw.input('a');
        cw.input('\u{261D}'); // narrow index-up at the last column
        cw.input('\u{0301}'); // combining mark joins the cluster
        assert_eq!(extras_of(&cw, 0, 2), ['\u{0301}']);

        cw.input('\u{1F3FB}'); // skin tone: width-bearing joiner, widens

        assert_eq!(cw.grid[Line(0)][Column(2)].wide(), Wide::LeadingSpacer);
        assert_eq!(cw.grid[Line(1)][Column(0)].c(), '\u{261D}');
        assert_eq!(cw.grid[Line(1)][Column(0)].wide(), Wide::Wide);
        assert_eq!(
            extras_of(&cw, 1, 0),
            ['\u{0301}', '\u{1F3FB}'],
            "extras must ride the wrap with their base"
        );
        assert_eq!(cw.grid[Line(1)][Column(1)].wide(), Wide::Spacer);
    }

    /// The measurement API must agree with what printing lays out:
    /// for a cluster printed into a fresh cell under mode 2027,
    /// `cluster_width` predicts both the codepoint count the cell
    /// absorbs (base plus extras) and its final cell width. Embedders
    /// rely on this to size cells without replaying the input path.
    #[test]
    fn cluster_width_matches_printed_layout() {
        use crate::grapheme_lut::cluster_width;
        use crate::performer::handler::Handler;
        let corpus: &[&[u32]] = &[
            &[0x61],                     // plain narrow
            &[0x4E00],                   // plain wide
            &[0x65, 0x301],              // combining mark
            &[0x2764, 0xFE0F],           // VS16 widens
            &[0x231A, 0xFE0E],           // VS15 narrows
            &[0x31, 0xFE0F, 0x20E3],     // keycap
            &[0x1F468, 0x200D, 0x1F33E], // ZWJ farmer
            &[0x1F44D, 0x1F3FB],         // skin tone
            &[0x1F1E7, 0x1F1F7],         // flag pair
            &[0x1F39F, 0xFE0F],          // text-default emoji + VS16
        ];
        for cps in corpus {
            let mut cw = term_2027(10, 3);
            for &cp in *cps {
                cw.input(char::from_u32(cp).unwrap());
            }
            let (len, width) = cluster_width(cps);
            assert_eq!(len, cps.len(), "one whole cluster: {cps:04X?}");
            assert_eq!(
                1 + extras_of(&cw, 0, 0).len(),
                len,
                "printed codepoint count: {cps:04X?}"
            );
            let printed = match cw.grid[Line(0)][Column(0)].wide() {
                Wide::Wide => 2,
                _ => 1,
            };
            assert_eq!(width, printed, "printed cell width: {cps:04X?}");
        }
    }

    /// Randomized version of the above: for arbitrary sequences over
    /// an alphabet of joiners, selectors, modifiers and bases, print
    /// exactly the codepoints `cluster_width` claims form the first
    /// cluster and demand the grid agrees on both the cell width and
    /// the cursor advance. Catches width-rule divergence the curated
    /// corpus misses (selectors after joiners, stacked modifiers).
    #[test]
    fn cluster_width_matches_printed_layout_randomized() {
        use crate::grapheme_lut::cluster_width;
        use crate::performer::handler::Handler;
        let alphabet: &[u32] = &[
            0x61, 0x31, 0x4E00, 0x301, 0x200D, 0xFE0F, 0xFE0E, 0x20E3, 0x1F468, 0x1F469,
            0x1F33E, 0x1F3FB, 0x1F1E7, 0x1F1F7, 0x2764, 0x231A, 0x1F39F, 0x2620, 0x1F3F4,
        ];
        let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = |bound: usize| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((seed >> 33) as usize) % bound
        };
        for _ in 0..2000 {
            let cps: Vec<u32> = (0..1 + next(7))
                .map(|_| alphabet[next(alphabet.len())])
                .collect();
            // A cluster only owns a cell when its base takes one.
            if crate::codepoint_width::codepoint_width(cps[0]).unwrap_or(0) == 0 {
                continue;
            }
            let (len, width) = cluster_width(&cps);
            let mut cw = term_2027(20, 4);
            for &cp in &cps[..len] {
                cw.input(char::from_u32(cp).unwrap());
            }
            let printed = match cw.grid[Line(0)][Column(0)].wide() {
                Wide::Wide => 2,
                _ => 1,
            };
            assert_eq!(width, printed, "cell width for {cps:04X?}");
            assert_eq!(
                cw.grid.cursor.pos.col.0, width as usize,
                "cursor advance for {cps:04X?}"
            );
        }
    }

    /// Legacy widths: a selector after a non-emoji base is presentation
    /// noise and is dropped rather than attached, so copy/serialize and
    /// the extras table never carry it.
    #[test]
    fn legacy_variation_selector_dropped_off_emoji_base() {
        use crate::performer::handler::Handler;
        let mut cw = new_term(10, 3);
        cw.set_grapheme_clustering(false);

        cw.input('a');
        cw.input('\u{FE0F}');
        assert!(cw.grid[Line(0)][Column(0)].extras_id().is_none());
        assert_eq!(cw.grid[Line(0)][Column(0)].wide(), Wide::Narrow);

        // Ordinary combining marks still attach to anything.
        cw.input('\u{0301}');
        assert_eq!(extras_of(&cw, 0, 0), ['\u{0301}']);
    }

    /// Legacy widths (clustering opted out): variation selectors attach
    /// as combining data and never change the base cell's width, pure
    /// wcwidth semantics. A width the application cannot predict
    /// desyncs every wcwidth-based redraw (tmux most of all).
    #[test]
    fn legacy_variation_selectors_never_change_width() {
        use crate::performer::handler::Handler;
        let mut cw = new_term(10, 3);
        cw.set_grapheme_clustering(false);

        // Text-default emoji + VS16: stays narrow, selector attached.
        cw.input('\u{1F39F}');
        cw.input('\u{FE0F}');
        assert_eq!(cw.grid[Line(0)][Column(0)].wide(), Wide::Narrow);
        assert_eq!(extras_of(&cw, 0, 0), ['\u{FE0F}']);
        assert_eq!(cw.grid.cursor.pos.col, Column(1));

        // Emoji-default wide char + VS15: stays wide, selector attached.
        cw.input('\u{231A}');
        cw.input('\u{FE0E}');
        assert_eq!(cw.grid[Line(0)][Column(1)].wide(), Wide::Wide);
        assert_eq!(extras_of(&cw, 0, 1), ['\u{FE0E}']);
        assert_eq!(cw.grid.cursor.pos.col, Column(3));
    }

    #[test]
    fn vs16_then_following_char_does_not_overlap() {
        use crate::performer::handler::Handler;
        // Reproduces the original vim-split-misalignment scenario: after
        // widening the text-presentation emoji, the next character must
        // land *past* the spacer, not on top of it.
        let mut cw = new_term(10, 3);
        cw.input('"');
        cw.input('\u{1F39F}');
        cw.input('\u{FE0F}');
        cw.input('"');

        let row = Line(0);
        assert_eq!(cw.grid[row][Column(0)].c(), '"');
        assert_eq!(cw.grid[row][Column(1)].c(), '\u{1F39F}');
        assert_eq!(cw.grid[row][Column(1)].wide(), Wide::Wide);
        assert_eq!(cw.grid[row][Column(2)].wide(), Wide::Spacer);
        assert_eq!(cw.grid[row][Column(3)].c(), '"');
        assert_eq!(cw.grid.cursor.pos.col, Column(4));
    }

    /// End-to-end: feed rio the exact byte sequence `kitten icat
    /// --unicode-placeholder` emits and verify the grid ends up with the
    /// expected U+10EEEE cells, fg colors, and diacritics. Reproduces
    /// the wire protocol from kitty's `kittens/icat/transmit.go:221`
    /// (`write_unicode_placeholder`) — `\e[38:2:R:G:Bm` foreground +
    /// per-cell `<U+10EEEE><row diac><col diac><high diac>` payload.
    #[test]
    fn icat_unicode_placeholder_wire_sequence_lands_in_grid() {
        use crate::ansi::kitty_virtual::{DIACRITICS, PLACEHOLDER};

        let size = CrosswordsSize::new(40, 20);
        let columns = size.columns;
        let window_id = crate::event::WindowId::from(0);
        let mut cw = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );
        let mut processor = crate::performer::handler::Processor::default();

        // 32-bit image_id with high byte non-zero (matches icat's
        // rejection-sample loop in `transmit.go:280-288`).
        let image_id: u32 = 0x0201_0203;
        let high = (image_id >> 24) & 0xFF;
        let r = (image_id >> 16) & 0xFF;
        let g = (image_id >> 8) & 0xFF;
        let b = image_id & 0xFF;

        // 1) Transmit a 1×1 RGBA pixel under the chosen image_id (we
        // don't care about the pixel data — we just need an entry in
        // `kitty_images` so the renderer's existence check passes).
        // base64("\xFF\x00\x00\xFF") = "/wAA/w==".
        let xmit = format!("\x1b_Gf=32,a=t,i={image_id},s=1,v=1;/wAA/w==\x1b\\");
        processor.advance(&mut cw, xmit.as_bytes());

        // 2) Register the virtual placement: 4 cols × 2 rows.
        let cols = 4u32;
        let rows = 2u32;
        let place = format!("\x1b_Ga=p,U=1,i={image_id},c={cols},r={rows},q=2\x1b\\");
        processor.advance(&mut cw, place.as_bytes());

        // 3) Emit the placeholder cells themselves (what icat writes
        // after the placement APC). Set fg via colon-separated SGR,
        // write `<U+10EEEE><row><col><high>` per cell.
        let id_high_diac = DIACRITICS[high as usize];
        let mut cells = format!("\x1b[38:2:{r}:{g}:{b}m");
        for (row, &row_diac) in DIACRITICS.iter().enumerate().take(rows as usize) {
            for &col_diac in DIACRITICS.iter().take(cols as usize) {
                cells.push(PLACEHOLDER);
                cells.push(row_diac);
                cells.push(col_diac);
                cells.push(id_high_diac);
            }
            if row < rows as usize - 1 {
                cells.push_str("\n\r");
            }
        }
        cells.push_str("\x1b[39m");
        processor.advance(&mut cw, cells.as_bytes());

        // Metadata stored.
        let vp = cw
            .graphics
            .kitty_virtual_placements
            .get(&(image_id, 0))
            .expect("virtual placement registered");
        assert_eq!(vp.columns, cols);
        assert_eq!(vp.rows, rows);

        // Image transmitted.
        assert!(
            cw.graphics.get_kitty_image(image_id).is_some(),
            "image was not stored under id {image_id:#X}"
        );

        // Each cell of the placement carries U+10EEEE + the right fg
        // RGB + the right two diacritics (zerowidth). The third
        // diacritic encodes image_id_high.
        let extras = cw.grid.extras_table.clone();
        let _ = columns;
        for (row, &row_diac) in DIACRITICS.iter().enumerate().take(rows as usize) {
            for (col, &col_diac) in DIACRITICS.iter().enumerate().take(cols as usize) {
                let sq = cw.grid[Line(row as i32)][Column(col)];
                assert_eq!(
                    sq.c(),
                    PLACEHOLDER,
                    "expected U+10EEEE at ({row},{col}), got {:#X}",
                    sq.c() as u32
                );
                let style = cw.grid.style_of(&sq);
                match style.fg {
                    crate::config::colors::AnsiColor::Spec(rgb) => {
                        assert_eq!(rgb.r as u32, r);
                        assert_eq!(rgb.g as u32, g);
                        assert_eq!(rgb.b as u32, b);
                    }
                    other => panic!("expected RGB fg at ({row},{col}), got {other:?}"),
                }
                let zw = sq
                    .extras_id()
                    .and_then(|id| extras.get(id))
                    .map(|e| e.zerowidth.as_slice())
                    .unwrap_or(&[]);
                assert_eq!(
                    zw.len(),
                    3,
                    "expected 3 diacritics at ({row},{col}), got {}",
                    zw.len()
                );
                assert_eq!(zw[0], row_diac, "row diacritic mismatch at ({row},{col})");
                assert_eq!(zw[1], col_diac, "col diacritic mismatch at ({row},{col})");
                assert_eq!(
                    zw[2], id_high_diac,
                    "high diacritic mismatch at ({row},{col})"
                );
            }
        }

        // Per-row dirty flag: rows that received placeholder cells must
        // have it set; other rows must not.
        for row in 0..(rows as i32) {
            assert!(
                cw.grid[Line(row)].kitty_virtual_placeholder,
                "row {row} should have kitty_virtual_placeholder = true",
            );
        }
        // A row past the placement (no placeholder cells written there).
        let past = rows as i32;
        if past < cw.grid.screen_lines() as i32 {
            assert!(
                !cw.grid[Line(past)].kitty_virtual_placeholder,
                "row {past} (no placeholders) must not have the flag set",
            );
        }
    }

    /// End-to-end: yazi's kgp driver (yazi-adapter/src/drivers/kgp.rs)
    /// transmits with `a=T,C=1,U=1` and NO `c=`/`r=` grid params, sets
    /// the fg via semicolon-form RGB, and writes 2 diacritics per
    /// placeholder cell. The placement must register and the implied
    /// grid must come from the image's pixel size, not collapse to a
    /// 1×1 cell box (the 0.4.12 yazi-preview regression from #1314).
    #[test]
    #[allow(clippy::needless_range_loop)]
    fn yazi_kgp_wire_sequence_implies_grid_from_image_size() {
        use crate::ansi::kitty_virtual::{DIACRITICS, PLACEHOLDER};

        let size = CrosswordsSize::new(40, 20);
        let window_id = crate::event::WindowId::from(0);
        let mut cw = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );
        let mut processor = crate::performer::handler::Processor::default();

        // yazi: id = pid % 0xffffff → 24 bits, no high-byte diacritic.
        let image_id: u32 = 0x0A12BF;
        let r = (image_id >> 16) & 0xFF;
        let g = (image_id >> 8) & 0xFF;
        let b = image_id & 0xFF;

        // Combined transmit+display, virtual, no c=/r=. 2×2 RGBA image
        // (base64 of 16 bytes of 0xFF).
        let xmit = format!(
            "\x1b_Gq=2,a=T,C=1,U=1,f=32,s=2,v=2,i={image_id},m=0;/////////////////////w==\x1b\\"
        );
        processor.advance(&mut cw, xmit.as_bytes());

        // Placeholder grid 4 cols × 2 rows, absolute cursor moves per
        // row, semicolon-form RGB fg carrying the image id.
        let cols = 4usize;
        let rows = 2usize;
        let mut cells = format!("\x1b[38;2;{r};{g};{b}m");
        for row in 0..rows {
            cells.push_str(&format!("\x1b[{};1H", row + 1));
            for col in 0..cols {
                cells.push(PLACEHOLDER);
                cells.push(DIACRITICS[row]);
                cells.push(DIACRITICS[col]);
            }
        }
        cells.push_str("\x1b[0m");
        processor.advance(&mut cw, cells.as_bytes());

        let vp = cw
            .graphics
            .kitty_virtual_placements
            .get(&(image_id, 0))
            .expect("a=T,U=1 must register a virtual placement");
        assert_eq!((vp.columns, vp.rows), (0, 0), "c=/r= omitted on the wire");
        assert!(
            cw.graphics.get_kitty_image(image_id).is_some(),
            "image stored under id {image_id:#X}"
        );

        // Placeholder cells landed with both diacritics.
        let extras = cw.grid.extras_table.clone();
        for row in 0..rows {
            let sq = cw.grid[Line(row as i32)][Column(0)];
            assert_eq!(sq.c(), PLACEHOLDER);
            let zw = sq
                .extras_id()
                .and_then(|id| extras.get(id))
                .map(|e| e.zerowidth.as_slice())
                .unwrap_or(&[]);
            assert_eq!(zw, &[DIACRITICS[row], DIACRITICS[0]]);
        }

        // The renderer's geometry for each row's run: with the grid
        // implied from a 2×2 px image on 1×2 px cells (2 cols × 1 row),
        // row 0 is visible and drawn 1:1. Before the fix the placement
        // box collapsed to one cell and row 1 of a taller image drew
        // nothing while row 0 squeezed the whole image.
        let run = crate::ansi::kitty_virtual::PlaceholderRun {
            image_id,
            placement_id: 0,
            row: 0,
            col: 0,
            width: cols as u32,
        };
        let geom = crate::ansi::kitty_virtual::compute_run_geometry(
            &run,
            vp.columns,
            vp.rows,
            2,
            2,
            (vp.x, vp.y, vp.width, vp.height),
            1.0,
            2.0,
            0.0,
            0.0,
            0,
            0,
        )
        .expect("implied-grid run must be drawable");
        assert_eq!(geom.width, 2.0);
        assert_eq!(geom.height, 2.0);
        assert_eq!(geom.source_rect, [0.0, 0.0, 1.0, 1.0]);
    }

    /// `place_virtual_graphic` is the handler for `_Ga=p,U=1,…\e\` — the
    /// virtual-placement registration used by `kitten icat
    /// --unicode-placeholder`. Per the kitty protocol spec the command
    /// only stores metadata; the application emits the U+10EEEE
    /// placeholder cells itself as ordinary text afterwards. This test
    /// pins that contract: previously rio also auto-wrote the cells,
    /// which raced kitty's own writes and broke the rendering.
    /// `a=d,d=i` must remove virtual (U=1) placements, while `d=a`
    /// ("all visible placements") leaves them alone as kitty does.
    #[test]
    fn delete_removes_virtual_placements() {
        use crate::ansi::kitty_graphics_protocol::{DeleteRequest, PlacementRequest};

        let size = CrosswordsSize::new(40, 20);
        let window_id = crate::event::WindowId::from(0);
        let mut cw = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );

        let store = |cw: &mut Crosswords<VoidListener>, id: u32| {
            cw.graphics.store_kitty_image(
                id,
                None,
                GraphicData {
                    id: rio_graphics::GraphicId::new(id as u64),
                    width: 1,
                    height: 1,
                    pixels: vec![0u8; 4],
                    color_type: rio_graphics::ColorType::Rgba,
                    is_opaque: true,
                    display_width: None,
                    display_height: None,
                    resize: None,
                    transmit_time: crate::time::Instant::now(),
                },
            );
        };
        let place = |cw: &mut Crosswords<VoidListener>, id: u32, pid: u32| {
            assert!(cw.place_graphic(PlacementRequest {
                image_id: id,
                placement_id: pid,
                x: 0,
                y: 0,
                width: 0,
                height: 0,
                columns: 2,
                rows: 1,
                z_index: 0,
                virtual_placement: true,
                unicode_placeholder: 0,
                cursor_movement: 0,
                cell_x_offset: 0,
                cell_y_offset: 0,
            }));
        };
        let delete = |action: u8, image_id: u32, placement_id: u32| DeleteRequest {
            action,
            image_id,
            image_number: 0,
            placement_id,
            x: 0,
            y: 0,
            z_index: 0,
            delete_data: false,
        };

        store(&mut cw, 1);
        store(&mut cw, 2);
        place(&mut cw, 1, 5);
        place(&mut cw, 1, 6);
        place(&mut cw, 2, 5);
        assert_eq!(cw.graphics.kitty_virtual_placements.len(), 3);

        // By id and placement: only (1, 5) goes.
        cw.delete_graphics(delete(b'i', 1, 5));
        assert_eq!(cw.graphics.kitty_virtual_placements.len(), 2);
        assert!(!cw.graphics.kitty_virtual_placements.contains_key(&(1, 5)));

        // By id: the rest of image 1 goes, image 2 stays.
        cw.delete_graphics(delete(b'i', 1, 0));
        assert_eq!(cw.graphics.kitty_virtual_placements.len(), 1);
        assert!(cw.graphics.kitty_virtual_placements.contains_key(&(2, 5)));

        // Delete all visible placements: virtual ones are not visible
        // by themselves and stay.
        cw.delete_graphics(delete(b'a', 0, 0));
        assert!(cw.graphics.kitty_virtual_placements.contains_key(&(2, 5)));
    }

    #[test]
    fn place_virtual_graphic_stores_metadata_without_writing_cells() {
        use crate::ansi::kitty_graphics_protocol::PlacementRequest;

        let size = CrosswordsSize::new(40, 20);
        let columns = size.columns;
        let screen_lines = size.screen_lines as i32;
        let window_id = crate::event::WindowId::from(0);
        let mut cw = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            window_id,
            0,
            10_000,
        );

        // Virtual placements reference a transmitted image like any
        // other `a=p`; placing an unknown id is ENOENT.
        cw.graphics.store_kitty_image(
            1234,
            None,
            GraphicData {
                id: rio_graphics::GraphicId::new(1234),
                width: 1,
                height: 1,
                pixels: vec![0u8; 4],
                color_type: rio_graphics::ColorType::Rgba,
                is_opaque: true,
                display_width: None,
                display_height: None,
                resize: None,
                transmit_time: crate::time::Instant::now(),
            },
        );

        let cursor_before = cw.grid.cursor.pos;
        let placement = PlacementRequest {
            image_id: 1234,
            placement_id: 0,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            columns: 8,
            rows: 4,
            z_index: 0,
            virtual_placement: true,
            unicode_placeholder: 0,
            cursor_movement: 0,
            cell_x_offset: 0,
            cell_y_offset: 0,
        };

        assert!(cw.place_graphic(placement), "image exists, must place");

        // Metadata stored …
        let vp = cw
            .graphics
            .kitty_virtual_placements
            .get(&(1234, 0))
            .expect("virtual placement registered");
        assert_eq!(vp.image_id, 1234);
        assert_eq!(vp.columns, 8);
        assert_eq!(vp.rows, 4);

        // … but no placeholder cells written. The application is
        // responsible for emitting U+10EEEE itself; the terminal must not
        // double-write.
        for line in 0..screen_lines {
            for col in 0..columns {
                let cp = cw.grid[Line(line)][Column(col)].c();
                assert_ne!(
                    cp,
                    crate::ansi::kitty_virtual::PLACEHOLDER,
                    "unexpected U+10EEEE cell at ({line},{col})"
                );
            }
        }

        // Cursor must be untouched.
        assert_eq!(cw.grid.cursor.pos, cursor_before);
    }

    fn kitty_test_image(id: u32, width: usize, height: usize) -> GraphicData {
        GraphicData {
            id: rio_graphics::GraphicId::new(id as u64),
            width,
            height,
            pixels: vec![0u8; width * height * 4],
            color_type: rio_graphics::ColorType::Rgba,
            is_opaque: true,
            display_width: None,
            display_height: None,
            resize: None,
            transmit_time: crate::time::Instant::now(),
        }
    }

    fn virtual_request(
        image_id: u32,
        placement_id: u32,
    ) -> crate::ansi::kitty_graphics_protocol::PlacementRequest {
        crate::ansi::kitty_graphics_protocol::PlacementRequest {
            image_id,
            placement_id,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            columns: 8,
            rows: 4,
            z_index: 0,
            virtual_placement: true,
            unicode_placeholder: 0,
            cursor_movement: 0,
            cell_x_offset: 0,
            cell_y_offset: 0,
        }
    }

    fn uploaded(cw: &Crosswords<VoidListener>, image_id: u32) -> bool {
        cw.graphics.is_kitty_uploaded(image_id)
    }

    #[test]
    fn virtual_placement_uploads_pixels_once_per_image() {
        let mut cw = make_crosswords();
        // Unknown image: nothing to upload.
        assert!(!cw.ensure_kitty_upload(1234));

        // `a=t` stores without uploading; the first `a=p,U=1` queues
        // the pixels, a re-place (relayout) must not resend them.
        cw.store_graphic(kitty_test_image(1234, 1, 1));
        assert!(!uploaded(&cw, 1234));
        assert!(cw.place_graphic(virtual_request(1234, 7)));
        assert!(uploaded(&cw, 1234));
        assert!(!cw.ensure_kitty_upload(1234));
        assert!(cw.place_graphic(virtual_request(1234, 7)));
        assert!(cw.place_graphic(virtual_request(1234, 8)));
        assert!(!cw.ensure_kitty_upload(1234));

        // Deleting the placements but keeping the data leaves the
        // texture valid, so placing again does not resend either.
        cw.graphics.kitty_virtual_placements.clear();
        assert!(cw.place_graphic(virtual_request(1234, 7)));
        assert!(!cw.ensure_kitty_upload(1234));

        // A retransmission of a placed id uploads the new pixels once.
        cw.store_graphic(kitty_test_image(1234, 2, 2));
        assert!(uploaded(&cw, 1234));
        assert!(!cw.ensure_kitty_upload(1234));

        // A retransmission of an unplaced id waits for the placement.
        cw.graphics.kitty_virtual_placements.clear();
        cw.store_graphic(kitty_test_image(1234, 3, 3));
        assert!(!uploaded(&cw, 1234));
        assert!(cw.ensure_kitty_upload(1234));
    }

    #[test]
    fn transmit_and_display_virtual_uploads_once() {
        let mut cw = make_crosswords();
        cw.kitty_transmit_and_display(kitty_test_image(5, 2, 2), virtual_request(5, 1));
        assert!(uploaded(&cw, 5));
        assert!(cw.graphics.kitty_virtual_placements.contains_key(&(5, 1)));
        assert!(!cw.ensure_kitty_upload(5));
    }

    #[test]
    fn direct_placement_shares_texture_with_virtual_placement() {
        use crate::ansi::kitty_graphics_protocol::PlacementRequest;
        let mut cw = make_crosswords();
        cw.graphics.cell_width = 10.0;
        cw.graphics.cell_height = 20.0;
        cw.store_graphic(kitty_test_image(3, 10, 20));
        cw.place_graphic(PlacementRequest {
            virtual_placement: false,
            columns: 0,
            rows: 0,
            ..virtual_request(3, 1)
        });
        assert!(uploaded(&cw, 3));
        assert!(!cw.ensure_kitty_upload(3));
        cw.place_graphic(virtual_request(3, 2));
        assert!(uploaded(&cw, 3));
    }

    #[test]
    fn alt_screen_swap_reuploads_images_whose_texture_was_overwritten() {
        let mut cw = make_crosswords();
        cw.store_graphic(kitty_test_image(1, 1, 1));
        cw.place_graphic(virtual_request(1, 1));
        cw.store_graphic(kitty_test_image(2, 1, 1));
        cw.place_graphic(virtual_request(2, 1));
        assert!(uploaded(&cw, 1) && uploaded(&cw, 2));

        // The alt screen transmits its own image 1, overwriting the
        // window-wide texture, and a never-uploaded image 2.
        cw.swap_alt();
        assert!(cw.graphics.get_kitty_image(1).is_none());
        cw.store_graphic(kitty_test_image(1, 2, 2));
        cw.place_graphic(virtual_request(1, 1));
        cw.store_graphic(kitty_test_image(2, 2, 2));
        assert!(uploaded(&cw, 1) && !uploaded(&cw, 2));

        // Back on the main screen: image 1 must be resent, image 2's
        // texture was never touched so it is still current.
        assert!(cw.graphics.swap_kitty_screen_state());
        let queued: Vec<u32> = cw
            .graphics
            .pending_images
            .iter()
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(queued, vec![1]);
        assert_eq!(cw.graphics.pending_images[0].1.width, 1);
        assert!(uploaded(&cw, 1) && uploaded(&cw, 2));
        cw.graphics.pending_images.clear();

        // And the alt screen's image 1 is stale again when it returns;
        // its unplaced image 2 is not resent.
        assert!(cw.graphics.swap_kitty_screen_state());
        let queued: Vec<u32> = cw
            .graphics
            .pending_images
            .iter()
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(queued, vec![1]);
        assert_eq!(cw.graphics.pending_images[0].1.width, 2);
        assert!(!uploaded(&cw, 2));
        cw.graphics.pending_images.clear();
        cw.graphics.kitty_virtual_placements.clear();
        cw.graphics
            .kitty_inactive_screen
            .kitty_virtual_placements
            .clear();

        // Without a placement on the incoming screen nothing is resent;
        // the record still mismatches, so a later placement uploads.
        assert!(!cw.graphics.swap_kitty_screen_state());
        assert!(cw.graphics.pending_images.is_empty());
        assert!(!uploaded(&cw, 1));
        assert!(cw.ensure_kitty_upload(1));
    }

    #[test]
    fn alt_screen_delete_still_invalidates_main_texture() {
        use crate::ansi::kitty_graphics_protocol::DeleteRequest;
        let mut cw = make_crosswords();
        cw.store_graphic(kitty_test_image(1, 1, 1));
        cw.place_graphic(virtual_request(1, 1));
        assert!(uploaded(&cw, 1));

        // The alt screen uploads its own image 1, then deletes it with
        // its data before switching back; the texture still holds the
        // alt pixels.
        cw.swap_alt();
        cw.store_graphic(kitty_test_image(1, 2, 2));
        cw.place_graphic(virtual_request(1, 1));
        cw.delete_graphics(DeleteRequest {
            action: b'I',
            delete_data: true,
            image_id: 1,
            image_number: 0,
            placement_id: 0,
            x: 0,
            y: 0,
            z_index: 0,
        });
        assert!(cw.graphics.get_kitty_image(1).is_none());

        assert!(cw.graphics.swap_kitty_screen_state());
        assert_eq!(cw.graphics.pending_images.len(), 1);
        assert_eq!(cw.graphics.pending_images[0].1.width, 1);
        assert!(uploaded(&cw, 1));
    }

    #[test]
    fn alt_screen_retransmit_without_placement_invalidates_main_texture() {
        let mut cw = make_crosswords();
        cw.store_graphic(kitty_test_image(1, 1, 1));
        cw.place_graphic(virtual_request(1, 1));

        // Alt uploads image 1, drops its placements, then retransmits
        // without placing: the texture holds the first alt pixels.
        cw.swap_alt();
        cw.store_graphic(kitty_test_image(1, 2, 2));
        cw.place_graphic(virtual_request(1, 1));
        cw.graphics.kitty_virtual_placements.clear();
        cw.store_graphic(kitty_test_image(1, 3, 3));
        assert!(!uploaded(&cw, 1));

        assert!(cw.graphics.swap_kitty_screen_state());
        assert_eq!(cw.graphics.pending_images.len(), 1);
        assert_eq!(cw.graphics.pending_images[0].1.width, 1);
        assert!(uploaded(&cw, 1));
    }

    #[test]
    fn cursor_delete_with_data_keeps_virtually_placed_images() {
        use crate::ansi::kitty_graphics_protocol::DeleteRequest;
        let mut cw = make_crosswords();
        cw.store_graphic(kitty_test_image(1, 1, 1));
        cw.place_graphic(virtual_request(1, 1));
        cw.store_graphic(kitty_test_image(2, 1, 1));
        cw.delete_graphics(DeleteRequest {
            action: b'C',
            delete_data: true,
            image_id: 0,
            image_number: 0,
            placement_id: 0,
            x: 0,
            y: 0,
            z_index: 0,
        });
        assert!(cw.graphics.get_kitty_image(1).is_some());
        assert!(cw.graphics.get_kitty_image(2).is_none());
    }

    #[test]
    fn evicting_inactive_same_id_image_keeps_active_texture() {
        let mut cw = make_crosswords();
        cw.graphics.total_limit = 100_000;
        cw.store_graphic(kitty_test_image(1, 100, 100));
        cw.place_graphic(virtual_request(1, 1));
        assert!(uploaded(&cw, 1));

        cw.swap_alt();
        cw.store_graphic(kitty_test_image(1, 100, 100));
        cw.swap_alt();
        assert!(uploaded(&cw, 1));

        // Storing image 2 has to evict; the inactive image 1 goes first.
        // The texture holds the active copy, so it is neither freed nor
        // resent.
        cw.store_graphic(kitty_test_image(2, 100, 100));
        assert!(cw.graphics.kitty_inactive_screen.kitty_images.is_empty());
        assert!(cw.graphics.get_kitty_image(1).is_some());
        assert!(uploaded(&cw, 1));
        assert!(cw
            .graphics
            .texture_operations
            .lock()
            .iter()
            .all(|key| *key != rio_graphics::kitty_image_key(1)));
    }

    #[test]
    fn delete_all_keeps_virtual_placements_and_their_images() {
        use crate::ansi::kitty_graphics_protocol::{DeleteRequest, PlacementRequest};
        let mut cw = make_crosswords();
        cw.graphics.cell_width = 10.0;
        cw.graphics.cell_height = 20.0;
        cw.store_graphic(kitty_test_image(1, 10, 20));
        cw.place_graphic(virtual_request(1, 1));
        cw.store_graphic(kitty_test_image(2, 10, 20));
        cw.place_graphic(PlacementRequest {
            virtual_placement: false,
            columns: 0,
            rows: 0,
            ..virtual_request(2, 1)
        });
        assert_eq!(cw.graphics.kitty_placements.len(), 1);

        cw.delete_graphics(DeleteRequest {
            action: b'A',
            delete_data: true,
            image_id: 0,
            image_number: 0,
            placement_id: 0,
            x: 0,
            y: 0,
            z_index: 0,
        });
        assert!(cw.graphics.kitty_placements.is_empty());
        assert!(cw.graphics.kitty_virtual_placements.contains_key(&(1, 1)));
        assert!(cw.graphics.get_kitty_image(1).is_some());
        assert!(cw.graphics.get_kitty_image(2).is_none());
    }

    #[test]
    fn delete_range_covers_virtual_placements() {
        use crate::ansi::kitty_graphics_protocol::DeleteRequest;
        let mut cw = make_crosswords();
        cw.store_graphic(kitty_test_image(5, 1, 1));
        cw.place_graphic(virtual_request(5, 1));
        cw.store_graphic(kitty_test_image(9, 1, 1));
        cw.place_graphic(virtual_request(9, 1));

        cw.delete_graphics(DeleteRequest {
            action: b'R',
            delete_data: true,
            image_id: 0,
            image_number: 0,
            placement_id: 0,
            x: 4,
            y: 6,
            z_index: 0,
        });
        assert!(!cw.graphics.kitty_virtual_placements.contains_key(&(5, 1)));
        assert!(cw.graphics.get_kitty_image(5).is_none());
        assert!(cw.graphics.kitty_virtual_placements.contains_key(&(9, 1)));
        assert!(cw.graphics.get_kitty_image(9).is_some());
    }

    #[test]
    fn deleting_image_data_sweeps_its_placements() {
        let mut cw = make_crosswords();
        cw.store_graphic(kitty_test_image(1, 1, 1));
        cw.place_graphic(virtual_request(1, 1));
        cw.graphics.delete_kitty_images(|id, _| *id == 1);
        assert!(cw.graphics.kitty_virtual_placements.is_empty());
    }

    #[test]
    fn retransmit_resets_virtual_source_rect_outside_new_image() {
        let mut cw = make_crosswords();
        cw.store_graphic(kitty_test_image(9, 100, 100));
        let mut req = virtual_request(9, 1);
        req.x = 50;
        req.y = 10;
        req.width = 20;
        req.height = 20;
        cw.place_graphic(req);
        let mut req = virtual_request(9, 2);
        req.x = 5;
        req.width = 10;
        cw.place_graphic(req);

        cw.store_graphic(kitty_test_image(9, 20, 20));
        let stale = &cw.graphics.kitty_virtual_placements[&(9, 1)];
        assert_eq!((stale.x, stale.y, stale.width, stale.height), (0, 0, 0, 0));
        let kept = &cw.graphics.kitty_virtual_placements[&(9, 2)];
        assert_eq!((kept.x, kept.width), (5, 10));
    }

    /// DECSTBM bounds where scrolling happens, not what is on screen. Both
    /// shapes matter: `1;N` only shrinks `end`, so a fix that just zeroed
    /// `start` would pass that case while still dropping the bottom rows.
    #[test]
    fn visible_rows_ignore_the_scroll_region() {
        use crate::performer::handler::Processor;
        fn term_with_lines(rows: usize) -> (Crosswords<VoidListener>, Processor) {
            let mut term = Crosswords::new(
                CrosswordsSize::new(20, rows),
                CursorShape::Block,
                VoidListener,
                crate::event::WindowId::from(0),
                0,
                100,
            );
            let mut parser = Processor::default();
            for i in 0..rows {
                parser
                    .advance(&mut term, format!("\x1b[{};1Hline{}", i + 1, i).as_bytes());
            }
            (term, parser)
        }

        let first_row_text = |term: &Crosswords<VoidListener>| -> String {
            let rows = term.visible_rows();
            (0..5).map(|c| rows[0][Column(c)].c()).collect()
        };

        // Region narrowed at both ends: full height, no vertical shift.
        let (mut term, mut parser) = term_with_lines(10);
        parser.advance(&mut term, b"\x1b[2;8r");
        assert_eq!(term.screen_lines(), 10);
        assert_eq!(term.visible_rows().len(), 10);
        assert_eq!(first_row_text(&term), "line0");

        // Region narrowed at the bottom only, as vim does around its status
        // line: the last row must still be reported.
        let (mut term, mut parser) = term_with_lines(10);
        parser.advance(&mut term, b"\x1b[1;9r");
        assert_eq!(term.visible_rows().len(), 10);
        assert_eq!(first_row_text(&term), "line0");

        // Same on the alternate screen.
        let (mut term, mut parser) = term_with_lines(6);
        parser.advance(&mut term, b"\x1b[?1049h\x1b[1;3r");
        assert_eq!(term.visible_rows().len(), 6);

        // Scrolled into history, the viewport still spans the screen height
        // and starts one line up per scrolled line.
        let (mut term, mut parser) = term_with_lines(4);
        for i in 0..4 {
            parser.advance(&mut term, format!("\r\nscroll{i}").as_bytes());
        }
        parser.advance(&mut term, b"\x1b[2;3r");
        term.scroll_display(crate::crosswords::grid::Scroll::Delta(2));
        assert_eq!(term.display_offset(), 2);
        assert_eq!(term.visible_rows().len(), 4);
    }

    /// The flag byte an embedder needs to encode keys is the one the terminal
    /// would report; it must survive set, push and pop.
    #[test]
    fn keyboard_mode_reports_the_active_flags() {
        use crate::performer::handler::Processor;
        let mut term = Crosswords::new(
            CrosswordsSize::new(10, 3),
            CursorShape::Block,
            VoidListener,
            crate::event::WindowId::from(0),
            0,
            10,
        );
        let mut parser = Processor::default();
        assert_eq!(term.keyboard_mode(), KeyboardModes::NO_MODE);

        // CSI = 5 ; 1 u -> disambiguate + report alternate keys.
        parser.advance(&mut term, b"\x1b[=5;1u");
        assert_eq!(
            term.keyboard_mode(),
            KeyboardModes::DISAMBIGUATE_ESC_CODES | KeyboardModes::REPORT_ALTERNATE_KEYS
        );

        // Pushing a new mode shadows it, popping restores it.
        parser.advance(&mut term, b"\x1b[>1u");
        assert_eq!(term.keyboard_mode(), KeyboardModes::DISAMBIGUATE_ESC_CODES);
        parser.advance(&mut term, b"\x1b[<1u");
        assert_eq!(
            term.keyboard_mode(),
            KeyboardModes::DISAMBIGUATE_ESC_CODES | KeyboardModes::REPORT_ALTERNATE_KEYS
        );
    }

    /// `c()` alone drops combining marks, which is the trap `cell_text`
    /// exists to close. Also covers the bg-only guard: those cells reuse the
    /// extras-id bits for colour, so an unguarded lookup reads a channel as
    /// an id.
    #[test]
    fn cell_text_includes_zero_width_marks() {
        use crate::performer::handler::Processor;
        let mut term = Crosswords::new(
            CrosswordsSize::new(10, 2),
            CursorShape::Block,
            VoidListener,
            crate::event::WindowId::from(0),
            0,
            10,
        );
        let mut parser = Processor::default();
        // "e" + U+0301 COMBINING ACUTE, then "X".
        parser.advance(&mut term, "e\u{0301}X".as_bytes());

        let cell = Pos::new(Line(0), Column(0));
        assert_eq!(term.grid[cell].c(), 'e', "base char only");
        assert_eq!(
            term.grid.cell_text(cell).collect::<String>(),
            "e\u{0301}",
            "cell_text keeps the mark"
        );
        assert_eq!(
            term.grid
                .cell_text(Pos::new(Line(0), Column(1)))
                .collect::<String>(),
            "X"
        );

        // A background-only cell has no text and must not read its colour
        // bits as an extras id.
        let bg = Pos::new(Line(1), Column(0));
        term.grid[bg].set_bg_rgb(0xAA, 0xBB, 0xCC);
        assert!(term.grid[bg].is_bg_only());
        assert_eq!(
            term.grid.cell_text(bg).collect::<String>(),
            "\0",
            "bg-only cell is blank, with no marks"
        );
    }

    /// modifyOtherKeys has no Mode bit, so without this an embedder has to
    /// sniff raw PTY bytes outside the parser to find it.
    #[test]
    fn modify_other_keys_tracks_the_level() {
        use crate::performer::handler::Processor;
        let mut term = Crosswords::new(
            CrosswordsSize::new(10, 2),
            CursorShape::Block,
            VoidListener,
            crate::event::WindowId::from(0),
            0,
            10,
        );
        let mut parser = Processor::default();
        assert_eq!(term.modify_other_keys(), None);

        parser.advance(&mut term, b"\x1b[>4;2m");
        assert_eq!(term.modify_other_keys(), Some(2));

        parser.advance(&mut term, b"\x1b[>4;1m");
        assert_eq!(term.modify_other_keys(), Some(1));

        // No level, and an explicit 0, both disable it.
        parser.advance(&mut term, b"\x1b[>4m");
        assert_eq!(term.modify_other_keys(), None);
        parser.advance(&mut term, b"\x1b[>4;2m\x1b[>4;0m");
        assert_eq!(term.modify_other_keys(), None);

        // Another resource must not be mistaken for modifyOtherKeys, and an
        // out-of-range level must not reach an embedder as a level it cannot
        // interpret.
        parser.advance(&mut term, b"\x1b[>2;2m");
        assert_eq!(term.modify_other_keys(), None);
        parser.advance(&mut term, b"\x1b[>4;7m");
        assert_eq!(term.modify_other_keys(), None);

        // RIS clears it, like the other keyboard state.
        parser.advance(&mut term, b"\x1b[>4;2m");
        assert_eq!(term.modify_other_keys(), Some(2));
        parser.advance(&mut term, b"\x1bc");
        assert_eq!(term.modify_other_keys(), None);
    }
    /// A grid one column wide cannot hold a wide pair. Every path that can
    /// place or preserve one has to cope: the per-cell write, the bulk wide
    /// run, the VS16 promotion that widens an already-written cell, and the
    /// reflow that shrinks a grid under existing wide content. Before this
    /// was handled the first two panicked out of bounds and the last looped
    /// forever, growing the new row buffer without bound.
    #[test]
    fn one_column_grid_handles_wide_glyphs() {
        use crate::performer::handler::Processor;

        let feed = |cols: usize, bytes: &[u8]| {
            let mut term = Crosswords::new(
                CrosswordsSize::new(cols, 3),
                CursorShape::Block,
                VoidListener,
                crate::event::WindowId::from(0),
                0,
                100,
            );
            Processor::default().advance(&mut term, bytes);
            term
        };

        // The reported repro: one wide char, one column. The glyph cannot be
        // represented, so it is dropped for a blank narrow cell.
        let term = feed(1, "你".as_bytes());
        let cell = term.grid[Pos::new(Line(0), Column(0))];
        assert!(!cell.is_wide(), "nothing to pair a wide cell with");
        assert!(!cell.is_spacer(), "and no orphaned Spacer either");
        assert_eq!(cell.c(), ' ');

        for cols in [1, 2] {
            feed(cols, "你好世界".as_bytes());
            feed(cols, "🦀".as_bytes());
            feed(cols, "\u{2764}\u{FE0F}".as_bytes());
            feed(cols, "你\u{0301}".as_bytes());
            feed(cols, "你\r\n好\r\n世".as_bytes());
            feed(cols, "\x1b[?1049h你好".as_bytes());
            feed(cols, "\x1b[1;2r你好".as_bytes());
        }

        // Shrinking under existing wide content, then writing, then growing.
        // Reflowing to one column destroys the wide chars, so no row should
        // come out of it still claiming to be wide or a Spacer.
        let mut term = feed(4, "你好".as_bytes());
        term.resize(CrosswordsSize::new(1, 3));
        for row in term.visible_rows() {
            let cell = row[Column(0)];
            assert!(!cell.is_wide() && !cell.is_spacer());
        }
        Processor::default().advance(&mut term, "世".as_bytes());
        term.resize(CrosswordsSize::new(4, 3));
    }

    /// The mouse protocols are one setting with four states, not four
    /// independent flags, so the last mode set wins and clears the rest.
    #[test]
    fn x10_mouse_mode_displaces_the_other_protocols() {
        use crate::performer::handler::Processor;

        let mut term = Crosswords::new(
            CrosswordsSize::new(10, 10),
            CursorShape::Block,
            VoidListener,
            WindowId::from(0),
            0,
            10,
        );
        let mut parser = Processor::default();

        parser.advance(&mut term, b"\x1b[?9h");
        assert!(term.mode().contains(Mode::MOUSE_REPORT_X10));
        assert!(term.mode().intersects(Mode::MOUSE_MODE));

        // 1000 supersedes X10.
        parser.advance(&mut term, b"\x1b[?1000h");
        assert!(!term.mode().contains(Mode::MOUSE_REPORT_X10));
        assert!(term.mode().contains(Mode::MOUSE_REPORT_CLICK));

        // And X10 displaces 1000 in turn.
        parser.advance(&mut term, b"\x1b[?9h");
        assert!(term.mode().contains(Mode::MOUSE_REPORT_X10));
        assert!(!term.mode().contains(Mode::MOUSE_REPORT_CLICK));

        parser.advance(&mut term, b"\x1b[?9l");
        assert!(!term.mode().intersects(Mode::MOUSE_MODE));
    }

    /// Resetting one protocol while another is active turns reporting off
    /// rather than leaving the active one running, because the four are one
    /// setting and a reset cannot name a protocol to fall back to.
    #[test]
    fn resetting_any_mouse_protocol_clears_reporting() {
        use crate::performer::handler::Processor;

        let sequences: [&[u8]; 4] =
            [b"\x1b[?9l", b"\x1b[?1000l", b"\x1b[?1002l", b"\x1b[?1003l"];
        for reset in sequences {
            let sets: [&[u8]; 4] =
                [b"\x1b[?9h", b"\x1b[?1000h", b"\x1b[?1002h", b"\x1b[?1003h"];
            for set in sets {
                let mut term = Crosswords::new(
                    CrosswordsSize::new(10, 10),
                    CursorShape::Block,
                    VoidListener,
                    WindowId::from(0),
                    0,
                    10,
                );
                let mut parser = Processor::default();

                parser.advance(&mut term, set);
                assert!(term.mode().intersects(Mode::MOUSE_MODE));

                parser.advance(&mut term, reset);
                assert!(
                    !term.mode().intersects(Mode::MOUSE_MODE),
                    "{:?} left reporting on after {:?}",
                    String::from_utf8_lossy(reset),
                    String::from_utf8_lossy(set),
                );
            }
        }
    }

    /// DECRQM has to answer for mode 9 now that it is tracked, otherwise
    /// programs cannot tell that their request took effect.
    #[test]
    fn x10_mouse_mode_answers_decrqm() {
        use crate::performer::handler::Processor;
        use std::cell::RefCell;
        use std::rc::Rc;

        #[derive(Clone)]
        struct TestListener {
            events: Rc<RefCell<Vec<RioEvent>>>,
        }

        impl EventListener for TestListener {
            fn send_event(&self, event: RioEvent, _id: WindowId) {
                self.events.borrow_mut().push(event);
            }
        }

        let events = Rc::new(RefCell::new(Vec::new()));
        let mut term = Crosswords::new(
            CrosswordsSize::new(10, 10),
            CursorShape::Block,
            TestListener {
                events: events.clone(),
            },
            WindowId::from(0),
            0,
            10,
        );
        let mut parser = Processor::default();

        let replies = |events: &Rc<RefCell<Vec<RioEvent>>>| -> Vec<String> {
            events
                .borrow()
                .iter()
                .filter_map(|event| match event {
                    RioEvent::PtyWrite(_, text) => Some(text.clone()),
                    _ => None,
                })
                .collect()
        };

        // Reset (2) before it is set, set (1) after.
        parser.advance(&mut term, b"\x1b[?9$p");
        assert_eq!(replies(&events), vec!["\x1b[?9;2$y".to_string()]);

        events.borrow_mut().clear();
        parser.advance(&mut term, b"\x1b[?9h\x1b[?9$p");
        assert_eq!(replies(&events), vec!["\x1b[?9;1$y".to_string()]);
    }

    #[test]
    fn test_color_scheme_notification() {
        use crate::performer::handler::Handler;
        use std::cell::RefCell;
        use std::rc::Rc;

        #[derive(Clone)]
        struct TestListener {
            events: Rc<RefCell<Vec<RioEvent>>>,
        }

        impl EventListener for TestListener {
            fn send_event(&self, event: RioEvent, _id: WindowId) {
                self.events.borrow_mut().push(event);
            }
        }

        let size = CrosswordsSize::new(10, 10);
        let window_id = WindowId::from(0);
        let events = Rc::new(RefCell::new(Vec::new()));
        let listener = TestListener {
            events: events.clone(),
        };
        let mut term =
            Crosswords::new(size, CursorShape::Block, listener, window_id, 0, 10_000);

        let collect_writes = |events: &Rc<RefCell<Vec<RioEvent>>>| {
            let writes: Vec<String> = events
                .borrow()
                .iter()
                .filter_map(|e| match e {
                    RioEvent::PtyWrite(_, text) => Some(text.clone()),
                    _ => None,
                })
                .collect();
            events.borrow_mut().clear();
            writes
        };

        // Not subscribed: no unsolicited notification.
        term.report_color_scheme(true);
        assert!(events.borrow().is_empty(), "no event without DECSET 2031");

        // Subscribe via DECSET 2031, then notify dark, then light.
        term.set_private_mode(NamedPrivateMode::ColorSchemeUpdates.into());
        term.report_color_scheme(true);
        term.report_color_scheme(false);
        assert_eq!(
            collect_writes(&events),
            vec!["\x1b[?997;1n", "\x1b[?997;2n"]
        );

        // DSR query `CSI ? 996 n` replies with the stored scheme,
        // regardless of DECSET 2031.
        term.unset_private_mode(NamedPrivateMode::ColorSchemeUpdates.into());
        term.set_color_scheme(false);
        term.report_color_scheme_query();
        assert_eq!(collect_writes(&events), vec!["\x1b[?997;2n"]);
        term.set_color_scheme(true);
        term.report_color_scheme_query();
        assert_eq!(collect_writes(&events), vec!["\x1b[?997;1n"]);
    }
}
