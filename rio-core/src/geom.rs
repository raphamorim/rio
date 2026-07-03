//! Grid geometry newtypes (a minimal landing zone for the relocation of
//! `rio-backend::crosswords::pos`).
//!
//! These are intentionally small for the scaffold; the full set of index
//! newtypes (`Line`, `Column`, `Pos`, `Boundary`, `Side`, and the
//! `Physical`/`Visible`/`Stable` row-index distinctions the design calls
//! for) lands when `crosswords/pos.rs` moves. See `canario/ROADMAP.md`
//! Phase 2.

/// A row index. Negative values address scrollback above the viewport.
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Line(pub i32);

/// A column index within a row.
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Column(pub usize);

/// A grid position (`line`, `column`).
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Pos {
    pub row: Line,
    pub col: Column,
}

impl Pos {
    #[inline]
    pub const fn new(row: Line, col: Column) -> Self {
        Self { row, col }
    }
}

/// Which side of a cell a position refers to (for selection geometry).
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum Side {
    Left,
    Right,
}
