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
use crate::storage::Storage;
use pos::{Column, Cursor, Line, Pos};
use square::Square;
use std::ops::{Index, IndexMut};

// impl<T: Default> Default for Cursor<T> {
//     // #000000 Color Hex Black #000
//     fn default() -> Self {
//         Self {
//             pos: Pos {
//              row: 0,
//              col: 0,
//             },
//             template: T::default()
//         }
//     }
// }

#[derive(Debug, Clone)]
pub struct Crosswords<T> {
    rows: usize,
    cols: usize,
    raw: Storage<T>,
    cursor: Cursor<T>,
    // scroll:
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

impl<T: Default + PartialEq + Clone> Crosswords<T> {
    pub fn new(rows: usize, cols: usize) -> Crosswords<T> {
        Crosswords::<T> {
            cols,
            rows,
            raw: Storage::with_capacity(rows, cols),
            cursor: Cursor::default(),
        }
    }

    pub fn lines(&mut self) -> usize {
        self.raw.len()
    }

    pub fn input(&mut self, c: char) {
        // let row = self.grid.cursor.point.row ;
        // let col = self.grid.cursor.point.col;

        // Calculate if can be render in the row, otherwise break to next col
        // self[row][col].push_zerowidth(c);

        self.cursor.pos.col += 1;
    }

    pub fn feedline(&mut self, c: char) {
        self.cursor.pos.row += 1;
    }

    // pub fn to_arr_u8(&mut self, row: Line) -> Row<T> {
    // self.raw[row]
    // }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feedline() {
        let mut cw: Crosswords<Square> = Crosswords::new(1, 3);
        assert_eq!(cw.lines(), 1);

        cw.feedline('"');
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
