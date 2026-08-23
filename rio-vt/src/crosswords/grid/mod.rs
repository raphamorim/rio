// grid/mod.rs was originally taken from Alacritty
// https://github.com/alacritty/alacritty/blob/e35e5ad14fce8456afdd89f2b392b9924bb27471/alacritty_terminal/src/grid/mod.rs
// which is licensed under Apache 2.0 license.

pub mod resize;
pub mod row;
pub mod storage;

#[cfg(test)]
mod tests;

use crate::crosswords::pos::Pos;
use crate::crosswords::Cursor;
use crate::crosswords::{Column, Line};
use row::Row;
use std::cmp::{max, min};
use std::ops::{Bound, Deref, Index, IndexMut, Range, RangeBounds};
use storage::Storage;

#[derive(Debug, Copy, Clone)]
pub enum Scroll {
    Delta(i32),
    PageUp,
    PageDown,
    Top,
    Bottom,
}

pub trait GridSquare: Sized {
    fn is_empty(&self) -> bool;
    fn reset(&mut self, template: &Self);
}

#[derive(Debug, Clone)]
pub struct Grid<T> {
    /// Current cursor for writing data.
    pub cursor: Cursor<T>,

    /// Last saved cursor.
    pub saved_cursor: Cursor<T>,

    /// Lines in the grid. Each row holds a list of cells corresponding to the
    /// columns in that row.
    pub raw: Storage<T>,

    /// Number of columns.
    columns: usize,

    /// Number of visible lines.
    lines: usize,

    /// Offset of displayed area.
    ///
    /// If the displayed region isn't at the bottom of the screen, it stays
    /// stationary while more text is emitted. The scrolling implementation
    /// updates this offset accordingly.
    display_offset: usize,

    /// Maximum number of lines in history.
    max_scroll_limit: usize,

    /// Total lines ever evicted off the scrollback ring (dropped from
    /// history at the cap, shrunk by a config change, or purged by
    /// `clear_history`). Together with `history_size` this defines a
    /// stable absolute row space: image placements anchor at
    /// `total_lines_scrolled + history_size + screen_row` and stay
    /// glued to their content even after the ring saturates.
    total_lines_scrolled: u64,

    /// Per-grid intern table for cell styles. Cells store only a `StyleId`;
    /// the actual fg/bg/underline_color/sgr-flags live here and are looked up
    /// at render/SGR-mutation time. The renderer snapshots a clone under the
    /// terminal lock so post-unlock reads don't race PTY writes.
    pub style_set: crate::crosswords::style::StyleSet,

    /// Per-grid storage for the rare per-cell data that used to live inside
    /// `CellExtra` (zero-width chars, hyperlinks, sixel/iterm graphics).
    pub extras_table: ExtrasTable,

    /// When set before `resize`, the column reflow records an exact
    /// old-row to new-row mapping into `reflow_remap` so the caller
    /// can re-anchor image placements to wherever their rows landed.
    /// Costs one Vec sized to the ring, so it is only requested when
    /// placements exist.
    pub track_reflow_remap: bool,

    /// Output of the last tracked column reflow; `None` when tracking
    /// was off or the column count did not change.
    pub reflow_remap: Option<ReflowRemap>,
}

/// Exact row mapping recorded during a column reflow, in oldest-first
/// ring positions. A row's absolute index is `base_abs + position`;
/// this holds on both sides of the reflow because cap truncation drops
/// oldest rows and advances the eviction base by the same amount.
#[derive(Debug, Clone)]
pub struct ReflowRemap {
    /// Absolute index of ring position 0 when the reflow started.
    pub base_abs: u64,
    /// For each old position, the position where that row's first
    /// cell landed, or `-1` if the row's content was dropped.
    pub new_pos: Vec<i64>,
}

impl ReflowRemap {
    /// Remap an absolute row through the reflow. `None` means the row
    /// was dropped. Rows already off the ring pass through unchanged;
    /// scrollback expiry owns those.
    pub fn remap_abs(&self, abs: i64) -> Option<i64> {
        let pos = abs - self.base_abs as i64;
        if pos < 0 {
            return Some(abs);
        }
        let new = *self.new_pos.get(pos as usize)?;
        if new < 0 {
            return None;
        }
        Some(self.base_abs as i64 + new)
    }
}

