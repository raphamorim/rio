//! Damage / render-contract types (the pull model).
//!
//! Faithful to Rio's *current* model (per-row boolean dirty + a coarse
//! enum), not the design's aspirational `SequenceNo` — which does not exist
//! in Rio yet and is a Phase 9 stretch (see ROADMAP ground-truth notes).
//! The render contract is "take the write lock briefly, copy dirty rows out,
//! release, then paint" — never hold the terminal lock across GPU work.

/// Grid dimensions in cells.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Dimensions {
    pub rows: u16,
    pub columns: u16,
}

impl Dimensions {
    #[inline]
    pub const fn new(rows: u16, columns: u16) -> Self {
        Self { rows, columns }
    }
}

/// Per-row dirty record (Rio's `LineDamage`).
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct LineDamage {
    pub line: usize,
    pub damaged: bool,
}

/// What changed since the last render. `Partial` is a unit variant in Rio —
/// the dirty row set lives in the terminal's `LineDamage` array, not in the
/// enum payload (ROADMAP ground-truth note).
#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub enum TerminalDamage {
    /// Nothing changed.
    #[default]
    Noop,
    /// The whole screen must repaint.
    Full,
    /// Some rows changed; consult the per-row dirty array.
    Partial,
    /// Only the cursor moved/blinked.
    CursorOnly,
}

/// Cursor shape for rendering.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CursorShape {
    Block,
    Underline,
    Beam,
    Hidden,
}

/// A renderer-facing snapshot of cursor state.
#[derive(Debug, Clone, Copy)]
pub struct CursorState {
    pub row: usize,
    pub column: usize,
    pub shape: CursorShape,
    pub visible: bool,
    pub blinking: bool,
}
