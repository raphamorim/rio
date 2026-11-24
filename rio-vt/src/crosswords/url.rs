//! Plain-text URLs under a cell, for hover underlines and clicks. OSC 8
//! hyperlinks are explicit and should be checked first.

use crate::crosswords::pos::{Column, Direction, Line, Pos};
use crate::crosswords::search::{RegexIter, RegexSearch};
use crate::crosswords::square::CellFlags;
use crate::crosswords::Crosswords;
use crate::event::EventListener;

/// Scheme-prefixed URL pattern for plain-text link detection. The engine
/// is regex-automata (no lookbehind), so trailing prose punctuation is
/// trimmed afterwards by `trailing_url_punctuation`.
const URL_REGEX: &str = "(?:https://|http://|mailto:|ftp://|file:|ssh://|ssh:|git://|tel:|magnet:|ipfs://|ipns://|gemini://|gopher://|news:)[^\u{0000}-\u{001F}\u{007F}-\u{009F}<>\"\\s{|}\\^⟨⟩`]+";

/// How many trailing characters of a URL match are prose punctuation
/// rather than URL: `.,;:!?'"` always, and a closing paren/bracket only
/// when its opener is not part of the match.
fn trailing_url_punctuation(text: &str) -> usize {
    let mut len = text.len();
    let mut trimmed = 0;
    while let Some(c) = text[..len].chars().last() {
        let cut = match c {
            '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"' => true,
            ')' => text[..len].matches('(').count() < text[..len].matches(')').count(),
            ']' => text[..len].matches('[').count() < text[..len].matches(']').count(),
            _ => false,
        };
        if !cut {
            break;
        }
        len -= c.len_utf8();
        trimmed += 1;
    }
    trimmed
}

/// A fully wrapped scrollback (one endless logical line) must not turn a
/// hover into a whole-buffer regex scan.
const MAX_WRAP_SCAN: i32 = 100;

/// Finds URLs, keeping its compiled pattern between lookups.
#[derive(Default)]
pub struct UrlFinder {
    regex: Option<RegexSearch>,
}

impl UrlFinder {
    /// The URL under a viewport cell, with the run to underline on that
    /// row: `(uri, start_col, end_col)`. Detection runs over the logical
    /// (unwrapped) line holding the cell, so wrapped URLs resolve whole.
    pub fn find<U: EventListener>(
        &mut self,
        term: &Crosswords<U>,
        viewport_line: u16,
        col: u16,
    ) -> Option<(String, u16, u16)> {
        let regex = self.regex.get_or_insert_with(|| {
            RegexSearch::new(URL_REGEX).expect("static URL pattern compiles")
        });
        let cols = term.columns();
        let rows = term.screen_lines() as i32;
        let history = term.history_size() as i32;
        let display_offset = term.display_offset() as i32;
        let grid_line = viewport_line as i32 - display_offset;
        if grid_line >= rows || col as usize >= cols {
            return None;
        }
        let point = Pos::new(Line(grid_line), Column(col as usize));

        let wraps = |line: i32| {
            term.grid[Line(line)][Column(cols - 1)]
                .cell_flags()
                .contains(CellFlags::WRAPLINE)
        };
        let mut start_line = grid_line;
        while start_line > -history
            && grid_line - start_line < MAX_WRAP_SCAN
            && wraps(start_line - 1)
        {
            start_line -= 1;
        }
        let mut end_line = grid_line;
        while end_line < rows - 1
            && end_line - grid_line < MAX_WRAP_SCAN
            && wraps(end_line)
        {
            end_line += 1;
        }

        let from = Pos::new(Line(start_line), Column(0));
        let to = Pos::new(Line(end_line), Column(cols - 1));
        for m in RegexIter::new(from, to, Direction::Right, term, regex) {
            let (s, mut e) = (*m.start(), *m.end());
            if point < s {
                break;
            }
            let text = term.bounds_to_string(s, e);
            // The pattern is greedy about trailing punctuation; prose is
            // not: `https://rio.dev,` links without the comma, and a paren
            // only belongs to the URL when it was opened inside it.
            let trim = trailing_url_punctuation(&text);
            for _ in 0..trim {
                if e.col.0 == 0 {
                    e.row.0 -= 1;
                    e.col = Column(cols - 1);
                } else {
                    e.col.0 -= 1;
                }
            }
            if point > e {
                continue;
            }
            let uri: String = {
                let chars = text.chars().count() - trim;
                text.chars().take(chars).collect()
            };
            let start_col = if s.row.0 < grid_line {
                0
            } else {
                s.col.0 as u16
            };
            let end_col = if e.row.0 > grid_line {
                (cols - 1) as u16
            } else {
                e.col.0 as u16
            };
            return Some((uri, start_col, end_col));
        }
        None
    }
}
