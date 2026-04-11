// Cell performance benches.
//
// These establish the Phase 0 baseline numbers for `PERF_PLAN_CELL.md`. They
// must run cleanly on `main` before any cell-repack work begins, so that the
// later phases have something to compare against.
//
// Three benches:
//
//   * bench_grid_walk        — render-shaped traversal of a populated grid
//                              (the per-frame read pattern).
//   * bench_pty_parse        — drive the parser over a fixed mixed text +
//                              escape sequence payload (the per-byte write
//                              pattern).
//   * bench_scrollback_walk  — walk every cell of a populated scrollback the
//                              way regex search / selection do.
//
// Run with:
//
//     cargo bench -p rio-backend --bench cell_perf
//
// Expected runtime: a few minutes.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};

use rio_backend::ansi::CursorShape;
use rio_backend::config::colors::{AnsiColor, NamedColor};
use rio_backend::crosswords::grid::Dimensions;
use rio_backend::crosswords::pos::{Column, Line};
use rio_backend::crosswords::square::Square;
use rio_backend::crosswords::style::{Style, StyleFlags};
use rio_backend::crosswords::{Crosswords, CrosswordsSize};
use rio_backend::event::{VoidListener, WindowId};
use rio_backend::performer::handler::{Processor, StdSyncHandler};

const COLS: usize = 200;
const ROWS: usize = 60;
const SCROLLBACK_ROWS: usize = 10_000;

fn make_terminal(cols: usize, rows: usize) -> Crosswords<VoidListener> {
    let size = CrosswordsSize::new(cols, rows);
    let window_id = WindowId::from(0);
    Crosswords::new(size, CursorShape::Block, VoidListener {}, window_id, 0)
}

/// Populate every cell of the visible region with a deterministic pattern.
/// Each (row, col) gets a distinct style interned in the per-grid StyleSet.
fn populate_visible(cw: &mut Crosswords<VoidListener>, cols: usize, rows: usize) {
    // Pre-intern a small variety of styles so the cells point to a mix of
    // ids — exercising both the cache hit (same id repeated) and the lookup
    // path. We pre-build the styles outside the cell loop so the bench
    // measures cell writes, not style hashing.
    let mut styles: Vec<rio_backend::crosswords::style::StyleId> =
        Vec::with_capacity(64);
    for i in 0..64u32 {
        let mut style = Style::default();
        style.fg = match i % 4 {
            0 => AnsiColor::Named(NamedColor::Foreground),
            1 => AnsiColor::Indexed((i & 0xff) as u8),
            2 => AnsiColor::Spec(rio_backend::config::colors::ColorRgb {
                r: (i & 0xff) as u8,
                g: ((i * 7) & 0xff) as u8,
                b: 0x80,
            }),
            _ => AnsiColor::Named(NamedColor::Red),
        };
        style.bg = if i % 5 == 0 {
            AnsiColor::Named(NamedColor::Background)
        } else {
            AnsiColor::Indexed((i * 11 & 0xff) as u8)
        };
        if i % 3 == 0 {
            style.flags.insert(StyleFlags::BOLD);
        }
        if i % 7 == 0 {
            style.flags.insert(StyleFlags::ITALIC);
        }
        if i % 11 == 0 {
            style.flags.insert(StyleFlags::UNDERLINE);
        }
        if i % 13 == 0 {
            style.flags.insert(StyleFlags::INVERSE);
        }
        styles.push(cw.grid.style_set.intern(style));
    }

    for r in 0..rows {
        for c in 0..cols {
            let style_id = styles[(r * 17 + c) % styles.len()];
            let ch = match c % 16 {
                0 => 'a',
                1 => 'b',
                2 => 'c',
                3 => '中',
                4 => 'é',
                5 => '0',
                6 => 'X',
                7 => '!',
                8 => 'q',
                9 => 'w',
                10 => 'e',
                11 => 'r',
                12 => 't',
                13 => 'y',
                14 => 'u',
                _ => 'i',
            };
            let cell = &mut cw.grid[Line(r as i32)][Column(c)];
            cell.set_c(ch);
            cell.set_style_id(style_id);
        }
    }
}

