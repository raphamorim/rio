use rio_backend::crosswords::grid::row::Row;
use rio_backend::crosswords::pos::CursorState;
use rio_backend::crosswords::square::Square;

#[derive(Default, Clone, Debug)]
pub enum RenderableContentStrategy {
    Noop,
    #[default]
    Full,
    Lines(Vec<usize>),
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
        self.strategy = RenderableContentStrategy::Noop;
        self.display_offset = display_offset as i32;

        if self.cursor != cursor {
            self.cursor = cursor;
            self.strategy = RenderableContentStrategy::Full;
            self.inner = rows.clone();
            return;
        }
        self.cursor = cursor;

        if self.has_blinking_enabled != has_blinking_enabled {
            self.has_blinking_enabled = has_blinking_enabled;
            self.strategy = RenderableContentStrategy::Full;
            self.inner = rows.clone();
            return;
        }
        self.has_blinking_enabled = has_blinking_enabled;

        if self.inner.len() != rows.len() {
            self.strategy = RenderableContentStrategy::Full;
            self.inner = rows.clone();
            return;
        }

        let mut diff = Vec::with_capacity(rows.len());
        for current_idx in 0..(rows.len() - 1) {
            if rows[current_idx] != self.inner[current_idx] {
                diff.push(current_idx);
            }
        }

        self.inner = rows.clone();
        if !diff.is_empty() {
            self.strategy = RenderableContentStrategy::Lines(diff);
        }
    }
}
