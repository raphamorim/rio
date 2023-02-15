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

use crate::row::Row;
use crate::square::ResetDiscriminant;
use crate::square::Square;
use crate::storage::Storage;
use pos::{Column, Cursor, Line, Pos};
use std::cmp::min;
use std::ops::{Index, IndexMut, Range};
use std::{cmp, mem, ptr, slice, str};

#[derive(Debug, Clone)]
pub struct Crosswords<T> {
    rows: usize,
    cols: usize,
    raw: Storage<T>,
    cursor: Cursor<T>,
    scroll: usize,
}

impl<T> Index<Line> for Crosswords<T> {
    type Output = Row<T>;

    #[inline]
    fn index(&self, index: Line) -> &Row<T> {
        &self.raw[index]
    }
}

impl<T> IndexMut<Line> for Crosswords<T> {
    #[inline]
    fn index_mut(&mut self, index: Line) -> &mut Row<T> {
        &mut self.raw[index]
    }
}

impl<T> Index<Pos> for Crosswords<T> {
    type Output = T;

    #[inline]
    fn index(&self, pos: Pos) -> &T {
        &self[pos.row][pos.col]
    }
}

impl<T> IndexMut<Pos> for Crosswords<T> {
    #[inline]
    fn index_mut(&mut self, pos: Pos) -> &mut T {
        &mut self[pos.row][pos.col]
    }
}

pub trait CrosswordsSquare: Sized {
    /// Check if the cell contains any content.
    fn is_empty(&self) -> bool;

    /// Perform an opinionated cell reset based on a template cell.
    fn reset(&mut self, template: &Self);

    fn set_char(&mut self, character: char);

    fn get_char(&mut self) -> char;
}

impl<T: CrosswordsSquare + Default + PartialEq + Clone> Crosswords<T> {
    pub fn new(cols: usize, rows: usize) -> Crosswords<T> {
        Crosswords::<T> {
            cols,
            rows,
            raw: Storage::with_capacity(rows, cols),
            cursor: Cursor::default(),
            scroll: 0,
        }
    }

    /// Move lines at the bottom toward the top.
    ///
    /// This is the performance-sensitive part of scrolling.
    pub fn scroll_up<D>(&mut self, region: &Range<Line>, positions: usize)
    where
        T: ResetDiscriminant<D>,
        D: PartialEq,
    {
        // When rotating the entire region with fixed lines at the top, just reset everything.
        if region.end - region.start <= positions && region.start != 0 {
            for i in (region.start.0..region.end.0).map(Line::from) {
                // self.raw[i].reset(&self.cursor.template);
            }

            return;
        }

        // Update display offset when not pinned to active area.
        if self.scroll != 0 {
            // self.scroll = min(self.scroll + positions, self.max_scroll_limit);
        }

        // Create scrollback for the new lines.
        // self.increase_scroll_limit(positions);

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
        // let screen_lines = self.screen_lines();
        // for i in ((screen_lines - positions)..screen_lines).map(Line::from) {
        //     self.raw[i].reset(&self.cursor.template);
        // }

        // Swap the fixed lines at the bottom back into position.
        // for i in (region.end.0..(screen_lines as i32)).rev().map(Line::from) {
        //     self.raw.swap(i, i - positions);
        // }
    }

    #[inline]
    fn scroll_up_per_line(&mut self, mut lines: usize) {
        let origin = Line(self.rows.try_into().unwrap());

        println!("Scrolling up relative: origin={}, lines={}", origin, lines);

        // lines = cmp::min(lines, (self.scroll_region.end - self.scroll_region.start).0 as usize);

        let region = origin..lines.into();

        // Scroll selection.
        // self.selection = self.selection.take().and_then(|s| s.rotate(self, &region, lines as i32));

        // self.scroll_up(&region, lines);

        // // Scroll vi mode cursor.
        // let viewport_top = Line(-(self.grid.display_offset() as i32));
        // let top = if region.start == 0 { viewport_top } else { region.start };
        // let line = &mut self.vi_mode_cursor.point.line;
        // if (top <= *line) && region.end > *line {
        // *line = cmp::max(*line - lines, top);
        // }
        // self.mark_fully_damaged();
    }

    pub fn lines(&mut self) -> usize {
        self.raw.len()
    }

    // fn write_at_cursor(&mut self, c: char) {
    //     let c = self.grid.cursor.charsets[self.active_charset].map(c);
    //     let fg = self.grid.cursor.template.fg;
    //     let bg = self.grid.cursor.template.bg;
    //     let flags = self.grid.cursor.template.flags;
    //     let extra = self.grid.cursor.template.extra.clone();

    //     let mut cursor_cell = self.grid.cursor_cell();

    //     // Clear all related cells when overwriting a fullwidth cell.
    //     if cursor_cell.flags.intersects(Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER) {
    //         // Remove wide char and spacer.
    //         let wide = cursor_cell.flags.contains(Flags::WIDE_CHAR);
    //         let point = self.grid.cursor.point;
    //         if wide && point.column < self.last_column() {
    //             self.grid[point.line][point.column + 1].flags.remove(Flags::WIDE_CHAR_SPACER);
    //         } else if point.column > 0 {
    //             self.grid[point.line][point.column - 1].clear_wide();
    //         }

    //         // Remove leading spacers.
    //         if point.column <= 1 && point.line != self.topmost_line() {
    //             let column = self.last_column();
    //             self.grid[point.line - 1i32][column].flags.remove(Flags::LEADING_WIDE_CHAR_SPACER);
    //         }

    //         cursor_cell = self.grid.cursor_cell();
    //     }

    //     cursor_cell.c = c;
    //     cursor_cell.fg = fg;
    //     cursor_cell.bg = bg;
    //     cursor_cell.flags = flags;
    //     cursor_cell.extra = extra;
    // }

