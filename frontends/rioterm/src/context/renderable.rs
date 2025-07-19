use rio_backend::event::sync::FairMutex;
use rio_backend::crosswords::Crosswords;
use rio_backend::config::colors::term::TermColors;
use rio_backend::crosswords::square::Square;
use rio_backend::crosswords::grid::row::Row;
use smallvec::SmallVec;
use rio_backend::crosswords::LineDamage;
use rio_backend::config::CursorConfig;
use rio_backend::crosswords::pos::CursorState;
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
    pub pending_updates: PendingUpdates,
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
            pending_updates: PendingUpdates::default(),
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
pub enum Update {
    /// The actual snapshot
    Snapshot(TerminalSnapshot),
    /// Calls render function but if there's no damage from term will do nothing
    Sync,
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

pub fn create_snapshot<U: rio_backend::event::EventListener>(terminal: &FairMutex<Crosswords<U>>, damage: TerminalDamage) -> TerminalSnapshot {
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
pub struct PendingUpdates {
    pub queue: SmallVec<[Update; 3]>,
}

impl PendingUpdates {
    /// Create a new empty queue
    pub fn new() -> Self {
        Self {
            queue: SmallVec::new(),
        }
    }

    /// Check if queue has any pending updates
    #[inline]
    pub fn has(&self) -> bool {
        !self.queue.is_empty()
    }

    /// Check if queue is full
    #[inline]
    pub fn is_full(&self) -> bool {
        self.queue.len() >= 3
    }

    /// Get current queue length
    #[inline]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Check if queue is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Push snapshot to the queue, combining with existing snapshots when possible
    #[inline]
    pub fn push_snapshot(&mut self, snapshot: TerminalSnapshot) {
        self.push_update(Update::Snapshot(snapshot));
    }

    #[inline]
    pub fn push_full_snapshot<U: rio_backend::event::EventListener>(&mut self, terminal: &FairMutex<Crosswords<U>>) {
        let snapshot = create_snapshot(terminal, TerminalDamage::Full);
        self.push_update(Update::Snapshot(snapshot));
    }

    /// Push a sync event to the queue
    #[inline]
    pub fn push_sync(&mut self) {
        self.push_update(Update::Sync);
    }

    /// Push an update to the queue
    pub fn push_update(&mut self, update: Update) {
        // Try to combine with existing snapshots
        if let Update::Snapshot(ref new_snapshot) = update {
            if self.try_combine_snapshot(new_snapshot) {
                return;
            }
        }

        // If queue is full, remove the oldest item
        if self.queue.len() >= 3 {
            self.queue.remove(0);
        }

        // Add to queue
        self.queue.push(update);
    }

    /// Try to combine new snapshot with existing snapshots in the queue
    fn try_combine_snapshot(&mut self, new_snapshot: &TerminalSnapshot) -> bool {
        for update in &mut self.queue {
            if let Update::Snapshot(existing_snapshot) = update {
                if let Some(combined) = Self::combine_snapshots(existing_snapshot, &new_snapshot) {
                    *existing_snapshot = combined;
                    return true;
                }
            }
        }
        false
    }

    /// Combine two snapshots into one if possible
    fn combine_snapshots(existing: &TerminalSnapshot, new: &TerminalSnapshot) -> Option<TerminalSnapshot> {
        // We can combine snapshots if they have the same terminal state but different damage
        if existing.colors == new.colors
            && existing.display_offset == new.display_offset
            && existing.blinking_cursor == new.blinking_cursor
            && existing.visible_rows == new.visible_rows
            && existing.cursor == new.cursor {

            // Combine the damage
            if let Some(combined_damage) = Self::combine_damages(&existing.damage, &new.damage) {
                return Some(TerminalSnapshot {
                    colors: new.colors,
                    display_offset: new.display_offset,
                    blinking_cursor: new.blinking_cursor,
                    visible_rows: new.visible_rows.clone(),
                    cursor: new.cursor.clone(),
                    damage: combined_damage,
                });
            }
        }

        // If terminal state is different, we can't combine, but we can replace with the newer snapshot
        // that has combined damage if the damage can be combined
        if let Some(combined_damage) = Self::combine_damages(&existing.damage, &new.damage) {
            return Some(TerminalSnapshot {
                colors: new.colors,
                display_offset: new.display_offset,
                blinking_cursor: new.blinking_cursor,
                visible_rows: new.visible_rows.clone(),
                cursor: new.cursor.clone(),
                damage: combined_damage,
            });
        }

        None
    }

    /// Combine two damages into one if possible
    fn combine_damages(existing: &TerminalDamage, new: &TerminalDamage) -> Option<TerminalDamage> {
        match (existing, new) {
            // Any damage + Full = Full
            (_, TerminalDamage::Full) | (TerminalDamage::Full, _) => {
                Some(TerminalDamage::Full)
            }
            // CursorOnly can be absorbed by any other damage
            (TerminalDamage::CursorOnly, other) | (other, TerminalDamage::CursorOnly) => {
                Some(other.clone())
            }
            // Combine partial damages
            (TerminalDamage::Partial(lines1), TerminalDamage::Partial(lines2)) => {
                let mut combined: Vec<LineDamage> = lines1.iter().chain(lines2.iter()).cloned().collect();
                // Remove duplicates based on line number
                combined.sort_by_key(|damage| damage.line);
                combined.dedup_by_key(|damage| damage.line);
                Some(TerminalDamage::Partial(combined))
            }
        }
    }

    /// Pop the next update from the queue
    pub fn pop(&mut self) -> Option<Update> {
        if self.queue.is_empty() {
            None
        } else {
            Some(self.queue.remove(0))
        }
    }

    /// Clear all pending updates
    pub fn clear(&mut self) {
        self.queue.clear();
    }

    /// Peek at the next update without removing it
    pub fn peek(&self) -> Option<&Update> {
        self.queue.first()
    }

    /// Get all pending updates as a slice
    pub fn as_slice(&self) -> &[Update] {
        &self.queue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to create a test snapshot
    fn create_test_snapshot(damage: TerminalDamage) -> TerminalSnapshot {
        TerminalSnapshot {
            colors: TermColors::default(),
            display_offset: 0,
            blinking_cursor: false,
            visible_rows: vec![],
            cursor: CursorState::default(),
            damage,
        }
    }

    #[test]
    fn test_new_queue() {
        let queue = PendingUpdates::new();
        assert!(!queue.has());
        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn test_push_snapshot() {
        let mut queue = PendingUpdates::new();

        let snapshot = create_test_snapshot(TerminalDamage::Full);
        queue.push_snapshot(snapshot);
        assert!(queue.has());
        assert_eq!(queue.len(), 1);

        if let Some(Update::Snapshot(snap)) = queue.peek() {
            assert!(matches!(snap.damage, TerminalDamage::Full));
        } else {
            panic!("Expected Full snapshot");
        }
    }

    #[test]
    fn test_cursor_only_snapshot() {
        let mut queue = PendingUpdates::new();

        let snapshot = create_test_snapshot(TerminalDamage::CursorOnly);
        queue.push_snapshot(snapshot);
        assert!(queue.has());
        assert_eq!(queue.len(), 1);

        if let Some(Update::Snapshot(snap)) = queue.peek() {
            assert!(matches!(snap.damage, TerminalDamage::CursorOnly));
        } else {
            panic!("Expected CursorOnly snapshot");
        }
    }

    #[test]
    fn test_partial_snapshot() {
        let mut queue = PendingUpdates::new();

        let line_damage = vec![LineDamage { line: 5, damaged: true }];
        let snapshot = create_test_snapshot(TerminalDamage::Partial(line_damage));
        queue.push_snapshot(snapshot);
        assert!(queue.has());
        assert_eq!(queue.len(), 1);

        if let Some(Update::Snapshot(snap)) = queue.peek() {
            if let TerminalDamage::Partial(lines) = &snap.damage {
                assert_eq!(lines.len(), 1);
                assert_eq!(lines[0].line, 5);
            } else {
                panic!("Expected Partial damage");
            }
        } else {
            panic!("Expected Partial snapshot");
        }
    }

    #[test]
    fn test_push_sync() {
        let mut queue = PendingUpdates::new();

        queue.push_sync();
        assert!(queue.has());
        assert_eq!(queue.len(), 1);

        if let Some(Update::Sync) = queue.peek() {
            // Expected
        } else {
            panic!("Expected Sync update");
        }
    }

    #[test]
    fn test_queue_capacity() {
        let mut queue = PendingUpdates::new();
        // Fill queue to capacity with non-combinable updates
        queue.push_sync();
        let snapshot = create_test_snapshot(TerminalDamage::Full);
        queue.push_snapshot(snapshot);
        queue.push_sync();
        assert_eq!(queue.len(), 3);
        assert!(queue.is_full());

        // Adding another sync should shift the queue (syncs don't combine)
        queue.push_sync();
        assert_eq!(queue.len(), 3);

        // First item should now be the Full snapshot (was second item originally)
        if let Some(Update::Snapshot(snap)) = queue.peek() {
            assert!(matches!(snap.damage, TerminalDamage::Full));
        } else {
            panic!("Expected Full snapshot at front after shift");
        }
    }

    #[test]
    fn test_queue_capacity_with_combinables() {
        let mut queue = PendingUpdates::new();
        // Fill queue to capacity
        queue.push_sync();
        let snapshot = create_test_snapshot(TerminalDamage::Full);
        queue.push_snapshot(snapshot);
        queue.push_sync();
        assert_eq!(queue.len(), 3);
        assert!(queue.is_full());

        // Adding more should either combine or shift the queue
        let partial_snapshot = create_test_snapshot(TerminalDamage::Partial(vec![
            LineDamage { line: 1, damaged: true },
            LineDamage { line: 2, damaged: true },
            LineDamage { line: 3, damaged: true },
        ]));
        queue.push_snapshot(partial_snapshot);
        assert_eq!(queue.len(), 3);

        // The Full snapshot should have absorbed the Partial snapshot (Full + anything = Full)
        // So the queue should still be [Sync, Snapshot(Full), Sync]
        assert!(matches!(queue.peek(), Some(Update::Sync)));

        // To test actual overflow, add something that can't combine
        queue.push_sync(); // This should cause overflow since queue is full
        assert_eq!(queue.len(), 3);

        // First item should now be the Full snapshot (was second item originally)
        if let Some(Update::Snapshot(snap)) = queue.peek() {
            assert!(matches!(snap.damage, TerminalDamage::Full));
        } else {
            panic!("Expected Full snapshot at front after shift");
        }
    }

    #[test]
    fn test_pop() {
        let mut queue = PendingUpdates::new();

        queue.push_sync();
        let snapshot = create_test_snapshot(TerminalDamage::Full);
        queue.push_snapshot(snapshot);

        assert_eq!(queue.len(), 2);

        let first = queue.pop().unwrap();
        assert!(matches!(first, Update::Sync));
        assert_eq!(queue.len(), 1);

        let second = queue.pop().unwrap();
        if let Update::Snapshot(snap) = second {
            assert!(matches!(snap.damage, TerminalDamage::Full));
        } else {
            panic!("Expected snapshot");
        }
        assert_eq!(queue.len(), 0);
        assert!(!queue.has());

        let third = queue.pop();
        assert!(third.is_none());
    }

    #[test]
    fn test_snapshot_combination_full() {
        let mut queue = PendingUpdates::new();

        // Add some partial snapshot
        let partial_snapshot = create_test_snapshot(TerminalDamage::Partial(vec![
            LineDamage { line: 1, damaged: true },
            LineDamage { line: 2, damaged: true }
        ]));
        queue.push_snapshot(partial_snapshot);
        assert_eq!(queue.len(), 1);

        // Add full snapshot - should combine to full
        let full_snapshot = create_test_snapshot(TerminalDamage::Full);
        queue.push_snapshot(full_snapshot);
        assert_eq!(queue.len(), 1);

        if let Some(Update::Snapshot(snap)) = queue.peek() {
            assert!(matches!(snap.damage, TerminalDamage::Full));
        } else {
            panic!("Expected Full snapshot after combination");
        }
    }

    #[test]
    fn test_snapshot_combination_cursor_absorbed() {
        let mut queue = PendingUpdates::new();

        // Add cursor snapshot first
        let cursor_snapshot = create_test_snapshot(TerminalDamage::CursorOnly);
        queue.push_snapshot(cursor_snapshot);
        assert_eq!(queue.len(), 1);

        // Add partial snapshot - should absorb cursor damage
        let partial_snapshot = create_test_snapshot(TerminalDamage::Partial(vec![
            LineDamage { line: 5, damaged: true }
        ]));
        queue.push_snapshot(partial_snapshot);
        assert_eq!(queue.len(), 1);

        if let Some(Update::Snapshot(snap)) = queue.peek() {
            if let TerminalDamage::Partial(lines) = &snap.damage {
                assert_eq!(lines.len(), 1);
                assert_eq!(lines[0].line, 5);
            } else {
                panic!("Expected Partial damage after cursor absorption");
            }
        } else {
            panic!("Expected snapshot");
        }
    }

    #[test]
    fn test_snapshot_combination_cursor_absorbed_reverse() {
        let mut queue = PendingUpdates::new();

        // Add partial snapshot first
        let partial_snapshot = create_test_snapshot(TerminalDamage::Partial(vec![
            LineDamage { line: 3, damaged: true }
        ]));
        queue.push_snapshot(partial_snapshot);
        assert_eq!(queue.len(), 1);

        // Add cursor snapshot - should be absorbed
        let cursor_snapshot = create_test_snapshot(TerminalDamage::CursorOnly);
        queue.push_snapshot(cursor_snapshot);
        assert_eq!(queue.len(), 1);

        if let Some(Update::Snapshot(snap)) = queue.peek() {
            if let TerminalDamage::Partial(lines) = &snap.damage {
                assert_eq!(lines.len(), 1);
                assert_eq!(lines[0].line, 3);
            } else {
                panic!("Expected Partial damage after cursor absorption");
            }
        } else {
            panic!("Expected snapshot");
        }
    }

    #[test]
    fn test_snapshot_combination_partial() {
        let mut queue = PendingUpdates::new();

        let snapshot1 = create_test_snapshot(TerminalDamage::Partial(vec![
            LineDamage { line: 1, damaged: true },
            LineDamage { line: 3, damaged: true }
        ]));
        queue.push_snapshot(snapshot1);

        let snapshot2 = create_test_snapshot(TerminalDamage::Partial(vec![
            LineDamage { line: 2, damaged: true },
            LineDamage { line: 3, damaged: true }
        ])); // line 3 is duplicate
        queue.push_snapshot(snapshot2);

        assert_eq!(queue.len(), 1);

        if let Some(Update::Snapshot(snap)) = queue.peek() {
            if let TerminalDamage::Partial(lines) = &snap.damage {
                assert_eq!(lines.len(), 3);
                assert_eq!(lines[0].line, 1);
                assert_eq!(lines[1].line, 2);
                assert_eq!(lines[2].line, 3);
            } else {
                panic!("Expected combined partial damage");
            }
        } else {
            panic!("Expected snapshot");
        }
    }

    #[test]
    fn test_clear() {
        let mut queue = PendingUpdates::new();

        let snapshot = create_test_snapshot(TerminalDamage::Full);
        queue.push_snapshot(snapshot);
        queue.push_sync();

        assert_eq!(queue.len(), 2);

        queue.clear();

        assert_eq!(queue.len(), 0);
        assert!(!queue.has());
        assert!(queue.is_empty());
    }

    #[test]
    fn test_as_slice() {
        let mut queue = PendingUpdates::new();

        assert_eq!(queue.as_slice().len(), 0);

        queue.push_sync();
        let snapshot = create_test_snapshot(TerminalDamage::Full);
        queue.push_snapshot(snapshot);

        let slice = queue.as_slice();
        assert_eq!(slice.len(), 2);
        assert!(matches!(slice[0], Update::Sync));
        if let Update::Snapshot(snap) = &slice[1] {
            assert!(matches!(snap.damage, TerminalDamage::Full));
        } else {
            panic!("Expected snapshot");
        }
    }

    #[test]
    fn test_no_combination_different_types() {
        let mut queue = PendingUpdates::new();

        queue.push_sync();
        let snapshot = create_test_snapshot(TerminalDamage::Full);
        queue.push_snapshot(snapshot);

        // Sync and Snapshot should not combine
        assert_eq!(queue.len(), 2);

        let slice = queue.as_slice();
        assert!(matches!(slice[0], Update::Sync));
        if let Update::Snapshot(snap) = &slice[1] {
            assert!(matches!(snap.damage, TerminalDamage::Full));
        } else {
            panic!("Expected snapshot");
        }
    }

    #[test]
    fn test_overflow_behavior() {
        let mut queue = PendingUpdates::new();

        // Fill queue
        queue.push_sync();
        assert_eq!(queue.len(), 1);

        let snapshot1 = create_test_snapshot(TerminalDamage::Partial(vec![
            LineDamage { line: 1, damaged: true }
        ]));
        queue.push_snapshot(snapshot1);

        let snapshot2 = create_test_snapshot(TerminalDamage::Partial(vec![
            LineDamage { line: 2, damaged: true }
        ]));
        queue.push_snapshot(snapshot2);

        assert_eq!(queue.len(), 2);

        // This should combine with existing partial damage
        let snapshot3 = create_test_snapshot(TerminalDamage::Partial(vec![
            LineDamage { line: 3, damaged: true }
        ]));
        queue.push_snapshot(snapshot3);

        assert_eq!(queue.len(), 2);

        // Add something that can't combine
        queue.push_sync();

        assert_eq!(queue.len(), 3);

        let slice = queue.as_slice();
        // The first item should still be the original Sync since no overflow occurred
        assert!(matches!(slice[0], Update::Sync));
    }

    #[test]
    fn test_edge_cases() {
        let mut queue = PendingUpdates::new();

        // Test empty partial damage
        let empty_partial = create_test_snapshot(TerminalDamage::Partial(vec![]));
        queue.push_snapshot(empty_partial);
        assert_eq!(queue.len(), 1);

        // Test cursor only
        let cursor_snapshot = create_test_snapshot(TerminalDamage::CursorOnly);
        queue.push_snapshot(cursor_snapshot);
        assert_eq!(queue.len(), 1);

        // Should combine - empty partial should remain as empty partial
        if let Some(Update::Snapshot(snap)) = queue.peek() {
            if let TerminalDamage::Partial(lines) = &snap.damage {
                assert_eq!(lines.len(), 0);
            } else {
                panic!("Expected empty partial damage");
            }
        } else {
            panic!("Expected snapshot");
        }
    }

    #[test]
    fn test_full_overrides_everything() {
        let mut queue = PendingUpdates::new();

        // Add various damage types
        let cursor_snapshot = create_test_snapshot(TerminalDamage::CursorOnly);
        queue.push_snapshot(cursor_snapshot);

        let partial_snapshot = create_test_snapshot(TerminalDamage::Partial(vec![
            LineDamage { line: 1, damaged: true }
        ]));
        queue.push_snapshot(partial_snapshot);

        let full_snapshot = create_test_snapshot(TerminalDamage::Full);
        queue.push_snapshot(full_snapshot);

        assert_eq!(queue.len(), 1);

        if let Some(Update::Snapshot(snap)) = queue.peek() {
            assert!(matches!(snap.damage, TerminalDamage::Full));
        } else {
            panic!("Expected Full snapshot to override everything");
        }
    }
}