/// Slot table for `square::Extras`. Index `0` is reserved as the "no extras"
/// sentinel — `Square::extras_id() == None` corresponds to id 0. Slots are
/// reused via a free list when cells are cleared.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExtrasTable {
    slots: Vec<Option<crate::crosswords::square::Extras>>,
    free: Vec<u16>,
    /// Content-hash interning: identical extras share one slot.
    /// Emoji and combining sequences repeat constantly, and every
    /// entry here used to burn a fresh slot toward the `u16::MAX`
    /// cap (whose overflow silently drops extras). Invariant: every
    /// live slot's content maps back to its id, maintained by
    /// `alloc` and `sweep_unmarked`/`free`/`clear`. Interning also
    /// means slots are shared — mutating one in place would edit
    /// every referencing cell, so writers must copy-on-write
    /// (clone, modify, re-alloc).
    lookup: rustc_hash::FxHashMap<crate::crosswords::square::Extras, u16>,
    /// Allocations since the last mark-and-sweep. The table holds
    /// hyperlink and zero-width data whose slots stay referenced by
    /// cells until their rows scroll off the ring; without a periodic
    /// sweep, hyperlink-heavy workloads exhaust the u16 id space and
    /// new hyperlinks silently drop. (Images no longer live here;
    /// placements own them with deterministic cleanup.)
    allocs_since_reclaim: usize,
}

/// One `reclaim_extras` mark-and-sweep per this many allocations. The
/// mark walks history rows gated by `has_extras`, so the amortized
/// cost per allocation stays sub-microsecond while dead slots are
/// recycled within a bounded drift window.
const EXTRAS_RECLAIM_CADENCE: usize = 4096;

impl ExtrasTable {
    pub fn new() -> Self {
        // Reserve slot 0 as the "none" sentinel so we can use a non-zero id
        // to mean "has extras".
        Self {
            slots: vec![None],
            free: Vec::new(),
            lookup: rustc_hash::FxHashMap::default(),
            allocs_since_reclaim: 0,
        }
    }

    pub fn get(
        &self,
        id: crate::crosswords::square::ExtrasId,
    ) -> Option<&crate::crosswords::square::Extras> {
        self.slots.get(id as usize)?.as_ref()
    }

    // NOTE: there is deliberately no `get_mut`. Slots are interned and
    // shared by every cell with equal content; handing out `&mut`
    // would let a writer edit all of them at once. Mutate by cloning,
    // changing, and re-`alloc`ing (copy-on-write).

    /// Return the slot holding `extras`, interning by content: repeat
    /// content reuses its existing slot, new content allocates one.
    /// The returned id is always non-zero (0 on slot exhaustion).
    pub fn alloc(
        &mut self,
        extras: crate::crosswords::square::Extras,
    ) -> crate::crosswords::square::ExtrasId {
        if let Some(&id) = self.lookup.get(&extras) {
            return id;
        }
        self.allocs_since_reclaim += 1;
        if let Some(id) = self.free.pop() {
            self.lookup.insert(extras.clone(), id);
            self.slots[id as usize] = Some(extras);
            return id;
        }
        if self.slots.len() >= u16::MAX as usize {
            tracing::warn!("ExtrasTable hit u16::MAX slots; dropping new extras");
            return 0;
        }
        let id = self.slots.len() as u16;
        self.lookup.insert(extras.clone(), id);
        self.slots.push(Some(extras));
        id
    }

    /// Nearly out of slot ids: time for `Grid::reclaim_extras`.
    pub fn under_pressure(&self) -> bool {
        self.slots.len() >= u16::MAX as usize && self.free.len() < 256
    }

    /// Whether the caller should run `reclaim_extras` now: either the
    /// allocation cadence elapsed, or the id space is nearly full.
    pub fn should_reclaim(&self) -> bool {
        self.allocs_since_reclaim >= EXTRAS_RECLAIM_CADENCE || self.under_pressure()
    }

    pub(crate) fn reset_reclaim_cadence(&mut self) {
        self.allocs_since_reclaim = 0;
    }

    /// Free every allocated slot whose bit is not set in `live`.
    pub fn sweep_unmarked(&mut self, live: &[u64]) {
        for id in 1..self.slots.len() {
            let marked = live[id / 64] & (1 << (id % 64)) != 0;
            if !marked && self.slots[id].is_some() {
                if let Some(extras) = self.slots[id].take() {
                    self.lookup.remove(&extras);
                }
                self.free.push(id as u16);
            }
        }
    }

