// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Screen serialization: one walk of the grid with several emit targets.
//! Today `Plain` (text) and `Vt` (ANSI escape sequences) are implemented;
//! the enum leaves room for `Html`.
//!
//! `Vt` reproduces the terminal as a byte blob that, written to a fresh
//! terminal of the same size, renders the same thing: the classic
//! "reconnect / restore snapshot" (vt100's `contents_formatted`). Colors are
//! emitted in their ORIGINAL form (named / indexed / rgb), NOT resolved to
//! RGB, so the client applies its own palette.
//!
//! [`FormatOptions::vt`] covers the visible screen and the cursor.
//! [`FormatOptions::snapshot`] covers everything a terminal server needs to
//! hand a new client the session as it stands: the scrollback, soft-wrapped
//! lines kept soft-wrapped (so the client reflows them on resize), OSC 8
//! hyperlinks, both screens (the primary one under a running alt-screen
//! program), and the state programs set: modes, scroll region, tab stops,
//! charsets, kitty keyboard flags, the cursor's position (pending wrap
//! included), pen, style and saved position, the title and the working
//! directory.
//!
//! Not carried: palette changes (OSC 4/10/11; the client keeps its theme),
//! the title stack, images, and a half-parsed escape sequence (that is the
//! parser's, not the terminal's).

use super::Crosswords;
use crate::ansi::CursorShape;
use crate::config::colors::AnsiColor;
use crate::crosswords::grid::{Dimensions, Grid};
use crate::crosswords::pos::{CharsetIndex, Column, Line, Pos, StandardCharset};
use crate::crosswords::square::{ContentTag, Hyperlink, Square};
use crate::crosswords::style::{Style, StyleFlags, StyleId};
use crate::crosswords::Mode;
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
    /// Include the scrollback above the screen.
    pub scrollback: bool,
    /// `Vt`: write a soft-wrapped line as one line and let the client wrap
    /// it, so it stays soft-wrapped there (and reflows on resize), instead
    /// of a hard line break per row. `Plain` always joins them.
    pub unwrap: bool,
    /// `Vt`: OSC 8 around linked cells.
    pub hyperlinks: bool,
    /// `Vt`: the terminal's state after the content (see the module docs),
    /// and the primary screen under an active alt screen. Without it only
    /// the cursor's position and visibility are restored.
    pub state: bool,
}

impl FormatOptions {
    /// The visible screen as text.
    pub fn plain() -> Self {
        Self {
            emit: Emit::Plain,
            trim: true,
            scrollback: false,
            unwrap: true,
            hyperlinks: false,
            state: false,
        }
    }

    /// The visible screen with its styles, and the cursor.
    pub fn vt() -> Self {
        Self {
            emit: Emit::Vt,
            trim: true,
            scrollback: false,
            unwrap: false,
            hyperlinks: false,
            state: false,
        }
    }

    /// Everything a fresh terminal of the same size needs to become this
    /// one: see the module docs.
    pub fn snapshot() -> Self {
        Self {
            emit: Emit::Vt,
            trim: true,
            scrollback: true,
            unwrap: true,
            hyperlinks: true,
            state: true,
        }
    }

    pub fn with_trim(mut self, on: bool) -> Self {
        self.trim = on;
        self
    }

    pub fn with_scrollback(mut self, on: bool) -> Self {
        self.scrollback = on;
        self
    }

    pub fn with_unwrap(mut self, on: bool) -> Self {
        self.unwrap = on;
        self
    }

    pub fn with_hyperlinks(mut self, on: bool) -> Self {
        self.hyperlinks = on;
        self
    }

    pub fn with_state(mut self, on: bool) -> Self {
        self.state = on;
        self
    }
}

/// The modes a snapshot restores (DECSET numbers), with their bits. Mouse
/// tracking modes are exclusive, so at most one of those is set.
const PRIVATE_MODES: &[(Mode, u16)] = &[
    (Mode::APP_CURSOR, 1),
    (Mode::LINE_WRAP, 7),
    (Mode::MOUSE_REPORT_X10, 9),
    (Mode::MOUSE_REPORT_CLICK, 1000),
    (Mode::MOUSE_DRAG, 1002),
    (Mode::MOUSE_MOTION, 1003),
    (Mode::FOCUS_IN_OUT, 1004),
    (Mode::UTF8_MOUSE, 1005),
    (Mode::SGR_MOUSE, 1006),
    (Mode::ALTERNATE_SCROLL, 1007),
    (Mode::URGENCY_HINTS, 1042),
    (Mode::BRACKETED_PASTE, 2004),
    (Mode::GRAPHEME_CLUSTER, 2027),
];

/// ANSI modes (SM/RM numbers).
const ANSI_MODES: &[(Mode, u16)] = &[(Mode::INSERT, 4), (Mode::LINE_FEED_NEW_LINE, 20)];

/// What a fresh terminal starts with (`Crosswords::new`); a snapshot sets
/// or resets the modes that differ.
fn initial_mode() -> Mode {
    Mode::SHOW_CURSOR
        | Mode::LINE_WRAP
        | Mode::ALTERNATE_SCROLL
        | Mode::URGENCY_HINTS
        | Mode::GRAPHEME_CLUSTER
}

/// The output and what the client has in effect while it's written, so an
/// SGR or OSC 8 goes out only when it changes.
struct Out {
    s: String,
    style: Style,
    link: Option<Hyperlink>,
    /// The style id (in the grid being written) that `style` came from,
    /// so a run of cells with one style costs an id compare per cell, not
    /// a style lookup and compare. `None` when unknown.
    style_id: Option<StyleId>,
    /// The same for `link`: the extras id it came from.
    link_id: Option<u16>,
}

