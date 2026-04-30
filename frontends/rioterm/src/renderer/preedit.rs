//! IME preedit overlay.
//!
//! Plotting model. The IME hands us a `Preedit` carrying the in-flight
//! composition string and an optional cursor byte offset. We map that
//! string onto the visible terminal grid starting from the cursor cell,
//! producing a sparse `(row, col) -> PreeditCell` overlay that the row
//! emit pass consults per cell.
//!
//! Wide chars (CJK / emoji) consume two cells: the leading cell carries
//! `PreeditCell::Char(ch)` and the trailing cell carries
//! `PreeditCell::Spacer`. The renderer skips spacer cells so the wide
//! glyph's two-cell advance covers the continuation; emitting a literal
//! ' ' there caused visible gaps between CJK characters in the
//! composition.
//!
//! `ime_cursor_pos` records the cell where the IME cursor sits (when
//! the IME provides a `cursor_byte_offset`). The renderer paints a
//! caret beam on that cell to break the wezterm-style block so the user
//! can see where arrow-key navigation lands inside the composition.

use crate::ime::Preedit;
use unicode_width::UnicodeWidthChar;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreeditCell {
    Char(char),
    Spacer,
}

pub struct PreeditOverlay {
    columns: usize,
    cells: Vec<Option<PreeditCell>>,
    /// Per-row "any preedit cell present?" bitmap, used by the
    /// `build_row_*` fast paths so they don't have to scan a row's
    /// columns just to discover the row is uninvolved.
    row_has_any: Vec<bool>,
    /// Position of the IME cursor within the preedit, in (row, col) of
    /// the visible grid. `None` when the IME didn't report a cursor
    /// offset.
    ime_cursor_pos: Option<(usize, usize)>,
}

impl PreeditOverlay {
    pub fn new(
        preedit: &Preedit,
        start_row: usize,
        start_col: usize,
        columns: usize,
        rows: usize,
    ) -> Option<Self> {
        if preedit.text.is_empty() || columns == 0 || rows == 0 {
            return None;
        }

        let mut cells = vec![None; rows.saturating_mul(columns)];
        let mut row_has_any = vec![false; rows];
        let mut row = start_row;
        let mut col = start_col;
        let mut byte: usize = 0;
        let mut ime_cursor_pos = None;
        let cursor_byte_offset = preedit.cursor_byte_offset;

        let record_ime_cursor = |r: usize, c: usize, out: &mut Option<(usize, usize)>| {
            if r < rows && c < columns {
                *out = Some((r, c));
            }
        };

        for ch in preedit.text.chars() {
            // Record the IME cursor position BEFORE placing the char
            // when the cursor's byte offset lands right before it. We
            // do this before any wrap adjustments so the cursor sits
            // next to the cell the IME is about to edit.
            if cursor_byte_offset == Some(byte) && ime_cursor_pos.is_none() {
                if col >= columns && row + 1 < rows {
                    record_ime_cursor(row + 1, 0, &mut ime_cursor_pos);
                } else {
                    record_ime_cursor(row, col, &mut ime_cursor_pos);
                }
            }

            if row >= rows {
                break;
            }

            if col >= columns {
                row += 1;
                col = 0;
            }
            if row >= rows {
                break;
            }

            let width = ch.width().unwrap_or(1).max(1);
            if width > 1 && col + 1 >= columns {
                row += 1;
                col = 0;
                if row >= rows {
                    break;
                }
            }

            let idx = row * columns + col;
            if let Some(cell) = cells.get_mut(idx) {
                *cell = Some(PreeditCell::Char(ch));
                if let Some(slot) = row_has_any.get_mut(row) {
                    *slot = true;
                }
            }

            if width > 1 && col + 1 < columns {
                let spacer_idx = idx + 1;
                if let Some(cell) = cells.get_mut(spacer_idx) {
                    *cell = Some(PreeditCell::Spacer);
                }
            }

            byte = byte.saturating_add(ch.len_utf8());
            col = col.saturating_add(width);
            if col >= columns {
                row += 1;
                col = 0;
            }
        }

        // IME cursor at the end of the preedit text.
        if cursor_byte_offset == Some(preedit.text.len()) && ime_cursor_pos.is_none() {
            record_ime_cursor(row, col, &mut ime_cursor_pos);
        }

        Some(Self {
            columns,
            cells,
            row_has_any,
            ime_cursor_pos,
        })
    }

    #[inline]
    pub fn get(&self, row: usize, col: usize) -> Option<PreeditCell> {
        let idx = row.checked_mul(self.columns)?.saturating_add(col);
        self.cells.get(idx).copied().flatten()
    }