    // NOTE: there is deliberately no per-slot `free()`. Slots are
    // interned and may be referenced by any number of cells; the only
    // safe reclamation is the mark-and-sweep (`sweep_unmarked`), which
    // proves a slot unreferenced before releasing it.
}

impl<T: GridSquare + Default + PartialEq + Clone> Grid<T> {
    pub fn new(lines: usize, columns: usize, max_scroll_limit: usize) -> Grid<T> {
        Grid {
            raw: Storage::with_capacity(lines, columns),
            max_scroll_limit,
            total_lines_scrolled: 0,
            track_reflow_remap: false,
            reflow_remap: None,
            display_offset: 0,
            saved_cursor: Cursor::default(),
            cursor: Cursor::default(),
            lines,
            columns,
            style_set: crate::crosswords::style::StyleSet::new(),
            extras_table: ExtrasTable::new(),
        }
    }

    /// Update the size of the scrollback history.
    pub fn update_history(&mut self, history_size: usize) {
        let current_history_size = self.history_size();
        if current_history_size > history_size {
            let dropped = current_history_size - history_size;
            self.total_lines_scrolled += dropped as u64;
            self.raw.shrink_lines(dropped);
        }
        self.display_offset = min(self.display_offset, history_size);
        self.max_scroll_limit = history_size;
    }

    pub fn scroll_display(&mut self, scroll: Scroll) {
        self.display_offset = match scroll {
            Scroll::Delta(count) => min(
                max((self.display_offset as i32) + count, 0) as usize,
                self.history_size(),
            ),
            Scroll::PageUp => min(self.display_offset + self.lines, self.history_size()),
            Scroll::PageDown => self.display_offset.saturating_sub(self.lines),
            Scroll::Top => self.history_size(),
            Scroll::Bottom => 0,
        };
    }

    fn increase_scroll_limit(&mut self, count: usize) {
        let count = min(count, self.max_scroll_limit - self.history_size());
        if count != 0 {
            self.raw.initialize(count, self.columns);
        }
    }

    fn decrease_scroll_limit(&mut self, count: usize) {
        // NOTE: not counted into `total_lines_scrolled`. The only
        // caller is `grow_lines`, which trims the surplus rows created
        // by growing the visible area; no content leaves the ring.
        let count = min(count, self.history_size());
        if count != 0 {
            self.raw.shrink_lines(min(count, self.history_size()));
            self.display_offset = min(self.display_offset, self.history_size());
        }
    }

    #[inline]
    pub fn scroll_down(&mut self, region: &Range<Line>, positions: usize) {
        // When rotating the entire region, just reset everything.
        if region.end - region.start <= positions {
            for i in (region.start.0..region.end.0).map(Line::from) {
                self.raw[i].reset(&self.cursor.template);
            }

            return;
        }

        // Which implementation we can use depends on the existence of a scrollback history.
        //
        // Since a scrollback history prevents us from rotating the entire buffer downwards, we
        // instead have to rely on a slower, swap-based implementation.
        if self.max_scroll_limit == 0 {
            // Swap the lines fixed at the bottom to their target positions after rotation.
            //
            // Since we've made sure that the rotation will never rotate away the entire region, we
            // know that the position of the fixed lines before the rotation must already be
            // visible.
            //
            // We need to start from the top, to make sure the fixed lines aren't swapped with each
            // other.
            let screen_lines = self.screen_lines() as i32;
            for i in (region.end.0..screen_lines).map(Line::from) {
                self.raw.swap(i, i - positions as i32);
            }

            // Rotate the entire line buffer downward.
            self.raw.rotate_down(positions);

            // Ensure all new lines are fully cleared.
            for i in (0..positions).map(Line::from) {
                self.raw[i].reset(&self.cursor.template);
            }

            // Swap the fixed lines at the top back into position.
            for i in (0..region.start.0).map(Line::from) {
                self.raw.swap(i, i + positions);
            }
        } else {
            // Subregion rotation.
            let range = (region.start + positions).0..region.end.0;
            for line in range.rev().map(Line::from) {
                self.raw.swap(line, line - positions);
            }

            let range = region.start.0..(region.start + positions).0;
            for line in range.rev().map(Line::from) {
                self.raw[line].reset(&self.cursor.template);
            }
        }

        // See `scroll_up` for the rationale — `raw.swap` / `raw.rotate`
        // don't propagate `Row::dirty` across line indices, so we mark
        // the whole region dirty at the end.
        for i in (region.start.0..region.end.0).map(Line::from) {
            self.raw[i].dirty = true;
        }
    }

