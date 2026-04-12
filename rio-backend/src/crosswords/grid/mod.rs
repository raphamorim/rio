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

    /// Per-grid intern table for cell styles. Cells store only a `StyleId`;
    /// the actual fg/bg/underline_color/sgr-flags live here and are looked up
    /// at render/SGR-mutation time.
    pub style_set: crate::crosswords::style::StyleSet,

    /// Per-grid storage for the rare per-cell data that used to live inside
    /// `CellExtra` (zero-width chars, hyperlinks, sixel/iterm graphics).
    pub extras_table: ExtrasTable,
}

/// Slot table for `square::Extras`. Index `0` is reserved as the "no extras"
/// sentinel — `Square::extras_id() == None` corresponds to id 0. Slots are
/// reused via a free list when cells are cleared.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExtrasTable {
    slots: Vec<Option<crate::crosswords::square::Extras>>,
    free: Vec<u16>,
}

impl ExtrasTable {
    pub fn new() -> Self {
        // Reserve slot 0 as the "none" sentinel so we can use a non-zero id
        // to mean "has extras".
        Self {
            slots: vec![None],
            free: Vec::new(),
        }
    }

    pub fn get(
        &self,
        id: crate::crosswords::square::ExtrasId,
    ) -> Option<&crate::crosswords::square::Extras> {
        self.slots.get(id as usize)?.as_ref()
    }

    pub fn get_mut(
        &mut self,
        id: crate::crosswords::square::ExtrasId,
    ) -> Option<&mut crate::crosswords::square::Extras> {
        self.slots.get_mut(id as usize)?.as_mut()
    }

    /// Allocate a new extras slot, returning its id (always non-zero).
    pub fn alloc(
        &mut self,
        extras: crate::crosswords::square::Extras,
    ) -> crate::crosswords::square::ExtrasId {
        if let Some(id) = self.free.pop() {
            self.slots[id as usize] = Some(extras);
            return id;
        }
        if self.slots.len() >= u16::MAX as usize {
            tracing::warn!("ExtrasTable hit u16::MAX slots; dropping new extras");
            return 0;
        }
        let id = self.slots.len() as u16;
        self.slots.push(Some(extras));
        id
    }

    /// Free a previously-allocated slot. No-op if `id == 0`.
    pub fn free(&mut self, id: crate::crosswords::square::ExtrasId) {
        if id == 0 {
            return;
        }
        if let Some(slot) = self.slots.get_mut(id as usize) {
            if slot.take().is_some() {
                self.free.push(id);
            }
        }
    }

    pub fn clear(&mut self) {
        self.slots.clear();
        self.slots.push(None);
        self.free.clear();
    }
}

impl<T: GridSquare + Default + PartialEq + Clone> Grid<T> {
    pub fn new(lines: usize, columns: usize, max_scroll_limit: usize) -> Grid<T> {
        Grid {
            raw: Storage::with_capacity(lines, columns),
            max_scroll_limit,
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
            self.raw.shrink_lines(current_history_size - history_size);
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
    }

    pub fn cursor_square(&mut self) -> &mut T {
        let pos = &self.cursor.pos;
        &mut self.raw[pos.row][pos.col]
    }

    /// Move lines at the bottom toward the top.
    ///
    /// This is the performance-sensitive part of scrolling.
    pub fn scroll_up(&mut self, region: &Range<Line>, positions: usize) {
        // When rotating the entire region with fixed lines at the top, just reset everything.
        if region.end - region.start <= positions && region.start != 0 {
            for i in (region.start.0..region.end.0).map(Line::from) {
                self.raw[i].reset(&self.cursor.template);
            }

            return;
        }

        // Update display offset when not pinned to active area.
        if self.display_offset != 0 {
            self.display_offset =
                min(self.display_offset + positions, self.max_scroll_limit);
        }

        // Only rotate the entire history if the active region starts at the top.
        if region.start == 0 {
            // Create scrollback for the new lines.
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
        T: GridSquare + Clone + Default,
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

    #[inline]
    pub fn clear_history(&mut self) {
        // Explicitly purge all lines from history.
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

// ---------------------------------------------------------------------------
// Helpers specific to Grid<Square> for working with the per-grid style table
// and extras storage.
// ---------------------------------------------------------------------------

use crate::crosswords::square::Square;
use crate::crosswords::style::{Style, StyleId};

impl Grid<Square> {
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
    }

    /// Mutate the cursor template's style by recomputing-and-reinterning.
    /// Used by the SGR handler: `cursor.template` doesn't carry inline
    /// fg/bg/flags anymore, so updates have to round-trip through the
    /// style table.
    #[inline]
    pub fn update_template_style(&mut self, f: impl FnOnce(&mut Style)) {
        let mut s = self.style_set.get(self.cursor.template.style_id());
        f(&mut s);
        let id = self.style_set.intern(s);
        self.cursor.template.set_style_id(id);
    }

    /// Set the template style by passing a fully-formed `Style`.
    #[inline]
    pub fn set_template_style(&mut self, style: Style) {
        let id = self.style_set.intern(style);
        self.cursor.template.set_style_id(id);
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
        let id = self.style_set.intern(style);
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
