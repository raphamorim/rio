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
}

pub fn create_snapshot<U: rio_backend::event::EventListener>(
    terminal: &FairMutex<Crosswords<U>>,
    damage: TerminalDamage,
) -> TerminalSnapshot {
    let mut terminal = terminal.lock();
    println!("create_snapshot {:?}", damage);
    let result = TerminalSnapshot {
        colors: terminal.colors,
        display_offset: terminal.display_offset(),
        blinking_cursor: terminal.blinking_cursor,
        visible_rows: terminal.visible_rows(),
        cursor: terminal.cursor(),
        damage,
    };
    terminal.reset_damage();
    drop(terminal);
    return result;
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

    /// Clear without taking damage
    pub fn clear(&mut self) {
        self.dirty = false;
        self.accumulated_damage = None;
    }

    /// Merge two damages into one - this is critical for correctness
    fn merge_damages(existing: &TerminalDamage, new: &TerminalDamage) -> TerminalDamage {
        match (existing, new) {
            // Any damage + Full = Full
            (_, TerminalDamage::Full) | (TerminalDamage::Full, _) => TerminalDamage::Full,
            // Partial damages: merge the line lists
            (TerminalDamage::Partial(lines1), TerminalDamage::Partial(lines2)) => {
                let mut merged = lines1.clone();
                for line in lines2 {
                    if !merged.iter().any(|l| l.line == line.line) {
                        merged.push(line.clone());
                    }
                }
                merged.sort_by_key(|damage| damage.line);
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

    pub fn push_snapshot(&mut self, snapshot: TerminalSnapshot) {
        self.invalidate(snapshot.damage);
    }

    pub fn push_full_snapshot<U: rio_backend::event::EventListener>(
        &mut self,
        terminal: &FairMutex<Crosswords<U>>,
    ) {
        self.invalidate_full(terminal);
    }

    pub fn push_sync(&mut self) {
        // Sync just marks as dirty without specific damage
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rio_backend::crosswords::pos::{Column, Pos};

    // Helper function to create a test snapshot with mock cursor
    fn create_test_snapshot_with_cursor(
        damage: TerminalDamage,
        cursor_line: usize,
    ) -> TerminalSnapshot {
        TerminalSnapshot {
            colors: TermColors::default(),
            display_offset: 0,
            blinking_cursor: false,
            visible_rows: vec![],
            cursor: CursorState {
                pos: Pos {
                    row: cursor_line.into(),
                    col: Column(0),
                },
                ..Default::default()
            },
            damage,
        }
    }

    // Helper function to create a test snapshot
    fn create_test_snapshot(damage: TerminalDamage) -> TerminalSnapshot {
        create_test_snapshot_with_cursor(damage, 0)
    }

    #[test]
    fn test_new_queue() {
        let queue = PendingUpdates::new();
        assert!(!queue.has());
    }

    #[test]
    fn test_push_snapshot() {
        let mut queue = PendingUpdates::new();

        let snapshot = create_test_snapshot(TerminalDamage::Full);
        queue.push_snapshot(snapshot);
        assert!(queue.has());

        if let Some(Update::Snapshot(snap)) = &queue.pending {
            assert!(matches!(snap.damage, TerminalDamage::Full));
        } else {
            panic!("Expected Full snapshot");
        }
    }

    #[test]
    fn test_replace_snapshot() {
        let mut queue = PendingUpdates::new();

        // Push first snapshot
        let snapshot1 = create_test_snapshot(TerminalDamage::CursorOnly);
        queue.push_snapshot(snapshot1);

        // Push second snapshot - should replace
        let snapshot2 = create_test_snapshot(TerminalDamage::Full);
        queue.push_snapshot(snapshot2);

        // Should only have one update
        assert!(queue.has());
        if let Some(Update::Snapshot(snap)) = &queue.pending {
            assert!(matches!(snap.damage, TerminalDamage::Full));
        }
    }

    #[test]
    fn test_merge_partial_damages() {
        let mut queue = PendingUpdates::new();

        // First partial damage
        let damage1 = TerminalDamage::Partial(vec![
            LineDamage {
                line: 1,
                damaged: true,
            },
            LineDamage {
                line: 3,
                damaged: true,
            },
        ]);
        queue.push_snapshot(create_test_snapshot(damage1));

        // Second partial damage - should merge
        let damage2 = TerminalDamage::Partial(vec![
            LineDamage {
                line: 2,
                damaged: true,
            },
            LineDamage {
                line: 4,
                damaged: true,
            },
        ]);
        queue.push_snapshot(create_test_snapshot(damage2));

        // Check merged result
        if let Some(Update::Snapshot(snap)) = &queue.pending {
            if let TerminalDamage::Partial(lines) = &snap.damage {
                assert_eq!(lines.len(), 4);
                assert_eq!(lines[0].line, 1);
                assert_eq!(lines[1].line, 2);
                assert_eq!(lines[2].line, 3);
                assert_eq!(lines[3].line, 4);
            } else {
                panic!("Expected Partial damage");
            }
        }
    }

    #[test]
    fn test_cursor_movement_damage() {
        let mut queue = PendingUpdates::new();

        // Cursor on line 5
        let snapshot1 = create_test_snapshot_with_cursor(TerminalDamage::CursorOnly, 5);
        queue.push_snapshot(snapshot1);

        // Cursor moves to line 10 - should preserve damage from line 5
        let snapshot2 = create_test_snapshot_with_cursor(TerminalDamage::CursorOnly, 10);
        queue.push_snapshot(snapshot2);

        // Both cursor positions should be preserved as damage
        if let Some(Update::Snapshot(snap)) = &queue.pending {
            // CursorOnly damages are merged as-is in our current implementation
            // The actual cursor position tracking would need to be handled by the terminal
            assert!(matches!(snap.damage, TerminalDamage::CursorOnly));
        }
    }

    #[test]
    fn test_sync_update() {
        let mut queue = PendingUpdates::new();

        queue.push_sync();
        assert!(queue.has());
        assert!(matches!(queue.pending, Some(Update::Sync)));
    }

    #[test]
    fn test_snapshot_replaces_sync() {
        let mut queue = PendingUpdates::new();

        queue.push_sync();
        let snapshot = create_test_snapshot(TerminalDamage::Full);
        queue.push_snapshot(snapshot);

        // Snapshot should replace Sync
        assert!(matches!(queue.pending, Some(Update::Snapshot(_))));
    }

    #[test]
    fn test_sync_does_not_replace_snapshot() {
        let mut queue = PendingUpdates::new();

        let snapshot = create_test_snapshot(TerminalDamage::Full);
        queue.push_snapshot(snapshot);
        queue.push_sync();

        // Snapshot should still be there
        assert!(matches!(queue.pending, Some(Update::Snapshot(_))));
    }

    #[test]
    fn test_take() {
        let mut queue = PendingUpdates::new();

        let snapshot = create_test_snapshot(TerminalDamage::Full);
        queue.push_snapshot(snapshot);

        assert!(queue.has());
        let taken = queue.take();
        assert!(taken.is_some());
        assert!(!queue.has());
    }
}
