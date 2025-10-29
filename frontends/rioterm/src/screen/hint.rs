use rio_backend::crosswords::pos::{Column, Direction, Line, Pos};
use rio_backend::crosswords::search::Match;
use rio_backend::crosswords::search::{RegexIter, RegexSearch};
use rio_backend::crosswords::Crosswords;
use std::borrow::Cow;
use std::ops::Deref;

/// Maximum number of linewraps followed outside of the viewport during search highlighting.
pub const MAX_SEARCH_LINES: usize = 100;

/// Iterate over all visible regex matches.
pub fn visible_regex_match_iter<'a, T: rio_backend::event::EventListener>(
    term: &'a Crosswords<T>,
    regex: &'a mut RegexSearch,
) -> impl Iterator<Item = Match> + 'a {
    let viewport_start = Line(-(term.grid.display_offset() as i32));
    let viewport_end = viewport_start + term.bottommost_line();
    let mut start = term.line_search_left(Pos::new(viewport_start, Column(0)));
    let mut end = term.line_search_right(Pos::new(viewport_end, Column(0)));
    start.row = start.row.max(viewport_start - MAX_SEARCH_LINES);
    end.row = end.row.min(viewport_end + MAX_SEARCH_LINES);

    RegexIter::new(start, end, Direction::Right, term, regex)
        .skip_while(move |rm| rm.end().row < viewport_start)
        .take_while(move |rm| rm.start().row <= viewport_end)
}

/// Visible hint match tracking.
#[derive(Default)]
pub struct HintMatches<'a> {
    /// All visible matches.
    matches: Cow<'a, [Match]>,

    /// Index of the last match checked.
    #[allow(dead_code)]
    index: usize,
}

impl<'a> HintMatches<'a> {
    /// Create new renderable matches iterator..
    pub fn new(matches: impl Into<Cow<'a, [Match]>>) -> Self {
        Self {
            matches: matches.into(),
            index: 0,
        }
    }

    /// Create from regex matches on term visible part.
    #[inline]
    pub fn visible_regex_matches<T: rio_backend::event::EventListener>(
        term: &Crosswords<T>,
        dfas: &mut RegexSearch,
    ) -> Self {
        let matches = visible_regex_match_iter(term, dfas).collect::<Vec<_>>();
        Self::new(matches)
    }

    /// Advance the regex tracker to the next point.
    ///
    /// This will return `true` if the point passed is part of a regex match.
    #[allow(dead_code)]
    pub fn advance(&mut self, point: Pos) -> bool {
        while let Some(bounds) = self.get(self.index) {
            if bounds.start() > &point {
                break;
            } else if bounds.end() < &point {
                self.index += 1;
            } else {
                return true;
            }
        }
        false
    }
}

impl Deref for HintMatches<'_> {
    type Target = [Match];

    fn deref(&self) -> &Self::Target {
        self.matches.deref()
    }
}
