//! iTerm2-style smart selection.
//!
//! Given a click position, evaluate a list of precision-tagged regex
//! rules against the surrounding logical line and pick the
//! highest-precision match that contains the click. Falls back to
//! `None` when nothing matches, leaving the caller free to use a
//! plainer selection strategy (semantic, word, etc.).

use crate::crosswords::grid::Dimensions;
use crate::crosswords::pos::{Direction, Pos};
use crate::crosswords::search::{Match, RegexIter, RegexSearch};
use crate::crosswords::Crosswords;
use crate::event::EventListener;
use crate::selection::SelectionRange;

/// Skip smart selection when the logical line exceeds this many cells.
/// 6 regexes over a multi-megabyte line is not worth the click-to-paint
/// latency; the caller falls back to semantic selection instead.
pub const MAX_SCAN_CELLS: usize = 8192;

/// One precision-tagged regex rule.
#[derive(Debug)]
pub struct SmartRule {
    pub name: String,
    pub regex: RegexSearch,
    /// Higher wins. Length is the tiebreaker among equal-precision
    /// matches, so e.g. an IPv4 inside a URL gets shadowed by the URL.
    pub precision: u8,
}

impl SmartRule {
    pub fn new(
        name: impl Into<String>,
        pattern: &str,
        precision: u8,
    ) -> Result<Self, Box<regex_automata::hybrid::BuildError>> {
        Ok(Self {
            name: name.into(),
            regex: RegexSearch::new(pattern)?,
            precision,
        })
    }
}

/// Run the rules against the logical line containing `click` and
/// return the best matching span, or `None` if no rule covers the
/// click position.
pub fn smart_select_at<T: EventListener>(
    term: &Crosswords<T>,
    rules: &mut [SmartRule],
    click: Pos,
) -> Option<SelectionRange> {
    let start = term.line_search_left(click);
    let end = term.line_search_right(click);

    if span_cells(term, start, end) > MAX_SCAN_CELLS {
        return None;
    }

    let mut best: Option<(u8, usize, Match)> = None;
    for rule in rules.iter_mut() {
        for m in RegexIter::new(start, end, Direction::Right, term, &mut rule.regex) {
            if !m.contains(&click) {
                continue;
            }
            let len = span_cells(term, *m.start(), *m.end());
            let better = match &best {
                None => true,
                Some((p, l, _)) => {
                    rule.precision > *p || (rule.precision == *p && len > *l)
                }
            };
            if better {
                best = Some((rule.precision, len, m));
            }
            // RegexIter yields non-overlapping matches, so at most one
            // match per rule contains the click — no point scanning the
            // rest of the line for this rule.
            break;
        }
    }
    best.map(|(_, _, m)| SelectionRange {
        start: *m.start(),
        end: *m.end(),
        is_block: false,
    })
}

