// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! The dialog that holds a paste which could run commands until the user
//! confirms it. It looks like the command palette: a backdrop, a rounded
//! panel, then why it asks, the lines that matter, and the keys.

use rio_backend::paste;
use rio_backend::sugarloaf::text::{DrawOpts, Text};
use rio_backend::sugarloaf::Sugarloaf;

// Shared with the command palette, so both read as one UI.
const BACKDROP_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 0.50];
const BG_COLOR: [f32; 4] = [0.08, 0.08, 0.08, 0.98];
const STROKE_COLOR: [f32; 4] = [0.16, 0.16, 0.16, 1.0];
const BLOCK_COLOR: [f32; 4] = [0.15, 0.15, 0.15, 1.0];
const SEPARATOR_COLOR: [f32; 4] = [0.15, 0.15, 0.15, 1.0];
const TEXT_COLOR: [u8; 4] = [217, 217, 217, 255];
const WHY_COLOR: [u8; 4] = [138, 138, 138, 255];
const DIM_COLOR: [u8; 4] = [90, 90, 90, 255];
const KEY_LABEL_COLOR: [u8; 4] = [154, 154, 154, 255];
const CHIP_COLOR: [f32; 4] = [0.11, 0.11, 0.11, 1.0];
const CHIP_STROKE_COLOR: [f32; 4] = [0.23, 0.23, 0.23, 1.0];
/// A control character that will be sent as a space.
const WARN_COLOR: [u8; 4] = [224, 162, 78, 255];
/// The same, when the paste holds a bracket end.
const DANGER_COLOR: [u8; 4] = [226, 104, 91, 255];

const DEPTH_BACKDROP: f32 = 0.0;
const DEPTH_BG: f32 = 0.1;
const DEPTH_ELEMENT: f32 = 0.2;
const ORDER: u8 = 20;

const WIDTH_MIN: f32 = 320.0;
const WIDTH_MAX: f32 = 560.0;
const WINDOW_MARGIN: f32 = 16.0;
const RADIUS: f32 = 8.0;
const PADDING_X: f32 = 14.0;
const PADDING_TOP: f32 = 12.0;
const PADDING_BOTTOM: f32 = 12.0;
const TITLE_SIZE: f32 = 14.0;
const TITLE_HEIGHT: f32 = 20.0;
const BODY_SIZE: f32 = 12.0;
const BODY_HEIGHT: f32 = 17.0;
const META_SIZE: f32 = 11.0;
const SEPARATOR_GAP: f32 = 8.0;
const BLOCK_MARGIN: f32 = 10.0;
const BLOCK_PADDING: f32 = 8.0;
const BLOCK_RADIUS: f32 = 4.0;
const NUMBER_GAP: f32 = 10.0;
const CHIP_HEIGHT: f32 = 17.0;
const CHIP_PADDING_X: f32 = 5.0;
const CHIP_RADIUS: f32 = 4.0;
const CHIP_LABEL_GAP: f32 = 6.0;
const KEYS_GAP: f32 = 12.0;
const FOOTER_GAP: f32 = 16.0;
/// How many lines of the paste the dialog shows.
const PREVIEW_LINES: usize = 4;
/// Longer lines are cut before measuring; the panel can't show more.
const LINE_CHARS_MAX: usize = 200;
/// The key chips: key, label, whether it's the primary action.
const KEYS: [(&str, &str, bool); 2] =
    [("enter", "Paste", true), ("esc", "Cancel", false)];

/// Why a paste is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reason {
    /// The program takes the paste as typing, so each line would run.
    Lines,
    /// Bracketed, with `paste-bracketed-safe` off.
    BracketedLines,
    /// It holds the end of a bracketed paste.
    BracketEnd,
}

/// How a piece of a preview line is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Piece {
    Text,
    /// A control character the paste sends as a space.
    Replaced,
    /// Any other control character, which goes as is.
    Control,
}

