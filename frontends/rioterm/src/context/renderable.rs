use rio_backend::config::colors::term::TermColors;
use rio_backend::config::CursorConfig;
use rio_backend::crosswords::grid::row::Row;
use rio_backend::crosswords::pos::CursorState;
use rio_backend::crosswords::square::Square;
use rio_backend::crosswords::Crosswords;
use rio_backend::crosswords::LineDamage;
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

#[derive(Default)]
pub struct RenderableContent {
    // TODO: Should not use default
    pub cursor: Cursor,
    pub has_blinking_enabled: bool,
    pub is_blinking_cursor_visible: bool,
    pub selection_range: Option<SelectionRange>,
    pub hyperlink_range: Option<SelectionRange>,
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
            hyperlink_range: None,
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
    /// Accumulated damage since last render - tracks ALL changes
    accumulated_damage: Option<TerminalDamage>,
}

impl PendingUpdate {
    /// Check if there's a pending update
    #[inline]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark as needing update with the given damage
    pub fn invalidate(&mut self, damage: TerminalDamage) {
        self.dirty = true;

        // Always accumulate damage
        match &mut self.accumulated_damage {
            None => {
                self.accumulated_damage = Some(damage);
            }
            Some(existing) => {
                *existing = Self::merge_damages(existing, &damage);
            }
        }
    }

    /// Mark as needing full update
    pub fn invalidate_full<U: rio_backend::event::EventListener>(
        &mut self,
        _terminal: &FairMutex<Crosswords<U>>,
    ) {
        self.dirty = true;
        self.accumulated_damage = Some(TerminalDamage::Full);
    }

    /// Take the accumulated damage and reset dirty flag
    /// This should only be called when actually rendering!
    pub fn take_damage(&mut self) -> Option<TerminalDamage> {
        self.dirty = false;
        self.accumulated_damage.take()
    }

    /// Merge two damages into one - this is critical for correctness
    fn merge_damages(existing: &TerminalDamage, new: &TerminalDamage) -> TerminalDamage {
        use std::collections::BTreeSet;
        
        match (existing, new) {
            // Any damage + Full = Full
            (_, TerminalDamage::Full) | (TerminalDamage::Full, _) => TerminalDamage::Full,
            // Partial damages: merge the line lists efficiently using BTreeSet
            (TerminalDamage::Partial(lines1), TerminalDamage::Partial(lines2)) => {
                let mut line_set: BTreeSet<usize> = BTreeSet::new();
                
                // Add all damaged lines from both sets
                for damage in lines1.iter().chain(lines2.iter()) {
                    if damage.damaged {
                        line_set.insert(damage.line);
                    }
                }
                
                // BTreeSet iterator yields items in sorted order
                let merged: Vec<LineDamage> = line_set
                    .into_iter()
                    .map(|line| LineDamage { line, damaged: true })
                    .collect();
                
                TerminalDamage::Partial(merged)
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

// Compatibility layer for old API
impl PendingUpdate {
    pub fn has(&self) -> bool {
        self.is_dirty()
    }

    pub fn push_full_snapshot<U: rio_backend::event::EventListener>(
        &mut self,
        terminal: &FairMutex<Crosswords<U>>,
    ) {
        self.invalidate_full(terminal);
    }
}