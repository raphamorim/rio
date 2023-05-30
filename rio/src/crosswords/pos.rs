use crate::ansi::CursorShape;
use crate::crosswords::grid::Dimensions;
use config::Config;
use std::cmp::{max, min, Ord, Ordering};
use std::fmt;
use std::ops::{Add, AddAssign, Deref, Index, IndexMut, Sub, SubAssign};
use std::rc::Rc;

pub type Side = Direction;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Direction {
    Left,
    Right,
}

impl Direction {
    #[allow(unused)]
    pub fn opposite(self) -> Self {
        match self {
            Side::Right => Side::Left,
            Side::Left => Side::Right,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Cursor<T> {
    pub pos: Pos,

    /// Template Square when using this cursor.
    pub template: T,

    /// Currently configured graphic character sets.
    pub charsets: Charsets,

    /// Tracks if the next call to input will need to first handle wrapping.
    pub should_wrap: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CursorState {
    pub pos: Pos,
    pub content: CursorShape,
}

impl CursorState {
    pub fn new(config: &Rc<Config>) -> CursorState {
        CursorState {
            pos: Pos::default(),
            content: CursorShape::from_char(config.cursor),
        }
    }
    pub fn is_visible(&self) -> bool {
        self.content != CursorShape::Hidden
    }
}

#[derive(Clone, Default, Copy, Debug, Eq, PartialEq)]
pub enum StandardCharset {
    #[default]
    Ascii,
    SpecialCharacterAndLineDrawing,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub struct Charsets([StandardCharset; 4]);

/// Identifiers which can be assigned to a graphic character set.
#[derive(Clone, Default, Copy, Debug, Eq, PartialEq)]
pub enum CharsetIndex {
    /// Default set, is designated as ASCII at startup.
    #[default]
    G0,
    G1,
    G2,
    G3,
}

impl Index<CharsetIndex> for Charsets {
    type Output = StandardCharset;

    fn index(&self, index: CharsetIndex) -> &StandardCharset {
        &self.0[index as usize]
    }
}

impl IndexMut<CharsetIndex> for Charsets {
    fn index_mut(&mut self, index: CharsetIndex) -> &mut StandardCharset {
        &mut self.0[index as usize]
    }
}

impl StandardCharset {
    /// Switch/Map character to the active charset. Ascii is the common case and
    /// for that we want to do as little as possible.
    #[inline]
    pub fn map(self, c: char) -> char {
        match self {
            StandardCharset::Ascii => c,
            StandardCharset::SpecialCharacterAndLineDrawing => match c {
                '_' => ' ',
                '`' => '◆',
                'a' => '▒',
                'b' => '\u{2409}', // Symbol for horizontal tabulation
                'c' => '\u{240c}', // Symbol for form feed
                'd' => '\u{240d}', // Symbol for carriage return
                'e' => '\u{240a}', // Symbol for line feed
                'f' => '°',
                'g' => '±',
                'h' => '\u{2424}', // Symbol for newline
                'i' => '\u{240b}', // Symbol for vertical tabulation
                'j' => '┘',
                'k' => '┐',
                'l' => '┌',
                'm' => '└',
                'n' => '┼',
                'o' => '⎺',
                'p' => '⎻',
                'q' => '─',
                'r' => '⎼',
                's' => '⎽',
                't' => '├',
                'u' => '┤',
                'v' => '┴',
                'w' => '┬',
                'x' => '│',
                'y' => '≤',
                'z' => '≥',
                '{' => 'π',
                '|' => '≠',
                '}' => '£',
                '~' => '·',
                _ => c,
            },
        }
    }
}

pub enum Boundary {
    /// Cursor's range of motion in the grid.
    ///
    /// This is equal to the viewport when the user isn't scrolled into the history.
    Cursor,

    /// Topmost line in history until the bottommost line in the terminal.
    Grid,

    /// Unbounded.
    None,
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialOrd, PartialEq)]
pub struct Pos<L = Line, C = Column> {
    pub row: L,
    pub col: C,
}

impl<L, C> Pos<L, C> {
    pub fn new(row: L, col: C) -> Pos<L, C> {
        Pos { row, col }
    }
}

impl Pos {
    #[inline]
    pub fn sub<D>(mut self, dimensions: &D, boundary: Boundary, rhs: usize) -> Self
    where
        D: Dimensions,
    {
        let cols = dimensions.columns();
        let line_changes = (rhs + cols - 1).saturating_sub(self.col.0) / cols;
        self.row -= line_changes;
        self.col = Column((cols + self.col.0 - rhs % cols) % cols);
        self.grid_clamp(dimensions, boundary)
    }

    #[inline]
    #[allow(unused)]
    pub fn add<D>(mut self, dimensions: &D, boundary: Boundary, rhs: usize) -> Self
    where
        D: Dimensions,
    {
        let cols = dimensions.columns();
        self.row += (rhs + self.col.0) / cols;
        self.col = Column((self.col.0 + rhs) % cols);
        self.grid_clamp(dimensions, boundary)
    }

    pub fn grid_clamp<D>(mut self, dimensions: &D, boundary: Boundary) -> Self
    where
        D: Dimensions,
    {
        let last_column = dimensions.last_column();
        self.col = min(self.col, last_column);

        let topmost_line = dimensions.topmost_line();
        let bottommost_line = dimensions.bottommost_line();

        match boundary {
            Boundary::Cursor if self.row < 0 => Pos::new(Line(0), Column(0)),
            Boundary::Grid if self.row < topmost_line => {
                Pos::new(topmost_line, Column(0))
            }
            Boundary::Cursor | Boundary::Grid if self.row > bottommost_line => {
                Pos::new(bottommost_line, last_column)
            }
            Boundary::None => {
                self.row = self.row.grid_clamp(dimensions, boundary);
                self
            }
            _ => self,
        }
    }
}

/// A line.
///
/// Newtype to avoid passing values incorrectly.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Default, Ord, PartialOrd)]
pub struct Line(pub i32);

impl Line {
    /// Clamp a line to a grid boundary.
    #[must_use]
    pub fn grid_clamp<D: Dimensions>(self, dimensions: &D, boundary: Boundary) -> Self {
        match boundary {
            Boundary::Cursor => max(Line(0), min(dimensions.bottommost_line(), self)),
            Boundary::Grid => {
                let bottommost_line = dimensions.bottommost_line();
                let topmost_line = dimensions.topmost_line();
                max(topmost_line, min(bottommost_line, self))
            }
            Boundary::None => {
                let screen_lines = dimensions.screen_lines() as i32;
                let total_lines = dimensions.total_lines() as i32;

                if self >= screen_lines {
                    let topmost_line = dimensions.topmost_line();
                    let extra = (self.0 - screen_lines) % total_lines;
                    topmost_line + extra
                } else {
                    let bottommost_line = dimensions.bottommost_line();
                    let extra = (self.0 - screen_lines + 1) % total_lines;
                    bottommost_line + extra
                }
            }
        }
    }
}

impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<usize> for Line {
    fn from(source: usize) -> Self {
        Self(source as i32)
    }
}

impl Add<usize> for Line {
    type Output = Line;

    #[inline]
    fn add(self, rhs: usize) -> Line {
        self + rhs as i32
    }
}

impl AddAssign<usize> for Line {
    #[inline]
    fn add_assign(&mut self, rhs: usize) {
        *self += rhs as i32;
    }
}

impl Sub<usize> for Line {
    type Output = Line;

    #[inline]
    fn sub(self, rhs: usize) -> Line {
        self - rhs as i32
    }
}

impl SubAssign<usize> for Line {
    #[inline]
    fn sub_assign(&mut self, rhs: usize) {
        *self -= rhs as i32;
    }
}

impl PartialOrd<usize> for Line {
    #[inline]
    fn partial_cmp(&self, other: &usize) -> Option<Ordering> {
        self.0.partial_cmp(&(*other as i32))
    }
}

impl PartialEq<usize> for Line {
    #[inline]
    fn eq(&self, other: &usize) -> bool {
        self.0.eq(&(*other as i32))
    }
}

/// A column.
///
/// Newtype to avoid passing values incorrectly.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Default, Ord, PartialOrd)]
pub struct Column(pub usize);

impl fmt::Display for Column {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

macro_rules! ops {
    ($ty:ty, $construct:expr, $primitive:ty) => {
        impl Deref for $ty {
            type Target = $primitive;

            #[inline]
            fn deref(&self) -> &$primitive {
                &self.0
            }
        }

        impl From<$primitive> for $ty {
            #[inline]
            fn from(val: $primitive) -> $ty {
                $construct(val)
            }
        }

        impl Add<$ty> for $ty {
            type Output = $ty;

            #[inline]
            fn add(self, rhs: $ty) -> $ty {
                $construct(self.0 + rhs.0)
            }
        }

        impl AddAssign<$ty> for $ty {
            #[inline]
            fn add_assign(&mut self, rhs: $ty) {
                self.0 += rhs.0;
            }
        }

        impl Add<$primitive> for $ty {
            type Output = $ty;

            #[inline]
            fn add(self, rhs: $primitive) -> $ty {
                $construct(self.0 + rhs)
            }
        }

        impl AddAssign<$primitive> for $ty {
            #[inline]
            fn add_assign(&mut self, rhs: $primitive) {
                self.0 += rhs
            }
        }

        impl Sub<$ty> for $ty {
            type Output = $ty;

            #[inline]
            fn sub(self, rhs: $ty) -> $ty {
                $construct(self.0 - rhs.0)
            }
        }

        impl SubAssign<$ty> for $ty {
            #[inline]
            fn sub_assign(&mut self, rhs: $ty) {
                self.0 -= rhs.0;
            }
        }

        impl Sub<$primitive> for $ty {
            type Output = $ty;

            #[inline]
            fn sub(self, rhs: $primitive) -> $ty {
                $construct(self.0 - rhs)
            }
        }

        impl SubAssign<$primitive> for $ty {
            #[inline]
            fn sub_assign(&mut self, rhs: $primitive) {
                self.0 -= rhs
            }
        }

        impl PartialEq<$ty> for $primitive {
            #[inline]
            fn eq(&self, other: &$ty) -> bool {
                self.eq(&other.0)
            }
        }

        impl PartialEq<$primitive> for $ty {
            #[inline]
            fn eq(&self, other: &$primitive) -> bool {
                self.0.eq(other)
            }
        }

        impl PartialOrd<$ty> for $primitive {
            #[inline]
            fn partial_cmp(&self, other: &$ty) -> Option<Ordering> {
                self.partial_cmp(&other.0)
            }
        }

        impl PartialOrd<$primitive> for $ty {
            #[inline]
            fn partial_cmp(&self, other: &$primitive) -> Option<Ordering> {
                self.0.partial_cmp(other)
            }
        }
    };
}

ops!(Column, Column, usize);
ops!(Line, Line, i32);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_ordering() {
        assert!(Pos::new(Line(0), Column(0)) == Pos::new(Line(0), Column(0)));
        assert!(Pos::new(Line(1), Column(0)) > Pos::new(Line(0), Column(0)));
        assert!(Pos::new(Line(0), Column(1)) > Pos::new(Line(0), Column(0)));
        assert!(Pos::new(Line(1), Column(1)) > Pos::new(Line(0), Column(0)));
        assert!(Pos::new(Line(1), Column(1)) > Pos::new(Line(0), Column(1)));
        assert!(Pos::new(Line(1), Column(1)) > Pos::new(Line(1), Column(0)));
        assert!(Pos::new(Line(0), Column(0)) > Pos::new(Line(-1), Column(0)));
    }
}
