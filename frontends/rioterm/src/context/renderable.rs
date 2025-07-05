use rio_backend::config::CursorConfig;
use rio_backend::crosswords::pos::CursorState;
use rio_backend::selection::SelectionRange;
use std::time::Instant;

#[derive(Default, Clone, Debug)]
pub struct Cursor {
    pub state: CursorState,
    pub content: char,
    pub content_ref: char,
    pub is_ime_enabled: bool,
}

/// Hint label information for rendering
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct HintLabel {
    pub position: rio_backend::crosswords::pos::Pos,
    pub label: Vec<char>,
    pub is_first: bool,
}

#[derive(Default)]
pub struct RenderableContent {
    // TODO: Should not use default
    pub cursor: Cursor,
    pub has_blinking_enabled: bool,
    pub is_blinking_cursor_visible: bool,
    pub selection_range: Option<SelectionRange>,
    pub hint_labels: Vec<HintLabel>,
    pub highlighted_hint: Option<crate::hints::HintMatch>,
    pub last_typing: Option<Instant>,
    pub last_blink_toggle: Option<Instant>,
    pub has_pending_updates: bool,
}

impl RenderableContent {
    pub fn new(cursor: Cursor) -> Self {
        RenderableContent {
            cursor,
            has_blinking_enabled: false,
            selection_range: None,
            hint_labels: Vec::new(),
            highlighted_hint: None,
            last_typing: None,
            last_blink_toggle: None,
            has_pending_updates: false,
            is_blinking_cursor_visible: false,
        }
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
}