    pub fn cursor_square(&mut self) -> &mut T {
        let pos = &self.cursor.pos;
        &mut self.raw[pos.row][pos.col]
    }

    /// Move lines at the bottom toward the top.
    ///
    /// This is the performance-sensitive part of scrolling.
    pub fn scroll_up(&mut self, region: &Range<Line>, positions: usize) {
        // Storage-level shifts below (`raw.swap`, `raw.rotate`) move
        // row content between line indices without going through
        // `IndexMut`, so the moved Row's `dirty` bit travels with the
        // content rather than tracking the destination line. We mark
        // the whole region dirty at the end so the snapshot picks up
        // the post-scroll layout. Same fix for `scroll_down` below.
        // When rotating the entire region with fixed lines at the top, just reset everything.
        if region.end - region.start <= positions && region.start != 0 {
            for i in (region.start.0..region.end.0).map(Line::from) {
                self.raw[i].reset(&self.cursor.template);
            }

            return;
        }

        // Only rotate the entire history if the active region starts at the top.
        if region.start == 0 {
            // A viewport scrolled into history stays pinned to the same
            // absolute rows by growing the offset alongside history.
            // This belongs to this branch only: a sub-region scroll
            // (region.start != 0, e.g. IL/DL) adds nothing to history,
            // and bumping the offset for it drifts the offset past the
            // rows that exist; `compute_index` then resolves visible
            // lines to the wrong storage slots.
            if self.display_offset != 0 {
                self.display_offset =
                    min(self.display_offset + positions, self.max_scroll_limit);
            }
            // Create scrollback for the new lines. Whatever the cap
            // refuses to grow is evicted off the ring instead.
            let grown = min(positions, self.max_scroll_limit - self.history_size());
            self.total_lines_scrolled += (positions - grown) as u64;
            self.increase_scroll_limit(positions);

            // Swap the lines fixed at the top to their target positions after rotation.
            //
            // Since we've made sure that the rotation will never rotate away the entire region, we
            // know that the position of the fixed lines before the rotation must already be
            // visible.
            //
            // We need to start from the bottom, to make sure the fixed lines aren't swapped with
            // each other.
            for i in (0..region.start.0).rev().map(Line::from) {
                self.raw.swap(i, i + positions);
            }

            // Rotate the entire line buffer upward.
            self.raw.rotate(-(positions as isize));

            // Swap the fixed lines at the bottom back into position.
            let screen_lines = self.screen_lines() as i32;
            for i in (region.end.0..screen_lines).rev().map(Line::from) {
                self.raw.swap(i, i - positions);
            }
        } else {
            // Rotate lines without moving anything into history.
            for i in (region.start.0..region.end.0 - positions as i32).map(Line::from) {
                self.raw.swap(i, i + positions);
            }
        }

        // Ensure all new lines are fully cleared.
        for i in (region.end.0 - positions as i32..region.end.0).map(Line::from) {
            self.raw[i].reset(&self.cursor.template);
        }

        // Mark every row in the region dirty. Reset rows above already
        // got `dirty = true` from `Row::reset`; the swap/rotate'd ones
        // still have whatever bit they carried in via the source line.
        // Full-screen scrolls skip this; the caller marks full damage,
        // and the snapshot's full path copies rows regardless of the bit.
        if region.start.0 != 0 || region.end.0 as usize != self.screen_lines() {
            self.raw
                .mark_lines_dirty(region.start, (region.end.0 - region.start.0) as usize);
        }
    }

    pub fn clear_viewport(&mut self) {
        // Determine how many lines to scroll up by.
        let end = Pos::new(Line(self.lines as i32 - 1), Column(self.columns()));
        let mut iter = self.iter_from(end);
        while let Some(square) = iter.prev() {
            if !square.is_empty() || square.pos.row < 0 {
                break;
            }
        }
        debug_assert!(iter.current.row >= -1);
        let positions = (iter.current.row.0 + 1) as usize;
        let region = Line(0)..Line(self.lines as i32);

        // Clear the viewport.
        self.scroll_up(&region, positions);

        // Reset rotated lines.
        for line in (0..(self.lines - positions)).map(Line::from) {
            self.raw[line].reset(&self.cursor.template);
        }
    }

