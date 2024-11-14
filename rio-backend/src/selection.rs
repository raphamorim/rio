// Retired from: https://github.com/alacritty/alacritty/blob/6e7f466c68b387f41726757eed4f3e70d05479d2/alacritty_terminal/src/selection.rs
// which is licensed under Apache 2.0 license.
//! State management for a selection in the grid.
//!
//! A selection should start when the mouse is clicked, and it should be
//! finalized when the button is released. The selection should be cleared
//! when text is added/removed/scrolled on the screen. The selection should
//! also be cleared if the user clicks off of the selection.

use std::cmp::min;
use std::mem;
use std::ops::{Bound, Range, RangeBounds};

use crate::ansi::CursorShape;
use crate::crosswords::grid::{Dimensions, GridSquare, Indexed};
use crate::crosswords::pos::{Boundary, Column, Line, Pos, Side};
use crate::crosswords::square::{Flags, Square};
use crate::crosswords::Crosswords;
use crate::event::EventListener;

/// A Pos and side within that point.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Anchor {
    pub point: Pos,
    side: Side,
}

impl Anchor {
    fn new(point: Pos, side: Side) -> Anchor {
        Anchor { point, side }
    }
}

/// Represents a range of selected cells.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct SelectionRange {
    /// Start point, top left of the selection.
    pub start: Pos,
    /// End point, bottom right of the selection.
    pub end: Pos,
    /// Whether this selection is a block selection.
    pub is_block: bool,
}

impl SelectionRange {
    #[allow(unused)]
    pub fn new(start: Pos, end: Pos, is_block: bool) -> Self {
        assert!(start <= end);
        Self {
            start,
            end,
            is_block,
        }
    }
}

impl SelectionRange {
    /// Check if a point lies within the selection.
    #[allow(unused)]
    pub fn contains(&self, point: Pos) -> bool {
        self.start.row <= point.row
            && self.end.row >= point.row
            && (self.start.col <= point.col
                || (self.start.row != point.row && !self.is_block))
            && (self.end.col >= point.col
                || (self.end.row != point.row && !self.is_block))
    }

    /// Check if the square at a point is part of the selection.
    #[allow(unused)]
    pub fn contains_square(
        &self,
        indexed: &Indexed<&Square>,
        point: Pos,
        shape: CursorShape,
    ) -> bool {
        // Do not invert block cursor at selection boundaries.
        if shape == CursorShape::Block
            && point == indexed.pos
            && (self.start == indexed.pos
                || self.end == indexed.pos
                || (self.is_block
                    && ((self.start.row == indexed.pos.row
                        && self.end.col == indexed.pos.col)
                        || (self.end.row == indexed.pos.row
                            && self.start.col == indexed.pos.col))))
        {
            return false;
        }

        // Pos itself is selected.
        if self.contains(indexed.pos) {
            return true;
        }

        // Check if a wide char's trailing spacer is selected.
        indexed.square.flags().contains(Flags::WIDE_CHAR)
            && self.contains(Pos::new(indexed.pos.row, indexed.pos.col + 1))
    }
}

/// Different kinds of selection.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SelectionType {
    Simple,
    Block,
    Semantic,
    Lines,
}

/// Describes a region of a 2-dimensional area.
///
/// Used to track a text selection. There are four supported modes, each with its own constructor:
/// [`simple`], [`block`], [`semantic`], and [`lines`]. The [`simple`] mode precisely tracks which
/// cells are selected without any expansion. [`block`] will select rectangular regions.
/// [`lines`] will always select entire lines.
///
/// Calls to [`update`] operate different based on the selection kind. The [`simple`] and [`block`]
/// mode do nothing special, simply track points and sides.
///
/// [`simple`]: enum.Selection.html#method.simple
/// [`block`]: enum.Selection.html#method.block
/// [`lines`]: enum.Selection.html#method.rows
/// [`update`]: enum.Selection.html#method.update
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub ty: SelectionType,
    region: Range<Anchor>,
}

