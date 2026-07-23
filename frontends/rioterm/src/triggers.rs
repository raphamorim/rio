use rio_backend::config::triggers::{TriggerAction, Triggers as TriggersConfig};
use rio_backend::crosswords::grid::Dimensions;
use rio_backend::crosswords::pos::{Column, Line, Pos};
use rio_backend::crosswords::search::Match;
use rio_backend::crosswords::square::Wide;
use rio_backend::crosswords::{Crosswords, Mode};
use rio_backend::event::{EventListener, TerminalDamage};
use rustc_hash::{FxHashMap, FxHashSet};
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

/// Longest line (chars) matched against trigger regexes.
const LINE_SCAN_CAP: usize = 4096;

/// Feedback-loop breaker for actions that write back into the pty
/// (send_text / coprocess): the SAME rule may fire at most
/// `WRITE_BURST_MAX` times within `WRITE_BURST_WINDOW`. A legitimate
/// flow (login automation, a burst of [y/n] answers) stays far below the
/// budget and pays zero latency; output produced by the action
/// re-matching its own rule fires at render rate and hits the cap, so a
/// loop is cut off instead of storming the pty. Read-only actions
/// (notify/tab_color/run) are not budgeted.
const WRITE_BURST_WINDOW: Duration = Duration::from_secs(1);
const WRITE_BURST_MAX: u32 = 8;

/// Scrollback lines (above the visible bottom) included when a feed_screen
/// coprocess captures the screen, so a multi-line block that scrolled partly
/// off the top is still captured whole.
const FEED_HISTORY_LINES: i32 = 200;

/// Upper bound (bytes) on a feed_screen payload. The consumer writes stdin
/// before draining stdout, so a payload larger than the OS pipe buffer (~64KB
/// on Linux) could deadlock; keep the capture comfortably under it. Truncated
/// at a char boundary from the newest (bottom) end so the visible prompt is
/// always kept.
const FEED_PAYLOAD_CAP: usize = 48 * 1024;

struct CompiledTrigger {
    regex: onig::Regex,
    instant: bool,
    once: bool,
    action: TriggerAction,
    /// Stable identity (regex + action), independent of the rule's index in
    /// the list, so `once` dedup survives a config reload that inserts or
    /// reorders rules (an index would then point at a different rule).
    id: u64,
}

/// The cursor (live prompt) line's fire state for one route: the row the
/// cursor last sat on, and which (rule, match text) pairs already fired there.
/// The set is cleared only when the cursor ROW NUMBER changes, so an action
/// whose output echoes back into the same line (`send_text "y"` -> `[y/n]y`)
/// does not re-fire the rule, yet a fresh prompt drawn at a new row does.
#[derive(Default)]
struct CursorFired {
    /// The cursor's ABSOLUTE line (history + screen row) when these matches
    /// fired — not the screen row, which stays constant on a scrolling
    /// console and would suppress every re-drawn prompt as an echo.
    row: i64,
    fired: FxHashSet<(u64, u64)>,
}

/// Compiled trigger rules plus per-route dedup. Owned on the main thread
/// (`onig::Regex` is `!Send`).
#[derive(Default)]
pub struct Triggers {
    rules: Vec<CompiledTrigger>,
    has_highlight: bool,
    /// Any rule pipes the screen to a coprocess; gate the screen capture.
    has_feed_screen: bool,
    /// Per route, the set of (alt screen, absolute line, content hash,
    /// finalized) we've already evaluated, so a given line+content fires
    /// once. The absolute line is `scroll_epoch + row` (a never-clamped
    /// physical-line identity; `history_size` stops at the scrollback
    /// limit and is always 0 in the alt screen). The alt flag keeps the
    /// two screens' keys apart so retention/purge on one never wipes the
    /// other's, and swapping back doesn't mass re-fire.
    seen: FxHashMap<usize, FxHashSet<(bool, i64, u64, bool)>>,
    /// (route, stable rule id) of `once` rules that have already fired.
    /// Retained across rebuild (config reload) — a stable id keeps the match
    /// correct even when the rule list changes.
    fired_once: FxHashSet<(usize, u64)>,
    /// Per route, the cursor line's fire state (row + fired rule/match pairs).
    /// A rule fires again on the cursor line only when a genuinely new match
    /// appears; an echo of an action's own output, or the line merely growing
    /// by a chunk, does not re-fire. See `CursorFired`.
    cursor_fired: FxHashMap<usize, CursorFired>,
    /// Per route, the last (screen_lines, columns) seen. A change means a
    /// resize/font-zoom reflowed the grid, rewriting absolute line numbers and
    /// wrapping, which invalidates `seen`'s (abs_line, content) keys; the next
    /// scan then re-seeds `seen` without firing so reflow doesn't mass re-fire.
    dims: FxHashMap<usize, (usize, usize)>,
    /// Per (route, rule id): (window start, fires in window) for the
    /// pty-writing burst budget. See WRITE_BURST_WINDOW's doc.
    write_bursts: FxHashMap<(usize, u64), (Instant, u32)>,
}

