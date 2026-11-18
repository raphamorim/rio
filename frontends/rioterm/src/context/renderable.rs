use rio_backend::ansi::graphics::{
    AtlasPlacement, KittyPlacement, StoredImage, VirtualPlacement,
};
use rio_backend::config::colors::term::TermColors;
use rio_backend::config::CursorConfig;
use rio_backend::crosswords::grid::row::Row;
use rio_backend::crosswords::pos::{CursorState, Pos};
use rio_backend::crosswords::square::Square;
use rio_backend::crosswords::{Crosswords, Mode};
use rio_backend::event::{EventListener, TerminalDamage};
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

/// `content` is the configured cursor shape as a char, read once to
/// seed the terminal's cursor shape. The glyph actually drawn each
/// frame comes from `state`; the old `content`/`content_ref` pair
/// (drawn vs configured) collapsed when the IME preview stopped
/// swapping the drawn char.
#[derive(Default, Clone, Debug)]
pub struct Cursor {
    pub state: CursorState,
    pub content: char,
}

#[derive(Clone, Copy, Debug)]
pub struct HintLabel {
    pub position: rio_backend::crosswords::pos::Pos,
    pub label: char,
    pub is_first: bool,
}

#[derive(Default)]
pub struct RenderableContent {
    // TODO: Should not use default
    pub cursor: Cursor,
    pub has_blinking_enabled: bool,
    pub is_blinking_cursor_visible: bool,
    pub selection_range: Option<SelectionRange>,
    pub hint_labels: Option<Vec<HintLabel>>,
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
    /// Per-row resolved cell styles, index-parallel to `visible_rows`.
    /// Values, not ids: rows copied on earlier frames can't be
    /// retinted by later style-table mutations.
    pub row_styles: Vec<Vec<rio_backend::crosswords::style::Style>>,
    /// Style runs the snapshot recorded under the terminal lock, filled
    /// into `row_styles` by `finish_update` once the lock is released.
    pub pending_styles: rio_backend::crosswords::style::PendingStyles,
    /// Per viewport row, the cells drawn by a ligature glyph, as the
    /// row builder recorded them the last time the row was built. A
    /// cursor move rebuilds a row only when the cursor was, or is, on
    /// one of these cells; everywhere else the row's glyphs do not
    /// depend on the cursor.
    pub row_ligatures: Vec<rio_grid::RowLigatures>,
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
    /// Lines ever evicted off the scrollback ring; base of the
    /// absolute row space image placements anchor in.
    pub lines_evicted: u64,
    /// Sixel/iTerm2 placements (snapshot; DEC grid-plane semantics).
    pub atlas_placements: Vec<AtlasPlacement>,
    /// `true` when the terminal has cursor blink enabled this frame.
    pub blinking_cursor: bool,
    /// `true` when the terminal was in Vi mode at snapshot time, so
    /// the cursor is drawn with the `vi-cursor` color.
    pub is_vi_mode: bool,
    /// Kitty graphics state captured under the snapshot lock. Owned
    /// here so the kitty overlay path doesn't need to lock again.
    pub kitty_virtual_placements: FxHashMap<(u32, u32), VirtualPlacement>,
    pub kitty_images: FxHashMap<u32, StoredImage>,
    pub kitty_placements: Vec<KittyPlacement>,
    pub kitty_graphics_dirty: bool,
}

impl RenderableContent {
    pub fn new(cursor: Cursor) -> Self {
        RenderableContent {
            cursor,
            has_blinking_enabled: false,
            selection_range: None,
            hint_labels: None,
            highlighted_hint: None,
            hint_matches: None,
            last_typing: None,
            last_blink_toggle: None,
            pending_update: PendingUpdate::default(),
            is_blinking_cursor_visible: false,
            background: None,
            frame_damage: TerminalDamage::Full,
            visible_rows: Vec::new(),
            row_ligatures: Vec::new(),
            row_styles: Vec::new(),
            pending_styles: Default::default(),
            extras: rustc_hash::FxHashMap::default(),
            term_colors: TermColors::default(),
            display_offset: 0,
            columns: 0,
            screen_lines: 0,
            history_size: 0,
            lines_evicted: 0,
            atlas_placements: Vec::new(),
            blinking_cursor: false,
            is_vi_mode: false,
            kitty_virtual_placements: FxHashMap::default(),
            kitty_images: FxHashMap::default(),
            kitty_placements: Vec::new(),
            kitty_graphics_dirty: false,
        }
    }

