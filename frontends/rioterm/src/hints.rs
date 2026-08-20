use rio_backend::config::hints::Hint;
use rio_backend::crosswords::grid::Dimensions;
use rio_backend::crosswords::pos::{Column, Line, Pos};
use rio_backend::crosswords::square::Wide;
use rio_backend::crosswords::Crosswords;
use rio_backend::event::EventListener;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// State for hint selection mode
pub struct HintState {
    /// Currently active hint configuration
    active_hint: Option<Rc<Hint>>,

    /// Visible matches for the current hint
    matches: Vec<HintMatch>,

    /// Labels for each match (as Vec<char>)
    labels: Vec<Vec<char>>,

    /// Keys pressed so far for hint selection
    keys: Vec<char>,

    /// Alphabet for generating labels
    alphabet: String,
}

/// A match found by a hint
#[derive(Debug, Clone)]
pub struct HintMatch {
    /// The text that was matched
    pub text: String,

    /// Start position of the match
    pub start: Pos,

    /// End position of the match
    pub end: Pos,

    /// The hint configuration that created this match
    pub hint: Rc<Hint>,
}

impl HintState {
    pub fn new(alphabet: String) -> Self {
        Self {
            active_hint: None,
            matches: Vec::new(),
            labels: Vec::new(),
            keys: Vec::new(),
            alphabet,
        }
    }

    /// Check if hint mode is active
    pub fn is_active(&self) -> bool {
        self.active_hint.is_some()
    }

    /// Start hint mode with the given hint configuration
    pub fn start(&mut self, hint: Rc<Hint>) {
        self.active_hint = Some(hint);
        self.keys.clear();
        // matches and labels will be updated by update_matches
    }

    /// Stop hint mode
    pub fn stop(&mut self) {
        self.active_hint = None;
        self.matches.clear();
        self.labels.clear();
        self.keys.clear();
    }

    /// Update visible matches for the current hint
    pub fn update_matches<T: EventListener>(
        &mut self,
        term: &rio_backend::crosswords::Crosswords<T>,
    ) {
        self.matches.clear();

        let hint = match &self.active_hint {
            Some(hint) => hint.clone(),
            None => {
                return;
            }
        };

        // Find regex matches if regex is specified
        if let Some(regex_pattern) = &hint.regex {
            if let Ok(regex) = onig::Regex::new(regex_pattern) {
                self.find_regex_matches(term, &regex, hint.clone());
            }
        }

        // Find OSC 8 hyperlinks if enabled
        if hint.hyperlinks {
            self.find_hyperlink_matches(term, hint.clone());
        }

        // Cancel hint mode if no matches found
        if self.matches.is_empty() {
            self.stop();
            return;
        }

        // Sort and dedup matches
        self.matches.sort_by_key(|m| (m.start.row, m.start.col));
        self.matches.dedup_by_key(|m| m.start);

        // Generate labels for matches
        self.generate_labels();
    }

    /// Handle keyboard input during hint selection
    pub fn keyboard_input<T: EventListener>(
        &mut self,
        term: &rio_backend::crosswords::Crosswords<T>,
        c: char,
    ) -> Option<HintMatch> {
        match c {
            // Use backspace to remove the last character pressed
            '\x08' | '\x1f' => {
                self.keys.pop();
                // Only update matches after backspace to regenerate visible labels
                self.update_matches(term);
                return None;
            }
            // Cancel hint highlighting on ESC/Ctrl+c
            '\x1b' | '\x03' => {
                self.stop();
                return None;
            }
            _ => (),
        }

        let hint = self.active_hint.as_ref()?;

        // Get visible labels (labels filtered by keys pressed so far)
        let visible_labels = self.visible_labels();

        // Find the last label starting with the input character
        let mut matching_labels = visible_labels.iter().rev();
        let (index, remaining_label) = matching_labels
            .find(|(_, remaining)| !remaining.is_empty() && remaining[0] == c)?;

        // Check if this completes the label (only one character remaining)
        if remaining_label.len() == 1 {
            let hint_match = self.matches.get(*index)?.clone();
            let hint_config = hint.clone();

            // Exit hint mode unless it requires explicit dismissal
            if hint_config.persist {
                self.keys.clear();
            } else {
                self.stop();
            }

            Some(hint_match)
        } else {
            // Store character to preserve the selection
            self.keys.push(c);
            None
        }
    }

    /// Get current matches
    pub fn matches(&self) -> &[HintMatch] {
        &self.matches
    }

    /// Get keys pressed so far
    #[allow(dead_code)]
    pub fn keys_pressed(&self) -> &[char] {
        &self.keys
    }

