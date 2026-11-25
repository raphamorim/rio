// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Compressed scrollback: history rows packed into blocks, decoded when read.
//!
//! A block holds up to [`BLOCK_ROWS`] consecutive history rows. Their cells
//! (a packed `u64` each) are split into eight byte planes, every cell's
//! lowest byte, then every cell's next byte, and so on. Splitting is what
//! makes cells compress: a cell's high bytes (style, extras, flags) repeat
//! across a row and from row to row, and its low byte is text. A plane that
//! is one byte throughout (most are: the codepoint's high bytes in ASCII
//! text, the extras id where nothing is linked) is kept as that byte; the
//! others are LZ4 compressed one by one. On real terminal output (logs,
//! diffs, listings) that is 25-30x smaller than the rows, against about 10x
//! for LZ4 over the cells as they are, and constant planes cost nothing to
//! decode (LZ4 rebuilds a run of one byte a byte at a time).
//!
//! A compressed row keeps its place in the ring: its `Row` keeps the row's
//! flags, gives its cell buffer back, and points at its block (an `Arc`, so
//! rows move through the ring as before). Reading one (`&Row` through the
//! storage's `Index`) decodes its block once into the block's cache and
//! hands out that copy; writing one (`IndexMut`) makes it a plain row
//! again. Compressing again later drops the caches reads filled, without
//! re-encoding.
//!
//! The style and extras ids a block's cells carry are kept beside it, so
//! the tables' mark-and-sweep keeps them without decoding anything.

use super::row::{Row, SemanticPrompt};
use crate::crosswords::square::Square;
use std::sync::{Arc, OnceLock};

/// Rows per block. Bigger blocks compress better (cells repeat from row to
/// row) and cost more to decode when one of their rows is read: 256 rows of
/// 120 columns decode in a fraction of a millisecond.
///
/// Tests use small blocks, so small terminals have many of them.
pub const BLOCK_ROWS: usize = if cfg!(test) { 16 } else { 256 };

/// What a compressed row needs besides its cells.
#[derive(Clone, Copy, Debug)]
struct RowMeta {
    len: u32,
    occ: u32,
    kitty_virtual_placeholder: bool,
    has_extras: bool,
    has_styles: bool,
    semantic_prompt: SemanticPrompt,
}

/// One byte plane of a block's cells.
#[derive(Debug)]
enum Plane {
    /// The same byte in every cell.
    Same(u8),
    /// An LZ4 block of the plane.
    Lz4(Box<[u8]>),
}

/// A block's encoded rows and what reading them needs.
#[derive(Debug)]
pub(crate) struct Encoded {
    /// The cells' byte planes, the lowest byte first.
    planes: [Plane; 8],
    /// Cells in the block (the sum of the rows' lengths).
    cells: usize,
    rows: Box<[RowMeta]>,
    /// The non-default style ids and the extras ids its cells carry,
    /// sorted, for the tables' mark-and-sweep.
    pub(crate) styles: Box<[u16]>,
    pub(crate) extras: Box<[u16]>,
}

impl Encoded {
    /// Heap bytes this block keeps.
    fn heap_bytes(&self) -> usize {
        self.planes
            .iter()
            .map(|p| match p {
                Plane::Same(_) => 0,
                Plane::Lz4(b) => b.len(),
            })
            .sum::<usize>()
            + self.rows.len() * std::mem::size_of::<RowMeta>()
            + (self.styles.len() + self.extras.len()) * 2
    }
}

/// A block, shared by the rows in it.
pub(crate) struct Block<T> {
    pub(crate) data: Arc<Encoded>,
    /// The rows, decoded by the first read of any of them.
    cache: OnceLock<Box<[Row<T>]>>,
    decode: fn(&Encoded) -> Box<[Row<T>]>,
}

impl<T> std::fmt::Debug for Block<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Block")
            .field("rows", &self.data.rows.len())
            .field("bytes", &self.data.heap_bytes())
            .field("decoded", &self.cache.get().is_some())
            .finish()
    }
}

impl<T> Block<T> {
    /// The block's rows, decoding them on the first call.
    #[inline]
    pub(crate) fn rows(&self) -> &[Row<T>] {
        self.cache.get_or_init(|| (self.decode)(&self.data))
    }

