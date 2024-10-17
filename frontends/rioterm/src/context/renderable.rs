use rio_backend::crosswords::grid::row::Row;
use rio_backend::crosswords::pos::CursorState;
use rio_backend::crosswords::square::Square;
use rio_backend::selection::SelectionRange;
use std::collections::HashSet;

#[derive(Default, Clone, Debug)]
pub enum RenderableContentStrategy {
    #[allow(unused)]
    Noop,
    #[default]
    Full,
    #[allow(unused)]
    Lines(HashSet<usize>),
}

#[derive(Default, Clone)]
pub struct Cursor {
    pub state: CursorState,
    pub content: char,
    pub content_ref: char,
    pub is_ime_enabled: bool,
}

#[derive(Default)]
pub struct RenderableContent {
    pub inner: Vec<Row<Square>>,
    pub display_offset: i32,
    // TODO: Should not use default
    pub cursor: Cursor,
    pub has_blinking_enabled: bool,
    pub strategy: RenderableContentStrategy,
    pub selection_range: Option<SelectionRange>,
    pub hyperlink_range: Option<SelectionRange>,
    pub has_pending_updates: bool,
}

impl RenderableContent {
    #[inline]
    pub fn update(
        &mut self,
        rows: Vec<Row<Square>>,
        display_offset: usize,
        cursor: CursorState,
        has_blinking_enabled: bool,
    ) {
        let mut diff: HashSet<usize> = HashSet::with_capacity(rows.len());

        if self.cursor.state.pos != cursor.pos {
            // Add old row cursor
            diff.insert(*self.cursor.state.pos.row as usize);
            // Add new row cursor
            diff.insert(*cursor.pos.row as usize);
        }
        self.cursor.state = cursor;
        self.strategy = RenderableContentStrategy::Full;

        let require_full_clone = self.display_offset != display_offset as i32
            || self.has_blinking_enabled != has_blinking_enabled
            || self.has_pending_updates
            || self.inner.len() != rows.len()
            || self.selection_range.is_some()
            || self.hyperlink_range.is_some();

        self.has_pending_updates = false;

        self.display_offset = display_offset as i32;
        self.has_blinking_enabled = has_blinking_enabled;

        // if let Some(selection_range) = self.selection_range {
        // println!("{:?}", self.selection_range);
        // for i in (*selection_range.start.row as usize)..(*selection_range.end.row as usize) {
        //     println!("{:?}", i);
        //     diff.insert(i);
        // }
        // }

        // if let Some(hyperlink_range) = self.hyperlink_range {
        // for i in (*hyperlink_range.start.row as usize)..(*hyperlink_range.end.row as usize) {
        //     diff.insert(i);
        // }
        // }

        if require_full_clone {
            self.inner = rows.clone();
            return;
        }

        for current_idx in 0..(rows.len()) {
            if rows[current_idx] != self.inner[current_idx] {
                self.inner[current_idx] = rows[current_idx].clone();
                diff.insert(current_idx);
            }
        }

        if !diff.is_empty() {
            self.strategy = RenderableContentStrategy::Lines(diff);
        } else {
            self.strategy = RenderableContentStrategy::Noop;
        }
    }
}
