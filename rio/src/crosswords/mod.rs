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

*/

pub mod attr;
pub mod dimensions;
pub mod pos;
pub mod row;
pub mod square;
pub mod storage;

use crate::crosswords::square::CrosswordsSquare;
use crate::performer::handler::Handler;
use attr::*;
use bitflags::bitflags;
use colors::AnsiColor;
use dimensions::Dimensions;
use pos::CharsetIndex;
use pos::{Column, Cursor, Line, Pos};
use row::Row;
use square::Square;
use std::cmp::max;
use std::cmp::min;
use std::cmp::Ordering;
use std::mem;
use std::ops::{Index, IndexMut, Range};
use std::ptr;
use storage::Storage;
use unicode_width::UnicodeWidthChar;
use crate::ansi::mode::Mode as AnsiMode;

pub type NamedColor = colors::NamedColor;

pub const MIN_COLUMNS: usize = 2;
pub const MIN_VISIBLE_ROWS: usize = 1;

bitflags! {
    #[derive(Debug, Clone)]
    pub struct Mode: u32 {
        const NONE                = 0;
        const SHOW_CURSOR         = 0b0000_0000_0000_0000_0001;
        const APP_CURSOR          = 0b0000_0000_0000_0000_0010;
        const APP_KEYPAD          = 0b0000_0000_0000_0000_0100;
        const MOUSE_REPORT_CLICK  = 0b0000_0000_0000_0000_1000;
        const BRACKETED_PASTE     = 0b0000_0000_0000_0001_0000;
        const SGR_MOUSE           = 0b0000_0000_0000_0010_0000;
        const MOUSE_MOTION        = 0b0000_0000_0000_0100_0000;
        const LINE_WRAP           = 0b0000_0000_0000_1000_0000;
        const LINE_FEED_NEW_LINE  = 0b0000_0000_0001_0000_0000;
        const ORIGIN              = 0b0000_0000_0010_0000_0000;
        const INSERT              = 0b0000_0000_0100_0000_0000;
        const FOCUS_IN_OUT        = 0b0000_0000_1000_0000_0000;
        const ALT_SCREEN          = 0b0000_0001_0000_0000_0000;
        const MOUSE_DRAG          = 0b0000_0010_0000_0000_0000;
        const MOUSE_MODE          = 0b0000_0010_0000_0100_1000;
        const UTF8_MOUSE          = 0b0000_0100_0000_0000_0000;
        const ALTERNATE_SCROLL    = 0b0000_1000_0000_0000_0000;
        const VI                  = 0b0001_0000_0000_0000_0000;
        const URGENCY_HINTS       = 0b0010_0000_0000_0000_0000;
        const ANY                 = u32::MAX;
    }
}

impl Default for Mode {
    fn default() -> Mode {
        Mode::SHOW_CURSOR
            | Mode::LINE_WRAP
            | Mode::ALTERNATE_SCROLL
            | Mode::URGENCY_HINTS
    }
}

#[derive(Debug, Clone)]
pub struct Crosswords<U> {
    active_charset: CharsetIndex,
    cols: usize,
    cursor: Cursor<Square>,
    saved_cursor: Cursor<Square>,
    mode: Mode,
    rows: usize,
    scroll: usize,
    scroll_limit: usize,
    scroll_region: Range<Line>,
    storage: Storage<Square>,
    tabs: TabStops,
    #[allow(dead_code)]
    event_proxy: U,
    window_title: Option<String>,
    damage: TermDamageState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineDamageBounds {
    /// Damaged line number.
    pub line: usize,

    /// Leftmost damaged column.
    pub left: usize,

    /// Rightmost damaged column.
    pub right: usize,
}

impl LineDamageBounds {
    #[inline]
    pub fn undamaged(line: usize, num_cols: usize) -> Self {
        Self {
            line,
            left: num_cols,
            right: 0,
        }
    }

    #[inline]
    pub fn reset(&mut self, num_cols: usize) {
        *self = Self::undamaged(self.line, num_cols);
    }

    #[inline]
    pub fn expand(&mut self, left: usize, right: usize) {
        self.left = std::cmp::min(self.left, left);
        self.right = std::cmp::max(self.right, right);
    }

    #[inline]
    pub fn is_damaged(&self) -> bool {
        self.left <= self.right
    }
}

#[derive(Debug, Clone)]
struct TermDamageState {
    /// Hint whether terminal should be damaged entirely regardless of the actual damage changes.
    is_fully_damaged: bool,

    /// Information about damage on terminal lines.
    lines: Vec<LineDamageBounds>,

    /// Old terminal cursor point.
    last_cursor: Pos,

    /// Last Vi cursor point.
    last_vi_cursor_point: Option<Pos>,
    // Old selection range.
    // last_selection: Option<SelectionRange>,
}

impl TermDamageState {
    fn new(num_cols: usize, num_lines: usize) -> Self {
        let lines = (0..num_lines)
            .map(|line| LineDamageBounds::undamaged(line, num_cols))
            .collect();

        Self {
            is_fully_damaged: true,
            lines,
            last_cursor: Default::default(),
            last_vi_cursor_point: Default::default(),
            // last_selection: Default::default(),
        }
    }

    #[inline]
    fn resize(&mut self, num_cols: usize, num_lines: usize) {
        // Reset point, so old cursor won't end up outside of the viewport.
        self.last_cursor = Default::default();
        self.last_vi_cursor_point = None;
        // self.last_selection = None;
        self.is_fully_damaged = true;

        self.lines.clear();
        self.lines.reserve(num_lines);
        for line in 0..num_lines {
            self.lines.push(LineDamageBounds::undamaged(line, num_cols));
        }
    }

    /// Damage point inside of the viewport.
    #[inline]
    fn damage_point(&mut self, point: Pos) {
        // self.damage_line(point.line, point.column.0, point.column.0);
    }

    /// Expand `line`'s damage to span at least `left` to `right` column.
    #[inline]
    fn damage_line(&mut self, line: usize, left: usize, right: usize) {
        self.lines[line].expand(left, right);
    }