impl Out {
    fn new(capacity: usize) -> Out {
        Out {
            s: String::with_capacity(capacity),
            style: Style::default(),
            link: None,
            style_id: None,
            link_id: None,
        }
    }

    /// One of `grid`'s cells: its link, its style, then its text.
    #[inline]
    fn cell(&mut self, grid: &Grid<Square>, square: Square, pos: Pos, hyperlinks: bool) {
        if hyperlinks {
            if square.has_hyperlink() {
                let id = square.extras_id();
                if id.is_none() || id != self.link_id {
                    self.link(cell_link(grid, &square));
                    self.link_id = id;
                }
            } else if self.link.is_some() {
                self.link(None);
            }
        }
        // Background-only cells carry their color inline, not an id.
        let id = matches!(square.content_tag(), ContentTag::Codepoint)
            .then(|| square.style_id());
        if id.is_none() || id != self.style_id {
            self.style(grid.style_of(&square));
            self.style_id = id;
        }
        if square.extras_id_checked().is_some() {
            // Combining marks live in the extras.
            for c in grid.cell_text(pos) {
                self.s.push(if c == '\0' { ' ' } else { c });
            }
        } else {
            let c = square.c();
            self.s.push(if c == '\0' { ' ' } else { c });
        }
    }

    fn style(&mut self, style: Style) {
        self.style_id = None;
        if style != self.style {
            write_sgr_change(&mut self.s, &self.style, &style);
            self.style = style;
        }
    }

    fn link(&mut self, link: Option<Hyperlink>) {
        self.link_id = None;
        if link == self.link {
            return;
        }
        if self.link.is_some() {
            self.s.push_str("\x1b]8;;\x1b\\");
        }
        if let Some(link) = &link {
            // Ids rio made up for a link that came without one stay
            // implicit, so the client makes up its own.
            let id = link.id();
            if id.ends_with("_rio") {
                let _ = write!(self.s, "\x1b]8;;{}\x1b\\", link.uri());
            } else {
                let _ = write!(self.s, "\x1b]8;id={id};{}\x1b\\", link.uri());
            }
        }
        self.link = link;
    }

    /// Back to no link and the default style.
    fn plain(&mut self) {
        self.link(None);
        self.style_id = None;
        if self.style != Style::default() {
            self.s.push_str("\x1b[0m");
            self.style = Style::default();
        }
    }
}

fn cell_link(grid: &Grid<Square>, square: &Square) -> Option<Hyperlink> {
    if !square.has_hyperlink() {
        return None;
    }
    grid.extras_table
        .get(square.extras_id()?)
        .and_then(|e| e.hyperlink.clone())
}

impl<U: EventListener> Crosswords<U> {
    /// Serialize the terminal using `opts`.
    pub fn format(&self, opts: FormatOptions) -> String {
        match opts.emit {
            Emit::Plain => self.format_plain(opts),
            Emit::Vt => self.format_vt(opts),
        }
    }

    /// The visible screen as a VT/ANSI byte blob (vt100 `contents_formatted`
    /// equivalent): clear + home, cells with SGR, then the cursor restored.
    pub fn contents_formatted(&self) -> Vec<u8> {
        self.format(FormatOptions::vt()).into_bytes()
    }

    /// VT bytes that turn a fresh terminal of the same size (and enough
    /// scrollback) into this one: [`FormatOptions::snapshot`].
    ///
    /// It walks the whole scrollback under `&self`, so a caller that takes
    /// the terminal's lock for it holds that lock, and the PTY reader
    /// waits, for the whole walk: about 2.4 ms for 10k lines of styled
    /// 120-column history, and about 21 ms when every cell has a color of
    /// its own (`benches/vt_snapshot.rs`). Take it where output can wait
    /// that long, not on the render path.
    pub fn snapshot_formatted(&self) -> Vec<u8> {
        self.format(FormatOptions::snapshot()).into_bytes()
    }

    fn format_plain(&self, opts: FormatOptions) -> String {
        let rows = self.screen_lines();
        let cols = self.columns();
        if rows == 0 || cols == 0 {
            return String::new();
        }
        let top = if opts.scrollback {
            -(self.grid.history_size() as i32)
        } else {
            0
        };
        let start = Pos::new(Line(top), Column(0));
        let end = Pos::new(Line(rows as i32 - 1), Column(cols - 1));
        self.bounds_to_string(start, end)
    }

    fn format_vt(&self, opts: FormatOptions) -> String {
        let rows = self.screen_lines();
        let cols = self.columns();
        if rows == 0 || cols == 0 {
            return String::new();
        }

        // Lines of text run about half the width in escapes and glyphs.
        let lines = rows
            + if opts.scrollback {
                self.grid.history_size()
            } else {
                0
            };
        let mut out = Out::new(lines * (cols / 2 + 8));
        // Reset, clear, home so the snapshot renders on a fresh client.
        out.s.push_str("\x1b[0m\x1b[2J\x1b[H");
        if opts.scrollback {
            out.s.push_str("\x1b[3J");
        }

        let alt = opts.state && self.mode.contains(Mode::ALT_SCREEN);
        if !alt {
            self.write_rows(&mut out, &self.grid, opts);
            out.plain();
            if opts.state {
                self.write_state(&mut out);
            } else {
                self.push_cursor(&mut out.s);
            }
            shrink_slack(&mut out.s);
            return out.s;
        }

        // An alt-screen program runs: the primary screen first, with its
        // cursor where the program found it (where leaving the alt screen
        // puts it back), then the alt screen over it.
        let primary = &self.inactive_grid;
        self.write_rows(&mut out, primary, opts);
        out.plain();
        write_keyboard_stack(
            &mut out.s,
            &self.inactive_keyboard_mode_stack,
            self.inactive_keyboard_mode_idx,
        );
        let at = primary.cursor.pos;
        self.place_cursor(
            &mut out,
            primary,
            (at.row.0, at.col.0),
            0,
            primary.cursor.should_wrap && self.mode.contains(Mode::LINE_WRAP),
        );
        out.s.push_str("\x1b[?1049h\x1b[H");
        self.write_rows(&mut out, &self.grid, opts.with_scrollback(false));
        out.plain();
        self.write_state(&mut out);
        shrink_slack(&mut out.s);
        out.s
    }