impl Selection {
    pub fn new(ty: SelectionType, location: Pos, side: Side) -> Selection {
        Self {
            region: Range {
                start: Anchor::new(location, side),
                end: Anchor::new(location, side),
            },
            ty,
        }
    }

    /// Update the end of the selection.
    pub fn update(&mut self, point: Pos, side: Side) {
        self.region.end = Anchor::new(point, side);
    }

    pub fn rotate<D: Dimensions>(
        mut self,
        dimensions: &D,
        range: &Range<Line>,
        delta: i32,
    ) -> Option<Selection> {
        let bottommost_line = dimensions.bottommost_line();
        let range_bottom = range.end;
        let range_top = range.start;

        let (mut start, mut end) = (&mut self.region.start, &mut self.region.end);
        if start.point > end.point {
            mem::swap(&mut start, &mut end);
        }

        // Rotate start of selection.
        if (start.point.row >= range_top || range_top == 0)
            && start.point.row < range_bottom
        {
            start.point.row = min(start.point.row - delta, bottommost_line);

            // If end is within the same region, delete selection once start rotates out.
            if start.point.row >= range_bottom && end.point.row < range_bottom {
                return None;
            }

            // Clamp selection to start of region.
            if start.point.row < range_top && range_top != 0 {
                if self.ty != SelectionType::Block {
                    start.point.col = Column(0);
                    start.side = Side::Left;
                }
                start.point.row = range_top;
            }
        }

        // Rotate end of selection.
        if (end.point.row >= range_top || range_top == 0) && end.point.row < range_bottom
        {
            end.point.row = min(end.point.row - delta, bottommost_line);

            // Delete selection if end has overtaken the start.
            if end.point.row < start.point.row {
                return None;
            }

            // Clamp selection to end of region.
            if end.point.row >= range_bottom {
                if self.ty != SelectionType::Block {
                    end.point.col = dimensions.last_column();
                    end.side = Side::Right;
                }
                end.point.row = range_bottom - 1;
            }
        }

        Some(self)
    }

    pub fn is_empty(&self) -> bool {
        match self.ty {
            SelectionType::Simple => {
                let (mut start, mut end) = (self.region.start, self.region.end);
                if start.point > end.point {
                    mem::swap(&mut start, &mut end);
                }

                // Simple selection is empty when the points are identical
                // or two adjacent cells have the sides right -> left.
                start == end
                    || (start.side == Side::Right
                        && end.side == Side::Left
                        && (start.point.row == end.point.row)
                        && start.point.col + 1 == end.point.col)
            }
            SelectionType::Block => {
                let (start, end) = (self.region.start, self.region.end);

                // Block selection is empty when the points' columns and sides are identical
                // or two cells with adjacent columns have the sides right -> left,
                // regardless of their lines
                (start.point.col == end.point.col && start.side == end.side)
                    || (start.point.col + 1 == end.point.col
                        && start.side == Side::Right
                        && end.side == Side::Left)
                    || (end.point.col + 1 == start.point.col
                        && start.side == Side::Left
                        && end.side == Side::Right)
            }
            SelectionType::Semantic | SelectionType::Lines => false,
        }
    }

    /// Check whether selection contains any point in a given range.
    pub fn intersects_range<R: RangeBounds<Line>>(&self, range: R) -> bool {
        let mut start = self.region.start.point.row;
        let mut end = self.region.end.point.row;

        if start > end {
            mem::swap(&mut start, &mut end);
        }

        let range_top = match range.start_bound() {
            Bound::Included(&range_start) => range_start,
            Bound::Excluded(&range_start) => range_start + 1,
            Bound::Unbounded => Line(i32::MIN),
        };

        let range_bottom = match range.end_bound() {
            Bound::Included(&range_end) => range_end,
            Bound::Excluded(&range_end) => range_end - 1,
            Bound::Unbounded => Line(i32::MAX),
        };

        range_bottom >= start && range_top <= end
    }