    /// Cells of the block's `at`th row, without decoding anything.
    pub(crate) fn data_len(&self, at: usize) -> usize {
        self.data.rows[at].len as usize
    }

    /// Whether a read decoded the rows.
    pub(crate) fn is_decoded(&self) -> bool {
        self.cache.get().is_some()
    }

    /// The same block without its decoded rows.
    pub(crate) fn without_cache(&self) -> Block<T> {
        Block {
            data: self.data.clone(),
            cache: OnceLock::new(),
            decode: self.decode,
        }
    }

    /// Heap bytes the encoded block keeps, its decoded rows aside.
    pub(crate) fn encoded_bytes(&self) -> usize {
        self.data.heap_bytes()
    }

    /// Heap bytes of the decoded rows, if a read decoded them.
    pub(crate) fn decoded_bytes(&self) -> usize
    where
        T: Sized,
    {
        self.cache.get().map_or(0, |rows| {
            rows.iter()
                .map(|r| r.inner.capacity() * std::mem::size_of::<T>())
                .sum::<usize>()
                + std::mem::size_of_val::<[Row<T>]>(rows)
        })
    }
}

/// Compress `rows` (up to [`BLOCK_ROWS`], all resident) into one block.
pub(crate) fn encode(rows: &[&Row<Square>]) -> Block<Square> {
    debug_assert!(rows.len() <= BLOCK_ROWS);
    let cells: usize = rows.iter().map(|r| r.inner.len()).sum();
    let mut bits = Vec::with_capacity(cells);
    let mut styles = Vec::new();
    let mut extras = Vec::new();
    for row in rows {
        for sq in &row.inner {
            bits.push(sq.raw());
            if let Some(id) = sq
                .style_id_checked()
                .filter(|&id| id != crate::crosswords::style::DEFAULT_STYLE_ID)
            {
                styles.push(id);
            }
            if let Some(id) = sq.extras_id_checked() {
                extras.push(id);
            }
        }
    }
    // A plane at a time: one sequential pass each.
    let mut plane = vec![0u8; cells];
    let planes = std::array::from_fn(|b| {
        for (byte, cell) in plane.iter_mut().zip(&bits) {
            *byte = (cell >> (8 * b)) as u8;
        }
        match plane.first() {
            Some(&first) if plane.iter().all(|&x| x == first) => Plane::Same(first),
            None => Plane::Same(0),
            _ => Plane::Lz4(lz4_flex::block::compress(&plane).into_boxed_slice()),
        }
    });
    for ids in [&mut styles, &mut extras] {
        ids.sort_unstable();
        ids.dedup();
    }
    let meta = rows
        .iter()
        .map(|r| RowMeta {
            len: r.inner.len() as u32,
            occ: r.occ as u32,
            kitty_virtual_placeholder: r.kitty_virtual_placeholder,
            has_extras: r.has_extras,
            has_styles: r.has_styles,
            semantic_prompt: r.semantic_prompt,
        })
        .collect();
    Block {
        data: Arc::new(Encoded {
            planes,
            cells,
            rows: meta,
            styles: styles.into_boxed_slice(),
            extras: extras.into_boxed_slice(),
        }),
        cache: OnceLock::new(),
        decode: decode_squares,
    }
}

#[cfg(test)]
thread_local! {
    /// Blocks decoded on this thread, for tests.
    pub(crate) static DECODES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn decode_squares(data: &Encoded) -> Box<[Row<Square>]> {
    #[cfg(test)]
    DECODES.with(|d| d.set(d.get() + 1));
    let cells = data.cells;
    // The constant planes at once, then the others a plane at a time.
    let same = data
        .planes
        .iter()
        .enumerate()
        .fold(0u64, |acc, (b, p)| match p {
            Plane::Same(byte) => acc | (*byte as u64) << (8 * b),
            Plane::Lz4(_) => acc,
        });
    let mut bits = vec![same; cells];
    let mut plane = vec![0u8; cells];
    for (b, p) in data.planes.iter().enumerate() {
        let Plane::Lz4(lz4) = p else { continue };
        // Encoded here from this size; anything else is memory corruption.
        let n = lz4_flex::block::decompress_into(lz4, &mut plane)
            .expect("a compressed history block decodes");
        assert_eq!(n, cells, "a compressed history block's size");
        for (cell, &byte) in bits.iter_mut().zip(&plane) {
            *cell |= (byte as u64) << (8 * b);
        }
    }
    let mut at = 0;
    data.rows
        .iter()
        .map(|m| {
            let inner = bits[at..at + m.len as usize]
                .iter()
                .map(|&b| Square::from_raw(b))
                .collect();
            at += m.len as usize;
            Row {
                inner,
                occ: m.occ as usize,
                kitty_virtual_placeholder: m.kitty_virtual_placeholder,
                has_extras: m.has_extras,
                has_styles: m.has_styles,
                semantic_prompt: m.semantic_prompt,
                // Read through a block, a row is always worth copying.
                dirty: true,
                packed: None,
                packed_at: 0,
            }
        })
        .collect()
}

/// How much of a grid's history is compressed, and what it takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HistoryMemory {
    /// History rows held compressed.
    pub compressed_rows: usize,
    /// History rows held as plain rows.
    pub plain_rows: usize,
    /// Heap bytes of the compressed blocks.
    pub compressed_bytes: usize,
    /// Heap bytes of blocks decoded by reads, until the next compression.
    pub decoded_bytes: usize,
}