    #[inline]
    pub fn has_any_in_row(&self, row: usize) -> bool {
        self.row_has_any.get(row).copied().unwrap_or(false)
    }

    #[allow(dead_code)] // exposed for tests / future renderer paths
    #[inline]
    pub fn is_ime_cursor_at(&self, row: usize, col: usize) -> bool {
        self.ime_cursor_pos == Some((row, col))
    }

    #[inline]
    pub fn ime_cursor_in_row(&self, row: usize) -> Option<usize> {
        match self.ime_cursor_pos {
            Some((r, c)) if r == row => Some(c),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_wide_chars_and_spacers() {
        let preedit = Preedit::new("啊a".to_string(), None);
        let overlay = PreeditOverlay::new(&preedit, 0, 0, 4, 1).unwrap();

        assert_eq!(overlay.get(0, 0), Some(PreeditCell::Char('啊')));
        assert_eq!(overlay.get(0, 1), Some(PreeditCell::Spacer));
        assert_eq!(overlay.get(0, 2), Some(PreeditCell::Char('a')));
        assert_eq!(overlay.get(0, 3), None);
        assert!(overlay.has_any_in_row(0));
    }

    #[test]
    fn wraps_wide_chars() {
        let preedit = Preedit::new("啊".to_string(), None);
        let overlay = PreeditOverlay::new(&preedit, 0, 2, 3, 2).unwrap();

        assert_eq!(overlay.get(0, 2), None);
        assert_eq!(overlay.get(1, 0), Some(PreeditCell::Char('啊')));
        assert_eq!(overlay.get(1, 1), Some(PreeditCell::Spacer));
        assert!(!overlay.has_any_in_row(0));
        assert!(overlay.has_any_in_row(1));
    }

    #[test]
    fn tracks_ime_cursor_at_end() {
        // Typical Japanese IME: "あい" with the IME cursor at the end
        // of the composition (byte_offset == text.len()). Width is 4
        // cells; the cursor lands on the cell just past the
        // composition.
        let text = "あい".to_string();
        let len = text.len();
        let preedit = Preedit::new(text, Some(len));
        let overlay = PreeditOverlay::new(&preedit, 0, 0, 10, 1).unwrap();

        assert_eq!(overlay.get(0, 0), Some(PreeditCell::Char('あ')));
        assert_eq!(overlay.get(0, 1), Some(PreeditCell::Spacer));
        assert_eq!(overlay.get(0, 2), Some(PreeditCell::Char('い')));
        assert_eq!(overlay.get(0, 3), Some(PreeditCell::Spacer));
        assert!(overlay.is_ime_cursor_at(0, 4));
        assert_eq!(overlay.ime_cursor_in_row(0), Some(4));
        assert!(!overlay.is_ime_cursor_at(0, 0));
        assert!(!overlay.is_ime_cursor_at(0, 3));
    }

    #[test]
    fn tracks_ime_cursor_inside_preedit() {
        // IME cursor placed between the two wide characters of "あい".
        // "あ" is 3 bytes, so cursor_byte_offset == 3 puts the cursor
        // at column 2 (start of the second wide char).
        let preedit = Preedit::new("あい".to_string(), Some(3));
        let overlay = PreeditOverlay::new(&preedit, 0, 0, 10, 1).unwrap();

        assert!(overlay.is_ime_cursor_at(0, 2));
        assert!(!overlay.is_ime_cursor_at(0, 0));
        assert!(!overlay.is_ime_cursor_at(0, 4));
    }

    #[test]
    fn no_ime_cursor_when_offset_none() {
        let preedit = Preedit::new("hi".to_string(), None);
        let overlay = PreeditOverlay::new(&preedit, 0, 0, 10, 1).unwrap();

        assert!(!overlay.is_ime_cursor_at(0, 0));
        assert!(!overlay.is_ime_cursor_at(0, 1));
        assert!(!overlay.is_ime_cursor_at(0, 2));
        assert_eq!(overlay.ime_cursor_in_row(0), None);
    }

    #[test]
    fn ime_cursor_at_start_matches_terminal_cursor() {
        // cursor_byte_offset == 0 places the IME cursor on the same
        // cell as the terminal cursor (the preedit start column).
        let preedit = Preedit::new("abc".to_string(), Some(0));
        let overlay = PreeditOverlay::new(&preedit, 0, 2, 10, 1).unwrap();

        assert!(overlay.is_ime_cursor_at(0, 2));
    }

    #[test]
    fn empty_preedit_returns_none() {
        let preedit = Preedit::new(String::new(), None);
        assert!(PreeditOverlay::new(&preedit, 0, 0, 10, 1).is_none());
    }
}