    /// Expand selection sides to include all square.
    pub fn include_all(&mut self) {
        let (start, end) = (self.region.start.point, self.region.end.point);
        let (start_side, end_side) = match self.ty {
            SelectionType::Block
                if start.col > end.col
                    || (start.col == end.col && start.row > end.row) =>
            {
                (Side::Right, Side::Left)
            }
            SelectionType::Block => (Side::Left, Side::Right),
            _ if start > end => (Side::Right, Side::Left),
            _ => (Side::Left, Side::Right),
        };

        self.region.start.side = start_side;
        self.region.end.side = end_side;
    }

    /// Convert selection to grid coordinates.
    pub fn to_range<T: EventListener>(
        &self,
        term: &Crosswords<T>,
    ) -> Option<SelectionRange> {
        let columns = term.grid.columns();

        // Order start above the end.
        let mut start = self.region.start;
        let mut end = self.region.end;

        if start.point > end.point {
            mem::swap(&mut start, &mut end);
        }

        // Clamp selection to within grid boundaries.
        if end.point.row < term.grid.topmost_line() {
            return None;
        }
        start.point = start.point.grid_clamp(&term.grid, Boundary::Grid);

        match self.ty {
            SelectionType::Simple => self.range_simple(start, end, columns),
            SelectionType::Block => self.range_block(start, end),
            SelectionType::Lines => Some(Self::range_lines(term, start.point, end.point)),
            SelectionType::Semantic => {
                Some(Self::range_semantic(term, start.point, end.point))
            }
        }
    }

    fn range_semantic<T: EventListener>(
        term: &Crosswords<T>,
        mut start: Pos,
        mut end: Pos,
    ) -> SelectionRange {
        if start == end {
            if let Some(matching) = term.bracket_search(start) {
                if (matching.row == start.row && matching.col < start.col)
                    || (matching.row < start.row)
                {
                    start = matching;
                } else {
                    end = matching;
                }

                return SelectionRange {
                    start,
                    end,
                    is_block: false,
                };
            }
        }

        let start = term.semantic_search_left(start);
        let end = term.semantic_search_right(end);

        SelectionRange {
            start,
            end,
            is_block: false,
        }
    }

    fn range_lines<T: EventListener>(
        term: &Crosswords<T>,
        start: Pos,
        end: Pos,
    ) -> SelectionRange {
        let start = term.row_search_left(start);
        let end = term.row_search_right(end);

        SelectionRange {
            start,
            end,
            is_block: false,
        }
    }

    fn range_simple(
        &self,
        mut start: Anchor,
        mut end: Anchor,
        columns: usize,
    ) -> Option<SelectionRange> {
        if self.is_empty() {
            return None;
        }

        // Remove last cell if selection ends to the left of a cell.
        if end.side == Side::Left && start.point != end.point {
            // Special case when selection ends to left of first cell.
            if end.point.col == 0 {
                end.point.col = Column(columns - 1);
                end.point.row -= 1;
            } else {
                end.point.col -= 1;
            }
        }

        // Remove first cell if selection starts at the right of a cell.
        if start.side == Side::Right && start.point != end.point {
            start.point.col += 1;

            // Wrap to next line when selection starts to the right of last column.
            if start.point.col == columns {
                start.point.col = Column(0);
                start.point.row += 1;
            }
        }

        Some(SelectionRange {
            start: start.point,
            end: end.point,
            is_block: false,
        })
    }

    fn range_block(&self, mut start: Anchor, mut end: Anchor) -> Option<SelectionRange> {
        if self.is_empty() {
            return None;
        }

        // Always go top-left -> bottom-right.
        if start.point.col > end.point.col {
            mem::swap(&mut start.side, &mut end.side);
            mem::swap(&mut start.point.col, &mut end.point.col);
        }

        // Remove last cell if selection ends to the left of a cell.
        if end.side == Side::Left && start.point != end.point && end.point.col.0 > 0 {
            end.point.col -= 1;
        }

        // Remove first cell if selection starts at the right of a cell.
        if start.side == Side::Right && start.point != end.point {
            start.point.col += 1;
        }

        Some(SelectionRange {
            start: start.point,
            end: end.point,
            is_block: true,
        })
    }
}

