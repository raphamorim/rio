//! OSC 8 hyperlink span resolution.
//!
//! A click on a hyperlinked cell maps to the full span of cells that
//! share the same OSC 8 `extras_id`. The walk crosses soft-wrapped row
//! boundaries (via the `wrapline` flag) so that a link broken across
//! two rows still selects as one range.

use crate::crosswords::grid::Dimensions;
use crate::crosswords::pos::{Column, Direction, Pos};
use crate::crosswords::Crosswords;
use crate::event::EventListener;
use crate::selection::SelectionRange;

/// Resolve the OSC 8 hyperlink span containing `click`, if any.
/// Returns the inclusive cell range covering every cell that shares
/// the click cell's `extras_id`.
pub fn hyperlink_span_at<T: EventListener>(
    term: &Crosswords<T>,
    click: Pos,
) -> Option<SelectionRange> {
    let id = term.cell_hyperlink_id(click.row, click.col)?;
    Some(SelectionRange {
        start: walk(term, click, id, Direction::Left),
        end: walk(term, click, id, Direction::Right),
        is_block: false,
    })
}

/// Step outward from `pos` while cells keep the same `extras_id`,
/// following the `wrapline` flag across soft-wrapped row boundaries.
fn walk<T: EventListener>(
    term: &Crosswords<T>,
    mut pos: Pos,
    id: u16,
    dir: Direction,
) -> Pos {
    let last_col = term.grid.last_column();
    loop {
        let candidate = match dir {
            Direction::Left if pos.col > Column(0) => Pos::new(pos.row, pos.col - 1),
            Direction::Right if pos.col < last_col => Pos::new(pos.row, pos.col + 1),
            // At a row edge: only continue if the row that feeds this
            // one was soft-wrapped rather than hard-broken.
            Direction::Left
                if pos.row > term.grid.topmost_line()
                    && term.grid[pos.row - 1i32][last_col].wrapline() =>
            {
                Pos::new(pos.row - 1i32, last_col)
            }
            Direction::Right
                if pos.row < term.grid.bottommost_line()
                    && term.grid[pos.row][last_col].wrapline() =>
            {
                Pos::new(pos.row + 1i32, Column(0))
            }
            _ => return pos,
        };
        if term.cell_hyperlink_id(candidate.row, candidate.col) != Some(id) {
            return pos;
        }
        pos = candidate;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ansi::CursorShape;
    use crate::crosswords::pos::Line;
    use crate::crosswords::{Crosswords, CrosswordsSize};
    use crate::event::{VoidListener, WindowId};
    use crate::performer::handler::Processor;

    fn cw(cols: usize, lines: usize) -> Crosswords<VoidListener> {
        let size = CrosswordsSize::new(cols, lines);
        Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            WindowId::from(0),
            0,
            10_000,
        )
    }

    #[test]
    fn no_hyperlink_returns_none() {
        let mut term = cw(20, 5);
        let mut p = Processor::default();
        p.advance(&mut term, b"hello");
        assert!(hyperlink_span_at(&term, Pos::new(Line(0), Column(2))).is_none());
    }

    #[test]
    fn single_cell_hyperlink() {
        let mut term = cw(20, 5);
        let mut p = Processor::default();
        p.advance(&mut term, b"\x1b]8;;https://example.com\x07X\x1b]8;;\x07");
        let r = hyperlink_span_at(&term, Pos::new(Line(0), Column(0))).unwrap();
        assert_eq!(r.start, Pos::new(Line(0), Column(0)));
        assert_eq!(r.end, Pos::new(Line(0), Column(0)));
        assert!(!r.is_block);
    }

    #[test]
    fn multi_cell_hyperlink_single_row() {
        let mut term = cw(20, 5);
        let mut p = Processor::default();
        p.advance(
            &mut term,
            b"go \x1b]8;;https://example.com\x07click\x1b]8;;\x07.",
        );
        // Click in the middle of "click" (cols 3..=7).
        let r = hyperlink_span_at(&term, Pos::new(Line(0), Column(5))).unwrap();
        assert_eq!(r.start, Pos::new(Line(0), Column(3)));
        assert_eq!(r.end, Pos::new(Line(0), Column(7)));
    }

    #[test]
    fn adjacent_links_with_different_ids_return_only_clicked() {
        let mut term = cw(20, 5);
        let mut p = Processor::default();
        p.advance(
            &mut term,
            b"\x1b]8;;https://a.example\x07A\x1b]8;;\x07\
              \x1b]8;;https://b.example\x07B\x1b]8;;\x07",
        );
        let a = hyperlink_span_at(&term, Pos::new(Line(0), Column(0))).unwrap();
        let b = hyperlink_span_at(&term, Pos::new(Line(0), Column(1))).unwrap();
        assert_eq!(a.start, Pos::new(Line(0), Column(0)));
        assert_eq!(a.end, Pos::new(Line(0), Column(0)));
        assert_eq!(b.start, Pos::new(Line(0), Column(1)));
        assert_eq!(b.end, Pos::new(Line(0), Column(1)));
    }

    #[test]
    fn hyperlink_crossing_soft_wrap() {
        // 5-col grid; write 8 chars inside one OSC 8 link so the row wraps.
        let mut term = cw(5, 4);
        let mut p = Processor::default();
        p.advance(
            &mut term,
            b"\x1b]8;;https://example.com\x07abcdefgh\x1b]8;;\x07",
        );
        // Sanity: the wrap should be a soft wrap (wrapline=true on row 0
        // last col); not a CRLF.
        assert!(term.grid[Line(0)][Column(4)].wrapline());

        let r = hyperlink_span_at(&term, Pos::new(Line(0), Column(2))).unwrap();
        assert_eq!(r.start, Pos::new(Line(0), Column(0)));
        assert_eq!(r.end, Pos::new(Line(1), Column(2)));

        // Click on the wrapped row, same span.
        let r2 = hyperlink_span_at(&term, Pos::new(Line(1), Column(1))).unwrap();
        assert_eq!(r2.start, Pos::new(Line(0), Column(0)));
        assert_eq!(r2.end, Pos::new(Line(1), Column(2)));
    }
}
