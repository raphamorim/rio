//! Inline IME preedit: the composition string drawn as a wezterm-style
//! block at the cursor, on a single row.
//!
//! Layout rules, in priority order:
//!
//! - The composition always occupies exactly one row — the cursor row.
//!   When it doesn't fit between the cursor column and the right edge,
//!   it slides left (the wezterm/alacritty behavior); when it is wider
//!   than the whole row, leading clusters are dropped so the tail —
//!   where the IME caret lives — stays visible. It never wraps to the
//!   line below and never truncates silently at the right edge.
//! - Text is segmented into grapheme clusters, not chars: a cluster is
//!   what the user perceives as one character, so a ZWJ emoji or a
//!   decomposed `e` + U+0302 occupies one slot and shapes as one unit.
//!   Cluster width is clamped to 1 or 2 cells, the only widths a
//!   terminal cell can express.
//! - The caret marks where IME editing (arrow keys between conversion
//!   segments) currently is: on a composition cell it renders as a
//!   thick underline (visible on the cursor-colored block), past the
//!   end of the text as a beam on the following cell (visible on the
//!   normal background). Both presentations exist because a beam drawn
//!   in the cursor color on a cursor-colored block is invisible. When
//!   the IME reports a hidden caret (candidate paging), none renders.
//!
//! The caller decides visibility: the overlay is only built for the
//! active panel and never while the viewport is scrolled into history
//! (`display_offset != 0`), where the cursor row is off-screen and the
//! anchor would lie.

use rio_unicode::grapheme::GraphemeIndices;
use rio_unicode::UnicodeWidthStr;

/// Where the IME put its caret, as the platform reported it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreeditCursor {
    /// The IME asked for no visible caret. Wayland (`cursor_begin =
    /// -1`) and Windows deliver this with non-empty text while the
    /// user pages through conversion candidates; drawing a caret then
    /// would contradict the IME's own UI.
    Hidden,
    /// Caret before the cluster containing this byte offset; offsets
    /// at or past the end of the text mean end-of-text.
    Byte(usize),
}

/// One cell of the composition line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreeditCell {
    /// Leading cell of a grapheme cluster; index into `clusters`.
    Start(u16),
    /// Trailing cell of a two-cell cluster. No glyph of its own — the
    /// leading cell's advance covers it — but it takes the block fill.
    Continuation,
}

/// Where and how the IME caret renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreeditCaret {
    /// Caret on a composition cell: thick underline under that cell.
    OnCell(usize),
    /// Caret one past the composition: beam on that (non-block) cell.
    PastEnd(usize),
    /// The IME asked for no caret (candidate paging): draw none.
    Hidden,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreeditLine {
    /// Screen row (viewport-relative) the composition renders on.
    pub row: usize,
    /// First column of the block.
    pub start_col: usize,
    /// Dense cells from `start_col`; never extends past the row edge.
    cells: Vec<PreeditCell>,
    /// Grapheme clusters, indexed by `PreeditCell::Start`.
    clusters: Vec<String>,
    pub caret: PreeditCaret,
}

impl PreeditLine {
    /// Lay out the composition `text` anchored at the cursor cell.
    /// Returns `None` for an empty composition or a degenerate grid.
    /// `cursor` byte offsets never slice `text`, only compare against
    /// cluster starts, so intra-cluster and non-char-boundary offsets
    /// are safe and mark the cluster containing them.
    pub fn new(
        text: &str,
        cursor: PreeditCursor,
        cursor_row: usize,
        cursor_col: usize,
        columns: usize,
    ) -> Option<Self> {
        if text.is_empty() || columns == 0 {
            return None;
        }

        // Segment into clusters with terminal cell widths.
        struct Seg<'a> {
            byte_start: usize,
            cluster: &'a str,
            width: usize,
        }
        let mut segs: Vec<Seg> = GraphemeIndices::new(text)
            .map(|(byte_start, cluster)| Seg {
                byte_start,
                cluster,
                width: match UnicodeWidthStr::width(cluster) {
                    0 | 1 => 1,
                    _ => 2,
                },
            })
            .collect();
        if segs.is_empty() {
            return None;
        }

        // The caret sits on the cluster CONTAINING the byte offset, so
        // an intra-cluster offset (jamo-level Korean IMEs report these)
        // marks the cluster being edited, not its neighbor; offsets at
        // or past the end mean end-of-text. A hidden caret (candidate
        // paging) stays hidden.
        let mut caret_index = match cursor {
            PreeditCursor::Hidden => None,
            PreeditCursor::Byte(offset) if offset >= text.len() => Some(segs.len()),
            PreeditCursor::Byte(offset) => Some(
                segs.iter()
                    .rposition(|seg| seg.byte_start <= offset)
                    .unwrap_or(0),
            ),
        };

