use crate::hints::extract_line_text;
use rio_backend::config::triggers::{TriggerAction, Triggers as TriggersConfig};
use rio_backend::crosswords::grid::Dimensions;
use rio_backend::crosswords::pos::{Column, Line, Pos};
use rio_backend::crosswords::search::Match;
use rio_backend::crosswords::Crosswords;
use rio_backend::event::EventListener;
use rustc_hash::{FxHashMap, FxHashSet};
use std::hash::{Hash, Hasher};

/// Longest line (chars) matched against trigger regexes.
const LINE_SCAN_CAP: usize = 4096;

struct CompiledTrigger {
    regex: onig::Regex,
    instant: bool,
    action: TriggerAction,
}

/// Compiled trigger rules plus per-route dedup. Owned on the main thread
/// (`onig::Regex` is `!Send`).
#[derive(Default)]
pub struct Triggers {
    rules: Vec<CompiledTrigger>,
    has_highlight: bool,
    /// Per route, the set of (absolute line, content hash, finalized) we've
    /// already evaluated, so a given line+content fires once. Keyed on
    /// content rather than a cursor counter so prompt redraws and TUIs
    /// (which don't scroll) still register new output.
    seen: FxHashMap<usize, FxHashSet<(i64, u64, bool)>>,
}

/// A one-shot trigger action with captures already substituted.
pub enum ResolvedAction {
    Notify { title: String, body: String },
    TabColor([f32; 4]),
    Run { program: String, args: Vec<String> },
    SendText(String),
    Coprocess { program: String, args: Vec<String> },
}

#[inline]
fn rgba_u8(c: [f32; 4]) -> [u8; 4] {
    [
        (c[0] * 255.0).round() as u8,
        (c[1] * 255.0).round() as u8,
        (c[2] * 255.0).round() as u8,
        (c[3] * 255.0).round() as u8,
    ]
}

#[inline]
fn hash_text(s: &str) -> u64 {
    let mut h = rustc_hash::FxHasher::default();
    s.hash(&mut h);
    h.finish()
}

impl Triggers {
    pub fn new(config: &TriggersConfig) -> Self {
        let mut rules = Vec::with_capacity(config.rules.len());
        for rule in &config.rules {
            match onig::Regex::new(&rule.regex) {
                Ok(regex) => rules.push(CompiledTrigger {
                    regex,
                    instant: rule.instant,
                    action: rule.action.clone(),
                }),
                Err(err) => {
                    tracing::warn!("invalid trigger regex {:?}: {}", rule.regex, err);
                }
            }
        }
        let has_highlight = rules
            .iter()
            .any(|r| matches!(r.action, TriggerAction::Highlight { .. }));
        Self {
            rules,
            has_highlight,
            seen: FxHashMap::default(),
        }
    }