    pub fn input(&mut self, c: char) {
        // let width = match c.width() {
        //     Some(width) => width,
        //     None => return,
        // };

        // if width == 1 {
        //     self.write_at_cursor(c);
        // }

        let row = self.cursor.pos.row;
        let col = self.cursor.pos.col;
        let square = &self[row][col];

        // if square.is_empty() {
        self[row][col].set_char(c);
        self.cursor.pos.col += 1;

        println!("{:?} {:?} {:?}", row, col, c);

        if self.cursor.pos.col == self.cols {
            // Place cursor to beginning if hits the max of cols
            self.cursor.pos.row += 1;
            self.cursor.pos.col = pos::Column(0);
        }

        let next_row = self.cursor.pos.row;
        let next_col = self.cursor.pos.col;
        self[next_row][next_col].set_char('█');
        // }

        // Calculate if can be render in the row, otherwise break to next col
        // self[row][col].push_zerowidth(c);
        // self[row][col].c = c;
    }

    #[inline]
    pub fn backspace(&mut self) {
        // let row = self.cursor.pos.row;
        // let col = self.cursor.pos.col;
        // let square = &mut self[row][col];
        // self[row][col].set_char(' ');

        if self.cursor.pos.col > Column(0) {
            let row = self.cursor.pos.row;
            let col = self.cursor.pos.col;
            let square = &mut self[row][col];
            square.set_char(' ');
            self.cursor.pos.col -= 1;
        }
    }

    pub fn feedline(&mut self) {
        let row = self.cursor.pos.row;
        let col = self.cursor.pos.col;
        let square = &mut self[row][col];
        if square.get_char() == '█' {
            self[row][col].set_char(' ');
        }

        // Break line and put cursor in the front
        self.cursor.pos.row += 1;
        self.cursor.pos.col = pos::Column(0);

        // if self.cursor.pos.row >= self.cols {
        //     self.scroll_up_per_line(1);
        // }
    }

    pub fn to_string(&mut self) -> String {
        let mut text = String::from("");

        for row in 0..24 {
            for colums in 0..self.cols {
                let s = &mut self[Line(row)][Column(colums)];
                // text.push(s.get_char());
                // text.push(row_squares.c);
                // for c in row_squares.zerowidth().into_iter().flatten() {
                //     text.push(*c);
                // }

                // let square = &mut self[Line(row)][Column(colums)];
                // if !s.is_empty() {
                text.push(s.get_char());
                // }

                if colums == (self.cols - 1) {
                    text.push('\n');
                }
            }
        }

        println!("{:?}", text);

        // string.push('█');
        text
    }

    // fn line_to_string(
    //     &self,
    //     line: Line,
    //     mut cols: Range<Column>,
    //     include_wrapped_wide: bool,
    // ) -> String {
    //     let mut text = String::new();

    //     let grid_line = &self.grid[line];
    //     let line_length = cmp::min(grid_line.line_length(), cols.end + 1);

    //     // Include wide char when trailing spacer is selected.
    //     if grid_line[cols.start].flags.contains(Flags::WIDE_CHAR_SPACER) {
    //         cols.start -= 1;
    //     }

    //     let mut tab_mode = false;
    //     for column in (cols.start.0..line_length.0).map(Column::from) {
    //         let cell = &grid_line[column];

    //         // Skip over cells until next tab-stop once a tab was found.
    //         if tab_mode {
    //             if self.tabs[column] || cell.c != ' ' {
    //                 tab_mode = false;
    //             } else {
    //                 continue;
    //             }
    //         }

    //         if cell.c == '\t' {
    //             tab_mode = true;
    //         }

    //         if !cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
    //             // Push cells primary character.
    //             text.push(cell.c);

    //             // Push zero-width characters.
    //             for c in cell.zerowidth().into_iter().flatten() {
    //                 text.push(*c);
    //             }
    //         }
    //     }

    //     if cols.end >= self.columns() - 1
    //         && (line_length.0 == 0
    //             || !self.grid[line][line_length - 1].flags.contains(Flags::WRAPLINE))
    //     {
    //         text.push('\n');
    //     }

    //     // If wide char is not part of the selection, but leading spacer is, include it.
    //     if line_length == self.columns()
    //         && line_length.0 >= 2
    //         && grid_line[line_length - 1].flags.contains(Flags::LEADING_WIDE_CHAR_SPACER)
    //         && include_wrapped_wide
    //     {
    //         text.push(self.grid[line - 1i32][Column(0)].c);
    //     }

    //     text
    // }

    // pub fn feedline(&mut self, _c: char) {
    //     self.cursor.pos.row += 1;
    // }

    // pub fn to_arr_u8(&mut self, row: Line) -> Row<T> {
    // self.raw[row]
    // }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::square::Square;

    #[test]
    fn test_feedline() {
        let mut cw: Crosswords<Square> = Crosswords::new(1, 3);
        assert_eq!(cw.lines(), 1);

        cw.feedline();
        // assert_eq!(cw.lines(), 2);
    }

    #[ignore]
    #[test]
    fn test_input() {
        let mut cw: Crosswords<Square> = Crosswords::new(1, 5);
        // println!("{:?}", cw);
        for i in 0..5 {
            cw[Line(0)][Column(i)].c = 'a';
        }
        // grid[Pos { row: 0, col: 0 }].c = '"';
        cw[Line(0)][Column(3)].c = '"';

        // println!("{:?}", cw[Line(0)][Column(1)]);
        // println!("{:?}", cw[Line(0)]);
        // println!("{:?}", cw.to_arr_u8(Line(0)));

        assert_eq!("1", "Error: Character is not valid");
    }
}