    /// The rows of `grid` (its history too with `opts.scrollback`), from
    /// the client's home position down.
    fn write_rows(&self, out: &mut Out, grid: &Grid<Square>, opts: FormatOptions) {
        let rows = grid.screen_lines() as i32;
        let cols = grid.columns();
        let history = if opts.scrollback {
            grid.history_size() as i32
        } else {
            0
        };
        let top = -history;
        // With history above, every screen row is written, blank ones
        // too: the screen must end up on the client's screen and the
        // history above it. Without, trailing blank rows can go.
        let last = if history > 0 || !opts.trim {
            Some(rows - 1)
        } else {
            (0..rows)
                .rev()
                .find(|&line| row_last_col(grid, line, cols, true).is_some())
        };
        let Some(last) = last else { return };
        let last_col = Column(cols - 1);
        let mut wrapped = false;

        for line in top..=last {
            let row = &grid[Line(line)];
            // A soft wrap is left to the client: the full row's pending
            // wrap makes it wrap at the next character.
            let wraps = opts.unwrap && line < last && row[last_col].wrapline();
            let end = if wraps {
                Some(cols - 1)
            } else {
                row_last_col(grid, line, cols, opts.trim)
            };
            if wrapped && end.is_none() {
                // Wrapped onto a blank row: one blank makes the client wrap.
                out.plain();
                out.s.push(' ');
            }
            wrapped = wraps;
            if let Some(end) = end {
                for (col, &square) in row[..].iter().enumerate().take(end + 1) {
                    // A wide character's second column, and the column a
                    // wide character didn't fit in: the client makes both.
                    if square.is_spacer() || square.is_leading_spacer() {
                        continue;
                    }
                    let pos = Pos::new(Line(line), Column(col));
                    out.cell(grid, square, pos, opts.hyperlinks);
                }
            }
            if line < last && !wraps {
                // A newline at the bottom scrolls in a row the client
                // fills with the pen's background: go plain first.
                if out.style.bg != Style::default().bg {
                    out.s.push_str("\x1b[0m");
                    out.style = Style::default();
                    out.style_id = None;
                }
                out.s.push_str("\r\n");
            }
        }
    }

    /// Move the client's cursor to `grid`'s row `row`, column `col`; the
    /// client's row is `row - offset` (origin mode's rows start at the
    /// scroll region). With `pending_wrap`, the cell under it is written
    /// again, which puts the client's cursor in the same spot with the same
    /// pending wrap.
    fn place_cursor(
        &self,
        out: &mut Out,
        grid: &Grid<Square>,
        (row, col): (i32, usize),
        offset: i32,
        pending_wrap: bool,
    ) {
        let row = row.clamp(0, grid.screen_lines() as i32 - 1);
        let col = col.min(grid.columns() - 1);
        let to = (row - offset).max(0) + 1;
        if pending_wrap && col + 1 == grid.columns() {
            let line = &grid[Line(row)];
            // Under a wide character the cursor is on its second column.
            let at = if line[Column(col)].is_spacer() && col > 0 {
                col - 1
            } else {
                col
            };
            let square = &line[Column(at)];
            let _ = write!(out.s, "\x1b[{to};{}H", at + 1);
            out.link(cell_link(grid, square));
            out.style(grid.style_of(square));
            for c in grid.cell_text(Pos::new(Line(row), Column(at))) {
                out.s.push(if c == '\0' { ' ' } else { c });
            }
            out.plain();
        } else {
            let _ = write!(out.s, "\x1b[{to};{}H", col + 1);
        }
    }