    pub fn rebuild(&mut self, config: &TriggersConfig) {
        *self = Triggers::new(config);
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Match new output against the one-shot rules and return resolved
    /// actions. Scans the live (non-scrolled) screen and dedups by
    /// (line, content) so each line fires once: lines above the cursor are
    /// "finalized" (non-instant rules); the cursor line fires instant rules.
    pub fn scan<T: EventListener>(
        &mut self,
        route_id: usize,
        term: &Crosswords<T>,
    ) -> Vec<ResolvedAction> {
        if self.rules.is_empty() {
            return Vec::new();
        }

        let grid = &term.grid;
        // Only the live bottom — don't re-fire history while scrolled back.
        if grid.display_offset() != 0 {
            return Vec::new();
        }
        let history = grid.history_size() as i64;
        let cursor_row = grid.cursor.pos.row.0 as i64;
        let screen_lines = grid.screen_lines();

        let seen = self.seen.entry(route_id).or_default();
        // Drop lines that have scrolled out of the live view.
        seen.retain(|(abs, _, _)| *abs >= history);

        let mut actions = Vec::new();
        for i in 0..screen_lines {
            let abs = history + i as i64;
            let finalized = (i as i64) < cursor_row;
            let text = extract_line_text(term, Line(i as i32));
            if text.is_empty() {
                continue;
            }
            let text: &str = if text.len() > LINE_SCAN_CAP {
                match text.char_indices().nth(LINE_SCAN_CAP) {
                    Some((byte, _)) => &text[..byte],
                    None => &text,
                }
            } else {
                &text
            };

            // (line, content, phase) — re-evaluated only when the content
            // changes, so unchanged lines cost just a hash lookup.
            if !seen.insert((abs, hash_text(text), finalized)) {
                continue;
            }

            for rule in &self.rules {
                if matches!(rule.action, TriggerAction::Highlight { .. }) {
                    continue;
                }
                // Finalized lines run non-instant rules; the cursor line
                // runs instant rules (prompts with no trailing newline).
                if rule.instant == finalized {
                    continue;
                }
                for caps in rule.regex.captures_iter(text) {
                    actions.push(resolve(&rule.action, &caps));
                }
            }
        }
        actions
    }

    /// Recompute highlight ranges over the visible region. Highlights are a
    /// visual state, re-evaluated each frame so they track the live text.
    pub fn highlights<T: EventListener>(
        &self,
        term: &Crosswords<T>,
    ) -> Vec<(Match, [u8; 4])> {
        if !self.has_highlight {
            return Vec::new();
        }
        let grid = &term.grid;
        let display_offset = grid.display_offset() as i32;
        let topmost = grid.topmost_line().0;
        let mut out = Vec::new();
        for i in 0..grid.screen_lines() {
            let line = Line(i as i32 - display_offset);
            if line.0 < topmost {
                continue;
            }
            let text = extract_line_text(term, line);
            if text.is_empty() {
                continue;
            }
            for rule in &self.rules {
                let TriggerAction::Highlight { color } = &rule.action else {
                    continue;
                };
                let rgba = rgba_u8(*color);
                for caps in rule.regex.captures_iter(&text) {
                    if let Some((start, end)) = span(&text, &caps) {
                        out.push((Pos::new(line, start)..=Pos::new(line, end), rgba));
                    }
                }
            }
        }
        out
    }
}

/// Match span as cell columns (onig reports byte offsets; columns are
/// per-cell, one char each).
fn span(text: &str, caps: &onig::Captures) -> Option<(Column, Column)> {
    let (start_b, end_b) = caps.pos(0)?;
    let start = text[..start_b].chars().count();
    let end = text[..end_b].chars().count().saturating_sub(1);
    Some((Column(start), Column(end.max(start))))
}

fn resolve(action: &TriggerAction, caps: &onig::Captures) -> ResolvedAction {
    match action {
        TriggerAction::Notify { title, body } => ResolvedAction::Notify {
            title: substitute(title, caps),
            body: substitute(body, caps),
        },
        TriggerAction::TabColor { color } => ResolvedAction::TabColor(*color),
        TriggerAction::Run { program, args } => ResolvedAction::Run {
            program: program.clone(),
            args: args.iter().map(|a| substitute(a, caps)).collect(),
        },
        TriggerAction::SendText { text } => {
            ResolvedAction::SendText(substitute(text, caps))
        }
        TriggerAction::Coprocess { program, args } => ResolvedAction::Coprocess {
            program: program.clone(),
            args: args.iter().map(|a| substitute(a, caps)).collect(),
        },
        // Handled by `highlights()`; `scan` skips it.
        TriggerAction::Highlight { .. } => ResolvedAction::SendText(String::new()),
    }
}

/// Expand `\0..\9` (whole match / capture groups) and `\\` in `template`.
fn substitute(template: &str, caps: &onig::Captures) -> String {
    if !template.contains('\\') {
        return template.to_owned();
    }
    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some(d) if d.is_ascii_digit() => {
                let n = (*d as u8 - b'0') as usize;
                chars.next();
                if let Some(group) = caps.at(n) {
                    out.push_str(group);
                }
            }
            Some('\\') => {
                out.push('\\');
                chars.next();
            }
            _ => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps<'a>(re: &str, text: &'a str) -> onig::Captures<'a> {
        onig::Regex::new(re).unwrap().captures(text).unwrap()
    }

    #[test]
    fn substitute_groups() {
        let c = caps(r"error: (\w+) (\w+)", "error: disk full");
        assert_eq!(substitute(r"\0", &c), "error: disk full");
        assert_eq!(substitute(r"\1/\2", &c), "disk/full");
        assert_eq!(substitute(r"\9", &c), "");
        assert_eq!(substitute(r"a\\b", &c), r"a\b");
        assert_eq!(substitute("plain", &c), "plain");
    }
}
