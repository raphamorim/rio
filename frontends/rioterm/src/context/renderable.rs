use rio_backend::config::colors::term::TermColors;
use rio_backend::config::CursorConfig;
use rio_backend::crosswords::grid::row::Row;
use rio_backend::crosswords::pos::CursorState;
use rio_backend::crosswords::square::Square;
use rio_backend::crosswords::Crosswords;
use rio_backend::event::sync::FairMutex;
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
    pub hint_labels: Vec<HintLabel>,
    pub highlighted_hint: Option<crate::hints::HintMatch>,
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
            last_typing: None,
            last_blink_toggle: None,
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
    /// The terminal snapshot with accumulated damage
    snapshot: Option<TerminalSnapshot>,
}

impl PendingUpdate {
    /// Check if there's a pending update
    #[inline]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark as needing update with the given damage
    pub fn invalidate<U: rio_backend::event::EventListener>(
        &mut self,
        damage: TerminalDamage,
        terminal: &FairMutex<Crosswords<U>>,
    ) {
        self.dirty = true;

        let mut terminal = terminal.lock();

        // Get the terminal's current damage and merge with incoming damage
        let terminal_damage = terminal.peek_damage_event();
        let merged_damage = match (terminal_damage, &damage) {
            (None, damage) => damage.clone(),
            (Some(term_damage), damage) => Self::merge_damages(&term_damage, damage),
        };

        // Create or update the snapshot
        match &mut self.snapshot {
            None => {
                // Create new snapshot
                self.snapshot = Some(TerminalSnapshot {
                    colors: terminal.colors,
                    display_offset: terminal.display_offset(),
                    blinking_cursor: terminal.blinking_cursor,
                    visible_rows: terminal.visible_rows(),
                    cursor: terminal.cursor(),
                    damage: merged_damage,
                    columns: terminal.columns(),
                    screen_lines: terminal.screen_lines(),
                });
            }
            Some(existing_snapshot) => {
                // Update existing snapshot with fresh terminal state but merge damage
                existing_snapshot.colors = terminal.colors;
                existing_snapshot.display_offset = terminal.display_offset();
                existing_snapshot.blinking_cursor = terminal.blinking_cursor;
                existing_snapshot.visible_rows = terminal.visible_rows();
                existing_snapshot.cursor = terminal.cursor();
                existing_snapshot.damage =
                    Self::merge_damages(&existing_snapshot.damage, &merged_damage);
                existing_snapshot.columns = terminal.columns();
                existing_snapshot.screen_lines = terminal.screen_lines();
            }
        }

        // Reset terminal damage since we've captured it in the snapshot
        terminal.reset_damage();
    }

    /// Mark as needing full update
    pub fn invalidate_full<U: rio_backend::event::EventListener>(
        &mut self,
        terminal: &FairMutex<Crosswords<U>>,
    ) {
        self.invalidate(TerminalDamage::Full, terminal);
    }

    /// Take the snapshot and reset dirty flag
    /// This should only be called when actually rendering!
    pub fn take_snapshot(&mut self) -> Option<TerminalSnapshot> {
        self.dirty = false;
        self.snapshot.take()
    }

    /// Merge two damages into one - this is critical for correctness
    fn merge_damages(existing: &TerminalDamage, new: &TerminalDamage) -> TerminalDamage {
        use std::collections::BTreeSet;

        match (existing, new) {
            // Any damage + Full = Full
            (_, TerminalDamage::Full) | (TerminalDamage::Full, _) => TerminalDamage::Full,
            // Partial damages: merge the line lists efficiently using BTreeSet
            (TerminalDamage::Partial(lines1), TerminalDamage::Partial(lines2)) => {
                let mut line_set = BTreeSet::new();

                // Add all damaged lines from both sets
                for damage in lines1.iter().chain(lines2.iter()) {
                    if damage.damaged {
                        line_set.insert(*damage);
                    }
                }

                TerminalDamage::Partial(line_set)
            }
            // CursorOnly damages need special handling
            (TerminalDamage::CursorOnly, TerminalDamage::Partial(lines)) => {
                TerminalDamage::Partial(lines.clone())
            }
            (TerminalDamage::Partial(lines), TerminalDamage::CursorOnly) => {
                TerminalDamage::Partial(lines.clone())
            }
            (TerminalDamage::CursorOnly, TerminalDamage::CursorOnly) => {
                TerminalDamage::CursorOnly
            }
        }
    }
}