    /// The current screen's state, after its rows.
    fn write_state(&self, out: &mut Out) {
        let grid = &self.grid;
        let rows = grid.screen_lines();
        let cols = grid.columns();

        // Tab stops, where a program changed them.
        let defaults = (0..cols).all(|c| self.tabs.tabs[c] == (c % 8 == 0));
        if !defaults {
            out.s.push_str("\x1b[3g");
            for c in (0..cols).filter(|&c| self.tabs.tabs[c]) {
                let _ = write!(out.s, "\x1b[{}G\x1bH", c + 1);
            }
        }

        // The saved cursor (DECSC): its position and pen. Saved before the
        // scroll region and origin mode are set, so its position is the
        // screen's.
        let saved = &grid.saved_cursor;
        if saved.pos != Pos::default() || saved.pending_style != Style::default() {
            let row = saved.pos.row.0.clamp(0, rows as i32 - 1);
            let col = saved.pos.col.0.min(cols - 1);
            let _ = write!(out.s, "\x1b[{};{}H", row + 1, col + 1);
            out.style(saved.pending_style);
            out.s.push_str("\x1b7");
            out.plain();
        }

        // The scroll region (setting it homes the cursor), then origin
        // mode, then the cursor, which origin mode places in the region.
        let region = &self.scroll_region;
        let full = region.start == Line(0) && region.end == Line(rows as i32);
        if !full {
            let _ = write!(out.s, "\x1b[{};{}r", region.start.0 + 1, region.end.0);
        }
        let origin = self.mode.contains(Mode::ORIGIN);
        if origin {
            out.s.push_str("\x1b[?6h");
        }
        let cursor = &grid.cursor;
        self.place_cursor(
            out,
            grid,
            (cursor.pos.row.0, cursor.pos.col.0),
            if origin { region.start.0 } else { 0 },
            cursor.should_wrap && self.mode.contains(Mode::LINE_WRAP),
        );

        // Modes (after the cursor: insert mode would shift the rewrite
        // above, and autowrap must still be on for it).
        let initial = initial_mode();
        for &(bit, n) in PRIVATE_MODES {
            let on = self.mode.contains(bit);
            if on != initial.contains(bit) {
                let _ = write!(out.s, "\x1b[?{n}{}", if on { 'h' } else { 'l' });
            }
        }
        for &(bit, n) in ANSI_MODES {
            if self.mode.contains(bit) {
                let _ = write!(out.s, "\x1b[{n}h");
            }
        }
        if self.mode.contains(Mode::APP_KEYPAD) {
            out.s.push_str("\x1b=");
        }

        // Charsets (after every character is written: a designated line
        // drawing set would translate them).
        let finals = ['(', ')', '*', '+'];
        let indices = [
            CharsetIndex::G0,
            CharsetIndex::G1,
            CharsetIndex::G2,
            CharsetIndex::G3,
        ];
        for (index, intro) in indices.into_iter().zip(finals) {
            if cursor.charsets[index] == StandardCharset::SpecialCharacterAndLineDrawing {
                let _ = write!(out.s, "\x1b{intro}0");
            }
        }
        if self.active_charset == CharsetIndex::G1 {
            out.s.push('\x0e');
        }

        // Keyboard: the kitty flags stack, and xterm's modifyOtherKeys.
        write_keyboard_stack(
            &mut out.s,
            &self.keyboard_mode_stack,
            self.keyboard_mode_idx,
        );
        if self.modify_other_keys != 0 {
            let _ = write!(out.s, "\x1b[>4;{}m", self.modify_other_keys);
        }

        // The pen: what the program writes next looks like this, and
        // carries the link it left open.
        let template = &cursor.template;
        out.style(cursor.pending_style);
        out.link(cell_link(grid, template));

        // The cursor's style, if a program chose one, and its visibility.
        if !self.cursor_is_default {
            let n = match self.cursor_shape {
                CursorShape::Block => Some(2),
                CursorShape::Underline => Some(4),
                CursorShape::Beam => Some(6),
                CursorShape::Hidden => None,
            };
            if let Some(n) = n {
                let _ = write!(out.s, "\x1b[{} q", n - self.blinking_cursor as u8);
            }
        }
        out.s.push_str(if self.mode.contains(Mode::SHOW_CURSOR) {
            "\x1b[?25h"
        } else {
            "\x1b[?25l"
        });

        // The title and working directory.
        let title: String = self.title.chars().filter(|c| !c.is_control()).collect();
        if !title.is_empty() {
            let _ = write!(out.s, "\x1b]2;{title}\x1b\\");
        }
        if let Some(dir) = self.current_directory.as_ref().and_then(|d| d.to_str()) {
            out.s.push_str("\x1b]7;file://");
            percent_encode_path(&mut out.s, dir);
            out.s.push_str("\x1b\\");
        }
    }

    fn push_cursor(&self, out: &mut String) {
        let pos = self.grid.cursor.pos;
        let row = pos.row.0.max(0) + 1;
        let col = pos.col.0 + 1;
        let _ = write!(out, "\x1b[{row};{col}H");
        // Restore cursor visibility (DECTCEM). A screen with a hidden cursor
        // is common while a TUI redraws; without this a restored snapshot
        // would leave a stray cursor on the client until the next output.
        if self.mode.contains(Mode::SHOW_CURSOR) {
            out.push_str("\x1b[?25h");
        } else {
            out.push_str("\x1b[?25l");
        }
    }
}

/// Give back the buffer's spare room when it's more than an eighth of
/// it: the estimate reserved up front, or its last doubling, would
/// otherwise stay allocated as long as the caller keeps the output.
pub(crate) fn shrink_slack(s: &mut String) {
    if s.capacity() - s.len() > s.len() / 8 {
        s.shrink_to_fit();
    }
}

/// The kitty keyboard flags stack, from the bottom: the bottom entry is set
/// in place (`CSI = flags ; 1 u`), the rest pushed (`CSI > flags u`).
fn write_keyboard_stack(out: &mut String, stack: &[u8], top: usize) {
    if stack[0] != 0 {
        let _ = write!(out, "\x1b[={};1u", stack[0]);
    }
    for &flags in &stack[1..=top.min(stack.len() - 1)] {
        let _ = write!(out, "\x1b[>{flags}u");
    }
}

/// A path for a `file://` URL: bytes outside the unreserved set (and `/`)
/// as `%XX`.
fn percent_encode_path(out: &mut String, path: &str) {
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
}