/// A one-shot trigger action with captures already substituted.
pub enum ResolvedAction {
    Notify {
        title: String,
        body: String,
        urgency: u8,
    },
    TabColor([f32; 4]),
    Run {
        program: String,
        args: Vec<String>,
    },
    SendText(String),
    Coprocess {
        program: String,
        args: Vec<String>,
        stdin: Option<String>,
    },
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

/// Stable identity for a rule: its regex plus a textual rendering of its
/// action. Unlike the rule's list index, this survives inserting/reordering
/// rules across a config reload, so `once` dedup stays attached to the same
/// rule rather than being re-armed or leaking onto a different one.
fn rule_id(regex: &str, action: &TriggerAction) -> u64 {
    let mut h = rustc_hash::FxHasher::default();
    regex.hash(&mut h);
    format!("{action:?}").hash(&mut h);
    h.finish()
}

/// Hash of the matched substring (whole match), used to dedup cursor-line
/// fires by what matched rather than by the whole line's content, so an echo
/// or a mid-line chunk append that reproduces an already-fired match is
/// suppressed while a genuinely new match still fires.
#[inline]
fn match_hash(text: &str, caps: &onig::Captures) -> u64 {
    match caps.pos(0) {
        Some((s, e)) => hash_text(&text[s..e]),
        None => 0,
    }
}

impl Triggers {
    pub fn new(config: &TriggersConfig) -> Self {
        let mut rules = Vec::with_capacity(config.rules.len());
        // Byte-identical duplicate rules would share an id (second `once`
        // copy never fires, second instant copy swallowed by the shared
        // cursor-line key); salt each repeat with its occurrence index —
        // stable across reloads since file order is preserved.
        let mut dups: FxHashMap<u64, u64> = FxHashMap::default();
        for rule in &config.rules {
            match onig::Regex::new(&rule.regex) {
                Ok(regex) => {
                    let base = rule_id(&rule.regex, &rule.action);
                    let salt = dups.entry(base).or_insert(0);
                    let id = base ^ salt.rotate_left(32).wrapping_add(*salt);
                    *salt += 1;
                    rules.push(CompiledTrigger {
                        regex,
                        instant: rule.instant,
                        once: rule.once,
                        id,
                        action: rule.action.clone(),
                    })
                }
                Err(err) => {
                    tracing::warn!("invalid trigger regex {:?}: {}", rule.regex, err);
                }
            }
        }
        let has_highlight = rules
            .iter()
            .any(|r| matches!(r.action, TriggerAction::Highlight { .. }));
        let has_feed_screen = rules.iter().any(|r| {
            matches!(
                r.action,
                TriggerAction::Coprocess {
                    feed_screen: true,
                    ..
                }
            )
        });
        Self {
            rules,
            has_highlight,
            has_feed_screen,
            seen: FxHashMap::default(),
            fired_once: FxHashSet::default(),
            cursor_fired: FxHashMap::default(),
            dims: FxHashMap::default(),
            write_bursts: FxHashMap::default(),
        }
    }

    /// Recompile rules from config (config hot-reload), PRESERVING the
    /// per-route dedup state. `*self = Triggers::new(...)` would wipe
    /// `seen`/`fired_once`, so the next scan would treat every line
    /// already on screen as new — re-firing `once` rules and re-running
    /// `send_text` (e.g. re-typing a saved credential) just because an
    /// unrelated config file was edited. Only the rules change here.
    pub fn rebuild(&mut self, config: &TriggersConfig) {
        let fresh = Triggers::new(config);
        self.rules = fresh.rules;
        self.has_highlight = fresh.has_highlight;
        self.has_feed_screen = fresh.has_feed_screen;
        // seen / fired_once / cursor_fired / dims intentionally retained;
        // fired_once is keyed on stable rule ids so retention stays correct
        // even when the rule list is edited.
    }