    /// Copy the terminal's visible state into this snapshot, or keep the
    /// last one while a synchronized update holds presentation. Returns
    /// whether a new snapshot was taken.
    pub fn update_from_terminal<U: EventListener>(
        &mut self,
        terminal: &mut Crosswords<U>,
        force_full_damage: bool,
        cursor_shaping_break: bool,
    ) -> bool {
        // Clear in-flight flag so PTY thread can notify again
        terminal.damage_event_in_flight = false;

        if terminal.synchronized_output() {
            if force_full_damage {
                self.pending_update
                    .set_terminal_damage(TerminalDamage::Full);
            }
            self.pending_update.reset();
            self.frame_damage = TerminalDamage::Noop;
            self.kitty_graphics_dirty = false;
            return false;
        }

        // UI-side damage (scroll, selection, resize, etc.)
        let ui_terminal_damage = self.pending_update.take_terminal_damage();
        self.pending_update.reset();

        let pty_damage = terminal.peek_damage_event();

        let damage = if force_full_damage {
            TerminalDamage::Full
        } else {
            match (ui_terminal_damage, pty_damage) {
                (Some(ui), Some(pty)) => PendingUpdate::merge_terminal_damages(ui, pty),
                (Some(d), None) | (None, Some(d)) => d,
                // UI-only damage (overlay hover, command-palette
                // input, etc.): cells didn't change, but the
                // panel still has to go through the render path
                // so UI overlays paint on top of a fresh frame.
                // Noop propagates to `RowsToRebuild::None` in
                // `screen::render`'s emit loop — grid keeps its
                // resident CPU bg/fg buffers, zero row work.
                (None, None) => TerminalDamage::Noop,
            }
        };

        // The row builder shapes the cursor cell as its own run when the
        // cursor sits on a ligature, so the rows the cursor left and
        // entered are rebuilt when their recorded ligature cells say
        // the cursor was, or is, on one. The terminal's own marking is
        // off for this frontend: it cannot know the font.
        if cursor_shaping_break {
            if let Some((from, to)) = terminal.cursor_move() {
                let offset = terminal.display_offset() as i32;
                for pos in cursor_rows_on_ligatures(from, to, offset, &self.row_ligatures)
                {
                    terminal.damage_range_rows(&SelectionRange::new(pos, pos, false));
                }
            }
        }

        terminal.reset_damage();

        terminal.snapshot_visible(
            &damage,
            &mut self.visible_rows,
            &mut self.row_styles,
            &mut self.extras,
            &mut self.pending_styles,
        );
        self.term_colors = terminal.colors;
        self.display_offset = terminal.display_offset();
        self.columns = terminal.columns();
        self.screen_lines = terminal.screen_lines();
        self.history_size = terminal.history_size();
        self.lines_evicted = terminal.lines_evicted();
        self.blinking_cursor = terminal.blinking_cursor;
        self.cursor.state = terminal.cursor();
        self.is_vi_mode = terminal.mode().contains(Mode::VI);
        if terminal.graphics.kitty_graphics_dirty {
            self.kitty_virtual_placements =
                terminal.graphics.kitty_virtual_placements.clone();
            self.kitty_images = terminal.graphics.kitty_images.clone();
            self.kitty_placements = {
                let mut placements: Vec<_> = terminal
                    .graphics
                    .kitty_placements
                    .values()
                    .filter(|p| terminal.graphics.kitty_images.contains_key(&p.image_id))
                    .cloned()
                    .collect();
                // Tie-break on the unique key so equal
                // z-indexes keep a stable paint order across
                // frames (map iteration order is not).
                placements.sort_by_key(|p| (p.z_index, p.image_id, p.placement_id));
                placements
            };
            self.atlas_placements = terminal.graphics.atlas_placements.clone();
            self.kitty_graphics_dirty = true;
            terminal.graphics.kitty_graphics_dirty = false;
        } else {
            self.kitty_graphics_dirty = false;
        }
        self.frame_damage = damage;
        true
    }

    /// The half of the snapshot that needs no terminal: fill the styles
    /// of the rows copied this frame. Called after the terminal lock is
    /// released and before anything reads `row_styles`.
    pub fn finish_update(&mut self) {
        self.pending_styles.fill(&mut self.row_styles);
    }