        // Wider than the row: drop leading clusters, keeping the tail
        // (and with it the caret, which lives near the end while
        // composing). A caret pointing into the dropped region clamps
        // to the first visible cluster.
        let mut total: usize = segs.iter().map(|seg| seg.width).sum();
        let mut dropped = 0usize;
        while total > columns && segs.len() > 1 {
            total -= segs.remove(0).width;
            dropped += 1;
        }
        caret_index = caret_index.map(|index| index.saturating_sub(dropped));
        if total > columns {
            // A single cluster wider than the grid; nothing sane to draw.
            return None;
        }

        // Anchor at the cursor, sliding left so the tail stays on-row.
        let start_col = cursor_col.min(columns - total);

        let mut cells = Vec::with_capacity(total);
        let mut clusters = Vec::with_capacity(segs.len());
        let mut caret = match caret_index {
            Some(_) => None,
            None => Some(PreeditCaret::Hidden),
        };
        for (i, seg) in segs.iter().enumerate() {
            if caret_index == Some(i) {
                caret = Some(PreeditCaret::OnCell(start_col + cells.len()));
            }
            cells.push(PreeditCell::Start(clusters.len() as u16));
            for _ in 1..seg.width {
                cells.push(PreeditCell::Continuation);
            }
            clusters.push(seg.cluster.to_string());
        }
        let caret = caret.unwrap_or({
            let past = start_col + cells.len();
            if past < columns {
                PreeditCaret::PastEnd(past)
            } else {
                // Composition touches the right edge: underline the
                // last cluster instead of pushing the beam off-row.
                let last_start = cells
                    .iter()
                    .rposition(|c| matches!(c, PreeditCell::Start(_)))
                    .unwrap_or(0);
                PreeditCaret::OnCell(start_col + last_start)
            }
        });