    /// Get visible labels (filtered by current input)
    pub fn visible_labels(&self) -> Vec<(usize, Vec<char>)> {
        let keys_len = self.keys.len();
        self.labels
            .iter()
            .enumerate()
            .filter_map(|(i, label)| {
                if label.len() >= keys_len && label[..keys_len] == self.keys[..] {
                    let remaining: Vec<char> = label[keys_len..].to_vec();
                    Some((i, remaining))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Update the alphabet used for hint labels
    #[allow(dead_code)]
    pub fn update_alphabet(&mut self, alphabet: &str) {
        if self.alphabet != alphabet {
            self.alphabet = alphabet.to_string();
            self.keys.clear();
        }
    }

    // Private helper methods

    fn find_regex_matches<T: EventListener>(
        &mut self,
        term: &rio_backend::crosswords::Crosswords<T>,
        regex: &onig::Regex,
        hint: Rc<Hint>,
    ) {
        // Get the visible area of the terminal
        let grid = &term.grid;
        let display_offset = grid.display_offset();
        let visible_lines = grid.screen_lines();

        // Scan each visible line for matches
        for line_idx in 0..visible_lines {
            let line = Line(line_idx as i32 - display_offset as i32);
            if line < Line(0) || line.0 >= grid.total_lines() as i32 {
                continue;
            }

            // Extract text from the line
            let line_text = self.extract_line_text(term, line);

            // Find all matches in this line. Onig yields (byte_start, byte_end);
            for (start, end) in regex.find_iter(&line_text) {
                let start_col = Column(line_text[..start].chars().count());
                let mut match_text = line_text[start..end].to_string();

                // Apply post-processing if enabled
                if hint.post_processing {
                    match_text = post_process_hyperlink_uri(&match_text);
                }

                let end_col =
                    Column(start_col.0 + match_text.chars().count().saturating_sub(1));

                let hint_match = HintMatch {
                    text: match_text,
                    start: Pos::new(line, start_col),
                    end: Pos::new(line, end_col),
                    hint: hint.clone(),
                };

                self.matches.push(hint_match);
            }
        }
    }

    fn find_hyperlink_matches<T: EventListener>(
        &mut self,
        term: &rio_backend::crosswords::Crosswords<T>,
        hint: Rc<Hint>,
    ) {
        // Walk the visible region looking for OSC 8 hyperlink spans.
        //
        // Spans are found by comparing the hyperlink itself, not the
        // cell's `extras_id`: extras slots are interned by content, so
        // a cell that also carries combining marks holds a different
        // slot than its neighbors while belonging to the same link.
        // `Hyperlink` is an `Arc` around (id, uri); the comparison is
        // content equality, matching the OSC 8 `id=` semantics.
        let grid = &term.grid;
        let display_offset = grid.display_offset();
        let visible_lines = grid.screen_lines();

        for line_idx in 0..visible_lines {
            let line = Line(line_idx as i32 - display_offset as i32);
            if line < Line(0) || line.0 >= grid.total_lines() as i32 {
                continue;
            }

            let mut col = 0usize;
            let cols = grid.columns();
            while col < cols {
                let link = match term.cell_hyperlink(line, Column(col)) {
                    Some(link) => link,
                    None => {
                        col += 1;
                        continue;
                    }
                };

                // Found the start of a hyperlink span. Walk forward
                // while the cells carry the same hyperlink.
                let start_col = col;
                let mut end_col = col;
                while end_col < cols
                    && term.cell_hyperlink(line, Column(end_col)).as_ref() == Some(&link)
                {
                    end_col += 1;
                }

                // Look up the URI once for the whole span.
                if let Some(hyperlink) = term.cell_hyperlink(line, Column(start_col)) {
                    let mut uri = hyperlink.uri().to_string();
                    if hint.post_processing {
                        uri = post_process_hyperlink_uri(&uri);
                    }
                    self.matches.push(HintMatch {
                        text: uri,
                        start: Pos::new(line, Column(start_col)),
                        end: Pos::new(line, Column(end_col - 1)),
                        hint: hint.clone(),
                    });
                }

                col = end_col;
            }
        }
    }

    fn extract_line_text<T: EventListener>(
        &self,
        term: &rio_backend::crosswords::Crosswords<T>,
        line: Line,
    ) -> String {
        let grid = &term.grid;
        let mut text = String::new();

        for col in 0..grid.columns() {
            let cell = &grid[line][Column(col)];
            text.push(cell.c());
        }

        text.trim_end().to_string()
    }

    fn generate_labels(&mut self) {
        self.labels.clear();
        let n = self.matches.len();
        let mut generator = LabelGenerator::new(&self.alphabet, n);

        for _ in 0..n {
            self.labels.push(generator.next());
        }
    }
}

/// Generates hint labels using the specified alphabet
struct LabelGenerator {
    labels: Vec<Vec<char>>,
    index: usize,
}

impl LabelGenerator {
    fn new(alphabet: &str, n_labels: usize) -> Self {
        let alphabet: Vec<char> = alphabet.chars().collect();
        let alphabet_len = alphabet.len();

        // Initially just populate the labels with the alphabet.
        // If the number of requested labels is smaller than the alphabet size, then
        // these single-character labels will be sufficient.
        let mut labels: Vec<Vec<char>> = Vec::with_capacity(alphabet_len.min(n_labels));
        for c in alphabet.iter() {
            if labels.len() >= n_labels {
                break;
            } else {
                labels.push(vec![*c]);
            }
        }

        // If the number of labels is larger than the alphabet size, then we need to
        // widen the labels to more than just one character wide.
        // We take care to make sure that when we add two-character labels, we remove
        // the associated single-character label.  For example, if we add the two-character
        // labels "ja" and "jb" and "jc" we make sure to first remove the single-character
        // label "j" because if we don't remove "j" then the user will never be able to actually
        // select the object with label "j" because the hint engine will still be
        // trying to match against "ja" or "jb" etc.
        // If necessary, we continue to three-character labels, and so on.
        while labels.len() < n_labels {
            for i in (0..labels.len()).rev() {
                // Get the label that we are replacing
                let parent_label = &(labels[i]);

                // Create the list of labels that will be replacing the given label,
                // where each new label is the same as the original label but with
                // another character added
                let mut children: Vec<Vec<char>> = Vec::with_capacity(alphabet_len);
                for c in alphabet.iter() {
                    let mut label = parent_label.clone();
                    label.push(*c);
                    children.push(label);
                }

                // Replace the label with the new labels we just generated
                labels.splice(i..i + 1, children);

                // Check whether we now have enough labels
                if labels.len() >= n_labels {
                    break;
                }
            }
        }

        // Reverse the order of the labels so that the "simpler" ones (shorter labels and
        // labels that use characters earlier in the alphabet) come first.  This is done because
        // the user is probably more likely to want to select the label that is lower
        // on the screen.
        labels.truncate(n_labels);
        labels.reverse();

        Self { labels, index: 0 }
    }

    fn next(&mut self) -> Vec<char> {
        let label = self.labels[self.index].clone();
        self.index += 1;
        label
    }
}

/// A regex match resolved against the grid: inclusive cell bounds plus
/// the matched text.
#[derive(Debug, PartialEq, Eq)]
pub struct GridMatch {
    pub start: Pos,
    pub end: Pos,
    pub text: String,
}

/// The logical (unwrapped) line under a point: its text plus the source
/// cell of every byte, so regex byte offsets anchor back to cells.
///
/// Extraction is separate from matching so a probe with several hint
/// rules extracts once and runs every regex against the same buffer.
pub struct LogicalLine {
    text: String,
    map: Vec<Pos>,
}

impl LogicalLine {
    /// Extract the logical line containing `point`, following soft
    /// wraps in both directions.
    pub fn extract<T: EventListener>(term: &Crosswords<T>, point: Pos) -> Option<Self> {
        /// How many cells of the logical line are followed across soft
        /// wraps on each side of the hovered row. Only matches
        /// containing the hovered cell matter, so nothing hoverable is
        /// lost for matches shorter than this. An unbounded scan is
        /// quadratic in oniguruma on a wrapped wall of word characters
        /// (the URL pattern rescans ahead at every start position),
        /// measured at ~90ms per probe; bounded it stays single-digit ms.
        const SCAN_CELLS: usize = 2048;

        let cols = term.columns();
        let topmost = -(term.history_size() as i32);
        let bottommost = term.bottommost_line().0;
        if cols == 0
            || point.row.0 < topmost
            || point.row.0 > bottommost
            || point.col.0 >= cols
        {
            return None;
        }

        let wraps = |line: i32| term.grid[Line(line)][Column(cols - 1)].wrapline();
        let max_rows = (SCAN_CELLS / cols).max(1) as i32;

        let mut start_line = point.row.0;
        while start_line > topmost
            && point.row.0 - start_line < max_rows
            && wraps(start_line - 1)
        {
            start_line -= 1;
        }
        let mut end_line = point.row.0;
        while end_line < bottommost
            && end_line - point.row.0 < max_rows
            && wraps(end_line)
        {
            end_line += 1;
        }

        // Record the source cell of every byte. Wide-char spacers are
        // skipped so the text holds each character once; blank (`\0`)
        // cells read as spaces, matching what is on screen; zero-width
        // marks are emitted with their base character, every byte
        // mapping back to the same cell.
        let rows = (end_line - start_line + 1) as usize;
        let mut text = String::with_capacity(rows * cols);
        let mut map: Vec<Pos> = Vec::with_capacity(rows * cols);
        for l in start_line..=end_line {
            let line = Line(l);
            for c in 0..cols {
                let col = Column(c);
                let pos = Pos::new(line, col);
                let square = &term.grid[line][col];
                if matches!(square.wide(), Wide::Spacer | Wide::LeadingSpacer) {
                    continue;
                }
                for (i, ch) in term.grid.cell_text(pos).enumerate() {
                    let ch = if i == 0 && ch == '\0' { ' ' } else { ch };
                    text.push(ch);
                    for _ in 0..ch.len_utf8() {
                        map.push(pos);
                    }
                }
            }
        }
        while text.ends_with(' ') {
            text.pop();
            map.pop();
        }

        Some(LogicalLine { text, map })
    }

    /// Find the regex match covering `point` in this line.
    ///
    /// Cell bounds come through the byte-to-cell map, never from byte
    /// offsets used as columns: the previous implementation did the
    /// latter, so any multi-byte character left of the match (a `│`
    /// prefix, CJK, emoji) dragged the hover underline that many bytes
    /// to the right, and its single-row search matched soft-wrapped
    /// URLs truncated or not at all.
    pub fn match_at<T: EventListener>(
        &self,
        term: &Crosswords<T>,
        point: Pos,
        regex: &onig::Regex,
        post_processing: bool,
    ) -> Option<GridMatch> {
        let cols = term.columns();
        let text = &self.text;
        let map = &self.map;

        // Manual search loop instead of `find_iter` so each attempt
        // carries a retry budget. The text is terminal output, i.e.
        // attacker-controlled, the pattern is user-config, and this
        // runs on mouse movement: unbounded backtracking here is a
        // denial of service. Hitting the budget reads as "no more
        // matches".
        const RETRY_LIMIT: u32 = 100_000;

        let mut region = onig::Region::new();
        let mut offset = 0;
        while offset < text.len() {
            region.clear();
            let match_param = onig::MatchParam::default();
            // The in-search limit bounds the whole call, every start
            // position included; the in-match limit the safe wrapper
            // exposes is per attempt, which a long pathological line
            // multiplies by its length. Safety: `as_raw` is a live
            // pointer for the parameter owned just above.
            unsafe {
                onig_sys::onig_set_retry_limit_in_search_of_match_param(
                    match_param.as_raw(),
                    RETRY_LIMIT.into(),
                );
            }
            match regex.search_with_param(
                text.as_str(),
                offset,
                text.len(),
                onig::SearchOptions::SEARCH_OPTION_NONE,
                Some(&mut region),
                match_param,
            ) {
                Ok(Some(_)) => (),
                Ok(None) | Err(_) => break,
            }
            let Some((m_start, m_end)) = region.pos(0) else {
                break;
            };
            // Guard against a stalled loop on an empty match.
            offset = m_end.max(m_start + 1);

            if m_start >= m_end || m_end > map.len() {
                continue;
            }
            let start = map[m_start];
            if point < start {
                // Matches arrive in order; everything further is past
                // the point.
                break;
            }

            let trimmed_len = if post_processing {
                trim_match_tail(&text[m_start..m_end])
            } else {
                m_end - m_start
            };
            if trimmed_len == 0 {
                continue;
            }
            let mut end = map[m_start + trimmed_len - 1];
            if point > end {
                continue;
            }

            // A match ending on a wide character owns its spacer cell
            // too, so the hover underline covers the full glyph.
            if end.col.0 + 1 < cols && term.grid[end.row][end.col].wide() == Wide::Wide {
                end.col = Column(end.col.0 + 1);
            }

            return Some(GridMatch {
                start,
                end,
                text: text[m_start..m_start + trimmed_len].to_string(),
            });
        }

        None
    }
}

/// How many leading bytes of a regex match survive post-processing: an
/// unmatched `)` or `]` ends the match, then trailing prose delimiters
/// are dropped. Same rules the grid-walking `hint_post_processing` used,
/// applied to the matched text instead of cells.
fn trim_match_tail(text: &str) -> usize {
    let mut open_parens = 0i32;
    let mut open_brackets = 0i32;
    let mut cut = text.len();
    for (idx, ch) in text.char_indices() {
        match ch {
            '(' => open_parens += 1,
            '[' => open_brackets += 1,
            ')' => {
                if open_parens == 0 {
                    cut = idx;
                    break;
                }
                open_parens -= 1;
            }
            ']' => {
                if open_brackets == 0 {
                    cut = idx;
                    break;
                }
                open_brackets -= 1;
            }
            _ => (),
        }
    }
    text[..cut]
        .trim_end_matches(['.', ',', ':', ';', '?', '!', '(', '[', '\''])
        .len()
}

/// URI scheme prefixes that should never be resolved as file paths.
/// Matches the scheme branch of `DEFAULT_URL_REGEX`.
const URI_SCHEMES: &[&str] = &[
    "ipfs:",
    "ipns:",
    "magnet:",
    "mailto:",
    "gemini://",
    "gopher://",
    "https://",
    "http://",
    "news:",
    "file:",
    "git://",
    "ssh:",
    "ssh://",
    "ftp://",
    "tel:",
];

/// If `text` looks like a local filesystem path, resolve it against `cwd` and
/// return the absolute path when it exists on disk. Returns `None` for
/// URL-scheme strings, paths that don't exist, or anything we can't resolve
/// (e.g. relative path with no known `cwd`). On `None`, the caller should
/// fall back to the raw text and let the OS opener handle it.
///
/// Modelled on ghostty's `resolvePathForOpening` (`src/Surface.zig:2045`).
/// core only joins relative paths against the OSC 7 cwd; tilde
/// expansion lives in the macOS apprt's Swift `openURL`
/// (`.App.swift:715`, via `NSString.standardizingPath`), so `~/x`
/// works on macOS but isn't expanded on Linux/BSD where `xdg-open` gets the
/// literal `~`. Rio doesn't have a per-platform apprt layer, so we do the
/// expansion here to get consistent cross-platform behaviour:
///
/// 1. `~/x` and `~` expand via `dirs::home_dir()`.
/// 2. `$VAR/x` expands via `std::env::var` (ghostty doesn't do this on any
///    platform).
/// 3. Strings starting with a known URI scheme are rejected up front so the
///    OS opener routes them as URLs (saves one filesystem syscall vs
///    ghostty's "join cwd + stat → fail" path).
/// 4. Absolute paths are existence-checked too. short-circuits
///    absolute paths to `None` (caller passes raw); user-visible behaviour
///    is the same since the raw and resolved strings match.
pub fn resolve_path_for_opening(text: &str, cwd: Option<&Path>) -> Option<PathBuf> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }

    // Scheme URLs are not paths — let the OS opener route them.
    if URI_SCHEMES.iter().any(|s| text.starts_with(s)) {
        return None;
    }

    // Expand a recognized path prefix. Anything falling through is treated as
    // a bare relative path (e.g. `src/main.rs`).
    let expanded: PathBuf = if let Some(rest) = text.strip_prefix("~/") {
        dirs::home_dir()?.join(rest)
    } else if text == "~" {
        dirs::home_dir()?
    } else if let Some(rest) = text.strip_prefix('$') {
        let (var_name, tail) = rest.split_once('/').unwrap_or((rest, ""));
        if var_name.is_empty() {
            return None;
        }
        let value = std::env::var(var_name).ok()?;
        let base = PathBuf::from(value);
        if tail.is_empty() {
            base
        } else {
            base.join(tail)
        }
    } else {
        PathBuf::from(text)
    };

    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        cwd?.join(expanded)
    };

    if absolute.exists() {
        Some(absolute)
    } else {
        None
    }
}

/// Apply post-processing to hyperlink URIs (same as in screen/mod.rs)
fn post_process_hyperlink_uri(uri: &str) -> String {
    let chars: Vec<char> = uri.chars().collect();
    if chars.is_empty() {
        return String::new();
    }

    let mut end_idx = chars.len() - 1;
    let mut open_parents = 0;
    let mut open_brackets = 0;

    // First pass: handle uneven brackets/parentheses
    for (i, &c) in chars.iter().enumerate() {
        match c {
            '(' => open_parents += 1,
            '[' => open_brackets += 1,
            ')' => {
                if open_parents == 0 {
                    // Unmatched closing parenthesis, truncate here
                    end_idx = i.saturating_sub(1);
                    break;
                } else {
                    open_parents -= 1;
                }
            }
            ']' => {
                if open_brackets == 0 {
                    // Unmatched closing bracket, truncate here
                    end_idx = i.saturating_sub(1);
                    break;
                } else {
                    open_brackets -= 1;
                }
            }
            _ => (),
        }
    }

    // Second pass: remove trailing delimiters
    while end_idx > 0 {
        match chars[end_idx] {
            '.' | ',' | ':' | ';' | '?' | '!' | '(' | '[' | '\'' => {
                end_idx = end_idx.saturating_sub(1);
            }
            _ => break,
        }
    }

    chars.into_iter().take(end_idx + 1).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rio_backend::config::hints::{HintAction, HintInternalAction};

    #[test]
    fn test_label_generator() {
        let mut gen_abc3 = LabelGenerator::new("abc", 3);
        assert_eq!(gen_abc3.next(), vec!['c']);
        assert_eq!(gen_abc3.next(), vec!['b']);
        assert_eq!(gen_abc3.next(), vec!['a']);

        let mut gen_abc4 = LabelGenerator::new("abc", 4);
        assert_eq!(gen_abc4.next(), vec!['c', 'b']);
        assert_eq!(gen_abc4.next(), vec!['c', 'a']);
        assert_eq!(gen_abc4.next(), vec!['b']);
        assert_eq!(gen_abc4.next(), vec!['a']);

        let mut gen_abc6 = LabelGenerator::new("abc", 6);
        assert_eq!(gen_abc6.next(), vec!['c', 'b']);
        assert_eq!(gen_abc6.next(), vec!['c', 'a']);
        assert_eq!(gen_abc6.next(), vec!['b', 'c']);
        assert_eq!(gen_abc6.next(), vec!['b', 'b']);
        assert_eq!(gen_abc6.next(), vec!['b', 'a']);
        assert_eq!(gen_abc6.next(), vec!['a']);

        let mut gen_abc7 = LabelGenerator::new("abc", 7);
        assert_eq!(gen_abc7.next(), vec!['c', 'c']);
        assert_eq!(gen_abc7.next(), vec!['c', 'b']);
        assert_eq!(gen_abc7.next(), vec!['c', 'a']);
        assert_eq!(gen_abc7.next(), vec!['b', 'c']);
        assert_eq!(gen_abc7.next(), vec!['b', 'b']);
        assert_eq!(gen_abc7.next(), vec!['b', 'a']);
        assert_eq!(gen_abc7.next(), vec!['a']);

        let mut gen_abc8 = LabelGenerator::new("abc", 8);
        assert_eq!(gen_abc8.next(), vec!['c', 'b']);
        assert_eq!(gen_abc8.next(), vec!['c', 'a']);
        assert_eq!(gen_abc8.next(), vec!['b', 'c']);
        assert_eq!(gen_abc8.next(), vec!['b', 'b']);
        assert_eq!(gen_abc8.next(), vec!['b', 'a']);
        assert_eq!(gen_abc8.next(), vec!['a', 'c']);
        assert_eq!(gen_abc8.next(), vec!['a', 'b']);
        assert_eq!(gen_abc8.next(), vec!['a', 'a']);

        let mut gen_abc11 = LabelGenerator::new("abc", 11);
        assert_eq!(gen_abc11.next(), vec!['c', 'c', 'c']);
        assert_eq!(gen_abc11.next(), vec!['c', 'c', 'b']);
        assert_eq!(gen_abc11.next(), vec!['c', 'c', 'a']);
        assert_eq!(gen_abc11.next(), vec!['c', 'b']);
        assert_eq!(gen_abc11.next(), vec!['c', 'a']);
        assert_eq!(gen_abc11.next(), vec!['b', 'c']);
        assert_eq!(gen_abc11.next(), vec!['b', 'b']);
        assert_eq!(gen_abc11.next(), vec!['b', 'a']);
        assert_eq!(gen_abc11.next(), vec!['a', 'c']);
        assert_eq!(gen_abc11.next(), vec!['a', 'b']);
        assert_eq!(gen_abc11.next(), vec!['a', 'a']);

        let mut gen_abc12 = LabelGenerator::new("abc", 12);
        assert_eq!(gen_abc12.next(), vec!['c', 'c', 'b']);
        assert_eq!(gen_abc12.next(), vec!['c', 'c', 'a']);
        assert_eq!(gen_abc12.next(), vec!['c', 'b', 'c']);
        assert_eq!(gen_abc12.next(), vec!['c', 'b', 'b']);
        assert_eq!(gen_abc12.next(), vec!['c', 'b', 'a']);
        assert_eq!(gen_abc12.next(), vec!['c', 'a']);
        assert_eq!(gen_abc12.next(), vec!['b', 'c']);
        assert_eq!(gen_abc12.next(), vec!['b', 'b']);
        assert_eq!(gen_abc12.next(), vec!['b', 'a']);
        assert_eq!(gen_abc12.next(), vec!['a', 'c']);
        assert_eq!(gen_abc12.next(), vec!['a', 'b']);
        assert_eq!(gen_abc12.next(), vec!['a', 'a']);

        let mut gen_ab7 = LabelGenerator::new("ab", 7);
        assert_eq!(gen_ab7.next(), vec!['b', 'b', 'b']);
        assert_eq!(gen_ab7.next(), vec!['b', 'b', 'a']);
        assert_eq!(gen_ab7.next(), vec!['b', 'a', 'b']);
        assert_eq!(gen_ab7.next(), vec!['b', 'a', 'a']);
        assert_eq!(gen_ab7.next(), vec!['a', 'b', 'b']);
        assert_eq!(gen_ab7.next(), vec!['a', 'b', 'a']);
        assert_eq!(gen_ab7.next(), vec!['a', 'a']);
    }

    #[test]
    fn test_hint_state_lifecycle() {
        let mut state = HintState::new("abc".to_string());
        assert!(!state.is_active());

        let hint = Rc::new(Hint {
            regex: Some("test".to_string()),
            hyperlinks: false,
            post_processing: true,
            persist: false,
            action: HintAction::Action {
                action: HintInternalAction::Copy,
            },
            mouse: Default::default(),
            binding: None,
        });

        state.start(hint);
        assert!(state.is_active());

        state.stop();
        assert!(!state.is_active());
    }

    #[test]
    fn test_visible_labels() {
        let mut state = HintState::new("abc".to_string());
        state.labels = vec![vec!['a'], vec!['b'], vec!['a', 'b'], vec!['a', 'c']];

        // No input - all labels visible
        let visible = state.visible_labels();
        assert_eq!(visible.len(), 4);

        // Input "a" - should show labels that start with "a"
        state.keys = vec!['a'];
        let visible = state.visible_labels();
        assert_eq!(visible.len(), 3); // "a", "ab", "ac"
        assert_eq!(visible[0].1, Vec::<char>::new()); // "a" with "a" removed = []
        assert_eq!(visible[1].1, vec!['b']); // "ab" with "a" removed = ['b']
        assert_eq!(visible[2].1, vec!['c']); // "ac" with "a" removed = ['c']
    }

    #[test]
    fn test_keyboard_input_logic() {
        let mut state = HintState::new("jfkdls".to_string());

        // Simulate having some labels
        state.labels = vec![
            vec!['j'], // index 0
            vec!['f'], // index 1
            vec!['k'], // index 2
            vec!['d'], // index 3
            vec!['l'], // index 4
            vec!['s'], // index 5
        ];

        // Simulate having matches (we'll use dummy matches)
        state.matches = vec![
            HintMatch {
                text: "match0".to_string(),
                start: rio_backend::crosswords::pos::Pos::new(
                    rio_backend::crosswords::pos::Line(0),
                    rio_backend::crosswords::pos::Column(0),
                ),
                end: rio_backend::crosswords::pos::Pos::new(
                    rio_backend::crosswords::pos::Line(0),
                    rio_backend::crosswords::pos::Column(5),
                ),
                hint: Rc::new(Hint {
                    regex: Some("test".to_string()),
                    hyperlinks: false,
                    post_processing: true,
                    persist: false,
                    action: HintAction::Action {
                        action: HintInternalAction::Copy,
                    },
                    mouse: Default::default(),
                    binding: None,
                }),
            },
            HintMatch {
                text: "match1".to_string(),
                start: rio_backend::crosswords::pos::Pos::new(
                    rio_backend::crosswords::pos::Line(0),
                    rio_backend::crosswords::pos::Column(10),
                ),
                end: rio_backend::crosswords::pos::Pos::new(
                    rio_backend::crosswords::pos::Line(0),
                    rio_backend::crosswords::pos::Column(15),
                ),
                hint: Rc::new(Hint {
                    regex: Some("test".to_string()),
                    hyperlinks: false,
                    post_processing: true,
                    persist: false,
                    action: HintAction::Action {
                        action: HintInternalAction::Copy,
                    },
                    mouse: Default::default(),
                    binding: None,
                }),
            },
        ];

        let hint = Rc::new(Hint {
            regex: Some("test".to_string()),
            hyperlinks: false,
            post_processing: true,
            persist: false,
            action: HintAction::Action {
                action: HintInternalAction::Copy,
            },
            mouse: Default::default(),
            binding: None,
        });

        state.active_hint = Some(hint);

        // Test keyboard input logic without needing a terminal
        // Test that 'j' should match the first label
        let mut test_keys = state.keys.clone();
        test_keys.push('j');

        let mut matching_indices = Vec::new();
        for (i, label) in state.labels.iter().enumerate() {
            if label.len() >= test_keys.len() && label[..test_keys.len()] == test_keys[..]
            {
                matching_indices.push(i);
            }
        }

        assert!(
            !matching_indices.is_empty(),
            "Should find matching labels for 'j'"
        );
        assert_eq!(matching_indices, vec![0], "Should match index 0 for 'j'");

        // Test that the label should be completed (single character)
        let index = *matching_indices.last().unwrap();
        let label = &state.labels[index];
        assert_eq!(
            label.len(),
            test_keys.len(),
            "Label should be completed with single character"
        );
    }

    #[test]
    fn test_resolve_path_skips_scheme_urls() {
        assert!(resolve_path_for_opening("https://example.com", None).is_none());
        assert!(resolve_path_for_opening("mailto:a@b.c", None).is_none());
        assert!(resolve_path_for_opening("file:///tmp", None).is_none());
        assert!(resolve_path_for_opening("ssh://host/path", None).is_none());
    }

    #[test]
    fn test_resolve_path_returns_none_when_nonexistent() {
        let cwd = std::env::temp_dir();
        assert!(resolve_path_for_opening(
            "rio-definitely-does-not-exist-xyz",
            Some(&cwd)
        )
        .is_none());
        assert!(resolve_path_for_opening(
            "./rio-definitely-does-not-exist-xyz",
            Some(&cwd)
        )
        .is_none());
    }

    #[test]
    fn test_resolve_path_absolute_existing_file() {
        let tmp = std::env::temp_dir();
        let file = tmp.join("rio-test-resolve-abs.txt");
        std::fs::write(&file, "hi").unwrap();

        let resolved = resolve_path_for_opening(&file.to_string_lossy(), None).unwrap();
        // PathBuf::exists() follows symlinks; on macOS /tmp is a symlink to
        // /private/tmp, so compare existence rather than exact paths.
        assert!(resolved.exists());

        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn test_resolve_path_relative_joined_with_cwd() {
        let tmp = std::env::temp_dir();
        let subdir = tmp.join("rio-test-resolve-dir");
        std::fs::create_dir_all(&subdir).unwrap();
        let file = subdir.join("child.txt");
        std::fs::write(&file, "hi").unwrap();

        let resolved = resolve_path_for_opening("child.txt", Some(&subdir)).unwrap();
        assert!(resolved.exists());
        assert!(resolved.ends_with("child.txt"));

        let _ = std::fs::remove_file(&file);
        let _ = std::fs::remove_dir(&subdir);
    }

    #[test]
    fn test_resolve_path_dot_relative_joined_with_cwd() {
        let tmp = std::env::temp_dir();
        let subdir = tmp.join("rio-test-resolve-dot-dir");
        std::fs::create_dir_all(&subdir).unwrap();
        let file = subdir.join("dot-child.txt");
        std::fs::write(&file, "hi").unwrap();

        let resolved =
            resolve_path_for_opening("./dot-child.txt", Some(&subdir)).unwrap();
        assert!(resolved.exists());

        let _ = std::fs::remove_file(&file);
        let _ = std::fs::remove_dir(&subdir);
    }

    #[test]
    fn test_resolve_path_requires_cwd_for_relative() {
        // With no cwd and a relative path, we can't resolve; return None.
        assert!(resolve_path_for_opening("foo/bar.txt", None).is_none());
    }

    #[test]
    fn test_resolve_path_expands_env_var() {
        let tmp = std::env::temp_dir();
        // Safety: setting an env var inside a process-local test. This is
        // unsafe in Rust 2024; rio-backend uses an earlier edition so it's
        // permitted here. If rio moves to 2024 this test needs adjustment.
        unsafe {
            std::env::set_var("RIO_TEST_PATH_VAR", tmp.to_string_lossy().to_string());
        }

        let file = tmp.join("rio-test-env-var.txt");
        std::fs::write(&file, "hi").unwrap();

        let resolved =
            resolve_path_for_opening("$RIO_TEST_PATH_VAR/rio-test-env-var.txt", None)
                .unwrap();
        assert!(resolved.exists());

        let _ = std::fs::remove_file(&file);
        unsafe {
            std::env::remove_var("RIO_TEST_PATH_VAR");
        }
    }

    use rio_backend::ansi::CursorShape;
    use rio_backend::config::hints::DEFAULT_URL_REGEX;
    use rio_backend::crosswords::CrosswordsSize;
    use rio_backend::event::{VoidListener, WindowId};
    use rio_unicode::UnicodeWidthChar;

    /// Build a terminal from literal content, the same way rio-vt's
    /// search tests do: `\n` continues a soft-wrapped line, `\r\n` is a
    /// hard line break. Soft-wrapped rows must be written full width,
    /// as they are in a live grid.
    fn mock_term(content: &str) -> Crosswords<VoidListener> {
        let lines: Vec<&str> = content.split('\n').collect();
        let num_cols = lines
            .iter()
            .map(|line| {
                line.chars()
                    .filter(|c| *c != '\r')
                    .map(|c| c.width().unwrap())
                    .sum()
            })
            .max()
            .unwrap_or(0);

        let size = CrosswordsSize::new(num_cols, lines.len());
        let mut term = Crosswords::new(
            size,
            CursorShape::Block,
            VoidListener {},
            WindowId::from(0),
            0,
            10_000,
        );

        for (line, text) in lines.iter().enumerate() {
            let line = Line(line as i32);
            if !text.ends_with('\r') && line + 1 != lines.len() {
                term.grid[line][Column(num_cols - 1)].set_wrapline(true);
            }

            let mut index = 0;
            for c in text.chars().take_while(|c| *c != '\r') {
                term.grid[line][Column(index)].set_c(c);
                let width = c.width().unwrap();
                if width == 2 {
                    term.grid[line][Column(index)].set_wide(Wide::Wide);
                    term.grid[line][Column(index + 1)].set_wide(Wide::Spacer);
                }
                index += width;
            }
        }

        term
    }

    fn url_regex() -> onig::Regex {
        onig::Regex::new(DEFAULT_URL_REGEX).unwrap()
    }

    fn match_at(
        term: &Crosswords<VoidListener>,
        row: i32,
        col: usize,
    ) -> Option<GridMatch> {
        let point = Pos::new(Line(row), Column(col));
        LogicalLine::extract(term, point)?.match_at(term, point, &url_regex(), true)
    }

    // Hovering anywhere on a mid-line URL yields exactly the URL's
    // cells, and the parenthesized note after it stays prose.
    #[test]
    fn test_match_at_point_plain_ascii_line() {
        let term = mock_term(
            "Dev server is listening at http://localhost:1313/ (bind address 127.0.0.1)",
        );

        // "Dev server is listening at " is 27 cells; the URL is 22 more.
        for col in [27, 35, 48] {
            let m = match_at(&term, 0, col)
                .unwrap_or_else(|| panic!("no match at col {col}"));
            assert_eq!(m.text, "http://localhost:1313/");
            assert_eq!(m.start, Pos::new(Line(0), Column(27)));
            assert_eq!(m.end, Pos::new(Line(0), Column(48)));
        }
        assert!(match_at(&term, 0, 26).is_none(), "space before the url");
        assert!(match_at(&term, 0, 50).is_none(), "note after the url");
    }

    // Multi-byte characters left of the match must not drag the bounds
    // to the right. The old byte-offset code returned cols shifted by
    // one per extra UTF-8 byte: two box-drawing characters shifted the
    // hover underline four cells.
    #[test]
    fn test_match_at_point_multibyte_prefix() {
        let term = mock_term("\u{2502}\u{2502} see http://a.b/c after");

        let m = match_at(&term, 0, 10).expect("hover on the url");
        assert_eq!(m.text, "http://a.b/c");
        assert_eq!(m.start, Pos::new(Line(0), Column(7)));
        assert_eq!(m.end, Pos::new(Line(0), Column(18)));
    }

    // Wide characters occupy two cells but one char: bounds are cells,
    // not chars, and hovering the spacer half still hits.
    #[test]
    fn test_match_at_point_wide_prefix() {
        let term = mock_term("\u{65e5}\u{672c} http://a.b/c");

        let m = match_at(&term, 0, 8).expect("hover on the url");
        assert_eq!(m.text, "http://a.b/c");
        assert_eq!(m.start, Pos::new(Line(0), Column(5)));
        assert_eq!(m.end, Pos::new(Line(0), Column(16)));
    }

    // A soft-wrapped URL matches whole from either row, with bounds
    // spanning the wrap. The old single-row code matched a truncated
    // URL on the first row and nothing on the second.
    #[test]
    fn test_match_at_point_wrapped_url() {
        let term = mock_term("see http://examp\nle.com/path here");

        let from_first = match_at(&term, 0, 6).expect("hover on first row");
        assert_eq!(from_first.text, "http://example.com/path");
        assert_eq!(from_first.start, Pos::new(Line(0), Column(4)));
        assert_eq!(from_first.end, Pos::new(Line(1), Column(10)));

        let from_second = match_at(&term, 1, 3).expect("hover on second row");
        assert_eq!(from_second, from_first);

        assert!(match_at(&term, 1, 13).is_none(), "prose after the url");
    }

    // A URL deep inside a huge fully-wrapped logical line still
    // resolves with exact bounds: the extraction clips to a byte
    // window around the point, and the window must land on the match.
    #[test]
    fn test_match_at_point_clipped_long_line() {
        let url = "http://example.com/path";
        let mut rows: Vec<String> = (0..201).map(|_| "x".repeat(300)).collect();
        rows[100] = format!(
            "{} {} {}",
            "x".repeat(99),
            url,
            "x".repeat(300 - 99 - url.len() - 2)
        );
        let term = mock_term(&rows.join("\n"));

        let m = match_at(&term, 100, 110).expect("hover on the url");
        assert_eq!(m.text, url);
        assert_eq!(m.start, Pos::new(Line(100), Column(100)));
        assert_eq!(m.end, Pos::new(Line(100), Column(122)));

        assert!(match_at(&term, 100, 95).is_none(), "filler is not a link");
    }

    #[test]
    fn test_trim_match_tail() {
        assert_eq!(trim_match_tail("http://a.b/c"), "http://a.b/c".len());
        // Unmatched closing paren ends the match.
        assert_eq!(trim_match_tail("http://a.b/c)x"), "http://a.b/c".len());
        // Balanced pairs survive.
        assert_eq!(trim_match_tail("http://a.b/(c)"), "http://a.b/(c)".len());
        // Trailing prose delimiters are dropped.
        assert_eq!(trim_match_tail("http://a.b/c,."), "http://a.b/c".len());
    }
}