/// Rightmost column of `grid`'s row `line` with a non-blank glyph or a
/// style that renders on a blank cell (`Style::renders_on_blank`), or
/// `None` for a row with neither. Fg-only attributes must NOT count as
/// content: `ED`/`EL` reset cells with the full cursor template style, so
/// after `\x1b[31m` + `clear` every blank carries a red fg, and trimming on
/// any non-default style would then emit the whole screen as styled spaces.
fn row_last_col(
    grid: &Grid<Square>,
    line: i32,
    cols: usize,
    trim: bool,
) -> Option<usize> {
    if !trim {
        return Some(cols.saturating_sub(1));
    }
    let row = &grid[Line(line)][..];
    (0..cols.min(row.len())).rev().find(|&col| {
        let square = &row[col];
        // Most trailing cells were never written: no lookup for those.
        if square.is_default() {
            return false;
        }
        let c = square.c();
        (c != ' ' && c != '\0') || grid.style_of(square).renders_on_blank()
    })
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
                out.push_str("\x1b[58;5;");
                push_u8(out, i);
                out.push('m');
            }
            AnsiColor::Spec(rgb) => {
                out.push_str("\x1b[58;2;");
                push_rgb(out, rgb.r, rgb.g, rgb.b);
                out.push('m');
            }
            AnsiColor::Named(n) => {
                // Specials (Foreground = 256 and up) have no palette
                // index; SGR 58 cannot express them, so leave the reset's
                // unset underline color rather than emit an invalid index.
                if let Ok(idx) = u8::try_from(n as u16) {
                    out.push_str("\x1b[58;5;");
                    push_u8(out, idx);
                    out.push('m');
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
                params.push(';');
                push_u8(params, (base + n) as u8);
            } else if n <= 15 {
                let base = if fg { 90 } else { 100 };
                params.push(';');
                push_u8(params, (base + (n - 8)) as u8);
            }
            // Named default fg/bg (256/257) and specials: covered by the
            // leading `0` reset, so emit nothing.
        }
        AnsiColor::Indexed(i) => {
            let intro = if fg { "38" } else { "48" };
            params.push(';');
            params.push_str(intro);
            params.push_str(";5;");
            push_u8(params, i);
        }
        AnsiColor::Spec(rgb) => {
            let intro = if fg { "38" } else { "48" };
            params.push(';');
            params.push_str(intro);
            params.push_str(";2;");
            push_rgb(params, rgb.r, rgb.g, rgb.b);
        }
    }
}

/// `n` in decimal, without the formatting machinery (a snapshot can
/// write a color for every cell).
#[inline]
fn push_u8(out: &mut String, n: u8) {
    if n >= 100 {
        out.push((b'0' + n / 100) as char);
    }
    if n >= 10 {
        out.push((b'0' + n / 10 % 10) as char);
    }
    out.push((b'0' + n % 10) as char);
}

fn push_rgb(out: &mut String, r: u8, g: u8, b: u8) {
    push_u8(out, r);
    out.push(';');
    push_u8(out, g);
    out.push(';');
    push_u8(out, b);
}

/// Like `push_color`, but a color that `push_color` leaves to the reset
/// (the default, and specials with no SGR form) is written as the default
/// (39 / 49): for a change of colors only, which has no reset.
fn push_color_change(params: &mut String, color: AnsiColor, fg: bool) {
    match color {
        AnsiColor::Named(named) if named as u16 > 15 => {
            params.push_str(if fg { ";39" } else { ";49" });
        }
        _ => push_color(params, color, fg),
    }
}

