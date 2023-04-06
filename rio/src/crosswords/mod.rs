/*
    Crosswords -> Rio's grid manager

    |----------------------------------|
    |-$-bash:-echo-1-------------------|
    |-1--------------------------------|
    |----------------------------------|
    |----------------------------------|
    |----------------------------------|
    |----------------------------------|
    |----------------------------------|

*/

pub mod attr;
pub mod grid;
pub mod pos;
pub mod square;

use crate::ansi::mode::Mode as AnsiMode;
use crate::crosswords::grid::Dimensions;
use crate::crosswords::grid::Grid;
use crate::performer::handler::Handler;
use attr::*;
use bitflags::bitflags;
use colors::AnsiColor;
use grid::row::Row;
use pos::CharsetIndex;
use pos::{Column, Cursor, Line, Pos};
use square::Square;
use std::mem;
use std::ops::{Index, IndexMut, Range};
use std::option::Option;
use std::ptr;
use unicode_width::UnicodeWidthChar;

pub type NamedColor = colors::NamedColor;

pub const MIN_COLUMNS: usize = 2;
pub const MIN_VISIBLE_ROWS: usize = 1;

bitflags! {
    #[derive(Debug, Clone)]
    pub struct Mode: u32 {
        const NONE                = 0;
        const SHOW_CURSOR         = 0b0000_0000_0000_0000_0001;
        const APP_CURSOR          = 0b0000_0000_0000_0000_0010;
        const APP_KEYPAD          = 0b0000_0000_0000_0000_0100;
        const MOUSE_REPORT_CLICK  = 0b0000_0000_0000_0000_1000;
        const BRACKETED_PASTE     = 0b0000_0000_0000_0001_0000;
        const SGR_MOUSE           = 0b0000_0000_0000_0010_0000;
        const MOUSE_MOTION        = 0b0000_0000_0000_0100_0000;
        const LINE_WRAP           = 0b0000_0000_0000_1000_0000;
        const LINE_FEED_NEW_LINE  = 0b0000_0000_0001_0000_0000;
        const ORIGIN              = 0b0000_0000_0010_0000_0000;
        const INSERT              = 0b0000_0000_0100_0000_0000;
        const FOCUS_IN_OUT        = 0b0000_0000_1000_0000_0000;
        const ALT_SCREEN          = 0b0000_0001_0000_0000_0000;
        const MOUSE_DRAG          = 0b0000_0010_0000_0000_0000;
        const MOUSE_MODE          = 0b0000_0010_0000_0100_1000;
        const UTF8_MOUSE          = 0b0000_0100_0000_0000_0000;
        const ALTERNATE_SCROLL    = 0b0000_1000_0000_0000_0000;
        const VI                  = 0b0001_0000_0000_0000_0000;
        const URGENCY_HINTS       = 0b0010_0000_0000_0000_0000;
        const ANY                 = u32::MAX;
    }
}

impl Default for Mode {
    fn default() -> Mode {
        Mode::SHOW_CURSOR | Mode::LINE_WRAP | Mode::ALTERNATE_SCROLL | Mode::URGENCY_HINTS
    }
}

#[derive(Debug, Clone)]
pub struct Crosswords<U> {
    active_charset: CharsetIndex,
    mode: Mode,
    grid: Grid<Square>,
    inactive_grid: Grid<Square>,
    scroll_region: Range<Line>,
    tabs: TabStops,
    #[allow(dead_code)]
    event_proxy: U,
    window_title: Option<String>,
    damage: TermDamageState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineDamageBounds {
    /// Damaged line number.
    pub line: usize,

    /// Leftmost damaged column.
    pub left: usize,

    /// Rightmost damaged column.
    pub right: usize,
}

impl LineDamageBounds {
    #[inline]
    pub fn undamaged(num_cols: usize, line: usize) -> Self {
        Self {
            line,
            left: num_cols,
            right: 0,
        }
    }

    #[inline]
    pub fn reset(&mut self, num_cols: usize) {
        *self = Self::undamaged(num_cols, self.line);
    }

