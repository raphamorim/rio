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

#[cfg(test)]
mod tests {
    use super::*;
    use rio_backend::crosswords::pos::{Column, Line, Pos};
    use rio_backend::crosswords::{Crosswords, LineDamage};
    use rio_backend::event::VoidListener;
    use std::collections::BTreeSet;

    // Helper to create a test terminal
    fn create_test_terminal() -> FairMutex<Crosswords<VoidListener>> {
        use rio_backend::ansi::CursorShape;
        use rio_backend::crosswords::CrosswordsSize;
        use rio_window::window::WindowId;

        let dimensions = CrosswordsSize::new(80, 24);
        let terminal = Crosswords::new(
            dimensions,
            CursorShape::Block,
            VoidListener,
            WindowId::from(0),
            0,
        );
        FairMutex::new(terminal)
    }

    #[test]
    fn test_hint_matches_persistence() {
        let mut content = RenderableContent::from_cursor_config(&CursorConfig::default());

        // Set hint matches
        let matches = vec![
            Pos::new(Line(0), Column(0))..=Pos::new(Line(0), Column(4)),
            Pos::new(Line(1), Column(5))..=Pos::new(Line(1), Column(9)),
        ];
        content.hint_matches = Some(matches.clone());

        // Verify matches persist
        assert_eq!(content.hint_matches, Some(matches));
    }

    #[test]
    fn test_hint_labels_with_damage() {
        let mut content = RenderableContent::from_cursor_config(&CursorConfig::default());
        let terminal = create_test_terminal();

        // Add hint labels
        content.hint_labels.push(HintLabel {
            position: Pos::new(Line(5), Column(10)),
            label: vec!['a', 'b'],
            is_first: true,
        });
        content.hint_labels.push(HintLabel {
            position: Pos::new(Line(10), Column(20)),
            label: vec!['c'],
            is_first: false,
        });

        // Create damage for the hint label lines
        let mut damaged_lines = BTreeSet::new();
        damaged_lines.insert(LineDamage::new(5, true));
        damaged_lines.insert(LineDamage::new(10, true));
        let damage = TerminalDamage::Partial(damaged_lines);

        // Invalidate with damage
        content.pending_update.invalidate(damage, &terminal);

        // Verify update is marked as dirty
        assert!(content.pending_update.is_dirty());

        // Take snapshot and verify damage
        let snapshot = content.pending_update.take_snapshot().unwrap();
        match snapshot.damage {
            TerminalDamage::Partial(lines) => {
                assert_eq!(lines.len(), 2);
                assert!(lines.iter().any(|l| l.line == 5));
                assert!(lines.iter().any(|l| l.line == 10));
            }
            _ => panic!("Expected partial damage"),
        }
    }

    #[test]
    fn test_clear_hint_state_triggers_full_damage() {
        let mut content = RenderableContent::from_cursor_config(&CursorConfig::default());
        let terminal = create_test_terminal();

        // Set hint state
        content.hint_matches = Some(vec![
            Pos::new(Line(0), Column(0))..=Pos::new(Line(0), Column(4)),
        ]);
        content.hint_labels.push(HintLabel {
            position: Pos::new(Line(0), Column(0)),
            label: vec!['a'],
            is_first: true,
        });

        // Clear hint state and trigger full damage
        content.hint_matches = None;
        content.hint_labels.clear();
        content
            .pending_update
            .invalidate(TerminalDamage::Full, &terminal);

        // Verify update is marked as dirty
        assert!(content.pending_update.is_dirty());

        // Take snapshot and verify full damage
        let snapshot = content.pending_update.take_snapshot().unwrap();
        assert_eq!(snapshot.damage, TerminalDamage::Full);
    }

