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

use crate::ansi::{
    mode::Mode as AnsiMode, ClearMode, CursorShape, LineClearMode, TabulationClearMode,
};
use crate::clipboard::ClipboardType;
use crate::crosswords::grid::{BidirectionalIterator, Dimensions, Grid, Scroll};
use crate::event::{EventListener, RioEvent};
use crate::performer::handler::Handler;
use crate::selection::{Selection, SelectionRange, SelectionType};
use attr::*;
use base64::{engine::general_purpose, Engine as _};
use bitflags::bitflags;
use colors::{AnsiColor, ColorRgb, Colors};
use grid::row::Row;
use log::{debug, info, warn};
use pos::{CharsetIndex, Column, Cursor, CursorState, Line, Pos};
use square::{LineLength, Square};
use std::mem;
use std::ops::{Index, IndexMut, Range};
use std::option::Option;
use std::ptr;
use std::sync::Arc;
use unicode_width::UnicodeWidthChar;

pub type NamedColor = colors::NamedColor;

pub const MIN_COLUMNS: usize = 2;
pub const MIN_VISIBLE_ROWS: usize = 1;
const BRACKET_PAIRS: [(char, char); 4] = [('(', ')'), ('[', ']'), ('{', '}'), ('<', '>')];

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
    #[allow(dead_code)]
    pub fn reset(&mut self, num_cols: usize) {
        *self = Self::undamaged(num_cols, self.line);
    }

    #[inline]
    pub fn expand(&mut self, left: usize, right: usize) {
        self.left = std::cmp::min(self.left, left);
        self.right = std::cmp::max(self.right, right);
    }

    #[inline]
    #[allow(dead_code)]
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
    last_selection: Option<SelectionRange>,
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
            last_selection: Default::default(),
        }
    }

    #[inline]
    fn resize(&mut self, num_cols: usize, num_lines: usize) {
        // Reset point, so old cursor won't end up outside of the viewport.
        self.last_cursor = Default::default();
        self.last_vi_cursor_point = None;
        self.last_selection = None;
        self.is_fully_damaged = true;

        self.lines.clear();
        self.lines.reserve(num_lines);
        for line in 0..num_lines {
            self.lines.push(LineDamageBounds::undamaged(num_cols, line));
        }
    }

    /// Damage point inside of the viewport.
    #[inline]
    fn damage_point(&mut self, pos: Pos) {
        self.damage_line(pos.row.0 as usize, pos.col.0, pos.col.0);
    }

    /// Expand `line`'s damage to span at least `left` to `right` column.
    #[inline]
    fn damage_line(&mut self, line: usize, left: usize, right: usize) {
        self.lines[line].expand(left, right);
    }

    #[allow(dead_code)]
    fn damage_selection(
        &mut self,
        selection: SelectionRange,
        display_offset: usize,
        num_cols: usize,
    ) {
        let display_offset = display_offset as i32;
        let last_visible_line = self.lines.len() as i32 - 1;

        // Don't damage invisible selection.
        if selection.end.row.0 + display_offset < 0
            || selection.start.row.0.abs() < display_offset - last_visible_line
        {
            return;
        };

        let start = std::cmp::max(selection.start.row.0 + display_offset, 0);
        let end = (selection.end.row.0 + display_offset).clamp(0, last_visible_line);
        for line in start as usize..=end as usize {
            self.damage_line(line, 0, num_cols - 1);
        }
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
    fn clear_all(&mut self) {
        unsafe {
            ptr::write_bytes(self.tabs.as_mut_ptr(), 0, self.tabs.len());
        }
    }

    /// Increase tabstop capacity.
    #[inline]
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

#[derive(Debug, Clone)]
pub struct Crosswords<U>
where
    U: EventListener,
{
    active_charset: CharsetIndex,
    mode: Mode,
    #[allow(unused)]
    semantic_escape_chars: String,
    pub grid: Grid<Square>,
    inactive_grid: Grid<Square>,
    scroll_region: Range<Line>,
    tabs: TabStops,
    event_proxy: U,
    pub selection: Option<Selection>,
    #[allow(dead_code)]
    colors: Colors,
    title: Option<String>,
    damage: TermDamageState,
    pub vi_mode_cursor: Pos,
}

impl<U: EventListener> Crosswords<U> {
    pub fn new(cols: usize, rows: usize, event_proxy: U) -> Crosswords<U> {
        let grid = Grid::new(rows, cols, 10_000);
        let alt = Grid::new(rows, cols, 0);

        let scroll_region = Line(0)..Line(rows as i32);
        let semantic_escape_chars = String::from(",│`|:\"' ()[]{}<>\t");

        Crosswords {
            vi_mode_cursor: Pos::default(),
            semantic_escape_chars,
            selection: None,
            grid,
            inactive_grid: alt,
            active_charset: CharsetIndex::default(),
            scroll_region,
            event_proxy,
            colors: Colors::default(),
            title: None,
            tabs: TabStops::new(cols),
            mode: Mode::SHOW_CURSOR
                | Mode::LINE_WRAP
                | Mode::ALTERNATE_SCROLL
                | Mode::URGENCY_HINTS,
            damage: TermDamageState::new(cols, rows),
        }
    }

    pub fn mark_fully_damaged(&mut self) {
        self.damage.is_fully_damaged = true;
    }

    #[allow(dead_code)]
    pub fn reset_damage(&mut self) {
        self.damage.reset(self.grid.columns());
    }

    pub fn display_offset(&mut self) -> usize {
        self.grid.display_offset()
    }

    pub fn scroll_display(&mut self, scroll: Scroll) {
        let old_display_offset = self.grid.display_offset();
        self.grid.scroll_display(scroll);
        self.event_proxy.send_event(RioEvent::MouseCursorDirty);

        // Clamp vi mode cursor to the viewport.
        let viewport_start = -(self.grid.display_offset() as i32);
        let viewport_end = viewport_start + self.grid.bottommost_line().0;
        let vi_cursor_line = &mut self.vi_mode_cursor.row.0;
        *vi_cursor_line =
            std::cmp::min(viewport_end, std::cmp::max(viewport_start, *vi_cursor_line));
        // self.vi_mode_recompute_selection();

        // Damage everything if display offset changed.
        if old_display_offset != self.grid.display_offset() {
            self.mark_fully_damaged();
        }
    }

    pub fn bottommost_line(&self) -> Line {
        self.grid.bottommost_line()
    }

    pub fn resize<S: Dimensions>(&mut self, num_cols: usize, num_lines: usize) {
        let old_cols = self.grid.columns();
        let old_lines = self.grid.screen_lines();

        if old_cols == num_cols && old_lines == num_lines {
            info!("Crosswords::resize dimensions unchanged");
            return;
        }
        // Move vi mode cursor with the content.
        let history_size = self.history_size();
        let mut delta = num_lines as i32 - old_lines as i32;
        let min_delta =
            std::cmp::min(0, num_lines as i32 - self.grid.cursor.pos.row.0 - 1);

        delta = std::cmp::min(std::cmp::max(delta, min_delta), history_size as i32);
        self.vi_mode_cursor.row += delta;

        let is_alt = self.mode.contains(Mode::ALT_SCREEN);
        self.grid.resize(!is_alt, num_lines, num_cols);
        self.inactive_grid.resize(is_alt, num_lines, num_cols);

        // Invalidate selection and tabs only when necessary.
        if old_cols != num_cols {
            self.selection = None;

            // Recreate tabs list.
            self.tabs.resize(num_cols);
        }

        // else if let Some(selection) = self.selection.take() {
        //     let max_lines = std::cmp::max(num_lines, old_lines) as i32;
        //     let range = Line(0)..Line(max_lines);
        //     self.selection = selection.rotate(self, &range, -delta);
        // }

        // Clamp vi cursor to viewport.
        let vi_pos = self.vi_mode_cursor;
        let viewport_top = Line(-(self.grid.display_offset() as i32));
        let viewport_bottom = viewport_top + self.bottommost_line();
        self.vi_mode_cursor.row =
            std::cmp::max(std::cmp::min(vi_pos.row, viewport_bottom), viewport_top);
        self.vi_mode_cursor.col = std::cmp::min(vi_pos.col, self.grid.last_column());

        // Reset scrolling region.
        self.scroll_region = Line(0)..Line(self.grid.screen_lines() as i32);

        // Resize damage information.
        self.damage.resize(num_cols, num_lines);
    }

    #[inline]
    #[allow(dead_code)]
    fn dynamic_color_sequence(
        &mut self,
        prefix: String,
        index: usize,
        _terminator: &str,
    ) {
        warn!(
            "Requested write of escape sequence for color code {}: color[{}]",
            prefix, index
        );
    }

    /// Toggle the vi mode.
    #[inline]
    #[allow(dead_code)]
    pub fn toggle_vi_mode(&mut self)
    where
        U: EventListener,
    {
        self.mode ^= Mode::VI;

        if self.mode.contains(Mode::VI) {
            let display_offset = self.grid.display_offset() as i32;
            if self.grid.cursor.pos.row > self.grid.bottommost_line() - display_offset {
                // Move cursor to top-left if terminal cursor is not visible.
                let point = Pos::new(Line(-display_offset), Column(0));
                self.vi_mode_cursor = point;
            } else {
                // Reset vi mode cursor position to match primary cursor.
                self.vi_mode_cursor = self.grid.cursor.pos;
            }
        }

        // Update UI about cursor blinking state changes.
        self.event_proxy.send_event(RioEvent::CursorBlinkingChange);
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
            self.damage_cursor();
            self.grid.cursor.pos.row += 1;
        }

        self.grid.cursor.pos.col = Column(0);
        self.grid.cursor.should_wrap = false;
        self.damage_cursor();
    }

    pub fn history_size(&self) -> usize {
        self.grid
            .total_lines()
            .saturating_sub(self.grid.screen_lines())
    }

    #[inline]
    fn damage_cursor(&mut self) {
        // The normal cursor coordinates are always in viewport.
        let point = Pos::new(Line(self.grid.cursor.pos.row.0), self.grid.cursor.pos.col);
        self.damage.damage_point(point);
    }

    #[inline]
    fn scroll_down_relative(&mut self, origin: Line, mut lines: usize) {
        debug!(
            "Scrolling down relative: origin={}, lines={}",
            origin, lines
        );

        lines = std::cmp::min(
            lines,
            (self.scroll_region.end - self.scroll_region.start).0 as usize,
        );
        lines = std::cmp::min(lines, (self.scroll_region.end - origin).0 as usize);

        let region = origin..self.scroll_region.end;

        // Scroll selection.
        self.selection = self
            .selection
            .take()
            .and_then(|s| s.rotate(&self.grid, &region, -(lines as i32)));

        // Scroll vi mode cursor.
        let line = &mut self.vi_mode_cursor.row;
        if region.start <= *line && region.end > *line {
            *line = std::cmp::min(*line + lines, region.end - 1);
        }

        // Scroll between origin and bottom
        self.grid.scroll_down(&region, lines);
        self.mark_fully_damaged();
    }

    #[inline]
    pub fn scroll_up_relative(&mut self, origin: Line, mut lines: usize) {
        debug!("Scrolling up: origin={origin}, lines={lines}");

        lines = std::cmp::min(
            lines,
            (self.scroll_region.end - self.scroll_region.start).0 as usize,
        );

        let region = origin..self.scroll_region.end;

        // Scroll selection.
        self.selection = self
            .selection
            .take()
            .and_then(|s| s.rotate(&self.grid, &region, lines as i32));

        self.grid.scroll_up(&region, lines);

        // Scroll vi mode cursor.
        let viewport_top = Line(-(self.grid.display_offset() as i32));
        let top = if region.start == 0 {
            viewport_top
        } else {
            region.start
        };
        let line = &mut self.vi_mode_cursor.row;
        if (top <= *line) && region.end > *line {
            *line = std::cmp::max(*line - lines, top);
        }
        self.mark_fully_damaged();
    }

    pub fn bracket_search(&self, point: Pos) -> Option<Pos> {
        let start_char = self.grid[point].c;

        // Find the matching bracket we're looking for
        let (forward, end_char) = BRACKET_PAIRS.iter().find_map(|(open, close)| {
            if open == &start_char {
                Some((true, *close))
            } else if close == &start_char {
                Some((false, *open))
            } else {
                None
            }
        })?;

        let mut iter = self.grid.iter_from(point);

        // For every character match that equals the starting bracket, we
        // ignore one bracket of the opposite type.
        let mut skip_pairs = 0;

        loop {
            // Check the next cell
            let cell = if forward { iter.next() } else { iter.prev() };

            // Break if there are no more cells
            let cell = match cell {
                Some(cell) => cell,
                None => break,
            };

            // Check if the bracket matches
            if cell.c == end_char && skip_pairs == 0 {
                return Some(cell.pos);
            } else if cell.c == start_char {
                skip_pairs += 1;
            } else if cell.c == end_char {
                skip_pairs -= 1;
            }
        }

        None
    }

    pub fn semantic_search_left(&self, mut point: Pos) -> Pos {
        // Limit the starting point to the last line in the history
        point.row = std::cmp::max(point.row, self.grid.topmost_line());

        let mut iter = self.grid.iter_from(point);
        let last_column = self.grid.columns() - 1;

        let wide = square::Flags::WIDE_CHAR
            | square::Flags::WIDE_CHAR_SPACER
            | square::Flags::LEADING_WIDE_CHAR_SPACER;
        while let Some(cell) = iter.prev() {
            if !cell.flags.intersects(wide) && self.semantic_escape_chars.contains(cell.c)
            {
                break;
            }

            if cell.pos.col == last_column
                && !cell.flags.contains(square::Flags::WRAPLINE)
            {
                break; // cut off if on new line or hit escape char
            }

            point = cell.pos;
        }

        point
    }

    pub fn semantic_search_right(&self, mut point: Pos) -> Pos {
        // Limit the starting point to the last line in the history
        point.row = std::cmp::max(point.row, self.grid.topmost_line());

        let wide = square::Flags::WIDE_CHAR
            | square::Flags::WIDE_CHAR_SPACER
            | square::Flags::LEADING_WIDE_CHAR_SPACER;
        let last_column = self.grid.columns() - 1;

        for cell in self.grid.iter_from(point) {
            if !cell.flags.intersects(wide) && self.semantic_escape_chars.contains(cell.c)
            {
                break;
            }

            point = cell.pos;

            if point.col == last_column && !cell.flags.contains(square::Flags::WRAPLINE) {
                break; // cut off if on new line or hit escape char
            }
        }

        point
    }

    pub fn write_at_cursor(&mut self, c: char) {
        let c = self.grid.cursor.charsets[self.active_charset].map(c);
        let fg = self.grid.cursor.template.fg;
        let bg = self.grid.cursor.template.bg;
        let flags = self.grid.cursor.template.flags;
        let extra = self.grid.cursor.template.extra.clone();

        let mut cursor_square = self.grid.cursor_square();
        cursor_square.c = c;
        cursor_square.fg = fg;
        cursor_square.bg = bg;
        cursor_square.flags = flags;
        cursor_square.extra = extra;
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
        let mut start = self.scroll_region.start.0;
        let mut end = self.scroll_region.end.0;

        let scroll = self.display_offset() as i32;
        if scroll != 0 {
            start -= scroll;
            end -= scroll;
        }

        for row in start..end {
            visible_rows.push(self.grid[Line(row)].to_owned());
        }

        visible_rows
    }

    fn deccolm(&mut self)
    where
        U: EventListener,
    {
        // Setting 132 column font makes no sense, but run the other side effects.
        // Clear scrolling region.
        self.set_scrolling_region(1, None);

        // Clear grid.
        self.grid.reset_region(..);
        self.mark_fully_damaged();
    }

    pub fn mode(&self) -> Mode {
        self.mode.clone()
    }

    #[inline]
    pub fn cursor(&mut self) -> CursorState {
        let vi_mode = self.mode().contains(Mode::VI);
        let mut pos = if vi_mode {
            self.vi_mode_cursor
        } else {
            self.grid.cursor.pos
        };
        if self.grid[pos]
            .flags
            .contains(square::Flags::WIDE_CHAR_SPACER)
        {
            pos.col -= 1;
        }
        let mut content = CursorShape::Block;

        // // Cursor shape.
        if !vi_mode && !self.mode().contains(Mode::SHOW_CURSOR) {
            content = CursorShape::Hidden;
        }

        CursorState { pos, content }
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
        self.selection = None;
        self.mark_fully_damaged();
    }

    pub fn selection_to_string(&self) -> Option<String> {
        let selection_range = self.selection.as_ref().and_then(|s| s.to_range(self))?;
        let SelectionRange { start, end, .. } = selection_range;

        let mut res = String::new();

        match self.selection.as_ref() {
            Some(Selection {
                ty: SelectionType::Block,
                ..
            }) => {
                for line in (start.row.0..end.row.0).map(Line::from) {
                    res += self
                        .line_to_string(line, start.col..end.col, start.col.0 != 0)
                        .trim_end();
                    res += "\n";
                }

                res += self
                    .line_to_string(end.row, start.col..end.col, true)
                    .trim_end();
            }
            Some(Selection {
                ty: SelectionType::Lines,
                ..
            }) => {
                res = self.bounds_to_string(start, end) + "\n";
            }
            _ => {
                res = self.bounds_to_string(start, end);
            }
        }

        Some(res)
    }

    pub fn bounds_to_string(&self, start: Pos, end: Pos) -> String {
        let mut res = String::new();

        for line in (start.row.0..=end.row.0).map(Line::from) {
            let start_col = if line == start.row {
                start.col
            } else {
                Column(0)
            };
            let end_col = if line == end.row {
                end.col
            } else {
                self.grid.last_column()
            };

            res += &self.line_to_string(line, start_col..end_col, line == end.row);
        }

        res.strip_suffix('\n').map(str::to_owned).unwrap_or(res)
    }

    /// Convert a single line in the grid to a String.
    fn line_to_string(
        &self,
        line: Line,
        mut cols: Range<Column>,
        include_wrapped_wide: bool,
    ) -> String {
        let mut text = String::new();

        let grid_line = &self.grid[line];
        let line_length = std::cmp::min(grid_line.line_length(), cols.end + 1);

        // Include wide char when trailing spacer is selected.
        if grid_line[cols.start]
            .flags
            .contains(square::Flags::WIDE_CHAR_SPACER)
        {
            cols.start -= 1;
        }

        let mut tab_mode = false;
        for column in (cols.start.0..line_length.0).map(Column::from) {
            let cell = &grid_line[column];

            // Skip over cells until next tab-stop once a tab was found.
            if tab_mode {
                if self.tabs[column] || cell.c != ' ' {
                    tab_mode = false;
                } else {
                    continue;
                }
            }

            if cell.c == '\t' {
                tab_mode = true;
            }

            if !cell.flags.intersects(
                square::Flags::WIDE_CHAR_SPACER | square::Flags::LEADING_WIDE_CHAR_SPACER,
            ) {
                // Push cells primary character.
                text.push(cell.c);

                // Push zero-width characters.
                for c in cell.zerowidth().into_iter().flatten() {
                    text.push(*c);
                }
            }
        }

        if cols.end >= self.grid.columns() - 1
            && (line_length.0 == 0
                || !self.grid[line][line_length - 1]
                    .flags
                    .contains(square::Flags::WRAPLINE))
        {
            text.push('\n');
        }

        // If wide char is not part of the selection, but leading spacer is, include it.
        if line_length == self.grid.columns()
            && line_length.0 >= 2
            && grid_line[line_length - 1]
                .flags
                .contains(square::Flags::LEADING_WIDE_CHAR_SPACER)
            && include_wrapped_wide
        {
            text.push(self.grid[line - 1i32][Column(0)].c);
        }

        text
    }

    /// Find the beginning of the current line across linewraps.
    pub fn row_search_left(&self, mut point: Pos) -> Pos {
        while point.row > self.grid.topmost_line()
            && self.grid[point.row - 1i32][self.grid.last_column()]
                .flags
                .contains(square::Flags::WRAPLINE)
        {
            point.row -= 1;
        }

        point.col = Column(0);

        point
    }

    /// Find the end of the current line across linewraps.
    pub fn row_search_right(&self, mut point: Pos) -> Pos {
        while point.row + 1 < self.grid.screen_lines()
            && self.grid[point.row][self.grid.last_column()]
                .flags
                .contains(square::Flags::WRAPLINE)
        {
            point.row += 1;
        }

        point.col = self.grid.last_column();

        point
    }
}