    #[inline]
    pub fn expand(&mut self, left: usize, right: usize) {
        self.left = std::cmp::min(self.left, left);
        self.right = std::cmp::max(self.right, right);
    }

    #[inline]
    pub fn is_damaged(&self) -> bool {
        self.left <= self.right
    }
}

#[derive(Debug, Clone)]
struct TermDamageState {
    /// Hint whether terminal should be damaged entirely regardless of the actual damage changes.
    is_fully_damaged: bool,

    /// Information about damage on terminal lines.
    lines: Vec<LineDamageBounds>,

    /// Old terminal cursor point.
    last_cursor: Pos,

    /// Last Vi cursor point.
    last_vi_cursor_point: Option<Pos>,
    // Old selection range.
    // last_selection: Option<SelectionRange>,
}

impl TermDamageState {
    fn new(num_cols: usize, num_lines: usize) -> Self {
        let lines = (0..num_lines)
            .map(|line| LineDamageBounds::undamaged(num_cols, line))
            .collect();

        Self {
            is_fully_damaged: true,
            lines,
            last_cursor: Default::default(),
            last_vi_cursor_point: Default::default(),
            // last_selection: Default::default(),
        }
    }

    #[inline]
    fn resize(&mut self, num_cols: usize, num_lines: usize) {
        // Reset point, so old cursor won't end up outside of the viewport.
        self.last_cursor = Default::default();
        self.last_vi_cursor_point = None;
        // self.last_selection = None;
        self.is_fully_damaged = true;

        self.lines.clear();
        self.lines.reserve(num_lines);
        for line in 0..num_lines {
            self.lines.push(LineDamageBounds::undamaged(num_cols, line));
        }
    }

    /// Damage point inside of the viewport.
    #[inline]
    fn damage_point(&mut self, _point: Pos) {
        // self.damage_line(point.line, point.column.0, point.column.0);
    }

    /// Expand `line`'s damage to span at least `left` to `right` column.
    #[inline]
    fn damage_line(&mut self, line: usize, left: usize, right: usize) {
        self.lines[line].expand(left, right);
    }

    fn damage_selection(
        &mut self,
        // selection: SelectionRange,
        display_offset: usize,
        num_cols: usize,
    ) {
        let display_offset = display_offset as i32;
        let last_visible_line = self.lines.len() as i32 - 1;

        // Don't damage invisible selection.
        // if selection.end.line.0 + display_offset < 0
        //     || selection.start.line.0.abs() < display_offset - last_visible_line
        // {
        //     return;
        // };

        // let start = std::cmp::max(selection.start.line.0 + display_offset, 0);
        // let end = (selection.end.line.0 + display_offset).clamp(0, last_visible_line);
        // for line in start as usize..=end as usize {
        //     self.damage_line(line, 0, num_cols - 1);
        // }
    }

    /// Reset information about terminal damage.
    fn reset(&mut self, num_cols: usize) {
        self.is_fully_damaged = false;
        self.lines.iter_mut().for_each(|line| line.reset(num_cols));
    }
}

#[derive(Debug, Clone)]
struct TabStops {
    tabs: Vec<bool>,
}

/// Default tab interval, corresponding to terminfo `it` value.
const INITIAL_TABSTOPS: usize = 8;

impl TabStops {
    #[inline]
    fn new(columns: usize) -> TabStops {
        TabStops {
            tabs: (0..columns).map(|i| i % INITIAL_TABSTOPS == 0).collect(),
        }
    }

    /// Remove all tabstops.
    #[inline]
    #[allow(unused)]
    fn clear_all(&mut self) {
        unsafe {
            ptr::write_bytes(self.tabs.as_mut_ptr(), 0, self.tabs.len());
        }
    }

    /// Increase tabstop capacity.
    #[inline]
    #[allow(unused)]
    fn resize(&mut self, columns: usize) {
        let mut index = self.tabs.len();
        self.tabs.resize_with(columns, || {
            let is_tabstop = index % INITIAL_TABSTOPS == 0;
            index += 1;
            is_tabstop
        });
    }
}

impl Index<Column> for TabStops {
    type Output = bool;

