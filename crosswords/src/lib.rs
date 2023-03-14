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
    start: usize,
    end: usize,
}

#[derive(Debug, Clone)]
pub struct Crosswords {
    rows: usize,
    cols: usize,
    raw: Storage<Square>,
    cursor: Cursor<Square>,
    scroll: usize,
    mode: Mode,
    active_charset: CharsetIndex,
    scroll_region: ScrollRegion,
}

impl Index<Line> for Crosswords {
    type Output = Row<Square>;

    #[inline]
    fn index(&self, index: Line) -> &Row<Square> {
        &self.raw[index]
    }
}

impl IndexMut<Line> for Crosswords {
    #[inline]
    fn index_mut(&mut self, index: Line) -> &mut Row<Square> {
        &mut self.raw[index]
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
            raw: Storage::with_capacity(rows, cols),
            cursor: Cursor::default(),
            active_charset: CharsetIndex::default(),
            scroll: 0,
            scroll_region: ScrollRegion {
                start: 0,
                end: rows,
            },
            mode: Mode::SHOW_CURSOR
                | Mode::LINE_WRAP
                | Mode::ALTERNATE_SCROLL
                | Mode::URGENCY_HINTS,
        }
    }

    /// Move lines at the bottom toward the top.
    ///
    /// This is the performance-sensitive part of scrolling.
    pub fn scroll_up(&mut self, region: &Range<Line>, positions: usize) {
        // When rotating the entire region with fixed lines at the top, just reset everything.
        // if region.end - region.start <= positions && region.start != 0 {
        // for i in (region.start.0..region.end.0).map(Line::from) {
        // self.raw[i].reset(&self.cursor.template);
        // }

        // return;
        // }

        // Update display offset when not pinned to active area.
        if self.scroll != 0 {
            // TODO: update to proper limit instead of usize::MAX
            self.scroll = std::cmp::min(self.scroll + positions, usize::MAX);
        }

        // Create scrollback for the new lines.
        let count = std::cmp::min(positions, usize::MAX - self.history_size());
        if count != 0 {
            self.raw.initialize(count, self.cols);
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
            self.raw.swap(i, i + positions);
        }

        // Rotate the entire line buffer upward.
        self.raw.rotate(-(positions as isize));

        // Ensure all new lines are fully cleared.
        let screen_lines = self.rows();
        // for i in ((screen_lines - positions)..screen_lines).map(Line::from) {
        // self.raw[i].reset(&self.cursor.template);
        // }

        // Swap the fixed lines at the bottom back into position.
        for i in (region.end.0..(screen_lines as i32)).rev().map(Line::from) {
            self.raw.swap(i, i - positions);
        }
    }

    fn history_size(&self) -> usize {
        self.total_lines().saturating_sub(self.screen_lines())
    }

    #[inline]
    fn scroll_up_per_line(&mut self, mut lines: usize) {
        let origin = Line(self.rows.try_into().unwrap());

        println!("Scrolling up relative: origin={origin}, lines={lines}");

        lines = std::cmp::min(lines, self.scroll_region.end - self.scroll_region.start);

        let region = origin..lines.into();

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
        self.raw.len()
    }

    fn cursor_square(&mut self) -> &mut Square {
        let pos = &self.cursor.pos;
        &mut self.raw[pos.row][pos.col]
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
        } else {
            if self.cursor.pos.col == self.cols {
                // Place cursor to beginning if hits the max of cols
                self.cursor.pos.row += 1;
                self.cursor.pos.col = pos::Column(0);
            }

            let next_row = self.cursor.pos.row;
            let next_col = self.cursor.pos.col;
            self[next_row][next_col].c = '█';
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

        // if self.cursor.pos.col + 1 >= self.scroll_region.end {
        self.linefeed();
        // } else {
        // self.damage_cursor();
        // self.cursor.point.line += 1;
        // }

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
        // if self.cursor_square().c == '█' {
        // self[row][col].p(' ');
        // }

        // Break line and put cursor in the front
        self.cursor.pos.row += 1;
        self.cursor.pos.col = pos::Column(0);

        // println!(">>>>> linefeed");

        if self.cursor.pos.row >= self.cols {
            self.scroll_up_per_line(1);
        }
    }

    // #[inline]
    // fn damage_row(&mut self, line: usize, left: usize, right: usize) {
    // self.raw[line.into()].expand(left, right);
    // }

    pub fn carriage_return(&mut self) {
        println!("Carriage return");
        let new_col = 0;
        // let row = self.cursor.pos.row.0 as usize;
        // self.damage_row(row, new_col, self.cursor.pos.col.0);
        self.cursor.pos.col = Column(new_col);
        self.cursor.should_wrap = false;
    }

    pub fn visible_rows_to_string(&mut self) -> String {
        let mut text = String::from("");

        for row in 0..24 {
            for colums in 0..self.cols {
                let square_content = &mut self[Line(row)][Column(colums)];
                text.push(square_content.c);
                for c in square_content.zerowidth().into_iter().flatten() {
                    text.push(*c);
                }

                if colums == (self.cols - 1) {
                    text.push('\n');
                }
            }
        }

        text
    }

    // pub fn to_arr_u8(&mut self, line: Line) -> Row<Square> {
    //     self.raw[line]
    // }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore]
    #[test]
    fn test_linefeed() {
        let mut cw: Crosswords = Crosswords::new(1, 1);
        assert_eq!(cw.rows(), 1);

        cw.linefeed();
        assert_eq!(cw.rows(), 2);
    }

    #[test]
    fn test_input() {
        let columns: usize = 5;
        let rows: usize = 10;
        let mut cw: Crosswords = Crosswords::new(columns, rows);
        for i in 0..4 {
            println!("{i:?}");
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
