// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Screen serialization: one walk of the active screen with several emit
//! targets. Today `Plain` (text) and `Vt` (ANSI escape sequences) are
//! implemented; the enum leaves room for `Html`.
//!
//! `Vt` reproduces the visible screen as a byte blob that, written to a fresh
//! terminal, renders the same thing: the classic "reconnect / restore
//! snapshot" (vt100's `contents_formatted`). Colors are emitted in their
//! ORIGINAL form (named / indexed / rgb), NOT resolved to RGB, so the client
//! applies its own palette.

use super::Crosswords;
use crate::config::colors::AnsiColor;
use crate::crosswords::pos::{Column, Line};
use crate::crosswords::style::{Style, StyleFlags};
use crate::event::EventListener;
use std::fmt::Write as _;

/// Serialization target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Emit {
    /// Plain UTF-8 text, no attributes.
    Plain,
    /// VT/ANSI escape sequences (SGR + positioning): a restore snapshot.
    Vt,
}

/// Options for [`Crosswords::format`].
#[derive(Debug, Clone, Copy)]
pub struct FormatOptions {
    pub emit: Emit,
    /// Trim trailing whitespace on each row and drop trailing blank rows.
    pub trim: bool,
}

impl FormatOptions {
    pub fn plain() -> Self {
        Self {
            emit: Emit::Plain,
            trim: true,
        }
    }
    pub fn vt() -> Self {
        Self {
            emit: Emit::Vt,
            trim: true,
        }
    }
}

impl<U: EventListener> Crosswords<U> {
    /// Serialize the visible screen using `opts`.
    pub fn format(&self, opts: FormatOptions) -> String {
        match opts.emit {
            Emit::Plain => self.format_plain(),
            Emit::Vt => self.format_vt(opts),
        }
    }

    /// The visible screen as a VT/ANSI byte blob (vt100 `contents_formatted`
    /// equivalent): clear + home, cells with SGR, then the cursor restored.
    pub fn contents_formatted(&self) -> Vec<u8> {
        self.format(FormatOptions::vt()).into_bytes()
    }

    fn format_plain(&self) -> String {
        let rows = self.screen_lines();
        let cols = self.columns();
        if rows == 0 || cols == 0 {
            return String::new();
        }
        let start = crate::crosswords::pos::Pos::new(Line(0), Column(0));
        let end =
            crate::crosswords::pos::Pos::new(Line(rows as i32 - 1), Column(cols - 1));
        self.bounds_to_string(start, end)
    }

    fn format_vt(&self, opts: FormatOptions) -> String {
        let rows = self.screen_lines();
        let cols = self.columns();
        if rows == 0 || cols == 0 {
            return String::new();
        }

        let mut out = String::new();
        // Reset, clear, home so the snapshot renders on a fresh client.
        out.push_str("\x1b[0m\x1b[2J\x1b[H");
        // The style currently in effect on the client. Starts as the default
        // (matching the `\x1b[0m` above); we emit an SGR only when it changes.
        let mut current = Style::default();

        // Find the last non-blank row so we can drop trailing blank rows.
        let last_row = if opts.trim {
            (0..rows)
                .rev()
                .find(|&line| self.row_has_content(line, cols))
                .map(|l| l as i32)
        } else {
            Some(rows as i32 - 1)
        };
        let Some(last_row) = last_row else {
            // Blank screen; still restore the cursor.
            self.push_cursor(&mut out);
            return out;
        };

        for line in 0..=last_row {
            let last_col = self.row_last_content_col(line, cols, opts.trim);
            let mut col = 0usize;
            while let Some(limit) = last_col {
                if col > limit {
                    break;
                }
                let square = &self.grid[Line(line)][Column(col)];
                if square.is_spacer() {
                    col += 1;
                    continue;
                }
                let style = self.grid.style_of(square);
                if style != current {
                    write_sgr(&mut out, &style);
                    current = style;
                }
                let c = square.c();
                out.push(if c == '\0' { ' ' } else { c });
                col += if square.is_wide() { 2 } else { 1 };
            }
            if line < last_row {
                out.push_str("\r\n");
            }
        }

        out.push_str("\x1b[0m");
        self.push_cursor(&mut out);
        out
    }

    fn push_cursor(&self, out: &mut String) {
        let pos = self.grid.cursor.pos;
        let row = pos.row.0.max(0) + 1;
        let col = pos.col.0 + 1;
        let _ = write!(out, "\x1b[{row};{col}H");
        // Restore cursor visibility (DECTCEM). A screen with a hidden cursor
        // is common while a TUI redraws; without this a restored snapshot
        // would leave a stray cursor on the client until the next output.
        if self.mode.contains(super::Mode::SHOW_CURSOR) {
            out.push_str("\x1b[?25h");
        } else {
            out.push_str("\x1b[?25l");
        }
    }

    fn row_has_content(&self, line: usize, cols: usize) -> bool {
        self.row_last_content_col(line as i32, cols, true).is_some()
    }

