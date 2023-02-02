/*
    Crosswords -> Rio's grid manager

    |----------------------------------|
    |-$-bash:-echo-1-------------------|
    |-1--------------------------------|
    |----------------------------------|
    |----------------------------------|
    |----------------------------------|
*/

pub mod cell;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CursorLocation {
    row: u16,
    col: u16
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Cursor {
    pub location: CursorLocation,

    // Template cell when using this cursor.
    // pub template: T,
}

// impl<T: std::default::Default> Default for Cursor {
//     // #000000 Color Hex Black #000
//     fn default() -> Self {
//         Self {
//             location: CursorLocation {
//              row: 0,
//              col: 0,
//             },
//             template: T::default()
//         }
//     }
// }

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Grid {
    rows: u16,
    cols: u16,
    cursor: Cursor,
    // scroll:  
}

impl Grid {
    pub fn new() -> Grid {
        Grid {
            cols: 80,
            rows: 25,
            cursor: Cursor::default()
        }
    }

    pub fn input(&mut self, c: char) {
        // let row = self.grid.cursor.point.row ;
        // let col = self.grid.cursor.point.col;

        // Calculate if can be render in the row, otherwise break to next col
        // self[row][col].push_zerowidth(c);

        self.cursor.location.col += 1;
    }

    pub fn feedline(&mut self, c: char) {
        self.cursor.location.row += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feedline() {
        // let mut grid: Grid<Cell> = Grid::new(1, 5, 0);
        // for i in 0..5 {
        //     grid[Line(0)][Column(i)].c = 'a';
        // }
        // grid[Line(0)][Column(0)].c = '"';
        // grid[Line(0)][Column(3)].c = '"';        

        // assert_eq!(invalid_character_color, "Error: Character is not valid");
    }
}