/// A held paste and what the dialog says about it, worked out once. Only
/// the lines shown are copied: a large paste isn't held twice.
#[derive(Debug)]
struct Held {
    text: String,
    reason: Reason,
    line_count: usize,
    bytes: usize,
    replaced: usize,
    /// The lines shown, from the line before the first one with a control
    /// character that will be replaced, or the first lines.
    shown: std::ops::Range<usize>,
    /// Those lines, each cut to what the panel could show.
    preview: Vec<String>,
}

impl Held {
    fn new(text: String, bracketed: bool) -> Self {
        let reason = if !paste::is_safe_with(&text, true) {
            Reason::BracketEnd
        } else if bracketed {
            Reason::BracketedLines
        } else {
            Reason::Lines
        };
        let mut line_count = 0;
        let mut replaced = 0;
        let mut first_replaced_line = None;
        for (index, line) in Lines::new(&text).enumerate() {
            let in_line = line.chars().filter(|&c| paste::is_replaced(c)).count();
            if in_line > 0 && first_replaced_line.is_none() {
                first_replaced_line = Some(index);
            }
            replaced += in_line;
            line_count = index + 1;
        }
        let start = first_replaced_line.map_or(0, |line: usize| line.saturating_sub(1));
        let end = (start + PREVIEW_LINES).min(line_count);
        let shown = end.saturating_sub(PREVIEW_LINES)..end;
        let preview = Lines::new(&text)
            .skip(shown.start)
            .take(shown.len())
            .map(|line| line.chars().take(LINE_CHARS_MAX).collect())
            .collect();
        Self {
            bytes: text.len(),
            text,
            reason,
            line_count,
            replaced,
            shown,
            preview,
        }
    }

    fn title(&self) -> String {
        match self.reason {
            Reason::BracketEnd => "Paste holds a bracketed paste end".to_owned(),
            Reason::Lines | Reason::BracketedLines => {
                format!("Paste {}?", plural(self.line_count, "line"))
            }
        }
    }

    fn why(&self) -> &'static str {
        match self.reason {
            Reason::Lines => {
                "The program isn't taking a bracketed paste, so each line runs as typed."
            }
            Reason::BracketedLines => {
                "Line breaks are confirmed even in bracketed pastes (paste-bracketed-safe)."
            }
            Reason::BracketEnd => {
                "It could end the paste early and let the rest run as typed."
            }
        }
    }

    fn meta(&self) -> String {
        let mut meta = format!(
            "{} · {}",
            plural(self.line_count, "line"),
            plural(self.bytes, "byte")
        );
        if self.replaced > 0 {
            meta.push_str(&format!(
                " · {} sent as spaces",
                plural(self.replaced, "control character")
            ));
        }
        meta
    }

    fn shown(&self) -> std::ops::Range<usize> {
        self.shown.clone()
    }

    /// What's left out of the preview, if anything.
    fn hidden(&self) -> Option<String> {
        let len = self.line_count;
        let shown = self.shown();
        if shown.start == 0 && shown.end == len {
            return None;
        }
        if shown.start == 0 {
            let more = len - shown.end;
            return Some(format!(
                "+{more} more {}",
                if more == 1 { "line" } else { "lines" }
            ));
        }
        let mut parts = vec![line_range(1, shown.start)];
        if shown.end < len {
            parts.push(line_range(shown.end + 1, len));
        }
        Some(format!("{} hidden", parts.join(" and ")))
    }

    fn accent(&self) -> [u8; 4] {
        if self.reason == Reason::BracketEnd {
            DANGER_COLOR
        } else {
            WARN_COLOR
        }
    }
}

/// `text`'s lines, any line break (CRLF, LF or CR) ending one; a final
/// break doesn't start an empty line, and empty text is one empty line.
struct Lines<'a> {
    rest: Option<&'a str>,
}

impl<'a> Lines<'a> {
    fn new(text: &'a str) -> Self {
        Self { rest: Some(text) }
    }
}

impl<'a> Iterator for Lines<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        let rest = self.rest?;
        match rest.find(['\r', '\n']) {
            Some(at) => {
                let skip = if rest[at..].starts_with("\r\n") { 2 } else { 1 };
                let after = &rest[at + skip..];
                self.rest = (!after.is_empty()).then_some(after);
                Some(&rest[..at])
            }
            None => {
                self.rest = None;
                Some(rest)
            }
        }
    }
}