    /// Completely reset the grid state.
    pub fn reset(&mut self) {
        self.clear_history();

        self.saved_cursor = Cursor::default();
        self.cursor = Cursor::default();
        self.display_offset = 0;

        // Reset all visible lines.
        let range = self.topmost_line().0..(self.screen_lines() as i32);
        for line in range.map(Line::from) {
            self.raw[line].reset(&self.cursor.template);
        }
    }
}

impl<T> Grid<T> {
    /// Reset a visible region within the grid.
    pub fn reset_region<R: RangeBounds<Line>>(&mut self, bounds: R)
    where
        T: GridSquare + Clone + Default + PartialEq,
    {
        let start = match bounds.start_bound() {
            Bound::Included(line) => *line,
            Bound::Excluded(line) => *line + 1,
            Bound::Unbounded => Line(0),
        };

        let end = match bounds.end_bound() {
            Bound::Included(line) => *line + 1,
            Bound::Excluded(line) => *line,
            Bound::Unbounded => Line(self.screen_lines() as i32),
        };

        debug_assert!(start < self.screen_lines() as i32);
        debug_assert!(end <= self.screen_lines() as i32);

        for line in (start.0..end.0).map(Line::from) {
            self.raw[line].reset(&self.cursor.template);
        }
    }

    /// Absolute index of the oldest row still in the ring: the base
    /// of the stable absolute row space image placements anchor in.
    #[inline]
    pub fn lines_evicted(&self) -> u64 {
        self.total_lines_scrolled
    }

    pub fn clear_history(&mut self) {
        // Explicitly purge all lines from history.
        self.total_lines_scrolled += self.history_size() as u64;
        self.raw.shrink_lines(self.history_size());

        // Reset display offset.
        self.display_offset = 0;
    }

    /// This is used only for initializing after loading ref-tests.
    #[inline]
    #[allow(unused)]
    pub fn initialize_all(&mut self)
    where
        T: GridSquare + Clone + Default,
    {
        // Remove all cached lines to clear them of any content.
        self.truncate();

        // Initialize everything with empty new lines.
        self.raw
            .initialize(self.max_scroll_limit - self.history_size(), self.columns);
    }

    /// This is used only for truncating before saving ref-tests.
    #[inline]
    #[allow(unused)]
    pub fn truncate(&mut self) {
        self.raw.truncate();
    }

    /// Iterate over all cells in the grid starting at a specific pos.
    #[inline]
    pub fn iter_from(&self, current: Pos) -> GridIterator<'_, T> {
        let end = Pos::new(self.bottommost_line(), self.last_column());
        GridIterator {
            grid: self,
            current,
            end,
        }
    }

    /// Iterate over all visible cells.
    ///
    /// This is slightly more optimized than calling `Grid::iter_from` in combination with
    /// `Iterator::take_while`.
    #[inline]
    #[allow(unused)]
    pub fn display_iter(&self) -> GridIterator<'_, T> {
        let last_column = self.last_column();
        let start = Pos::new(Line(-(self.display_offset() as i32) - 1), last_column);
        let end_line = min(start.row + self.screen_lines(), self.bottommost_line());
        let end = Pos::new(end_line, last_column);

        GridIterator {
            grid: self,
            current: start,
            end,
        }
    }

    #[inline]
    pub fn display_offset(&self) -> usize {
        self.display_offset
    }

    #[inline]
    pub fn cursor_cell(&mut self) -> &mut T {
        let point = self.cursor.pos;
        &mut self[point.row][point.col]
    }
}

use crate::crosswords::square::Square;
use crate::crosswords::style::{Style, StyleId};

