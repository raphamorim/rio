use rio_backend::crosswords::grid::row::Row;
use rio_backend::crosswords::pos::CursorState;
use rio_backend::crosswords::square::Square;
use std::collections::HashSet;

#[derive(Default, Clone, Debug)]
pub enum RenderableContentStrategy {
    Noop,
    #[default]
    Full,
    Lines(HashSet<usize>),
}

#[derive(Default)]
pub struct RenderableContent {
    pub inner: Vec<Row<Square>>,
    pub display_offset: i32,
    // TODO: Should not use default
    pub cursor: CursorState,
    pub has_blinking_enabled: bool,
    pub strategy: RenderableContentStrategy,
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
        self.inner = rows.clone();
        self.strategy = RenderableContentStrategy::Full;
        self.cursor = cursor;
        self.has_blinking_enabled = has_blinking_enabled;
        self.display_offset = display_offset as i32;

        // let mut diff = HashSet::with_capacity(rows.len());
        // if self.cursor.pos != cursor.pos {
        //     // Add old row cursor
        //     diff.insert(*self.cursor.pos.row as usize);
        //     // Add new row cursor
        //     diff.insert(*cursor.pos.row as usize);
        // }
        // self.cursor = cursor;
        // self.strategy = RenderableContentStrategy::Full;

        // let require_full = self.display_offset != display_offset as i32 ||
        //     self.has_blinking_enabled != has_blinking_enabled ||
        //     self.inner.len() != rows.len();

        // self.display_offset = display_offset as i32;
        // self.has_blinking_enabled = has_blinking_enabled;

        // if require_full {
        //     self.inner = rows.clone();
        //     return;
        // }

        // let mut diff = HashSet::with_capacity(rows.len());
        // for current_idx in 0..(rows.len()) {
        //     if current_idx == 0 {
        //         println!("{:?} {:?}", self.inner[current_idx], rows[current_idx]);
        //     }
        //     if rows[current_idx] != self.inner[current_idx] {
        //         self.inner[current_idx] = rows[current_idx].clone();
        //         diff.insert(current_idx);
        //     }
        // }

        // if !diff.is_empty() {
        //     self.strategy = RenderableContentStrategy::Lines(diff);
        // } else {
        //     self.strategy = RenderableContentStrategy::Noop;
        // }
    }
}