#[cfg(test)]
mod tests {
    use crate::ansi::CursorShape;
    use crate::crosswords::formatter::FormatOptions;
    use crate::crosswords::grid::{Dimensions, Scroll};
    use crate::crosswords::pos::{Column, Line};
    use crate::crosswords::{Crosswords, CrosswordsSize};
    use crate::event::{VoidListener, WindowId};
    use crate::performer::handler::Processor;

    struct T {
        term: Crosswords<VoidListener>,
        parser: Processor,
    }

    impl T {
        fn new(cols: usize, rows: usize, history: usize) -> T {
            let size = CrosswordsSize::new(cols, rows);
            T {
                term: Crosswords::new(
                    size,
                    CursorShape::Block,
                    VoidListener,
                    WindowId::from(0),
                    0,
                    history,
                ),
                parser: Processor::default(),
            }
        }
        fn feed(&mut self, bytes: &[u8]) {
            self.parser.advance(&mut self.term, bytes);
        }
        /// Everything readable: the snapshot (history, both screens, styles,
        /// links, state) with the palette, and the text.
        fn read(&self) -> (String, String) {
            (
                self.term
                    .format(FormatOptions::snapshot().with_palette(true)),
                self.term
                    .format(FormatOptions::plain().with_scrollback(true)),
            )
        }
    }

    /// Lines with what rows carry: styles, links, wide characters, soft
    /// wraps, prompt marks, combining marks.
    fn output(lines: usize) -> Vec<u8> {
        let mut out = Vec::new();
        for i in 0..lines {
            let line = match i % 6 {
                0 => format!("\x1b]133;A\x07$ cmd {i}\x1b]133;B\x07\r\n"),
                1 => format!(
                    "\x1b[1;3{}mstyled {i}\x1b[0m plain \x1b[48;5;{}mbg\x1b[0m\r\n",
                    i % 8,
                    i % 256
                ),
                2 => format!(
                    "see \x1b]8;;https://x.dev/{i}\x1b\\link {i}\x1b]8;;\x1b\\ end\r\n"
                ),
                3 => format!("wide \u{754c}\u{9762} e\u{301} {i}\r\n"),
                4 => format!("{}\r\n", "wrapped-".repeat(6)),
                _ => format!("\x1b[38;2;{};20;30mrgb {i}\x1b[0m\r\n", i % 256),
            };
            out.extend_from_slice(line.as_bytes());
        }
        out
    }

    #[test]
    fn compressing_history_changes_nothing_you_can_read() {
        let mut a = T::new(30, 5, 2_000);
        a.feed(&output(1_200));
        let before = a.read();
        assert!(!a.term.compress_history(None));
        let m = a.term.history_memory();
        assert!(m.compressed_rows >= 1_000, "{m:?}");
        assert!(m.plain_rows < super::BLOCK_ROWS, "{m:?}");
        assert_eq!(a.read(), before);
        // Styles and links of a compressed row read through the tables.
        let g = &a.term.grid;
        let row = &g[Line(-(g.history_size() as i32) + 1)];
        assert!(g.raw.rows().any(|r| r.is_compressed()));
        let _ = (g.style_of(&row[Column(0)]), row.len());
    }

