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

pub mod dimensions;
pub mod pos;
pub mod row;
pub mod square;
pub mod storage;

use crate::dimensions::Dimensions;
use crate::pos::CharsetIndex;
use crate::row::Row;
use crate::square::Square;
use crate::storage::Storage;
use bitflags::bitflags;
use pos::{Column, Cursor, Line, Pos};
use std::ops::{Index, IndexMut, Range};
use std::ptr;
use unicode_width::UnicodeWidthChar;

bitflags! {
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

#[derive(Debug, Clone)]
struct ScrollRegion {
    start: Line,
    end: Line,
}

#[derive(Debug, Clone)]
pub struct Crosswords {
    active_charset: CharsetIndex,
    cols: usize,
    cursor: Cursor<Square>,
    mode: Mode,
    rows: usize,
    scroll: usize,
    scroll_limit: usize,
    scroll_region: ScrollRegion,
    storage: Storage<Square>,
    tabs: TabStops,
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

impl Index<Line> for Crosswords {
    type Output = Row<Square>;

    #[inline]
    fn index(&self, index: Line) -> &Row<Square> {
        &self.storage[index]
    }
}

impl IndexMut<Line> for Crosswords {
    #[inline]
    fn index_mut(&mut self, index: Line) -> &mut Row<Square> {
        &mut self.storage[index]
    }
}

impl Index<Pos> for Crosswords {
    type Output = Square;

    #[inline]
    fn index(&self, pos: Pos) -> &Square {
        &self[pos.row][pos.col]
    }
}

impl IndexMut<Pos> for Crosswords {
    #[inline]
    fn index_mut(&mut self, pos: Pos) -> &mut Square {
        &mut self[pos.row][pos.col]
    }
}

impl Crosswords {
    pub fn new(cols: usize, rows: usize) -> Crosswords {
        Crosswords {
            cols,
            rows,
            storage: Storage::with_capacity(rows, cols),
            cursor: Cursor::default(),
            active_charset: CharsetIndex::default(),
            scroll: 0,
            scroll_region: ScrollRegion {
                start: pos::Line(0),
                end: pos::Line(rows.try_into().unwrap()),
            },
            tabs: TabStops::new(cols),
            scroll_limit: 10_000,
            mode: Mode::SHOW_CURSOR
                | Mode::LINE_WRAP
                | Mode::ALTERNATE_SCROLL
                | Mode::URGENCY_HINTS,
        }
    }

    // pub fn scroll_display(&mut self, scroll: Scroll) {
    //     self.display_offset = match scroll {
    //         Scroll::Delta(count) => {
    //             min(max((self.display_offset as i32) + count, 0) as usize, self.history_size())
    //         },
    //         Scroll::PageUp => min(self.display_offset + self.lines, self.history_size()),
    //         Scroll::PageDown => self.display_offset.saturating_sub(self.lines),
    //         Scroll::Top => self.history_size(),
    //         Scroll::Bottom => 0,
    //     };
    // }

    pub fn update_history(&mut self, history_size: usize) {
        let current_history_size = self.history_size();
        if current_history_size > history_size {
            self.storage
                .shrink_lines(current_history_size - history_size);
        }
        self.scroll = std::cmp::min(self.scroll, history_size);
        self.scroll_limit = history_size;
    }

    #[inline]
    pub fn cursor(&self) -> (Column, Line) {
        (self.cursor.pos.col, self.cursor.pos.row)
    }

    /// Move lines at the bottom toward the top.
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

    fn history_size(&self) -> usize {
        self.total_lines().saturating_sub(self.screen_lines())
    }

    /// Text moves up; clear at top
    #[inline]
    fn scroll_up_from_origin(&mut self, origin: Line, mut lines: usize) {
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
        // let line = &mut self.vi_mode_cursor.point.line;
        // if (top <= *line) && region.end > *line {
        // *line = cmp::max(*line - lines, top);
        // }
        // self.mark_fully_damaged();
    }

    pub fn rows(&mut self) -> usize {
        self.storage.len()
    }

    fn cursor_square(&mut self) -> &mut Square {
        let pos = &self.cursor.pos;
        &mut self.storage[pos.row][pos.col]
    }

    fn write_at_cursor(&mut self, c: char) {
        let c = self.cursor.charsets[self.active_charset].map(c);
        //     let fg = self.grid.cursor.template.fg;
        //     let bg = self.grid.cursor.template.bg;
        //     let flags = self.grid.cursor.template.flags;
        //     let extra = self.grid.cursor.template.extra.clone();

        let mut cursor_square = self.cursor_square();
        cursor_square.c = c;
        // cursor_cell.fg = fg;
        // cursor_cell.bg = bg;
        // cursor_cell.flags = flags;
        // cursor_cell.extra = extra;
    }

    pub fn input(&mut self, c: char) {
        let width = match c.width() {
            Some(width) => width,
            None => return,
        };

        let row = self.cursor.pos.row;

        // Handle zero-width characters.
        if width == 0 {
            // // Get previous column.
            let mut column = self.cursor.pos.col;
            if !self.cursor.should_wrap {
                column.0 = column.saturating_sub(1);
            }

            // // Put zerowidth characters over first fullwidth character cell.
            // let row = self.cursor.pos.row;
            // if self[row][column].flags.contains(Flags::WIDE_CHAR_SPACER) {
            //     column.0 = column.saturating_sub(1);
            // }

            self[row][column].push_zerowidth(c);
            return;
        }

        if self.cursor.should_wrap {
            self.wrapline();
        }

        if width == 1 {
            self.write_at_cursor(c);
        } else if self.cursor.pos.col == self.cols {
            // Place cursor to beginning if hits the max of cols
            self.cursor.pos.row += 1;
            self.cursor.pos.col = pos::Column(0);
        }

        if self.cursor.pos.col + 1 < self.cols {
            self.cursor.pos.col += 1;
        } else {
            self.cursor.should_wrap = true;
        }
    }

    #[inline]
    fn wrapline(&mut self) {
        if !self.mode.contains(Mode::LINE_WRAP) {
            return;
        }

        // self.cursor_cell().flags.insert(Flags::WRAPLINE);

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

    #[inline]
    pub fn backspace(&mut self) {
        if self.cursor.pos.col > Column(0) {
            self.cursor.should_wrap = false;
            self.cursor.pos.col -= 1;
        }
    }

    pub fn linefeed(&mut self) {
        let next = self.cursor.pos.row + 1;
        if next == self.scroll_region.end {
            self.scroll_up_from_origin(self.scroll_region.start, 1);
        } else if next < self.screen_lines() {
            self.cursor.pos.row += 1;
        }
    }

    #[inline]
    pub fn bell(&mut self) {
        println!("[unimplemented] Bell");
    }

    #[inline]
    pub fn substitute(&mut self) {
        println!("[unimplemented] Substitute");
    }

    #[inline]
    pub fn put_tab(&mut self, mut count: u16) {
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

    // #[inline]
    // fn damage_row(&mut self, line: usize, left: usize, right: usize) {
    //     self.storage[line.into()].expand(left, right);
    // }

    pub fn carriage_return(&mut self) {
        let new_col = 0;
        // let row = self.cursor.pos.row.0 as usize;
        // self.damage_row(row, new_col, self.cursor.pos.col.0);
        self.cursor.pos.col = Column(new_col);
        self.cursor.should_wrap = false;
    }

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
    pub fn clear_line(&mut self, mode: u16) {
        let cursor = &self.cursor;
        let _bg = cursor.template.bg;
        let pos = &cursor.pos;
        let (_left, _right) = match mode {
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

        // self.damage.damage_line(point.line.0 as usize, left.0, right.0 - 1);
        // let row = &mut self[pos.row];
        // for cell in &mut row[left..right] {
        // *cell = bg.into();
        // }
        // let range = self.cursor.pos.row..=self.cursor.pos.row;
        // self.selection = self.selection.take().filter(|s| !s.intersects_range(range));
    }

    // pub fn to_arr_u8(&mut self, line: Line) -> Row<Square> {
    //     self.storage[line]
    // }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_up() {
        let mut cw = Crosswords::new(1, 10);
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
    fn test_linefeed() {
        let mut cw: Crosswords = Crosswords::new(1, 1);
        assert_eq!(cw.rows(), 1);

        cw.linefeed();
        assert_eq!(cw.rows(), 2);
    }

    #[test]
    fn test_linefeed_moving_cursor() {
        let mut cw: Crosswords = Crosswords::new(1, 3);
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
        let mut cw: Crosswords = Crosswords::new(columns, rows);
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
