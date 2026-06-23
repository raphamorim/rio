use rio_backend::ansi::graphics::{KittyPlacement, StoredImage, VirtualPlacement};
use rio_backend::config::colors::term::TermColors;
use rio_backend::config::CursorConfig;
use rio_backend::crosswords::grid::row::Row;
use rio_backend::crosswords::pos::CursorState;
use rio_backend::crosswords::square::Square;
use rio_backend::event::TerminalDamage;
use rio_backend::selection::SelectionRange;
use rustc_hash::FxHashMap;
use std::time::Instant;

#[derive(Clone, Copy, Debug)]
pub enum BackgroundState {
    Set(rio_backend::sugarloaf::Color),
    Reset,
}

#[derive(Clone, Copy, Debug)]
pub enum WindowUpdate {
    Background(BackgroundState),
}

#[derive(Default, Clone, Debug)]
pub struct Cursor {
    pub state: CursorState,
    pub content: char,
    pub content_ref: char,
    pub is_ime_enabled: bool,
}

/// Hint label information for rendering
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct HintLabel {
    pub position: rio_backend::crosswords::pos::Pos,
    pub label: Vec<char>,
    pub is_first: bool,
}

#[derive(Default)]
pub struct RenderableContent {
    // TODO: Should not use default
    pub cursor: Cursor,
    pub has_blinking_enabled: bool,
    pub is_blinking_cursor_visible: bool,
    pub selection_range: Option<SelectionRange>,
    pub hyperlink_range: Option<SelectionRange>,
    pub hint_labels: Vec<HintLabel>,
    pub highlighted_hint: Option<crate::hints::HintMatch>,
    pub hint_matches: Option<Vec<rio_backend::crosswords::search::Match>>,
    pub last_typing: Option<Instant>,
    pub last_blink_toggle: Option<Instant>,
    pub pending_update: PendingUpdate,
    pub background: Option<BackgroundState>,
    /// Damage hint for the in-progress frame. Set by `Renderer::run`
    /// from PTY + UI damage merging, consumed by `Screen::render`'s
    /// grid emit to choose `RowsToRebuild::{None,Dirty,All}`. The
    /// per-row decision under `Dirty` reads `visible_rows[y].dirty`
    /// rather than this hint, so this is just a coarse gate.
    ///
    /// `Full` on construction so the first frame's emission rebuilds
    /// everything — the grid's CPU+GPU buffers start zeroed and
    /// need a full fill. `mem::replace`'d to `Noop` by `Screen::render`
    /// after consumption so next frame only re-emits if damage
    /// actually arrived.
    pub frame_damage: TerminalDamage,

    /// Per-context viewport row buffer. Populated once per frame by
    /// `Renderer::run` via `Crosswords::snapshot_visible` (which
    /// reuses the existing `Row<Square>` allocations across frames),
    /// then read by `Screen::render`'s grid-emit path and the kitty
    /// virtual-placement overlay path. Single source of truth — only
    /// one terminal lock + one materialize pass per frame per panel.
    pub visible_rows: Vec<Row<Square>>,
    pub style_table: Vec<rio_backend::crosswords::style::Style>,
    /// Per-frame snapshot of extras (zero-width chars, hyperlinks,
    /// sixel/iterm graphics) actually referenced by visible cells —
    /// keyed by the cell's `extras_id`. Refreshed per-dirty-row by
    /// `snapshot_visible`. Bounded by visible-cells-with-extras, not
    /// by total session-lifetime allocations on the live grid's
    /// `ExtrasTable`.
    pub extras: rustc_hash::FxHashMap<u16, rio_backend::crosswords::square::Extras>,
    /// Per-context palette + named-color overrides as of the snapshot.
    /// `Copy` — captured by value alongside the row data.
    pub term_colors: TermColors,
    /// Visible-area scroll offset at the time of the snapshot. Used by
    /// downstream selection-line / hint-line math.
    pub display_offset: usize,
    /// Cached terminal dimensions captured under the same lock as
    /// `visible_rows`. Used for kitty placement positioning.
    pub columns: usize,
    pub screen_lines: usize,
    pub history_size: usize,
    /// `true` when the terminal has cursor blink enabled this frame.
    pub blinking_cursor: bool,
    /// Kitty graphics state captured under the snapshot lock. Owned
    /// here so the kitty overlay path doesn't need to lock again.
    pub kitty_virtual_placements: FxHashMap<(u32, u32), VirtualPlacement>,
    pub kitty_images: FxHashMap<u32, StoredImage>,
    pub kitty_placements: Vec<KittyPlacement>,
    pub kitty_graphics_dirty: bool,
    /// Number of extra rows fetched above the viewport for smooth scroll
    /// partial reveal. 0 when not animating, 1 when a fractional scroll
    /// offset is active.
    pub extra_rows: usize,
}