/// Worst case: every cell does a fresh `style_set.get(...)` call, with no
/// caching across consecutive cells. This is the slowest path through the
/// new layout — the renderer never actually does this in practice.
fn walk_visible(cw: &Crosswords<VoidListener>, cols: usize, rows: usize) -> u64 {
    let mut acc: u64 = 0;
    for r in 0..rows {
        for c in 0..cols {
            let cell = &cw.grid[Line(r as i32)][Column(c)];
            acc = acc.wrapping_add(cell.c() as u64);
            let style = cw.grid.style_set.get(cell.style_id());
            acc = acc.wrapping_add(mix_style(&style));
        }
    }
    acc
}

/// Renderer pattern: cache the looked-up style across consecutive cells
/// with the same `style_id`. Only re-fetch when the id changes. This is
/// what `frontends/rioterm/src/renderer/mod.rs::create_line` actually does.
///
/// Three fast paths:
/// 1. Same id as the last cell → reuse cached mix (no lookup at all).
/// 2. `style_id == 0` (default) → use a precomputed constant default,
///    skipping the StyleSet table entirely.
/// 3. Bg-only cell (Ghostty-style content tag) → read the inline color
///    bits directly from the cell, skipping the StyleSet table entirely.
fn walk_visible_cached(
    cw: &Crosswords<VoidListener>,
    cols: usize,
    rows: usize,
) -> u64 {
    use rio_backend::crosswords::square::ContentTag;
    use rio_backend::crosswords::style::{Style, StyleId, DEFAULT_STYLE_ID};

    let default_style = Style::default();
    let default_mix = mix_style(&default_style);

    let mut acc: u64 = 0;
    for r in 0..rows {
        // Per-row cache. The key combines the content tag and the upper
        // 32 bits of the cell (which hold either the style_id or the bg
        // color encoding). One integer compare detects any change in the
        // cell's effective style — Codepoint vs Codepoint, bg-only vs
        // bg-only, or transitions between the two.
        let mut cached_key: u64 = u64::MAX;
        let mut cached_mix: u64 = default_mix;

        for c in 0..cols {
            let cell = &cw.grid[Line(r as i32)][Column(c)];
            acc = acc.wrapping_add(cell.c() as u64);

            // Read the raw bits once. The tag is in bits 30..31 and the
            // style/bg payload is in bits 32..63 — combine them into a
            // single key so the cache hit check is one compare.
            let bits = cell.raw();
            let key = (bits >> 30) & 0x3_FFFF_FFFF;

            if key != cached_key {
                cached_key = key;
                cached_mix = match ContentTag::from_bits(bits) {
                    ContentTag::Codepoint => {
                        let sid = cell.style_id();
                        if sid == DEFAULT_STYLE_ID {
                            default_mix
                        } else {
                            mix_style(&cw.grid.style_set.get(sid))
                        }
                    }
                    ContentTag::BgPalette => cell.bg_palette_index() as u64,
                    ContentTag::BgRgb => {
                        let (rr, gg, bb) = cell.bg_rgb();
                        ((rr as u64) << 16) | ((gg as u64) << 8) | bb as u64
                    }
                };
            }
            acc = acc.wrapping_add(cached_mix);
        }
    }
    acc
}

/// Populate every cell as a bg-only cell with a small palette of inline
/// colors. Mimics a worst-case "selection highlight" or "color block"
/// workload — every cell triggers the bg-only fast path; the StyleSet is
/// never touched. With the inline encoding this should be the *fastest*
/// possible walk through the new layout.
fn populate_visible_bg_only(
    cw: &mut Crosswords<VoidListener>,
    cols: usize,
    rows: usize,
) {
    for r in 0..rows {
        for c in 0..cols {
            let cell = &mut cw.grid[Line(r as i32)][Column(c)];
            // Mix of palette and rgb cells.
            if (r + c) % 2 == 0 {
                cell.set_bg_palette(((r * 3 + c) & 0xff) as u8);
            } else {
                cell.set_bg_rgb(
                    ((r * 17) & 0xff) as u8,
                    ((c * 31) & 0xff) as u8,
                    0x40,
                );
            }
        }
    }
}