impl Grid<Square> {
    /// The full text of the cell at `pos`: its base character followed by any
    /// zero-width marks (combining accents, ZWJ joiners, variation selectors).
    ///
    /// Prefer this over [`Square::c`], which returns the base character alone
    /// and so drops the marks. Reading the marks by hand also needs a guard
    /// that is easy to miss: a background-only cell reuses the extras-id bits
    /// for its color, so `extras_id()` on one yields a colour channel rather
    /// than an id.
    pub fn cell_text(&self, pos: Pos) -> impl Iterator<Item = char> + '_ {
        let square = self[pos];
        let marks = square
            .extras_id()
            .filter(|_| !square.is_bg_only())
            .and_then(|id| self.extras_table.get(id))
            .map(|extras| extras.zerowidth.as_slice())
            .unwrap_or(&[]);
        std::iter::once(square.c()).chain(marks.iter().copied())
    }

    /// Free extras slots no longer referenced by any cell.
    ///
    /// Cells are overwritten and rows drop off the scrollback ring without
    /// freeing their extras slot, so a session heavy on per-cell extras
    /// eventually exhausts the u16 id space.
    /// Mark every slot referenced by a live row (visible + history) or a
    /// cursor template, then free the rest. Swept graphic slots drop their
    /// slot contents (hyperlinks, zero-width overlays).
    /// Allocate an extras slot, transparently running the cadence
    /// mark-and-sweep when it is due. Callers never orchestrate
    /// reclamation; the table counts allocations internally and this
    /// is the only place that acts on the signal.
    pub fn alloc_extras(
        &mut self,
        extras: crate::crosswords::square::Extras,
    ) -> crate::crosswords::square::ExtrasId {
        if self.extras_table.should_reclaim() {
            self.reclaim_extras();
        }
        self.extras_table.alloc(extras)
    }

    pub fn reclaim_extras(&mut self) {
        self.extras_table.reset_reclaim_cadence();
        #[inline]
        fn mark(live: &mut [u64], sq: &Square) {
            if matches!(
                sq.content_tag(),
                crate::crosswords::square::ContentTag::Codepoint
            ) {
                if let Some(eid) = sq.extras_id() {
                    live[eid as usize / 64] |= 1 << (eid % 64);
                }
            }
        }

        let mut live = vec![0u64; (u16::MAX as usize).div_ceil(64)];
        for l in self.topmost_line().0..=self.bottommost_line().0 {
            let row = &self.raw[Line(l)];
            if !row.has_extras {
                continue;
            }
            for sq in &row.inner {
                mark(&mut live, sq);
            }
        }
        mark(&mut live, &self.cursor.template);
        mark(&mut live, &self.saved_cursor.template);
        self.extras_table.sweep_unmarked(&live);
    }

    /// Read the style associated with the cell's style id.
    #[inline]
    pub fn style_of(&self, square: &Square) -> Style {
        self.style_set.get(square.style_id())
    }

    /// Read the style id of the current cursor template.
    #[inline]
    pub fn template_style_id(&self) -> StyleId {
        self.cursor.template.style_id()
    }

    /// Set the cursor template's style id directly.
    #[inline]
    pub fn set_template_style_id(&mut self, id: StyleId) {
        self.cursor.template.set_style_id(id);
        self.cursor.pending_style = self.style_set.get(id);
        self.cursor.style_dirty = false;
    }

    /// Mutate the cursor's SGR state. Deliberately does NOT touch the
    /// style table: a sequence like `CSI 1;38;2;r;g;b m` mutates the
    /// pending style twice and interns nothing; the id is refreshed once,
    /// lazily, by [`Self::sync_template_style`] when a cell is written.
    #[inline]
    pub fn update_template_style(&mut self, f: impl FnOnce(&mut Style)) {
        f(&mut self.cursor.pending_style);
        self.cursor.style_dirty = true;
    }

    /// Set the template style by passing a fully-formed `Style`.
    #[inline]
    pub fn set_template_style(&mut self, style: Style) {
        self.cursor.pending_style = style;
        self.cursor.style_dirty = true;
    }

    /// The cursor's current SGR state, without touching the intern table.
    #[inline]
    pub fn template_style(&self) -> Style {
        self.cursor.pending_style
    }

    /// Absorb pending SGR changes into the template's interned style id.
    /// Must run before `cursor.template` is used as a cell (prints, fills,
    /// scroll resets); a no-op when nothing changed.
    #[inline]
    pub fn sync_template_style(&mut self) {
        if self.cursor.style_dirty {
            let id = self.intern_style(self.cursor.pending_style);
            self.cursor.template.set_style_id(id);
            self.cursor.style_dirty = false;
        }
    }

    /// Intern a style, transparently running the mark-and-sweep when the
    /// id space is close to exhausted. Same contract as `alloc_extras`:
    /// callers never orchestrate reclamation.
    #[inline]
    fn intern_style(&mut self, style: Style) -> StyleId {
        if self.style_set.should_sweep() {
            self.reclaim_styles();
        }
        self.style_set.intern(style)
    }

    /// Free style ids no longer referenced by any cell.
    ///
    /// Cells are overwritten and rows drop off the scrollback ring without
    /// freeing their style id, so a session that keeps generating novel
    /// truecolor pairs (gradients, animations) eventually exhausts the
    /// u16 id space. Mark every id referenced by a live row (visible +
    /// history) or a cursor template, then free the rest for reuse.
    pub fn reclaim_styles(&mut self) {
        #[inline]
        fn mark(live: &mut [u64], sq: &Square) {
            // Only Codepoint cells carry a style id; bg-only cells reuse
            // those bits for their inline color.
            if matches!(
                sq.content_tag(),
                crate::crosswords::square::ContentTag::Codepoint
            ) {
                let id = sq.style_id();
                live[id as usize / 64] |= 1 << (id % 64);
            }
        }

        let mut live = vec![0u64; (u16::MAX as usize + 1).div_ceil(64)];
        live[0] |= 1;
        for l in self.topmost_line().0..=self.bottommost_line().0 {
            for sq in &self.raw[Line(l)].inner {
                mark(&mut live, sq);
            }
        }
        mark(&mut live, &self.cursor.template);
        mark(&mut live, &self.saved_cursor.template);
        self.style_set.sweep_unmarked(&live);
    }

    /// Build a "blank cell with this bg color" using the default style for
    /// every other field. Used by `erase_chars`/`delete_chars`/`insert_blank`
    /// which need to overwrite cells with a colored background but reset
    /// every other attribute.
    ///
    /// When the bg color can be encoded inline (palette index or RGB), this
    /// returns a bg-only cell that bypasses the style table entirely. The
    /// renderer's hot path detects bg-only cells and skips the lookup,
    /// which makes large filled regions (selection highlight, blank lines
    /// after `clear`, color block fills) essentially free to render.
    #[inline]
    pub fn blank_with_bg(&mut self, bg: crate::config::colors::AnsiColor) -> Square {
        use crate::config::colors::{AnsiColor, NamedColor};

        let mut cell = Square::default();
        match bg {
            // Default background → fully default cell, no encoding needed.
            AnsiColor::Named(NamedColor::Background) => return cell,

            // Palette index → bg-only cell, inline encoding.
            AnsiColor::Indexed(idx) => {
                cell.set_bg_palette(idx);
                return cell;
            }

            // RGB spec → bg-only cell, inline encoding.
            AnsiColor::Spec(rgb) => {
                cell.set_bg_rgb(rgb.r, rgb.g, rgb.b);
                return cell;
            }

            // Named palette colors 0..15 → encode as palette index.
            AnsiColor::Named(named) => {
                let n = named as u16;
                if n < 16 {
                    cell.set_bg_palette(n as u8);
                    return cell;
                }
                // Special named colors (Foreground, Cursor, Dim*, Light*)
                // fall through to the style table because their meaning
                // depends on the active palette and would require lookup
                // anyway.
            }
        }

        // Fallback: intern a regular style. Should be rare in practice.
        let style = Style {
            bg,
            ..Style::default()
        };
        let id = self.intern_style(style);
        Square::default().with_style_id(id)
    }
}

