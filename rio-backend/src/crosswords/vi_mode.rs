// vi_mode.rs was originally from Alacritty https://github.com/alacritty/alacritty/blob/e35e5ad14fce8456afdd89f2b392b9924bb27471/alacritty_terminal/src/vi_mode.rs
// which is licensed under Apache 2.0 license.

use std::cmp::min;

use crate::crosswords::grid::{Dimensions, GridSquare};
use crate::crosswords::pos::{Boundary, Column, Direction, Line, Pos, Side};
use crate::crosswords::square::Flags;
use crate::crosswords::Crosswords;
use crate::event::EventListener;

/// Possible vi mode motion movements.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ViMotion {
    /// Move up.
    Up,
    /// Move down.
    Down,
    /// Move left.
    Left,
    /// Move right.
    Right,
    /// Move to start of line.
    First,
    /// Move to end of line.
    Last,
    /// Move to the first non-empty cell.
    FirstOccupied,
    /// Move to top of screen.
    High,
    /// Move to center of screen.
    Middle,
    /// Move to bottom of screen.
    Low,
    /// Move to start of semantically separated word.
    SemanticLeft,
    /// Move to start of next semantically separated word.
    SemanticRight,
    /// Move to end of previous semantically separated word.
    #[allow(unused)]
    SemanticLeftEnd,
    /// Move to end of semantically separated word.
    SemanticRightEnd,
    /// Move to start of whitespace separated word.
    WordLeft,
    /// Move to start of next whitespace separated word.
    WordRight,
    /// Move to end of previous whitespace separated word.
    #[allow(unused)]
    WordLeftEnd,
    /// Move to end of whitespace separated word.
    WordRightEnd,
    /// Move to opposing bracket.
    Bracket,
}

/// Cursor tracking vi mode position.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub struct ViModeCursor {
    pub pos: Pos,
}

impl ViModeCursor {
    pub fn new(pos: Pos) -> Self {
        Self { pos }
    }

    /// Move vi mode cursor.
    #[must_use = "this returns the result of the operation, without modifying the original"]
    pub fn motion<T: EventListener>(
        mut self,
        term: &mut Crosswords<T>,
        motion: ViMotion,
    ) -> Self {
        match motion {
            ViMotion::Up => {
                if self.pos.row > term.grid.topmost_line() {
                    self.pos.row -= 1;
                }
            }
            ViMotion::Down => {
                if self.pos.row + 1 < term.grid.screen_lines() as i32 {
                    self.pos.row += 1;
                }
            }
            ViMotion::Left => {
                self.pos = term.expand_wide(self.pos, Direction::Left);
                let wrap_pos = Pos::new(self.pos.row - 1, term.grid.last_column());
                if self.pos.col == 0
                    && self.pos.row > term.grid.topmost_line()
                    && is_wrap(term, wrap_pos)
                {
                    self.pos = wrap_pos;
                } else {
                    self.pos.col = Column(self.pos.col.saturating_sub(1));
                }
            }
            ViMotion::Right => {
                self.pos = term.expand_wide(self.pos, Direction::Right);
                if is_wrap(term, self.pos) {
                    self.pos = Pos::new(self.pos.row + 1, Column(0));
                } else {
                    self.pos.col = min(self.pos.col + 1, term.grid.last_column());
                }
            }
            ViMotion::First => {
                self.pos = term.expand_wide(self.pos, Direction::Left);
                while self.pos.col == 0
                    && self.pos.row > term.grid.topmost_line()
                    && is_wrap(term, Pos::new(self.pos.row - 1, term.grid.last_column()))
                {
                    self.pos.row -= 1;
                }
                self.pos.col = Column(0);
            }
            ViMotion::Last => self.pos = last(term, self.pos),
            ViMotion::FirstOccupied => self.pos = first_occupied(term, self.pos),
            ViMotion::High => {
                let line = Line(-(term.display_offset() as i32));
                let col = first_occupied_in_line(term, line).unwrap_or_default().col;
                self.pos = Pos::new(line, col);
            }
            ViMotion::Middle => {
                let display_offset = term.display_offset() as i32;
                let line =
                    Line(-display_offset + term.grid.screen_lines() as i32 / 2 - 1);
                let col = first_occupied_in_line(term, line).unwrap_or_default().col;
                self.pos = Pos::new(line, col);
            }
            ViMotion::Low => {
                let display_offset = term.display_offset() as i32;
                let line = Line(-display_offset + term.grid.screen_lines() as i32 - 1);
                let col = first_occupied_in_line(term, line).unwrap_or_default().col;
                self.pos = Pos::new(line, col);
            }
            ViMotion::SemanticLeft => {
                self.pos = semantic(term, self.pos, Direction::Left, Side::Left);
            }
            ViMotion::SemanticRight => {
                self.pos = semantic(term, self.pos, Direction::Right, Side::Left);
            }
            ViMotion::SemanticLeftEnd => {
                self.pos = semantic(term, self.pos, Direction::Left, Side::Right);
            }
            ViMotion::SemanticRightEnd => {
                self.pos = semantic(term, self.pos, Direction::Right, Side::Right);
            }
            ViMotion::WordLeft => {
                self.pos = word(term, self.pos, Direction::Left, Side::Left);
            }
            ViMotion::WordRight => {
                self.pos = word(term, self.pos, Direction::Right, Side::Left);
            }
            ViMotion::WordLeftEnd => {
                self.pos = word(term, self.pos, Direction::Left, Side::Right);
            }
            ViMotion::WordRightEnd => {
                self.pos = word(term, self.pos, Direction::Right, Side::Right);
            }
            ViMotion::Bracket => {
                self.pos = term.bracket_search(self.pos).unwrap_or(self.pos)
            }
        }

        term.scroll_to_pos(self.pos);

        self
    }

