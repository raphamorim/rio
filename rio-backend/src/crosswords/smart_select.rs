//! iTerm2-style smart selection.
//!
//! Given a click position, evaluate a list of precision-tagged regex
//! rules against the surrounding logical line and pick the
//! highest-precision match that contains the click. Falls back to
//! `None` when nothing matches, leaving the caller free to use a
//! plainer selection strategy (semantic, word, etc.).

use crate::config::smart_selection::SmartSelectionConfig;
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

/// Owns the compiled rule list and exposes the two operations the
/// rest of the app needs: select-at-click and reload-from-config.
/// Splitting the recompile out of `Screen::update_config` keeps the
/// hot-reload path testable without spinning up a window.
#[derive(Debug)]
pub struct SmartSelector {
    rules: Vec<SmartRule>,
}

impl SmartSelector {
    pub fn new(config: &SmartSelectionConfig) -> Self {
        Self {
            rules: config.compile(),
        }
    }

    /// Replace the rule set with one freshly compiled from `config`.
    /// Called on config reload; cheap relative to a click, but heavy
    /// enough (compiles every regex) to be off the click hot path.
    pub fn reload(&mut self, config: &SmartSelectionConfig) {
        self.rules = config.compile();
    }

    /// Resolve the click against the active rule set.
    pub fn select_at<T: EventListener>(
        &mut self,
        term: &Crosswords<T>,
        click: Pos,
    ) -> Option<SelectionRange> {
        smart_select_at(term, &mut self.rules, click)
    }

    /// Number of active rules. Useful for assertions; not on a hot path.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
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

    // --- reload-path tests (the "hot-reload D" check from the manual
    // sampler, asserted directly so a regression in the reload wiring
    // shows up in CI instead of needing a windowed integration run).
    use crate::config::smart_selection::{SmartSelectionConfig, UserRule};

    fn url_click_setup() -> (Crosswords<VoidListener>, Pos) {
        let mut term = cw(80, 5);
        write(&mut term, b"see https://cli.github.com/manual end");
        // Click on the `g` of `github` (inside the URL).
        let click = Pos::new(Line(0), Column(15));
        (term, click)
    }

    #[test]
    fn selector_picks_up_master_disable_on_reload() {
        let (term, click) = url_click_setup();
        let mut sel = SmartSelector::new(&SmartSelectionConfig::default());
        assert!(
            sel.select_at(&term, click).is_some(),
            "defaults should match the URL",
        );

        sel.reload(&SmartSelectionConfig {
            enabled: false,
            rules: vec![],
        });
        assert!(
            sel.select_at(&term, click).is_none(),
            "after disabling the engine the URL must no longer smart-select",
        );

        // Re-enable and confirm the URL match returns — proves reload
        // isn't a one-shot.
        sel.reload(&SmartSelectionConfig::default());
        assert!(sel.select_at(&term, click).is_some());
    }

    #[test]
    fn selector_picks_up_rule_disable_on_reload() {
        let mut term = cw(80, 5);
        write(&mut term, b"trace src/main.rs:42:7 here");
        let click = Pos::new(Line(0), Column(15));

        let mut sel = SmartSelector::new(&SmartSelectionConfig::default());
        let r = sel.select_at(&term, click).expect("file_line should match");
        // Full `src/main.rs:42:7` (cols 6..=21).
        assert_eq!(r.end, Pos::new(Line(0), Column(21)));

        sel.reload(&SmartSelectionConfig {
            enabled: true,
            rules: vec![UserRule {
                name: "file_line".into(),
                regex: None,
                precision: None,
                enabled: false,
            }],
        });
        // After disabling the file_line rule, no other default matches
        // `src/main.rs:42:7` (URL needs a scheme, ipv4 needs all
        // dotted numbers, etc.) — so smart-select returns None and
        // the click falls through to semantic.
        assert!(sel.select_at(&term, click).is_none());
    }

    #[test]
    fn worst_case_select_stays_under_budget() {
        // The engine has to be cheap enough for a worst-case logical
        // line not to introduce visible click-to-paint latency. Cap
        // the budget at 50 ms — typical is sub-millisecond, so 50× is
        // a regression sentinel (e.g. an accidental per-click DFA
        // recompile, an O(n²) sneaking in) not a tight bound that
        // CI noise would trip.
        use std::time::Instant;

        let cols = MAX_SCAN_CELLS;
        let mut term = cw(cols, 1);

        // Repeating filler that frequently *starts* a match for each
        // built-in rule (hex char → git_sha, digit+dot → ipv4, dot+
        // alnum → file_line, `@` → email) without ever completing,
        // forcing the DFAs to do real work. One real URL sits
        // mid-line so the chosen-match path also executes.
        let chunk = b"abc123.de4@xy5 ";
        let url = b"https://example.com/path";
        let mut body = Vec::with_capacity(cols);
        while body.len() + chunk.len() + url.len() < cols / 2 {
            body.extend_from_slice(chunk);
        }
        let url_start = body.len();
        body.extend_from_slice(url);
        while body.len() + chunk.len() < cols {
            body.extend_from_slice(chunk);
        }
        write(&mut term, &body);

        let mut sel = SmartSelector::new(&SmartSelectionConfig::default());
        let click = Pos::new(Line(0), Column(url_start + url.len() / 2));

        // Warm up — first call may pay one-shot lazy-DFA caching cost.
        let _ = sel.select_at(&term, click);

        let start = Instant::now();
        let r = sel.select_at(&term, click);
        let elapsed = start.elapsed();

        assert!(r.is_some(), "URL should still match on a long line");
        assert!(
            elapsed.as_millis() < 50,
            "smart_select_at took {:?} on a {}-cell line (budget: 50ms)",
            elapsed,
            cols,
        );
    }

    #[test]
    fn selector_picks_up_added_user_rule_on_reload() {
        let mut term = cw(80, 5);
        // Phabricator-style ticket containing a `:` so semantic
        // can't catch it. Without the user rule no built-in matches.
        write(&mut term, b"see T:1234 today");
        let click = Pos::new(Line(0), Column(6));

        let mut sel = SmartSelector::new(&SmartSelectionConfig::default());
        assert!(
            sel.select_at(&term, click).is_none(),
            "no default rule should match a `T:1234` token",
        );

        sel.reload(&SmartSelectionConfig {
            enabled: true,
            rules: vec![UserRule {
                name: "phab".into(),
                regex: Some(r"T:\d+".into()),
                precision: Some(120),
                enabled: true,
            }],
        });
        let r = sel
            .select_at(&term, click)
            .expect("after reload the user rule should match");
        assert_eq!(r.start, Pos::new(Line(0), Column(4)));
        assert_eq!(r.end, Pos::new(Line(0), Column(9)));
    }
}