/// Tests for selection.
///
/// There are comments on all of the tests describing the selection. Pictograms
/// are used to avoid ambiguity. Grid cells are represented by a [  ]. Only
/// cells that are completely covered are counted in a selection. Ends are
/// represented by `B` and `E` for begin and end, respectively.  A selected cell
/// looks like [XX], [BX] (at the start), [XB] (at the end), [XE] (at the end),
/// and [EX] (at the start), or [BE] for a single cell. Partially selected cells
/// look like [ B] and [E ].
#[cfg(test)]
mod tests {

    use super::*;
    use crate::crosswords::CrosswordsSize;
    use crate::event::VoidListener;

    use crate::crosswords::pos::{Column, Pos, Side};
    use crate::crosswords::Crosswords;

    fn term(height: usize, width: usize) -> Crosswords<VoidListener> {
        let size = CrosswordsSize::new(width, height);
        let window_id = crate::event::WindowId::from(0);

        Crosswords::new(size, CursorShape::Block, VoidListener {}, window_id, 0)
    }

    /// Test case of single cell selection.
    ///
    /// 1. [  ]
    /// 2. [B ]
    /// 3. [BE]
    #[test]
    fn single_cell_left_to_right() {
        let location = Pos::new(Line(0), Column(0));
        let mut selection = Selection::new(SelectionType::Simple, location, Side::Left);
        selection.update(location, Side::Right);

        assert_eq!(
            selection.to_range(&term(1, 2)).unwrap(),
            SelectionRange {
                start: location,
                end: location,
                is_block: false
            }
        );
    }

    /// Test case of single cell selection.
    ///
    /// 1. [  ]
    /// 2. [ B]
    /// 3. [EB]
    #[test]
    fn single_cell_right_to_left() {
        let location = Pos::new(Line(0), Column(0));
        let mut selection = Selection::new(SelectionType::Simple, location, Side::Right);
        selection.update(location, Side::Left);

        assert_eq!(
            selection.to_range(&term(1, 2)).unwrap(),
            SelectionRange {
                start: location,
                end: location,
                is_block: false
            }
        );
    }

    /// Test adjacent cell selection from left to right.
    ///
    /// 1. [  ][  ]
    /// 2. [ B][  ]
    /// 3. [ B][E ]
    #[test]
    fn between_adjacent_cells_left_to_right() {
        let mut selection = Selection::new(
            SelectionType::Simple,
            Pos::new(Line(0), Column(0)),
            Side::Right,
        );
        selection.update(Pos::new(Line(0), Column(1)), Side::Left);

        assert_eq!(selection.to_range(&term(1, 2)), None);
    }

    /// Test adjacent cell selection from right to left.
    ///
    /// 1. [  ][  ]
    /// 2. [  ][B ]
    /// 3. [ E][B ]
    #[test]
    fn between_adjacent_cells_right_to_left() {
        let mut selection = Selection::new(
            SelectionType::Simple,
            Pos::new(Line(0), Column(1)),
            Side::Left,
        );
        selection.update(Pos::new(Line(0), Column(0)), Side::Right);

        assert_eq!(selection.to_range(&term(1, 2)), None);
    }

    /// Test selection across adjacent lines.
    ///
    /// 1.  [  ][  ][  ][  ][  ]
    ///     [  ][  ][  ][  ][  ]
    /// 2.  [  ][ B][  ][  ][  ]
    ///     [  ][  ][  ][  ][  ]
    /// 3.  [  ][ B][XX][XX][XX]
    ///     [XX][XE][  ][  ][  ]
    #[test]
    fn across_adjacent_lines_upward_final_cell_exclusive() {
        let mut selection = Selection::new(
            SelectionType::Simple,
            Pos::new(Line(0), Column(1)),
            Side::Right,
        );
        selection.update(Pos::new(Line(1), Column(1)), Side::Right);

        assert_eq!(
            selection.to_range(&term(2, 5)).unwrap(),
            SelectionRange {
                start: Pos::new(Line(0), Column(2)),
                end: Pos::new(Line(1), Column(1)),
                is_block: false,
            }
        );
    }

