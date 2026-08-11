//! Terminal-owned anchors for graphics rendered by an embedder.
//!
//! The terminal owns mutation semantics while the embedder owns the payload
//! and rendering.  This keeps external protocols from reconstructing scroll,
//! margin, alternate-screen, history, and reflow state from PTY bytes.

/// Stable identifier chosen by the embedder.
pub type ExternalPlacementId = u64;

/// Terminal screen that owns an external placement.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ExternalPlacementScreen {
    /// Primary screen, including its retained scrollback.
    Main,
    /// Alternate screen.
    Alternate,
}

/// How terminal row mutations affect an external placement.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ExternalPlacementScrollPolicy {
    /// Move with grid content, including margin scrolling and reflow.
    #[default]
    Content,
    /// Keep the absolute row supplied by the embedder.
    Absolute,
}

/// How text erasure affects an external placement.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ExternalPlacementErasePolicy {
    /// Text writes and erase commands do not remove the placement.
    #[default]
    Preserve,
    /// Remove the entire placement when text erasure intersects it.
    Remove,
}

/// One terminal-owned external placement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalPlacement {
    /// Stable identifier chosen by the embedder.
    pub id: ExternalPlacementId,
    /// Screen that owns this placement.
    pub screen: ExternalPlacementScreen,
    /// Top row in the grid's stable signed absolute row space.
    pub abs_row: i64,
    /// Leftmost grid column.
    pub col: usize,
    /// Current visible cell span after terminal clipping.
    pub columns: usize,
    /// Current visible row span after terminal clipping.
    pub rows: usize,
    /// Horizontal cell offset into the original placement after clipping.
    pub source_col: usize,
    /// Vertical row offset into the original placement after clipping.
    pub source_row: usize,
    /// Original placement width in cells.
    pub source_columns: usize,
    /// Original placement height in rows.
    pub source_rows: usize,
    /// Terminal mutation policy.
    pub scroll_policy: ExternalPlacementScrollPolicy,
    /// Text erase policy.
    pub erase_policy: ExternalPlacementErasePolicy,
}

impl ExternalPlacement {
    /// Construct a placement with an unclipped source rectangle.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: ExternalPlacementId,
        screen: ExternalPlacementScreen,
        abs_row: i64,
        col: usize,
        columns: usize,
        rows: usize,
        scroll_policy: ExternalPlacementScrollPolicy,
        erase_policy: ExternalPlacementErasePolicy,
    ) -> Option<Self> {
        if columns == 0 || rows == 0 {
            return None;
        }
        col.checked_add(columns)?;
        let signed_rows = i64::try_from(rows).ok()?;
        abs_row.checked_add(signed_rows)?;
        Some(Self {
            id,
            screen,
            abs_row,
            col,
            columns,
            rows,
            source_col: 0,
            source_row: 0,
            source_columns: columns,
            source_rows: rows,
            scroll_policy,
            erase_policy,
        })
    }

    pub(crate) fn end_row(&self) -> i64 {
        self.abs_row.saturating_add(self.rows as i64)
    }

    pub(crate) fn end_col(&self) -> usize {
        self.col.saturating_add(self.columns)
    }

    pub(crate) fn intersects(&self, r0: i64, r1: i64, c0: usize, c1: usize) -> bool {
        self.abs_row < r1 && self.end_row() > r0 && self.col < c1 && self.end_col() > c0
    }

    /// Move a placement wholly inside `r0..r1`, clipping any rows that
    /// cross a margin. Placements crossing either margin before the move are
    /// deliberately left fixed, matching the Kitty page-margin rule.
    pub(crate) fn scroll_region(&mut self, r0: i64, r1: i64, delta: i64) -> bool {
        if self.scroll_policy != ExternalPlacementScrollPolicy::Content
            || self.abs_row < r0
            || self.end_row() > r1
        {
            return false;
        }

        let shifted_start = self.abs_row.saturating_add(delta);
        let shifted_end = self.end_row().saturating_add(delta);
        let clipped_start = shifted_start.max(r0);
        let clipped_end = shifted_end.min(r1);
        if clipped_end <= clipped_start {
            self.rows = 0;
            return true;
        }

        let clipped_top = clipped_start.saturating_sub(shifted_start) as usize;
        self.source_row = self.source_row.saturating_add(clipped_top);
        self.abs_row = clipped_start;
        self.rows = (clipped_end - clipped_start) as usize;
        true
    }
}

/// Viewport-relative geometry for an external placement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExternalPlacementGeometry {
    /// Stable embedder identifier.
    pub id: ExternalPlacementId,
    /// Signed row relative to the current viewport top.
    pub row: i64,
    /// Leftmost grid column.
    pub col: usize,
    /// Current cell span after margin clipping.
    pub columns: usize,
    /// Current row span after margin clipping.
    pub rows: usize,
    /// Horizontal source offset in original placement cells.
    pub source_col: usize,
    /// Vertical source offset in original placement rows.
    pub source_row: usize,
    /// Original width before clipping.
    pub source_columns: usize,
    /// Original height before clipping.
    pub source_rows: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placement(abs_row: i64, rows: usize) -> ExternalPlacement {
        ExternalPlacement::new(
            1,
            ExternalPlacementScreen::Main,
            abs_row,
            0,
            2,
            rows,
            ExternalPlacementScrollPolicy::Content,
            ExternalPlacementErasePolicy::Preserve,
        )
        .unwrap()
    }

    #[test]
    fn constructor_rejects_signed_row_overflow() {
        assert!(ExternalPlacement::new(
            1,
            ExternalPlacementScreen::Main,
            i64::MAX,
            0,
            1,
            1,
            ExternalPlacementScrollPolicy::Content,
            ExternalPlacementErasePolicy::Preserve,
        )
        .is_none());
    }

    #[test]
    fn region_scroll_clips_wholly_contained_placement_at_margin() {
        let mut p = placement(11, 2);
        assert!(p.scroll_region(11, 13, -1));
        assert_eq!((p.abs_row, p.rows, p.source_row), (11, 1, 1));
    }

    #[test]
    fn region_scroll_leaves_margin_crossing_placement_fixed() {
        let mut p = placement(10, 2);
        assert!(!p.scroll_region(11, 13, -1));
        assert_eq!((p.abs_row, p.rows, p.source_row), (10, 2, 0));
    }
}