    fn damage_selection(
        &mut self,
        // selection: SelectionRange,
        display_offset: usize,
        num_cols: usize,
    ) {
        let display_offset = display_offset as i32;
        let last_visible_line = self.lines.len() as i32 - 1;

        // Don't damage invisible selection.
        // if selection.end.line.0 + display_offset < 0
        //     || selection.start.line.0.abs() < display_offset - last_visible_line
        // {
        //     return;
        // };

        // let start = std::cmp::max(selection.start.line.0 + display_offset, 0);
        // let end = (selection.end.line.0 + display_offset).clamp(0, last_visible_line);
        // for line in start as usize..=end as usize {
        //     self.damage_line(line, 0, num_cols - 1);
        // }
    }

    /// Reset information about terminal damage.
    fn reset(&mut self, num_cols: usize) {
        self.is_fully_damaged = false;
        self.lines.iter_mut().for_each(|line| line.reset(num_cols));
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
    #[allow(unused)]
    fn clear_all(&mut self) {
        unsafe {
            ptr::write_bytes(self.tabs.as_mut_ptr(), 0, self.tabs.len());
        }
    }

    /// Increase tabstop capacity.
    #[inline]
    #[allow(unused)]
    fn resize(&mut self, columns: usize) {
        let mut index = self.tabs.len();
        self.tabs.resize_with(columns, || {
            let is_tabstop = index % INITIAL_TABSTOPS == 0;
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

impl<U> Index<Line> for Crosswords<U> {
    type Output = Row<Square>;

    #[inline]
    fn index(&self, index: Line) -> &Row<Square> {
        &self.storage[index]
    }
}

impl<U> IndexMut<Line> for Crosswords<U> {
    #[inline]
    fn index_mut(&mut self, index: Line) -> &mut Row<Square> {
        &mut self.storage[index]
    }
}

impl<U> Index<Pos> for Crosswords<U> {
    type Output = Square;

    #[inline]
    fn index(&self, pos: Pos) -> &Square {
        &self[pos.row][pos.col]
    }
}

impl<U> IndexMut<Pos> for Crosswords<U> {
    #[inline]
    fn index_mut(&mut self, pos: Pos) -> &mut Square {
        &mut self[pos.row][pos.col]
    }
}

impl<U> Crosswords<U> {
    pub fn new(cols: usize, rows: usize, event_proxy: U) -> Crosswords<U> {
        let scroll_region = Line(0)..Line(rows as i32);

        Crosswords {
            cols,
            rows,
            storage: Storage::with_capacity(rows, cols),
            cursor: Cursor::default(),
            saved_cursor: Cursor::default(),
            active_charset: CharsetIndex::default(),
            scroll: 0,
            scroll_region,
            event_proxy,
            window_title: std::option::Option::Some(String::from("")),
            tabs: TabStops::new(cols),
            scroll_limit: 10_000,
            mode: Mode::SHOW_CURSOR
                | Mode::LINE_WRAP
                | Mode::ALTERNATE_SCROLL
                | Mode::URGENCY_HINTS,
            damage: TermDamageState::new(cols, rows),
        }
    }

    pub fn resize(&mut self, num_cols: usize, num_lines: usize) {
        let old_cols = self.columns();
        let old_lines = self.screen_lines();

        if old_cols == num_cols && old_lines == num_lines {
            println!("Term::resize dimensions unchanged");
            return;
        }

        println!("Old cols is {} and lines is {}", old_cols, old_lines);
        println!(
            "New num_cols is {} and num_lines is {}",
            num_cols, num_lines
        );

        // Move vi mode cursor with the content.
        let history_size = self.history_size();
        let mut delta = num_lines as i32 - old_lines as i32;
        let min_delta = std::cmp::min(0, num_lines as i32 - self.cursor.pos.row.0 - 1);
        delta = std::cmp::min(std::cmp::max(delta, min_delta), history_size as i32);
        // self.vi_mode_cursor.point.line += delta;

        let is_alt = self.mode.contains(Mode::ALT_SCREEN);
        self.resize_grid(!is_alt, num_lines, num_cols);
        // self.inactive_grid.resize(is_alt, num_lines, num_cols);

        // Invalidate selection and tabs only when necessary.
        if old_cols != num_cols {
            // self.selection = None;

            // Recreate tabs list.
            self.tabs.resize(num_cols);
        }
        //  else if let Some(selection) = self.selection.take() {
        //     let max_lines = cmp::max(num_lines, old_lines) as i32;
        //     let range = Line(0)..Line(max_lines);
        //     self.selection = selection.rotate(self, &range, -delta);
        // }

        // Clamp vi cursor to viewport.
        // let vi_point = self.vi_mode_cursor.point;
        let viewport_top = Line(-(self.scroll as i32));
        let viewport_bottom = viewport_top + self.bottommost_line();
        // self.vi_mode_cursor.point.line =
        // cmp::max(cmp::min(vi_point.line, viewport_bottom), viewport_top);
        // self.vi_mode_cursor.point.column = cmp::min(vi_point.column, self.last_column());

        // Reset scrolling region.
        self.scroll_region = Line(0)..Line(self.screen_lines() as i32);

        // Resize damage information.
        self.damage.resize(num_cols, num_lines);
    }

    fn resize_grid(&mut self, reflow: bool, columns: usize, lines: usize) {
        // Use empty template cell for resetting cells due to resize.
        let template = mem::take(&mut self.cursor.template);

        match self.rows.cmp(&lines) {
            Ordering::Less => self.grow_lines(lines),
            Ordering::Greater => self.storage.shrink_lines(lines),
            Ordering::Equal => (),
        }

        match self.cols.cmp(&columns) {
            Ordering::Less => self.grow_columns(reflow, columns),
            Ordering::Greater => self.shrink_columns(reflow, columns),
            Ordering::Equal => (),
        }

        // Restore template cell.
        self.cursor.template = template;
    }

    fn grow_lines(&mut self, target: usize) {
        let lines_added = target - self.rows;

        // Need to resize before updating buffer.
        self.storage.grow_visible_lines(target);
        self.rows = target;

        let history_size = self.history_size();
        let from_history = min(history_size, lines_added);

        // Move existing lines up for every line that couldn't be pulled from history.
        if from_history != lines_added {
            let delta = lines_added - from_history;
            self.scroll_up(&(Line(0)..Line(target as i32)), delta);
        }

        // Move cursor down for every line pulled from history.
        self.saved_cursor.pos.row += from_history;
        self.cursor.pos.row += from_history;

        self.scroll = self.scroll.saturating_sub(lines_added);
        self.decrease_scroll_limit(lines_added);
    }

    #[inline]
    pub fn mode(&self) -> &Mode {
        &self.mode
    }

    fn decrease_scroll_limit(&mut self, count: usize) {
        let count = min(count, self.history_size());
        if count != 0 {
            self.storage.shrink_lines(min(count, self.history_size()));
            self.scroll = min(self.scroll, self.history_size());
        }
    }

    fn shrink_columns(&mut self, reflow: bool, cols: usize) {
        self.cols = cols;

        // Remove the linewrap special case, by moving the cursor outside of the grid.
        if self.cursor.should_wrap && reflow {
            self.cursor.should_wrap = false;
            self.cursor.pos.col += 1;
        }

        let mut new_raw = Vec::with_capacity(self.storage.len());
        let mut buffered: Option<Vec<Square>> = None;

        let mut rows = self.storage.take_all();
        for (i, mut row) in rows.drain(..).enumerate().rev() {
            // Append lines left over from the previous row.
            if let Some(buffered) = buffered.take() {
                // Add a column for every cell added before the cursor, if it goes beyond the new
                // width it is then later reflown.
                let cursor_buffer_line = self.rows - self.cursor.pos.row.0 as usize - 1;
                if i == cursor_buffer_line {
                    self.cursor.pos.col += buffered.len();
                }

                row.append_front(buffered);
            }

            loop {
                // Remove all cells which require reflowing.
                let mut wrapped = match row.shrink(cols) {
                    Some(wrapped) if reflow => wrapped,
                    _ => {
                        let cursor_buffer_line =
                            self.rows - self.cursor.pos.row.0 as usize - 1;
                        if reflow && i == cursor_buffer_line && self.cursor.pos.col > cols
                        {
                            // If there are empty cells before the cursor, we assume it is explicit
                            // whitespace and need to wrap it like normal content.
                            Vec::new()
                        } else {
                            // Since it fits, just push the existing line without any reflow.
                            new_raw.push(row);
                            break;
                        }
                    }
                };

                // Insert spacer if a wide char would be wrapped into the last column.
                if row.len() >= cols
                    && row[Column(cols - 1)]
                        .flags()
                        .contains(square::Flags::WIDE_CHAR)
                {
                    let mut spacer = Square::default();
                    spacer
                        .flags_mut()
                        .insert(square::Flags::LEADING_WIDE_CHAR_SPACER);

                    let wide_char = mem::replace(&mut row[Column(cols - 1)], spacer);
                    wrapped.insert(0, wide_char);
                }

                // Remove wide char spacer before shrinking.
                let len = wrapped.len();
                if len > 0
                    && wrapped[len - 1]
                        .flags()
                        .contains(square::Flags::LEADING_WIDE_CHAR_SPACER)
                {
                    if len == 1 {
                        row[Column(cols - 1)]
                            .flags_mut()
                            .insert(square::Flags::WRAPLINE);
                        new_raw.push(row);
                        break;
                    } else {
                        // Remove the leading spacer from the end of the wrapped row.
                        wrapped[len - 2].flags_mut().insert(square::Flags::WRAPLINE);
                        wrapped.truncate(len - 1);
                    }
                }

                new_raw.push(row);

                // Set line as wrapped if cells got removed.
                if let Some(cell) = new_raw.last_mut().and_then(|r| r.last_mut()) {
                    cell.flags_mut().insert(square::Flags::WRAPLINE);
                }

                if wrapped
                    .last()
                    .map(|c| c.flags().contains(square::Flags::WRAPLINE) && i >= 1)
                    .unwrap_or(false)
                    && wrapped.len() < cols
                {
                    // Make sure previous wrap flag doesn't linger around.
                    if let Some(cell) = wrapped.last_mut() {
                        cell.flags_mut().remove(square::Flags::WRAPLINE);
                    }

                    // Add removed cells to start of next row.
                    buffered = Some(wrapped);
                    break;
                } else {
                    // Reflow cursor if a line below it is deleted.
                    let cursor_buffer_line =
                        self.rows - self.cursor.pos.row.0 as usize - 1;
                    if (i == cursor_buffer_line && self.cursor.pos.col < cols)
                        || i < cursor_buffer_line
                    {
                        self.cursor.pos.row = max(self.cursor.pos.row - 1, Line(0));
                    }

                    // Reflow the cursor if it is on this line beyond the width.
                    if i == cursor_buffer_line && self.cursor.pos.col >= cols {
                        // Since only a single new line is created, we subtract only `columns`
                        // from the cursor instead of reflowing it completely.
                        self.cursor.pos.col -= cols;
                    }

                    // Make sure new row is at least as long as new width.
                    let occ = wrapped.len();
                    if occ < cols {
                        wrapped.resize_with(cols, Square::default);
                    }
                    row = Row::from_vec(wrapped, occ);

                    if i < self.scroll {
                        // Since we added a new line, rotate up the viewport.
                        self.scroll += 1;
                    }
                }
            }
        }

        // Reverse iterator and use it as the new grid storage.
        let mut reversed: Vec<Row<Square>> = new_raw.drain(..).rev().collect();
        reversed.truncate(self.scroll_limit + self.rows);
        self.storage.replace_inner(reversed);

        // Reflow the primary cursor, or clamp it if reflow is disabled.
        if !reflow {
            self.cursor.pos.col = min(self.cursor.pos.col, Column(cols - 1));
        } else if self.cursor.pos.col == cols
            && !self.cursor_cell().flags().contains(square::Flags::WRAPLINE)
        {
            self.cursor.should_wrap = true;
            self.cursor.pos.col -= 1;
        } else {
            self.cursor.pos = self
                .cursor
                .pos
                .clone()
                .grid_clamp(self, pos::Boundary::Cursor);
        }

        // Clamp the saved cursor to the grid.
        self.saved_cursor.pos.col = min(self.saved_cursor.pos.col, Column(cols - 1));
    }

    fn grow_columns(&mut self, reflow: bool, columns: usize) {
        // Check if a row needs to be wrapped.
        let should_reflow = |row: &Row<Square>| -> bool {
            let len = Column(row.len());
            reflow
                && len.0 > 0
                && len < columns
                && row[len - 1].flags().contains(square::Flags::WRAPLINE)
        };

        self.cols = columns;

        let mut reversed: Vec<Row<Square>> = Vec::with_capacity(self.storage.len());
        let mut cursor_line_delta = 0;

        // Remove the linewrap special case, by moving the cursor outside of the grid.
        if self.cursor.should_wrap && reflow {
            self.cursor.should_wrap = false;
            self.cursor.pos.col += 1;
        }

        let mut rows = self.storage.take_all();

        for (i, mut row) in rows.drain(..).enumerate().rev() {
            // Check if reflowing should be performed.
            let last_row = match reversed.last_mut() {
                Some(last_row) if should_reflow(last_row) => last_row,
                _ => {
                    reversed.push(row);
                    continue;
                }
            };

            // Remove wrap flag before appending additional cells.
            if let Some(cell) = last_row.last_mut() {
                cell.flags_mut().remove(square::Flags::WRAPLINE);
            }

            // Remove leading spacers when reflowing wide char to the previous line.
            let mut last_len = last_row.len();
            if last_len >= 1
                && last_row[Column(last_len - 1)]
                    .flags()
                    .contains(square::Flags::LEADING_WIDE_CHAR_SPACER)
            {
                last_row.shrink(last_len - 1);
                last_len -= 1;
            }

            // Don't try to pull more cells from the next line than available.
            let mut num_wrapped = columns - last_len;
            let len = min(row.len(), num_wrapped);

            // Insert leading spacer when there's not enough room for reflowing wide char.
            let mut cells = row.front_split_off(len);
            if row[Column(len - 1)]
                .flags()
                .contains(square::Flags::WIDE_CHAR)
            {
                num_wrapped -= 1;

                let mut cells = row.front_split_off(len - 1);

                let mut spacer = Square::default();
                spacer
                    .flags_mut()
                    .insert(square::Flags::LEADING_WIDE_CHAR_SPACER);
                cells.push(spacer);

                cells
            } else {
                row.front_split_off(len)
            };

            // Add removed cells to previous row and reflow content.
            last_row.inner.append(&mut cells);

            let cursor_buffer_line = self.rows - self.cursor.pos.row.0 as usize - 1;

            if i == cursor_buffer_line && reflow {
                // Resize cursor's line and reflow the cursor if necessary.
                let mut target =
                    self.cursor
                        .pos
                        .clone()
                        .sub(self, pos::Boundary::Cursor, num_wrapped);

                // Clamp to the last column, if no content was reflown with the cursor.
                if target.col.0 == 0 && row.is_clear() {
                    self.cursor.should_wrap = true;
                    target = target.sub(self, pos::Boundary::Cursor, 1);
                }
                self.cursor.pos.col = target.col;

                // Get required cursor line changes. Since `num_wrapped` is smaller than `columns`
                // this will always be either `0` or `1`.
                let line_delta = self.cursor.pos.row - target.row;

                if line_delta != 0 && row.is_clear() {
                    continue;
                }

                cursor_line_delta += line_delta.0 as usize;
            } else if row.is_clear() {
                if i < self.scroll {
                    // Since we removed a line, rotate down the viewport.
                    self.scroll = self.scroll.saturating_sub(1);
                }

                // Rotate cursor down if content below them was pulled from history.
                if i < cursor_buffer_line {
                    self.cursor.pos.row += 1;
                }

                // Don't push line into the new buffer.
                continue;
            }

            if let Some(cell) = last_row.last_mut() {
                // Set wrap flag if next line still has cells.
                cell.flags_mut().insert(square::Flags::WRAPLINE);
            }

            reversed.push(row);
        }

        // Make sure we have at least the viewport filled.
        if reversed.len() < self.rows {
            let delta = (self.rows - reversed.len()) as i32;
            self.cursor.pos.row = max(self.cursor.pos.row - delta, Line(0));
            reversed.resize_with(self.rows, || Row::new(columns));
        }

        // Pull content down to put cursor in correct position, or move cursor up if there's no
        // more lines to delete below the cursor.
        if cursor_line_delta != 0 {
            let cursor_buffer_line = self.rows - self.cursor.pos.row.0 as usize - 1;
            let available = min(cursor_buffer_line, reversed.len() - self.rows);
            let overflow = cursor_line_delta.saturating_sub(available);
            reversed.truncate(reversed.len() + overflow - cursor_line_delta);
            self.cursor.pos.row = max(self.cursor.pos.row - overflow, Line(0));
        }

        // Reverse iterator and fill all rows that are still too short.
        let mut new_raw = Vec::with_capacity(reversed.len());
        for mut row in reversed.drain(..).rev() {
            if row.len() < columns {
                row.grow(columns);
            }
            new_raw.push(row);
        }

        self.storage.replace_inner(new_raw);

        // Clamp display offset in case lines above it got merged.
        self.scroll = min(self.scroll, self.history_size());
    }

    #[inline]
    pub fn cursor_cell(&mut self) -> &mut Square {
        let position = self.cursor.pos.clone();
        &mut self[position]
    }

    #[inline]
    pub fn wrapline(&mut self) {
        if !self.mode.contains(Mode::LINE_WRAP) {
            return;
        }

        self.cursor_cell().flags.insert(square::Flags::WRAPLINE);

        if self.cursor.pos.row + 1 >= self.scroll_region.end {
            self.linefeed();
        } else {
            // self.damage_cursor();
            self.cursor.pos.row += 1;
        }

        self.cursor.pos.col = Column(0);
        self.cursor.should_wrap = false;
        // self.damage_cursor();
    }

    #[allow(dead_code)]
    pub fn update_history(&mut self, history_size: usize) {
        let current_history_size = self.history_size();
        if current_history_size > history_size {
            self.storage
                .shrink_lines(current_history_size - history_size);
        }
        self.scroll = std::cmp::min(self.scroll, history_size);
        self.scroll_limit = history_size;
    }

    #[allow(dead_code)]
    #[inline]
    pub fn cursor(&self) -> (Column, Line) {
        (self.cursor.pos.col, self.cursor.pos.row)
    }

    // pub fn scroll_display(&mut self, scroll: Scroll) {
    //     self.scroll = match scroll {
    //         Scroll::Delta(count) => {
    //             min(max((self.scroll as i32) + count, 0) as usize, self.history_size())
    //         },
    //         Scroll::PageUp => min(self.scroll + self.rows, self.history_size()),
    //         Scroll::PageDown => self.scroll.saturating_sub(self.rows),
    //         Scroll::Top => self.history_size(),
    //         Scroll::Bottom => 0,
    //     };
    // }

    pub fn scroll_up(&mut self, region: &Range<Line>, positions: usize) {
        // When rotating the entire region with fixed lines at the top, just reset everything.
        if region.end - region.start <= positions && region.start != 0 {
            for i in (region.start.0..region.end.0).map(Line::from) {
                self.storage[i].reset(&self.cursor.template);
            }

            return;
        }

        // Update display offset when not pinned to active area.
        if self.scroll != 0 {
            self.scroll = std::cmp::min(self.scroll + positions, self.scroll_limit);
        }

        // Increase scroll limit
        let count = std::cmp::min(positions, self.scroll_limit - self.history_size());
        if count != 0 {
            self.storage.initialize(count, self.cols);
        }

        // Swap the lines fixed at the top to their target positions after rotation.
        //
        // Since we've made sure that the rotation will never rotate away the entire region, we
        // know that the position of the fixed lines before the rotation must already be
        // visible.
        //
        // We need to start from the bottom, to make sure the fixed lines aren't swapped with each
        // other.
        for i in (0..region.start.0).rev().map(Line::from) {
            self.storage.swap(i, i + positions);
        }

        // Rotate the entire line buffer upward.
        self.storage.rotate(-(positions as isize));

        // Ensure all new lines are fully cleared.
        let screen_lines = self.screen_lines();
        for i in ((screen_lines - positions)..screen_lines).map(Line::from) {
            self.storage[i].reset(&self.cursor.template);
        }

        // Swap the fixed lines at the bottom back into position.
        for i in (region.end.0..(screen_lines as i32)).rev().map(Line::from) {
            self.storage.swap(i, i - positions);
        }
    }

    pub fn history_size(&self) -> usize {
        self.total_lines().saturating_sub(self.screen_lines())
    }

    #[inline]
    pub fn scroll_up_from_origin(&mut self, origin: Line, mut lines: usize) {
        // println!("Scrolling up: origin={origin}, lines={lines}");

        lines = std::cmp::min(
            lines,
            (self.scroll_region.end - self.scroll_region.start).0 as usize,
        );

        let region = origin..self.scroll_region.end;

        // Scroll selection.
        // self.selection = self.selection.take().and_then(|s| s.rotate(self, &region, lines as i32));

        self.scroll_up(&region, lines);

        // // Scroll vi mode cursor.
        // let viewport_top = Line(-(self.grid.display_offset() as i32));
        // let top = if region.start == 0 { viewport_top } else { region.start };
        // let line = &mut self.vi_mode_cursor.pos.row;
        // if (top <= *line) && region.end > *line {
        // *line = cmp::max(*line - lines, top);
        // }
        // self.mark_fully_damaged();
    }

    #[allow(dead_code)]
    pub fn rows(&mut self) -> usize {
        self.storage.len()
    }

    pub fn cursor_square(&mut self) -> &mut Square {
        let pos = &self.cursor.pos;
        &mut self.storage[pos.row][pos.col]
    }

    pub fn write_at_cursor(&mut self, c: char) {
        let c = self.cursor.charsets[self.active_charset].map(c);
        let fg = self.cursor.template.fg;
        let bg = self.cursor.template.bg;
        let flags = self.cursor.template.flags;
        //     let extra = self.grid.cursor.template.extra.clone();

        let mut cursor_square = self.cursor_square();
        cursor_square.c = c;
        cursor_square.fg = fg;
        cursor_square.bg = bg;
        cursor_square.flags = flags;
        // cursor_cell.extra = extra;
    }

    #[allow(dead_code)]
    pub fn visible_rows_to_string(&mut self) -> String {
        let mut text = String::from("");

        for row in self.scroll_region.start.0..self.scroll_region.end.0 {
            for column in 0..self.cols {
                let square_content = &mut self[Line(row)][Column(column)];
                text.push(square_content.c);
                for c in square_content.zerowidth().into_iter().flatten() {
                    text.push(*c);
                }

                if column == (self.cols - 1) {
                    text.push('\n');
                }
            }
        }

        text
    }

    #[inline]
    pub fn visible_rows(&mut self) -> Vec<Row<Square>> {
        let mut visible_rows = vec![];
        for row in self.scroll_region.start.0..self.scroll_region.end.0 {
            visible_rows.push(self[Line(row)].to_owned());
        }

        visible_rows
    }

    pub fn swap_alt(&mut self) {
        if !self.mode.contains(Mode::ALT_SCREEN) {
            // Set alt screen cursor to the current primary screen cursor.
            // self.inactive_grid.cursor = self.grid.cursor.clone();

            // Drop information about the primary screens saved cursor.
            self.saved_cursor = self.cursor.clone();

            // Reset alternate screen contents.
            // self.inactive_grid.reset_region(..);
        }

        // mem::swap(&mut self.grid, &mut self.inactive_grid);
        self.mode ^= Mode::ALT_SCREEN;
        // self.selection = None;
        // self.mark_fully_damaged();
    }


}

impl<U> Handler for Crosswords<U> {
    #[inline]
    fn set_mode(&mut self, mode: AnsiMode) {
        match mode {
            AnsiMode::UrgencyHints => self.mode.insert(Mode::URGENCY_HINTS),
            AnsiMode::SwapScreenAndSetRestoreCursor => {
                if !self.mode.contains(Mode::ALT_SCREEN) {
                    self.swap_alt();
                }
            },
            AnsiMode::ShowCursor => self.mode.insert(Mode::SHOW_CURSOR),
            AnsiMode::CursorKeys => self.mode.insert(Mode::APP_CURSOR),
            // Mouse protocols are mutually exclusive.
            AnsiMode::ReportMouseClicks => {
                self.mode.remove(Mode::MOUSE_MODE);
                self.mode.insert(Mode::MOUSE_REPORT_CLICK);
                // self.event_proxy.send_event(Event::MouseCursorDirty);
            },
            AnsiMode::ReportCellMouseMotion => {
                self.mode.remove(Mode::MOUSE_MODE);
                self.mode.insert(Mode::MOUSE_DRAG);
                // self.event_proxy.send_event(Event::MouseCursorDirty);
            },
            AnsiMode::ReportAllMouseMotion => {
                self.mode.remove(Mode::MOUSE_MODE);
                self.mode.insert(Mode::MOUSE_MOTION);
                // self.event_proxy.send_event(Event::MouseCursorDirty);
            },
            AnsiMode::ReportFocusInOut => self.mode.insert(Mode::FOCUS_IN_OUT),
            AnsiMode::BracketedPaste => self.mode.insert(Mode::BRACKETED_PASTE),
            // Mouse encodings are mutually exclusive.
            AnsiMode::SgrMouse => {
                self.mode.remove(Mode::UTF8_MOUSE);
                self.mode.insert(Mode::SGR_MOUSE);
            },
            AnsiMode::Utf8Mouse => {
                self.mode.remove(Mode::SGR_MOUSE);
                self.mode.insert(Mode::UTF8_MOUSE);
            },
            AnsiMode::AlternateScroll => self.mode.insert(Mode::ALTERNATE_SCROLL),
            AnsiMode::LineWrap => self.mode.insert(Mode::LINE_WRAP),
            AnsiMode::LineFeedNewLine => self.mode.insert(Mode::LINE_FEED_NEW_LINE),
            AnsiMode::Origin => self.mode.insert(Mode::ORIGIN),
            AnsiMode::ColumnMode => {
                // self.deccolm(),
            }
            AnsiMode::Insert => self.mode.insert(Mode::INSERT),
            AnsiMode::BlinkingCursor => {
                // let style = self.cursor_style.get_or_insert(self.default_cursor_style);
                // style.blinking = true;
                // self.event_proxy.send_event(Event::CursorBlinkingChange);
            },
        }
    }

    #[inline]
    fn insert_blank_lines(&mut self, lines: usize) {
        println!("insert_blank_lines still unfinished");
        let origin = self.cursor.pos.row;
        if self.scroll_region.contains(&origin) {
            // self.scroll_down_relative(origin, lines);
        }
    }

    #[inline]
    fn terminal_attribute(&mut self, attr: Attr) {
        let cursor = &mut self.cursor;
        // println!("{:?}", attr);
        match attr {
            Attr::Foreground(color) => cursor.template.fg = color,
            Attr::Background(color) => cursor.template.bg = color,
            // Attr::UnderlineColor(color) => cursor.template.set_underline_color(color),
            Attr::Reset => {
                cursor.template.fg = AnsiColor::Named(NamedColor::Foreground);
                cursor.template.bg = AnsiColor::Named(NamedColor::Background);
                cursor.template.flags = square::Flags::empty();
                // cursor.template.set_underline_color(None);
            }
            Attr::Reverse => cursor.template.flags.insert(square::Flags::INVERSE),
            Attr::CancelReverse => cursor.template.flags.remove(square::Flags::INVERSE),
            Attr::Bold => cursor.template.flags.insert(square::Flags::BOLD),
            Attr::CancelBold => cursor.template.flags.remove(square::Flags::BOLD),
            Attr::Dim => cursor.template.flags.insert(square::Flags::DIM),
            Attr::CancelBoldDim => cursor
                .template
                .flags
                .remove(square::Flags::BOLD | square::Flags::DIM),
            Attr::Italic => cursor.template.flags.insert(square::Flags::ITALIC),
            Attr::CancelItalic => cursor.template.flags.remove(square::Flags::ITALIC),
            // Attr::Underline => {
            //     cursor.template.flags.remove(Flags::ALL_UNDERLINES);
            //     cursor.template.flags.insert(Flags::UNDERLINE);
            // },
            // Attr::DoubleUnderline => {
            //     cursor.template.flags.remove(Flags::ALL_UNDERLINES);
            //     cursor.template.flags.insert(Flags::DOUBLE_UNDERLINE);
            // },
            // Attr::Undercurl => {
            //     cursor.template.flags.remove(Flags::ALL_UNDERLINES);
            //     cursor.template.flags.insert(Flags::UNDERCURL);
            // },
            // Attr::DottedUnderline => {
            //     cursor.template.flags.remove(Flags::ALL_UNDERLINES);
            //     cursor.template.flags.insert(Flags::DOTTED_UNDERLINE);
            // },
            // Attr::DashedUnderline => {
            //     cursor.template.flags.remove(Flags::ALL_UNDERLINES);
            //     cursor.template.flags.insert(Flags::DASHED_UNDERLINE);
            // },
            // Attr::CancelUnderline => cursor.template.flags.remove(Flags::ALL_UNDERLINES),
            // Attr::Hidden => cursor.template.flags.insert(Flags::HIDDEN),
            // Attr::CancelHidden => cursor.template.flags.remove(Flags::HIDDEN),
            // Attr::Strike => cursor.template.flags.insert(Flags::STRIKEOUT),
            // Attr::CancelStrike => cursor.template.flags.remove(Flags::STRIKEOUT),
            _ => {
                println!("Term got unhandled attr: {:?}", attr);
            }
        }
    }

    fn set_title(&mut self, window_title: Option<String>) {
        self.window_title = window_title;

        let _title: String = match &self.window_title {
            Some(title) => title.to_string(),
            None => String::from(""),
        };
        // title
    }

    fn input(&mut self, c: char) {
        let width = match c.width() {
            Some(width) => width,
            None => return,
        };

        // Handle zero-width characters.
        if width == 0 {
            // // Get previous column.
            let mut column = self.cursor.pos.col;
            if !self.cursor.should_wrap {
                column.0 = column.saturating_sub(1);
            }

            // // Put zerowidth characters over first fullwidth character cell.
            let row = self.cursor.pos.row;
            if self[row][column]
                .flags
                .contains(square::Flags::WIDE_CHAR_SPACER)
            {
                column.0 = column.saturating_sub(1);
            }

            self[row][column].push_zerowidth(c);
            return;
        }

        if self.cursor.should_wrap {
            self.wrapline();
        }

        if width == 1 {
            self.write_at_cursor(c);
        } else {
            if self.cursor.pos.col + 1 >= self.cols {
                if self.mode.contains(Mode::LINE_WRAP) {
                    // Insert placeholder before wide char if glyph does not fit in this row.
                    self.cursor.template.flags.insert(square::Flags::LEADING_WIDE_CHAR_SPACER);
                    self.write_at_cursor(' ');
                    self.cursor.template.flags.remove(square::Flags::LEADING_WIDE_CHAR_SPACER);
                    self.wrapline();
                } else {
                    // Prevent out of bounds crash when linewrapping is disabled.
                    self.cursor.should_wrap = true;
                    return
                }
            }

            self.cursor.template.flags.insert(square::Flags::WIDE_CHAR);
            self.write_at_cursor(c);
            self.cursor.template.flags.remove(square::Flags::WIDE_CHAR);

            // Write spacer to cell following the wide glyph.
            self.cursor.pos.col += 1;
            self.cursor
                .template
                .flags
                .insert(square::Flags::WIDE_CHAR_SPACER);
            self.write_at_cursor(' ');
            self.cursor
                .template
                .flags
                .remove(square::Flags::WIDE_CHAR_SPACER);
        }

        if self.cursor.pos.col + 1 < self.cols {
            self.cursor.pos.col += 1;
        } else {
            self.cursor.should_wrap = true;
        }
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
        if self.cursor.pos.col > Column(0) {
            self.cursor.should_wrap = false;
            self.cursor.pos.col -= 1;
        }
    }

    fn linefeed(&mut self) {
        let next = self.cursor.pos.row + 1;
        if next == self.scroll_region.end {
            self.scroll_up_from_origin(self.scroll_region.start, 1);
        } else if next < self.screen_lines() {
            self.cursor.pos.row += 1;
        }
    }

    #[inline]
    fn bell(&mut self) {
        println!("[unimplemented] Bell");
    }

    #[inline]
    fn substitute(&mut self) {
        println!("[unimplemented] Substitute");
    }

    #[inline]
    fn put_tab(&mut self, mut count: u16) {
        // A tab after the last column is the same as a linebreak.
        if self.cursor.should_wrap {
            self.wrapline();
            return;
        }

        while self.cursor.pos.col < self.columns() && count != 0 {
            count -= 1;

            let c = self.cursor.charsets[self.active_charset].map('\t');
            let cell = self.cursor_square();
            if cell.c == ' ' {
                cell.c = c;
            }

            loop {
                if (self.cursor.pos.col + 1) == self.columns() {
                    break;
                }

                self.cursor.pos.col += 1;

                if self.tabs[self.cursor.pos.col] {
                    break;
                }
            }
        }
    }

    fn carriage_return(&mut self) {
        let new_col = 0;
        let row = self.cursor.pos.row.0 as usize;
        self.damage.damage_line(row, new_col, self.cursor.pos.col.0);
        self.cursor.pos.col = Column(new_col);
        self.cursor.should_wrap = false;
    }

    #[inline]
    fn clear_line(&mut self, mode: u16) {
        let cursor = &self.cursor;
        let bg = cursor.template.bg;
        let pos = &cursor.pos;
        let (left, right) = match mode {
            // Right
            0 => {
                if self.cursor.should_wrap {
                    return;
                }
                (pos.col, Column(self.columns()))
            }
            // Left
            1 => (Column(0), pos.col + 1),
            // All
            2 => (Column(0), Column(self.columns())),
            _ => todo!(),
        };

        self.damage
            .damage_line(pos.row.0 as usize, left.0, right.0 - 1);
        let position = pos.row;
        let row = &mut self[position];
        for square in &mut row[left..right] {
            // *square = bg.into();
            *square = Square::default();
        }
        // let range = self.cursor.pos.row..=self.cursor.pos.row;
        // self.selection = self.selection.take().filter(|s| !s.intersects_range(range));
    }

    #[inline]
    fn text_area_size_pixels(&mut self) {
        println!("text_area_size_pixels");
        // self.event_proxy.send_event(Event::TextAreaSizeRequest(Arc::new(move |window_size| {
        // let height = window_size.num_lines * window_size.cell_height;
        // let width = window_size.num_cols * window_size.cell_width;
        // format!("\x1b[4;{height};{width}t")
        // })));
    }

    #[inline]
    fn text_area_size_chars(&mut self) {
        let text = format!("\x1b[8;{};{}t", self.screen_lines(), self.columns());
        println!("text_area_size_chars {:?}", text);
        // self.event_proxy.send_event(Event::PtyWrite(text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::VoidListener;

    #[test]
    fn scroll_up() {
        let mut cw = Crosswords::new(1, 10, VoidListener {});
        for i in 0..10 {
            cw[Line(i)][Column(0)].c = i as u8 as char;
        }

        cw.scroll_up(&(Line(0)..Line(10)), 2);

        assert_eq!(cw[Line(0)][Column(0)].c, '\u{2}');
        assert_eq!(cw[Line(0)].occ, 1);
        assert_eq!(cw[Line(1)][Column(0)].c, '\u{3}');
        assert_eq!(cw[Line(1)].occ, 1);
        assert_eq!(cw[Line(2)][Column(0)].c, '\u{4}');
        assert_eq!(cw[Line(2)].occ, 1);
        assert_eq!(cw[Line(3)][Column(0)].c, '\u{5}');
        assert_eq!(cw[Line(3)].occ, 1);
        assert_eq!(cw[Line(4)][Column(0)].c, '\u{6}');
        assert_eq!(cw[Line(4)].occ, 1);
        assert_eq!(cw[Line(5)][Column(0)].c, '\u{7}');
        assert_eq!(cw[Line(5)].occ, 1);
        assert_eq!(cw[Line(6)][Column(0)].c, '\u{8}');
        assert_eq!(cw[Line(6)].occ, 1);
        assert_eq!(cw[Line(7)][Column(0)].c, '\u{9}');
        assert_eq!(cw[Line(7)].occ, 1);
        assert_eq!(cw[Line(8)][Column(0)].c, ' '); // was 0.
        assert_eq!(cw[Line(8)].occ, 0);
        assert_eq!(cw[Line(9)][Column(0)].c, ' '); // was 1.
        assert_eq!(cw[Line(9)].occ, 0);
    }

    #[test]
    fn resize_shrink_column() {
        // 5 columns and 3 lines
        let mut cw = Crosswords::new(5, 3, VoidListener {});

        cw[Line(0)][Column(0)].c = 'f';
        cw[Line(0)][Column(1)].c = 'i';
        cw[Line(0)][Column(2)].c = 'r';
        cw[Line(0)][Column(3)].c = 's';
        cw[Line(0)][Column(4)].c = 't';

        cw[Line(1)][Column(0)].c = ' ';
        cw[Line(1)][Column(1)].c = '~';
        cw[Line(1)][Column(2)].c = ' ';
        cw[Line(1)][Column(3)].c = '1';
        cw[Line(1)][Column(4)].c = ' ';

        // Before:
        // |first| <- visible
        // | ~ 1 | <- visible
        cw.resize(4, 3);
        // After:
        // |firs|
        // |t   | <- visible
        // | ~ 1| <- visible
        assert_eq!(cw[Line(0)][Column(0)].c, 't');
        assert_eq!(cw[Line(0)][Column(1)].c, ' ');
        assert_eq!(cw[Line(0)][Column(2)].c, ' ');
        assert_eq!(cw[Line(0)][Column(3)].c, ' ');

        assert_eq!(cw[Line(1)][Column(0)].c, ' ');
        assert_eq!(cw[Line(1)][Column(1)].c, '~');
        assert_eq!(cw[Line(1)][Column(2)].c, ' ');
        assert_eq!(cw[Line(1)][Column(3)].c, '1');

        // 3 columns and 2 lines (max lines should increase)
        let mut cw = Crosswords::new(3, 2, VoidListener {});

        cw[Line(0)][Column(0)].c = 'a';
        cw[Line(0)][Column(1)].c = 'b';
        cw[Line(0)][Column(2)].c = 'c';

        cw[Line(1)][Column(0)].c = 'd';
        cw[Line(1)][Column(1)].c = 'e';
        cw[Line(1)][Column(2)].c = 'f';

        // Before:
        // |abc| <- visible
        // |def| <- visible
        cw.resize(2, 2);
        // After:
        // |ab|
        // |c |
        // |de| <- visible
        // |f | <- visible
        assert_eq!(cw[Line(0)][Column(0)].c, 'd');
        assert_eq!(cw[Line(0)][Column(1)].c, 'e');
        assert_eq!(cw[Line(1)][Column(0)].c, 'f');
        assert_eq!(cw[Line(1)][Column(1)].c, ' ');

        // 3 columns and 2 lines (max lines should increase)
        let mut cw = Crosswords::new(3, 10, VoidListener {});

        cw[Line(0)][Column(0)].c = '1';
        cw[Line(0)][Column(1)].c = '2';
        cw[Line(0)][Column(2)].c = '2';
        cw[Line(1)][Column(0)].c = '3';
        cw[Line(1)][Column(1)].c = '4';
        cw[Line(0)][Column(2)].c = '2';
        cw[Line(2)][Column(0)].c = ' ';
        cw[Line(2)][Column(1)].c = ' ';

        // Before:
        // |123| <- visible
        // |456| <- visible
        // |   | <- visible
        // ...
        cw.resize(2, 10);
        // After:
        // |12| <- visible
        // |3 | <- visible
        // |45| <- visible
        // |6 | <- visible
        // |  | <- visible
        // ...
        assert_eq!(cw[Line(0)][Column(0)].c, '1');
        assert_eq!(cw[Line(0)][Column(1)].c, '2');
        assert_eq!(cw[Line(1)][Column(0)].c, '3');
        assert_eq!(cw[Line(1)][Column(1)].c, ' ');
        assert_eq!(cw[Line(2)][Column(0)].c, '4');
        assert_eq!(cw[Line(2)][Column(1)].c, '5');
        assert_eq!(cw[Line(3)][Column(0)].c, '6');
        assert_eq!(cw[Line(3)][Column(1)].c, ' ');
        assert_eq!(cw[Line(4)][Column(0)].c, ' ');
        assert_eq!(cw[Line(4)][Column(1)].c, ' ');
    }

    #[test]
    fn test_linefeed() {
        let mut cw: Crosswords<VoidListener> = Crosswords::new(1, 1, VoidListener {});
        assert_eq!(cw.rows(), 1);

        cw.linefeed();
        assert_eq!(cw.rows(), 2);
    }

    #[test]
    fn test_linefeed_moving_cursor() {
        let mut cw: Crosswords<VoidListener> = Crosswords::new(1, 3, VoidListener {});
        let (col, row) = cw.cursor();
        assert_eq!(col, 0);
        assert_eq!(row, 0);

        cw.linefeed();
        let (col, row) = cw.cursor();
        assert_eq!(col, 0);
        assert_eq!(row, 1);

        // Keep adding lines but keep cursor at max row
        for _ in 0..20 {
            cw.linefeed();
        }
        let (col, row) = cw.cursor();
        assert_eq!(col, 0);
        assert_eq!(row, 2);
        assert_eq!(cw.rows(), 22);
    }

    #[test]
    fn test_input() {
        let columns: usize = 5;
        let rows: usize = 10;
        let mut cw: Crosswords<VoidListener> =
            Crosswords::new(columns, rows, VoidListener {});
        for i in 0..4 {
            cw[Line(0)][Column(i)].c = i as u8 as char;
        }
        cw[Line(1)][Column(3)].c = 'b';

        assert_eq!(cw[Line(0)][Column(0)].c, '\u{0}');
        assert_eq!(cw[Line(0)][Column(1)].c, '\u{1}');
        assert_eq!(cw[Line(0)][Column(2)].c, '\u{2}');
        assert_eq!(cw[Line(0)][Column(3)].c, '\u{3}');
        assert_eq!(cw[Line(0)][Column(4)].c, ' ');
        assert_eq!(cw[Line(1)][Column(2)].c, ' ');
        assert_eq!(cw[Line(1)][Column(3)].c, 'b');
        assert_eq!(cw[Line(0)][Column(4)].c, ' ');
    }
}