    fn index(&self, index: Column) -> &bool {
        &self.tabs[index.0]
    }
}

impl IndexMut<Column> for TabStops {
    fn index_mut(&mut self, index: Column) -> &mut bool {
        self.tabs.index_mut(index.0)
    }
}

impl<U> Crosswords<U> {
    pub fn new(cols: usize, rows: usize, event_proxy: U) -> Crosswords<U> {
        let grid = Grid::new(rows, cols, 10_000);
        let alt = Grid::new(rows, cols, 0);

        let scroll_region = Line(0)..Line(rows as i32);

        Crosswords {
            grid,
            inactive_grid: alt,
            active_charset: CharsetIndex::default(),
            scroll_region,
            event_proxy,
            window_title: Option::Some(String::from("")),
            tabs: TabStops::new(cols),
            mode: Mode::SHOW_CURSOR
                | Mode::LINE_WRAP
                | Mode::ALTERNATE_SCROLL
                | Mode::URGENCY_HINTS,
            damage: TermDamageState::new(cols, rows),
        }
    }

    pub fn resize<S: Dimensions>(&mut self, num_cols: usize, num_lines: usize) {
        let old_cols = self.grid.columns();
        let old_lines = self.grid.screen_lines();

        if old_cols == num_cols && old_lines == num_lines {
            println!("Term::resize dimensions unchanged");
            return;
        }

        println!("Old cols is {} and lines is {}", old_cols, old_lines);
        println!("New cols is {} and lines is {}", num_cols, num_lines);

        // Move vi mode cursor with the content.
        let history_size = self.history_size();
        let mut delta = num_lines as i32 - old_lines as i32;
        let min_delta =
            std::cmp::min(0, num_lines as i32 - self.grid.cursor.pos.row.0 - 1);
        delta = std::cmp::min(std::cmp::max(delta, min_delta), history_size as i32);
        // self.vi_mode_cursor.point.line += delta;

        let is_alt = self.mode.contains(Mode::ALT_SCREEN);
        self.grid.resize(!is_alt, num_lines, num_cols);
        self.inactive_grid.resize(is_alt, num_lines, num_cols);

        // Invalidate selection and tabs only when necessary.
        if old_cols != num_cols {
            // self.selection = None;

            // Recreate tabs list.
            self.tabs.resize(num_cols);
        }
        //  else if let Some(selection) = self.selection.take() {
        //     let max_lines = cmp::max(num_lines, old_lines) as i32;
        //     let range = Line(0)..Line(max_lines);
        //     self.selection = selection.rotate(self, &range, -delta);
        // }

        // Clamp vi cursor to viewport.
        // let vi_point = self.vi_mode_cursor.point;
        // let viewport_top = Line(-(self.scroll as i32));
        // let viewport_bottom = viewport_top + self.bottommost_line();
        // self.vi_mode_cursor.point.line =
        // cmp::max(cmp::min(vi_point.line, viewport_bottom), viewport_top);
        // self.vi_mode_cursor.point.column = cmp::min(vi_point.column, self.last_column());

        // Reset scrolling region.
        self.scroll_region = Line(0)..Line(self.grid.screen_lines() as i32);

        // Resize damage information.
        self.damage.resize(num_cols, num_lines);
    }

    #[inline]
    pub fn mode(&self) -> &Mode {
        &self.mode
    }

    #[inline]
    pub fn wrapline(&mut self) {
        if !self.mode.contains(Mode::LINE_WRAP) {
            return;
        }

        self.grid
            .cursor_cell()
            .flags
            .insert(square::Flags::WRAPLINE);

        if self.grid.cursor.pos.row + 1 >= self.scroll_region.end {
            self.linefeed();
        } else {
            // self.damage_cursor();
            self.grid.cursor.pos.row += 1;
        }

        self.grid.cursor.pos.col = Column(0);
        self.grid.cursor.should_wrap = false;
        // self.damage_cursor();
    }

    #[allow(dead_code)]
    #[inline]
    pub fn cursor(&self) -> (Column, Line) {
        (self.grid.cursor.pos.col, self.grid.cursor.pos.row)
    }

