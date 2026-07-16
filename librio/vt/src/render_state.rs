use crate::{Listener, Surface};
use rio_backend::crosswords::grid::row::Row;
use rio_backend::crosswords::grid::Dimensions;
use rio_backend::crosswords::pos::Column;
use rio_backend::crosswords::square::{Extras, Square};
use rio_backend::crosswords::style::Style;
use rio_backend::crosswords::Crosswords;
use rio_backend::event::sync::FairMutex;
use rio_backend::event::TerminalDamage;
use rustc_hash::FxHashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewportSelection {
    pub start_line: u16,
    pub start_col: u16,
    pub end_line: u16,
    pub end_col: u16,
    pub is_block: bool,
}

pub struct RenderState {
    terminal: Arc<FairMutex<Crosswords<Listener>>>,
    rows: Vec<Row<Square>>,
    styles: Vec<Style>,
    extras: FxHashMap<u16, Extras>,
    columns: usize,
    cursor_line: usize,
    cursor_column: usize,
    display_offset: usize,
    selection: Option<ViewportSelection>,
}

impl RenderState {
    pub fn new(surface: &Surface) -> Self {
        let terminal = surface.terminal();
        let columns = {
            let term = terminal.lock();
            term.grid.columns()
        };
        Self {
            terminal,
            rows: Vec::new(),
            styles: Vec::new(),
            extras: FxHashMap::default(),
            columns,
            cursor_line: 0,
            cursor_column: 0,
            display_offset: 0,
            selection: None,
        }
    }

    pub fn update(&mut self) {
        let mut term = self.terminal.lock();
        let damage = if self.rows.is_empty() {
            TerminalDamage::Full
        } else {
            match term.peek_damage_event() {
                Some(damage) => damage,
                None => TerminalDamage::Full,
            }
        };
        term.snapshot_visible(
            &damage,
            self.columns,
            &mut self.rows,
            &mut self.styles,
            &mut self.extras,
        );
        term.reset_damage();
        term.damage_event_in_flight = false;
        self.columns = term.grid.columns();
        self.display_offset = term.display_offset();
        let cursor = term.cursor();
        self.cursor_line = cursor.pos.row.0.max(0) as usize;
        self.cursor_column = cursor.pos.col.0;
        self.selection = term
            .selection
            .as_ref()
            .and_then(|selection| selection.to_range(&term))
            .and_then(|range| {
                let offset = term.display_offset() as i32;
                let lines = self.rows.len() as i32;
                let start = range.start.row.0 + offset;
                let end = range.end.row.0 + offset;
                if end < 0 || start >= lines {
                    return None;
                }
                let clamped_start = start.max(0);
                let clamped_end = end.min(lines - 1);
                Some(ViewportSelection {
                    start_line: clamped_start as u16,
                    start_col: if start < 0 {
                        0
                    } else {
                        range.start.col.0 as u16
                    },
                    end_line: clamped_end as u16,
                    end_col: if end >= lines {
                        (self.columns.saturating_sub(1)) as u16
                    } else {
                        range.end.col.0 as u16
                    },
                    is_block: range.is_block,
                })
            });
    }

    pub fn columns(&self) -> usize {
        self.columns
    }

    pub fn lines(&self) -> usize {
        self.rows.len()
    }

    pub fn row_dirty(&self, line: usize) -> bool {
        self.rows.get(line).map(|row| row.dirty).unwrap_or(false)
    }

    pub fn reset_dirty(&mut self) {
        for row in &mut self.rows {
            row.dirty = false;
        }
    }

    pub fn square(&self, line: usize, column: usize) -> Option<&Square> {
        let row = self.rows.get(line)?;
        if column >= self.columns {
            return None;
        }
        Some(&row[Column(column)])
    }

    pub fn style_of(&self, square: &Square) -> Style {
        self.styles
            .get(square.style_id() as usize)
            .copied()
            .unwrap_or_default()
    }

    pub fn styles(&self) -> &[Style] {
        &self.styles
    }

    pub fn cursor(&self) -> (usize, usize) {
        (self.cursor_line, self.cursor_column)
    }

    pub fn display_offset(&self) -> usize {
        self.display_offset
    }

    pub fn selection(&self) -> Option<ViewportSelection> {
        self.selection
    }

    pub fn text_row(&self, line: usize) -> String {
        let Some(row) = self.rows.get(line) else {
            return String::new();
        };
        let mut text = String::with_capacity(self.columns);
        for column in 0..self.columns {
            text.push(row[Column(column)].c());
        }
        text.trim_end().to_string()
    }
}