    /// Rightmost column with a non-blank glyph or a style that renders
    /// on a blank cell (`Style::renders_on_blank`), or `None` for a row
    /// with neither. Fg-only attributes must NOT count as content:
    /// `ED`/`EL` reset cells with the full cursor template style, so
    /// after `\x1b[31m` + `clear` every blank carries a red fg, and
    /// trimming on any non-default style would then emit the whole
    /// screen as styled spaces.
    fn row_last_content_col(&self, line: i32, cols: usize, trim: bool) -> Option<usize> {
        if !trim {
            return Some(cols.saturating_sub(1));
        }
        let row = &self.grid[Line(line)];
        (0..cols).rev().find(|&col| {
            let square = &row[Column(col)];
            let c = square.c();
            (c != ' ' && c != '\0') || self.grid.style_of(square).renders_on_blank()
        })
    }
}

/// Write the SGR sequence for `style` directly into `out`, starting with `0`
/// (reset) then re-applying: styles are fully self-contained. Written inline
/// (no intermediate allocation); call it only when the style actually
/// changes. Colors keep their original named / indexed / rgb form. Also the
/// style emitter for librio's `Surface::serialize`.
pub fn write_sgr(out: &mut String, style: &Style) {
    use crate::crosswords::style::UnderlineKind;

    out.push_str("\x1b[0");
    let flags = style.flags;
    if flags.contains(StyleFlags::BOLD) {
        out.push_str(";1");
    }
    if flags.contains(StyleFlags::DIM) {
        out.push_str(";2");
    }
    if flags.contains(StyleFlags::ITALIC) {
        out.push_str(";3");
    }
    match flags.underline_kind() {
        None => {}
        Some(UnderlineKind::Single) => out.push_str(";4"),
        Some(UnderlineKind::Double) => out.push_str(";4:2"),
        Some(UnderlineKind::Curly) => out.push_str(";4:3"),
        Some(UnderlineKind::Dotted) => out.push_str(";4:4"),
        Some(UnderlineKind::Dashed) => out.push_str(";4:5"),
    }
    if flags.contains(StyleFlags::SLOW_BLINK) {
        out.push_str(";5");
    } else if flags.contains(StyleFlags::RAPID_BLINK) {
        out.push_str(";6");
    }
    if flags.contains(StyleFlags::INVERSE) {
        out.push_str(";7");
    }
    if flags.contains(StyleFlags::HIDDEN) {
        out.push_str(";8");
    }
    if flags.contains(StyleFlags::STRIKEOUT) {
        out.push_str(";9");
    }
    push_color(out, style.fg, true);
    push_color(out, style.bg, false);
    out.push('m');

    // Underline color travels as its own sequence; the leading reset
    // already cleared any previous one.
    if let Some(underline) = style.underline_color {
        match underline {
            AnsiColor::Indexed(i) => {
                let _ = write!(out, "\x1b[58;5;{i}m");
            }
            AnsiColor::Spec(rgb) => {
                let _ = write!(out, "\x1b[58;2;{};{};{}m", rgb.r, rgb.g, rgb.b);
            }
            AnsiColor::Named(n) => {
                // Specials (Foreground = 256 and up) have no palette
                // index; SGR 58 cannot express them, so leave the reset's
                // unset underline color rather than emit an invalid index.
                if let Ok(idx) = u8::try_from(n as u16) {
                    let _ = write!(out, "\x1b[58;5;{idx}m");
                }
            }
        }
    }
}