    /// Test selection across adjacent lines.
    ///
    /// 1.  [  ][  ][  ][  ][  ]
    ///     [  ][  ][  ][  ][  ]
    /// 2.  [  ][  ][  ][  ][  ]
    ///     [  ][ B][  ][  ][  ]
    /// 3.  [  ][ E][XX][XX][XX]
    ///     [XX][XB][  ][  ][  ]
    /// 4.  [ E][XX][XX][XX][XX]
    ///     [XX][XB][  ][  ][  ]
    #[test]
    fn selection_bigger_then_smaller() {
        let mut selection = Selection::new(
            SelectionType::Simple,
            Pos::new(Line(1), Column(1)),
            Side::Right,
        );
        selection.update(Pos::new(Line(0), Column(1)), Side::Right);
        selection.update(Pos::new(Line(0), Column(0)), Side::Right);

        assert_eq!(
            selection.to_range(&term(2, 5)).unwrap(),
            SelectionRange {
                start: Pos::new(Line(0), Column(1)),
                end: Pos::new(Line(1), Column(1)),
                is_block: false,
            }
        );
    }

    #[test]
    fn line_selection() {
        let size = (10, 5);
        let mut selection = Selection::new(
            SelectionType::Lines,
            Pos::new(Line(9), Column(1)),
            Side::Left,
        );
        selection.update(Pos::new(Line(4), Column(1)), Side::Right);
        selection = selection
            .rotate(&size, &(Line(0)..Line(size.0 as i32)), 4)
            .unwrap();

        assert_eq!(
            selection.to_range(&term(size.0, size.1)).unwrap(),
            SelectionRange {
                start: Pos::new(Line(0), Column(0)),
                end: Pos::new(Line(5), Column(4)),
                is_block: false,
            }
        );
    }

    #[test]
    fn simple_selection() {
        let size = (10, 5);
        let mut selection = Selection::new(
            SelectionType::Simple,
            Pos::new(Line(9), Column(3)),
            Side::Right,
        );
        selection.update(Pos::new(Line(4), Column(1)), Side::Right);
        selection = selection
            .rotate(&size, &(Line(0)..Line(size.0 as i32)), 4)
            .unwrap();

        assert_eq!(
            selection.to_range(&term(size.0, size.1)).unwrap(),
            SelectionRange {
                start: Pos::new(Line(0), Column(2)),
                end: Pos::new(Line(5), Column(3)),
                is_block: false,
            }
        );
    }

    #[test]
    fn semantic_selection() {
        let size = (10, 5);
        let mut selection = Selection::new(
            SelectionType::Semantic,
            Pos::new(Line(9), Column(3)),
            Side::Left,
        );
        selection.update(Pos::new(Line(4), Column(1)), Side::Right);
        selection = selection
            .rotate(&size, &(Line(0)..Line(size.0 as i32)), 4)
            .unwrap();

        assert_eq!(
            selection.to_range(&term(size.0, size.1)).unwrap(),
            SelectionRange {
                start: Pos::new(Line(0), Column(1)),
                end: Pos::new(Line(5), Column(3)),
                is_block: false,
            }
        );
    }

    #[test]
    fn block_selection() {
        let size = (10, 5);
        let mut selection = Selection::new(
            SelectionType::Block,
            Pos::new(Line(9), Column(3)),
            Side::Right,
        );
        selection.update(Pos::new(Line(4), Column(1)), Side::Right);
        selection = selection
            .rotate(&size, &(Line(0)..Line(size.0 as i32)), 4)
            .unwrap();

        assert_eq!(
            selection.to_range(&term(size.0, size.1)).unwrap(),
            SelectionRange {
                start: Pos::new(Line(0), Column(2)),
                end: Pos::new(Line(5), Column(3)),
                is_block: true
            }
        );
    }