/// Realistic populate: 80% of cells are default-styled text, 15% have a
/// shared "prompt" style, 5% are bg-only highlight cells. This mirrors a
/// real terminal showing command output with a colored prompt and the
/// occasional selected/highlighted span.
fn populate_visible_realistic(
    cw: &mut Crosswords<VoidListener>,
    cols: usize,
    rows: usize,
) {
    let prompt_style = Style {
        fg: AnsiColor::Named(NamedColor::Green),
        flags: StyleFlags::BOLD,
        ..Style::default()
    };
    let prompt_id = cw.grid.style_set.intern(prompt_style);

    for r in 0..rows {
        for c in 0..cols {
            let bucket = (r * 13 + c) % 100;
            let cell = &mut cw.grid[Line(r as i32)][Column(c)];
            if bucket < 80 {
                // Default-styled text — most cells.
                let ch = match c % 8 {
                    0 => 'a',
                    1 => 'b',
                    2 => 'c',
                    3 => 'd',
                    4 => '中',
                    5 => 'e',
                    6 => 'f',
                    _ => 'g',
                };
                cell.set_c(ch);
            } else if bucket < 95 {
                // Prompt-styled text — long runs at the start of rows.
                cell.set_c('$');
                cell.set_style_id(prompt_id);
            } else {
                // Highlight bg cell — bg-only fast path.
                cell.set_bg_palette(11);
            }
        }
    }
}

#[inline]
fn mix_style(style: &rio_backend::crosswords::style::Style) -> u64 {
    let mut acc: u64 = 0;
    acc = acc.wrapping_add(match style.fg {
        AnsiColor::Named(n) => n as u64,
        AnsiColor::Spec(rgb) => {
            (rgb.r as u64) << 16 | (rgb.g as u64) << 8 | (rgb.b as u64)
        }
        AnsiColor::Indexed(i) => i as u64,
    });
    acc = acc.wrapping_add(match style.bg {
        AnsiColor::Named(n) => n as u64,
        AnsiColor::Spec(rgb) => {
            (rgb.r as u64) << 16 | (rgb.g as u64) << 8 | (rgb.b as u64)
        }
        AnsiColor::Indexed(i) => i as u64,
    });
    acc = acc.wrapping_add(style.flags.bits() as u64);
    acc
}

/// Populate the grid with realistic style runs: each row has 1-3 long runs
/// of cells sharing a style, mimicking typical terminal output (ANSI prompt,
/// syntax highlighting, log levels, etc.). With this layout the renderer's
/// style cache hits ~95% of the time.
fn populate_visible_runs(
    cw: &mut Crosswords<VoidListener>,
    cols: usize,
    rows: usize,
) {
    let mut styles: Vec<rio_backend::crosswords::style::StyleId> =
        Vec::with_capacity(8);
    for i in 0..8u32 {
        let mut style = Style::default();
        style.fg = match i % 3 {
            0 => AnsiColor::Named(NamedColor::Foreground),
            1 => AnsiColor::Indexed((i + 30) as u8),
            _ => AnsiColor::Named(NamedColor::Red),
        };
        if i % 2 == 0 {
            style.flags.insert(StyleFlags::BOLD);
        }
        if i % 3 == 0 {
            style.flags.insert(StyleFlags::ITALIC);
        }
        styles.push(cw.grid.style_set.intern(style));
    }

    for r in 0..rows {
        // Each row gets 2 style runs: one for the first ~30 cells (a prompt),
        // and one for the rest (the command output). The split point and
        // style ids vary per row to keep things interesting.
        let split = 25 + (r * 7) % 20;
        let style_a = styles[r % styles.len()];
        let style_b = styles[(r * 3 + 1) % styles.len()];
        for c in 0..cols {
            let style_id = if c < split { style_a } else { style_b };
            let ch = match c % 16 {
                0 => 'a',
                1 => 'b',
                2 => 'c',
                3 => '中',
                4 => 'é',
                5 => '0',
                6 => 'X',
                7 => '!',
                8 => 'q',
                9 => 'w',
                10 => 'e',
                11 => 'r',
                12 => 't',
                13 => 'y',
                14 => 'u',
                _ => 'i',
            };
            let cell = &mut cw.grid[Line(r as i32)][Column(c)];
            cell.set_c(ch);
            cell.set_style_id(style_id);
        }
    }
}

