use rio_backend::ansi::graphics::{
    AtlasPlacement, KittyPlacement, StoredImage, VirtualPlacement,
};
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
            row_styles: Vec::new(),
            extras: rustc_hash::FxHashMap::default(),
            term_colors: TermColors::default(),
            display_offset: 0,
            columns: 0,
            screen_lines: 0,
            history_size: 0,
            lines_evicted: 0,
            atlas_placements: Vec::new(),
            blinking_cursor: false,
            kitty_virtual_placements: FxHashMap::default(),
            kitty_images: FxHashMap::default(),
            kitty_placements: Vec::new(),
            kitty_graphics_dirty: false,
        }
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
    use super::PendingUpdate;
    use rio_backend::ansi::CursorShape;
    use rio_backend::crosswords::grid::row::Row;
    use rio_backend::crosswords::pos::{Column, Line};
    use rio_backend::crosswords::square::{Extras, Square};
    use rio_backend::crosswords::{Crosswords, CrosswordsSize};
    use rio_backend::event::{TerminalDamage, VoidListener, WindowId};
    use rio_backend::performer::handler::Processor;
    use rustc_hash::FxHashMap;

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

    /// The renderer's per-panel state: snapshot buffers plus the
    /// painted mirror standing in for the GPU grid.
    struct Frame {
        visible_rows: Vec<Row<Square>>,
        row_styles: Vec<Vec<rio_backend::crosswords::style::Style>>,
        extras: FxHashMap<u16, Extras>,
        painted: Vec<String>,
    }

    impl Frame {
        fn new() -> Self {
            Self {
                visible_rows: Vec::new(),
                row_styles: Vec::new(),
                extras: FxHashMap::default(),
                painted: vec![String::new(); ROWS],
            }
        }

        /// Mirror of `renderer/mod.rs` consumption + `screen::render`
        /// row-rebuild decisions, step for step.
        fn consume(
            &mut self,
            term: &mut Crosswords<VoidListener>,
            ui: Option<TerminalDamage>,
        ) {
            let rows = term.screen_lines();
            let cols = term.columns();
            term.damage_event_in_flight = false;
            let pty = term.peek_damage_event();
            let damage = match (ui, pty) {
                (Some(u), Some(p)) => PendingUpdate::merge_terminal_damages(u, p),
                (Some(d), None) | (None, Some(d)) => d,
                (None, None) => TerminalDamage::Noop,
            };
            term.reset_damage();
            let needs_full =
                matches!(damage, TerminalDamage::Full) || self.visible_rows.len() != rows;
            term.snapshot_visible(
                &damage,
                &mut self.visible_rows,
                &mut self.row_styles,
                &mut self.extras,
            );
            self.painted.resize(rows, String::new());
            if needs_full {
                // RowsToRebuild::All: rebuild everything, bits not
                // cleared (matches screen::render).
                for y in 0..self.visible_rows.len() {
                    self.painted[y] = row_text(&self.visible_rows[y], cols);
                }
                return;
            }
            match damage {
                TerminalDamage::Full => unreachable!(),
                TerminalDamage::Partial => {
                    for y in 0..self.visible_rows.len() {
                        if !self.visible_rows[y].dirty {
                            continue;
                        }
                        self.painted[y] = row_text(&self.visible_rows[y], cols);
                        self.visible_rows[y].dirty = false;
                    }
                }
                TerminalDamage::CursorOnly | TerminalDamage::Noop => {}
            }
        }
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
            frame.consume(term, None);
            spins += 1;
            assert!(spins < 8, "{context}: damage never quiesces");
        }
        // A cursor-only event may fire once more; consuming it must not
        // change painted content and must reach silence.
        if event_would_fire(term) {
            frame.consume(term, None);
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

    /// Deterministic vim-shaped session: alt screen, scroll region with
    /// a status line, line redraws with EL, scrolls in both directions,
    /// insert/delete lines, with a renderer consuming at every step.
    #[test]
    fn vim_scroll_session_converges() {
        let mut term = new_term();
        let mut parser = Processor::default();
        let mut frame = Frame::new();
        frame.consume(&mut term, Some(TerminalDamage::Full));

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
            frame.consume(&mut term, Some(TerminalDamage::Full));

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
                            frame.consume(&mut term, ui.take());
                        }
                    }
                    _ => {
                        journal.push(format!("s{step} consume(forced)"));
                        frame.consume(&mut term, ui.take());
                    }
                }
                if let Some(ui) = ui {
                    // UI damage that missed this frame is carried to
                    // the next consume, like PendingUpdate does.
                    frame.consume(&mut term, Some(ui));
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