    /// Forget a route's accumulated dedup state when its pane closes,
    /// so `seen`/`fired_once`/`cursor_fired`/`dims` don't grow without bound
    /// over a long session that opens and closes many tabs.
    pub fn forget_route(&mut self, route_id: usize) {
        self.seen.remove(&route_id);
        self.cursor_fired.remove(&route_id);
        self.dims.remove(&route_id);
        self.fired_once.retain(|(r, _)| *r != route_id);
        self.write_bursts.retain(|(r, _), _| *r != route_id);
    }

    /// Re-arm `once` rules so the automation can run again. Bound to
    /// `ResetTriggers` (e.g. Alt+R). Drops only the cursor-line (non-finalized)
    /// dedup, so instant rules (a `login:`/`Password:` prompt) re-fire on the
    /// current live line even when it sits where one fired before. Finalized
    /// (scrolled-past) content stays deduped, so a re-arm doesn't replay a
    /// whole stale flow at once.
    pub fn reset(&mut self) {
        self.fired_once.clear();
        for seen in self.seen.values_mut() {
            seen.retain(|(_, _, _, finalized)| *finalized);
        }
        self.cursor_fired.clear();
        self.write_bursts.clear();
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
        // Never-clamped physical-line identity base. `history_size` stops
        // at the scrollback limit (and is 0 in the alt screen), which made
        // `history + row` collapse once saturated: every scroll step gave
        // each visible line a fresh key (mass re-fire) and pinned the
        // cursor's key (prompts suppressed again). The epoch always
        // advances with the rows.
        let epoch = grid.scroll_epoch();
        let cursor_row = grid.cursor.pos.row.0 as i64;
        let screen_lines = grid.screen_lines();
        let columns = term.columns();
        let is_alt = term.mode().contains(Mode::ALT_SCREEN);

        // A resize / font-zoom reflows the grid: absolute line numbers and
        // wrapping are rewritten, so `seen`'s (abs_line, content) keys no
        // longer identify the same output and every visible line would look
        // new. Detect it by a dimension change and re-seed `seen` from the
        // current screen WITHOUT firing, so already-visible finalized output
        // is suppressed while genuinely new output still fires.
        let reflowed = self.dims.insert(route_id, (screen_lines, columns))
            != Some((screen_lines, columns));

        // Skip when the terminal content did not change since the last render.
        // scan() runs at the top of every render() — cursor blink, mouse hover
        // and other UI-only repaints included — but only Full/Partial damage
        // means cells actually changed. Noop/CursorOnly frames do no scan work
        // (this walks and hashes visible lines under the terminal lock).
        // A reflow always reports Full damage, so the re-seed above is reached.
        let full_damage = match term.peek_damage_event() {
            Some(TerminalDamage::Full) => true,
            Some(TerminalDamage::Partial) => false,
            _ => return Vec::new(),
        };

        // Captured lazily on the first feed_screen match (see below) so the
        // common path — and every non-matching frame — pays nothing.
        let mut screen_text: Option<String> = None;

        let seen = self.seen.entry(route_id).or_default();
        if reflowed {
            seen.clear();
        }
        // Drop this screen's lines that scrolled out of the live view. The
        // other screen's keys are kept for the swap back (leaving the alt
        // screen must not make every restored primary line look new).
        seen.retain(|(alt, abs, _, _)| *alt != is_alt || *abs >= epoch);
        // A redrawing TUI (htop/watch) keeps producing new content hashes
        // on the same rows, growing the set without bound. Cap it — but
        // purge only this screen's keys, and re-seed silently below (like
        // reflow) instead of letting the purge re-fire everything visible.
        let mut purged = false;
        if seen.len() > 8192 {
            seen.retain(|(alt, _, _, _)| *alt != is_alt);
            purged = true;
        }
        // Re-seed pass: record keys, fire nothing.
        let reseed = reflowed || purged;

        // Reset the cursor line's fire set when the cursor moved to a new
        // absolute line, so a fresh prompt fires while an echo/growth on
        // the same line cannot (a scrolling console keeps the cursor on
        // the bottom SCREEN row forever, so the epoch-based line is the
        // only identity that both advances with new prompts and stays put
        // for an echo). A scrolled-back view (offset != 0) is not the live
        // prompt, so leave the fire set untouched.
        let live = grid.display_offset() == 0;
        if live {
            let cursor_abs = epoch + cursor_row;
            let cf = self.cursor_fired.entry(route_id).or_default();
            if cf.row != cursor_abs {
                cf.row = cursor_abs;
                cf.fired.clear();
            }
        }

        let mut actions = Vec::new();
        let mut text = String::new();
        let mut cells: Vec<(u16, u16)> = Vec::new();
        for i in 0..screen_lines {
            // While scrolled back, the cursor row is still being written:
            // matching it now would fire non-instant rules on a half-drawn
            // line (truncated captures) and again once it finalizes. Skip
            // it; it is evaluated when the view returns to the bottom.
            if !live && (i as i64) == cursor_row {
                continue;
            }
            // On a Partial frame only damaged rows can have changed; the
            // rest keep both their content and their (epoch-based) keys.
            // Re-seed passes must walk everything to rebuild the keys.
            if !full_damage && !reseed && !term.peek_line_damaged(i) {
                continue;
            }
            let abs = epoch + i as i64;
            let is_cursor = live && (i as i64) == cursor_row;
            let finalized = (i as i64) < cursor_row;
            extract_match_line(term, Line(i as i32), &mut text, &mut cells);
            if text.is_empty() {
                continue;
            }
            let text: &str = if cells.len() > LINE_SCAN_CAP {
                match text.char_indices().nth(LINE_SCAN_CAP) {
                    Some((byte, _)) => &text[..byte],
                    None => &text,
                }
            } else {
                &text
            };

            // The cursor (live prompt) line is handled per-match below so an
            // echo of an action's own output doesn't re-fire it (findings on
            // send_text/coprocess feedback and growing instant matches). Other
            // lines fire once each, keyed on (line, content, phase). When
            // re-seeding (reflow / cap purge), record the key but don't fire.
            if !is_cursor {
                let fresh = seen.insert((is_alt, abs, hash_text(text), finalized));
                if !fresh || reseed {
                    continue;
                }
            }

            for rule in &self.rules {
                if matches!(rule.action, TriggerAction::Highlight { .. }) {
                    continue;
                }
                // Finalized lines run non-instant rules; the cursor line
                // runs instant rules (prompts with no trailing newline).
                if rule.instant != is_cursor {
                    continue;
                }
                if rule.once && self.fired_once.contains(&(route_id, rule.id)) {
                    continue;
                }
                let mut matched = false;
                for caps in rule.regex.captures_iter(text) {
                    // On the cursor line, dedup by the matched substring so an
                    // action's echo (or the line growing by a chunk) that
                    // reproduces an already-fired match is suppressed, while a
                    // new match on the same row still fires. On a re-seed
                    // (reflow / purge) record the key silently — a resize
                    // must not re-fire the still-visible prompt.
                    if is_cursor {
                        let key = (rule.id, match_hash(text, &caps));
                        if !self
                            .cursor_fired
                            .get_mut(&route_id)
                            .expect("cursor_fired seeded above")
                            .fired
                            .insert(key)
                        {
                            continue;
                        }
                        if reseed {
                            continue;
                        }
                    }
                    // Bound write-back feedback loops (see the const doc):
                    // a fire inside the budget pays nothing; only a loop
                    // exhausts it.
                    if matches!(
                        rule.action,
                        TriggerAction::SendText { .. } | TriggerAction::Coprocess { .. }
                    ) {
                        let now = Instant::now();
                        let burst = self
                            .write_bursts
                            .entry((route_id, rule.id))
                            .or_insert((now, 0));
                        if now - burst.0 > WRITE_BURST_WINDOW {
                            *burst = (now, 0);
                        }
                        burst.1 += 1;
                        if burst.1 > WRITE_BURST_MAX {
                            tracing::warn!(
                                "trigger write action exceeded {WRITE_BURST_MAX} fires/s; suppressing (feedback loop?)"
                            );
                            continue;
                        }
                    }
                    if self.has_feed_screen
                        && screen_text.is_none()
                        && matches!(
                            rule.action,
                            TriggerAction::Coprocess {
                                feed_screen: true,
                                ..
                            }
                        )
                    {
                        screen_text = Some(capture_screen(term));
                    }
                    actions.push(resolve(&rule.action, &caps, screen_text.as_deref()));
                    matched = true;
                    // A `once` rule fires a single action per line even
                    // when the pattern occurs several times — otherwise
                    // it would emit N notifications/runs before
                    // fired_once is set after the loop.
                    if rule.once {
                        break;
                    }
                }
                if matched && rule.once {
                    self.fired_once.insert((route_id, rule.id));
                }
            }
        }
        actions
    }