    #[test]
    fn sweeping_the_tables_keeps_what_compressed_rows_use() {
        // Styles and links used only in compressed rows: a sweep must not
        // free them (it reads the block's ids instead of its cells).
        let mut a = T::new(30, 5, 2_000);
        a.feed(&output(600));
        a.term.compress_history(None);
        let before = a.read();
        // Plain screens of new, unstyled output, and sweeps.
        for _ in 0..3 {
            a.feed(b"\x1b[2J\x1b[H");
            a.term.grid.reclaim_styles();
            a.term.grid.reclaim_extras();
        }
        let mut b = T::new(30, 5, 2_000);
        b.feed(&output(600));
        b.feed(b"\x1b[2J\x1b[H".repeat(3).as_slice());
        assert_eq!(a.read(), b.read());
        let _ = before;
    }

    #[test]
    fn scrolling_past_a_full_history_drops_compressed_rows_undecoded() {
        // A full history: rows leaving it stay in the ring's spare rows,
        // and come back as new lines once the ring wraps. Compressed ones
        // are blanked then, not decoded.
        let mut a = T::new(30, 5, 600);
        let mut b = T::new(30, 5, 600);
        a.feed(&output(700));
        b.feed(&output(700));
        let decodes = || super::DECODES.with(|d| d.get());
        for _ in 0..4 {
            a.term.compress_history(None);
            assert!(a.term.history_memory().compressed_rows > 0);
            let before = decodes();
            a.feed(&output(900));
            b.feed(&output(900));
            assert_eq!(decodes(), before, "scrolling decoded a block");
        }
        assert_eq!(a.read(), b.read());
    }

    #[test]
    fn reads_decode_and_compressing_again_drops_the_copies() {
        let mut a = T::new(30, 5, 2_000);
        a.feed(&output(1_000));
        a.term.compress_history(None);
        let m = a.term.history_memory();
        assert_eq!(m.decoded_bytes, 0);
        let _ = a.read();
        let read = a.term.history_memory();
        assert!(read.decoded_bytes > 0, "{read:?}");
        assert_eq!(read.compressed_bytes, m.compressed_bytes);
        assert!(!a.term.compress_history(None));
        assert_eq!(a.term.history_memory(), m);
    }

    #[test]
    fn writing_a_compressed_row_makes_it_plain() {
        let mut a = T::new(30, 5, 2_000);
        let mut b = T::new(30, 5, 2_000);
        a.feed(&output(600));
        b.feed(&output(600));
        a.term.compress_history(None);
        let top = Line(-(a.term.grid.history_size() as i32));
        a.term.grid[top][Column(0)].set_c('Z');
        b.term.grid[top][Column(0)].set_c('Z');
        assert!(!a
            .term
            .grid
            .raw
            .rows()
            .all(|r| r.is_compressed() || r.inner.is_empty()));
        assert_eq!(a.read(), b.read());
        // Compressing again packs it with the rows around it.
        a.term.compress_history(None);
        assert_eq!(a.read(), b.read());
    }

    #[test]
    fn a_column_resize_decompresses_then_reflows_the_same() {
        let mut a = T::new(30, 5, 2_000);
        let mut b = T::new(30, 5, 2_000);
        a.feed(&output(800));
        b.feed(&output(800));
        a.term.compress_history(None);
        for cols in [22, 41, 30] {
            a.term.resize(CrosswordsSize::new(cols, 5));
            b.term.resize(CrosswordsSize::new(cols, 5));
            assert_eq!(a.term.history_memory().compressed_rows, 0);
            assert_eq!(a.read(), b.read(), "{cols} columns");
            a.term.compress_history(None);
        }
    }

    #[test]
    fn the_viewport_stays_plain_and_work_comes_in_steps() {
        let mut a = T::new(30, 5, 4_000);
        a.feed(&output(3_000));
        a.term.scroll_display(Scroll::Delta(40));
        // One block per call, then done.
        let mut calls = 0;
        while a.term.compress_history(Some(1)) {
            calls += 1;
            assert!(calls < 10_000);
        }
        assert!(calls >= 5, "{calls}");
        let g = &a.term.grid;
        let offset = g.display_offset() as i32;
        for line in -offset..0 {
            let idx_row = &g[Line(line)];
            assert!(!idx_row.inner.is_empty(), "line {line} on screen");
        }
        let m = a.term.history_memory();
        assert!(m.plain_rows >= 40, "{m:?}");
    }

