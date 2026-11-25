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
//! Not carried: palette changes (OSC 4/10/11/12; the client keeps its
//! theme) unless [`FormatOptions::palette`] asks for them,
//! the title stack, images, and a half-parsed escape sequence (that is the
//! parser's, not the terminal's). A tab's `\t` (kept where it started, so
//! copying text gives the tab back) is written as a blank: written raw it
//! would move the client's cursor. Half of a wide character whose other
//! half is gone (inserting or deleting characters can split one) is a
//! blank in its style. A cursor waiting to wrap at the end of a row that
//! wraps on (a line feed or a restored saved cursor can leave one) keeps
//! the row's wrap where a row next to it doesn't wrap, or the saved cursor
//! is that same one. The blank a wide
//! character leaves at the end of
//! the row it didn't fit on takes the character's style and link. A soft wrap
//! flagged on the bottom row (a scroll
//! down can leave one, wrapping into nothing) is not carried: only writing
//! past the bottom sets it.

use super::Crosswords;
use crate::ansi::CursorShape;
use crate::config::colors::AnsiColor;
use crate::crosswords::grid::{Dimensions, Grid};
use crate::crosswords::pos::{
    CharsetIndex, Charsets, Column, Line, Pos, StandardCharset,
};
use crate::crosswords::square::{ContentTag, Hyperlink, Square, Wide};
use crate::crosswords::style::{Style, StyleFlags, StyleId};
use crate::crosswords::Mode;
use crate::event::EventListener;
use std::fmt::Write as _;
use std::io;

/// How much a streaming [`Crosswords::format_to`] buffers before writing
/// to its sink: a few dozen rows of styled text, small enough to stay in
/// cache, large enough that the sink sees few writes.
const CHUNK: usize = 64 * 1024;

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
    /// `Vt`: the colors programs set (OSC 4 palette entries, OSC 10/11/12
    /// foreground, background and cursor). Off in every preset: a client
    /// keeps its own theme. A checkpoint (a terminal rebuilt in place of
    /// this one, such as after a server restart) wants them.
    pub palette: bool,
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
            palette: false,
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
            palette: false,
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
            palette: false,
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

    pub fn with_palette(mut self, on: bool) -> Self {
        self.palette = on;
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
    (Mode::SYNC_UPDATE, 2026),
    (Mode::GRAPHEME_CLUSTER, 2027),
    (Mode::SIXEL_DISPLAY, 80),
    (Mode::SIXEL_PRIV_PALETTE, 1070),
    (Mode::SIXEL_CURSOR_TO_THE_RIGHT, 8452),
];

/// ANSI modes (SM/RM numbers).
const ANSI_MODES: &[(Mode, u16)] = &[(Mode::INSERT, 4), (Mode::LINE_FEED_NEW_LINE, 20)];

/// The charset slots and the intermediate that designates each.
const CHARSETS: [(CharsetIndex, char); 4] = [
    (CharsetIndex::G0, '('),
    (CharsetIndex::G1, ')'),
    (CharsetIndex::G2, '*'),
    (CharsetIndex::G3, '+'),
];

/// The slots of `charsets` that hold DEC line drawing, with their
/// designating intermediates.
fn line_drawing(charsets: &Charsets) -> impl Iterator<Item = (CharsetIndex, char)> + '_ {
    CHARSETS.into_iter().filter(|&(index, _)| {
        charsets[index] == StandardCharset::SpecialCharacterAndLineDrawing
    })
}

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
struct Out<'w> {
    s: String,
    /// Where `s` goes when streaming ([`Crosswords::format_to`]): written
    /// out and cleared between rows once it holds [`CHUNK`] bytes. `None`
    /// builds the whole output in `s`.
    sink: Option<&'w mut dyn io::Write>,
    /// How full `s` gets before it goes to the sink.
    chunk: usize,
    /// The sink's first error; nothing more is written after it.
    error: Option<io::Error>,
    style: Style,
    link: Option<Hyperlink>,
    /// The style id (in the grid being written) that `style` came from,
    /// so a run of cells with one style costs an id compare per cell, not
    /// a style lookup and compare. `None` when unknown.
    style_id: Option<StyleId>,
    /// The same for `link`: the extras id it came from.
    link_id: Option<u16>,
}