    /// Recompute highlight ranges over the visible region, or `None` when the
    /// terminal content did not change since the last render so the caller
    /// should keep the highlights it already has (passed as `prev`).
    /// Highlights are a visual state, but re-running onig over every visible
    /// line each frame — under the terminal lock, on cursor-blink and hover
    /// repaints too — is wasted work when no cell changed. On a Partial frame
    /// of the live view only damaged rows are rescanned; `prev`'s matches on
    /// untouched rows are carried over. An empty `Vec` still means "clear",
    /// e.g. when the last matching text scrolled off or the rules changed.
    pub fn highlights<T: EventListener>(
        &self,
        term: &Crosswords<T>,
        prev: Option<&[(Match, [u8; 4])]>,
    ) -> Option<Vec<(Match, [u8; 4])>> {
        if !self.has_highlight {
            return Some(Vec::new());
        }
        let full_damage = match term.peek_damage_event() {
            Some(TerminalDamage::Full) => true,
            Some(TerminalDamage::Partial) => false,
            _ => return None,
        };
        let grid = &term.grid;
        let display_offset = grid.display_offset() as i32;
        let topmost = grid.topmost_line().0;
        let screen_lines = grid.screen_lines();
        // Rescan damaged rows only when the view is live (line coords then
        // equal screen rows, so prev's untouched entries stay valid); any
        // scrolled view recomputes fully.
        let incremental = !full_damage && display_offset == 0;
        let mut out = Vec::new();
        if incremental {
            if let Some(prev) = prev {
                out.extend(
                    prev.iter()
                        .filter(|(m, _)| {
                            let row = m.start().row.0;
                            row >= 0 && !term.peek_line_damaged(row as usize)
                        })
                        .cloned(),
                );
            }
        }
        let mut text = String::new();
        let mut cells: Vec<(u16, u16)> = Vec::new();
        for i in 0..screen_lines {
            if incremental && !term.peek_line_damaged(i) {
                continue;
            }
            let line = Line(i as i32 - display_offset);
            if line.0 < topmost {
                continue;
            }
            extract_match_line(term, line, &mut text, &mut cells);
            if text.is_empty() {
                continue;
            }
            for rule in &self.rules {
                let TriggerAction::Highlight { color } = &rule.action else {
                    continue;
                };
                let rgba = rgba_u8(*color);
                // Cap matches per rule per line: a pathological pattern
                // on adversarial output could otherwise push an unbounded
                // number of ranges every frame (this runs under the
                // terminal lock, on the render path).
                for caps in rule.regex.captures_iter(&text).take(256) {
                    if let Some((start, end)) = span(&text, &cells, &caps) {
                        out.push((Pos::new(line, start)..=Pos::new(line, end), rgba));
                    }
                }
            }
        }
        Some(out)
    }
}

/// Fill `text` with one grid line's characters and `cells` with each
/// character's (first, last) cell column. Wide-char spacer cells are
/// skipped — with the padding spaces they'd otherwise inject, adjacent
/// wide characters (CJK, emoji) could never match a regex — which is why
/// the byte-offset -> column mapping needs the explicit `cells` table.
/// Trailing NUL/whitespace is trimmed. Buffers are caller-owned so a
/// screen walk reuses one allocation instead of two per line.
fn extract_match_line<T: EventListener>(
    term: &Crosswords<T>,
    line: Line,
    text: &mut String,
    cells: &mut Vec<(u16, u16)>,
) {
    text.clear();
    cells.clear();
    let grid = &term.grid;
    let mut keep_chars = 0;
    let mut keep_bytes = 0;
    for col in 0..grid.columns() {
        let sq = &grid[line][Column(col)];
        let wide = sq.wide();
        if matches!(wide, Wide::Spacer | Wide::LeadingSpacer) {
            continue;
        }
        let c = sq.c();
        let end = if matches!(wide, Wide::Wide) {
            col + 1
        } else {
            col
        };
        text.push(c);
        cells.push((col as u16, end as u16));
        if c != '\0' && !c.is_whitespace() {
            keep_chars = cells.len();
            keep_bytes = text.len();
        }
    }
    cells.truncate(keep_chars);
    text.truncate(keep_bytes);
}

/// Visible screen plus recent scrollback as one newline-joined string, for a
/// feed_screen coprocess. Recent history is included so a multi-line block
/// that has scrolled partly above the visible area is still captured whole.
fn capture_screen<T: EventListener>(term: &Crosswords<T>) -> String {
    let grid = &term.grid;
    let screen_lines = grid.screen_lines() as i32;
    let start = grid.topmost_line().0.max(-FEED_HISTORY_LINES);
    let mut line = String::new();
    let mut cells = Vec::new();
    let mut text = String::new();
    for i in start..screen_lines {
        extract_match_line(term, Line(i), &mut line, &mut cells);
        if i > start {
            text.push('\n');
        }
        text.push_str(&line);
    }
    if text.len() <= FEED_PAYLOAD_CAP {
        return text;
    }
    // Keep the newest end (visible prompt); drop the oldest scrollback.
    let cut = text.len() - FEED_PAYLOAD_CAP;
    match text.char_indices().find(|(i, _)| *i >= cut) {
        Some((byte, _)) => text[byte..].to_string(),
        None => text,
    }
}

/// Match span as cell columns. onig reports byte offsets; `cells` maps
/// each char index to its (first, last) cell column — wide characters
/// cover two cells, so the mapping isn't 1:1.
fn span(
    text: &str,
    cells: &[(u16, u16)],
    caps: &onig::Captures,
) -> Option<(Column, Column)> {
    let (start_b, end_b) = caps.pos(0)?;
    let start_char = text[..start_b].chars().count();
    let end_char = text[..end_b].chars().count().saturating_sub(1);
    let start = cells.get(start_char)?.0 as usize;
    let end = cells.get(end_char.max(start_char))?.1 as usize;
    Some((Column(start), Column(end.max(start))))
}

fn resolve(
    action: &TriggerAction,
    caps: &onig::Captures,
    screen: Option<&str>,
) -> ResolvedAction {
    match action {
        TriggerAction::Notify {
            title,
            body,
            urgency,
        } => ResolvedAction::Notify {
            title: substitute(title, caps),
            body: substitute(body, caps),
            urgency: urgency.level(),
        },
        TriggerAction::TabColor { color } => ResolvedAction::TabColor(*color),
        TriggerAction::Run { program, args } => ResolvedAction::Run {
            program: program.clone(),
            args: args.iter().map(|a| substitute(a, caps)).collect(),
        },
        TriggerAction::SendText { text } => {
            ResolvedAction::SendText(substitute(text, caps))
        }
        TriggerAction::Coprocess {
            program,
            args,
            feed_screen,
        } => ResolvedAction::Coprocess {
            program: program.clone(),
            args: args.iter().map(|a| substitute(a, caps)).collect(),
            stdin: if *feed_screen {
                screen.map(str::to_owned)
            } else {
                None
            },
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

    #[test]
    fn rule_id_is_stable_and_distinct() {
        let color = [0.0, 0.0, 0.0, 1.0];
        let a = TriggerAction::TabColor { color };
        let b = TriggerAction::SendText { text: "y\n".into() };
        // Same regex + same action -> same id across calls (survives reload).
        assert_eq!(rule_id("done", &a), rule_id("done", &a));
        // A different regex or a different action -> different id.
        assert_ne!(rule_id("done", &a), rule_id("finished", &a));
        assert_ne!(rule_id("done", &a), rule_id("done", &b));
    }

    #[test]
    fn match_hash_tracks_matched_substring() {
        let re = onig::Regex::new(r"\[y/n\]").unwrap();
        // The whole line grows as an echo appends, but the match text is the
        // same, so the cursor-line dedup key is unchanged -> no re-fire.
        let c1 = re.captures("Continue? [y/n]").unwrap();
        let c2 = re.captures("Continue? [y/n]y").unwrap();
        assert_eq!(
            match_hash("Continue? [y/n]", &c1),
            match_hash("Continue? [y/n]y", &c2)
        );
    }
}