fn push_color(params: &mut String, color: AnsiColor, fg: bool) {
    match color {
        AnsiColor::Named(named) => {
            let n = named as u16;
            if n <= 7 {
                let base = if fg { 30 } else { 40 };
                let _ = write!(params, ";{}", base + n);
            } else if n <= 15 {
                let base = if fg { 90 } else { 100 };
                let _ = write!(params, ";{}", base + (n - 8));
            }
            // Named default fg/bg (256/257) and specials: covered by the
            // leading `0` reset, so emit nothing.
        }
        AnsiColor::Indexed(i) => {
            let intro = if fg { "38" } else { "48" };
            let _ = write!(params, ";{intro};5;{i}");
        }
        AnsiColor::Spec(rgb) => {
            let intro = if fg { "38" } else { "48" };
            let _ = write!(params, ";{intro};2;{};{};{}", rgb.r, rgb.g, rgb.b);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ansi::CursorShape;
    use crate::crosswords::square::Square;
    use crate::crosswords::{Crosswords, CrosswordsSize};
    use crate::event::{VoidListener, WindowId};
    use crate::performer::handler::Processor;

    fn term(cols: usize, rows: usize) -> Crosswords<VoidListener> {
        Crosswords::new(
            CrosswordsSize::new(cols, rows),
            CursorShape::Block,
            VoidListener,
            WindowId::from(0),
            0,
            0,
        )
    }

    fn feed(t: &mut Crosswords<VoidListener>, bytes: &[u8]) {
        let mut p = Processor::default();
        p.advance(t, bytes);
    }

    #[test]
    fn vt_roundtrip_preserves_text_and_style() {
        let mut a = term(20, 4);
        // rapid-blink red "hello", default space, bold-blink-blue "world",
        // then a red-undercurled "curl" on the next line.
        feed(
            &mut a,
            b"\x1b[6;31mhello\x1b[0m \x1b[1;5;34mworld\x1b[0m\r\n\
                      \x1b[4:3;58;2;255;0;0mcurl\x1b[0m",
        );
        let snapshot = a.contents_formatted();

        // The snapshot must carry the colors (index form, not resolved rgb).
        let text = String::from_utf8(snapshot.clone()).unwrap();
        assert!(text.contains("31"), "missing red fg: {text:?}");
        assert!(text.contains("34"), "missing blue fg: {text:?}");

        // Re-parse into a fresh screen; the plain content must match.
        let mut b = term(20, 4);
        feed(&mut b, &snapshot);
        assert_eq!(
            a.format(FormatOptions::plain()),
            b.format(FormatOptions::plain()),
        );

        // And a styled cell round-trips its color. 'w' of "world" is at col 6.
        let sa = *(&a.grid[Line(0)][Column(6)] as &Square);
        let sb = *(&b.grid[Line(0)][Column(6)] as &Square);
        assert_eq!(sa.c(), 'w');
        assert_eq!(sb.c(), 'w');
        assert_eq!(a.grid.style_of(&sa).fg, b.grid.style_of(&sb).fg);
        assert!(b.grid.style_of(&sb).flags.contains(StyleFlags::BOLD));
        assert!(b.grid.style_of(&sb).flags.contains(StyleFlags::SLOW_BLINK));
        let hb = *(&b.grid[Line(0)][Column(0)] as &Square);
        assert!(b.grid.style_of(&hb).flags.contains(StyleFlags::RAPID_BLINK));

        // Underline kind and color survive too. 'c' of "curl" is at (1, 0).
        let cb = *(&b.grid[Line(1)][Column(0)] as &Square);
        assert_eq!(cb.c(), 'c');
        let curl = b.grid.style_of(&cb);
        assert!(curl.flags.contains(StyleFlags::UNDERCURL));
        assert_eq!(
            curl.underline_color,
            Some(AnsiColor::Spec(crate::config::colors::ColorRgb {
                r: 255,
                g: 0,
                b: 0
            }))
        );
    }

    #[test]
    fn vt_roundtrip_preserves_styled_blanks() {
        use crate::config::colors::NamedColor;

        // Red-background trailing spaces after "AB" render, so the trim
        // must keep them.
        let mut a = term(20, 4);
        feed(&mut a, b"AB\x1b[41m   \x1b[0m");
        let snapshot = a.contents_formatted();
        let text = String::from_utf8(snapshot.clone()).unwrap();
        assert!(text.contains(";41"), "missing red bg: {text:?}");

        let mut b = term(20, 4);
        feed(&mut b, &snapshot);
        let sb = *(&b.grid[Line(0)][Column(3)] as &Square);
        assert_eq!(b.grid.style_of(&sb).bg, AnsiColor::Named(NamedColor::Red));

        // A row made only of styled blanks must not be dropped as blank.
        let mut c = term(20, 4);
        feed(&mut c, b"top\r\n\x1b[44m    \x1b[0m");
        let text = String::from_utf8(c.contents_formatted()).unwrap();
        assert!(text.contains(";44"), "styled-blank row dropped: {text:?}");
    }

    #[test]
    fn vt_trims_fg_only_erased_cells() {
        // ED resets cells with the full cursor template, so after
        // `\x1b[31m` + clear every blank carries a red fg. Fg-only
        // attributes render nothing on a blank: the snapshot must trim
        // them, not emit the whole screen as styled spaces.
        let mut a = term(20, 4);
        feed(&mut a, b"\x1b[31mhi\x1b[2J\x1b[H");
        let text = String::from_utf8(a.contents_formatted()).unwrap();
        assert!(
            !text.contains(' '),
            "erased fg-styled blanks not trimmed: {text:?}"
        );
    }

    #[test]
    fn vt_restores_cursor_visibility() {
        // Hidden cursor (as a TUI leaves it mid-draw) must be restored hidden.
        let mut hidden = term(10, 3);
        feed(&mut hidden, b"\x1b[?25l");
        let text = String::from_utf8(hidden.contents_formatted()).unwrap();
        assert!(
            text.contains("\x1b[?25l"),
            "snapshot should hide cursor: {text:?}"
        );
        assert!(!text.contains("\x1b[?25h"));

        // Default (shown) cursor is restored shown.
        let shown = term(10, 3);
        let text = String::from_utf8(shown.contents_formatted()).unwrap();
        assert!(
            text.contains("\x1b[?25h"),
            "snapshot should show cursor: {text:?}"
        );
    }

    #[test]
    fn plain_and_vt_agree_on_text() {
        let mut a = term(10, 3);
        feed(&mut a, b"abc\r\ndef");
        let plain = a.format(FormatOptions::plain());
        assert!(plain.contains("abc"));
        assert!(plain.contains("def"));
    }
}