/// The SGR that takes the client from `from` to `to`. When only the
/// colors differ, just those, as the attributes stand; otherwise
/// [`write_sgr`]'s full form.
fn write_sgr_change(out: &mut String, from: &Style, to: &Style) {
    let colors_only =
        to.flags == from.flags && to.underline_color == from.underline_color;
    if !colors_only || *to == Style::default() {
        write_sgr(out, to);
        return;
    }
    out.push_str("\x1b[");
    let start = out.len();
    if to.fg != from.fg {
        push_color_change(out, to.fg, true);
    }
    if to.bg != from.bg {
        push_color_change(out, to.bg, false);
    }
    // Drop the first parameter's separator.
    out.remove(start);
    out.push('m');
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
        assert!(text.contains("41m"), "missing red bg: {text:?}");

        let mut b = term(20, 4);
        feed(&mut b, &snapshot);
        let sb = *(&b.grid[Line(0)][Column(3)] as &Square);
        assert_eq!(b.grid.style_of(&sb).bg, AnsiColor::Named(NamedColor::Red));

        // A row made only of styled blanks must not be dropped as blank.
        let mut c = term(20, 4);
        feed(&mut c, b"top\r\n\x1b[44m    \x1b[0m");
        let text = String::from_utf8(c.contents_formatted()).unwrap();
        assert!(text.contains("44m"), "styled-blank row dropped: {text:?}");
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

/// [`FormatOptions::snapshot`]: a fresh terminal fed the snapshot must be the
/// original, and stay it as both get the same input afterwards.
#[cfg(test)]
mod snapshot_tests {
    use super::*;
    use crate::crosswords::{Crosswords, CrosswordsSize};
    use crate::event::{VoidListener, WindowId};
    use crate::performer::handler::Processor;

    struct T {
        term: Crosswords<VoidListener>,
        parser: Processor,
    }

    impl T {
        fn new(cols: usize, rows: usize) -> T {
            let term = Crosswords::new(
                CrosswordsSize::new(cols, rows),
                CursorShape::Block,
                VoidListener,
                WindowId::from(0),
                0,
                1_000,
            );
            T {
                term,
                parser: Processor::default(),
            }
        }

        fn feed(&mut self, bytes: &[u8]) -> &mut Self {
            self.parser.advance(&mut self.term, bytes);
            self
        }

        /// A fresh terminal of the same size, fed this one's snapshot.
        fn copy(&self) -> T {
            let mut t = T::new(self.term.columns(), self.term.screen_lines());
            t.feed(&self.term.snapshot_formatted());
            t
        }

        /// Text of the history and the screen.
        fn text(&self) -> String {
            self.term
                .format(FormatOptions::plain().with_scrollback(true))
        }

        fn cursor(&self) -> (Pos, bool) {
            (self.term.grid.cursor.pos, self.term.grid.cursor.should_wrap)
        }

        /// Every linked cell: (line, column, uri).
        fn links(&self) -> Vec<(i32, usize, String)> {
            let grid = &self.term.grid;
            let top = -(grid.history_size() as i32);
            let mut out = Vec::new();
            for line in top..grid.screen_lines() as i32 {
                for col in 0..grid.columns() {
                    if let Some(l) = self.term.cell_hyperlink(Line(line), Column(col)) {
                        out.push((line, col, l.uri().to_owned()));
                    }
                }
            }
            out
        }
    }

    /// The same input to both, and they agree.
    fn same(a: &mut T, b: &mut T, bytes: &[u8]) {
        a.feed(bytes);
        b.feed(bytes);
        assert_eq!(a.text(), b.text());
        assert_eq!(a.cursor(), b.cursor());
    }

    fn assert_same(a: &T, b: &T) {
        assert_eq!(a.text(), b.text());
        assert_eq!(a.term.grid.history_size(), b.term.grid.history_size());
        assert_eq!(a.cursor(), b.cursor());
        assert_eq!(a.term.mode().bits(), b.term.mode().bits());
        // And the copy's snapshot is the original's.
        assert_eq!(
            String::from_utf8(a.term.snapshot_formatted()).unwrap(),
            String::from_utf8(b.term.snapshot_formatted()).unwrap()
        );
    }

    #[test]
    fn history_comes_along() {
        let mut a = T::new(20, 5);
        for i in 0..40 {
            a.feed(format!("line {i}\r\n").as_bytes());
        }
        a.feed(b"$ ");
        assert!(a.term.grid.history_size() > 30);
        let b = a.copy();
        assert_same(&a, &b);
        assert!(b.text().starts_with("line 0\n"));
        // What was reserved and not used is given back.
        let s = a.term.format(FormatOptions::snapshot());
        assert!(s.capacity() - s.len() <= s.len() / 8);
    }

    #[test]
    fn a_mostly_blank_screen_under_history_stays_in_place() {
        // The screen's content in its top rows, history above: written as
        // is, the history would slide down onto the screen.
        let mut a = T::new(20, 6);
        for i in 0..30 {
            a.feed(format!("old {i}\r\n").as_bytes());
        }
        a.feed(b"\x1b[H\x1b[Jtop");
        let mut b = a.copy();
        assert_same(&a, &b);
        assert_eq!(b.term.grid[Line(0)][Column(0)].c(), 't');
        same(&mut a, &mut b, b"!\r\nnext");
    }

    #[test]
    fn soft_wrapped_lines_stay_soft_wrapped() {
        let mut a = T::new(80, 6);
        a.feed(format!("{}\r\n$ ", "x".repeat(85)).as_bytes());
        let mut b = a.copy();
        assert_same(&a, &b);
        assert!(b.term.grid[Line(0)][Column(79)].wrapline());
        // Both reflow alike.
        a.term.resize(CrosswordsSize::new(100, 6));
        b.term.resize(CrosswordsSize::new(100, 6));
        assert_eq!(a.text(), b.text());
        assert!(b.text().starts_with(&format!("{}\n", "x".repeat(85))));
    }

    #[test]
    fn a_wrapped_row_over_a_blank_one() {
        // A full row flagged as wrapping into a row that was erased: the
        // blank row must stay a row.
        let mut a = T::new(10, 5);
        a.feed(b"0123456789abc\x1b[2;1H\x1b[2K\x1b[3;1Hthird");
        let b = a.copy();
        assert_eq!(a.text(), b.text());
        assert_eq!(b.term.grid[Line(2)][Column(0)].c(), 't');
    }

    #[test]
    fn cursor_pen_and_typing_after() {
        let mut a = T::new(40, 6);
        a.feed(b"\x1b[1;31mred\x1b[0m plain\r\nnext\x1b[2;3H\x1b[32m");
        let mut b = a.copy();
        assert_same(&a, &b);
        same(&mut a, &mut b, b"Z");
        let style = |t: &T| t.term.grid.style_of(&t.term.grid[Line(1)][Column(2)]);
        assert_eq!(style(&a), style(&b));
        assert_ne!(style(&b), Style::default());
    }

    #[test]
    fn pending_wrap_survives() {
        let mut a = T::new(10, 4);
        a.feed(b"\x1b[33m0123456789");
        assert!(a.term.grid.cursor.should_wrap);
        let mut b = a.copy();
        assert_same(&a, &b);
        same(&mut a, &mut b, b"Z");

        // Under a wide character too.
        let mut a = T::new(10, 4);
        a.feed("01234567界".as_bytes());
        assert!(a.term.grid.cursor.should_wrap);
        let mut b = a.copy();
        assert_same(&a, &b);
        same(&mut a, &mut b, b"Z");
    }

    #[test]
    fn wide_and_combining_characters() {
        let mut a = T::new(10, 4);
        // A wide character that doesn't fit at the end of a row (it leaves
        // a leading spacer and wraps), and a combining mark.
        a.feed("123456789界 e\u{301}!".as_bytes());
        let b = a.copy();
        assert_same(&a, &b);
        assert!(b.text().contains("e\u{301}!"), "{:?}", b.text());
    }

    #[test]
    fn alt_screen_over_primary_history() {
        let mut a = T::new(40, 4);
        for i in 0..10 {
            a.feed(format!("primary {i}\r\n").as_bytes());
        }
        a.feed(b"$ vim\x1b[?1049h\x1b[H\x1b[2Jalt-text\x1b[3;5H");
        let mut b = a.copy();
        assert!(b.term.mode().contains(Mode::ALT_SCREEN));
        assert_same(&a, &b);
        // Leaving the alt screen shows the same primary screen, with the
        // cursor where the program found it.
        same(&mut a, &mut b, b"\x1b[?1049l");
        assert!(b.text().contains("primary 9"));
        assert_eq!(b.term.grid.history_size(), a.term.grid.history_size());
        same(&mut a, &mut b, b"x");
    }

    #[test]
    fn alt_screen_with_a_scroll_region() {
        // A TUI with a status line: the region spans the rows between.
        let mut a = T::new(40, 8);
        a.feed(b"shell\r\n\x1b[?1049h\x1b[H\x1b[2Jtitle\r\n\x1b[2;7r\x1b[3;1Hbody\x1b[8;1Hstatus\x1b[4;5H");
        let mut b = a.copy();
        assert_same(&a, &b);
        assert_eq!(b.term.scroll_region, a.term.scroll_region);
        same(&mut a, &mut b, b"Z");
        // A newline at the region's bottom scrolls only the region.
        same(&mut a, &mut b, b"\x1b[7;1Hlast\n");
        assert!(b.text().trim_end().ends_with("status"));
    }

    #[test]
    fn origin_mode_places_the_cursor_in_the_region() {
        let mut a = T::new(20, 8);
        a.feed(b"\x1b[3;6r\x1b[?6h\x1b[2;4H");
        let mut b = a.copy();
        assert_same(&a, &b);
        same(&mut a, &mut b, b"Z\x1b[1;1HA");
    }

    #[test]
    fn modes_survive() {
        let mut a = T::new(40, 4);
        // Bracketed paste, app cursor keys, SGR mouse with drag tracking,
        // focus events, app keypad, insert mode, no autowrap, no 2027.
        a.feed(b"\x1b[?2004h\x1b[?1h\x1b[?1002h\x1b[?1006h\x1b[?1004h\x1b=\x1b[4h\x1b[?7l\x1b[?2027l");
        let b = a.copy();
        assert_same(&a, &b);
        for m in [
            Mode::BRACKETED_PASTE,
            Mode::APP_CURSOR,
            Mode::MOUSE_DRAG,
            Mode::SGR_MOUSE,
            Mode::FOCUS_IN_OUT,
            Mode::APP_KEYPAD,
            Mode::INSERT,
        ] {
            assert!(b.term.mode().contains(m), "{m:?}");
        }
        assert!(!b.term.mode().contains(Mode::LINE_WRAP));
        assert!(!b.term.mode().contains(Mode::GRAPHEME_CLUSTER));
    }

    #[test]
    fn keyboard_flags_survive_on_both_screens() {
        let mut a = T::new(40, 4);
        a.feed(b"\x1b[=1;1u\x1b[>3u\x1b[>4;2m\x1b[?1049h\x1b[>5u");
        let mut b = a.copy();
        assert_same(&a, &b);
        assert_eq!(b.term.keyboard_mode(), a.term.keyboard_mode());
        assert_eq!(b.term.modify_other_keys, 2);
        // Popping goes back through the same stack, on each screen.
        same(&mut a, &mut b, b"\x1b[<1u");
        assert_eq!(b.term.keyboard_mode(), a.term.keyboard_mode());
        same(&mut a, &mut b, b"\x1b[?1049l");
        assert_eq!(b.term.keyboard_mode(), a.term.keyboard_mode());
        same(&mut a, &mut b, b"\x1b[<1u");
        assert_eq!(b.term.keyboard_mode(), a.term.keyboard_mode());
    }

    #[test]
    fn hyperlinks_survive() {
        let mut a = T::new(12, 4);
        // In the history, after a wide character, wrapping onto the next
        // row, styled, and two on a line.
        a.feed("界 \x1b]8;;https://x.dev/a\x1b\\lbl\x1b]8;;\x1b\\ e\r\n".as_bytes());
        a.feed(
            b"see \x1b]8;id=k;https://x.dev/long;q\x1b\\0123456789ab\x1b]8;;\x1b\\.\r\n",
        );
        a.feed(b"\x1b[1;34m\x1b]8;;https://y.io\x1b\\y\x1b]8;;\x1b\\\x1b[0m and \x1b]8;;https://z.io\x1b\\z\x1b]8;;\x1b\\\r\n");
        a.feed(b"one\r\ntwo\r\nthree\r\nfour");
        assert!(a.term.grid.history_size() > 0);
        let b = a.copy();
        assert_eq!(a.text(), b.text());
        let want = a.links();
        assert!(
            want.iter().any(|(_, _, u)| u == "https://x.dev/long;q"),
            "{want:?}"
        );
        assert_eq!(b.links(), want);
        // An explicit id stays; a made-up one isn't sent.
        let snap = String::from_utf8(a.term.snapshot_formatted()).unwrap();
        assert!(snap.contains("\x1b]8;id=k;https://x.dev/long;q"));
        assert!(!snap.contains("_rio"));
    }

    #[test]
    fn an_open_link_stays_open() {
        let mut a = T::new(20, 4);
        a.feed(b"\x1b]8;;https://open.dev\x1b\\abc");
        let mut b = a.copy();
        same(&mut a, &mut b, b"def\x1b]8;;\x1b\\ghi");
        assert_eq!(b.links(), a.links());
        assert_eq!(b.links().len(), 6);
    }

    #[test]
    fn charsets_survive() {
        let mut a = T::new(20, 4);
        a.feed(b"\x1b(0lqk\x1b)B");
        let mut b = a.copy();
        assert_same(&a, &b);
        same(&mut a, &mut b, b"qx");
        assert!(b.text().contains("┌─┐─│"), "{:?}", b.text());
    }

    #[test]
    fn tab_stops_survive() {
        let mut a = T::new(40, 4);
        a.feed(b"\x1b[3g\x1b[6G\x1bH\x1b[20G\x1bH\r");
        let mut b = a.copy();
        same(&mut a, &mut b, b"\tA\tB\tC");
        assert_eq!(b.term.grid[Line(0)][Column(5)].c(), 'A');
        assert_eq!(b.term.grid[Line(0)][Column(19)].c(), 'B');
        // Untouched stops aren't written.
        let c = T::new(40, 4);
        let snap = String::from_utf8(c.term.snapshot_formatted()).unwrap();
        assert!(!snap.contains("\x1b[3g"));
    }

    #[test]
    fn saved_cursor_survives() {
        let mut a = T::new(20, 6);
        a.feed(b"\x1b[3;4H\x1b[35m\x1b7\x1b[0m\x1b[6;1Hend");
        let mut b = a.copy();
        assert_same(&a, &b);
        same(&mut a, &mut b, b"\x1b8S");
        let style = |t: &T| t.term.grid.style_of(&t.term.grid[Line(2)][Column(3)]);
        assert_eq!(style(&a), style(&b));
    }

    #[test]
    fn cursor_style_title_and_directory() {
        let mut a = T::new(20, 4);
        a.feed(b"\x1b[5 q\x1b]2;my title\x07\x1b]7;file:///tmp/a%20b\x07\x1b[?25l");
        let b = a.copy();
        assert_same(&a, &b);
        assert_eq!(b.term.cursor_shape, CursorShape::Beam);
        assert!(b.term.blinking_cursor);
        assert_eq!(b.term.title, "my title");
        assert_eq!(
            b.term.current_directory.as_deref(),
            Some(std::path::Path::new("/tmp/a b"))
        );
        assert!(!b.term.mode().contains(Mode::SHOW_CURSOR));
    }

    #[test]
    fn a_fresh_terminal_is_almost_nothing() {
        let a = T::new(20, 4);
        let snap = String::from_utf8(a.term.snapshot_formatted()).unwrap();
        assert_eq!(snap, "\x1b[0m\x1b[2J\x1b[H\x1b[3J\x1b[1;1H\x1b[?25h");
    }

    #[test]
    fn plain_with_scrollback() {
        let mut a = T::new(20, 3);
        a.feed(b"a\r\nb\r\nc\r\nd\r\ne");
        assert_eq!(a.term.format(FormatOptions::plain()), "c\nd\ne");
        assert_eq!(a.text(), "a\nb\nc\nd\ne");
    }
}

#[cfg(test)]
mod sgr_change_tests {
    use super::*;
    use crate::crosswords::{Crosswords, CrosswordsSize};
    use crate::event::{VoidListener, WindowId};
    use crate::performer::handler::Processor;

    fn term() -> Crosswords<VoidListener> {
        Crosswords::new(
            CrosswordsSize::new(40, 3),
            CursorShape::Block,
            VoidListener,
            WindowId::from(0),
            0,
            0,
        )
    }

    #[test]
    fn color_changes_come_out_the_same() {
        let mut a = term();
        // fg changes, a bg on top, fg back to default with the bg kept,
        // bright and rgb colors, then attributes changing under one color.
        Processor::default().advance(
            &mut a,
            b"\x1b[31ma\x1b[32mb\x1b[44mc\x1b[39md\x1b[91me\x1b[38;2;1;2;3mf\
              \x1b[38;5;200mg\x1b[49mh\x1b[1mi\x1b[22mj\x1b[4:3;58;5;9mk\x1b[33ml\x1b[0mm",
        );
        let snap = a.snapshot_formatted();
        let text = String::from_utf8(snap.clone()).unwrap();
        // Color-only changes have no reset.
        assert!(text.contains("\x1b[32mb"), "{text:?}");
        assert!(text.contains("\x1b[39md"), "{text:?}");
        let mut b = term();
        Processor::default().advance(&mut b, &snap);
        for col in 0..13 {
            let (sa, sb) = (a.grid[Line(0)][Column(col)], b.grid[Line(0)][Column(col)]);
            assert_eq!(sa.c(), sb.c());
            assert_eq!(a.grid.style_of(&sa), b.grid.style_of(&sb), "column {col}");
        }
    }

    #[test]
    fn digits_without_formatting() {
        for n in [0u8, 7, 9, 10, 42, 99, 100, 199, 255] {
            let mut s = String::new();
            push_u8(&mut s, n);
            assert_eq!(s, n.to_string());
        }
    }
}
