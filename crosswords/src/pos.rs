use crate::dimensions::Dimensions;
use std::cmp::{max, min, Ord, Ordering};
use std::fmt;
use std::ops::{Add, AddAssign, Deref, Sub, SubAssign};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Cursor<T> {
    pub pos: Pos,

    // Template Square when using this cursor.
    pub template: T,
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

#[derive(Debug, Default, Clone, PartialOrd, PartialEq, Eq)]
pub struct Pos {
    pub row: Line,
    pub col: Column,
}

impl Pos {
    fn new(row: Line, col: Column) -> Pos {
        Self { row, col }
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