    pub fn history_size(&self) -> usize {
        self.grid
            .total_lines()
            .saturating_sub(self.grid.screen_lines())
    }

    #[inline]
    pub fn scroll_up_from_origin(&mut self, origin: Line, mut lines: usize) {
        // println!("Scrolling up: origin={origin}, lines={lines}");

        lines = std::cmp::min(
            lines,
            (self.scroll_region.end - self.scroll_region.start).0 as usize,
        );

        let region = origin..self.scroll_region.end;

        // Scroll selection.
        // self.selection = self.selection.take().and_then(|s| s.rotate(self, &region, lines as i32));

        self.grid.scroll_up(&region, lines);

        // // Scroll vi mode cursor.
        // let viewport_top = Line(-(self.grid.display_offset() as i32));
        // let top = if region.start == 0 { viewport_top } else { region.start };
        // let line = &mut self.vi_mode_cursor.pos.row;
        // if (top <= *line) && region.end > *line {
        // *line = cmp::max(*line - lines, top);
        // }
        // self.mark_fully_damaged();
    }

    pub fn write_at_cursor(&mut self, c: char) {
        let c = self.grid.cursor.charsets[self.active_charset].map(c);
        let fg = self.grid.cursor.template.fg;
        let bg = self.grid.cursor.template.bg;
        let flags = self.grid.cursor.template.flags;
        //     let extra = self.grid.cursor.template.extra.clone();

        let mut cursor_square = self.grid.cursor_square();
        cursor_square.c = c;
        cursor_square.fg = fg;
        cursor_square.bg = bg;
        cursor_square.flags = flags;
        // cursor_cell.extra = extra;
    }

    #[allow(dead_code)]
    pub fn visible_to_string(&mut self) -> String {
        let mut text = String::from("");
        let columns = self.grid.columns();

        for row in self.scroll_region.start.0..self.scroll_region.end.0 {
            for column in 0..columns {
                let square_content = &mut self.grid[Line(row)][Column(column)];
                text.push(square_content.c);
                for c in square_content.zerowidth().into_iter().flatten() {
                    text.push(*c);
                }

                if column == (columns - 1) {
                    text.push('\n');
                }
            }
        }

        text
    }

    #[inline]
    pub fn visible_rows(&mut self) -> Vec<Row<Square>> {
        let mut visible_rows = vec![];
        for row in self.scroll_region.start.0..self.scroll_region.end.0 {
            visible_rows.push(self.grid[Line(row)].to_owned());
        }

        visible_rows
    }

    pub fn grid(&self) -> &Grid<Square> {
        &self.grid
    }

    /// Mutable access to the raw grid data structure.
    pub fn grid_mut(&mut self) -> &mut Grid<Square> {
        &mut self.grid
    }