impl RenderableContent {
    pub fn new(cursor: Cursor) -> Self {
        RenderableContent {
            cursor,
            has_blinking_enabled: false,
            selection_range: None,
            hint_labels: Vec::new(),
            highlighted_hint: None,
            hint_matches: None,
            last_typing: None,
            last_blink_toggle: None,
            hyperlink_range: None,
            pending_update: PendingUpdate::default(),
            is_blinking_cursor_visible: false,
            background: None,
            frame_damage: TerminalDamage::Full,
            visible_rows: Vec::new(),
            style_table: Vec::new(),
            extras: rustc_hash::FxHashMap::default(),
            term_colors: TermColors::default(),
            display_offset: 0,
            columns: 0,
            screen_lines: 0,
            history_size: 0,
            blinking_cursor: false,
            kitty_virtual_placements: FxHashMap::default(),
            kitty_images: FxHashMap::default(),
            kitty_placements: Vec::new(),
            kitty_graphics_dirty: false,
            extra_rows: 0,
        }
    }

    pub fn from_cursor_config(config_cursor: &CursorConfig) -> Self {
        let cursor = Cursor {
            content: config_cursor.shape.into(),
            content_ref: config_cursor.shape.into(),
            state: CursorState::new(config_cursor.shape.into()),
            is_ime_enabled: false,
        };
        Self::new(cursor)
    }
}

#[derive(Debug, Default)]
pub struct PendingUpdate {
    /// Whether there's any pending update that needs rendering
    dirty: bool,
    /// Terminal content damage (lines, text)
    terminal_damage: Option<TerminalDamage>,
}

impl PendingUpdate {
    /// Check if there's a pending update
    #[inline]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark as needing to check for damage on next render. Use this
    /// when UI overlays (command palette, assistant, search bar,
    /// island) change but terminal cells haven't — the `dirty` flag
    /// alone is enough to pass `Renderer::run`'s per-context gate,
    /// and `(None, None) => TerminalDamage::Noop` in the inner damage
    /// match keeps the panel in the render set with zero row work.
    pub fn set_dirty(&mut self) {
        self.dirty = true;
    }

    /// Mark terminal content as damaged
    pub fn set_terminal_damage(&mut self, damage: TerminalDamage) {
        self.dirty = true;
        self.terminal_damage = Some(match self.terminal_damage.take() {
            None => damage,
            Some(existing) => Self::merge_terminal_damages(existing, damage),
        });
    }

    /// Get and clear terminal damage
    pub fn take_terminal_damage(&mut self) -> Option<TerminalDamage> {
        self.terminal_damage.take()
    }

    /// Reset the dirty flag after rendering
    pub fn reset(&mut self) {
        self.dirty = false;
        // Note: terminal damage is cleared by take_terminal_damage during render
    }

    /// Merge two terminal damage hints into one. Strict ordering by
    /// "amount of work needed": Full > Partial > CursorOnly > Noop.
    pub fn merge_terminal_damages(
        existing: TerminalDamage,
        new: TerminalDamage,
    ) -> TerminalDamage {
        use TerminalDamage::*;
        match (existing, new) {
            (Full, _) | (_, Full) => Full,
            (Partial, _) | (_, Partial) => Partial,
            (CursorOnly, _) | (_, CursorOnly) => CursorOnly,
            (Noop, Noop) => Noop,
        }
    }
}
