use rio_backend::config::CursorConfig;
use rio_backend::crosswords::grid::row::Row;
use rio_backend::crosswords::pos::CursorState;
use rio_backend::crosswords::square::Square;
use rio_backend::selection::SelectionRange;
use std::collections::HashSet;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub enum RenderableContentStrategy {
    Noop,
    Full,
    Lines(HashSet<usize>),
}

#[derive(Default, Clone, Debug)]
pub struct Cursor {
    pub state: CursorState,
    pub content: char,
    pub content_ref: char,
    pub is_ime_enabled: bool,
}

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
    pub last_typing: Option<Instant>,
    pub is_cursor_visible: bool,
}

impl RenderableContent {
    pub fn new(cursor: Cursor) -> Self {
        RenderableContent {
            inner: vec![],
            cursor,
            has_blinking_enabled: false,
            display_offset: 0,
            strategy: RenderableContentStrategy::Noop,
            selection_range: None,
            hyperlink_range: None,
            has_pending_updates: false,
            last_typing: None,
            is_cursor_visible: true,
        }
    }

    #[inline]
    pub fn mark_pending_updates(&mut self) {
        self.has_pending_updates = true;
    }

    pub fn from_cursor_config(config_cursor: &CursorConfig) -> Self {
        let cursor = Cursor {
            content: config_cursor.shape.into(),
            content_ref: config_cursor.shape.into(),
            state: CursorState::new(config_cursor.shape.into()),
            is_ime_enabled: false,
        };
        Self::new(cursor)
    }

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

        let has_selection = self.selection_range.is_some();
        if !has_selection && has_blinking_enabled {
            let mut should_blink = true;
            if let Some(last_typing_time) = self.last_typing {
                if last_typing_time.elapsed() < Duration::from_secs(1) {
                    should_blink = false;
                }
            }

            if should_blink {
                self.is_cursor_visible = !self.is_cursor_visible;
                diff.insert(*self.cursor.state.pos.row as usize);
            } else {
                self.is_cursor_visible = true;
            }
        }

        self.strategy = RenderableContentStrategy::Full;

        let require_full_clone = self.display_offset != display_offset as i32
            || self.has_blinking_enabled != has_blinking_enabled
            || self.has_pending_updates
            || self.inner.len() != rows.len()
            || has_selection
            || self.hyperlink_range.is_some();

        self.has_pending_updates = false;

        self.display_offset = display_offset as i32;
        self.has_blinking_enabled = has_blinking_enabled;

        if require_full_clone {
            self.inner = rows.clone();
            return;
        }

        // inner and rows will always contains same len
        for (current_idx, _) in rows.iter().enumerate() {
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