    pub fn swap_alt(&mut self) {
        if !self.mode.contains(Mode::ALT_SCREEN) {
            // Set alt screen cursor to the current primary screen cursor.
            self.inactive_grid.cursor = self.grid.cursor.clone();

            // Drop information about the primary screens saved cursor.
            self.grid.saved_cursor = self.grid.cursor.clone();

            // Reset alternate screen contents.
            self.inactive_grid.reset_region(..);
        }

        mem::swap(&mut self.grid, &mut self.inactive_grid);
        self.mode ^= Mode::ALT_SCREEN;
        // self.selection = None;
        // self.mark_fully_damaged();
    }
}

impl<U> Handler for Crosswords<U> {
    #[inline]
    fn set_mode(&mut self, mode: AnsiMode) {
        match mode {
            AnsiMode::UrgencyHints => self.mode.insert(Mode::URGENCY_HINTS),
            AnsiMode::SwapScreenAndSetRestoreCursor => {
                if !self.mode.contains(Mode::ALT_SCREEN) {
                    self.swap_alt();
                }
            }
            AnsiMode::ShowCursor => self.mode.insert(Mode::SHOW_CURSOR),
            AnsiMode::CursorKeys => self.mode.insert(Mode::APP_CURSOR),
            // Mouse protocols are mutually exclusive.
            AnsiMode::ReportMouseClicks => {
                self.mode.remove(Mode::MOUSE_MODE);
                self.mode.insert(Mode::MOUSE_REPORT_CLICK);
                // self.event_proxy.send_event(Event::MouseCursorDirty);
            }
            AnsiMode::ReportCellMouseMotion => {
                self.mode.remove(Mode::MOUSE_MODE);
                self.mode.insert(Mode::MOUSE_DRAG);
                // self.event_proxy.send_event(Event::MouseCursorDirty);
            }
            AnsiMode::ReportAllMouseMotion => {
                self.mode.remove(Mode::MOUSE_MODE);
                self.mode.insert(Mode::MOUSE_MOTION);
                // self.event_proxy.send_event(Event::MouseCursorDirty);
            }
            AnsiMode::ReportFocusInOut => self.mode.insert(Mode::FOCUS_IN_OUT),
            AnsiMode::BracketedPaste => self.mode.insert(Mode::BRACKETED_PASTE),
            // Mouse encodings are mutually exclusive.
            AnsiMode::SgrMouse => {
                self.mode.remove(Mode::UTF8_MOUSE);
                self.mode.insert(Mode::SGR_MOUSE);
            }
            AnsiMode::Utf8Mouse => {
                self.mode.remove(Mode::SGR_MOUSE);
                self.mode.insert(Mode::UTF8_MOUSE);
            }
            AnsiMode::AlternateScroll => self.mode.insert(Mode::ALTERNATE_SCROLL),
            AnsiMode::LineWrap => self.mode.insert(Mode::LINE_WRAP),
            AnsiMode::LineFeedNewLine => self.mode.insert(Mode::LINE_FEED_NEW_LINE),
            AnsiMode::Origin => self.mode.insert(Mode::ORIGIN),
            AnsiMode::Column => {
                // self.deccolm(),
            }
            AnsiMode::Insert => self.mode.insert(Mode::INSERT),
            AnsiMode::BlinkingCursor => {
                // let style = self.grid.cursor_style.get_or_insert(self.default_cursor_style);
                // style.blinking = true;
                // self.event_proxy.send_event(Event::CursorBlinkingChange);
            }
        }
    }

    #[inline]
    fn insert_blank_lines(&mut self, lines: usize) {
        println!("insert_blank_lines still unfinished");
        let origin = self.grid.cursor.pos.row;
        if self.scroll_region.contains(&origin) {
            // self.scroll_down_relative(origin, lines);
        }
    }

    #[inline]
    fn terminal_attribute(&mut self, attr: Attr) {
        let cursor = &mut self.grid.cursor;
        // println!("{:?}", attr);
        match attr {
            Attr::Foreground(color) => cursor.template.fg = color,
            Attr::Background(color) => cursor.template.bg = color,
            // Attr::UnderlineColor(color) => cursor.template.set_underline_color(color),
            Attr::Reset => {
                cursor.template.fg = AnsiColor::Named(NamedColor::Foreground);
                cursor.template.bg = AnsiColor::Named(NamedColor::Background);
                cursor.template.flags = square::Flags::empty();
                // cursor.template.set_underline_color(None);
            }
            Attr::Reverse => cursor.template.flags.insert(square::Flags::INVERSE),
            Attr::CancelReverse => cursor.template.flags.remove(square::Flags::INVERSE),
            Attr::Bold => cursor.template.flags.insert(square::Flags::BOLD),
            Attr::CancelBold => cursor.template.flags.remove(square::Flags::BOLD),
            Attr::Dim => cursor.template.flags.insert(square::Flags::DIM),
            Attr::CancelBoldDim => cursor
                .template
                .flags
                .remove(square::Flags::BOLD | square::Flags::DIM),
            Attr::Italic => cursor.template.flags.insert(square::Flags::ITALIC),
            Attr::CancelItalic => cursor.template.flags.remove(square::Flags::ITALIC),
            // Attr::Underline => {
            //     cursor.template.flags.remove(Flags::ALL_UNDERLINES);
            //     cursor.template.flags.insert(Flags::UNDERLINE);
            // },
            // Attr::DoubleUnderline => {
            //     cursor.template.flags.remove(Flags::ALL_UNDERLINES);
            //     cursor.template.flags.insert(Flags::DOUBLE_UNDERLINE);
            // },
            // Attr::Undercurl => {
            //     cursor.template.flags.remove(Flags::ALL_UNDERLINES);
            //     cursor.template.flags.insert(Flags::UNDERCURL);
            // },
            // Attr::DottedUnderline => {
            //     cursor.template.flags.remove(Flags::ALL_UNDERLINES);
            //     cursor.template.flags.insert(Flags::DOTTED_UNDERLINE);
            // },
            // Attr::DashedUnderline => {
            //     cursor.template.flags.remove(Flags::ALL_UNDERLINES);
            //     cursor.template.flags.insert(Flags::DASHED_UNDERLINE);
            // },
            // Attr::CancelUnderline => cursor.template.flags.remove(Flags::ALL_UNDERLINES),
            // Attr::Hidden => cursor.template.flags.insert(Flags::HIDDEN),
            // Attr::CancelHidden => cursor.template.flags.remove(Flags::HIDDEN),
            // Attr::Strike => cursor.template.flags.insert(Flags::STRIKEOUT),
            // Attr::CancelStrike => cursor.template.flags.remove(Flags::STRIKEOUT),
            _ => {
                println!("Term got unhandled attr: {:?}", attr);
            }
        }
    }