/// Inclusive cell count between two ordered positions. Used both as
/// the early-out guard against pathologically long lines and as the
/// length tiebreaker between equal-precision matches.
fn span_cells<T: EventListener>(term: &Crosswords<T>, start: Pos, end: Pos) -> usize {
    let cols = term.grid.columns();
    let row_diff = (end.row.0 - start.row.0).max(0) as usize;
    let col_diff = end.col.0 as isize - start.col.0 as isize;
    let base = row_diff * cols;
    if col_diff >= 0 {
        base + col_diff as usize + 1
    } else {
        base.saturating_sub((-col_diff) as usize) + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ansi::CursorShape;
    use crate::crosswords::pos::{Column, Line};
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

    fn default_rules() -> Vec<SmartRule> {
        crate::config::smart_selection::compile_default_rules()
    }

    fn write(term: &mut Crosswords<VoidListener>, bytes: &[u8]) {
        let mut p = Processor::default();
        p.advance(term, bytes);
    }

    fn find_col(term: &Crosswords<VoidListener>, row: Line, needle: char) -> Column {
        let cols = term.grid.columns();
        for c in 0..cols {
            if term.grid[row][Column(c)].c() == needle {
                return Column(c);
            }
        }
        panic!("char {:?} not found on row {}", needle, row.0);
    }

    #[test]
    fn url_selected_in_full_including_scheme() {
        let mut term = cw(80, 5);
        // "go to " = 6 cols, URL = 29 chars (cols 6..=34), " end" follows.
        write(&mut term, b"go to https://cli.github.com/manual end");
        let mut rules = default_rules();
        // Click on 'g' in "github" (mid-URL).
        let click = Pos::new(Line(0), Column(17));
        let r = smart_select_at(&term, &mut rules, click).expect("URL should match");
        assert_eq!(r.start, Pos::new(Line(0), Column(6)));
        assert_eq!(r.end, Pos::new(Line(0), Column(34)));
    }

    #[test]
    fn url_next_to_bracket_stops_at_bracket() {
        let mut term = cw(80, 5);
        write(&mut term, b"see [https://example.com] for details");
        let mut rules = default_rules();
        let click = Pos::new(Line(0), Column(10));
        let r = smart_select_at(&term, &mut rules, click).expect("URL should match");
        // Selection starts at 'h' of https (col 5) and stops before the ']'.
        assert_eq!(r.start, Pos::new(Line(0), Column(5)));
        // Find the ']' column; URL ends at the cell just before it.
        let close = find_col(&term, Line(0), ']');
        assert_eq!(r.end.col.0, close.0 - 1);
    }

    #[test]
    fn file_line_reference_includes_line_and_column() {
        let mut term = cw(80, 5);
        write(&mut term, b"  at src/main.rs:42:7 in trace");
        let mut rules = default_rules();
        let click = Pos::new(Line(0), Column(10));
        let r = smart_select_at(&term, &mut rules, click)
            .expect("file:line:col should match");
        assert_eq!(r.start, Pos::new(Line(0), Column(5)));
        // `src/main.rs:42:7` ends at the `7` (col 20).
        assert_eq!(r.end, Pos::new(Line(0), Column(20)));
    }

    #[test]
    fn ipv4_matches_when_standalone() {
        let mut term = cw(80, 5);
        write(&mut term, b"host 192.168.1.1 up");
        let mut rules = default_rules();
        let click = Pos::new(Line(0), Column(8));
        let r = smart_select_at(&term, &mut rules, click).expect("IPv4 should match");
        assert_eq!(r.start, Pos::new(Line(0), Column(5)));
        assert_eq!(r.end, Pos::new(Line(0), Column(15)));
    }

    #[test]
    fn url_wins_over_ipv4_inside_it() {
        let mut term = cw(80, 5);
        write(&mut term, b"goto https://192.168.1.1/admin now");
        let mut rules = default_rules();
        // Click inside `192` — URL (200) should beat IPv4 (130).
        let click = Pos::new(Line(0), Column(14));
        let r = smart_select_at(&term, &mut rules, click).expect("URL should win");
        assert_eq!(r.start, Pos::new(Line(0), Column(5)));
        // Ends just before the trailing space.
        assert_eq!(r.end, Pos::new(Line(0), Column(29)));
    }

    #[test]
    fn git_sha_matches() {
        let mut term = cw(80, 5);
        write(&mut term, b"see commit abc1234567 today");
        let mut rules = default_rules();
        let click = Pos::new(Line(0), Column(13));
        let r = smart_select_at(&term, &mut rules, click).expect("git sha should match");
        assert_eq!(r.start, Pos::new(Line(0), Column(11)));
        assert_eq!(r.end, Pos::new(Line(0), Column(20)));
    }

    #[test]
    fn uuid_matches() {
        let mut term = cw(80, 5);
        write(&mut term, b"id 550e8400-e29b-41d4-a716-446655440000 done");
        let mut rules = default_rules();
        let click = Pos::new(Line(0), Column(10));
        let r = smart_select_at(&term, &mut rules, click).expect("UUID should match");
        assert_eq!(r.start, Pos::new(Line(0), Column(3)));
        assert_eq!(r.end, Pos::new(Line(0), Column(38)));
    }

    #[test]
    fn email_matches() {
        let mut term = cw(80, 5);
        write(&mut term, b"mail alice@example.com please");
        let mut rules = default_rules();
        let click = Pos::new(Line(0), Column(8));
        let r = smart_select_at(&term, &mut rules, click).expect("email should match");
        assert_eq!(r.start, Pos::new(Line(0), Column(5)));
        assert_eq!(r.end, Pos::new(Line(0), Column(21)));
    }

    #[test]
    fn no_match_on_plain_word_returns_none() {
        let mut term = cw(80, 5);
        write(&mut term, b"hello world goodbye");
        let mut rules = default_rules();
        let click = Pos::new(Line(0), Column(6));
        assert!(smart_select_at(&term, &mut rules, click).is_none());
    }

    #[test]
    fn click_on_whitespace_returns_none() {
        let mut term = cw(80, 5);
        write(&mut term, b"foo bar 192.168.1.1 baz");
        let mut rules = default_rules();
        // Click on the space at column 7 (between "bar" and "192...").
        assert_eq!(term.grid[Line(0)][Column(7)].c(), ' ');
        let click = Pos::new(Line(0), Column(7));
        assert!(smart_select_at(&term, &mut rules, click).is_none());
    }

    #[test]
    fn url_split_across_soft_wrap_is_selected() {
        // 10-col grid — write a URL that wraps mid-host.
        let mut term = cw(10, 4);
        write(&mut term, b"see https://example.com/x ok");
        // Confirm we wrapped softly.
        assert!(term.grid[Line(0)][Column(9)].wrapline());

        let mut rules = default_rules();
        // Click on the 'a' of "example" on the wrapped row.
        let click = Pos::new(Line(1), Column(2));
        let r = smart_select_at(&term, &mut rules, click).expect("URL should match");
        // URL begins on row 0 at col 4 (h of https).
        assert_eq!(r.start, Pos::new(Line(0), Column(4)));
        // URL ends on a later row, at the last URL char before the space.
        assert!(r.end.row > Line(0));
        assert!(r.end > r.start);
    }
}
