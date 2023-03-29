use crate::Crosswords;
use crate::{Column, Line};

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
}

impl<U> Dimensions for Crosswords<U> {
    #[inline]
    fn total_lines(&self) -> usize {
        self.storage.len()
    }

    #[inline]
    fn screen_lines(&self) -> usize {
        self.rows
    }

    #[inline]
    fn columns(&self) -> usize {
        self.cols
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
}