    fn set_title(&mut self, window_title: Option<String>) {
        self.window_title = window_title;

        let _title: String = match &self.window_title {
            Some(title) => title.to_string(),
            None => String::from(""),
        };
        // title
    }

    fn input(&mut self, c: char) {
        let width = match c.width() {
            Some(width) => width,
            None => return,
        };

        // Handle zero-width characters.
        if width == 0 {
            // // Get previous column.
            let mut column = self.grid.cursor.pos.col;
            if !self.grid.cursor.should_wrap {
                column.0 = column.saturating_sub(1);
            }

            // // Put zerowidth characters over first fullwidth character cell.
            let row = self.grid.cursor.pos.row;
            if self.grid[row][column]
                .flags
                .contains(square::Flags::WIDE_CHAR_SPACER)
            {
                column.0 = column.saturating_sub(1);
            }

            self.grid[row][column].push_zerowidth(c);
            return;
        }

        if self.grid.cursor.should_wrap {
            self.wrapline();
        }

        let columns = self.grid.columns();
        if self.mode.contains(Mode::INSERT) && self.grid.cursor.pos.col + width < columns
        {
            let line = self.grid.cursor.pos.row;
            let col = self.grid.cursor.pos.col;
            let row = &mut self.grid[line][..];

            for col in (col.0..(columns - width)).rev() {
                row.swap(col + width, col);
            }
        }

        if width == 1 {
            self.write_at_cursor(c);
        } else {
            if self.grid.cursor.pos.col + 1 >= columns {
                if self.mode.contains(Mode::LINE_WRAP) {
                    // Insert placeholder before wide char if glyph does not fit in this row.
                    self.grid
                        .cursor
                        .template
                        .flags
                        .insert(square::Flags::LEADING_WIDE_CHAR_SPACER);
                    self.write_at_cursor(' ');
                    self.grid
                        .cursor
                        .template
                        .flags
                        .remove(square::Flags::LEADING_WIDE_CHAR_SPACER);
                    self.wrapline();
                } else {
                    // Prevent out of bounds crash when linewrapping is disabled.
                    self.grid.cursor.should_wrap = true;
                    return;
                }
            }

            self.grid
                .cursor
                .template
                .flags
                .insert(square::Flags::WIDE_CHAR);
            self.write_at_cursor(c);
            self.grid
                .cursor
                .template
                .flags
                .remove(square::Flags::WIDE_CHAR);

            // Write spacer to cell following the wide glyph.
            self.grid.cursor.pos.col += 1;
            self.grid
                .cursor
                .template
                .flags
                .insert(square::Flags::WIDE_CHAR_SPACER);
            self.write_at_cursor(' ');
            self.grid
                .cursor
                .template
                .flags
                .remove(square::Flags::WIDE_CHAR_SPACER);
        }

        if self.grid.cursor.pos.col + 1 < columns {
            self.grid.cursor.pos.col += 1;
        } else {
            self.grid.cursor.should_wrap = true;
        }
    }

