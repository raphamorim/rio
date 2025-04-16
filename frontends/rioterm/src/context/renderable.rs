use rio_backend::config::colors::term::TermColors;
use rio_backend::config::CursorConfig;
use rio_backend::crosswords::grid::row::Row;
use rio_backend::crosswords::pos::Column;
use rio_backend::crosswords::pos::CursorState;
use rio_backend::crosswords::square::Square;
use rio_backend::crosswords::TermDamage;
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

#[derive(Default)]
pub struct RenderableContent {
    pub display_offset: i32,
    // TODO: Should not use default
    pub cursor: Cursor,
    pub colors: TermColors,
    pub has_blinking_enabled: bool,
    pub selection_range: Option<SelectionRange>,
    pub hyperlink_range: Option<SelectionRange>,
    pub has_pending_updates: bool,
    pub last_typing: Option<Instant>,
    pub is_cursor_visible: bool,
}

impl RenderableContent {
    pub fn new(cursor: Cursor) -> Self {
        RenderableContent {
            colors: TermColors::default(),
            cursor,
            has_blinking_enabled: false,
            display_offset: 0,
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
        cursor: CursorState,
        colors: TermColors,
        has_blinking_enabled: bool,
        damage: TermDamage,
    ) {
        // self.has_blinking_enabled = has_blinking_enabled;
        // self.cursor.state = cursor;
        // self.colors = colors;
        // match damage {
        //     TermDamage::Full => {
        //         self.strategy = RenderableContentStrategy::Full;
        //         self.inner = rows.clone();
        //     },
        //     TermDamage::Partial(lines) => {
        //         let mut diff: HashSet<usize> = HashSet::with_capacity(rows.len());
        //         for line in lines {
        //             println!(">>>>> {:?}", line);
        //             for i in line.left..line.right {
        //                 self.inner[line.line][Column(i)] = rows[line.line][Column(i)].clone();
        //             }
        //             diff.insert(line.line);
        //         }
        //         self.strategy = RenderableContentStrategy::Lines(diff);
        //     },
        // }

        // let has_selection = self.selection_range.is_some();
        // if !has_selection && has_blinking_enabled {
        //     let mut should_blink = true;
        //     if let Some(last_typing_time) = self.last_typing {
        //         if last_typing_time.elapsed() < Duration::from_secs(1) {
        //             should_blink = false;
        //         }
        //     }

        //     if should_blink {
        //         self.is_cursor_visible = !self.is_cursor_visible;
        //         diff.insert(*self.cursor.state.pos.row as usize);
        //     } else {
        //         self.is_cursor_visible = true;
        //     }
        // }

        // self.strategy = RenderableContentStrategy::Full;

        // let require_full_clone = self.display_offset != display_offset as i32
        //     || self.has_blinking_enabled != has_blinking_enabled
        //     || self.has_pending_updates
        //     || self.inner.len() != rows.len()
        //     || has_selection
        //     || self.colors != colors
        //     || self.hyperlink_range.is_some();

        // self.has_pending_updates = false;

        // self.display_offset = display_offset as i32;
        // self.has_blinking_enabled = has_blinking_enabled;

        // if require_full_clone {
        //     self.inner = rows.clone();
        //     return;
        // }
    }
}