    /// Get target cursor pos for vim-like page movement.
    #[allow(unused)]
    #[must_use = "this returns the result of the operation, without modifying the original"]
    pub fn scroll<T: EventListener>(mut self, term: &Crosswords<T>, lines: i32) -> Self {
        // Clamp movement to within visible region.
        let line = (self.pos.row - lines).grid_clamp(&term.grid, Boundary::Grid);

        // Find the first occupied cell after scrolling has been performed.
        let column = first_occupied_in_line(term, line).unwrap_or_default().col;

        // Move cursor.
        self.pos = Pos::new(line, column);

        self
    }
}

/// Find next end of line to move to.
fn last<T: EventListener>(term: &Crosswords<T>, mut pos: Pos) -> Pos {
    // Expand across wide cells.
    pos = term.expand_wide(pos, Direction::Right);

    // Find last non-empty cell in the current line.
    let occupied = last_occupied_in_line(term, pos.row).unwrap_or_default();

    if pos.col < occupied.col {
        // Jump to last occupied cell when not already at or beyond it.
        occupied
    } else if is_wrap(term, pos) {
        // Jump to last occupied cell across linewraps.
        while is_wrap(term, pos) {
            pos.row += 1;
        }

        last_occupied_in_line(term, pos.row).unwrap_or(pos)
    } else {
        // Jump to last column when beyond the last occupied cell.
        Pos::new(pos.row, term.grid.last_column())
    }
}

/// Find next non-empty cell to move to.
fn first_occupied<T: EventListener>(term: &Crosswords<T>, mut pos: Pos) -> Pos {
    let last_column = term.grid.last_column();

    // Expand left across wide chars, since we're searching lines left to right.
    pos = term.expand_wide(pos, Direction::Left);

    // Find first non-empty cell in current line.
    let occupied = first_occupied_in_line(term, pos.row)
        .unwrap_or_else(|| Pos::new(pos.row, last_column));

    // Jump across wrapped lines if we're already at this line's first occupied cell.
    if pos == occupied {
        let mut occupied = None;

        // Search for non-empty cell in previous lines.
        for line in (term.grid.topmost_line().0..pos.row.0)
            .rev()
            .map(Line::from)
        {
            if !is_wrap(term, Pos::new(line, last_column)) {
                break;
            }

            occupied = first_occupied_in_line(term, line).or(occupied);
        }

        // Fallback to the next non-empty cell.
        let mut line = pos.row;
        occupied.unwrap_or_else(|| loop {
            if let Some(occupied) = first_occupied_in_line(term, line) {
                break occupied;
            }

            let last_cell = Pos::new(line, last_column);
            if !is_wrap(term, last_cell) {
                break last_cell;
            }

            line += 1;
        })
    } else {
        occupied
    }
}