impl<U: EventListener> Handler for Crosswords<U> {
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
            AnsiMode::ReportSquareMouseMotion => {
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
            AnsiMode::Column => self.deccolm(),
            AnsiMode::Insert => self.mode.insert(Mode::INSERT),
            AnsiMode::BlinkingCursor => {
                // let style = self.grid.cursor_style.get_or_insert(self.default_cursor_style);
                // style.blinking = true;
                // self.event_proxy.send_event(Event::CursorBlinkingChange);
            }
        }
    }

    #[inline]
    fn unset_mode(&mut self, mode: AnsiMode) {
        match mode {
            AnsiMode::UrgencyHints => self.mode.remove(Mode::URGENCY_HINTS),
            AnsiMode::SwapScreenAndSetRestoreCursor => {
                if self.mode.contains(Mode::ALT_SCREEN) {
                    self.swap_alt();
                }
            }
            AnsiMode::ShowCursor => self.mode.remove(Mode::SHOW_CURSOR),
            AnsiMode::CursorKeys => self.mode.remove(Mode::APP_CURSOR),
            AnsiMode::ReportMouseClicks => {
                self.mode.remove(Mode::MOUSE_REPORT_CLICK);
                // self.event_proxy.send_event(RioEvent::MouseCursorDirty);
            }
            AnsiMode::ReportSquareMouseMotion => {
                self.mode.remove(Mode::MOUSE_DRAG);
                // self.event_proxy.send_event(Event::MouseCursorDirty);
            }
            AnsiMode::ReportAllMouseMotion => {
                self.mode.remove(Mode::MOUSE_MOTION);
                // self.event_proxy.send_event(Event::MouseCursorDirty);
            }
            AnsiMode::ReportFocusInOut => self.mode.remove(Mode::FOCUS_IN_OUT),
            AnsiMode::BracketedPaste => self.mode.remove(Mode::BRACKETED_PASTE),
            AnsiMode::SgrMouse => self.mode.remove(Mode::SGR_MOUSE),
            AnsiMode::Utf8Mouse => self.mode.remove(Mode::UTF8_MOUSE),
            AnsiMode::AlternateScroll => self.mode.remove(Mode::ALTERNATE_SCROLL),
            AnsiMode::LineWrap => self.mode.remove(Mode::LINE_WRAP),
            AnsiMode::LineFeedNewLine => self.mode.remove(Mode::LINE_FEED_NEW_LINE),
            AnsiMode::Origin => self.mode.remove(Mode::ORIGIN),
            AnsiMode::Column => self.deccolm(),
            AnsiMode::Insert => {
                self.mode.remove(Mode::INSERT);
                self.mark_fully_damaged();
            }
            AnsiMode::BlinkingCursor => {
                // let style = self.cursor_style.get_or_insert(self.default_cursor_style);
                // style.blinking = false;
                // self.event_proxy.send_event(Event::CursorBlinkingChange);
            }
        }
    }

    #[inline]
    fn goto(&mut self, line: Line, col: Column) {
        let (y_offset, max_y) = if self.mode.contains(Mode::ORIGIN) {
            (self.scroll_region.start, self.scroll_region.end - 1)
        } else {
            (Line(0), self.grid.bottommost_line())
        };

        self.damage_cursor();
        self.grid.cursor.pos.row =
            std::cmp::max(std::cmp::min(line + y_offset, max_y), Line(0));
        self.grid.cursor.pos.col = std::cmp::min(col, self.grid.last_column());
        self.damage_cursor();
        self.grid.cursor.should_wrap = false;
    }

    #[inline]
    fn set_active_charset(&mut self, index: CharsetIndex) {
        self.active_charset = index;
    }

    #[inline]
    fn move_forward(&mut self, cols: Column) {
        let last_column =
            std::cmp::min(self.grid.cursor.pos.col + cols, self.grid.last_column());

        let cursor_line = self.grid.cursor.pos.row.0 as usize;
        self.damage
            .damage_line(cursor_line, self.grid.cursor.pos.col.0, last_column.0);

        self.grid.cursor.pos.col = last_column;
        self.grid.cursor.should_wrap = false;
    }

    #[inline]
    fn move_backward(&mut self, cols: Column) {
        let column = self.grid.cursor.pos.col.saturating_sub(cols.0);

        let cursor_line = self.grid.cursor.pos.row.0 as usize;
        self.damage
            .damage_line(cursor_line, column, self.grid.cursor.pos.col.0);

        self.grid.cursor.pos.col = Column(column);
        self.grid.cursor.should_wrap = false;
    }

    #[inline]
    fn move_backward_tabs(&mut self, count: u16) {
        self.damage_cursor();

        let old_col = self.grid.cursor.pos.col.0;
        for _ in 0..count {
            let mut col = self.grid.cursor.pos.col;
            for i in (0..(col.0)).rev() {
                if self.tabs[Column(i)] {
                    col = Column(i);
                    break;
                }
            }
            self.grid.cursor.pos.col = col;
        }

        let line = self.grid.cursor.pos.row.0 as usize;
        self.damage
            .damage_line(line, self.grid.cursor.pos.col.0, old_col);
    }

    #[inline]
    fn goto_line(&mut self, line: Line) {
        self.goto(line, self.grid.cursor.pos.col)
    }

    #[inline]
    fn goto_col(&mut self, col: Column) {
        self.goto(self.grid.cursor.pos.row, col)
    }

    #[inline]
    fn decaln(&mut self) {
        for line in (0..self.grid.screen_lines()).map(Line::from) {
            for column in 0..self.grid.columns() {
                let cell = &mut self.grid[line][Column(column)];
                *cell = Square::default();
                cell.c = 'E';
            }
        }

        self.mark_fully_damaged();
    }

    #[inline]
    fn move_up(&mut self, rows: usize) {
        self.goto(self.grid.cursor.pos.row - rows, self.grid.cursor.pos.col)
    }

    #[inline]
    fn move_down(&mut self, rows: usize) {
        self.goto(self.grid.cursor.pos.row + rows, self.grid.cursor.pos.col)
    }

    #[inline]
    fn move_down_and_cr(&mut self, rows: usize) {
        self.goto(self.grid.cursor.pos.row + rows, Column(0))
    }

    #[inline]
    fn move_up_and_cr(&mut self, lines: usize) {
        self.goto(self.grid.cursor.pos.row - lines, Column(0))
    }

    #[inline]
    fn scroll_up(&mut self, lines: usize) {
        let origin = self.scroll_region.start;
        self.scroll_up_relative(origin, lines);
    }

    #[inline]
    fn delete_lines(&mut self, lines: usize) {
        let origin = self.grid.cursor.pos.row;
        let lines = std::cmp::min(self.grid.screen_lines() - origin.0 as usize, lines);

        if lines > 0 && self.scroll_region.contains(&origin) {
            self.scroll_up_relative(origin, lines);
        }
    }

    #[inline]
    fn erase_chars(&mut self, count: Column) {
        let cursor = &self.grid.cursor;

        let start = cursor.pos.col;
        let end = std::cmp::min(start + count, Column(self.grid.columns()));

        // Cleared cells have current background color set.
        let bg = self.grid.cursor.template.bg;
        let line = cursor.pos.row;
        self.damage.damage_line(line.0 as usize, start.0, end.0);
        let row = &mut self.grid[line];
        for cell in &mut row[start..end] {
            *cell = bg.into();
        }
    }

    #[inline]
    fn delete_chars(&mut self, count: usize) {
        let columns = self.grid.columns();
        let cursor = &self.grid.cursor;
        let bg = cursor.template.bg;

        // Ensure deleting within terminal bounds.
        let count = std::cmp::min(count, columns);

        let start = cursor.pos.col.0;
        let end = std::cmp::min(start + count, columns - 1);
        let num_cells = columns - end;

        let line = cursor.pos.row;
        self.damage
            .damage_line(line.0 as usize, 0, self.grid.columns() - 1);
        let row = &mut self.grid[line][..];

        for offset in 0..num_cells {
            row.swap(start + offset, end + offset);
        }

        // Clear last `count` cells in the row. If deleting 1 char, need to delete
        // 1 cell.
        let end = columns - count;
        for cell in &mut row[end..] {
            *cell = bg.into();
        }
    }

    #[inline]
    fn scroll_down(&mut self, lines: usize) {
        let origin = self.scroll_region.start;
        self.scroll_down_relative(origin, lines);
    }

    #[inline]
    fn insert_blank_lines(&mut self, lines: usize) {
        let origin = self.grid.cursor.pos.row;
        if self.scroll_region.contains(&origin) {
            self.scroll_down_relative(origin, lines);
        }
    }

    #[inline]
    fn insert_blank(&mut self, count: usize) {
        let cursor = &self.grid.cursor;
        let bg = cursor.template.bg;

        // Ensure inserting within terminal bounds
        let count = std::cmp::min(count, self.grid.columns() - cursor.pos.col.0);

        let source = cursor.pos.col;
        let destination = cursor.pos.col.0 + count;
        let num_cells = self.grid.columns() - destination;

        let line = cursor.pos.row;
        self.damage
            .damage_line(line.0 as usize, 0, self.grid.columns() - 1);

        let row = &mut self.grid[line][..];

        for offset in (0..num_cells).rev() {
            row.swap(destination + offset, source.0 + offset);
        }

        // Squares were just moved out toward the end of the line;
        // fill in between source and dest with blanks.
        for cell in &mut row[source.0..destination] {
            *cell = bg.into();
        }
    }

    #[inline]
    fn reverse_index(&mut self) {
        // If cursor is at the top.
        if self.grid.cursor.pos.row == self.scroll_region.start {
            self.scroll_down(1);
        } else {
            self.damage_cursor();
            self.grid.cursor.pos.row =
                std::cmp::max(self.grid.cursor.pos.row - 1, Line(0));
            self.damage_cursor();
        }
    }

    #[inline]
    fn terminal_attribute(&mut self, attr: Attr) {
        let cursor = &mut self.grid.cursor;
        // println!("{:?}", attr);
        match attr {
            Attr::Foreground(color) => cursor.template.fg = color,
            Attr::Background(color) => cursor.template.bg = color,
            Attr::UnderlineColor(color) => cursor.template.set_underline_color(color),
            Attr::Reset => {
                cursor.template.fg = AnsiColor::Named(NamedColor::Foreground);
                cursor.template.bg = AnsiColor::Named(NamedColor::Background);
                cursor.template.flags = square::Flags::empty();
                cursor.template.set_underline_color(None);
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
            Attr::Underline => {
                cursor.template.flags.remove(square::Flags::ALL_UNDERLINES);
                cursor.template.flags.insert(square::Flags::UNDERLINE);
            }
            Attr::DoubleUnderline => {
                cursor.template.flags.remove(square::Flags::ALL_UNDERLINES);
                cursor
                    .template
                    .flags
                    .insert(square::Flags::DOUBLE_UNDERLINE);
            }
            Attr::Undercurl => {
                cursor.template.flags.remove(square::Flags::ALL_UNDERLINES);
                cursor.template.flags.insert(square::Flags::UNDERCURL);
            }
            Attr::DottedUnderline => {
                cursor.template.flags.remove(square::Flags::ALL_UNDERLINES);
                cursor
                    .template
                    .flags
                    .insert(square::Flags::DOTTED_UNDERLINE);
            }
            Attr::DashedUnderline => {
                cursor.template.flags.remove(square::Flags::ALL_UNDERLINES);
                cursor
                    .template
                    .flags
                    .insert(square::Flags::DASHED_UNDERLINE);
            }
            Attr::CancelUnderline => {
                cursor.template.flags.remove(square::Flags::ALL_UNDERLINES)
            }
            Attr::Hidden => cursor.template.flags.insert(square::Flags::HIDDEN),
            Attr::CancelHidden => cursor.template.flags.remove(square::Flags::HIDDEN),
            Attr::Strike => cursor.template.flags.insert(square::Flags::STRIKEOUT),
            Attr::CancelStrike => cursor.template.flags.remove(square::Flags::STRIKEOUT),
            _ => {
                warn!("Term got unhandled attr: {:?}", attr);
            }
        }
    }

    fn set_title(&mut self, title: Option<String>) {
        self.title = title;

        let _title: String = match &self.title {
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
            let line = self.grid.cursor.pos.row.0 as usize;
            let column = self.grid.cursor.pos.col.0;
            self.grid.cursor.pos.col -= 1;
            self.grid.cursor.should_wrap = false;
            self.damage.damage_line(line, column - 1, column);
        }
    }

    #[inline]
    fn clear_screen(&mut self, mode: ClearMode) {
        let bg = self.grid.cursor.template.bg;

        let screen_lines = self.grid.screen_lines();

        match mode {
            ClearMode::Above => {
                let cursor = self.grid.cursor.pos;

                // If clearing more than one line.
                if cursor.row > 1 {
                    // Fully clear all lines before the current line.
                    self.grid.reset_region(..cursor.row);
                }

                // Clear up to the current column in the current line.
                let end = std::cmp::min(cursor.col + 1, Column(self.grid.columns()));
                for cell in &mut self.grid[cursor.row][..end] {
                    *cell = bg.into();
                }

                let range = Line(0)..=cursor.row;
                self.selection =
                    self.selection.take().filter(|s| !s.intersects_range(range));
            }
            ClearMode::Below => {
                let cursor = self.grid.cursor.pos;
                for cell in &mut self.grid[cursor.row][cursor.col..] {
                    *cell = bg.into();
                }

                if (cursor.row.0 as usize) < screen_lines - 1 {
                    self.grid.reset_region((cursor.row + 1)..);
                }

                let range = cursor.row..Line(screen_lines as i32);
                self.selection =
                    self.selection.take().filter(|s| !s.intersects_range(range));
            }
            ClearMode::All => {
                if self.mode.contains(Mode::ALT_SCREEN) {
                    self.grid.reset_region(..);
                } else {
                    // let old_offset = self.grid.display_offset();

                    self.grid.clear_viewport();

                    // Compute number of lines scrolled by clearing the viewport.
                    // let lines = self.grid.display_offset().saturating_sub(old_offset);

                    // self.vi_mode_cursor.pos.row =
                    // (self.vi_mode_cursor.pos.row - lines).grid_clamp(self, Boundary::Grid);
                }

                self.selection = None;
            }
            ClearMode::Saved if self.history_size() > 0 => {
                self.grid.clear_history();

                // self.vi_mode_cursor.pos.row =
                // self.vi_mode_cursor.pos.row.grid_clamp(self, Boundary::Cursor);

                self.selection = self
                    .selection
                    .take()
                    .filter(|s| !s.intersects_range(..Line(0)));
            }
            // We have no history to clear.
            ClearMode::Saved => (),
        }

        self.mark_fully_damaged();
    }

    #[inline]
    fn clear_tabs(&mut self, mode: TabulationClearMode) {
        match mode {
            TabulationClearMode::Current => {
                self.tabs[self.grid.cursor.pos.col] = false;
            }
            TabulationClearMode::All => {
                self.tabs.clear_all();
            }
        }
    }

    fn linefeed(&mut self) {
        let next = self.grid.cursor.pos.row + 1;
        if next == self.scroll_region.end {
            self.scroll_up_relative(self.scroll_region.start, 1);
        } else if next < self.grid.screen_lines() {
            self.damage_cursor();
            self.grid.cursor.pos.row += 1;
            self.damage_cursor();
        }
    }

    /// Set the indexed color value.
    #[inline]
    fn set_color(&mut self, _index: usize, _color: ColorRgb) {
        // Damage terminal if the color changed and it's not the cursor.
        // if index != NamedColor::Cursor as usize && self.colors[index] != Some(color) {
        // self.mark_fully_damaged();
        // }

        // self.colors[index] = Some(color);
    }

    // #[inline]
    // fn reset_color(&mut self, index: usize) {
    //     // Damage terminal if the color changed and it's not the cursor.
    //     if index != NamedColor::Cursor as usize && self.colors[index].is_some() {
    //         // self.mark_fully_damaged();
    //     }

    //     self.colors[index] = None;
    // }

    #[inline]
    fn bell(&mut self) {
        warn!("[unimplemented] Bell");
    }

    #[inline]
    fn substitute(&mut self) {
        warn!("[unimplemented] Substitute");
    }

    #[inline]
    fn clipboard_load(&mut self, clipboard: u8, terminator: &str) {
        let clipboard_type = match clipboard {
            b'c' => ClipboardType::Clipboard,
            b'p' | b's' => ClipboardType::Selection,
            _ => return,
        };

        let terminator = terminator.to_owned();

        self.event_proxy.send_event(RioEvent::ClipboardLoad(
            clipboard_type,
            Arc::new(move |text| {
                let base64 = general_purpose::STANDARD.encode(text);
                format!("\x1b]52;{};{}{}", clipboard as char, base64, terminator)
            }),
        ));
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
    fn clear_line(&mut self, mode: LineClearMode) {
        let cursor = &self.grid.cursor;
        let bg = cursor.template.bg;
        let point = cursor.pos;

        let (left, right) = match mode {
            LineClearMode::Right if cursor.should_wrap => return,
            LineClearMode::Right => (point.col, Column(self.grid.columns())),
            LineClearMode::Left => (Column(0), point.col + 1),
            LineClearMode::All => (Column(0), Column(self.grid.columns())),
        };

        self.damage
            .damage_line(point.row.0 as usize, left.0, right.0 - 1);

        let row = &mut self.grid[point.row];
        for cell in &mut row[left..right] {
            *cell = bg.into();
        }

        let range = self.grid.cursor.pos.row..=self.grid.cursor.pos.row;
        self.selection = self.selection.take().filter(|s| !s.intersects_range(range));
    }

    #[inline]
    fn set_scrolling_region(&mut self, top: usize, bottom: Option<usize>) {
        // Fallback to the last line as default.
        let bottom = bottom.unwrap_or_else(|| self.grid.screen_lines());

        if top >= bottom {
            warn!("Invalid scrolling region: ({};{})", top, bottom);
            return;
        }

        // Bottom should be included in the range, but range end is not
        // usually included. One option would be to use an inclusive
        // range, but instead we just let the open range end be 1
        // higher.
        let start = Line(top as i32 - 1);
        let end = Line(bottom as i32);

        info!("Setting scrolling region: ({};{})", start, end);

        let screen_lines = Line(self.grid.screen_lines() as i32);
        self.scroll_region.start = std::cmp::min(start, screen_lines);
        self.scroll_region.end = std::cmp::min(end, screen_lines);
        self.goto(Line(0), Column(0));
    }

    #[inline]
    fn text_area_size_pixels(&mut self) {
        info!("text_area_size_pixels");
        // self.event_proxy.send_event(RioEvent::TextAreaSizeRequest(Arc::new(move |window_size| {
        //     let height = window_size.num_lines * window_size.cell_height;
        //     let width = window_size.num_cols * window_size.cell_width;
        //     format!("\x1b[4;{height};{width}t")
        // })));
    }

    #[inline]
    fn text_area_size_chars(&mut self) {
        let text = format!(
            "\x1b[8;{};{}t",
            self.grid.screen_lines(),
            self.grid.columns()
        );
        info!("text_area_size_chars {:?}", text);
        self.event_proxy.send_event(RioEvent::PtyWrite(text));
    }
}

/// Terminal test helpers.
#[cfg(test)]
pub mod test {
    use super::*;

    pub struct CrosswordsSize {
        pub columns: usize,
        pub screen_lines: usize,
    }

    impl CrosswordsSize {
        pub fn new(columns: usize, screen_lines: usize) -> Self {
            Self {
                columns,
                screen_lines,
            }
        }
    }

    impl Dimensions for CrosswordsSize {
        fn total_lines(&self) -> usize {
            self.screen_lines()
        }

        fn screen_lines(&self) -> usize {
            self.screen_lines
        }

        fn columns(&self) -> usize {
            self.columns
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crosswords::pos::{Column, Line, Pos, Side};
    use crate::crosswords::test::CrosswordsSize;
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
        let cursor = cw.cursor();
        assert_eq!(cursor.pos.col, 0);
        assert_eq!(cursor.pos.row, 0);

        cw.linefeed();
        let cursor = cw.cursor();
        assert_eq!(cursor.pos.col, 0);
        assert_eq!(cursor.pos.row, 1);

        // Keep adding lines but keep cursor at max row
        for _ in 0..20 {
            cw.linefeed();
        }
        let cursor = cw.cursor();
        assert_eq!(cursor.pos.col, 0);
        assert_eq!(cursor.pos.row, 2);
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

    #[test]
    fn simple_selection_works() {
        let size = CrosswordsSize::new(5, 5);
        let mut term = Crosswords::new(size.columns, size.screen_lines, VoidListener {});
        let grid = &mut term.grid;
        for i in 0..4 {
            if i == 1 {
                continue;
            }

            grid[Line(i)][Column(0)].c = '"';

            for j in 1..4 {
                grid[Line(i)][Column(j)].c = 'a';
            }

            grid[Line(i)][Column(4)].c = '"';
        }
        grid[Line(2)][Column(0)].c = ' ';
        grid[Line(2)][Column(4)].c = ' ';
        grid[Line(2)][Column(4)]
            .flags
            .insert(square::Flags::WRAPLINE);
        grid[Line(3)][Column(0)].c = ' ';

        // Multiple lines contain an empty line.
        term.selection = Some(Selection::new(
            SelectionType::Simple,
            Pos {
                row: Line(0),
                col: Column(0),
            },
            Side::Left,
        ));
        if let Some(s) = term.selection.as_mut() {
            s.update(
                Pos {
                    row: Line(2),
                    col: Column(4),
                },
                Side::Right,
            );
        }
        assert_eq!(
            term.selection_to_string(),
            Some(String::from("\"aaa\"\n\n aaa "))
        );

        // A wrapline.
        term.selection = Some(Selection::new(
            SelectionType::Simple,
            Pos {
                row: Line(2),
                col: Column(0),
            },
            Side::Left,
        ));
        if let Some(s) = term.selection.as_mut() {
            s.update(
                Pos {
                    row: Line(3),
                    col: Column(4),
                },
                Side::Right,
            );
        }
        assert_eq!(
            term.selection_to_string(),
            Some(String::from(" aaa  aaa\""))
        );
    }

    #[test]
    fn line_selection_works() {
        let size = CrosswordsSize::new(5, 1);
        let mut term = Crosswords::new(size.columns, size.screen_lines, VoidListener {});
        let mut grid: Grid<Square> = Grid::new(1, 5, 0);
        for i in 0..5 {
            grid[Line(0)][Column(i)].c = 'a';
        }
        grid[Line(0)][Column(0)].c = '"';
        grid[Line(0)][Column(3)].c = '"';

        mem::swap(&mut term.grid, &mut grid);

        term.selection = Some(Selection::new(
            SelectionType::Lines,
            Pos {
                row: Line(0),
                col: Column(3),
            },
            Side::Left,
        ));
        assert_eq!(term.selection_to_string(), Some(String::from("\"aa\"a\n")));
    }

    #[test]
    fn block_selection_works() {
        let size = CrosswordsSize::new(5, 5);
        let mut term = Crosswords::new(size.columns, size.screen_lines, VoidListener {});
        let grid = &mut term.grid;
        for i in 1..4 {
            grid[Line(i)][Column(0)].c = '"';

            for j in 1..4 {
                grid[Line(i)][Column(j)].c = 'a';
            }

            grid[Line(i)][Column(4)].c = '"';
        }
        grid[Line(2)][Column(2)].c = ' ';
        grid[Line(2)][Column(4)]
            .flags
            .insert(square::Flags::WRAPLINE);
        grid[Line(3)][Column(4)].c = ' ';

        term.selection = Some(Selection::new(
            SelectionType::Block,
            Pos {
                row: Line(0),
                col: Column(3),
            },
            Side::Left,
        ));

        // The same column.
        if let Some(s) = term.selection.as_mut() {
            s.update(
                Pos {
                    row: Line(3),
                    col: Column(3),
                },
                Side::Right,
            );
        }
        assert_eq!(term.selection_to_string(), Some(String::from("\na\na\na")));

        // The first column.
        if let Some(s) = term.selection.as_mut() {
            s.update(
                Pos {
                    row: Line(3),
                    col: Column(0),
                },
                Side::Left,
            );
        }
        assert_eq!(
            term.selection_to_string(),
            Some(String::from("\n\"aa\n\"a\n\"aa"))
        );

        // The last column.
        if let Some(s) = term.selection.as_mut() {
            s.update(
                Pos {
                    row: Line(3),
                    col: Column(4),
                },
                Side::Right,
            );
        }
        assert_eq!(
            term.selection_to_string(),
            Some(String::from("\na\"\na\"\na"))
        );
    }
}