        Some(Self {
            row: cursor_row,
            start_col,
            cells,
            clusters,
            caret,
        })
    }

    /// The composition cell at an absolute column, bounds-checked on
    /// both sides.
    #[inline]
    pub fn cell(&self, col: usize) -> Option<PreeditCell> {
        col.checked_sub(self.start_col)
            .and_then(|i| self.cells.get(i))
            .copied()
    }

    #[inline]
    pub fn cluster(&self, index: u16) -> &str {
        self.clusters
            .get(index as usize)
            .map(String::as_str)
            .unwrap_or_default()
    }

    /// Column one past the block's last cell.
    #[inline]
    pub fn end_col(&self) -> usize {
        self.start_col + self.cells.len()
    }

    /// Column the OS candidate popup should anchor to: the caret,
    /// or the last composition cell when the caret is hidden.
    #[inline]
    pub fn popup_anchor_col(&self) -> usize {
        match self.caret {
            PreeditCaret::OnCell(col) | PreeditCaret::PastEnd(col) => col,
            PreeditCaret::Hidden => self.end_col().saturating_sub(1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(text: &str, caret: Option<usize>) -> Option<PreeditLine> {
        layout_at(text, caret, 0, 0)
    }

    fn layout_at(
        text: &str,
        caret: Option<usize>,
        row: usize,
        col: usize,
    ) -> Option<PreeditLine> {
        // `None` = end-of-text, the common IME idiom while composing.
        let cursor = PreeditCursor::Byte(caret.unwrap_or(text.len()));
        PreeditLine::new(text, cursor, row, col, 80)
    }

    #[test]
    fn ascii_at_cursor() {
        let line = layout_at("abc", Some(3), 5, 10).unwrap();
        assert_eq!(line.row, 5);
        assert_eq!(line.start_col, 10);
        assert_eq!(line.cell(10), Some(PreeditCell::Start(0)));
        assert_eq!(line.cell(12), Some(PreeditCell::Start(2)));
        assert_eq!(line.cell(13), None);
        assert_eq!(line.cell(9), None);
        assert_eq!(line.caret, PreeditCaret::PastEnd(13));
    }

    #[test]
    fn wide_clusters_take_two_cells() {
        // "日本" = two clusters, two cells each.
        let line = layout("日本", None).unwrap();
        assert_eq!(line.cell(0), Some(PreeditCell::Start(0)));
        assert_eq!(line.cell(1), Some(PreeditCell::Continuation));
        assert_eq!(line.cell(2), Some(PreeditCell::Start(1)));
        assert_eq!(line.cell(3), Some(PreeditCell::Continuation));
        assert_eq!(line.caret, PreeditCaret::PastEnd(4));
    }

    #[test]
    fn grapheme_clusters_stay_whole() {
        // Decomposed e + combining circumflex: one cluster, one cell.
        let line = layout("e\u{302}x", None).unwrap();
        assert_eq!(line.cell(0), Some(PreeditCell::Start(0)));
        assert_eq!(line.cluster(0), "e\u{302}");
        assert_eq!(line.cell(1), Some(PreeditCell::Start(1)));
        assert_eq!(line.cluster(1), "x");

        // ZWJ family emoji: one cluster, clamped to two cells.
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
        let line = layout(family, None).unwrap();
        assert_eq!(line.cell(0), Some(PreeditCell::Start(0)));
        assert_eq!(line.cell(1), Some(PreeditCell::Continuation));
        assert_eq!(line.cell(2), None);
        assert_eq!(line.cluster(0), family);
    }

    #[test]
    fn slides_left_at_the_right_edge() {
        // Cursor at col 77 of 80; "日本語" needs 6 cells → starts at 74.
        let line = layout_at("日本語", None, 24, 77).unwrap();
        assert_eq!(line.start_col, 74);
        assert_eq!(line.end_col(), 80);
        // Caret would land at col 80 — clamped to an underline on 語.
        assert_eq!(line.caret, PreeditCaret::OnCell(78));
        // Bottom row, no wrap: the row is exactly the cursor row.
        assert_eq!(line.row, 24);
    }

    #[test]
    fn longer_than_the_row_keeps_the_tail() {
        let text = "あ".repeat(50); // 100 cells wide
        let line = layout_at(&text, None, 0, 10).unwrap();
        // 10 clusters dropped: 40 remain (80 cells), flush to col 0.
        assert_eq!(line.start_col, 0);
        assert_eq!(line.end_col(), 80);
        assert_eq!(line.cluster(0), "あ");
        assert_eq!(line.caret, PreeditCaret::OnCell(78));
    }

    #[test]
    fn caret_inside_composition_is_an_underline() {
        // Caret before 本 (byte offset 3).
        let line = layout("日本語", Some(3)).unwrap();
        assert_eq!(line.caret, PreeditCaret::OnCell(2));
        // A char-boundary offset inside a grapheme cluster (between
        // `e` and its combining mark) marks the cluster being edited:
        // the one CONTAINING the offset, as jamo-level Korean IMEs
        // expect.
        let line = layout("e\u{302}x", Some(1)).unwrap();
        assert_eq!(line.caret, PreeditCaret::OnCell(0));
        // An intra-cluster offset in a wide cluster stays on it.
        let line = layout("日本語", Some(4)).unwrap();
        assert_eq!(line.caret, PreeditCaret::OnCell(2));
    }

    #[test]
    fn korean_intra_syllable_caret_marks_the_syllable() {
        // Jamo-level Korean IMEs report byte offsets inside an NFC
        // syllable while it is being edited; the caret must sit on
        // that syllable, not at the end of the composition.
        let line = layout("한글", Some(1)).unwrap();
        assert_eq!(line.caret, PreeditCaret::OnCell(0));
        let line = layout("한글", Some(4)).unwrap();
        assert_eq!(line.caret, PreeditCaret::OnCell(2));
        // The syllable boundary itself belongs to the next syllable.
        let line = layout("한글", Some(3)).unwrap();
        assert_eq!(line.caret, PreeditCaret::OnCell(2));
        // End-of-text stays a PastEnd beam.
        let line = layout("한글", Some(6)).unwrap();
        assert_eq!(line.caret, PreeditCaret::PastEnd(4));
    }

    #[test]
    fn caret_survives_tail_cropping() {
        let text = "あ".repeat(50);
        // Caret at the very start, which gets dropped: clamps to the
        // first visible cluster.
        let line = layout(&text, Some(0)).unwrap();
        assert_eq!(line.caret, PreeditCaret::OnCell(0));
    }

    #[test]
    fn hidden_caret_draws_no_caret() {
        // Wayland/Windows report a hidden caret while the user pages
        // through candidates; no caret may render then.
        let line = PreeditLine::new("日本語", PreeditCursor::Hidden, 0, 0, 80).unwrap();
        assert_eq!(line.caret, PreeditCaret::Hidden);
        assert_eq!(line.popup_anchor_col(), 5);
        // Cropping keeps it hidden too.
        let long = "あ".repeat(50);
        let line = PreeditLine::new(&long, PreeditCursor::Hidden, 0, 0, 80).unwrap();
        assert_eq!(line.caret, PreeditCaret::Hidden);
    }

    #[test]
    fn empty_and_degenerate_input() {
        assert!(layout("", None).is_none());
        assert!(PreeditLine::new("a", PreeditCursor::Byte(1), 0, 0, 0).is_none());
    }
}