impl<T: PartialEq> PartialEq for Grid<T> {
    fn eq(&self, other: &Self) -> bool {
        // Compare struct fields and check result of grid comparison.
        self.raw.eq(&other.raw)
            && self.columns.eq(&other.columns)
            && self.lines.eq(&other.lines)
            && self.display_offset.eq(&other.display_offset)
    }
}

impl<T> Index<Line> for Grid<T> {
    type Output = Row<T>;

    #[inline]
    fn index(&self, index: Line) -> &Row<T> {
        &self.raw[index]
    }
}

impl<T> IndexMut<Line> for Grid<T> {
    #[inline]
    fn index_mut(&mut self, index: Line) -> &mut Row<T> {
        &mut self.raw[index]
    }
}

impl<T> Index<Pos> for Grid<T> {
    type Output = T;

    #[inline]
    fn index(&self, pos: Pos) -> &T {
        &self[pos.row][pos.col]
    }
}

impl<T> IndexMut<Pos> for Grid<T> {
    #[inline]
    fn index_mut(&mut self, pos: Pos) -> &mut T {
        &mut self[pos.row][pos.col]
    }
}

pub trait Dimensions {
    /// Total number of lines in the buffer, this includes scrollback and visible lines.
    fn total_lines(&self) -> usize;

    /// Height of the viewport in lines.
    fn screen_lines(&self) -> usize;