fn bench_grid_walk(c: &mut Criterion) {
    let mut group = c.benchmark_group("cell");
    group.sample_size(50);

    // 1) Worst case — every cell has its own style id, no cache hits.
    let mut cw_worst = make_terminal(COLS, ROWS);
    populate_visible(&mut cw_worst, COLS, ROWS);
    group.bench_function("grid_walk_200x60_worst", |b| {
        b.iter(|| {
            black_box(walk_visible(&cw_worst, COLS, ROWS));
        });
    });

    // 2) Same data, renderer-style cached walk. Shows what the cache buys
    //    on the worst-case input distribution.
    group.bench_function("grid_walk_200x60_worst_cached", |b| {
        b.iter(|| {
            black_box(walk_visible_cached(&cw_worst, COLS, ROWS));
        });
    });

    // 3) Realistic data — long style runs per row + cached walk. This is
    //    what `frontends/rioterm/src/renderer/mod.rs::create_line` actually
    //    looks like in production.
    let mut cw_runs = make_terminal(COLS, ROWS);
    populate_visible_runs(&mut cw_runs, COLS, ROWS);
    group.bench_function("grid_walk_200x60_runs_cached", |b| {
        b.iter(|| {
            black_box(walk_visible_cached(&cw_runs, COLS, ROWS));
        });
    });

    // 4) Pure bg-only cells (Ghostty's content_tag fast path). Mimics a
    //    huge selection highlight or a color block fill — every cell skips
    //    the StyleSet entirely.
    let mut cw_bg = make_terminal(COLS, ROWS);
    populate_visible_bg_only(&mut cw_bg, COLS, ROWS);
    group.bench_function("grid_walk_200x60_bg_only", |b| {
        b.iter(|| {
            black_box(walk_visible_cached(&cw_bg, COLS, ROWS));
        });
    });

    // 5) Realistic mix: 80% default text, 15% prompt-styled, 5% bg-only.
    //    Closest match to actual terminal output.
    let mut cw_real = make_terminal(COLS, ROWS);
    populate_visible_realistic(&mut cw_real, COLS, ROWS);
    group.bench_function("grid_walk_200x60_realistic", |b| {
        b.iter(|| {
            black_box(walk_visible_cached(&cw_real, COLS, ROWS));
        });
    });

    group.finish();
}

/// A representative-ish chunk of bytes that hits the parser hard:
/// CSI styling, SGR resets, mixed UTF-8, control characters, and lots of
/// printable text. ~1024 bytes per chunk; we send N chunks per iteration.
fn pty_chunk() -> Vec<u8> {
    let mut buf = Vec::with_capacity(1024);
    // Some SGR styling
    buf.extend_from_slice(b"\x1b[1;31mERROR\x1b[0m: ");
    buf.extend_from_slice(b"\x1b[33mwarning\x1b[0m: ");
    // Mixed text
    buf.extend_from_slice(
        "Hello 🌍! café naïve 中文 — quick brown fox jumps over the lazy dog. "
            .as_bytes(),
    );
    // Repeat to ~1 KiB
    while buf.len() < 1024 {
        buf.extend_from_slice(
            b"\x1b[34mlorem\x1b[0m \x1b[32mipsum\x1b[0m \x1b[35mdolor\x1b[0m \
              sit amet consectetur adipiscing elit sed do eiusmod tempor. ",
        );
    }
    buf.truncate(1024);
    buf
}