    #[test]
    fn test_damage_merging() {
        let mut content = RenderableContent::from_cursor_config(&CursorConfig::default());
        let terminal = create_test_terminal();

        // First invalidation with partial damage
        let mut damaged_lines1 = BTreeSet::new();
        damaged_lines1.insert(LineDamage::new(5, true));
        content
            .pending_update
            .invalidate(TerminalDamage::Partial(damaged_lines1), &terminal);

        // Second invalidation with different partial damage
        let mut damaged_lines2 = BTreeSet::new();
        damaged_lines2.insert(LineDamage::new(10, true));
        damaged_lines2.insert(LineDamage::new(15, true));
        content
            .pending_update
            .invalidate(TerminalDamage::Partial(damaged_lines2), &terminal);

        // Take snapshot and verify merged damage
        let snapshot = content.pending_update.take_snapshot().unwrap();
        match snapshot.damage {
            TerminalDamage::Partial(lines) => {
                assert_eq!(lines.len(), 3);
                assert!(lines.iter().any(|l| l.line == 5));
                assert!(lines.iter().any(|l| l.line == 10));
                assert!(lines.iter().any(|l| l.line == 15));
            }
            _ => panic!("Expected partial damage"),
        }
    }

    #[test]
    fn test_full_damage_overrides_partial() {
        let mut content = RenderableContent::from_cursor_config(&CursorConfig::default());
        let terminal = create_test_terminal();

        // First invalidation with partial damage
        let mut damaged_lines = BTreeSet::new();
        damaged_lines.insert(LineDamage::new(5, true));
        content
            .pending_update
            .invalidate(TerminalDamage::Partial(damaged_lines), &terminal);

        // Second invalidation with full damage
        content
            .pending_update
            .invalidate(TerminalDamage::Full, &terminal);

        // Take snapshot and verify full damage
        let snapshot = content.pending_update.take_snapshot().unwrap();
        assert_eq!(snapshot.damage, TerminalDamage::Full);
    }

    #[test]
    fn test_hint_state_update_flow() {
        let mut content = RenderableContent::from_cursor_config(&CursorConfig::default());
        let terminal = create_test_terminal();

        // Simulate hint activation
        content.hint_matches = Some(vec![
            Pos::new(Line(2), Column(5))..=Pos::new(Line(2), Column(10)),
            Pos::new(Line(5), Column(0))..=Pos::new(Line(5), Column(7)),
        ]);
        content.hint_labels.push(HintLabel {
            position: Pos::new(Line(2), Column(5)),
            label: vec!['a'],
            is_first: true,
        });
        content.hint_labels.push(HintLabel {
            position: Pos::new(Line(5), Column(0)),
            label: vec!['b'],
            is_first: true,
        });

        // Trigger damage for hint lines
        let mut damaged_lines = BTreeSet::new();
        damaged_lines.insert(LineDamage::new(2, true));
        damaged_lines.insert(LineDamage::new(5, true));
        content
            .pending_update
            .invalidate(TerminalDamage::Partial(damaged_lines), &terminal);

        assert!(content.pending_update.is_dirty());

        // Simulate hint deactivation
        content.hint_matches = None;
        content.hint_labels.clear();
        content
            .pending_update
            .invalidate(TerminalDamage::Full, &terminal);

        // Verify state after deactivation
        assert!(content.hint_matches.is_none());
        assert!(content.hint_labels.is_empty());
        assert!(content.pending_update.is_dirty());
    }

    #[test]
    fn test_multiple_snapshots() {
        let mut content = RenderableContent::from_cursor_config(&CursorConfig::default());
        let terminal = create_test_terminal();

        // First update
        content
            .pending_update
            .invalidate(TerminalDamage::Full, &terminal);
        assert!(content.pending_update.is_dirty());

        // Take snapshot - should clear dirty flag
        let snapshot1 = content.pending_update.take_snapshot();
        assert!(snapshot1.is_some());
        assert!(!content.pending_update.is_dirty());

        // Second update
        let mut damaged_lines = BTreeSet::new();
        damaged_lines.insert(LineDamage::new(3, true));
        content
            .pending_update
            .invalidate(TerminalDamage::Partial(damaged_lines), &terminal);
        assert!(content.pending_update.is_dirty());

        // Take second snapshot
        let snapshot2 = content.pending_update.take_snapshot();
        assert!(snapshot2.is_some());
        assert!(!content.pending_update.is_dirty());
    }
}