    #[inline]
    fn newline(&mut self) {
        self.linefeed();

        if self.mode.contains(Mode::LINE_FEED_NEW_LINE) {
            self.carriage_return();
        }
    }

    #[inline]
    fn backspace(&mut self) {
        if self.grid.cursor.pos.col > Column(0) {
            self.grid.cursor.should_wrap = false;
            self.grid.cursor.pos.col -= 1;
        }
    }

    fn linefeed(&mut self) {
        let next = self.grid.cursor.pos.row + 1;
        if next == self.scroll_region.end {
            self.scroll_up_from_origin(self.scroll_region.start, 1);
        } else if next < self.grid.screen_lines() {
            self.grid.cursor.pos.row += 1;
        }
    }

    #[inline]
    fn bell(&mut self) {
        println!("[unimplemented] Bell");
    }

    #[inline]
    fn substitute(&mut self) {
        println!("[unimplemented] Substitute");
    }

    #[inline]
    fn put_tab(&mut self, mut count: u16) {
        // A tab after the last column is the same as a linebreak.
        if self.grid.cursor.should_wrap {
            self.wrapline();
            return;
        }

        while self.grid.cursor.pos.col < self.grid.columns() && count != 0 {
            count -= 1;

            let c = self.grid.cursor.charsets[self.active_charset].map('\t');
            let cell = self.grid.cursor_square();
            if cell.c == ' ' {
                cell.c = c;
            }

            loop {
                if (self.grid.cursor.pos.col + 1) == self.grid.columns() {
                    break;
                }

                self.grid.cursor.pos.col += 1;

                if self.tabs[self.grid.cursor.pos.col] {
                    break;
                }
            }
        }
    }

    fn carriage_return(&mut self) {
        let new_col = 0;
        let row = self.grid.cursor.pos.row.0 as usize;
        self.damage
            .damage_line(row, new_col, self.grid.cursor.pos.col.0);
        self.grid.cursor.pos.col = Column(new_col);
        self.grid.cursor.should_wrap = false;
    }

    #[inline]
    fn clear_line(&mut self, mode: u16) {
        let cursor = &self.grid.cursor;
        let bg = cursor.template.bg;
        let pos = &cursor.pos;
        let (left, right) = match mode {
            // Right
            0 => {
                if self.grid.cursor.should_wrap {
                    return;
                }
                (pos.col, Column(self.grid.columns()))
            }
            // Left
            1 => (Column(0), pos.col + 1),
            // All
            2 => (Column(0), Column(self.grid.columns())),
            _ => todo!(),
        };

        self.damage
            .damage_line(pos.row.0 as usize, left.0, right.0 - 1);
        let position = pos.row;
        let row = &mut self.grid[position];
        for square in &mut row[left..right] {
            // *square = bg.into();
            *square = Square::default();
        }
        // let range = self.grid.cursor.pos.row..=self.grid.cursor.pos.row;
        // self.selection = self.selection.take().filter(|s| !s.intersects_range(range));
    }

    #[inline]
    fn text_area_size_pixels(&mut self) {
        println!("text_area_size_pixels");
        // self.event_proxy.send_event(Event::TextAreaSizeRequest(Arc::new(move |window_size| {
        // let height = window_size.num_lines * window_size.cell_height;
        // let width = window_size.num_cols * window_size.cell_width;
        // format!("\x1b[4;{height};{width}t")
        // })));
    }