fn bench_pty_parse(c: &mut Criterion) {
    let chunks = 256; // 256 KiB per iteration
    let chunk = pty_chunk();
    let mut payload = Vec::with_capacity(chunks * chunk.len());
    for _ in 0..chunks {
        payload.extend_from_slice(&chunk);
    }

    let mut group = c.benchmark_group("cell");
    group.sample_size(20);
    group.throughput(criterion::Throughput::Bytes(payload.len() as u64));
    group.bench_function("pty_parse_256kib", |b| {
        b.iter_batched(
            || (make_terminal(COLS, ROWS), Processor::<StdSyncHandler>::new()),
            |(mut cw, mut processor)| {
                processor.advance(&mut cw, black_box(&payload));
                black_box(&cw);
            },
            criterion::BatchSize::LargeInput,
        );
    });
    group.finish();
}

fn populate_scrollback(
    cw: &mut Crosswords<VoidListener>,
    cols: usize,
    total_rows: usize,
) {
    // Walk far enough that the visible region has been scrolled past
    // `total_rows` lines, populating scrollback as we go.
    let mut processor = Processor::<StdSyncHandler>::new();
    for line in 0..total_rows {
        let mut row = String::with_capacity(cols + 8);
        // Sprinkle some SGR so the rows aren't all default-styled.
        if line % 3 == 0 {
            row.push_str("\x1b[1;36m");
        }
        for c in 0..cols {
            row.push((b'a' + ((line + c) % 26) as u8) as char);
        }
        if line % 3 == 0 {
            row.push_str("\x1b[0m");
        }
        row.push('\r');
        row.push('\n');
        processor.advance(cw, row.as_bytes());
    }
}

fn walk_scrollback(cw: &Crosswords<VoidListener>) -> u64 {
    let mut acc: u64 = 0;
    let history = cw.grid.history_size();
    let visible = cw.grid.screen_lines();
    // Negative line indices are scrollback (older).
    let oldest = -(history as i32);
    let newest = visible as i32;
    for line in oldest..newest {
        let row = &cw.grid[Line(line)];
        for col in 0..row.len() {
            let cell = &row[Column(col)];
            acc = acc.wrapping_add(cell.c() as u64);
            // Cheap mix of cell + style — exercise the lookup path.
            acc = acc.wrapping_add(cell.style_id() as u64);
        }
    }
    acc
}

fn bench_scrollback_walk(c: &mut Criterion) {
    let mut cw = make_terminal(COLS, ROWS);
    populate_scrollback(&mut cw, COLS, SCROLLBACK_ROWS);

    let mut group = c.benchmark_group("cell");
    group.sample_size(20);
    group.bench_function("scrollback_walk_10k", |b| {
        b.iter(|| {
            black_box(walk_scrollback(&cw));
        });
    });
    group.finish();
}

/// Print the absolute sizes of the types we care about. This isn't a bench
/// per se but it gets printed alongside criterion's output. Locked-in
/// numbers are documented in PERF_PLAN_CELL.md.
fn report_type_sizes() {
    use std::mem::size_of;
    eprintln!();
    eprintln!("== type sizes (bytes) ==");
    eprintln!("  Square         = {}", size_of::<Square>());
    eprintln!("  AnsiColor      = {}", size_of::<AnsiColor>());
    eprintln!("  StyleFlags     = {}", size_of::<StyleFlags>());
    eprintln!(
        "  Option<Square> = {}  (niche check)",
        size_of::<Option<Square>>()
    );
    eprintln!();
}

fn bench_size_report(c: &mut Criterion) {
    report_type_sizes();
    // Empty bench so criterion still finds something to do in this group.
    c.bench_function("cell/size_report", |b| {
        b.iter(|| black_box(std::mem::size_of::<Square>()));
    });
}

criterion_group!(
    benches,
    bench_size_report,
    bench_grid_walk,
    bench_pty_parse,
    bench_scrollback_walk,
);
criterion_main!(benches);