impl<'w> Out<'w> {
    fn new(capacity: usize, sink: Option<&'w mut dyn io::Write>) -> Out<'w> {
        Out {
            s: String::with_capacity(capacity),
            sink,
            chunk: CHUNK,
            error: None,
            style: Style::default(),
            link: None,
            style_id: None,
            link_id: None,
        }
    }

    /// Streaming: write `s` to the sink and clear it once it holds a
    /// chunk. False once the sink failed (stop writing).
    #[inline]
    fn drain(&mut self) -> bool {
        if self.s.len() >= self.chunk {
            self.write_out();
        }
        self.error.is_none()
    }

    #[cold]
    fn write_out(&mut self) {
        if let Some(sink) = self.sink.as_mut() {
            if self.error.is_none() {
                if let Err(e) = sink.write_all(self.s.as_bytes()) {
                    self.error = Some(e);
                }
            }
            self.s.clear();
        }
    }

    /// Streaming: the rest of `s` to the sink, and its first error.
    fn finish(mut self) -> io::Result<()> {
        self.write_out();
        match self.error {
            Some(e) => Err(e),
            None => Ok(()),
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
            let style = grid.style_of(&square);
            // Styles are interned: another id of the grid's is another
            // style, so there's nothing to compare.
            if self.style_id.is_some() && id.is_some() {
                write_sgr_change(&mut self.s, &self.style, &style);
                self.style = style;
            } else {
                self.style(style);
            }
            self.style_id = id;
        }
        if square.extras_id_checked().is_some() {
            // Combining marks live in the extras.
            for c in grid.cell_text(pos) {
                self.s.push(visible(c));
            }
        } else {
            let c = square.c();
            self.s.push(visible(c));
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

/// Whether `row[at]` is half of a wide character whose other half is gone:
/// a spacer with no wide character before it, or a wide character with no
/// spacer after it (the last column has no room for one).
fn broken_half(row: &[Square], at: usize) -> bool {
    let square = row[at];
    if square.is_spacer() {
        at == 0 || !row[at - 1].is_wide()
    } else if square.is_wide() {
        at + 1 >= row.len() || !row[at + 1].is_spacer()
    } else {
        false
    }
}

/// What a cell's character is written as: a cell never written is a
/// blank, and so is one holding a control character (the `\t` a tab
/// leaves where it started), which written raw would move the client's
/// cursor instead of filling the cell.
#[inline]
fn visible(c: char) -> char {
    if (c as u32) < 0x20 || (0x7f..0xa0).contains(&(c as u32)) {
        ' '
    } else {
        c
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

    /// Serialize the terminal using `opts` into `w`, a chunk at a time
    /// (about 64 KiB): the same bytes [`format`](Self::format) returns,
    /// without building them all in one `String` first (which grows to
    /// the whole output, 15 MB for a big history, and is copied each time
    /// it does). Stops at `w`'s first error and returns it. [`Emit::Plain`]
    /// is built whole, then written.
    ///
    /// The caller holds whatever guards `self` (the terminal's lock) until
    /// this returns, `w`'s writes included, so give it a sink that never
    /// waits on anyone: a `Vec<u8>` kept between snapshots (cleared, not
    /// dropped, so its capacity is reused), a file, a pipe with room. Not
    /// a client's socket: a slow client would stall the PTY reader, and
    /// the program behind it. Format under the lock, then send after
    /// releasing it, from another thread if need be.
    pub fn format_to<W: io::Write>(
        &self,
        opts: FormatOptions,
        w: &mut W,
    ) -> io::Result<()> {
        self.stream(opts, w, CHUNK)
    }

    fn stream(
        &self,
        opts: FormatOptions,
        w: &mut dyn io::Write,
        chunk: usize,
    ) -> io::Result<()> {
        match opts.emit {
            Emit::Plain => w.write_all(self.format_plain(opts).as_bytes()),
            Emit::Vt => {
                let mut out = Out::new(chunk + chunk / 4, Some(w));
                out.chunk = chunk;
                self.write_vt(&mut out, opts);
                out.finish()
            }
        }
    }

    /// [`snapshot_formatted`](Self::snapshot_formatted) into `w`, a chunk
    /// at a time: see [`format_to`](Self::format_to).
    pub fn snapshot_to<W: io::Write>(&self, w: &mut W) -> io::Result<()> {
        self.format_to(FormatOptions::snapshot(), w)
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
    /// waits, for the whole walk: about 1.1 ms for 10k lines of styled
    /// 120-column history, and about 13 ms when every cell has a color of
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
        // Lines of text run about half the width in escapes and glyphs.
        let lines = self.screen_lines()
            + if opts.scrollback {
                self.grid.history_size()
            } else {
                0
            };
        let mut out = Out::new(lines * (self.columns() / 2 + 8), None);
        self.write_vt(&mut out, opts);
        shrink_slack(&mut out.s);
        out.s
    }

    fn write_vt(&self, out: &mut Out, opts: FormatOptions) {
        if self.screen_lines() == 0 || self.columns() == 0 {
            return;
        }
        // Reset, clear, home so the snapshot renders on a fresh client.
        out.s.push_str("\x1b[0m\x1b[2J\x1b[H");
        if opts.scrollback {
            out.s.push_str("\x1b[3J");
        }

        if opts.palette {
            self.write_palette(out);
        }

        let alt = opts.state && self.mode.contains(Mode::ALT_SCREEN);
        if !alt {
            if opts.state {
                self.write_alt_state(out);
            }
            self.write_rows(out, &self.grid, opts);
            out.plain();
            if opts.state {
                self.write_state(out);
            } else {
                self.push_cursor(&mut out.s);
            }
            return;
        }

        // An alt-screen program runs: the primary screen first, with its
        // cursor where the program found it (where leaving the alt screen
        // puts it back), then the alt screen over it.
        let primary = &self.inactive_grid;
        self.write_rows(out, primary, opts);
        out.plain();
        write_keyboard_stack(
            &mut out.s,
            &self.inactive_keyboard_mode_stack,
            self.inactive_keyboard_mode_idx,
        );
        // Entering the alt screen saves origin mode with the cursor, and
        // leaving restores it: set it as it was saved, and reset it once
        // in (no region is set yet, so the cursor's row is the screen's).
        if primary.saved_origin {
            out.s.push_str("\x1b[?6h");
        }
        let at = primary.cursor.pos;
        self.place_cursor(
            out,
            primary,
            (at.row.0, at.col.0),
            0,
            primary.cursor.should_wrap,
        );
        // Its pending wrap's rewrite cleared the row's soft wrap: write
        // the next row's first cell over itself to wrap again, and come
        // back through the saved cursor (entering the alt screen saves it
        // anew, so borrowing it costs nothing).
        let rows = primary.screen_lines() as i32;
        let row = at.row.0;
        if primary.cursor.should_wrap
            && row + 1 < rows
            && primary[Line(row)][Column(primary.columns() - 1)].wrapline()
        {
            out.s.push_str("\x1b7");
            let next = Pos::new(Line(row + 1), Column(0));
            out.cell(primary, primary[next], next, true);
            out.plain();
            out.s.push_str("\x1b8");
        }
        // Its pen too (what leaving the alt screen goes back to writing
        // with). Entering the alt screen clears it with that pen, so clear
        // it again with none.
        // So are its charsets; the alt screen's cursor starts as a copy,
        // so its own go back to ASCII until its rows are written.
        out.style(primary.cursor.pending_style);
        out.link(cell_link(primary, &primary.cursor.template));
        for (_, intro) in line_drawing(&primary.cursor.charsets) {
            let _ = write!(out.s, "\x1b{intro}0");
        }
        out.s.push_str("\x1b[?1049h");
        if primary.saved_origin {
            out.s.push_str("\x1b[?6l");
        }
        for (_, intro) in line_drawing(&primary.cursor.charsets) {
            let _ = write!(out.s, "\x1b{intro}B");
        }
        out.plain();
        out.s.push_str("\x1b[H\x1b[2J");
        self.write_rows(out, &self.grid, opts.with_scrollback(false));
        out.plain();
        self.write_state(out);
    }

    /// What the alt screen keeps while the primary one is in use: its kitty
    /// keyboard flags and its saved cursor (DECSC), which the next alt
    /// session starts with. Its contents go: entering it clears them. Set
    /// on a visit to it before anything else is written (leaving it puts
    /// back the cursor entering saved, the home position).
    fn write_alt_state(&self, out: &mut Out) {
        let alt = &self.inactive_grid;
        let saved = &alt.saved_cursor;
        let saved_link = cell_link(alt, &saved.template);
        let keyboard = self.inactive_keyboard_mode_idx != 0
            || self.inactive_keyboard_mode_stack.iter().any(|&f| f != 0);
        let cursor = saved.pos != Pos::default()
            || saved.pending_style != Style::default()
            || saved_link.is_some()
            || line_drawing(&saved.charsets).next().is_some()
            || saved.should_wrap
            || alt.saved_origin;
        if !keyboard && !cursor {
            return;
        }
        out.s.push_str("\x1b[?1049h");
        write_keyboard_stack(
            &mut out.s,
            &self.inactive_keyboard_mode_stack,
            self.inactive_keyboard_mode_idx,
        );
        if cursor {
            if alt.saved_origin {
                out.s.push_str("\x1b[?6h");
            }
            let at = (saved.pos.row.0, saved.pos.col.0);
            self.place_cursor(out, alt, at, 0, saved.should_wrap);
            self.save_cursor(out, saved, saved_link);
            if alt.saved_origin {
                out.s.push_str("\x1b[?6l");
            }
        }
        out.s.push_str("\x1b[?1049l");
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
            (0..rows).rev().find(|&line| {
                row_last_col(grid, line, cols, true, opts.hyperlinks).is_some()
                    // A blank row's soft wrap is worth keeping too.
                    || (opts.unwrap
                        && line + 1 < rows
                        && grid[Line(line)][Column(cols - 1)].wrapline())
            })
        };
        let Some(mut last) = last else { return };
        let last_col = Column(cols - 1);
        // A row kept that wraps onto a trimmed one keeps that one too, so
        // the wrap comes along (as a blank, below).
        while opts.unwrap && last + 1 < rows && grid[Line(last)][last_col].wrapline() {
            last += 1;
        }
        let mut wrapped = false;
        // What this call writes starts here: an alt screen's rows follow
        // the primary's in the same buffer.
        let start = out.s.len();

        for line in top..=last {
            // Once a few rows are written, reserve what all of them will
            // take at that rate: one move of the buffer, not a doubling's
            // worth when the rows are heavier than estimated.
            if out.sink.is_some() {
                if !out.drain() {
                    return;
                }
            } else if line == top + 64 {
                let per_row = (out.s.len() - start) / 64 + 1;
                let left = (last - line + 1) as usize;
                out.s.reserve(per_row * left + per_row * left / 16);
            }
            let row = &grid[Line(line)];
            // A soft wrap is left to the client: the full row's pending
            // wrap makes it wrap at the next character.
            let wraps = opts.unwrap && line < last && row[last_col].wrapline();
            let end = if wraps {
                Some(cols - 1)
            } else {
                row_last_col(grid, line, cols, opts.trim, opts.hyperlinks)
            };
            if wrapped && end.is_none() {
                // Wrapped onto a blank row: one blank makes the client wrap.
                out.plain();
                out.s.push(' ');
            }
            let continues = wrapped;
            wrapped = wraps;
            if let Some(end) = end {
                let cells = &row[..][..=end];
                let mut col = 0;
                while col < cells.len() {
                    // Most cells are ASCII in the style already written,
                    // with no link to close: copy the run of them as bytes.
                    // With a link open, ones in that link (by its id).
                    let link = match (&out.link, out.link_id) {
                        (None, _) => Some(None),
                        (Some(_), Some(id)) => Some(Some(id)),
                        (Some(_), None) => None,
                    };
                    if let (Some(id), Some(link)) = (out.style_id, link) {
                        let run = ascii_run(&mut out.s, &cells[col..], id, link);
                        col += run;
                        if col == cells.len() {
                            break;
                        }
                    }
                    let square = cells[col];
                    let at = col;
                    col += 1;
                    // Wide characters, by the cell's half. The client makes
                    // a pair's second half itself, and the blank a wide
                    // character leaves where it didn't fit when one starts
                    // the next row. What it can't make is written as a
                    // blank in the cell's style, which keeps the columns
                    // where they are: half of a pair that lost its other
                    // half (inserting or deleting characters can split
                    // one), that blank where no wide character follows
                    // (overwritten since), and a character it gives two
                    // columns in a narrow cell, unless the cell after it
                    // is the orphaned half it then takes.
                    let pos = Pos::new(Line(line), Column(at));
                    let blank = match square.wide() {
                        Wide::Narrow => {
                            if square.c() as u32 >= 0x80 && squeezed(grid, &square, pos) {
                                // A cluster made wide in the last column,
                                // where autowrap was off: written so again.
                                if at + 1 == cols
                                    && !row[last_col].wrapline()
                                    && squeezed_cluster(grid, &square, pos)
                                {
                                    out.s.push_str("\x1b[?7l");
                                    out.cell(grid, square, pos, opts.hyperlinks);
                                    out.s.push_str("\x1b[?7h");
                                    continue;
                                }
                                let orphan_next =
                                    at + 1 < cols && row[Column(at + 1)].is_spacer();
                                if orphan_next && at + 1 < cells.len() {
                                    col += 1;
                                }
                                !orphan_next
                            } else {
                                false
                            }
                        }
                        Wide::Wide => !(at + 1 < cols && row[Column(at + 1)].is_spacer()),
                        Wide::Spacer => {
                            if at > 0 && row[Column(at - 1)].is_wide() {
                                continue;
                            }
                            true
                        }
                        Wide::LeadingSpacer => {
                            // Unless that character is written as a blank
                            // (it lost its other half), which fits here.
                            if line + 1 < rows
                                && grid[Line(line + 1)][Column(0)].is_wide()
                                && !broken_half(&grid[Line(line + 1)][..], 0)
                            {
                                continue;
                            }
                            true
                        }
                    };
                    if blank {
                        out.link(if opts.hyperlinks {
                            cell_link(grid, &square)
                        } else {
                            None
                        });
                        out.style(grid.style_of(&square));
                        out.s.push(' ');
                        continue;
                    }
                    // A neighbour the client would join to this cell (as
                    // it was written before it): written first, then this
                    // one, then on past both. Not one ending the row,
                    // whose pending wrap the way back would lose.
                    let width = if square.is_wide() { 2 } else { 1 };
                    let next = at + width;
                    if next < cells.len() {
                        let after = cells[next];
                        let next_width = match after.wide() {
                            Wide::Narrow => {
                                let p = Pos::new(Line(line), Column(next));
                                (!squeezed(grid, &after, p)).then_some(1)
                            }
                            Wide::Wide
                                if next + 1 < cols
                                    && row[Column(next + 1)].is_spacer() =>
                            {
                                Some(2)
                            }
                            _ => None,
                        };
                        let next_pos = Pos::new(Line(line), Column(next));
                        if let Some(nw) = next_width.filter(|nw| next + nw < cols) {
                            if would_join(grid, pos, next_pos) {
                                let _ = write!(out.s, "\x1b[{width}C");
                                out.cell(grid, after, next_pos, opts.hyperlinks);
                                let _ = write!(out.s, "\x1b[{}D", width + nw);
                                out.cell(grid, square, pos, opts.hyperlinks);
                                let _ = write!(out.s, "\x1b[{nw}C");
                                col = next + nw;
                                continue;
                            }
                        }
                    }
                    out.cell(grid, square, pos, opts.hyperlinks);
                }
                // The client wrapped onto this row at its first character,
                // and a wrap at the bottom scrolls in a row filled with the
                // pen of the moment, which no reset before a line break can
                // spare: if that pen shows on a blank, blank the trimmed
                // rest of the row again.
                if continues && end + 1 < cols {
                    let first = if row[Column(0)].is_spacer() { 1 } else { 0 };
                    if grid.style_of(&row[Column(first)]).renders_on_blank() {
                        out.plain();
                        out.s.push_str("\x1b[K");
                    }
                }
            }
            if line < last && !wraps {
                // A newline at the bottom scrolls in a row the client
                // fills with the pen: go plain first if it would show on a
                // blank (a background, inverse, an underline, a strikeout).
                if out.style.renders_on_blank() {
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
            let line = &grid[Line(row)][..];
            // Under a wide character the cursor is on its second column.
            let pair = line[col].is_spacer() && col > 0 && line[col - 1].is_wide();
            let at = if pair { col - 1 } else { col };
            let square = &line[at];
            let _ = write!(out.s, "\x1b[{to};{}H", at + 1);
            out.link(cell_link(grid, square));
            out.style(grid.style_of(square));
            // Written as the rows wrote it: half of a split pair, or a
            // double-width character in a narrow cell, is a blank (here at
            // the row's end it would wrap); a cluster made wide there,
            // with autowrap off.
            let pos = Pos::new(Line(row), Column(at));
            if squeezed_cluster(grid, square, pos) {
                out.s.push_str("\x1b[?7l");
                for c in grid.cell_text(pos) {
                    out.s.push(visible(c));
                }
                out.s.push_str("\x1b[?7h");
            } else if broken_half(line, at) || squeezed(grid, square, pos) {
                out.s.push(' ');
            } else {
                for c in grid.cell_text(Pos::new(Line(row), Column(at))) {
                    out.s.push(visible(c));
                }
            }
            out.plain();
        } else {
            let _ = write!(out.s, "\x1b[{to};{}H", col + 1);
        }
    }

    /// DECSC with `saved`'s pen, link and charsets, at the client's
    /// cursor; then back to a plain pen and ASCII.
    fn save_cursor(
        &self,
        out: &mut Out,
        saved: &crate::crosswords::pos::Cursor<Square>,
        link: Option<Hyperlink>,
    ) {
        out.style(saved.pending_style);
        out.link(link);
        for (_, intro) in line_drawing(&saved.charsets) {
            let _ = write!(out.s, "\x1b{intro}0");
        }
        out.s.push_str("\x1b7");
        for (_, intro) in line_drawing(&saved.charsets) {
            let _ = write!(out.s, "\x1b{intro}B");
        }
        out.plain();
    }

    /// The colors programs set: OSC 4 for palette entries, OSC 10/11/12
    /// for the foreground, background and cursor colors.
    fn write_palette(&self, out: &mut Out) {
        use crate::config::colors::{ColorRgb, NamedColor};
        let dynamic = [
            (NamedColor::Foreground, 10),
            (NamedColor::Background, 11),
            (NamedColor::Cursor, 12),
        ];
        let entries = (0..256)
            .map(|i| (i, None))
            .chain(dynamic.map(|(c, n)| (c as usize, Some(n))));
        for (index, osc) in entries {
            let Some(arr) = self.colors[index] else {
                continue;
            };
            let c = ColorRgb::from_color_arr(arr);
            let _ = match osc {
                None => write!(out.s, "\x1b]4;{index};"),
                Some(n) => write!(out.s, "\x1b]{n};"),
            };
            let _ = write!(out.s, "rgb:{:02x}/{:02x}/{:02x}\x1b\\", c.r, c.g, c.b);
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

        // A pending wrap is restored by writing the cell under the cursor
        // again, which clears the row's soft wrap (writing a cell does);
        // writing the next row's first cell over itself wraps it again.
        // Done before the scroll region is set, so it scrolls nothing.
        let region = &self.scroll_region;
        let rewrap = |out: &mut Out, row: i32| {
            let wraps = row >= 0
                && row + 1 < rows as i32
                && grid[Line(row)][Column(cols - 1)].wrapline();
            if wraps {
                let at = Pos::new(Line(row + 1), Column(0));
                out.cell(grid, grid[at], at, true);
                out.plain();
            }
        };

        // The saved cursor (DECSC): its position (and pending wrap), pen
        // (open link included) and charsets. Saved before the scroll region
        // and origin mode are set, so its position is the screen's. Where
        // it is the cursor itself, with a pending wrap on a row that wraps
        // (what restoring it leaves), it's saved with the cursor below:
        // the re-wrap moves the cursor, and restoring the saved one is
        // what brings it back.
        let saved = &grid.saved_cursor;
        let saved_link = cell_link(grid, &saved.template);
        let cursor = &grid.cursor;
        // In origin mode a CUP reaches only the region's rows; a cursor
        // outside them (DECRC after the region changed can restore one
        // there) is placed as near as it goes, and no cell is rewritten
        // for it.
        let origin = self.mode.contains(Mode::ORIGIN);
        let reachable = !origin || region.contains(&cursor.pos.row);
        let pending = cursor.should_wrap && reachable;
        let together = pending
            && saved.should_wrap
            && saved.pos == cursor.pos
            && grid.saved_origin == origin;
        if !together
            && (saved.pos != Pos::default()
                || saved.pending_style != Style::default()
                || saved_link.is_some()
                || line_drawing(&saved.charsets).next().is_some()
                || saved.should_wrap
                || grid.saved_origin)
        {
            // DECSC saves the whole cursor, a pending wrap and origin mode
            // too. No region is set yet, so with origin mode on the
            // cursor's row is still the screen's.
            if grid.saved_origin {
                out.s.push_str("\x1b[?6h");
            }
            self.place_cursor(
                out,
                grid,
                (saved.pos.row.0, saved.pos.col.0),
                0,
                saved.should_wrap,
            );
            self.save_cursor(out, saved, saved_link.clone());
            if saved.should_wrap {
                rewrap(out, saved.pos.row.0.clamp(0, rows as i32 - 1));
            }
            if grid.saved_origin {
                out.s.push_str("\x1b[?6l");
            }
        }

        // The cursor and the saved one together: placed and saved, the
        // row wrapped again (which moves the cursor), and restored once
        // the region is set. Restoring brings back origin mode too.
        if together {
            if origin {
                out.s.push_str("\x1b[?6h");
            }
            let at = (cursor.pos.row.0, cursor.pos.col.0);
            self.place_cursor(out, grid, at, 0, true);
            self.save_cursor(out, saved, saved_link.clone());
            rewrap(out, cursor.pos.row.0);
        }

        // The scroll region (setting it homes the cursor), then origin
        // mode, then the cursor, which origin mode places in the region.
        let full = region.start == Line(0) && region.end == Line(rows as i32);
        if !full {
            // DECSTBM wants its top above its bottom before the bottom is
            // clamped to the screen, so a region of one row (or none) at
            // the bottom is set with a bottom past the screen, as it was.
            let (top, mut bottom) = (region.start.0 + 1, region.end.0);
            if bottom == rows as i32 && top >= bottom {
                bottom = top + 1;
            }
            let _ = write!(out.s, "\x1b[{top};{bottom}r");
        }
        if origin && !together {
            out.s.push_str("\x1b[?6h");
        }
        // A pending wrap on a row that wraps: rewriting the cell under the
        // cursor would clear the row's wrap. Where a row next to it doesn't
        // wrap, the cursor waits to wrap there instead and moves over with
        // LF or RI, which keep a pending wrap and touch no cell.
        let offset = if origin { region.start.0 } else { 0 };
        let row = cursor.pos.row.0;
        let wraps = |r: i32| grid[Line(r)][Column(cols - 1)].wrapline();
        let reaches = |r: i32| !origin || region.contains(&Line(r));
        let via = if pending && !together && wraps(row) {
            let above = row - 1;
            let below = row + 1;
            if above >= 0 && !wraps(above) && reaches(above) && row != region.end.0 {
                Some((above, "\n"))
            } else if below < rows as i32
                && !wraps(below)
                && reaches(below)
                && row + 1 != region.start.0
            {
                Some((below, "\x1bM"))
            } else {
                None
            }
        } else {
            None
        };
        match via {
            _ if together => {}
            Some((next_to, mv)) => {
                let at = (next_to, cursor.pos.col.0);
                self.place_cursor(out, grid, at, offset, true);
                out.s.push_str(mv);
            }
            None => {
                self.place_cursor(out, grid, (row, cursor.pos.col.0), offset, pending)
            }
        }
        if together {
            out.s.push_str("\x1b8");
            // Restoring brought back the saved pen and charsets: the
            // cursor's own are set below, from ASCII.
            out.style = saved.pending_style;
            out.style_id = None;
            out.link = saved_link;
            out.link_id = None;
            for (_, intro) in line_drawing(&saved.charsets) {
                let _ = write!(out.s, "\x1b{intro}B");
            }
        }

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
        for (index, intro) in CHARSETS {
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
        } else if self.blinking_cursor != self.default_blinking_cursor {
            // The default shape, made to blink (or not) by mode 12.
            out.s.push_str(if self.blinking_cursor {
                "\x1b[?12h"
            } else {
                "\x1b[?12l"
            });
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
            // The path starts at the (empty) host's slash. Windows keeps a
            // path without it (`C:/...`, from `file:///C:/...`), so there
            // it's put back: `file://C:/...` would read `C:` as the host.
            // Elsewhere a path without one (OSC 9;9 can set any) has no
            // `file://` form that reads back as it; it's left out.
            let slash = dir.starts_with('/');
            if slash || cfg!(windows) {
                out.s.push_str(if slash {
                    "\x1b]7;file://"
                } else {
                    "\x1b]7;file:///"
                });
                percent_encode_path(&mut out.s, dir);
                out.s.push_str("\x1b\\");
            }
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
/// The stack is a ring (a push past its depth, or a pop past its bottom,
/// wraps around): one with entries above its top is written as pushes
/// all the way round, ending on the top.
fn write_keyboard_stack(out: &mut String, stack: &[u8], top: usize) {
    let depth = stack.len();
    if stack[top + 1..].iter().all(|&flags| flags == 0) {
        if stack[0] != 0 {
            let _ = write!(out, "\x1b[={};1u", stack[0]);
        }
        for &flags in &stack[1..=top] {
            let _ = write!(out, "\x1b[>{flags}u");
        }
        return;
    }
    // From a fresh stack (top 0), the n-th push writes entry n % depth:
    // the last `depth` of them write every entry and end on `top`.
    for n in 1..=top + depth {
        let _ = write!(out, "\x1b[>{}u", stack[n % depth]);
    }
}

/// A path for a `file://` URL: bytes outside the unreserved set (and `/`,
/// `:`) as `%XX`.
fn percent_encode_path(out: &mut String, path: &str) {
    for b in path.bytes() {
        if b.is_ascii_alphanumeric()
            || matches!(b, b'/' | b':' | b'-' | b'.' | b'_' | b'~')
        {
            out.push(b as char);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
}

/// Write the leading cells of `cells` that are ASCII in style `style`
/// (see [`Square::ascii_in_style`]) as their bytes; how many there were.
/// Into the buffer's spare room, with one capacity check for the run.
#[inline]
fn ascii_run(
    out: &mut String,
    cells: &[Square],
    style: StyleId,
    link: Option<u16>,
) -> usize {
    // SAFETY: only ASCII bytes are written, so `out` stays UTF-8; `set_len`
    // covers exactly the bytes written into the reserved spare room.
    let v = unsafe { out.as_mut_vec() };
    v.reserve(cells.len());
    let spare = v.spare_capacity_mut();
    let mut n = 0;
    for square in cells {
        match square.ascii_in(style, link) {
            Some(b) => {
                spare[n].write(b);
                n += 1;
            }
            None => break,
        }
    }
    unsafe { v.set_len(v.len() + n) };
    n
}

/// Plain text's [`ascii_run`]: the leading cells of `cells` that are ASCII
/// in any style, a blank counted in `blank_cells` and written only once a
/// character follows it (so trailing blanks are dropped); how many cells.
#[inline]
pub(crate) fn ascii_text_run(
    out: &mut String,
    cells: &[Square],
    blank_cells: &mut usize,
    had_content: &mut bool,
) -> usize {
    // SAFETY: only ASCII bytes are written, so `out` stays UTF-8; `set_len`
    // covers exactly the bytes written into the reserved spare room.
    let v = unsafe { out.as_mut_vec() };
    v.reserve(*blank_cells + cells.len());
    let spare = v.spare_capacity_mut();
    let mut n = 0;
    let mut blanks = *blank_cells;
    let mut taken = 0;
    for square in cells {
        let Some(b) = square.ascii() else { break };
        taken += 1;
        if b == b' ' {
            blanks += 1;
            continue;
        }
        for slot in &mut spare[n..n + blanks] {
            slot.write(b' ');
        }
        n += blanks;
        blanks = 0;
        spare[n].write(b);
        n += 1;
        *had_content = true;
    }
    unsafe { v.set_len(v.len() + n) };
    *blank_cells = blanks;
    taken
}

/// Rightmost column of `grid`'s row `line` with a non-blank glyph or a
/// style that renders on a blank cell (`Style::renders_on_blank`), or
/// `None` for a row with neither. Fg-only attributes must NOT count as
/// content: `ED`/`EL` reset cells with the full cursor template style, so
/// after `\x1b[31m` + `clear` every blank carries a red fg, and trimming on
/// any non-default style would then emit the whole screen as styled spaces.
/// With `links`, a linked blank is content too: it can still be clicked.
fn row_last_col(
    grid: &Grid<Square>,
    line: i32,
    cols: usize,
    trim: bool,
    links: bool,
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
        (c != ' ' && c != '\0')
            || (links && square.has_hyperlink())
            || grid.style_of(square).renders_on_blank()
            || has_marks(grid, square)
    })
}

/// Whether combining marks ride on the cell.
fn has_marks(grid: &Grid<Square>, square: &Square) -> bool {
    square
        .extras_id_checked()
        .and_then(|id| grid.extras_table.get(id))
        .is_some_and(|extras| !extras.zerowidth.is_empty())
}

/// The cell's codepoints, its base and its marks.
fn cluster(grid: &Grid<Square>, pos: Pos) -> Vec<u32> {
    grid.cell_text(pos).map(|c| c as u32).collect()
}

/// Whether a narrow cell holds what takes two columns: a wide character,
/// or a cluster made wide (a flag, an emoji with VS16) where it had no
/// room to widen.
fn squeezed(grid: &Grid<Square>, square: &Square, pos: Pos) -> bool {
    if square.is_wide() {
        return false;
    }
    let c = square.c() as u32;
    if c >= 0x80 && crate::codepoint_width::codepoint_width(c) == Some(2) {
        return true;
    }
    has_marks(grid, square)
        && crate::grapheme_lut::cluster_width(&cluster(grid, pos)).1 == 2
}

/// A [`squeezed`] cell whose own character is narrow: what made it wide
/// came after it, so written where it can't widen (the last column with
/// autowrap off) it is the same narrow cell.
fn squeezed_cluster(grid: &Grid<Square>, square: &Square, pos: Pos) -> bool {
    crate::codepoint_width::codepoint_width(square.c() as u32) != Some(2)
        && squeezed(grid, square, pos)
}

/// Whether the client, writing `next` right after `at` (it joins a
/// character to the cell before the cursor), would make one cluster of
/// them.
fn would_join(grid: &Grid<Square>, at: Pos, next: Pos) -> bool {
    let square = &grid[next];
    if (square.c() as u32) < 0x80 {
        return false;
    }
    let mut cps = cluster(grid, at);
    let len = cps.len();
    cps.extend(grid.cell_text(next).map(|c| c as u32));
    crate::grapheme_lut::cluster_width(&cps).0 > len
}

/// Write the SGR sequence for `style` directly into `out`, starting with `0`
/// (reset) then re-applying: styles are fully self-contained. Written inline
/// (no intermediate allocation); call it only when the style actually
/// changes. Colors keep their original named / indexed / rgb form. Also the
/// style emitter for librio's `Surface::serialize`.
pub fn write_sgr(out: &mut String, style: &Style) {
    Sgr::to(out, |sgr| sgr.style(style));
}

/// An SGR sequence written straight into the output's bytes, room for the
/// longest made first: a snapshot can write one for every cell, and then a
/// capacity check per byte that never grows the buffer is all it costs.
struct Sgr<'a> {
    v: &'a mut Vec<u8>,
}

impl Sgr<'_> {
    /// The longest sequence: every attribute, two rgb colors, and an rgb
    /// underline color.
    const MAX: usize = 96;

    #[inline]
    fn to(out: &mut String, f: impl FnOnce(&mut Sgr)) {
        // SAFETY: an `Sgr` writes ASCII only (`str` takes ASCII literals,
        // `num` and `byte` digits and separators), so `out` stays UTF-8.
        let v = unsafe { out.as_mut_vec() };
        v.reserve(Self::MAX);
        let start = v.len();
        f(&mut Sgr { v });
        debug_assert!(out.as_bytes()[start..].is_ascii());
    }

    #[inline]
    fn str(&mut self, s: &'static str) {
        debug_assert!(s.is_ascii());
        self.v.extend_from_slice(s.as_bytes());
    }

    #[inline]
    fn byte(&mut self, b: u8) {
        self.v.push(b);
    }

    /// `n` in decimal, without the formatting machinery.
    #[inline]
    fn num(&mut self, n: u8) {
        if n >= 100 {
            self.byte(b'0' + n / 100);
        }
        if n >= 10 {
            self.byte(b'0' + n / 10 % 10);
        }
        self.byte(b'0' + n % 10);
    }

    fn rgb(&mut self, r: u8, g: u8, b: u8) {
        self.num(r);
        self.byte(b';');
        self.num(g);
        self.byte(b';');
        self.num(b);
    }

    /// `style` in full: a reset, then each attribute and color. Colors keep
    /// their original named / indexed / rgb form.
    fn style(&mut self, style: &Style) {
        use crate::crosswords::style::UnderlineKind;

        self.str("\x1b[0");
        let flags = style.flags;
        if flags.contains(StyleFlags::BOLD) {
            self.str(";1");
        }
        if flags.contains(StyleFlags::DIM) {
            self.str(";2");
        }
        if flags.contains(StyleFlags::ITALIC) {
            self.str(";3");
        }
        match flags.underline_kind() {
            None => {}
            Some(UnderlineKind::Single) => self.str(";4"),
            Some(UnderlineKind::Double) => self.str(";4:2"),
            Some(UnderlineKind::Curly) => self.str(";4:3"),
            Some(UnderlineKind::Dotted) => self.str(";4:4"),
            Some(UnderlineKind::Dashed) => self.str(";4:5"),
        }
        if flags.contains(StyleFlags::SLOW_BLINK) {
            self.str(";5");
        } else if flags.contains(StyleFlags::RAPID_BLINK) {
            self.str(";6");
        }
        if flags.contains(StyleFlags::INVERSE) {
            self.str(";7");
        }
        if flags.contains(StyleFlags::HIDDEN) {
            self.str(";8");
        }
        if flags.contains(StyleFlags::STRIKEOUT) {
            self.str(";9");
        }
        self.color(style.fg, true);
        self.color(style.bg, false);
        self.byte(b'm');

        // Underline color travels as its own sequence; the leading reset
        // already cleared any previous one.
        if let Some(underline) = style.underline_color {
            match underline {
                AnsiColor::Indexed(i) => {
                    self.str("\x1b[58;5;");
                    self.num(i);
                    self.byte(b'm');
                }
                AnsiColor::Spec(rgb) => {
                    self.str("\x1b[58;2;");
                    self.rgb(rgb.r, rgb.g, rgb.b);
                    self.byte(b'm');
                }
                AnsiColor::Named(n) => {
                    // Specials (Foreground = 256 and up) have no palette
                    // index; SGR 58 cannot express them, so leave the
                    // reset's unset underline color rather than emit an
                    // invalid index.
                    if let Ok(idx) = u8::try_from(n as u16) {
                        self.str("\x1b[58;5;");
                        self.num(idx);
                        self.byte(b'm');
                    }
                }
            }
        }
    }

    /// `;` and a color's parameters. Named default fg/bg (256/257) and
    /// specials write nothing: in full form the leading reset covers them.
    fn color(&mut self, color: AnsiColor, fg: bool) {
        match color {
            AnsiColor::Named(named) => {
                let n = named as u16;
                if n <= 7 {
                    self.byte(b';');
                    self.num((if fg { 30 } else { 40 } + n) as u8);
                } else if n <= 15 {
                    self.byte(b';');
                    self.num((if fg { 90 } else { 100 } + (n - 8)) as u8);
                }
            }
            AnsiColor::Indexed(i) => {
                self.str(if fg { ";38;5;" } else { ";48;5;" });
                self.num(i);
            }
            AnsiColor::Spec(rgb) => {
                self.str(if fg { ";38;2;" } else { ";48;2;" });
                self.rgb(rgb.r, rgb.g, rgb.b);
            }
        }
    }

    /// Like `color`, but one the full form leaves to the reset (the
    /// default, and specials with no SGR form) is written as the default
    /// (39 / 49): for a change of colors only, which has no reset.
    fn color_change(&mut self, color: AnsiColor, fg: bool) {
        match color {
            AnsiColor::Named(named) if named as u16 > 15 => {
                self.str(if fg { ";39" } else { ";49" });
            }
            _ => self.color(color, fg),
        }
    }
}

/// The SGR that takes the client from `from` to `to`. When only the
/// colors differ, just those, as the attributes stand; otherwise
/// [`write_sgr`]'s full form.
fn write_sgr_change(out: &mut String, from: &Style, to: &Style) {
    let colors_only =
        to.flags == from.flags && to.underline_color == from.underline_color;
    // Back to no attributes and default colors: `ESC[0m` is the shortest.
    let default = |c: AnsiColor| matches!(c, AnsiColor::Named(n) if n as u16 > 15);
    let plain = to.flags.is_empty()
        && to.underline_color.is_none()
        && default(to.fg)
        && default(to.bg);
    Sgr::to(out, |sgr| {
        if !colors_only || plain {
            sgr.style(to);
            return;
        }
        // ESC, then each color's `;` and parameters; the first `;` becomes
        // the `[`.
        let start = sgr.v.len();
        sgr.byte(0x1b);
        if to.fg != from.fg {
            sgr.color_change(to.fg, true);
        }
        if to.bg != from.bg {
            sgr.color_change(to.bg, false);
        }
        sgr.v[start + 1] = b'[';
        sgr.byte(b'm');
    });
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
        /// A fresh terminal fed this one's snapshot. Streamed a row at a
        /// time too, which must give the same bytes.
        fn copy(&self) -> T {
            let whole = self.term.snapshot_formatted();
            let mut rows = Vec::new();
            self.term
                .stream(FormatOptions::snapshot(), &mut rows, 1)
                .unwrap();
            assert!(whole == rows, "streamed snapshot differs");
            let mut t = T::new(self.term.columns(), self.term.screen_lines());
            t.feed(&whole);
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
    fn the_palette_comes_along_when_asked() {
        let mut a = T::new(20, 4);
        a.feed(
            b"\x1b]4;1;rgb:c8/0a/0a\x1b\\\x1b]4;200;#123456\x07\
              \x1b]10;rgb:ee/dd/cc\x07\x1b]11;#010203\x07\x1b]12;rgb:ff/80/00\x07x",
        );
        // Not in a client's snapshot: the client keeps its theme.
        let plain = String::from_utf8(a.term.snapshot_formatted()).unwrap();
        assert!(
            !plain.contains("\x1b]4;") && !plain.contains("\x1b]1"),
            "{plain:?}"
        );
        let opts = FormatOptions::snapshot().with_palette(true);
        let snapshot = a.term.format(opts);
        for want in [
            "\x1b]4;1;rgb:c8/0a/0a\x1b\\",
            "\x1b]4;200;rgb:12/34/56\x1b\\",
            "\x1b]10;rgb:ee/dd/cc\x1b\\",
            "\x1b]11;rgb:01/02/03\x1b\\",
            "\x1b]12;rgb:ff/80/00\x1b\\",
        ] {
            assert!(snapshot.contains(want), "{want:?} in {snapshot:?}");
        }
        let mut b = T::new(20, 4);
        b.feed(snapshot.as_bytes());
        assert_same(&a, &b);
        for i in 0..crate::config::colors::term::COUNT {
            assert_eq!(a.term.colors[i], b.term.colors[i], "color {i}");
        }
        assert_eq!(b.term.format(opts), snapshot);
    }

    #[test]
    fn ground_is_between_sequences() {
        let mut p = Processor::default();
        let mut t = T::new(10, 3);
        let mut at = |p: &mut Processor, bytes: &[u8]| {
            p.advance(&mut t.term, bytes);
            p.is_ground()
        };
        assert!(Processor::default().is_ground());
        assert!(at(&mut p, b"plain text"));
        assert!(!at(&mut p, b"\x1b"));
        assert!(!at(&mut p, b"[3"));
        assert!(at(&mut p, b"1mred"));
        assert!(!at(&mut p, b"\x1b]2;a ti"));
        assert!(at(&mut p, b"tle\x07"));
        assert!(!at(&mut p, "\u{754c}".as_bytes().split_at(1).0));
        assert!(at(&mut p, "\u{754c}".as_bytes().split_at(1).1));
        assert!(!at(&mut p, b"\x1bP+q"));
        assert!(at(&mut p, b"544e\x1b\\"));
        assert!(!at(&mut p, b"\x1b_Ga=q;"));
        assert!(at(&mut p, b"\x1b\\"));
    }

    #[test]
    fn rep_repeats_the_character_from_before_a_checkpoint() {
        let mut a = T::new(20, 3);
        a.feed(b"abc");
        assert_eq!(a.parser.preceding_char(), Some('c'));
        let mut b = a.copy();
        b.parser.set_preceding_char(a.parser.preceding_char());
        same(&mut a, &mut b, b"\x1b[3b");
        assert!(b.text().starts_with("abcccc"), "{:?}", b.text());
    }

    /// A terminal checkpointed wherever its input is between sequences,
    /// then rebuilt from the checkpoint and fed the rest of the input, is
    /// the terminal fed all of it.
    #[test]
    fn checkpoints_at_ground_resume_exactly() {
        let input: &[u8] = b"hist 1\r\nhist 2\r\n\x1b[1;32mgreen\x1b[0m \
            \x1b]8;;http://a\x1b\\link\x1b]8;;\x1b\\\r\n\x1b]4;3;#aabbcc\x07\
            \x1b[2;4r\x1b[?6h\x1b[3;2Hin\x1b7\x1b[?1049h\x1b[>1ualt \xe7\x95\x8c\
            \x1b[44m\x1b[2Ksome\x1b[3bx\x1b[?1049l\x1b8tail\r\nmore\r\n";
        let opts = FormatOptions::snapshot().with_palette(true);
        let mut whole = T::new(12, 5);
        whole.feed(input);
        let mut points = 0;
        for cut in 0..=input.len() {
            let mut a = T::new(12, 5);
            a.feed(&input[..cut]);
            if !a.parser.is_ground() {
                continue;
            }
            points += 1;
            let mut b = T::new(12, 5);
            b.feed(a.term.format(opts).as_bytes());
            b.parser.set_preceding_char(a.parser.preceding_char());
            b.feed(&input[cut..]);
            assert_same(&whole, &b);
            assert_eq!(whole.term.format(opts), b.term.format(opts), "cut {cut}");
        }
        // (Most of these bytes are inside escape sequences.)
        assert!(points >= 60, "{points}");
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
    fn pending_wrap_after_a_line_feed_keeps_the_row_wrapped() {
        // LF (and IND) keep a pending wrap: here on a row that wraps,
        // whose next-door row above doesn't.
        for lf in [&b"\n"[..], b"\x1bD"] {
            let mut a = T::new(6, 5);
            a.feed(b"ab\r\n1234567890\x1b[H123456").feed(lf);
            assert!(a.term.grid.cursor.should_wrap);
            let mut b = a.copy();
            assert_same(&a, &b);
            same(&mut a, &mut b, b"\x1b[3;1HZ");
        }
        // The row above wraps too: the one below doesn't, and RI comes up.
        let mut a = T::new(6, 5);
        a.feed(b"abcdefghijklmnop\x1b[3;1Hxxxxxx\x1bM");
        assert!(a.term.grid.cursor.should_wrap);
        assert_eq!(a.term.grid.cursor.pos.row, Line(1));
        let mut b = a.copy();
        assert_same(&a, &b);
        same(&mut a, &mut b, b"\x1b[5;1HZ");
    }

    #[test]
    fn the_alt_screens_state_waits_under_the_primary() {
        // Its kitty flags and saved cursor (with its charsets), for the
        // next program that enters it.
        let mut a = T::new(10, 4);
        a.feed(b"\x1b[?1049h\x1b[>1u\x1b[2;3H\x1b(0\x1b7\x1b[?1049lprimary");
        let mut b = a.copy();
        assert_same(&a, &b);
        assert_eq!(b.term.keyboard_mode(), a.term.keyboard_mode());
        same(&mut a, &mut b, b"\x1b[?1049h");
        assert_eq!(b.term.keyboard_mode(), a.term.keyboard_mode());
        same(&mut a, &mut b, b"\x1b8q");
        same(&mut a, &mut b, b"\x1b[?1049l");
    }

    #[test]
    fn a_keyboard_stack_that_wrapped_around() {
        // The stack is a ring: a pop past the bottom goes to its top, and
        // more pushes than it holds go round.
        let inputs: [&[u8]; 2] = [
            b"\x1b[<u\x1b[>1u\x1b[<u\x1b[=5;1u\x1b[>1u",
            b"\x1b[>1u\x1b[>2u\x1b[>3u\x1b[>4u\x1b[>5u\x1b[>6u\x1b[>7u\x1b[>8u\x1b[>9u",
        ];
        for input in inputs {
            let mut a = T::new(10, 4);
            a.feed(input);
            let mut b = a.copy();
            assert_same(&a, &b);
            for _ in 0..9 {
                assert_eq!(b.term.keyboard_mode(), a.term.keyboard_mode());
                same(&mut a, &mut b, b"\x1b[<u");
            }
        }
    }

    #[test]
    fn sixel_synchronized_output_and_blink_modes() {
        let mut a = T::new(10, 4);
        a.feed(b"\x1b[?80h\x1b[?1070h\x1b[?8452h\x1b[?2026h\x1b[?12h");
        let b = a.copy();
        assert_same(&a, &b);
        assert!(b.term.blinking_cursor);
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

    /// A sink that records each write, and fails once it has taken
    /// `fail_after` bytes.
    struct Sink {
        writes: Vec<usize>,
        bytes: Vec<u8>,
        fail_after: usize,
    }

    impl std::io::Write for Sink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if self.bytes.len() >= self.fail_after {
                return Err(std::io::Error::other("full"));
            }
            self.writes.push(buf.len());
            self.bytes.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// 1,000 lines of styled history: about 130 KB of snapshot, a few
    /// chunks.
    fn big() -> T {
        let mut a = T::new(80, 24);
        for i in 0..1_000 {
            let mut line = format!("{i:04}");
            for k in 0..7 {
                line.push_str(&format!("\x1b[3{}m {}", (i + k) % 8, "x".repeat(9)));
            }
            line.push_str("\x1b[0m\r\n");
            a.feed(line.as_bytes());
        }
        a
    }

    #[test]
    fn format_to_writes_what_format_returns_in_chunks() {
        let mut a = big();
        for alt in [false, true] {
            if alt {
                a.feed(b"\x1b[?1049h\x1b[Halt screen\x1b[44m");
            }
            for opts in [
                FormatOptions::snapshot(),
                FormatOptions::vt(),
                FormatOptions::plain().with_scrollback(true),
            ] {
                let whole = a.term.format(opts);
                let mut sink = Sink {
                    writes: Vec::new(),
                    bytes: Vec::new(),
                    fail_after: usize::MAX,
                };
                a.term.format_to(opts, &mut sink).unwrap();
                assert!(sink.bytes == whole.as_bytes(), "alt {alt}: {opts:?}");
                if opts.scrollback && opts.emit == Emit::Vt {
                    // Several writes, none much past a chunk (a row over).
                    assert!(sink.writes.len() > 1, "{:?}", sink.writes);
                    assert!(sink.writes.iter().all(|&n| n < CHUNK + 4096));
                }
            }
        }
        let mut v = Vec::new();
        a.term.snapshot_to(&mut v).unwrap();
        assert_eq!(v, a.term.snapshot_formatted());
    }

    #[test]
    fn format_to_stops_at_the_first_error() {
        let a = big();
        let mut sink = Sink {
            writes: Vec::new(),
            bytes: Vec::new(),
            fail_after: 1,
        };
        let err = a.term.snapshot_to(&mut sink).unwrap_err();
        assert_eq!(err.to_string(), "full");
        // One chunk went out; the walk stopped at the next.
        assert_eq!(sink.writes.len(), 1);
    }

    #[test]
    fn format_to_an_empty_terminal() {
        let a = T::new(10, 3);
        let mut v = Vec::new();
        a.term.snapshot_to(&mut v).unwrap();
        assert_eq!(v, a.term.snapshot_formatted());
    }

    #[test]
    fn saved_origin_mode_survives() {
        // Saved in origin mode, which is off now: restoring turns it on,
        // and the next CUP lands in the region.
        let mut a = T::new(20, 6);
        a.feed(b"\x1b[2;5r\x1b[?6h\x1b[2;3H\x1b7\x1b[?6l\x1b[6;1Hend");
        let mut b = a.copy();
        assert_same(&a, &b);
        assert!(b.term.grid.saved_origin);
        same(&mut a, &mut b, b"\x1b8S\x1b[1;1HT");
        assert!(b.term.mode().contains(Mode::ORIGIN));

        // Saved outside it, with origin mode on now: restoring turns it off.
        let mut a = T::new(20, 6);
        a.feed(b"\x1b[4;4H\x1b7\x1b[2;5r\x1b[?6h\x1b[2;2Hin");
        let mut b = a.copy();
        assert_same(&a, &b);
        assert!(!b.term.grid.saved_origin);
        same(&mut a, &mut b, b"\x1b8S\x1b[1;1HT");
        assert!(!b.term.mode().contains(Mode::ORIGIN));
    }

    #[test]
    fn primary_origin_mode_survives_under_the_alt_screen() {
        // The shell set origin mode, a program entered the alt screen and
        // reset it: leaving gives the shell its origin mode back.
        let mut a = T::new(20, 6);
        a.feed(b"\x1b[2;5r\x1b[?6h$ \x1b[?1049h\x1b[?6l\x1b[r\x1b[3;3Halt");
        let mut b = a.copy();
        assert_same(&a, &b);
        same(&mut a, &mut b, b"\x1b[?1049l\x1b[1;1HT");
        assert!(b.term.mode().contains(Mode::ORIGIN));
    }

    /// A snapshot's OSC 7, for a working directory of `dir`.
    fn osc7(dir: &str) -> Option<String> {
        let mut a = T::new(20, 4);
        a.term.current_directory = Some(dir.into());
        let snapshot = String::from_utf8(a.term.snapshot_formatted()).unwrap();
        let at = snapshot.find("\x1b]7;")?;
        Some(snapshot[at..].to_string())
    }

    #[test]
    fn directories_as_file_urls() {
        assert_eq!(
            osc7("/tmp/a b:c").as_deref(),
            Some("\x1b]7;file:///tmp/a%20b:c\x1b\\")
        );
        // Windows keeps `file:///C:/...` as `C:/...`: the URL gets the
        // empty host's slash back, or `C:` would read as the host. Other
        // platforms keep the slash, so a path without one isn't a URL's.
        let drive = osc7("C:/Users/me too");
        if cfg!(windows) {
            assert_eq!(
                drive.as_deref(),
                Some("\x1b]7;file:///C:/Users/me%20too\x1b\\")
            );
        } else {
            assert_eq!(drive, None);
        }
    }

    #[test]
    fn directories_survive() {
        let mut inputs = vec![&b"\x1b]7;file:///C:/Users/me%20too\x07"[..]];
        // OSC 9;9 with a Windows path: no `file://` form for it elsewhere.
        if cfg!(windows) {
            inputs.push(b"\x1b]9;9;C:\\Users\\me\x07");
        }
        for input in inputs {
            let mut a = T::new(20, 4);
            a.feed(input);
            assert!(a.term.current_directory.is_some(), "{input:?}");
            let b = a.copy();
            assert_same(&a, &b);
            assert_eq!(b.term.current_directory, a.term.current_directory);
        }
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
        // (Windows keeps a path without the URL's leading slash.)
        let dir = if cfg!(windows) { "tmp/a b" } else { "/tmp/a b" };
        assert_eq!(
            b.term.current_directory.as_deref(),
            Some(std::path::Path::new(dir))
        );
        assert!(!b.term.mode().contains(Mode::SHOW_CURSOR));
    }

    /// Every cell of the history and both screens: text, style, link,
    /// soft wrap.
    #[allow(clippy::type_complexity)]
    fn cells(t: &T) -> Vec<(String, String, Style, Option<String>, bool)> {
        let mut out = Vec::new();
        let mut grids = vec![&t.term.grid];
        if t.term.mode().contains(Mode::ALT_SCREEN) {
            grids.push(&t.term.inactive_grid);
        }
        for grid in grids {
            let top = -(grid.history_size() as i32);
            for line in top..grid.screen_lines() as i32 {
                for col in 0..grid.columns() {
                    let pos = Pos::new(Line(line), Column(col));
                    let square = grid[pos];
                    // As it shows: a cell never written is a blank, and a
                    // blank's attributes that draw nothing don't matter.
                    let mut text: String = grid
                        .cell_text(pos)
                        .map(|c| if c == '\0' || c == '\t' { ' ' } else { c })
                        .collect();
                    let mut style = grid.style_of(&square);
                    if text == " " && !style.renders_on_blank() {
                        style = Style::default();
                    }
                    // A wide character's second half shows as a blank, kept
                    // or not; so does a broken half.
                    // And a double-width character in a narrow cell with
                    // no orphaned half after it to take.
                    let row = &grid[Line(line)][..];
                    let squeezed = !square.is_wide()
                        && crate::codepoint_width::codepoint_width(square.c() as u32)
                            == Some(2)
                        && !(col + 1 < row.len() && row[col + 1].is_spacer());
                    // The blank a wide character leaves where it didn't
                    // fit takes the character's style in the copy.
                    // (Where one does start the next row; an orphaned one
                    // is a blank in its own style.)
                    let rows = grid.screen_lines() as i32;
                    let kept_lead = square.is_leading_spacer()
                        && line + 1 < rows
                        && grid[Line(line + 1)][Column(0)].is_wide();
                    if kept_lead {
                        style = Style::default();
                    }
                    if square.is_spacer() || broken_half(row, col) || squeezed {
                        text = " ".into();
                        if !style.renders_on_blank() {
                            style = Style::default();
                        }
                    }
                    let link = cell_link(grid, &square)
                        .map(|l| l.uri().to_owned())
                        .filter(|_| !kept_lead);
                    // (The bottom row's wrap into nothing, which a scroll
                    // down can leave, is not carried.)
                    let wrap = col + 1 == grid.columns()
                        && line + 1 < grid.screen_lines() as i32
                        && square.wrapline();
                    out.push((format!("{line},{col}"), text, style, link, wrap));
                }
            }
        }
        out
    }

    fn first_difference(a: &T, b: &T, seed: usize, when: &str, input: &str) {
        let (ca, cb) = (cells(a), cells(b));
        let diff = ca.iter().zip(&cb).find(|(x, y)| x != y);
        if diff.is_some() || ca.len() != cb.len() {
            panic!(
                "seed {seed}{when} ({}x{}): {diff:?}\ninput: {input:?}\nsnapshot: {:?}\nhistory {} vs {}\na: {:?}\nb: {:?}",
                a.term.columns(),
                a.term.screen_lines(),
                String::from_utf8_lossy(&a.term.snapshot_formatted()),
                a.term.grid.history_size(),
                b.term.grid.history_size(),
                a.text(),
                b.text()
            );
        }
    }

    #[test]
    fn random_terminals_round_trip() {
        let tokens: &[&str] = &[
            "hello",
            " ",
            "  ",
            "\t",
            "\r\n",
            "\n",
            "\r",
            "界",
            "e\u{301}",
            "x",
            "1234567890123",
            "\x1b[31m",
            "\x1b[1;4m",
            "\x1b[38;5;200m",
            "\x1b[48;2;1;2;3m",
            "\x1b[44m",
            "\x1b[39m",
            "\x1b[22;24m",
            "\x1b[7m",
            "\x1b[4:3;58;5;9m",
            "\x1b[0m",
            "\x1b[K",
            "\x1b[1K",
            "\x1b[J",
            "\x1b[2J",
            "\x1b[H",
            "\x1b[5;3H",
            "\x1b[3G",
            "\x1b[2A",
            "\x1b[1P",
            "\x1b[2@",
            "\x1b[S",
            "\x1b[T",
            "\x1b]8;;http://a\x1b\\",
            "\x1b]8;id=q;http://b\x1b\\",
            "\x1b]8;;\x1b\\",
            "\x1b[3g\x1b[5G\x1bH",
            "\x1b7",
            "\x1b8",
            "\x1b[2;4r",
            "\x1b[r",
            "\x1b[?6h",
            "\x1b[?6l",
            "\x1b[?7l",
            "\x1b[?7h",
            "\x1b[4h",
            "\x1b[4l",
            "\x1b[?1049h",
            "\x1b[?1049l",
            "\x1b(0",
            "\x1b(B",
            "lqk",
            "\x1b[?2004h",
            "\x1b[>1u",
            "\x1b[<u",
            "\x1b[?25l",
            "\x1b[6 q",
        ];
        // Same input after the copy too: a short tail of plain tokens.
        let tail: &[&str] = &["Z", "\r\n", "ab", "\x1b[?1049l", "\x1b8", "q", "\t"];
        let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut rnd = |n: usize| {
            x = x
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (x >> 33) as usize % n
        };
        for seed in 0..2_000 {
            let cols = 6 + rnd(20);
            let rows = 2 + rnd(8);
            let mut a = T::new(cols, rows);
            let mut input = String::new();
            for _ in 0..20 + rnd(300) {
                let token = tokens[rnd(tokens.len())];
                input.push_str(token);
                a.feed(token.as_bytes());
            }
            // A cursor with a pending wrap on a row that wraps on, which
            // only restoring a saved cursor makes, comes along only while
            // the saved cursor is that one too (see `write_state`).
            let g = &a.term.grid;
            let c = &g.cursor;
            let row = c.pos.row.0;
            let on_wrapped = c.should_wrap
                && row + 1 < g.screen_lines() as i32
                && g[Line(row)][Column(g.columns() - 1)].wrapline();
            let saved = &g.saved_cursor;
            if on_wrapped && !(saved.should_wrap && saved.pos == c.pos) {
                continue;
            }
            // A cursor waiting to wrap on the blank a wide character left
            // turns it into a plain blank when its wrap is restored.
            let on_lead =
                |g: &Grid<Square>, cur: &crate::crosswords::pos::Cursor<Square>| {
                    cur.should_wrap
                        && g[Line(cur.pos.row.0.clamp(0, g.screen_lines() as i32 - 1))]
                            [Column(g.columns() - 1)]
                        .is_leading_spacer()
                };
            let ig = &a.term.inactive_grid;
            if on_lead(g, &g.cursor)
                || on_lead(g, &g.saved_cursor)
                || on_lead(ig, &ig.cursor)
            {
                continue;
            }
            let mut b = a.copy();
            first_difference(&a, &b, seed, "", &input);
            // (Not the plain text: a tab copies as `\t` from the original
            // and as blanks from the copy, which can't hold one.)
            assert_eq!(a.term.grid.history_size(), b.term.grid.history_size());
            // DECRC of a cursor saved in origin mode, after the region
            // changed, leaves it outside the region with origin mode on
            // (as in xterm); no CUP reaches that, so a copy can't either.
            let region = &a.term.scroll_region;
            let row = a.term.grid.cursor.pos.row;
            let outside = a.term.mode().contains(Mode::ORIGIN)
                && (row < region.start || row >= region.end);
            if outside {
                continue;
            }
            assert_eq!(a.cursor(), b.cursor(), "seed {seed}: cursor");
            assert_eq!(
                a.term.mode().bits(),
                b.term.mode().bits(),
                "seed {seed}: modes"
            );
            assert_eq!(
                a.term.keyboard_mode(),
                b.term.keyboard_mode(),
                "seed {seed}: keyboard"
            );
            // A bottom row's wrap into nothing isn't carried (see the
            // module docs); scrolled up by the tail, it would show.
            let bottom_wraps = |g: &Grid<Square>| {
                g[Line(g.screen_lines() as i32 - 1)][Column(g.columns() - 1)].wrapline()
            };
            // Nor are split wide characters (blanks in the copy); input
            // that moves cells (insert mode) would carry the difference to
            // where it no longer reads as one.
            let split = |g: &Grid<Square>| {
                (-(g.history_size() as i32)..g.screen_lines() as i32).any(|line| {
                    let row = &g[Line(line)][..];
                    (0..row.len()).any(|col| {
                        let c = row[col];
                        // (A wide character's leftover blank at a row's end
                        // too: the copy's takes the character's pen.)
                        c.is_leading_spacer()
                            || broken_half(row, col)
                            || (!c.is_wide()
                                && crate::codepoint_width::codepoint_width(c.c() as u32)
                                    == Some(2))
                    })
                })
            };
            if bottom_wraps(&a.term.grid)
                || bottom_wraps(&a.term.inactive_grid)
                || split(&a.term.grid)
                || split(&a.term.inactive_grid)
            {
                continue;
            }
            // Insert mode (compared above) off for the tail: shifting cells
            // moves soft-wrap flags rio leaves on cells that are no longer
            // last in their row, invisible state a copy doesn't get.
            a.feed(b"\x1b[4l");
            b.feed(b"\x1b[4l");
            input.push_str("\u{0}TAIL\u{0}");
            for _ in 0..8 {
                let token = tail[rnd(tail.len())];
                input.push_str(token);
                a.feed(token.as_bytes());
                b.feed(token.as_bytes());
            }
            first_difference(
                &a,
                &b,
                seed,
                " after the tail (after NUL TAIL NUL)",
                &input,
            );
            assert_eq!(a.cursor(), b.cursor(), "seed {seed}: cursor after the tail");
        }
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

    #[test]
    fn a_wide_character_that_wrapped_off_the_bottom() {
        // The bottom row ends in the blank a wide character left, the
        // character scrolled away: nothing of the row above comes in.
        let mut a = T::new(3, 2);
        a.feed("\r\nab界".as_bytes()).feed(b"\x1b[1T\x1b[1;1Hc");
        assert_eq!(a.term.format(FormatOptions::plain()), "c\nab");
        // The leading row of the history wraps into the next (the
        // character is there).
        let mut a = T::new(3, 1);
        a.feed("ab界\r\nx".as_bytes());
        assert_eq!(a.text(), "ab界\nx");
    }

    // Plain text.

    #[test]
    fn plain_joins_soft_wrapped_rows_and_keeps_hard_breaks() {
        let mut a = T::new(5, 6);
        a.feed(b"helloworld!\r\nnext\r\nabcdefghij");
        assert_eq!(
            a.term.format(FormatOptions::plain()),
            "helloworld!\nnext\nabcdefghij"
        );
    }

    #[test]
    fn plain_drops_trailing_blanks_but_keeps_blank_rows_between() {
        let mut a = T::new(10, 6);
        a.feed(b"a  \x1b[44m  \x1b[0m\r\n\r\n\r\n  b   \r\n\r\n");
        assert_eq!(a.term.format(FormatOptions::plain()), "a\n\n\n  b");
        // Nothing at all is nothing.
        assert_eq!(T::new(10, 6).term.format(FormatOptions::plain()), "");
    }

    #[test]
    fn plain_wide_characters_once_each() {
        let mut a = T::new(6, 3);
        // The third one doesn't fit on the first row and wraps.
        a.feed("界界 界x".as_bytes());
        assert_eq!(a.term.format(FormatOptions::plain()), "界界 界x");
        let mut a = T::new(4, 3);
        a.feed("界\r\n x界".as_bytes());
        assert_eq!(a.term.format(FormatOptions::plain()), "界\n x界");
    }

    #[test]
    fn plain_scrollback_joins_a_line_wrapped_into_the_screen() {
        let mut a = T::new(4, 2);
        a.feed(b"0123456789\r\nx");
        assert!(a.term.grid.history_size() > 0);
        assert_eq!(a.term.format(FormatOptions::plain()), "89\nx");
        assert_eq!(a.text(), "0123456789\nx");
    }

    // VT output.

    #[test]
    fn vt_writes_a_style_once_for_a_run_and_resets_it_before_plain_text() {
        let mut a = T::new(20, 3);
        a.feed(b"\x1b[1;31mbold\x1b[0m plain \x1b[1;31mbold");
        let s = a.term.format(FormatOptions::vt());
        assert_eq!(s.matches("\x1b[0;1;31m").count(), 2, "{s:?}");
        assert!(
            s.contains("\x1b[0;1;31mbold\x1b[0m plain \x1b[0;1;31mbold"),
            "{s:?}"
        );
    }

    #[test]
    fn vt_without_trim_keeps_blank_rows_at_the_bottom() {
        let mut a = T::new(4, 4);
        a.feed(b"ab\x1b[44m\x1b[4;1H\x1b[0m");
        let trimmed = a.term.format(FormatOptions::vt());
        let full = a.term.format(FormatOptions::vt().with_trim(false));
        assert_eq!(trimmed.matches("\r\n").count(), 0, "{trimmed:?}");
        assert_eq!(full.matches("\r\n").count(), 3, "{full:?}");
        // Either way, the cursor ends on the bottom row.
        for s in [trimmed, full] {
            let mut b = T::new(4, 4);
            b.feed(s.as_bytes());
            let screen = |t: &T| t.term.format(FormatOptions::plain());
            assert_eq!(screen(&b), screen(&a));
            assert_eq!(b.cursor(), a.cursor());
        }
    }

    #[test]
    fn background_only_cells_and_gaps_survive() {
        let mut a = T::new(12, 4);
        // A gap the cursor stepped over, blanks with only a background,
        // a background erased to the row's end, and an erased gap inside.
        a.feed(b"ab\x1b[6Gcd\r\n\x1b[41m  \x1b[0mx\r\n\x1b[42m\x1b[K\x1b[0m\r\nxyz\x1b[45m\x1b[1;2H\x1b[2X");
        let mut b = a.copy();
        assert_same(&a, &b);
        assert_eq!(cells(&a), cells(&b));
        same(&mut a, &mut b, b"\x1b[0m\x1b[3;3HZ");
        assert_eq!(cells(&a), cells(&b));
    }

    #[test]
    fn styles_survive_cell_by_cell() {
        let mut a = T::new(30, 4);
        a.feed(
            b"\x1b[1mB\x1b[2mD\x1b[22;3mI\x1b[23;4:3mU\x1b[58;2;1;2;3mC\x1b[24;59;5mK\
              \x1b[25;7mR\x1b[27;8mH\x1b[28;9mS\x1b[29;38;5;123mF\x1b[48;2;9;8;7mG\
              \x1b[39;49;91;102mN\x1b[0m.\r\n\x1b[4:2mdouble\x1b[4:4mdot\x1b[4:5mdash",
        );
        let b = a.copy();
        assert_same(&a, &b);
        assert_eq!(cells(&a), cells(&b));
    }

    #[test]
    fn a_combining_mark_on_a_trailing_blank() {
        let mut a = T::new(10, 3);
        a.feed("a \u{301}\r\nb".as_bytes());
        let b = a.copy();
        assert_eq!(cells(&a), cells(&b));
    }

    #[test]
    fn a_flag_in_the_last_column_without_autowrap() {
        // A flag (two regional indicators) or an emoji made wide by VS16,
        // kept in one narrow cell where autowrap is off.
        for input in [
            "\x1b[?7l\x1b[5G\u{1f1fa}\u{1f1f8}",
            "\x1b[?7l\x1b[5G\u{263a}\u{fe0f}",
        ] {
            let mut a = T::new(5, 3);
            a.feed(input.as_bytes());
            assert!(!a.term.grid[Line(0)][Column(4)].is_wide());
            let mut b = a.copy();
            assert_eq!(cells(&a), cells(&b), "{input:?}");
            same(&mut a, &mut b, b"\r\nx");
        }
    }

    #[test]
    fn neighbours_that_would_join_stay_apart() {
        // Each written after the one to its right: two regional
        // indicators, and an emoji ending in a joiner before another.
        for input in [
            "\x1b[2G\u{1f1fa}\x1b[1G\u{1f1f8}",
            "\x1b[3G\u{1f466}\x1b[1G\u{1f469}\u{200d}",
        ] {
            let mut a = T::new(6, 2);
            a.feed(input.as_bytes());
            let b = a.copy();
            assert_eq!(cells(&a), cells(&b), "{input:?}");
        }
    }

    // Hyperlinks.

    #[test]
    fn a_link_with_a_gap_in_it() {
        // The cells the cursor stepped over have no link: the copy's
        // don't either, and the cells on both sides of them do.
        let mut a = T::new(20, 3);
        a.feed(b"\x1b]8;;https://example.com\x1b\\ab\x1b[5Gcd\x1b]8;;\x1b\\");
        let b = a.copy();
        assert_eq!(b.links(), a.links());
        assert_eq!(
            b.links().iter().map(|l| l.1).collect::<Vec<_>>(),
            [0, 1, 4, 5]
        );
    }

    #[test]
    fn one_link_after_another_and_a_row_without_any() {
        let mut a = T::new(20, 3);
        a.feed(b"\x1b]8;;https://a.example\x1b\\ab\x1b]8;;https://b.example\x1b\\cd\r\n\x1b]8;;\x1b\\ef");
        let b = a.copy();
        assert_eq!(b.links(), a.links());
        // Closed before the next row's text.
        assert!(!b.links().iter().any(|l| l.0 == 1));
    }

    #[test]
    fn links_with_the_same_uri_and_different_ids_stay_apart() {
        let mut a = T::new(20, 3);
        a.feed(b"\x1b]8;id=1;https://same\x1b\\ab\x1b]8;id=2;https://same\x1b\\cd\x1b]8;;\x1b\\");
        let b = a.copy();
        let ids = |t: &T| {
            (0..4)
                .map(|c| {
                    t.term
                        .cell_hyperlink(Line(0), Column(c))
                        .map(|l| l.id().to_owned())
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&b), ids(&a));
        assert_ne!(ids(&b)[1], ids(&b)[2]);
    }

    #[test]
    fn a_link_on_a_wide_character_that_wrapped() {
        // The blanks before the wide character that went on to the next
        // row are outside the link.
        let mut a = T::new(10, 4);
        a.feed("\x1b]8;;https://example.com\x1b\\abc\x1b[10G界\x1b]8;;\x1b\\".as_bytes());
        let mut b = a.copy();
        assert_eq!(a.text(), b.text());
        assert_eq!(b.links(), a.links());
        same(&mut a, &mut b, b"!");
    }

    // The terminal's state.

    #[test]
    fn a_scroll_region_scrolls_the_same_after() {
        let mut a = T::new(10, 6);
        a.feed(b"1\r\n2\r\n3\r\n4\r\n5\r\n6\x1b[2;4r\x1b[4;1H");
        let mut b = a.copy();
        assert_same(&a, &b);
        assert_eq!(b.term.scroll_region, a.term.scroll_region);
        same(&mut a, &mut b, b"\nx\n\ny\x1b[2;1H\x1bMz");
        assert_eq!(a.term.grid.history_size(), b.term.grid.history_size());
    }

    #[test]
    fn a_one_row_region_at_the_bottom() {
        // DECSTBM wants two rows, but a region past the screen's end
        // clamps to its last one.
        let mut a = T::new(10, 4);
        a.feed(b"1\r\n2\r\n3\r\n4\x1b[4;9r");
        assert_eq!(a.term.scroll_region, Line(3)..Line(4));
        let mut b = a.copy();
        assert_eq!(b.term.scroll_region, a.term.scroll_region);
        same(&mut a, &mut b, b"\x1b[4;1Hx\ny\nz");
    }

    #[test]
    fn cursor_in_origin_mode_without_a_region() {
        let mut a = T::new(10, 4);
        a.feed(b"\x1b[?6h\x1b[3;5H");
        let mut b = a.copy();
        assert_same(&a, &b);
        same(&mut a, &mut b, b"x\x1b[1;1Hy\x1b[2;8r\x1b[1;1Hz");
    }

    #[test]
    fn cursor_in_origin_mode_waiting_to_wrap() {
        let mut a = T::new(5, 6);
        a.feed(b"\x1b[2;4r\x1b[?6h\x1b[3;1Habcde");
        assert!(a.term.grid.cursor.should_wrap);
        let mut b = a.copy();
        assert_same(&a, &b);
        // Wrapping at the region's bottom scrolls the region.
        same(&mut a, &mut b, b"f");
        assert_eq!(cells(&a), cells(&b));
    }

    #[test]
    fn all_tab_stops_cleared() {
        let mut a = T::new(20, 3);
        a.feed(b"\x1b[3g");
        let mut b = a.copy();
        same(&mut a, &mut b, b"\tx");
        assert_eq!(b.cursor().0.col, Column(19));
        assert_eq!(b.term.tabs.tabs, a.term.tabs.tabs);
    }

    #[test]
    fn tab_stops_off_the_eighth_columns() {
        let mut a = T::new(30, 3);
        a.feed(b"\x1b[3G\x1bH\x1b[9G\x1b[g\x1b[30G\x1bH\r");
        let mut b = a.copy();
        assert_eq!(b.term.tabs.tabs, a.term.tabs.tabs);
        same(&mut a, &mut b, b"\t1\t2\t3\t4\t5");
    }

    #[test]
    fn shifted_out_and_other_slots_of_line_drawing() {
        // G1 shifted in with SO, and G2 and G3 designated too.
        let mut a = T::new(20, 3);
        a.feed(b"\x1b)0\x1b*0\x1b+0\x0eqx");
        let mut b = a.copy();
        assert_same(&a, &b);
        assert_eq!(b.term.active_charset, a.term.active_charset);
        same(&mut a, &mut b, b"lqk\x0fqx");
        assert!(b.text().contains("─│┌─┐qx"), "{:?}", b.text());
    }

    #[test]
    fn keyboard_flags_set_in_place_over_pushed_ones() {
        let mut a = T::new(20, 3);
        a.feed(b"\x1b[>1u\x1b[>2u\x1b[=5;1u\x1b[=8;2u\x1b[=4;3u");
        let mut b = a.copy();
        assert_eq!(b.term.keyboard_mode_stack, a.term.keyboard_mode_stack);
        assert_eq!(b.term.keyboard_mode_idx, a.term.keyboard_mode_idx);
        for _ in 0..3 {
            same(&mut a, &mut b, b"\x1b[<u");
            assert_eq!(b.term.keyboard_mode(), a.term.keyboard_mode());
        }
    }

    #[test]
    fn a_directory_with_bytes_to_escape() {
        let mut a = T::new(20, 3);
        a.feed(b"\x1b]7;file:///tmp/%E7%95%8C%3B%25x\x07");
        let b = a.copy();
        assert!(a.term.current_directory.is_some());
        assert_eq!(b.term.current_directory, a.term.current_directory);
    }

    #[test]
    fn a_title_with_separators_and_wide_text() {
        let mut a = T::new(20, 3);
        a.feed("\x1b]0;vim a;b \u{754c} = 1\x07".as_bytes());
        let b = a.copy();
        assert_eq!(b.term.title, a.term.title);
    }

    #[test]
    fn a_marked_cell_inside_a_link() {
        let mut a = T::new(20, 3);
        a.feed("\x1b]8;;http://a\x1b\\xe\u{301}yz\x1b]8;;\x1b\\".as_bytes());
        let b = a.copy();
        assert_eq!(cells(&a), cells(&b));
        assert_eq!(b.links().len(), 4);
    }

    // The cursor and DECSC.

    #[test]
    fn a_pending_wrap_over_an_erased_cell() {
        // Saving keeps the pending wrap and erasing the row clears the
        // cell under it: restoring leaves one over a blank.
        let mut a = T::new(4, 2);
        a.feed(b"abcd\x1b7\x1b[2K\x1b8");
        assert!(a.term.grid.cursor.should_wrap);
        let mut b = a.copy();
        assert_same(&a, &b);
        same(&mut a, &mut b, b"X");
    }

    #[test]
    fn a_saved_cursor_waiting_to_wrap() {
        let mut a = T::new(5, 5);
        a.feed(b"\x1b[1;5HA\x1b7\x1b[1;1HB");
        let mut b = a.copy();
        assert_same(&a, &b);
        same(&mut a, &mut b, b"\x1b8X");
        assert_eq!(b.term.grid[Line(1)][Column(0)].c(), 'X');
    }

    #[test]
    fn waiting_to_wrap_on_a_wrapped_row_at_the_regions_bottom() {
        // Saved waiting to wrap on a row that wraps, which a region set
        // afterwards ends on; the second time restored there too. The
        // row wraps on in the copy, and the next character scrolls the
        // region the same.
        for input in [
            &b"\x1b[2;1HABCDE\x1b7F\x1b[1;2r"[..],
            b"\x1b[2;1HABCDE\x1b7F\x1b[1;2r\x1b8",
        ] {
            let mut a = T::new(5, 5);
            a.feed(input);
            assert!(a.term.grid[Line(1)][Column(4)].wrapline());
            let mut b = a.copy();
            assert_eq!(cells(&a), cells(&b), "{input:?}");
            same(&mut a, &mut b, b"\x1b8X");
            assert_eq!(cells(&a), cells(&b), "{input:?}");
        }
    }

    #[test]
    fn decsc_saves_the_pen_link_and_charsets_with_the_position() {
        let mut a = T::new(20, 4);
        a.feed(b"\x1b[2;3H\x1b[4;35m\x1b]8;;https://s\x1b\\\x1b(0\x1b7\x1b[0m\x1b]8;;\x1b\\\x1b(B\x1b[4;1Hplain");
        let mut b = a.copy();
        assert_eq!(state(&a), state(&b));
        same(&mut a, &mut b, b"\x1b8q");
        assert_eq!(state(&a), state(&b));
        assert_eq!(b.links(), [(1, 2, "https://s".to_owned())]);
    }

    #[test]
    fn decrc_after_the_region_changed() {
        // Saved in origin mode at the region's top-left; the region moved
        // since. Restoring puts the cursor where it was saved.
        let mut a = T::new(10, 8);
        a.feed(b"\x1b[2;4r\x1b[?6h\x1b7\x1b[?6l\x1b[5;7r");
        let mut b = a.copy();
        assert_eq!(state(&a), state(&b));
        same(&mut a, &mut b, b"\x1b8X\x1b[1;1HY\x1b[10BZ");
        assert_eq!(state(&a), state(&b));
    }

    #[test]
    fn the_primary_cursor_under_the_alt_screen_with_its_pen() {
        let mut a = T::new(10, 4);
        a.feed(b"\x1b[1;32m\x1b]8;;https://p\x1b\\\x1b)0\x0e$ \x1b[?1049h\x1b[0m\x1b]8;;\x1b\\\x0fvim\x1b7");
        let mut b = a.copy();
        assert_eq!(state(&a), state(&b));
        same(&mut a, &mut b, b"\x1b[?1049lqx");
        assert_eq!(state(&a), state(&b));
        assert_eq!(b.links(), a.links());
    }

    // Cursor movement and origin mode, round-tripped.

    #[test]
    fn cuu_and_cud_from_a_restored_cursor_stop_at_the_margins() {
        let mut a = T::new(10, 8);
        a.feed(b"\x1b[3;6r\x1b[5;2H");
        let mut b = a.copy();
        same(&mut a, &mut b, b"\x1b[9A");
        assert_eq!(b.cursor().0.row, Line(2));
        same(&mut a, &mut b, b"\x1b[9B");
        assert_eq!(b.cursor().0.row, Line(5));
        same(&mut a, &mut b, b"\x1b[8;1H\x1b[9A");
        assert_eq!(b.cursor().0.row, Line(2));
        same(&mut a, &mut b, b"\x1b[1;1H\x1b[9B");
        assert_eq!(b.cursor().0.row, Line(5));
    }

    #[test]
    fn cup_in_origin_mode_counts_from_the_region() {
        let mut a = T::new(5, 5);
        a.feed(b"\x1b[3;4r\x1b[?6h\x1b[1;1HX");
        assert_eq!(a.term.format(FormatOptions::plain()), "\n\nX");
        // And stops at its bottom.
        a.feed(b"\x1b[9;1HY");
        assert_eq!(a.term.format(FormatOptions::plain()), "\n\nX\nY");
        let b = a.copy();
        assert_eq!(state(&a), state(&b));
    }

    #[test]
    fn decom_while_waiting_to_wrap_homes_the_cursor() {
        let mut a = T::new(5, 5);
        a.feed(b"\x1b[2;4rabcde\x1b[?6h");
        assert!(!a.term.grid.cursor.should_wrap);
        assert_eq!(a.cursor().0, Pos::new(Line(1), Column(0)));
        a.feed(b"\x1b[?6l");
        assert_eq!(a.cursor().0, Pos::new(Line(0), Column(0)));
    }

    #[test]
    fn ris_turns_origin_mode_off_and_homes_the_cursor() {
        let mut a = T::new(10, 6);
        a.feed(b"\x1b[2;5r\x1b[?6h\x1b[3;5H\x1b7\x1b[?1049h\x1b[>3u\x1bc");
        assert!(!a.term.mode().contains(Mode::ORIGIN));
        assert_eq!(a.cursor(), (Pos::default(), false));
        assert_eq!(a.term.keyboard_mode(), crate::ansi::KeyboardModes::NO_MODE);
        let b = a.copy();
        assert_eq!(state(&a), state(&b));
    }

    /// Everything a checkpoint carries, as text to compare: the cells of
    /// both screens, the cursor and the saved one on each (position,
    /// pending wrap, pen, link, charsets, origin mode), modes, region, tab
    /// stops, charsets, both keyboard flag stacks, the keyboard, the cursor's
    /// look, title, directory and colors.
    fn state(t: &T) -> String {
        use crate::crosswords::pos::Cursor;
        let term = &t.term;
        let pen = |g: &Grid<Square>, c: &Cursor<Square>| {
            let link = cell_link(g, &c.template).map(|l| l.uri().to_owned());
            format!(
                "{:?} wrap {} {:?} {:?} {link:?}",
                c.pos, c.should_wrap, c.pending_style, c.charsets
            )
        };
        let (g, ig) = (&term.grid, &term.inactive_grid);
        let mut s = format!("{:?}\n", cells(t));
        let _ = writeln!(s, "cursor {}", pen(g, &g.cursor));
        let _ = writeln!(
            s,
            "saved {} origin {}",
            pen(g, &g.saved_cursor),
            g.saved_origin
        );
        if term.mode().contains(Mode::ALT_SCREEN) {
            let _ = writeln!(s, "primary cursor {}", pen(ig, &ig.cursor));
        }
        let _ = writeln!(
            s,
            "other saved {} origin {}",
            pen(ig, &ig.saved_cursor),
            ig.saved_origin
        );
        let _ = writeln!(
            s,
            "mode {:?} region {:?} tabs {:?} active {:?}",
            term.mode(),
            term.scroll_region,
            term.tabs.tabs,
            term.active_charset
        );
        let _ = writeln!(
            s,
            "keyboard {:?}@{} {:?}@{} {:?} other keys {}",
            term.keyboard_mode_stack,
            term.keyboard_mode_idx,
            term.inactive_keyboard_mode_stack,
            term.inactive_keyboard_mode_idx,
            term.keyboard_mode(),
            term.modify_other_keys
        );
        let _ = writeln!(
            s,
            "look {:?} {} {} title {:?} dir {:?}",
            term.cursor_shape,
            term.blinking_cursor,
            term.cursor_is_default,
            term.title,
            term.current_directory
        );
        for i in 0..crate::config::colors::term::COUNT {
            let _ = write!(s, "{:?}", term.colors[i]);
        }
        s
    }

    /// A terminal fed P then Q, and one rebuilt from the first one's
    /// checkpoint after P (palette and REP's character included) then fed
    /// Q, end up the same: state compared at the checkpoint and after Q.
    #[test]
    fn random_checkpoints_resume_exactly() {
        let tokens: &[&str] = &[
            "hello",
            "x",
            "1234567890123",
            " ",
            "\t",
            "\r\n",
            "\n",
            "\r",
            "\x08",
            "\x1bD",
            "\x1bM",
            "\x1bE",
            "界",
            "e\u{301}",
            "\u{301}",
            "\u{1f1fa}",
            "\u{1f1f8}",
            "\u{200d}",
            "\u{fe0f}",
            "\x1b[31m",
            "\x1b[1;4m",
            "\x1b[48;5;33m",
            "\x1b[7m",
            "\x1b[0m",
            "\x1b[K",
            "\x1b[1K",
            "\x1b[J",
            "\x1b[2X",
            "\x1b[H",
            "\x1b[3;3H",
            "\x1b[2A",
            "\x1b[9A",
            "\x1b[2B",
            "\x1b[9B",
            "\x1b[E",
            "\x1b[2F",
            "\x1b[3C",
            "\x1b[2D",
            "\x1b[4G",
            "\x1b[2d",
            "\x1b[3b",
            "\x1b[L",
            "\x1b[M",
            "\x1b[S",
            "\x1b[T",
            "\x1b7",
            "\x1b8",
            "\x1b[2;4r",
            "\x1b[3;9r",
            "\x1b[r",
            "\x1b[?6h",
            "\x1b[?6l",
            "\x1b[?7l",
            "\x1b[?7h",
            "\x1b[20h",
            "\x1b[20l",
            "\x1b[?1049h",
            "\x1b[?1049l",
            "\x1b(0",
            "\x1b(B",
            "\x1b)0",
            "\x1b*0",
            "\x0e",
            "\x0f",
            "lqkx",
            "\x1bH",
            "\x1b[g",
            "\x1b[3g",
            "\x1b]8;;http://a\x1b\\",
            "\x1b]8;id=q;http://b\x1b\\",
            "\x1b]8;;\x1b\\",
            "\x1b[>1u",
            "\x1b[>3u",
            "\x1b[=5;1u",
            "\x1b[=2;2u",
            "\x1b[<u",
            "\x1b[<3u",
            "\x1b[>4;1m",
            "\x1b[?2004h",
            "\x1b[?1000h",
            "\x1b[?1006h",
            "\x1b=",
            "\x1b>",
            "\x1b[?25l",
            "\x1b[?25h",
            "\x1b[4 q",
            "\x1b[0 q",
            "\x1b]2;title\x07",
            "\x1b]7;file:///tmp/x\x07",
            "\x1b]4;5;#102030\x07",
            "\x1b]11;rgb:01/02/03\x07",
        ];
        // Nothing moves cells sideways (insert mode, ICH, DCH): that carries
        // a soft-wrap flag rio leaves on a cell that is no longer last in
        // its row, which a copy doesn't have. Nor does it write marks that
        // join the cell before them (REP of one included): a mark joins a
        // blank that was written but not one never written, and the copy
        // can't tell those apart (it writes the gaps in a row as blanks
        // and trims the blanks at its end).
        let marks = ["\u{301}", "e\u{301}", "\u{200d}", "\u{fe0f}"];
        let q: Vec<&str> = tokens
            .iter()
            .copied()
            .filter(|t| !marks.contains(t))
            .collect();
        let no_rep: Vec<&str> = q.iter().copied().filter(|t| *t != "\x1b[3b").collect();
        let mut x: u64 = 0x2545_f491_4f6c_dd1d;
        let mut rnd = |n: usize| {
            x = x
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (x >> 33) as usize % n
        };
        let opts = FormatOptions::snapshot().with_palette(true);
        let mut compared = 0;
        for seed in 0..1_500 {
            let cols = 4 + rnd(12);
            let rows = 2 + rnd(6);
            let mut a = T::new(cols, rows);
            let mut input = String::new();
            for _ in 0..rnd(120) {
                let token = tokens[rnd(tokens.len())];
                input.push_str(token);
                a.feed(token.as_bytes());
            }
            assert!(a.parser.is_ground());
            if beyond_a_snapshot(&a) {
                continue;
            }
            let mut b = T::new(cols, rows);
            b.feed(a.term.format(opts).as_bytes());
            b.parser.set_preceding_char(a.parser.preceding_char());
            let (sa, sb) = (state(&a), state(&b));
            assert!(
                sa == sb,
                "seed {seed} ({cols}x{rows}) at the checkpoint\ninput {input:?}\n{}",
                diff(&sa, &sb)
            );
            input.push_str("\u{0}Q\u{0}");
            let mark = a.parser.preceding_char().is_some_and(|c| {
                crate::codepoint_width::codepoint_width(c as u32) == Some(0)
            });
            let q = if mark { &no_rep } else { &q };
            for _ in 0..rnd(40) {
                let token = q[rnd(q.len())];
                input.push_str(token);
                a.feed(token.as_bytes());
                b.feed(token.as_bytes());
            }
            let (sa, sb) = (state(&a), state(&b));
            assert!(
                sa == sb,
                "seed {seed} ({cols}x{rows}) after Q\ninput {input:?}\n{}",
                diff(&sa, &sb)
            );
            compared += 1;
        }
        assert!(compared > 500, "{compared}");
    }

    /// The first line of `a` and `b` that differs.
    fn diff(a: &str, b: &str) -> String {
        let la: Vec<_> = a.lines().collect();
        let lb: Vec<_> = b.lines().collect();
        for (i, (x, y)) in la.iter().zip(&lb).enumerate() {
            if x != y {
                if i == 0 {
                    // The cells: the first one that differs.
                    let first = cells_diff(x, y);
                    return format!("cells: {first}");
                }
                return format!("a: {x}\nb: {y}");
            }
        }
        format!("{} lines vs {}", la.len(), lb.len())
    }

    fn cells_diff(a: &str, b: &str) -> String {
        let at = a
            .bytes()
            .zip(b.bytes())
            .position(|(x, y)| x != y)
            .unwrap_or(a.len().min(b.len()));
        let from = a[..at].rfind("(\"").unwrap_or(0);
        let end = |s: &str| (at + 160).min(s.len());
        let (ea, eb) = (end(a), end(b));
        format!(
            "\na: {}\nb: {}",
            a.get(from..ea).unwrap_or("?"),
            b.get(from..eb).unwrap_or("?")
        )
    }

    /// What a snapshot is known not to carry (see the module docs and
    /// `random_terminals_round_trip`), and what the ignored tests above
    /// show it gets wrong.
    fn beyond_a_snapshot(a: &T) -> bool {
        let term = &a.term;
        let (g, ig) = (&term.grid, &term.inactive_grid);
        let (c, saved) = (&g.cursor, &g.saved_cursor);
        let wraps = |g: &Grid<Square>, row: i32| {
            row + 1 < g.screen_lines() as i32
                && g[Line(row)][Column(g.columns() - 1)].wrapline()
        };
        let together = saved.should_wrap && saved.pos == c.pos;
        if c.should_wrap && wraps(g, c.pos.row.0) && !together {
            return true;
        }
        // `waiting_to_wrap_on_a_wrapped_row_at_the_regions_bottom`.
        let region = &term.scroll_region;
        let row = saved.pos.row.0;
        if saved.should_wrap && row + 1 == region.end.0 && wraps(g, row) {
            return true;
        }
        if term.mode().contains(Mode::ORIGIN) && !region.contains(&c.pos.row) {
            return true;
        }
        let on_lead = |g: &Grid<Square>, cur: &crate::crosswords::pos::Cursor<Square>| {
            let row = cur.pos.row.0.clamp(0, g.screen_lines() as i32 - 1);
            cur.should_wrap && g[Line(row)][Column(g.columns() - 1)].is_leading_spacer()
        };
        if on_lead(g, c) || on_lead(g, saved) || on_lead(ig, &ig.cursor) {
            return true;
        }
        let ri = |c: char| ('\u{1f1e6}'..='\u{1f1ff}').contains(&c);
        let any = |g: &Grid<Square>| {
            let rows = g.screen_lines() as i32;
            let mut last: Option<char> = None;
            (-(g.history_size() as i32)..rows).any(|line| {
                let row = &g[Line(line)][..];
                // Joins carry over a soft wrap: the copy writes on.
                if line > -(g.history_size() as i32) && !wraps(g, line - 1) {
                    last = None;
                }
                let bottom_wraps = line == rows - 1 && row[row.len() - 1].wrapline();
                bottom_wraps
                    || (0..row.len()).any(|col| {
                        let cell = row[col];
                        let text: Vec<char> =
                            g.cell_text(Pos::new(Line(line), Column(col))).collect();
                        // A split or squeezed wide character.
                        let split = cell.is_leading_spacer()
                            || broken_half(row, col)
                            || (!cell.is_wide()
                                && crate::codepoint_width::codepoint_width(
                                    cell.c() as u32
                                ) == Some(2));
                        // A character joined to a wide one re-writes its
                        // second half with the pen (and link) of the
                        // moment: two halves in two styles.
                        let two_pens = cell.is_spacer()
                            && col > 0
                            && row[col - 1].is_wide()
                            && (g.style_of(&cell) != g.style_of(&row[col - 1])
                                || cell_link(g, &cell) != cell_link(g, &row[col - 1]));
                        // `a_combining_mark_on_a_trailing_blank`.
                        let marked_blank =
                            matches!(cell.c(), ' ' | '\0') && text.len() > 1;
                        // `a_flag_in_the_last_column_without_autowrap`.
                        let squeezed_cluster = !cell.is_wide()
                            && (text.contains(&'\u{fe0f}')
                                || text.iter().filter(|&&c| ri(c)).count() > 1);
                        // `neighbours_that_would_join_stay_apart`.
                        let joins = !cell.is_spacer()
                            && last.is_some_and(|p| {
                                p == '\u{200d}' || (ri(p) && ri(text[0]))
                            });
                        if !cell.is_spacer() {
                            last = text
                                .last()
                                .copied()
                                .filter(|&l| text.len() == 1 || l == '\u{200d}');
                        }
                        split || two_pens || marked_blank || squeezed_cluster || joins
                    })
            })
        };
        any(g) || any(ig)
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
            Sgr::to(&mut s, |sgr| sgr.num(n));
            assert_eq!(s, n.to_string());
        }
    }
}
