use rio_backend::config::colors::term::TermColors;
use rio_backend::config::CursorConfig;
use rio_backend::crosswords::grid::row::Row;
use rio_backend::crosswords::pos::CursorState;
use rio_backend::crosswords::square::Square;
use rio_backend::event::TerminalDamage;
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
    pub hyperlink_range: Option<SelectionRange>,
    pub hint_labels: Vec<HintLabel>,
    pub highlighted_hint: Option<crate::hints::HintMatch>,
    pub hint_matches: Option<Vec<rio_backend::crosswords::search::Match>>,
    pub last_typing: Option<Instant>,
    pub last_blink_toggle: Option<Instant>,
    pub pending_update: PendingUpdate,
}

impl RenderableContent {
    pub fn new(cursor: Cursor) -> Self {
        RenderableContent {
            cursor,
            has_blinking_enabled: false,
            selection_range: None,
            hint_labels: Vec::new(),
            highlighted_hint: None,
            hint_matches: None,
            last_typing: None,
            last_blink_toggle: None,
            hyperlink_range: None,
            pending_update: PendingUpdate::default(),
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

#[derive(Debug, Clone, PartialEq)]
pub struct TerminalSnapshot {
    pub colors: TermColors,
    pub display_offset: usize,
    pub blinking_cursor: bool,
    pub visible_rows: Vec<Row<Square>>,
    pub cursor: CursorState,
    pub damage: TerminalDamage,
    // Cache terminal dimensions to avoid repeated calls
    pub columns: usize,
    pub screen_lines: usize,
}

#[derive(Debug, Default)]
pub struct PendingUpdate {
    /// Whether there's any pending update that needs rendering
    dirty: bool,
    /// UI-level damage (hints, selections) that needs to be merged with terminal damage
    ui_damage: Option<TerminalDamage>,
}

impl PendingUpdate {
    /// Check if there's a pending update
    #[inline]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark as needing to check for damage on next render
    /// This is used by Wakeup events to defer damage calculation
    pub fn set_dirty(&mut self) {
        self.dirty = true;
    }

    /// Mark as needing update with UI-level damage (hints, selections)
    pub fn set_ui_damage(&mut self, damage: TerminalDamage) {
        self.dirty = true;
        self.ui_damage = Some(match self.ui_damage.take() {
            None => damage,
            Some(existing) => Self::merge_damages(existing, damage),
        });
    }

    /// Get and clear UI damage
    pub fn take_ui_damage(&mut self) -> Option<TerminalDamage> {
        self.ui_damage.take()
    }

    /// Reset the dirty flag after rendering
    pub fn reset(&mut self) {
        self.dirty = false;
        // Note: ui_damage is cleared by take_ui_damage during render
    }

    /// Merge two damages into one
    fn merge_damages(existing: TerminalDamage, new: TerminalDamage) -> TerminalDamage {
        match (existing, new) {
            // Any damage + Full = Full
            (_, TerminalDamage::Full) | (TerminalDamage::Full, _) => TerminalDamage::Full,
            // Partial damages: merge the line lists
            (TerminalDamage::Partial(mut lines1), TerminalDamage::Partial(lines2)) => {
                lines1.extend(lines2);
                TerminalDamage::Partial(lines1)
            }
            // CursorOnly damages need special handling
            (TerminalDamage::CursorOnly, TerminalDamage::Partial(lines))
            | (TerminalDamage::Partial(lines), TerminalDamage::CursorOnly) => {
                TerminalDamage::Partial(lines)
            }
            (TerminalDamage::CursorOnly, TerminalDamage::CursorOnly) => {
                TerminalDamage::CursorOnly
            }
        }
    }
}