/// Move by semantically separated word, like w/b/e/ge in vi.
fn semantic<T: EventListener>(
    term: &mut Crosswords<T>,
    mut pos: Pos,
    direction: Direction,
    side: Side,
) -> Pos {
    // Expand semantically based on movement direction.
    let expand_semantic = |pos: Pos| {
        // Do not expand when currently on a semantic escape char.
        let cell = &term.grid[pos];
        if term.semantic_escape_chars().contains(cell.c)
            && !cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
        {
            pos
        } else if direction == Direction::Left {
            term.semantic_search_left(pos)
        } else {
            term.semantic_search_right(pos)
        }
    };

    // Make sure we jump above wide chars.
    pos = term.expand_wide(pos, direction);

    // Move to word boundary.
    if direction != side && !is_boundary(term, pos, direction) {
        pos = expand_semantic(pos);
    }

    // Skip whitespace.
    let mut next_pos = advance(term, pos, direction);
    while !is_boundary(term, pos, direction) && is_space(term, next_pos) {
        pos = next_pos;
        next_pos = advance(term, pos, direction);
    }

    // Assure minimum movement of one cell.
    if !is_boundary(term, pos, direction) {
        pos = advance(term, pos, direction);
    }

    // Move to word boundary.
    if direction == side && !is_boundary(term, pos, direction) {
        pos = expand_semantic(pos);
    }

    pos
}

/// Move by whitespace separated word, like W/B/E/gE in vi.
fn word<T: EventListener>(
    term: &mut Crosswords<T>,
    mut pos: Pos,
    direction: Direction,
    side: Side,
) -> Pos {
    // Make sure we jump above wide chars.
    pos = term.expand_wide(pos, direction);

    if direction == side {
        // Skip whitespace until right before a word.
        let mut next_pos = advance(term, pos, direction);
        while !is_boundary(term, pos, direction) && is_space(term, next_pos) {
            pos = next_pos;
            next_pos = advance(term, pos, direction);
        }

        // Skip non-whitespace until right inside word boundary.
        let mut next_pos = advance(term, pos, direction);
        while !is_boundary(term, pos, direction) && !is_space(term, next_pos) {
            pos = next_pos;
            next_pos = advance(term, pos, direction);
        }
    }

    if direction != side {
        // Skip non-whitespace until just beyond word.
        while !is_boundary(term, pos, direction) && !is_space(term, pos) {
            pos = advance(term, pos, direction);
        }

        // Skip whitespace until right inside word boundary.
        while !is_boundary(term, pos, direction) && is_space(term, pos) {
            pos = advance(term, pos, direction);
        }
    }

    pos
}

/// Find first non-empty cell in line.
fn first_occupied_in_line<T: EventListener>(
    term: &Crosswords<T>,
    line: Line,
) -> Option<Pos> {
    (0..term.grid.columns())
        .map(|col| Pos::new(line, Column(col)))
        .find(|&pos| !is_space(term, pos))
}

/// Find last non-empty cell in line.
fn last_occupied_in_line<T: EventListener>(
    term: &Crosswords<T>,
    line: Line,
) -> Option<Pos> {
    (0..term.grid.columns())
        .map(|col| Pos::new(line, Column(col)))
        .rfind(|&pos| !is_space(term, pos))
}

/// Advance pos based on direction.
fn advance<T: EventListener>(
    term: &Crosswords<T>,
    pos: Pos,
    direction: Direction,
) -> Pos {
    if direction == Direction::Left {
        pos.sub(&term.grid, Boundary::Grid, 1)
    } else {
        pos.add(&term.grid, Boundary::Grid, 1)
    }
}

/// Check if cell at pos contains whitespace.
fn is_space<T: EventListener>(term: &Crosswords<T>, pos: Pos) -> bool {
    let cell = &term.grid[pos.row][pos.col];
    !cell
        .flags()
        .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
        && (cell.c == ' ' || cell.c == '\t')
}

/// Check if the cell at a pos contains the WRAPLINE flag.
fn is_wrap<T: EventListener>(term: &Crosswords<T>, pos: Pos) -> bool {
    term.grid[pos].flags.contains(Flags::WRAPLINE)
}

