//! Frames of the grid row builders at 300 by 100 cells, release
//! build, warm: how long a redraw of the whole screen takes once the
//! run shaping cache has seen it. Four screens: styled text with the
//! weight changing every few words, two densities; plain text, one run
//! per row; and a scroll, where one new row enters per frame. Run with
//! `cargo bench -p rio-grid`.

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use rio_backend::config::colors::term::TermColors;
use rio_backend::config::colors::{AnsiColor, ColorArray, Colors};
use rio_backend::crosswords::grid::row::Row;
use rio_backend::crosswords::pos::Column;
use rio_backend::crosswords::square::Square;
use rio_backend::crosswords::style::{Style, StyleFlags};
use rio_backend::sugarloaf::font::FontLibrary;
use rio_backend::sugarloaf::grid::cpu::CpuGridRenderer;
use rio_backend::sugarloaf::grid::{CellBg, CellText, GridRenderer};
use rio_grid::{build_row_bg, build_row_fg, ExtrasMap, GridGlyphRasterizer, GridPalette};

struct Palette(Colors);

impl GridPalette for Palette {
    fn named_colors(&self) -> &Colors {
        &self.0
    }
    fn compute_color(&self, _: &AnsiColor, _: StyleFlags, _: &TermColors) -> ColorArray {
        [1.0, 1.0, 1.0, 1.0]
    }
    fn compute_bg_color(&self, _: &Style, _: &TermColors) -> ColorArray {
        [0.0, 0.0, 0.0, 1.0]
    }
    fn color(&self, _: usize, _: &TermColors) -> ColorArray {
        [1.0, 1.0, 1.0, 1.0]
    }
    fn use_drawable_chars(&self) -> bool {
        false
    }
    fn opacity_cells(&self) -> bool {
        false
    }
    fn cell_bg_alpha(&self) -> u8 {
        255
    }
    fn ignore_selection_fg_color(&self) -> bool {
        false
    }
}

const COLS: usize = 300;
const ROWS: usize = 100;
/// Letters that never form a ligature the row builder breaks a run at.
const LETTERS: &[u8] = b"abcdeghjkmnopqruvwxyz";

#[derive(Clone)]
struct Screen {
    rows: Vec<Row<Square>>,
    styles: Vec<Vec<Style>>,
}

/// One row of words `word_len` cells long, the weight toggling per
/// word, every word's text distinct for `seed`; `word_len == 0` is one
/// plain run across the row.
fn make_row(seed: u64, word_len: usize) -> (Row<Square>, Vec<Style>) {
    let mut row: Row<Square> = Row::new(COLS);
    let mut styles = vec![Style::default(); COLS];
    let bold = Style {
        flags: StyleFlags::BOLD,
        ..Style::default()
    };
    let mut x = 0usize;
    let mut word = 0u64;
    while x < COLS {
        let len = if word_len == 0 {
            COLS
        } else {
            word_len.min(COLS - x)
        };
        let is_bold = word_len != 0 && word % 2 == 1;
        let mut s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ word.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        for k in 0..len {
            s ^= s >> 13;
            s = s.wrapping_mul(0x2545_F491_4F6C_DD1D);
            let ch = if word_len != 0 && k == len - 1 {
                ' '
            } else {
                LETTERS[(s >> 33) as usize % LETTERS.len()] as char
            };
            let mut sq = Square::from_char(ch);
            sq.set_style_id(if is_bold { 1 } else { 0 });
            row[Column(x + k)] = sq;
            if is_bold {
                styles[x + k] = bold;
            }
        }
        x += len;
        word += 1;
    }
    (row, styles)
}

fn make_screen(word_len: usize, seed: u64) -> Screen {
    let (rows, styles) = (0..ROWS)
        .map(|y| make_row(seed + y as u64, word_len))
        .unzip();
    Screen { rows, styles }
}

struct Frame {
    palette: Palette,
    term_colors: TermColors,
    extras: ExtrasMap,
    rasterizer: GridGlyphRasterizer,
    grid: GridRenderer,
    library: FontLibrary,
    bg: Vec<CellBg>,
    fg: Vec<CellText>,
}

impl Frame {
    fn new() -> Self {
        Self {
            palette: Palette(Colors::default()),
            term_colors: TermColors::default(),
            extras: ExtrasMap::default(),
            rasterizer: GridGlyphRasterizer::new(),
            grid: GridRenderer::Cpu(CpuGridRenderer::new(COLS as u32, ROWS as u32)),
            library: FontLibrary::default(),
            bg: Vec::new(),
            fg: Vec::new(),
        }
    }

    /// Draw every row of `screen`, as a full redraw does.
    fn draw(&mut self, screen: &Screen) {
        for y in 0..ROWS {
            build_row_bg(
                &screen.rows[y],
                COLS,
                &screen.styles[y],
                &self.palette,
                &self.term_colors,
                None,
                &[],
                None,
                &mut self.bg,
            );
            build_row_fg(
                &screen.rows[y],
                COLS,
                y as u16,
                &screen.styles[y],
                &self.extras,
                &self.palette,
                &self.term_colors,
                &mut self.rasterizer,
                &mut self.grid,
                14.0,
                8.0,
                17.0,
                None,
                &[],
                None,
                &self.library,
                0,
                None,
                &mut self.fg,
            );
            self.grid.write_row(y as u32, &self.bg, &self.fg);
        }
        self.rasterizer.end_frame();
    }
}

fn warm_frames(c: &mut Criterion) {
    let mut frame = Frame::new();
    let mut group = c.benchmark_group("grid_rows");
    group.sample_size(60);
    for (name, word_len) in [
        ("styled 9 runs per row", 33usize),
        ("styled 50 runs per row", 6),
        ("plain 1 run per row", 0),
    ] {
        let screen = make_screen(word_len, 1);
        frame.rasterizer.clear_font_caches();
        for _ in 0..5 {
            frame.draw(&screen);
        }
        group.bench_function(name, |b| b.iter(|| frame.draw(&screen)));
    }

    // One new row enters at the bottom every frame, the rest shift up.
    frame.rasterizer.clear_font_caches();
    let mut screen = make_screen(33, 1);
    let mut next_seed = 10_000u64;
    for _ in 0..5 {
        frame.draw(&screen);
    }
    group.bench_function("styled scroll one row per frame", |b| {
        b.iter_batched(
            || {
                screen.rows.remove(0);
                screen.styles.remove(0);
                let (row, styles) = make_row(next_seed, 33);
                next_seed += 1;
                screen.rows.push(row);
                screen.styles.push(styles);
                screen.clone()
            },
            |screen| frame.draw(&screen),
            BatchSize::LargeInput,
        )
    });
    group.finish();
}

criterion_group!(benches, warm_frames);
criterion_main!(benches);