    #[test]
    fn scrolling_back_through_the_renderer_leaves_rows_compressed() {
        use crate::event::TerminalDamage;
        let mut a = T::new(30, 10, 2_000);
        a.feed(&output(1_200));
        a.term.compress_history(None);
        let before = a.term.history_memory();
        assert!(before.compressed_rows > 1_000, "{before:?}");
        let (mut rows, mut styles) = (Vec::new(), Vec::new());
        let mut extras = rustc_hash::FxHashMap::default();
        let mut text = Vec::new();
        for i in 0..60 {
            a.term.scroll_display(Scroll::Delta(10));
            // Full frames, and frames that copy only dirty rows.
            let damage = if i % 2 == 0 {
                TerminalDamage::Full
            } else {
                TerminalDamage::Noop
            };
            a.term
                .snapshot_visible(&damage, &mut rows, &mut styles, &mut extras);
            text.push(rows.iter().map(|r| r[Column(0)].c()).collect::<String>());
        }
        let after = a.term.history_memory();
        assert_eq!(after.compressed_rows, before.compressed_rows, "{after:?}");
        // Each frame showed its rows.
        assert!(
            text.iter().all(|t| t.chars().all(|c| c != '\0')),
            "{text:?}"
        );
        assert_ne!(text[0], text[2]);
    }

    #[test]
    fn typical_output_compresses_well() {
        let mut a = T::new(100, 40, 10_000);
        let mut data = Vec::new();
        for i in 0..10_100 {
            data.extend_from_slice(format!("\x1b[32m{i:06}\x1b[0m some typical output from a build tool or an agent transcript\r\n").as_bytes());
        }
        a.feed(&data);
        let rows_bytes = a.term.grid.history_size() * 100 * 8;
        a.term.compress_history(None);
        let m = a.term.history_memory();
        // (16-row test blocks; 256-row ones do about 3x better.)
        assert!(
            m.compressed_bytes * 15 < rows_bytes,
            "{m:?} for {rows_bytes} bytes of rows"
        );
    }

    /// Random output, compression, resizes, scrolling back and screen
    /// switches on one terminal; the same without compression on another:
    /// they read the same throughout.
    #[test]
    fn random_compression_reads_the_same() {
        let chunks: Vec<Vec<u8>> = vec![
            output(37),
            output(300),
            b"\x1b[?1049h\x1b[Halt screen\r\nmore".to_vec(),
            b"\x1b[?1049l".to_vec(),
            b"\x1b[2;4r\x1b[4;1H\n\n\n\x1b[r".to_vec(),
            b"\x1b[3J".to_vec(),
            "\u{754c}".repeat(40).into_bytes(),
            b"\x1b[5S\x1b[2T".to_vec(),
        ];
        let mut x: u64 = 0x2545_f491_4f6c_dd1d;
        let mut rnd = |n: usize| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x % n as u64) as usize
        };
        for seed in 0..40 {
            let (cols, rows) = (8 + rnd(40), 3 + rnd(10));
            let history = 200 + rnd(2_000);
            let mut a = T::new(cols, rows, history);
            let mut b = T::new(cols, rows, history);
            for step in 0..30 {
                match rnd(10) {
                    0 | 1 => {
                        let max = [None, Some(1), Some(3)][rnd(3)];
                        a.term.compress_history(max);
                    }
                    2 => {
                        let (c, r) = (8 + rnd(40), 3 + rnd(10));
                        a.term.resize(CrosswordsSize::new(c, r));
                        b.term.resize(CrosswordsSize::new(c, r));
                    }
                    3 => {
                        let scroll = [
                            Scroll::Delta(5),
                            Scroll::Top,
                            Scroll::Bottom,
                            Scroll::PageUp,
                        ][rnd(4)];
                        a.term.scroll_display(scroll);
                        b.term.scroll_display(scroll);
                    }
                    _ => {
                        let chunk = &chunks[rnd(chunks.len())];
                        a.feed(chunk);
                        b.feed(chunk);
                    }
                }
                assert_eq!(a.read(), b.read(), "seed {seed} step {step}");
                assert_eq!(a.term.grid.display_offset(), b.term.grid.display_offset());
            }
        }
    }
}