    /// Width of the terminal in columns.
    fn columns(&self) -> usize;

    /// Index for the last column.
    #[inline]
    fn last_column(&self) -> Column {
        Column(self.columns() - 1)
    }

    /// Line farthest up in the grid history.
    #[inline]
    fn topmost_line(&self) -> Line {
        Line(-(self.history_size() as i32))
    }

    /// Line farthest down in the grid history.
    #[inline]
    fn bottommost_line(&self) -> Line {
        Line(self.screen_lines() as i32 - 1)
    }

    /// Number of invisible lines part of the scrollback history.
    #[inline]
    fn history_size(&self) -> usize {
        self.total_lines().saturating_sub(self.screen_lines())
    }

    /// square height in pixels.
    #[inline]
    fn square_height(&self) -> f32 {
        0.0
    }

    /// square width in pixels.
    #[inline]
    fn square_width(&self) -> f32 {
        0.0
    }
}

impl<G> Dimensions for Grid<G> {
    #[inline]
    fn total_lines(&self) -> usize {
        self.raw.len()
    }

    #[inline]
    fn screen_lines(&self) -> usize {
        self.lines
    }

    #[inline]
    fn columns(&self) -> usize {
        self.columns
    }

    #[inline]
    fn square_width(&self) -> f32 {
        0.
    }
    #[inline]
    fn square_height(&self) -> f32 {
        0.
    }
}

#[cfg(test)]
impl Dimensions for (usize, usize) {
    fn total_lines(&self) -> usize {
        self.0
    }
    fn screen_lines(&self) -> usize {
        self.0
    }
    fn columns(&self) -> usize {
        self.1
    }
    fn square_width(&self) -> f32 {
        0.
    }
    fn square_height(&self) -> f32 {
        0.
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Indexed<T> {
    pub pos: Pos,
    pub square: T,
}

impl<T> Deref for Indexed<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        &self.square
    }
}

pub struct GridIterator<'a, T> {
    /// Immutable grid reference.
    grid: &'a Grid<T>,

    /// Current position of the iterator within the grid.
    current: Pos,

    /// Last cell included in the iterator.
    end: Pos,
}

impl<'a, T> GridIterator<'a, T> {
    /// Current iterator position.
    #[allow(unused)]
    pub fn pos(&self) -> Pos {
        self.current
    }

    /// Cell at the current iterator position.
    #[allow(unused)]
    pub fn square(&self) -> &'a T {
        &self.grid[self.current]
    }
}

impl<'a, T> Iterator for GridIterator<'a, T> {
    type Item = Indexed<&'a T>;

    fn next(&mut self) -> Option<Self::Item> {
        // Stop once we've reached the end of the grid.
        if self.current >= self.end {
            return None;
        }

        match self.current {
            Pos { col, .. } if col == self.grid.last_column() => {
                self.current.col = Column(0);
                self.current.row += 1;
            }
            _ => self.current.col += Column(1),
        }

        // Guard both axes before indexing (#1713): positions can be
        // stale relative to the live grid (resize between capture and
        // use), and history rows keep their old length across column
        // growth. Ending the iteration beats panicking.
        let screen_lines = self.grid.screen_lines() as i32;
        let history = (self.grid.total_lines() - self.grid.screen_lines()) as i32;
        if self.current.row.0 >= screen_lines || self.current.row.0 < -history {
            return None;
        }
        let row = &self.grid[self.current.row];
        if self.current.col.0 >= row.len() {
            return None;
        }

        Some(Indexed {
            square: &self.grid[self.current],
            pos: self.current,
        })
    }
}

/// Bidirectional iterator.
pub trait BidirectionalIterator: Iterator {
    fn prev(&mut self) -> Option<Self::Item>;
}

impl<T> BidirectionalIterator for GridIterator<'_, T> {
    fn prev(&mut self) -> Option<Self::Item> {
        let topmost_line = self.grid.topmost_line();
        let last_column = self.grid.last_column();

        // Stop once we've reached the end of the grid.
        if self.current == Pos::new(topmost_line, Column(0)) {
            return None;
        }

        match self.current {
            Pos { col: Column(0), .. } => {
                self.current.col = last_column;
                self.current.row -= 1;
            }
            _ => self.current.col -= Column(1),
        }

        Some(Indexed {
            square: &self.grid[self.current],
            pos: self.current,
        })
    }
}
