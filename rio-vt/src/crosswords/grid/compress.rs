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

use super::cells::SlabCursor;
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

/// Where [`Storage::compress_from`](super::storage::Storage) stopped at
/// its limit: the ring it walked, and how far. Valid while the ring is
/// unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Resume {
    pub(crate) ring: (usize, usize, usize, usize),
    pub(crate) k: usize,
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
        if let Some(rows) = self.cache.get() {
            return rows;
        }
        if let Some(rows) = transient_rows(self) {
            return rows;
        }
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

    /// How many rows were encoded in it.
    pub(crate) fn len(&self) -> usize {
        self.data.rows.len()
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

/// Buffers the encoder and decoder work in, kept per thread: freed after
/// each block, a block's worth (a quarter of a megabyte at 120 columns)
/// would stay with the allocator.
#[derive(Default)]
struct Scratch {
    bits: Vec<u64>,
    plane: Vec<u8>,
    lz4: Vec<u8>,
}

thread_local! {
    static SCRATCH: std::cell::RefCell<Scratch> = std::cell::RefCell::new(Scratch::default());
}

/// Compress `rows` (up to [`BLOCK_ROWS`], all resident) into one block.
pub(crate) fn encode(rows: &[&Row<Square>]) -> Block<Square> {
    SCRATCH.with(|scratch| encode_with(rows, &mut scratch.borrow_mut()))
}

fn encode_with(rows: &[&Row<Square>], scratch: &mut Scratch) -> Block<Square> {
    debug_assert!(rows.len() <= BLOCK_ROWS);
    let cells: usize = rows.iter().map(|r| r.inner.len()).sum();
    let Scratch { bits, plane, lz4 } = scratch;
    bits.clear();
    bits.reserve(cells);
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
    plane.clear();
    plane.resize(cells, 0);
    lz4.clear();
    lz4.resize(lz4_flex::block::get_maximum_output_size(cells), 0);
    let planes = std::array::from_fn(|b| {
        for (byte, cell) in plane.iter_mut().zip(bits.iter()) {
            *byte = (cell >> (8 * b)) as u8;
        }
        match plane.first() {
            Some(&first) if plane.iter().all(|&x| x == first) => Plane::Same(first),
            None => Plane::Same(0),
            _ => {
                let n = lz4_flex::block::compress_into(plane, lz4)
                    .expect("room for any block's worst case");
                Plane::Lz4(Box::from(&lz4[..n]))
            }
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

/// Blocks a bulk read on this thread decoded for itself (see
/// [`transient_reads`]), each a `Box<Box<[Row<T>]>>` as a thin pointer with
/// what drops it, keyed by the block.
/// One block's rows in the scratch: the block, the rows, what drops them.
type Decoded = (*const (), *mut (), unsafe fn(*mut ()));

#[derive(Default)]
struct Transient {
    depth: usize,
    decoded: Vec<Decoded>,
    /// The last one looked up: a read stays in a block for its rows.
    last: usize,
}

thread_local! {
    static TRANSIENT: std::cell::RefCell<Transient> = std::cell::RefCell::default();
}

unsafe fn drop_transient<T>(rows: *mut ()) {
    // SAFETY: made by `transient_rows::<T>` from a `Box<Box<[Row<T>]>>`,
    // dropped once (the list is taken as the outermost scope ends).
    drop(unsafe { Box::from_raw(rows as *mut Box<[Row<T>]>) });
}

/// Run `f` with this thread's reads of compressed history (`Block::rows`)
/// decoding blocks no one decoded yet into a scratch of its own, freed
/// when the outermost such call returns, instead of into the blocks'
/// caches: a search over the history or a snapshot of it leaves nothing
/// decoded behind (and asks for no compression pass to drop it). Blocks
/// decoded already (the viewport's) are read as they are, and a block is
/// decoded once per call however often it's read.
///
/// # Safety
///
/// No reference into a grid obtained in `f` may be used after it returns:
/// rows it reads may live in the scratch. Its callers return owned values
/// (text, bytes, positions) and keep no reference past it.
pub(crate) unsafe fn transient_reads<R>(f: impl FnOnce() -> R) -> R {
    struct Scope;
    impl Drop for Scope {
        fn drop(&mut self) {
            let decoded = TRANSIENT.with(|t| {
                let mut t = t.borrow_mut();
                t.depth -= 1;
                if t.depth > 0 {
                    return Vec::new();
                }
                t.last = 0;
                std::mem::take(&mut t.decoded)
            });
            for (_, rows, drop) in decoded {
                // SAFETY: each made once by `transient_rows`, and no
                // reference to it outlives the scope (the caller's word).
                unsafe { drop(rows) };
            }
        }
    }
    TRANSIENT.with(|t| t.borrow_mut().depth += 1);
    let _scope = Scope;
    f()
}

/// `block`'s rows from this thread's bulk-read scratch, decoding them into
/// it if need be; `None` outside [`transient_reads`].
#[cold]
fn transient_rows<T>(block: &Block<T>) -> Option<&[Row<T>]> {
    TRANSIENT.with(|t| {
        let mut t = t.borrow_mut();
        if t.depth == 0 {
            return None;
        }
        let key = block as *const Block<T> as *const ();
        let last = t.last;
        let found = if t.decoded.get(last).is_some_and(|e| e.0 == key) {
            Some(last)
        } else {
            t.decoded.iter().rposition(|e| e.0 == key)
        };
        let at = found.unwrap_or_else(|| {
            let rows: Box<Box<[Row<T>]>> = Box::new((block.decode)(&block.data));
            t.decoded
                .push((key, Box::into_raw(rows) as *mut (), drop_transient::<T>));
            t.decoded.len() - 1
        });
        t.last = at;
        let rows = t.decoded[at].1 as *const Box<[Row<T>]>;
        // SAFETY: the rows stay where they are (behind their own box)
        // until the outermost scope ends, which outlives every use of what
        // `f` read (`transient_reads`'s contract). The key is a live block:
        // the grid it's in is borrowed for the whole read, so no other
        // block takes its address meanwhile.
        Some(unsafe { &**rows })
    })
}

#[cfg(test)]
thread_local! {
    /// Blocks decoded on this thread, for tests.
    pub(crate) static DECODES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn decode_squares(data: &Encoded) -> Box<[Row<Square>]> {
    #[cfg(test)]
    DECODES.with(|d| d.set(d.get() + 1));
    SCRATCH.with(|scratch| {
        let Scratch { bits, plane, .. } = &mut *scratch.borrow_mut();
        decode_with(data, bits, plane)
    })
}

fn decode_with(
    data: &Encoded,
    bits: &mut Vec<u64>,
    plane: &mut Vec<u8>,
) -> Box<[Row<Square>]> {
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
    bits.clear();
    bits.resize(cells, same);
    plane.clear();
    plane.resize(cells, 0);
    for (b, p) in data.planes.iter().enumerate() {
        let Plane::Lz4(lz4) = p else { continue };
        // Encoded here from this size; anything else is memory corruption.
        let n = lz4_flex::block::decompress_into(lz4, plane)
            .expect("a compressed history block decodes");
        assert_eq!(n, cells, "a compressed history block's size");
        for (cell, &byte) in bits.iter_mut().zip(plane.iter()) {
            *cell |= (byte as u64) << (8 * b);
        }
    }
    // In a slab of the block's own, unmapped with the decoded copy:
    // reading history leaves nothing with the allocator.
    let mut slabs = SlabCursor::default();
    let mut at = 0;
    data.rows
        .iter()
        .map(|m| {
            let mut inner = slabs.cells(m.len as usize);
            for (cell, &b) in inner.iter_mut().zip(&bits[at..at + m.len as usize]) {
                *cell = Square::from_raw(b);
            }
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
        /// Every history row read through the grid, as the viewport's are
        /// (into the blocks' caches).
        fn read_rows(&self) {
            let g = &self.term.grid;
            for line in -(g.history_size() as i32)..0 {
                let _ = g[Line(line)][Column(0)].c();
            }
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

    /// Every row slot of the grids in slabs: no row has a buffer of its own.
    fn all_in_slabs(t: &T) -> bool {
        [&t.term.grid, &t.term.inactive_grid]
            .iter()
            .all(|g| g.raw.rows().all(|r| r.is_compressed() || r.inner.in_slab()))
    }

    #[test]
    fn compressed_history_gives_its_slabs_back() {
        use crate::crosswords::grid::cells::{live_slabs, SLAB_ROWS};
        let base = live_slabs();
        let mut a = T::new(30, 5, 2_000);
        a.feed(&output(1_200));
        assert!(all_in_slabs(&a));
        let filled = live_slabs() - base;
        assert!(filled >= 1_200 / SLAB_ROWS, "{filled}");
        // What isn't compressed keeps its slabs: the screen's, the newest
        // rows short of a block, and the alt screen's.
        a.term.compress_history(None);
        let kept = live_slabs() - base;
        assert!(kept <= 5, "{kept} of {filled} slabs kept");
        // Output recycling compressed rows takes their cells from slabs in
        // order, which compressing them again gives back.
        a.feed(&output(1_200));
        assert!(all_in_slabs(&a));
        a.term.compress_history(None);
        assert!(live_slabs() - base <= kept + 1, "{}", live_slabs() - base);
        // A read through the caches (the viewport's) decodes blocks into
        // slabs of their own, unmapped by the next compression; a bulk
        // read's go when it's done.
        let read = a.read();
        assert!(live_slabs() - base <= kept + 1);
        a.read_rows();
        assert!(live_slabs() - base > kept + 1);
        a.term.compress_history(None);
        assert!(live_slabs() - base <= kept + 1);
        assert_eq!(a.read(), read);
    }

    #[test]
    fn a_column_resize_keeps_the_rows_in_slabs() {
        use crate::crosswords::grid::cells::live_slabs;
        // (That it reads the same: `a_column_resize_decompresses_then_
        // reflows_the_same`.)
        let base = live_slabs();
        let mut a = T::new(30, 5, 2_000);
        a.feed(&output(600));
        a.term.compress_history(None);
        let compressed = live_slabs() - base;
        for cols in [45, 20, 33, 33, 70] {
            a.term.resize(CrosswordsSize::new(cols, 5));
            assert!(all_in_slabs(&a), "{cols} columns");
            // The old width's slabs went with their rows.
            a.term.compress_history(None);
            let kept = live_slabs() - base;
            assert!(
                kept <= compressed + 2,
                "{cols} columns: {kept} slabs, {compressed} before"
            );
        }
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
        a.read_rows();
        let read = a.term.history_memory();
        assert!(read.decoded_bytes > 0, "{read:?}");
        assert_eq!(read.compressed_bytes, m.compressed_bytes);
        assert!(!a.term.compress_history(None));
        assert_eq!(a.term.history_memory(), m);
    }

    /// The steps left nothing for a pass over the whole ring to do.
    fn assert_nothing_left(a: &mut T, what: &str) {
        let m = a.term.history_memory();
        a.term.grid.forget_compress_resume();
        assert!(!a.term.compress_history(None));
        assert_eq!(a.term.history_memory(), m, "{what}");
    }

    #[test]
    fn a_resize_in_the_middle_of_a_pass_leaves_no_rows_behind() {
        for cols in [45, 20] {
            let mut a = T::new(30, 5, 4_000);
            // Past the limit: no spare rows, the oldest lines go first.
            a.feed(&output(5_000));
            // A grow with nothing soft-wrapped keeps the ring's shape.
            let short: String = (0..5_000).map(|i| format!("line {i}\r\n")).collect();
            a.feed(short.as_bytes());
            for _ in 0..4 {
                assert!(a.term.compress_history(Some(1)));
            }
            a.term.resize(CrosswordsSize::new(cols, 5));
            let mut calls = 0;
            while a.term.compress_history(Some(1)) {
                calls += 1;
                assert!(calls < 10_000);
            }
            assert_nothing_left(&mut a, &format!("{cols} columns"));
        }
    }

    #[test]
    fn writing_a_history_row_in_the_middle_of_a_pass_leaves_no_rows_behind() {
        let mut a = T::new(30, 5, 4_000);
        a.feed(&output(5_000));
        for _ in 0..4 {
            assert!(a.term.compress_history(Some(1)));
        }
        // The oldest rows, compressed by the steps so far, made plain.
        let top = -(a.term.grid.history_size() as i32);
        for line in top..top + 20 {
            let _ = &mut a.term.grid[Line(line)];
        }
        while a.term.compress_history(Some(1)) {}
        assert_nothing_left(&mut a, "written");
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
    fn a_pass_in_steps_keeps_the_block_above_the_viewport_decoded() {
        let mut a = T::new(30, 5, 2_000);
        a.feed(&output(1_200));
        a.term.compress_history(None);
        // What a frame reads past the viewport's top (a search running
        // into it): decoded.
        let _ = a.term.grid[Line(-1)][Column(0)].c();
        let read = a.term.history_memory().decoded_bytes;
        assert!(read > 0);
        while a.term.compress_history(Some(1)) {}
        // Not dropped for the next frame to decode again.
        assert_eq!(a.term.history_memory().decoded_bytes, read);
        // A whole pass (an embedder's) drops it.
        a.term.compress_history(None);
        assert_eq!(a.term.history_memory().decoded_bytes, 0);
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

    fn decodes() -> usize {
        super::DECODES.with(|d| d.get())
    }

    impl T {
        /// History rows held compressed.
        fn packed(&self) -> usize {
            self.term.history_memory().compressed_rows
        }
        fn resize(&mut self, cols: usize, rows: usize) {
            self.term.resize(CrosswordsSize::new(cols, rows));
        }
    }

    /// Two terminals fed the same `output(lines)`, the first compressed.
    fn twins(cols: usize, rows: usize, history: usize, lines: usize) -> (T, T) {
        let (mut a, mut b) = (T::new(cols, rows, history), T::new(cols, rows, history));
        a.feed(&output(lines));
        b.feed(&output(lines));
        assert!(!a.term.compress_history(None));
        assert!(a.packed() > 0);
        (a, b)
    }

    /// Feed `bytes` to both and check they read the same.
    fn both(a: &mut T, b: &mut T, bytes: &[u8]) {
        a.feed(bytes);
        b.feed(bytes);
        assert_eq!(
            a.read(),
            b.read(),
            "after {:?}",
            String::from_utf8_lossy(bytes)
        );
    }

    #[test]
    fn measuring_and_scrolling_decode_nothing() {
        let (mut a, _) = twins(30, 5, 2_000, 800);
        let before = decodes();
        let m = a.term.history_memory();
        a.term.scroll_display(Scroll::Top);
        a.term.scroll_display(Scroll::Bottom);
        assert_eq!(a.term.history_memory(), m);
        assert_eq!(decodes(), before);
    }

    #[test]
    fn compressing_again_is_idempotent_and_steps_add_up() {
        let (mut a, _) = twins(30, 5, 3_000, 2_000);
        let mut b = T::new(30, 5, 3_000);
        b.feed(&output(2_000));
        while b.term.compress_history(Some(2)) {}
        // The same blocks, however the work was split.
        assert_eq!(a.term.history_memory(), b.term.history_memory());
        let before = decodes();
        assert!(!a.term.compress_history(None));
        assert!(!a.term.compress_history(Some(1)));
        assert_eq!(decodes(), before);
        assert_eq!(a.term.history_memory(), b.term.history_memory());
        assert_eq!(a.read(), b.read());
    }

    #[test]
    fn compressing_new_history_leaves_older_blocks_undecoded() {
        let (mut a, mut b) = twins(30, 5, 3_000, 400);
        let first = a.packed();
        let before = decodes();
        a.feed(&output(300));
        b.feed(&output(300));
        a.term.compress_history(None);
        // The history's blocks stay as they are. New rows took over most
        // of a block of the spare rows past it: that one is decoded to
        // be compressed again, without the rows it lost.
        assert!(decodes() - before <= 1, "{}", decodes() - before);
        assert!(a.packed() > first);
        assert!(a.term.history_memory().plain_rows < super::BLOCK_ROWS);
        assert_eq!(a.read(), b.read());
    }

    #[test]
    fn editing_the_screen_leaves_history_compressed() {
        let (mut a, mut b) = twins(30, 8, 2_000, 600);
        let packed = a.packed();
        let edits: [&[u8]; 10] = [
            b"\x1b[H\x1b[2K",
            b"\x1b[3;5H\x1b[3X",
            b"\x1b[2;1H\x1b[2L",
            b"\x1b[4;1H\x1b[M",
            b"\x1b[1;3H\x1b[2@",
            b"\x1b[1;4H\x1b[2P",
            b"\x1b[6;1H\x1b[J",
            b"\x1b[2;3H\x1b[1J",
            b"\x1b[3;6r\x1b[3;1H\x1b[2M\x1b[4S\x1b[2T\x1b[r",
            b"\x1b#8",
        ];
        for edit in edits {
            both(&mut a, &mut b, edit);
            assert_eq!(a.packed(), packed, "{:?}", String::from_utf8_lossy(edit));
        }
    }

    #[test]
    fn clearing_history_and_resetting_after_compression() {
        for clear in [&b"\x1b[3J"[..], b"\x1bc"] {
            let (mut a, mut b) = twins(30, 5, 2_000, 700);
            both(&mut a, &mut b, clear);
            assert_eq!(a.term.grid.history_size(), 0);
            // The cleared rows come back as new lines, blank.
            both(&mut a, &mut b, &output(1_500));
            a.term.compress_history(None);
            assert_eq!(a.read(), b.read());
        }
    }

    #[test]
    fn growing_rows_shows_compressed_history_without_decoding() {
        let (mut a, mut b) = twins(30, 5, 2_000, 800);
        let before = decodes();
        for rows in [12, 25] {
            a.resize(30, rows);
            b.resize(30, rows);
        }
        assert_eq!(decodes(), before);
        assert_eq!(a.read(), b.read());
        // Writing over, scrolling and erasing the rows pulled on screen.
        both(
            &mut a,
            &mut b,
            b"\x1b[1;1Hover\x1b[3;2H\x1b[K\x1b[2T\x1b[3S\x1b[5;1H\x1b[2M",
        );
        // The renderer's snapshot of them, then shrinking them back.
        let snap = |t: &mut T| {
            let (mut rows, mut styles) = (Vec::new(), Vec::new());
            let mut extras = rustc_hash::FxHashMap::default();
            t.term.snapshot_visible(
                &crate::event::TerminalDamage::Full,
                &mut rows,
                &mut styles,
                &mut extras,
            );
            rows
        };
        assert!(snap(&mut a) == snap(&mut b));
        for rows in [9, 5] {
            a.resize(30, rows);
            b.resize(30, rows);
            assert_eq!(a.read(), b.read(), "{rows} rows");
            a.term.compress_history(None);
        }
        both(&mut a, &mut b, &output(50));
    }

    #[test]
    fn resizing_while_scrolled_back_keeps_the_same_viewport() {
        let (mut a, mut b) = twins(30, 6, 2_000, 800);
        a.term.scroll_display(Scroll::Delta(100));
        b.term.scroll_display(Scroll::Delta(100));
        for (cols, rows) in [(30, 10), (24, 10), (40, 4), (17, 7), (30, 6)] {
            a.resize(cols, rows);
            b.resize(cols, rows);
            assert_eq!(a.term.grid.display_offset(), b.term.grid.display_offset());
            assert_eq!(a.read(), b.read(), "{cols}x{rows}");
            a.term.compress_history(None);
        }
    }

    #[test]
    fn a_scrolled_back_viewport_stays_pinned_over_compressed_history() {
        // A full history: new output evicts compressed rows while the
        // viewport stays on the same lines.
        let (mut a, mut b) = twins(30, 5, 700, 900);
        a.term.scroll_display(Scroll::Delta(50));
        b.term.scroll_display(Scroll::Delta(50));
        a.term.compress_history(None);
        let before = decodes();
        a.feed(&output(200));
        b.feed(&output(200));
        assert_eq!(decodes(), before);
        assert_eq!(a.term.grid.display_offset(), b.term.grid.display_offset());
        assert_eq!(a.read(), b.read());
    }

    #[test]
    fn the_primary_history_compresses_under_the_alt_screen() {
        let mut a = T::new(30, 5, 2_000);
        let mut b = T::new(30, 5, 2_000);
        both(&mut a, &mut b, &output(600));
        both(&mut a, &mut b, b"\x1b[?1049h\x1b[Hfull screen app");
        assert!(!a.term.compress_history(None));
        let packed = a.packed();
        assert!(packed > 500, "{packed}");
        both(&mut a, &mut b, b"\x1b[2J\x1b[Hredraw");
        assert_eq!(a.packed(), packed);
        // A column resize under it reflows the primary screen too.
        a.resize(26, 5);
        b.resize(26, 5);
        assert_eq!(a.read(), b.read());
        a.term.compress_history(None);
        both(&mut a, &mut b, b"\x1b[?1049l");
        both(&mut a, &mut b, &output(40));
    }

    #[test]
    fn styles_links_and_marks_survive_id_reuse_after_compression() {
        // Many novel styles and links after compression, with sweeps in
        // between: the ids freed and reused must not be the ones the
        // compressed rows hold.
        let (mut a, mut b) = twins(30, 5, 3_000, 600);
        for round in 0..4 {
            let mut s = String::new();
            for i in 0..600 {
                s += &format!("\x1b[38;2;{round};{};{}m#", i % 256, i / 256);
            }
            s += &format!(
                "\x1b[0m\x1b]8;;https://y.dev/{round}\x1b\\L\x1b]8;;\x1b\\ a\u{302}\x1b[2J\x1b[H"
            );
            for t in [&mut a, &mut b] {
                t.feed(s.as_bytes());
                t.term.grid.reclaim_styles();
                t.term.grid.reclaim_extras();
            }
        }
        assert!(a.packed() > 500);
        assert_eq!(a.read(), b.read());
    }

    /// Every match of `re`, left to right from the top of the history.
    fn matches(t: &T, re: &str) -> Vec<crate::crosswords::search::Match> {
        use crate::crosswords::{Boundary, Direction, Pos, Side};
        let mut regex = crate::crosswords::search::RegexSearch::new(re).unwrap();
        let g = &t.term.grid;
        let mut origin = Pos::new(g.topmost_line(), Column(0));
        let mut out: Vec<crate::crosswords::search::Match> = Vec::new();
        while let Some(m) =
            t.term
                .search_next(&mut regex, origin, Direction::Right, Side::Left, None)
        {
            if out.last().is_some_and(|last| m.start() <= last.start()) {
                break;
            }
            origin = m.end().add(g, Boundary::None, 1);
            out.push(m);
        }
        out
    }

    #[test]
    fn searching_compressed_history_finds_the_same_matches() {
        let (a, b) = twins(30, 5, 2_000, 700);
        let packed = a.packed();
        // Soft-wrapped runs span rows, and so block boundaries.
        for re in [
            "wrapped-wrapped",
            "link \\d+",
            "\u{754c}\u{9762}",
            "rgb 6\\d9",
        ] {
            let found = matches(&a, re);
            assert!(!found.is_empty(), "{re}");
            assert_eq!(found, matches(&b, re), "{re}");
        }
        assert_eq!(a.packed(), packed);
    }

    #[test]
    fn bulk_reads_leave_the_history_undecoded() {
        use crate::crosswords::{Pos, Side};
        use crate::selection::{Selection, SelectionType};
        let (mut a, mut b) = twins(30, 5, 2_000, 1_200);
        let m = a.term.history_memory();
        assert_eq!(m.decoded_bytes, 0);
        let blocks = m.compressed_rows.div_ceil(super::BLOCK_ROWS);
        // A snapshot and the text, each decoding a block once however
        // often it's read (a row's neighbours, wraps).
        let before = decodes();
        assert_eq!(a.read(), b.read());
        assert!(
            decodes() - before <= 2 * (blocks + 2),
            "{} decodes, {blocks} blocks",
            decodes() - before
        );
        assert_eq!(a.term.history_memory(), m);
        // A search over the history, both ways round.
        assert_eq!(
            matches(&a, "wrapped-wrapped"),
            matches(&b, "wrapped-wrapped")
        );
        assert_eq!(matches(&a, "link \\d+"), matches(&b, "link \\d+"));
        assert_eq!(a.term.history_memory(), m);
        // A copy of all of it.
        let top = a.term.grid.topmost_line();
        let end = Pos::new(Line(4), Column(29));
        let mut copied = Vec::new();
        for t in [&mut a, &mut b] {
            let mut sel = Selection::new(
                SelectionType::Simple,
                Pos::new(top, Column(0)),
                Side::Left,
            );
            sel.update(end, Side::Right);
            t.term.selection = Some(sel);
            copied.push(t.term.selection_to_string());
        }
        assert_eq!(copied[0], copied[1]);
        assert_eq!(a.term.history_memory(), m);
    }

    #[test]
    fn a_bulk_read_takes_what_the_viewport_decoded_as_it_is() {
        let (mut a, b) = twins(30, 5, 2_000, 1_200);
        a.term.scroll_display(Scroll::Delta(300));
        // What a frame reads: into the caches.
        let offset = a.term.grid.display_offset() as i32;
        for line in -offset..-offset + 5 {
            let _ = a.term.grid[Line(line)][Column(0)].c();
        }
        let shown = a.term.history_memory();
        assert!(shown.decoded_bytes > 0);
        a.term.scroll_display(Scroll::Bottom);
        assert_eq!(a.read(), b.read());
        assert_eq!(a.term.history_memory(), shown);
    }

    #[test]
    fn selecting_compressed_history_copies_the_same_text() {
        use crate::crosswords::{Pos, Side};
        use crate::selection::{Selection, SelectionType};
        let (mut a, mut b) = twins(30, 5, 2_000, 700);
        let packed = a.packed();
        let top = a.term.grid.topmost_line();
        for ty in [
            SelectionType::Simple,
            SelectionType::Block,
            SelectionType::Semantic,
            SelectionType::Lines,
        ] {
            // Across several blocks, from inside one to inside another.
            let (start, end) = (
                Pos::new(top + 7, Column(3)),
                Pos::new(top + 7 + 3 * super::BLOCK_ROWS as i32, Column(12)),
            );
            for t in [&mut a, &mut b] {
                let mut sel = Selection::new(ty, start, Side::Left);
                sel.update(end, Side::Right);
                t.term.selection = Some(sel);
            }
            let text = a.term.selection_to_string();
            assert!(text.as_ref().is_some_and(|t| t.len() > 20), "{ty:?}");
            assert_eq!(text, b.term.selection_to_string(), "{ty:?}");
        }
        let bottom = Pos::new(a.term.grid.bottommost_line(), Column(29));
        let all = a.term.bounds_to_string(Pos::new(top, Column(0)), bottom);
        assert_eq!(
            all,
            b.term.bounds_to_string(Pos::new(top, Column(0)), bottom)
        );
        assert_eq!(a.packed(), packed);
    }

    #[test]
    fn vi_motions_through_compressed_history_land_the_same() {
        use crate::crosswords::vi_mode::ViMotion::*;
        let (mut a, mut b) = twins(30, 5, 2_000, 700);
        let packed = a.packed();
        a.term.toggle_vi_mode();
        b.term.toggle_vi_mode();
        let mut motions = vec![First, Last, FirstOccupied];
        motions.extend(std::iter::repeat_n(Up, 120));
        motions.extend([
            WordLeft,
            WordLeft,
            SemanticLeft,
            SemanticRightEnd,
            WordRightEnd,
            Bracket,
            High,
            Middle,
            Low,
            SemanticLeftEnd,
            WordRight,
            Left,
            Down,
            Right,
        ]);
        for (i, m) in motions.into_iter().enumerate() {
            a.term.vi_motion(m);
            b.term.vi_motion(m);
            assert_eq!(
                a.term.vi_mode_cursor.pos, b.term.vi_mode_cursor.pos,
                "{i}: {m:?}"
            );
            assert_eq!(a.term.grid.display_offset(), b.term.grid.display_offset());
        }
        assert_eq!(a.packed(), packed);
    }

    /// History of plain lines with one prompt at the top.
    fn one_old_prompt() -> (T, T) {
        let mut a = T::new(30, 5, 2_000);
        let mut b = T::new(30, 5, 2_000);
        let mut out = b"\x1b]133;A\x07$ make\r\n".to_vec();
        for i in 0..600 {
            out.extend_from_slice(format!("build step {i}\r\n").as_bytes());
        }
        a.feed(&out);
        b.feed(&out);
        a.term.compress_history(None);
        (a, b)
    }

    #[test]
    fn jumping_to_a_prompt_lands_where_it_does_uncompressed() {
        let (mut a, mut b) = one_old_prompt();
        for forward in [false, true, false] {
            a.term.scroll_to_prompt(forward);
            b.term.scroll_to_prompt(forward);
            assert_eq!(a.term.grid.display_offset(), b.term.grid.display_offset());
        }
        assert!(a.term.grid.display_offset() > 500);
    }

    #[test]
    fn jumping_to_a_prompt_decodes_only_its_block() {
        let (mut a, _) = one_old_prompt();
        let before = decodes();
        a.term.scroll_to_prompt(false);
        assert!(a.term.grid.display_offset() > 500);
        assert!(
            decodes() - before <= 2,
            "{} blocks decoded",
            decodes() - before
        );
    }

    #[test]
    fn highlighting_compressed_rows_leaves_them_compressed() {
        use crate::crosswords::Pos;
        use crate::selection::SelectionRange;
        let (mut a, _) = twins(30, 5, 2_000, 800);
        a.term.scroll_display(Scroll::Delta(300));
        let packed = a.packed();
        let range = SelectionRange::new(
            Pos::new(Line(-300), Column(0)),
            Pos::new(Line(-296), Column(29)),
            false,
        );
        a.term.damage_range_rows(&range);
        assert_eq!(a.packed(), packed);
    }

    #[test]
    fn overwriting_the_top_left_leaves_the_history_compressed() {
        let mut a = T::new(30, 5, 2_000);
        let mut b = T::new(30, 5, 2_000);
        both(&mut a, &mut b, &output(300));
        // Feed lines until the newest history row is in a block too.
        for _ in 0..=super::BLOCK_ROWS {
            a.term.compress_history(None);
            if a.term.history_memory().plain_rows == 0 {
                break;
            }
            a.feed(b"filler\r\n");
            b.feed(b"filler\r\n");
        }
        assert_eq!(a.term.history_memory().plain_rows, 0);
        both(&mut a, &mut b, "\x1b[H\u{754c}".as_bytes());
        let packed = a.packed();
        both(&mut a, &mut b, b"\x1b[Hx");
        assert_eq!(a.packed(), packed);
    }

    #[test]
    fn compressing_again_frees_blocks_whose_rows_were_unpacked() {
        let (mut a, mut b) = twins(30, 5, 2_000, 600);
        let before = a.term.history_memory();
        // Write to every history row but the first of each block (blocks
        // start at the top of the ring, its spare rows included).
        let g = &mut a.term.grid;
        let oldest = g.screen_lines() as i32 - g.raw.rows().len() as i32;
        for line in -(g.history_size() as i32)..0 {
            if (line - oldest) % super::BLOCK_ROWS as i32 != 0 {
                g[Line(line)][Column(0)].set_c('Z');
                b.term.grid[Line(line)][Column(0)].set_c('Z');
            }
        }
        a.term.compress_history(None);
        let after = a.term.history_memory();
        assert!(after.compressed_rows >= before.compressed_rows);
        assert_eq!(a.read(), b.read());
        assert!(
            after.compressed_bytes < before.compressed_bytes * 5 / 4,
            "{before:?} then {after:?}"
        );
    }
}