/// Check if pos is at screen boundary.
fn is_boundary<T: EventListener>(
    term: &Crosswords<T>,
    pos: Pos,
    direction: Direction,
) -> bool {
    (pos.row <= term.grid.topmost_line() && pos.col == 0 && direction == Direction::Left)
        || (pos.row == term.bottommost_line()
            && pos.col + 1 >= term.grid.columns()
            && direction == Direction::Right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crosswords::pos::{Column, Line};
    use crate::crosswords::CrosswordsSize;
    use crate::crosswords::{Crosswords, CursorShape};
    use crate::event::VoidListener;
    use crate::performer::handler::Handler;

    fn term() -> Crosswords<VoidListener> {
        let size = CrosswordsSize::new(20, 20);
        Crosswords::new(
            size,
            CursorShape::Underline,
            VoidListener,
            crate::event::WindowId::from(0),
            0,
        )
    }

    #[test]
    fn motion_simple() {
        let mut term = term();

        let mut cursor = ViModeCursor::new(Pos::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::Right);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(1)));

        cursor = cursor.motion(&mut term, ViMotion::Left);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::Down);
        assert_eq!(cursor.pos, Pos::new(Line(1), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::Up);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(0)));
    }

    #[test]
    fn simple_wide() {
        let mut term = term();
        term.grid[Line(0)][Column(0)].c = 'a';
        term.grid[Line(0)][Column(1)].c = '汉';
        term.grid[Line(0)][Column(1)].flags.insert(Flags::WIDE_CHAR);
        term.grid[Line(0)][Column(2)].c = ' ';
        term.grid[Line(0)][Column(2)]
            .flags
            .insert(Flags::WIDE_CHAR_SPACER);
        term.grid[Line(0)][Column(3)].c = 'a';

        let mut cursor = ViModeCursor::new(Pos::new(Line(0), Column(1)));
        cursor = cursor.motion(&mut term, ViMotion::Right);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(3)));

        let mut cursor = ViModeCursor::new(Pos::new(Line(0), Column(2)));
        cursor = cursor.motion(&mut term, ViMotion::Left);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(0)));
    }

    #[test]
    fn motion_start_end() {
        let mut term = term();

        let mut cursor = ViModeCursor::new(Pos::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::Last);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(19)));

        cursor = cursor.motion(&mut term, ViMotion::First);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(0)));
    }

    #[test]
    fn motion_first_occupied() {
        let mut term = term();
        term.grid[Line(0)][Column(0)].c = ' ';
        term.grid[Line(0)][Column(1)].c = 'x';
        term.grid[Line(0)][Column(2)].c = ' ';
        term.grid[Line(0)][Column(3)].c = 'y';
        term.grid[Line(0)][Column(19)].flags.insert(Flags::WRAPLINE);
        term.grid[Line(1)][Column(19)].flags.insert(Flags::WRAPLINE);
        term.grid[Line(2)][Column(0)].c = 'z';
        term.grid[Line(2)][Column(1)].c = ' ';

        let mut cursor = ViModeCursor::new(Pos::new(Line(2), Column(1)));

        cursor = cursor.motion(&mut term, ViMotion::FirstOccupied);
        assert_eq!(cursor.pos, Pos::new(Line(2), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::FirstOccupied);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(1)));
    }

    #[test]
    fn motion_high_middle_low() {
        let mut term = term();

        let mut cursor = ViModeCursor::new(Pos::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::High);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::Middle);
        assert_eq!(cursor.pos, Pos::new(Line(9), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::Low);
        assert_eq!(cursor.pos, Pos::new(Line(19), Column(0)));
    }

    #[test]
    fn motion_bracket() {
        let mut term = term();
        term.grid[Line(0)][Column(0)].c = '(';
        term.grid[Line(0)][Column(1)].c = 'x';
        term.grid[Line(0)][Column(2)].c = ')';

        let mut cursor = ViModeCursor::new(Pos::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::Bracket);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(2)));

        cursor = cursor.motion(&mut term, ViMotion::Bracket);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(0)));
    }

    fn motion_semantic_term() -> Crosswords<VoidListener> {
        let mut term = term();

        term.grid[Line(0)][Column(0)].c = 'x';
        term.grid[Line(0)][Column(1)].c = ' ';
        term.grid[Line(0)][Column(2)].c = 'x';
        term.grid[Line(0)][Column(3)].c = 'x';
        term.grid[Line(0)][Column(4)].c = ' ';
        term.grid[Line(0)][Column(5)].c = ' ';
        term.grid[Line(0)][Column(6)].c = ':';
        term.grid[Line(0)][Column(7)].c = ' ';
        term.grid[Line(0)][Column(8)].c = 'x';
        term.grid[Line(0)][Column(9)].c = ':';
        term.grid[Line(0)][Column(10)].c = 'x';
        term.grid[Line(0)][Column(11)].c = ' ';
        term.grid[Line(0)][Column(12)].c = ' ';
        term.grid[Line(0)][Column(13)].c = ':';
        term.grid[Line(0)][Column(14)].c = ' ';
        term.grid[Line(0)][Column(15)].c = 'x';

        term
    }

    #[test]
    fn motion_semantic_right_end() {
        let mut term = motion_semantic_term();

        let mut cursor = ViModeCursor::new(Pos::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRightEnd);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(3)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRightEnd);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(6)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRightEnd);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(8)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRightEnd);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(9)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRightEnd);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(10)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRightEnd);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(13)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRightEnd);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(15)));
    }

    #[test]
    fn motion_semantic_left_start() {
        let mut term = motion_semantic_term();

        let mut cursor = ViModeCursor::new(Pos::new(Line(0), Column(15)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(13)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(10)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(9)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(8)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(6)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(2)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(0)));
    }

    #[test]
    fn motion_semantic_right_start() {
        let mut term = motion_semantic_term();

        let mut cursor = ViModeCursor::new(Pos::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRight);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(2)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRight);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(6)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRight);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(8)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRight);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(9)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRight);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(10)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRight);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(13)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRight);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(15)));
    }

    #[test]
    fn motion_semantic_left_end() {
        let mut term = motion_semantic_term();

        let mut cursor = ViModeCursor::new(Pos::new(Line(0), Column(15)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeftEnd);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(13)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeftEnd);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(10)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeftEnd);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(9)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeftEnd);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(8)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeftEnd);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(6)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeftEnd);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(3)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeftEnd);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(0)));
    }

    #[test]
    fn scroll_semantic() {
        let mut term = term();
        term.grid.scroll_up(&(Line(0)..Line(20)), 5);

        let mut cursor = ViModeCursor::new(Pos::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft);
        assert_eq!(cursor.pos, Pos::new(Line(-5), Column(0)));
        assert_eq!(term.display_offset(), 5);

        cursor = cursor.motion(&mut term, ViMotion::SemanticRight);
        assert_eq!(cursor.pos, Pos::new(Line(19), Column(19)));
        assert_eq!(term.display_offset(), 0);

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeftEnd);
        assert_eq!(cursor.pos, Pos::new(Line(-5), Column(0)));
        assert_eq!(term.display_offset(), 5);

        cursor = cursor.motion(&mut term, ViMotion::SemanticRightEnd);
        assert_eq!(cursor.pos, Pos::new(Line(19), Column(19)));
        assert_eq!(term.display_offset(), 0);
    }

    #[test]
    fn semantic_wide() {
        let mut term = term();
        term.grid[Line(0)][Column(0)].c = 'a';
        term.grid[Line(0)][Column(1)].c = ' ';
        term.grid[Line(0)][Column(2)].c = '汉';
        term.grid[Line(0)][Column(2)].flags.insert(Flags::WIDE_CHAR);
        term.grid[Line(0)][Column(3)].c = ' ';
        term.grid[Line(0)][Column(3)]
            .flags
            .insert(Flags::WIDE_CHAR_SPACER);
        term.grid[Line(0)][Column(4)].c = ' ';
        term.grid[Line(0)][Column(5)].c = 'a';

        let mut cursor = ViModeCursor::new(Pos::new(Line(0), Column(2)));
        cursor = cursor.motion(&mut term, ViMotion::SemanticRight);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(5)));

        let mut cursor = ViModeCursor::new(Pos::new(Line(0), Column(3)));
        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(0)));
    }

    #[test]
    fn motion_word() {
        let mut term = term();
        term.grid[Line(0)][Column(0)].c = 'a';
        term.grid[Line(0)][Column(1)].c = ';';
        term.grid[Line(0)][Column(2)].c = ' ';
        term.grid[Line(0)][Column(3)].c = ' ';
        term.grid[Line(0)][Column(4)].c = 'a';
        term.grid[Line(0)][Column(5)].c = ';';

        let mut cursor = ViModeCursor::new(Pos::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::WordRightEnd);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(1)));

        cursor = cursor.motion(&mut term, ViMotion::WordRightEnd);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(5)));

        cursor = cursor.motion(&mut term, ViMotion::WordLeft);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(4)));

        cursor = cursor.motion(&mut term, ViMotion::WordLeft);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::WordRight);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(4)));

        cursor = cursor.motion(&mut term, ViMotion::WordLeftEnd);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(1)));
    }

    #[test]
    fn scroll_word() {
        let mut term = term();
        term.grid.scroll_up(&(Line(0)..Line(20)), 5);

        let mut cursor = ViModeCursor::new(Pos::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::WordLeft);
        assert_eq!(cursor.pos, Pos::new(Line(-5), Column(0)));
        assert_eq!(term.display_offset(), 5);

        cursor = cursor.motion(&mut term, ViMotion::WordRight);
        assert_eq!(cursor.pos, Pos::new(Line(19), Column(19)));
        assert_eq!(term.display_offset(), 0);

        cursor = cursor.motion(&mut term, ViMotion::WordLeftEnd);
        assert_eq!(cursor.pos, Pos::new(Line(-5), Column(0)));
        assert_eq!(term.display_offset(), 5);

        cursor = cursor.motion(&mut term, ViMotion::WordRightEnd);
        assert_eq!(cursor.pos, Pos::new(Line(19), Column(19)));
        assert_eq!(term.display_offset(), 0);
    }

    #[test]
    fn word_wide() {
        let mut term = term();
        term.grid[Line(0)][Column(0)].c = 'a';
        term.grid[Line(0)][Column(1)].c = ' ';
        term.grid[Line(0)][Column(2)].c = '汉';
        term.grid[Line(0)][Column(2)].flags.insert(Flags::WIDE_CHAR);
        term.grid[Line(0)][Column(3)].c = ' ';
        term.grid[Line(0)][Column(3)]
            .flags
            .insert(Flags::WIDE_CHAR_SPACER);
        term.grid[Line(0)][Column(4)].c = ' ';
        term.grid[Line(0)][Column(5)].c = 'a';

        let mut cursor = ViModeCursor::new(Pos::new(Line(0), Column(2)));
        cursor = cursor.motion(&mut term, ViMotion::WordRight);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(5)));

        let mut cursor = ViModeCursor::new(Pos::new(Line(0), Column(3)));
        cursor = cursor.motion(&mut term, ViMotion::WordLeft);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(0)));
    }

    #[test]
    fn scroll_simple() {
        let mut term = term();

        // Create 1 line of scrollback.
        for _ in 0..20 {
            term.newline();
        }

        let mut cursor = ViModeCursor::new(Pos::new(Line(0), Column(0)));

        cursor = cursor.scroll(&term, -1);
        assert_eq!(cursor.pos, Pos::new(Line(1), Column(0)));

        cursor = cursor.scroll(&term, 1);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(0)));

        cursor = cursor.scroll(&term, 1);
        assert_eq!(cursor.pos, Pos::new(Line(-1), Column(0)));
    }

    #[test]
    fn scroll_over_top() {
        let mut term = term();

        // Create 40 lines of scrollback.
        for _ in 0..59 {
            term.newline();
        }

        let mut cursor = ViModeCursor::new(Pos::new(Line(19), Column(0)));

        cursor = cursor.scroll(&term, 20);
        assert_eq!(cursor.pos, Pos::new(Line(-1), Column(0)));

        cursor = cursor.scroll(&term, 20);
        assert_eq!(cursor.pos, Pos::new(Line(-21), Column(0)));

        cursor = cursor.scroll(&term, 20);
        assert_eq!(cursor.pos, Pos::new(Line(-40), Column(0)));

        cursor = cursor.scroll(&term, 20);
        assert_eq!(cursor.pos, Pos::new(Line(-40), Column(0)));
    }

    #[test]
    fn scroll_over_bottom() {
        let mut term = term();

        // Create 40 lines of scrollback.
        for _ in 0..59 {
            term.newline();
        }

        let mut cursor = ViModeCursor::new(Pos::new(Line(-40), Column(0)));

        cursor = cursor.scroll(&term, -20);
        assert_eq!(cursor.pos, Pos::new(Line(-20), Column(0)));

        cursor = cursor.scroll(&term, -20);
        assert_eq!(cursor.pos, Pos::new(Line(0), Column(0)));

        cursor = cursor.scroll(&term, -20);
        assert_eq!(cursor.pos, Pos::new(Line(19), Column(0)));

        cursor = cursor.scroll(&term, -20);
        assert_eq!(cursor.pos, Pos::new(Line(19), Column(0)));
    }
}