    pub fn from_cursor_config(config_cursor: &CursorConfig) -> Self {
        let cursor = Cursor {
            content: config_cursor.shape.into(),
            state: CursorState::new(config_cursor.shape.into()),
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

    #[cfg(test)]
    pub fn has_terminal_damage(&self) -> bool {
        self.terminal_damage.is_some()
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

/// Of the positions the cursor left and entered, those where the break
/// around the cursor cell changes the row's glyphs: on a cell the row's
/// last build drew as part of a ligature, or right after one. `offset`
/// is the display offset that turns a grid row into a viewport row; a
/// position off screen or on a row never built is not on a ligature.
pub fn cursor_rows_on_ligatures(
    from: Pos,
    to: Pos,
    offset: i32,
    ligatures: &[rio_grid::RowLigatures],
) -> impl Iterator<Item = Pos> + '_ {
    [from, to].into_iter().filter(move |pos| {
        usize::try_from(pos.row.0 + offset)
            .ok()
            .and_then(|row| ligatures.get(row))
            .is_some_and(|cells| cells.at_or_before(pos.col.0))
    })
}

#[cfg(test)]
mod pipeline_tests {
    //! End-to-end damage pipeline harness: replicates the renderer's
    //! frame consumption (`renderer/mod.rs`) and row-rebuild decisions
    //! (`screen::render`) against a painted text mirror, drives
    //! vim-like scroll workloads through the real parser and grid, and
    //! asserts two invariants at every quiescent point:
    //!
    //! 1. no event starvation: if no TerminalDamaged event would fire,
    //!    no grid row may still be dirty;
    //! 2. what was painted equals what the grid holds.
    //!
    //! Any mutation path that dirties rows without viewport damage, or
    //! any frame classification that consumes damage without painting,
    //! fails here instead of as stale glyphs in someone's editor.
    use super::RenderableContent;
    use rio_backend::ansi::CursorShape;
    use rio_backend::crosswords::grid::row::Row;
    use rio_backend::crosswords::pos::{Column, Line};
    use rio_backend::crosswords::square::Square;
    use rio_backend::crosswords::{Crosswords, CrosswordsSize};
    use rio_backend::event::{TerminalDamage, VoidListener, WindowId};
    use rio_backend::performer::handler::Processor;

    const COLS: usize = 40;
    const ROWS: usize = 12;

    fn new_term() -> Crosswords<VoidListener> {
        Crosswords::new(
            CrosswordsSize::new(COLS, ROWS),
            CursorShape::Block,
            VoidListener,
            WindowId::from(0),
            0,
            100,
        )
    }

    fn row_text(row: &Row<Square>, cols: usize) -> String {
        (0..cols)
            .map(|c| {
                let ch = row[Column(c)].c();
                if ch == '\0' {
                    ' '
                } else {
                    ch
                }
            })
            .collect()
    }

    /// The window the snapshot serves: `display_offset` lines into
    /// history through the active area, like `visible_line_bounds`.
    fn visible_range(term: &Crosswords<VoidListener>) -> std::ops::Range<i32> {
        let start = -(term.display_offset() as i32);
        start..start + term.screen_lines() as i32
    }

    fn grid_text(term: &Crosswords<VoidListener>) -> Vec<String> {
        let cols = term.columns();
        visible_range(term)
            .map(|y| row_text(&term.grid[Line(y)], cols))
            .collect()
    }

    fn any_visible_row_dirty(term: &Crosswords<VoidListener>) -> bool {
        visible_range(term).any(|y| term.grid[Line(y)].dirty)
    }

    /// The renderer's per-panel state: the real snapshot plus the
    /// painted mirror standing in for the GPU grid.
    struct Frame {
        content: RenderableContent,
        painted: Vec<String>,
        /// Rows rebuilt by the last consumed frame.
        rebuilt: usize,
    }

    impl Frame {
        fn new() -> Self {
            Self {
                content: RenderableContent::new(Default::default()),
                painted: vec![String::new(); ROWS],
                rebuilt: 0,
            }
        }

        /// `Renderer::run` consumption followed by a mirror of
        /// `screen::render` row-rebuild decisions, step for step.
        fn consume(
            &mut self,
            term: &mut Crosswords<VoidListener>,
            ui: &mut Option<TerminalDamage>,
        ) {
            let rows = term.screen_lines();
            let cols = term.columns();
            if let Some(ui) = ui.take() {
                self.content.pending_update.set_terminal_damage(ui);
            }
            let snapshot_rows = self.content.visible_rows.len();
            self.rebuilt = 0;
            if !self.content.update_from_terminal(term, false, true) {
                return;
            }
            self.content.finish_update();
            let damage =
                std::mem::replace(&mut self.content.frame_damage, TerminalDamage::Noop);
            let needs_full =
                matches!(damage, TerminalDamage::Full) || snapshot_rows != rows;
            let visible_rows = &mut self.content.visible_rows;
            self.painted.resize(rows, String::new());
            if needs_full {
                // RowsToRebuild::All: rebuild everything and clear the
                // bits, as screen::render does, so the next partial
                // frame rebuilds only what it names.
                for (painted, row) in self.painted.iter_mut().zip(visible_rows) {
                    *painted = row_text(row, cols);
                    row.dirty = false;
                    self.rebuilt += 1;
                }
                return;
            }
            match damage {
                TerminalDamage::Full => unreachable!(),
                TerminalDamage::Partial => {
                    for (painted, row) in self.painted.iter_mut().zip(visible_rows) {
                        if !row.dirty {
                            continue;
                        }
                        *painted = row_text(row, cols);
                        row.dirty = false;
                        self.rebuilt += 1;
                    }
                }
                TerminalDamage::CursorOnly | TerminalDamage::Noop => {}
            }
        }
    }

    // A full frame rebuilds every row and leaves them clean, so the
    // partial frame after it, here a cursor move over plain text,
    // rebuilds nothing instead of the whole screen again.
    #[test]
    fn the_frame_after_a_full_one_rebuilds_only_what_changed() {
        let mut term = new_term();
        term.set_cursor_shaping_break(false);
        let mut frame = Frame::new();
        let mut parser = Processor::default();
        for i in 0..ROWS {
            parser.advance(&mut term, format!("line {i}\r\n").as_bytes());
        }
        frame.consume(&mut term, &mut None);
        assert_eq!(frame.rebuilt, ROWS, "the first frame is full");

        parser.advance(&mut term, b"\x1b[1;1H");
        assert_eq!(term.peek_damage_event(), Some(TerminalDamage::Partial));
        frame.consume(&mut term, &mut None);
        assert_eq!(frame.rebuilt, 0, "a cursor move over plain text");

        parser.advance(&mut term, b"\x1b[3;1Hchanged");
        frame.consume(&mut term, &mut None);
        assert_eq!(frame.rebuilt, 1, "one row of output");
    }

    fn event_would_fire(term: &Crosswords<VoidListener>) -> bool {
        !term.damage_event_in_flight && term.peek_damage_event().is_some()
    }

    /// Drain pending events like the real loop would, then check both
    /// invariants.
    fn assert_quiescent_converged(
        term: &mut Crosswords<VoidListener>,
        frame: &mut Frame,
        context: &str,
    ) {
        let mut spins = 0;
        while matches!(
            term.peek_damage_event(),
            Some(TerminalDamage::Full) | Some(TerminalDamage::Partial)
        ) {
            frame.consume(term, &mut None);
            spins += 1;
            assert!(spins < 8, "{context}: damage never quiesces");
        }
        assert!(
            !event_would_fire(term),
            "{context}: events keep firing with nothing to paint              (event storm: renders at PTY rate forever)"
        );
        assert!(
            !any_visible_row_dirty(term),
            "{context}: visible rows dirty but no damage event would fire \
             (event starvation: this stays stale on screen forever)"
        );
        let grid = grid_text(term);
        assert_eq!(frame.painted.len(), grid.len(), "{context}: row count");
        for (y, row) in grid.iter().enumerate() {
            assert_eq!(
                &frame.painted[y], row,
                "{context}: painted row {y} diverges from grid"
            );
        }
    }

    /// The snapshot records Vi mode so the cursor gets the `vi-cursor`
    /// color.
    #[test]
    fn snapshot_tracks_vi_mode() {
        let mut term = new_term();
        let mut frame = Frame::new();
        frame.consume(&mut term, &mut Some(TerminalDamage::Full));
        assert!(!frame.content.is_vi_mode);

        term.toggle_vi_mode();
        frame.consume(&mut term, &mut Some(TerminalDamage::Full));
        assert!(frame.content.is_vi_mode);

        term.toggle_vi_mode();
        frame.consume(&mut term, &mut Some(TerminalDamage::Full));
        assert!(!frame.content.is_vi_mode);
    }

    /// A held frame paints nothing and keeps its UI damage for the frame
    /// after the synchronized update ends.
    #[test]
    fn synchronized_update_holds_and_keeps_ui_damage() {
        let mut term = new_term();
        let mut parser = Processor::default();
        let mut frame = Frame::new();
        frame.consume(&mut term, &mut Some(TerminalDamage::Full));
        let before = frame.painted.clone();

        parser.advance(&mut term, b"\x1b[?2026h\x1b[1;1Hheld");
        frame.consume(&mut term, &mut Some(TerminalDamage::Full));
        assert_eq!(frame.painted, before);
        assert!(
            frame.content.pending_update.has_terminal_damage(),
            "UI damage stays pending during the hold"
        );

        parser.advance(&mut term, b"\x1b[?2026l");
        frame.consume(&mut term, &mut None);
        assert!(!frame.content.pending_update.has_terminal_damage());
        assert_eq!(frame.painted, grid_text(&term));
    }

    /// Deterministic vim-shaped session: alt screen, scroll region with
    /// a status line, line redraws with EL, scrolls in both directions,
    /// insert/delete lines, with a renderer consuming at every step.
    #[test]
    fn vim_scroll_session_converges() {
        let mut term = new_term();
        let mut parser = Processor::default();
        let mut frame = Frame::new();
        frame.consume(&mut term, &mut Some(TerminalDamage::Full));

        let steps: Vec<Vec<u8>> = vec![
            b"\x1b[?1049h".to_vec(),
            b"\x1b[1;11r".to_vec(),
            (1..=11)
                .flat_map(|y| {
                    format!("\x1b[{y};1Hline {y} conteudo previs\u{00f5}es\x1b[K")
                        .into_bytes()
                })
                .collect(),
            "\x1b[12;1Hstatus \u{00e0} espera\x1b[K".as_bytes().to_vec(),
            b"\x1b[11;1H\n".to_vec(),
            "\x1b[11;1Hnova linha final\x1b[K".as_bytes().to_vec(),
            b"\x1b[2S".to_vec(),
            "\x1b[10;1Hpenultima\x1b[K\x1b[11;1Hultima\x1b[K"
                .as_bytes()
                .to_vec(),
            b"\x1b[T".to_vec(),
            "\x1b[1;1Hprimeira de novo\x1b[K".as_bytes().to_vec(),
            b"\x1b[5;1H\x1b[2L".to_vec(),
            b"\x1b[7;1H\x1b[M".to_vec(),
            "\x1b[5;1Hinserida A\x1b[K\x1b[6;1Hinserida B\x1b[K"
                .as_bytes()
                .to_vec(),
            b"\x1b[r\x1b[?1049l".to_vec(),
        ];
        for (i, chunk) in steps.iter().enumerate() {
            parser.advance(&mut term, chunk);
            assert_quiescent_converged(&mut term, &mut frame, &format!("vim step {i}"));
        }
    }

    /// Randomized interleaving: chunks of vim-like traffic with the
    /// renderer consuming at arbitrary points (including mid-burst and
    /// when no event fired, matching forced redraws), across seeds.
    #[test]
    fn racing_consumption_converges() {
        let chunks: Vec<Vec<u8>> = vec![
            b"\x1b[1;11r".to_vec(),
            b"\x1b[11;1H\n".to_vec(),
            b"\x1b[S".to_vec(),
            b"\x1b[2S".to_vec(),
            b"\x1b[T".to_vec(),
            "\x1b[3;1Hcontent tail que fica \u{00e0} espera\x1b[K"
                .as_bytes()
                .to_vec(),
            "\x1b[9;1Hcurta\x1b[K".as_bytes().to_vec(),
            "\x1b[12;1Hstatus\x1b[K".as_bytes().to_vec(),
            b"\x1b[4;1H\x1b[L".to_vec(),
            b"\x1b[8;1H\x1b[M".to_vec(),
            b"\x1b[2;3H".to_vec(),
            "e\u{301}".as_bytes().to_vec(),
            "\u{301}".as_bytes().to_vec(),
            b"\x1b[6;10Hmeio".to_vec(),
            b"\x1b[2J\x1b[H".to_vec(),
            b"\x1b[?1049h".to_vec(),
            b"\x1b[?1049l".to_vec(),
            b"um\r\ndois\r\ntres\r\nquatro\r\ncinco\r\n".to_vec(),
            "\x1b[11;1H\n\x1b[11;1Hrolou\x1b[K".as_bytes().to_vec(),
        ];

        for seed in 0..200u64 {
            let mut state = 0x9E37_79B9_7F4A_7C15u64.wrapping_add(seed);
            let mut rng = move |n: usize| {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                ((state >> 33) as usize) % n
            };
            let mut term = new_term();
            let mut parser = Processor::default();
            let mut frame = Frame::new();
            frame.consume(&mut term, &mut Some(TerminalDamage::Full));

            let mut journal: Vec<String> = Vec::new();
            for step in 0..400 {
                for _ in 0..(1 + rng(3)) {
                    let ci = rng(chunks.len());
                    journal.push(format!(
                        "s{step} chunk[{ci}] (off={} hist={})",
                        term.display_offset(),
                        term.history_size()
                    ));
                    parser.advance(&mut term, &chunks[ci]);
                    assert!(
                        term.display_offset() <= term.history_size(),
                        "chunk[{ci}] broke offset {} > history {}\n{}",
                        term.display_offset(),
                        term.history_size(),
                        journal
                            .iter()
                            .rev()
                            .take(10)
                            .rev()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join("\n")
                    );
                }
                // Non-parser mutations the real app performs between
                // frames: viewport scrolling, resize, and UI damage
                // hints (blink, selection) merged into the frame.
                let mut ui = None;
                match rng(12) {
                    0 => {
                        use rio_backend::crosswords::grid::Scroll;
                        let d = rng(7) as i32 - 3;
                        journal.push(format!("s{step} scroll_display({d})"));
                        term.scroll_display(Scroll::Delta(d));
                    }
                    1 => {
                        let (c, r) = if rng(2) == 0 { (34, 10) } else { (40, 12) };
                        journal.push(format!("s{step} resize({c}x{r})"));
                        term.resize(CrosswordsSize::new(c, r));
                        ui = Some(TerminalDamage::Full);
                    }
                    2 => {
                        journal.push(format!("s{step} ui=CursorOnly"));
                        ui = Some(TerminalDamage::CursorOnly);
                    }
                    3 => {
                        journal.push(format!("s{step} ui=Full"));
                        ui = Some(TerminalDamage::Full);
                    }
                    _ => {}
                }
                // Renderer races the PTY: sometimes consumes on event,
                // sometimes forced (no event), sometimes not at all.
                match rng(4) {
                    0 => {}
                    1 => {
                        if event_would_fire(&term) {
                            journal.push(format!("s{step} consume(evt)"));
                            frame.consume(&mut term, &mut ui);
                        }
                    }
                    _ => {
                        journal.push(format!("s{step} consume(forced)"));
                        frame.consume(&mut term, &mut ui);
                    }
                }
                if let Some(ui) = ui {
                    // UI damage that missed this frame is carried to
                    // the next consume, like PendingUpdate does.
                    frame.consume(&mut term, &mut Some(ui));
                }
                if term.display_offset() > term.history_size() {
                    let tail: Vec<_> =
                        journal.iter().rev().take(12).rev().cloned().collect();
                    panic!(
                        "offset {} > history {} after:\n{}",
                        term.display_offset(),
                        term.history_size(),
                        tail.join("\n")
                    );
                }
                if step % 16 == 0 {
                    let tail: Vec<_> =
                        journal.iter().rev().take(48).rev().cloned().collect();
                    assert_quiescent_converged(
                        &mut term,
                        &mut frame,
                        &format!("seed {seed} step {step}\n{}", tail.join("\n")),
                    );
                }
            }
            assert_quiescent_converged(
                &mut term,
                &mut frame,
                &format!("seed {seed} end"),
            );
        }
    }
}

#[cfg(test)]
mod cursor_ligature_tests {
    use super::{cursor_rows_on_ligatures, Cursor, RenderableContent};
    use rio_backend::ansi::CursorShape;
    use rio_backend::crosswords::pos::{Column, Line, Pos};
    use rio_backend::crosswords::{Crosswords, CrosswordsSize};
    use rio_backend::event::{VoidListener, WindowId};
    use rio_backend::performer::handler::Handler;

    // Only a cursor that was, or is, on a cell a ligature glyph draws
    // changes a row's glyphs; positions off the ligature cells, off
    // screen, or on rows never built do not.
    #[test]
    fn cursor_rows_on_ligatures_picks_the_positions_on_ligature_cells() {
        let at = |row: i32, col: usize| Pos::new(Line(row), Column(col));
        let mut ligatures = vec![rio_grid::RowLigatures::default(); 3];
        ligatures[1].set(3);
        ligatures[1].set(4);
        let rows = |from: Pos, to: Pos, offset: i32| -> Vec<Pos> {
            cursor_rows_on_ligatures(from, to, offset, &ligatures).collect()
        };
        assert_eq!(
            rows(at(1, 3), at(2, 0), 0),
            vec![at(1, 3)],
            "left a ligature"
        );
        assert_eq!(rows(at(0, 0), at(1, 4), 0), vec![at(1, 4)], "entered one");
        assert_eq!(rows(at(1, 3), at(1, 4), 0), vec![at(1, 3), at(1, 4)]);
        assert!(rows(at(0, 0), at(2, 5), 0).is_empty(), "plain cells");
        assert_eq!(
            rows(at(1, 2), at(1, 5), 0),
            vec![at(1, 5)],
            "right after it counts, before it does not"
        );
        assert!(rows(at(1, 2), at(1, 6), 0).is_empty(), "further away");
        // Scrolled back two rows: grid row -1 is viewport row 1.
        assert_eq!(rows(at(0, 0), at(-1, 3), 2), vec![at(-1, 3)]);
        assert!(
            rows(at(0, 0), at(1, 3), 2).is_empty(),
            "viewport row 3 was never built"
        );
        assert!(
            rows(at(-5, 3), at(0, 0), 2).is_empty(),
            "above the viewport"
        );
    }

    // A frame consumed through the frontend marks the rows the cursor
    // left and entered only where its recorded ligature cells say so,
    // with the terminal's own marking off as a context sets it.
    #[test]
    fn a_cursor_move_rebuilds_rows_only_across_ligature_cells() {
        let mut term = Crosswords::new(
            CrosswordsSize::new(8, 4),
            CursorShape::Block,
            VoidListener,
            WindowId::from(0),
            0,
            10,
        );
        term.set_cursor_shaping_break(false);
        let mut content = RenderableContent::new(Cursor::default());
        // Words on rows 0 and 2; the font ligates columns 1 and 2 of
        // row 0, as the row builder would have recorded.
        term.input_str("a=>b");
        term.goto(Line(2), Column(0));
        term.input_str("cd");
        term.goto(Line(2), Column(1));
        assert!(content.update_from_terminal(&mut term, false, true));
        content.finish_update();
        content.row_ligatures = vec![rio_grid::RowLigatures::default(); 4];
        content.row_ligatures[0].set(1);
        content.row_ligatures[0].set(2);
        let clear = |content: &mut RenderableContent| {
            for row in &mut content.visible_rows {
                row.dirty = false;
            }
        };
        let dirty = |content: &RenderableContent| -> Vec<usize> {
            (0..content.visible_rows.len())
                .filter(|&y| content.visible_rows[y].dirty)
                .collect()
        };
        clear(&mut content);

        // From inside "cd" to "a": plain cells at both ends.
        term.goto(Line(0), Column(0));
        assert!(content.update_from_terminal(&mut term, false, true));
        assert!(
            dirty(&content).is_empty(),
            "no ligature at either end: {:?}",
            dirty(&content)
        );

        // Onto "=": the ligature splits, row 0 is rebuilt.
        term.goto(Line(0), Column(1));
        assert!(content.update_from_terminal(&mut term, false, true));
        assert_eq!(dirty(&content), vec![0]);
        clear(&mut content);

        // Onto "b", right after the ligature: the break before the
        // cursor cell separates it from the ligature, so row 0 again.
        term.goto(Line(0), Column(3));
        assert!(content.update_from_terminal(&mut term, false, true));
        assert_eq!(dirty(&content), vec![0]);
        clear(&mut content);
        term.goto(Line(0), Column(1));
        assert!(content.update_from_terminal(&mut term, false, true));
        clear(&mut content);

        // Off it onto "d": the ligature joins again, so row 0 is
        // rebuilt and row 2 is not.
        term.goto(Line(2), Column(1));
        assert!(content.update_from_terminal(&mut term, false, true));
        assert_eq!(dirty(&content), vec![0]);
        clear(&mut content);

        // With the break turned off nothing is marked, even onto "=".
        term.goto(Line(0), Column(1));
        assert!(content.update_from_terminal(&mut term, false, false));
        assert!(dirty(&content).is_empty());
    }
}