    #[test]
    fn simple_is_empty() {
        let mut selection = Selection::new(
            SelectionType::Simple,
            Pos::new(Line(1), Column(0)),
            Side::Right,
        );
        assert!(selection.is_empty());
        selection.update(Pos::new(Line(1), Column(1)), Side::Left);
        assert!(selection.is_empty());
        selection.update(Pos::new(Line(0), Column(0)), Side::Right);
        assert!(!selection.is_empty());
    }

    #[test]
    fn block_is_empty() {
        let mut selection = Selection::new(
            SelectionType::Block,
            Pos::new(Line(1), Column(0)),
            Side::Right,
        );
        assert!(selection.is_empty());
        selection.update(Pos::new(Line(1), Column(1)), Side::Left);
        assert!(selection.is_empty());
        selection.update(Pos::new(Line(1), Column(1)), Side::Right);
        assert!(!selection.is_empty());
        selection.update(Pos::new(Line(0), Column(0)), Side::Right);
        assert!(selection.is_empty());
        selection.update(Pos::new(Line(0), Column(1)), Side::Left);
        assert!(selection.is_empty());
        selection.update(Pos::new(Line(0), Column(1)), Side::Right);
        assert!(!selection.is_empty());
    }

    #[test]
    fn rotate_in_region_up() {
        let size = (10, 5);
        let mut selection = Selection::new(
            SelectionType::Simple,
            Pos::new(Line(7), Column(3)),
            Side::Right,
        );
        selection.update(Pos::new(Line(4), Column(1)), Side::Right);
        selection = selection
            .rotate(&size, &(Line(1)..Line(size.0 as i32 - 1)), 4)
            .unwrap();

        assert_eq!(
            selection.to_range(&term(size.0, size.1)).unwrap(),
            SelectionRange {
                start: Pos::new(Line(1), Column(0)),
                end: Pos::new(Line(3), Column(3)),
                is_block: false,
            }
        );
    }

    #[test]
    fn rotate_in_region_down() {
        let size = (10, 5);
        let mut selection = Selection::new(
            SelectionType::Simple,
            Pos::new(Line(4), Column(3)),
            Side::Right,
        );
        selection.update(Pos::new(Line(1), Column(1)), Side::Left);
        selection = selection
            .rotate(&size, &(Line(1)..Line(size.0 as i32 - 1)), -5)
            .unwrap();

        assert_eq!(
            selection.to_range(&term(size.0, size.1)).unwrap(),
            SelectionRange {
                start: Pos::new(Line(6), Column(1)),
                end: Pos::new(Line(8), size.last_column()),
                is_block: false,
            }
        );
    }

    #[test]
    fn rotate_in_region_up_block() {
        let size = (10, 5);
        let mut selection = Selection::new(
            SelectionType::Block,
            Pos::new(Line(7), Column(3)),
            Side::Right,
        );
        selection.update(Pos::new(Line(4), Column(1)), Side::Right);
        selection = selection
            .rotate(&size, &(Line(1)..Line(size.0 as i32 - 1)), 4)
            .unwrap();

        assert_eq!(
            selection.to_range(&term(size.0, size.1)).unwrap(),
            SelectionRange {
                start: Pos::new(Line(1), Column(2)),
                end: Pos::new(Line(3), Column(3)),
                is_block: true,
            }
        );
    }

    #[test]
    fn range_intersection() {
        let mut selection = Selection::new(
            SelectionType::Lines,
            Pos::new(Line(3), Column(1)),
            Side::Left,
        );
        selection.update(Pos::new(Line(6), Column(1)), Side::Right);

        assert!(selection.intersects_range(..));
        assert!(selection.intersects_range(Line(2)..));
        assert!(selection.intersects_range(Line(3)..=Line(3)));
        assert!(selection.intersects_range(Line(2)..=Line(4)));
        assert!(selection.intersects_range(Line(2)..=Line(7)));
        assert!(selection.intersects_range(Line(4)..=Line(5)));
        assert!(selection.intersects_range(Line(5)..Line(8)));

        assert!(!selection.intersects_range(..=Line(2)));
        assert!(!selection.intersects_range(Line(7)..=Line(8)));
    }
}