    #[inline]
    fn text_area_size_chars(&mut self) {
        let text = format!(
            "\x1b[8;{};{}t",
            self.grid.screen_lines(),
            self.grid.columns()
        );
        println!("text_area_size_chars {:?}", text);
        // self.event_proxy.send_event(Event::PtyWrite(text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::VoidListener;

    #[test]
    fn scroll_up() {
        let mut cw = Crosswords::new(1, 10, VoidListener {});
        for i in 0..10 {
            cw.grid[Line(i)][Column(0)].c = i as u8 as char;
        }

        cw.grid.scroll_up(&(Line(0)..Line(10)), 2);

        assert_eq!(cw.grid[Line(0)][Column(0)].c, '\u{2}');
        assert_eq!(cw.grid[Line(0)].occ, 1);
        assert_eq!(cw.grid[Line(1)][Column(0)].c, '\u{3}');
        assert_eq!(cw.grid[Line(1)].occ, 1);
        assert_eq!(cw.grid[Line(2)][Column(0)].c, '\u{4}');
        assert_eq!(cw.grid[Line(2)].occ, 1);
        assert_eq!(cw.grid[Line(3)][Column(0)].c, '\u{5}');
        assert_eq!(cw.grid[Line(3)].occ, 1);
        assert_eq!(cw.grid[Line(4)][Column(0)].c, '\u{6}');
        assert_eq!(cw.grid[Line(4)].occ, 1);
        assert_eq!(cw.grid[Line(5)][Column(0)].c, '\u{7}');
        assert_eq!(cw.grid[Line(5)].occ, 1);
        assert_eq!(cw.grid[Line(6)][Column(0)].c, '\u{8}');
        assert_eq!(cw.grid[Line(6)].occ, 1);
        assert_eq!(cw.grid[Line(7)][Column(0)].c, '\u{9}');
        assert_eq!(cw.grid[Line(7)].occ, 1);
        assert_eq!(cw.grid[Line(8)][Column(0)].c, ' '); // was 0.
        assert_eq!(cw.grid[Line(8)].occ, 0);
        assert_eq!(cw.grid[Line(9)][Column(0)].c, ' '); // was 1.
        assert_eq!(cw.grid[Line(9)].occ, 0);
    }

    #[test]
    fn test_linefeed() {
        let mut cw: Crosswords<VoidListener> = Crosswords::new(1, 1, VoidListener {});
        assert_eq!(cw.grid.total_lines(), 1);

        cw.linefeed();
        assert_eq!(cw.grid.total_lines(), 2);
    }

    #[test]
    fn test_linefeed_moving_cursor() {
        let mut cw: Crosswords<VoidListener> = Crosswords::new(1, 3, VoidListener {});
        let (col, row) = cw.cursor();
        assert_eq!(col, 0);
        assert_eq!(row, 0);

        cw.linefeed();
        let (col, row) = cw.cursor();
        assert_eq!(col, 0);
        assert_eq!(row, 1);

        // Keep adding lines but keep cursor at max row
        for _ in 0..20 {
            cw.linefeed();
        }
        let (col, row) = cw.cursor();
        assert_eq!(col, 0);
        assert_eq!(row, 2);
        assert_eq!(cw.grid.total_lines(), 22);
    }

    #[test]
    fn test_input() {
        let columns: usize = 5;
        let rows: usize = 10;
        let mut cw: Crosswords<VoidListener> =
            Crosswords::new(columns, rows, VoidListener {});
        for i in 0..4 {
            cw.grid[Line(0)][Column(i)].c = i as u8 as char;
        }
        cw.grid[Line(1)][Column(3)].c = 'b';

        assert_eq!(cw.grid[Line(0)][Column(0)].c, '\u{0}');
        assert_eq!(cw.grid[Line(0)][Column(1)].c, '\u{1}');
        assert_eq!(cw.grid[Line(0)][Column(2)].c, '\u{2}');
        assert_eq!(cw.grid[Line(0)][Column(3)].c, '\u{3}');
        assert_eq!(cw.grid[Line(0)][Column(4)].c, ' ');
        assert_eq!(cw.grid[Line(1)][Column(2)].c, ' ');
        assert_eq!(cw.grid[Line(1)][Column(3)].c, 'b');
        assert_eq!(cw.grid[Line(0)][Column(4)].c, ' ');
    }
}