/// A line as drawn: control characters in caret notation (`^[`, `^C`),
/// the way `cat -v` shows them, which any font can draw; a tab as a space.
fn pieces(line: &str) -> Vec<(String, Piece)> {
    let mut pieces: Vec<(String, Piece)> = Vec::new();
    for c in line.chars().take(LINE_CHARS_MAX) {
        let (shown, kind) = if c == '\t' {
            (" ".to_owned(), Piece::Text)
        } else if paste::is_replaced(c) {
            (caret(c), Piece::Replaced)
        } else if c.is_control() {
            (caret(c), Piece::Control)
        } else {
            (c.to_string(), Piece::Text)
        };
        match pieces.last_mut() {
            Some((text, last)) if *last == kind => text.push_str(&shown),
            _ => pieces.push((shown, kind)),
        }
    }
    pieces
}

fn caret(c: char) -> String {
    match c as u32 {
        code @ 0..=0x1f => format!("^{}", char::from(code as u8 + 0x40)),
        0x7f => "^?".to_owned(),
        code => format!("<{code:02X}>"),
    }
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

fn line_range(from: usize, to: usize) -> String {
    if from == to {
        format!("line {from}")
    } else {
        format!("lines {from}–{to}")
    }
}

/// A paste held back until the user confirms it, because it could run
/// commands: see `paste-protection` and `paste-bracketed-safe`.
pub struct ConfirmPaste {
    protection: bool,
    bracketed_safe: bool,
    pending: Option<Held>,
}

impl ConfirmPaste {
    pub fn new(protection: bool, bracketed_safe: bool) -> Self {
        Self {
            protection,
            bracketed_safe,
            pending: None,
        }
    }

    /// Whether pasting `text` should be confirmed first, `bracketed` being
    /// whether the program asked for bracketed paste. Bracketed, line
    /// breaks are the program's to handle when `paste-bracketed-safe` is
    /// on, and only a bracket end is asked about; otherwise any line
    /// break is too.
    pub fn needs_confirmation(&self, text: &str, bracketed: bool) -> bool {
        if !self.protection {
            return false;
        }
        if bracketed && self.bracketed_safe {
            !paste::is_safe_with(text, true)
        } else {
            !paste::is_safe(text)
        }
    }

    /// Hold `text` until it's confirmed or cancelled; a later paste
    /// replaces it. `bracketed` is whether the program asked for
    /// bracketed paste, which the dialog explains.
    pub fn hold(&mut self, text: String, bracketed: bool) {
        self.pending = Some(Held::new(text, bracketed));
    }

    #[inline]
    pub fn is_active(&self) -> bool {
        self.pending.is_some()
    }

    /// The paste, confirmed.
    pub fn take(&mut self) -> Option<String> {
        self.pending.take().map(|held| held.text)
    }

    pub fn cancel(&mut self) {
        self.pending = None;
    }

    /// `dimensions` is `(window_width, window_height, scale_factor)`,
    /// matching the other overlays' `render` signature.
    pub fn render(&self, sugarloaf: &mut Sugarloaf, dimensions: (f32, f32, f32)) {
        let Some(held) = &self.pending else {
            return;
        };

        let (width, height, scale) = dimensions;
        let win_w = width / scale;
        let win_h = height / scale;

        let opts = |font_size: f32, color: [u8; 4]| DrawOpts {
            font_size,
            color,
            ..DrawOpts::default()
        };
        let title_opts = DrawOpts {
            bold: true,
            ..opts(TITLE_SIZE, TEXT_COLOR)
        };
        let why_opts = opts(BODY_SIZE, WHY_COLOR);
        let line_opts = opts(BODY_SIZE, TEXT_COLOR);
        let number_opts = opts(BODY_SIZE, DIM_COLOR);
        let meta_opts = opts(META_SIZE, DIM_COLOR);
        let key_opts = opts(META_SIZE, TEXT_COLOR);
        let label_opts = |primary: bool| {
            opts(
                BODY_SIZE,
                if primary { TEXT_COLOR } else { KEY_LABEL_COLOR },
            )
        };

        let title = held.title();
        let why = held.why();
        let meta = held.meta();
        let shown = held.shown();
        let hidden = held.hidden();
        let preview: Vec<(String, Vec<(String, Piece)>)> = shown
            .clone()
            .zip(&held.preview)
            .map(|(index, line)| ((index + 1).to_string(), pieces(line)))
            .collect();

        // Measure first, then draw: the shapes and the text both borrow
        // sugarloaf.
        let ui = sugarloaf.text_mut();
        let title_w = ui.measure(&title, &title_opts);
        let why_w = ui.measure(why, &why_opts);
        let meta_w = ui.measure(&meta, &meta_opts);
        let number_w = ui.measure(&shown.end.to_string(), &number_opts);
        let mut line_w = 0.0_f32;
        for (_, pieces) in &preview {
            let text: String = pieces.iter().map(|(text, _)| text.as_str()).collect();
            line_w = line_w.max(ui.measure(&text, &line_opts));
        }
        let mut keys_w = Vec::with_capacity(KEYS.len());
        for (key, label, primary) in KEYS {
            keys_w.push((
                ui.measure(key, &key_opts),
                ui.measure(label, &label_opts(primary)),
            ));
        }
        let keys_total = keys_w
            .iter()
            .map(|(key_w, label_w)| {
                key_w + CHIP_PADDING_X * 2.0 + CHIP_LABEL_GAP + label_w
            })
            .sum::<f32>()
            + KEYS_GAP * (KEYS.len() - 1) as f32;

        let preview_w =
            (BLOCK_MARGIN + BLOCK_PADDING) * 2.0 + number_w + NUMBER_GAP + line_w;
        let content_w = (title_w + PADDING_X * 2.0)
            .max(why_w + PADDING_X * 2.0)
            .max(meta_w + FOOTER_GAP + keys_total + PADDING_X * 2.0)
            .max(preview_w);
        let max_w = (win_w - WINDOW_MARGIN * 2.0).clamp(0.0, WIDTH_MAX);
        let box_w = content_w.clamp(WIDTH_MIN.min(max_w), max_w);

        let preview_rows = preview.len() + usize::from(hidden.is_some());
        let block_h = BLOCK_PADDING * 2.0 + BODY_HEIGHT * preview_rows as f32;
        let box_h = PADDING_TOP
            + TITLE_HEIGHT
            + BODY_HEIGHT
            + SEPARATOR_GAP
            + 1.0
            + BLOCK_MARGIN
            + block_h
            + BLOCK_MARGIN
            + CHIP_HEIGHT
            + PADDING_BOTTOM;
        let box_x = ((win_w - box_w) / 2.0).max(0.0);
        let box_y = ((win_h - box_h) / 2.0).max(0.0);
        let inner_x = box_x + PADDING_X;
        let inner_w = box_w - PADDING_X * 2.0;
        let title_y = box_y + PADDING_TOP;
        let why_y = title_y + TITLE_HEIGHT;
        let separator_y = why_y + BODY_HEIGHT + SEPARATOR_GAP;
        let block_x = box_x + BLOCK_MARGIN;
        let block_y = separator_y + 1.0 + BLOCK_MARGIN;
        let block_w = box_w - BLOCK_MARGIN * 2.0;
        let footer_y = block_y + block_h + BLOCK_MARGIN;
        let keys_x = box_x + box_w - PADDING_X - keys_total;

        sugarloaf.rect(
            None,
            0.0,
            0.0,
            win_w,
            win_h,
            BACKDROP_COLOR,
            DEPTH_BACKDROP,
            ORDER,
        );
        stroke_rounded_rect(
            sugarloaf,
            (box_x, box_y, box_w, box_h),
            RADIUS,
            (STROKE_COLOR, BG_COLOR),
            DEPTH_BG,
        );
        sugarloaf.rect(
            None,
            box_x,
            separator_y,
            box_w,
            1.0,
            SEPARATOR_COLOR,
            DEPTH_ELEMENT,
            ORDER,
        );
        sugarloaf.rounded_rect(
            None,
            block_x,
            block_y,
            block_w,
            block_h,
            BLOCK_COLOR,
            DEPTH_ELEMENT,
            BLOCK_RADIUS,
            ORDER,
        );
        let mut chips = Vec::with_capacity(KEYS.len());
        let mut chip_x = keys_x;
        for (key_w, label_w) in &keys_w {
            let chip_w = key_w + CHIP_PADDING_X * 2.0;
            stroke_rounded_rect(
                sugarloaf,
                (chip_x, footer_y, chip_w, CHIP_HEIGHT),
                CHIP_RADIUS,
                (CHIP_STROKE_COLOR, CHIP_COLOR),
                DEPTH_ELEMENT,
            );
            chips.push((chip_x, chip_w));
            chip_x += chip_w + CHIP_LABEL_GAP + label_w + KEYS_GAP;
        }

        let ui = sugarloaf.text_mut();
        draw_clipped(ui, inner_x, title_y, &title, &title_opts, inner_w);
        draw_clipped(ui, inner_x, why_y, why, &why_opts, inner_w);

        let number_x = block_x + BLOCK_PADDING;
        let text_x = number_x + number_w + NUMBER_GAP;
        let text_end = block_x + block_w - BLOCK_PADDING;
        let mut row_y = block_y + BLOCK_PADDING;
        for (number, pieces) in &preview {
            let right = ui.measure(number, &number_opts);
            ui.draw(number_x + number_w - right, row_y, number, &number_opts);
            let mut x = text_x;
            for (text, kind) in pieces {
                let color = match kind {
                    Piece::Text => TEXT_COLOR,
                    Piece::Replaced => held.accent(),
                    Piece::Control => DIM_COLOR,
                };
                x += draw_clipped(
                    ui,
                    x,
                    row_y,
                    text,
                    &opts(BODY_SIZE, color),
                    text_end - x,
                );
                if x >= text_end - 0.5 {
                    break;
                }
            }
            row_y += BODY_HEIGHT;
        }
        if let Some(hidden) = &hidden {
            draw_clipped(ui, text_x, row_y, hidden, &number_opts, text_end - text_x);
        }

        let meta_room = keys_x - inner_x - FOOTER_GAP;
        draw_clipped(ui, inner_x, footer_y + 2.0, &meta, &meta_opts, meta_room);
        for (((key, label, primary), (chip_x, chip_w)), (key_w, _)) in
            KEYS.iter().zip(&chips).zip(&keys_w)
        {
            ui.draw(
                chip_x + (chip_w - key_w) / 2.0,
                footer_y + 2.0,
                key,
                &key_opts,
            );
            ui.draw(
                chip_x + chip_w + CHIP_LABEL_GAP,
                footer_y + 1.0,
                label,
                &label_opts(*primary),
            );
        }
    }
}

/// Draw `text` at `(x, y)`, cut with an ellipsis where it would pass
/// `max_w`. Returns the width drawn.
fn draw_clipped(
    ui: &mut Text,
    x: f32,
    y: f32,
    text: &str,
    opts: &DrawOpts,
    max_w: f32,
) -> f32 {
    if max_w <= 0.0 {
        return 0.0;
    }
    if ui.measure(text, opts) <= max_w {
        return ui.draw(x, y, text, opts);
    }
    // The longest prefix that fits with the ellipsis, by bisection: a few
    // measures instead of one per character, each a shaped string kept in
    // the text cache.
    let ends: Vec<usize> = text.char_indices().map(|(at, _)| at).skip(1).collect();
    let fits = |ui: &mut Text, end: usize| {
        let mut cut = text[..end].to_owned();
        cut.push('…');
        ui.measure(&cut, opts) <= max_w
    };
    let (mut lo, mut hi) = (0, ends.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if fits(ui, ends[mid - 1]) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let end = if lo == 0 { 0 } else { ends[lo - 1] };
    let mut cut = text[..end].to_owned();
    cut.push('…');
    ui.draw(x, y, &cut, opts)
}

/// A rounded rect with a 1px border: the border color, then the fill
/// inset by a pixel on top. Sugarloaf has no stroked-rect primitive.
fn stroke_rounded_rect(
    sugarloaf: &mut Sugarloaf,
    (x, y, width, height): (f32, f32, f32, f32),
    radius: f32,
    (stroke_color, fill_color): ([f32; 4], [f32; 4]),
    depth: f32,
) {
    sugarloaf.rounded_rect(
        None,
        x,
        y,
        width,
        height,
        stroke_color,
        depth,
        radius,
        ORDER,
    );
    sugarloaf.rounded_rect(
        None,
        x + 1.0,
        y + 1.0,
        (width - 2.0).max(0.0),
        (height - 2.0).max(0.0),
        fill_color,
        depth + 0.001,
        (radius - 1.0).max(0.0),
        ORDER,
    );
}

#[cfg(test)]
mod tests {
    use super::{caret, pieces, ConfirmPaste, Held, Lines, Piece, Reason};

    // As configured by default: line breaks are asked about only when
    // the paste isn't bracketed; a bracket end always is.
    #[test]
    fn asks_about_what_could_run_commands() {
        let confirm = ConfirmPaste::new(true, true);
        assert!(!confirm.needs_confirmation("one line", false));
        assert!(!confirm.needs_confirmation("one line", true));
        assert!(confirm.needs_confirmation("ls\nrm -rf ~", false));
        assert!(confirm.needs_confirmation("ls\r", false));
        assert!(!confirm.needs_confirmation("ls\nrm -rf ~", true));
        assert!(confirm.needs_confirmation("a\x1b[201~rm -rf ~\n", true));
    }

    #[test]
    fn bracketed_line_breaks_are_asked_about_when_not_trusted() {
        let confirm = ConfirmPaste::new(true, false);
        assert!(confirm.needs_confirmation("ls\nrm -rf ~", true));
        assert!(!confirm.needs_confirmation("one line", true));
    }

    #[test]
    fn nothing_is_asked_with_protection_off() {
        let confirm = ConfirmPaste::new(false, true);
        assert!(!confirm.needs_confirmation("ls\nrm -rf ~", false));
        assert!(!confirm.needs_confirmation("a\x1b[201~", true));
    }

    #[test]
    fn holds_one_paste_until_taken_or_cancelled() {
        let mut confirm = ConfirmPaste::new(true, true);
        assert!(!confirm.is_active());
        confirm.hold("first\n".into(), false);
        confirm.hold("second\n".into(), false);
        assert!(confirm.is_active());
        assert_eq!(confirm.take().as_deref(), Some("second\n"));
        assert!(!confirm.is_active());
        confirm.hold("third\n".into(), false);
        confirm.cancel();
        assert_eq!(confirm.take(), None);
    }

    #[test]
    fn lines_split_on_any_break_without_a_trailing_empty_one() {
        let lines = |text| Lines::new(text).collect::<Vec<_>>();
        assert_eq!(lines("a\r\nb\nc\rd"), ["a", "b", "c", "d"]);
        assert_eq!(lines("ls\n"), ["ls"]);
        assert_eq!(lines("a\n\nb"), ["a", "", "b"]);
        assert_eq!(lines("a\r\n\r\n"), ["a", ""]);
        assert_eq!(lines(""), [""]);
    }

    // Only the lines shown are kept, cut to what the panel could show.
    #[test]
    fn only_the_shown_lines_are_copied() {
        let text: String = (1..=1000).map(|n| format!("line {n}\n")).collect();
        let held = Held::new(text, false);
        assert_eq!(held.line_count, 1000);
        assert_eq!(held.preview, ["line 1", "line 2", "line 3", "line 4"]);
        let long = Held::new("x".repeat(5000), false);
        assert_eq!(long.preview[0].chars().count(), super::LINE_CHARS_MAX);
    }

    #[test]
    fn the_dialog_says_why_and_how_much() {
        let held = Held::new("cd ~/rio\ngit pull\ncargo build\n".into(), false);
        assert_eq!(held.reason, Reason::Lines);
        assert_eq!(held.title(), "Paste 3 lines?");
        assert_eq!(held.meta(), "3 lines · 30 bytes");
        assert_eq!(held.hidden(), None);

        let bracketed = Held::new("a\nb".into(), true);
        assert_eq!(bracketed.reason, Reason::BracketedLines);

        let end = Held::new("echo hi\x1b[201~\nrm -rf ~".into(), true);
        assert_eq!(end.reason, Reason::BracketEnd);
        assert_eq!(end.title(), "Paste holds a bracketed paste end");
        assert_eq!(
            end.meta(),
            "2 lines · 22 bytes · 1 control character sent as spaces"
        );
    }

    // The preview starts just before the first line with a control
    // character that will be replaced, and says what it leaves out.
    #[test]
    fn the_preview_shows_the_lines_that_matter() {
        let text: String = (1..=14)
            .map(|n| {
                if n == 9 {
                    "printf '\x1b]0;rio\x07'\n".to_owned()
                } else {
                    format!("line {n}\n")
                }
            })
            .collect();
        let held = Held::new(text, false);
        assert_eq!(held.shown(), 7..11);
        assert_eq!(
            held.hidden().as_deref(),
            Some("lines 1–7 and lines 12–14 hidden")
        );

        let plain: String = (1..=6).map(|n| format!("l{n}\n")).collect();
        let held = Held::new(plain, false);
        assert_eq!(held.shown(), 0..4);
        assert_eq!(held.hidden().as_deref(), Some("+2 more lines"));

        let late: String = (1..=5)
            .map(|n| {
                if n == 5 {
                    "\x03\n".to_owned()
                } else {
                    format!("l{n}\n")
                }
            })
            .collect();
        let held = Held::new(late, false);
        assert_eq!(held.shown(), 1..5);
        assert_eq!(held.hidden().as_deref(), Some("line 1 hidden"));
    }

    // A line too wide is cut to the longest prefix that fits with an
    // ellipsis; one that fits is drawn whole.
    #[test]
    fn a_long_line_is_cut_to_fit() {
        use rio_backend::sugarloaf::font::FontLibrary;
        use rio_backend::sugarloaf::text::{DrawOpts, Text};
        let mut ui = Text::new(&FontLibrary::default());
        ui.init_cpu();
        let opts = DrawOpts::default();
        let text = "cargo build --release --features wgpu";
        let whole = ui.measure(text, &opts);
        assert_eq!(
            super::draw_clipped(&mut ui, 0.0, 0.0, text, &opts, whole + 1.0),
            whole
        );
        let max_w = whole / 2.0;
        let drawn = super::draw_clipped(&mut ui, 0.0, 0.0, text, &opts, max_w);
        assert!(drawn <= max_w, "{drawn} > {max_w}");
        let char_w = ui.measure("c", &opts);
        assert!(
            drawn > max_w - char_w * 2.0,
            "{drawn} cut too short of {max_w}"
        );
        assert_eq!(
            super::draw_clipped(&mut ui, 0.0, 0.0, text, &opts, 0.0),
            0.0
        );
    }

    // Control characters show in caret notation, the replaced ones
    // marked so they can be colored.
    #[test]
    fn control_characters_show_in_caret_notation() {
        assert_eq!(caret('\x1b'), "^[");
        assert_eq!(caret('\x03'), "^C");
        assert_eq!(caret('\x7f'), "^?");
        assert_eq!(caret('\u{9b}'), "<9B>");
        assert_eq!(
            pieces("a\x1b]0\x07\tb"),
            vec![
                ("a".to_owned(), Piece::Text),
                ("^[".to_owned(), Piece::Replaced),
                ("]0".to_owned(), Piece::Text),
                ("^G".to_owned(), Piece::Control),
                (" b".to_owned(), Piece::Text),
            ]
        );
    }
}
