// The frame path a frontend runs per redraw: consume the damage, copy
// the rows it names into a persistent snapshot, fill their styles.
// Each scenario is one kind of change per frame on a styled 120 by 40
// screen. The timed region is the frame work; the input that caused it
// is parsed outside the timer. Rows copied per frame is printed once
// per scenario, since the row building that follows costs per row.

use std::time::{Duration, Instant};

use criterion::{criterion_group, criterion_main, Criterion};

use rio_vt::ansi::CursorShape;
use rio_vt::crosswords::grid::row::Row;
use rio_vt::crosswords::pos::{Column, Line, Pos};
use rio_vt::crosswords::square::{Extras, Square};
use rio_vt::crosswords::style::{PendingStyles, Style};
use rio_vt::crosswords::{Crosswords, CrosswordsSize};
use rio_vt::event::{TerminalDamage, VoidListener, WindowId};
use rio_vt::performer::handler::Processor;
use rio_vt::selection::SelectionRange;

const COLS: usize = 120;
const ROWS: usize = 40;

type Term = Crosswords<VoidListener>;

#[derive(Default)]
struct Snapshot {
    rows: Vec<Row<Square>>,
    row_styles: Vec<Vec<Style>>,
    extras: rustc_hash::FxHashMap<u16, Extras>,
    pending: PendingStyles,
}

impl Snapshot {
    /// Under the lock: consume the damage and copy the rows it names.
    fn copy(&mut self, cw: &mut Term) -> TerminalDamage {
        let damage = cw.peek_damage_event().unwrap_or(TerminalDamage::Noop);
        cw.reset_damage();
        cw.snapshot_visible(
            &damage,
            &mut self.rows,
            &mut self.row_styles,
            &mut self.extras,
            &mut self.pending,
        );
        damage
    }

    /// After the lock: fill the copied rows' styles.
    fn fill(&mut self) {
        self.pending.fill(&mut self.row_styles);
    }

    /// What a renderer does once it has drawn: count and clear the
    /// copied rows' bits.
    fn take_copied(&mut self) -> usize {
        let mut copied = 0;
        for row in &mut self.rows {
            if row.dirty {
                copied += 1;
                row.dirty = false;
            }
        }
        copied
    }
}

fn mark_range(cw: &mut Term, range: &SelectionRange) {
    cw.damage_range_rows(range);
}

fn term() -> (Term, Processor) {
    let mut cw = Crosswords::new(
        CrosswordsSize::new(COLS, ROWS),
        CursorShape::Block,
        VoidListener {},
        WindowId::from(0),
        0,
        2000,
    );
    let mut parser = Processor::default();
    // History to scroll into, then a styled screen: ten runs per row.
    for i in 0..200 {
        parser.advance(&mut cw, format!("history line {i}\r\n").as_bytes());
    }
    parser.advance(&mut cw, b"\x1b[H\x1b[2J");
    for y in 0..ROWS {
        parser.advance(&mut cw, styled_line(y).as_bytes());
    }
    (cw, parser)
}

fn styled_line(y: usize) -> String {
    let mut s = format!("\x1b[{};1H", y + 1);
    for run in 0..10 {
        s += &format!(
            "\x1b[38;5;{}m{:<12}",
            (y * 10 + run) % 256,
            format!("w{y}r{run}")
        );
    }
    s += "\x1b[0m";
    s
}

/// One kind of change per frame. `prepare` applies the change, outside
/// the timer; `frame` is timed.
type Prepare = Box<dyn FnMut(&mut Term, &mut Processor, usize)>;

struct Scenario {
    name: &'static str,
    prepare: Prepare,
}

fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "cursor move",
            prepare: Box::new(|cw, parser, i| {
                let row = 1 + (i * 7) % ROWS;
                parser.advance(cw, format!("\x1b[{row};5H").as_bytes());
            }),
        },
        Scenario {
            name: "selection drag one row",
            prepare: Box::new(|cw, _, i| {
                let anchor = Pos::new(Line(5), Column(0));
                let before = Line(6 + (i % 30) as i32);
                let after = Line(6 + ((i + 1) % 30) as i32);
                mark_range(
                    cw,
                    &SelectionRange::new(anchor, Pos::new(before, Column(3)), false),
                );
                mark_range(
                    cw,
                    &SelectionRange::new(anchor, Pos::new(after, Column(3)), false),
                );
            }),
        },
        Scenario {
            name: "search idle with matches",
            prepare: Box::new(|_, _, _| {}),
        },
        Scenario {
            name: "pager reverse scroll in region",
            prepare: Box::new(|cw, parser, i| {
                // Region rows 2 to 39; the cursor sits at its top and a
                // reverse index pushes a new line in.
                parser.advance(
                    cw,
                    format!("\x1b[2;39r\x1b[2;1H\x1bM\x1b[2;1H\x1b[31mline {i}\x1b[0m")
                        .as_bytes(),
                );
            }),
        },
        Scenario {
            name: "full scroll one line",
            prepare: Box::new(|cw, parser, i| {
                parser.advance(
                    cw,
                    format!("\x1b[r\x1b[{ROWS};1H\n{}", styled_line(ROWS - 1)).as_bytes(),
                );
                let _ = i;
            }),
        },
        Scenario {
            name: "styled redraw of the whole screen",
            prepare: Box::new(|cw, parser, _| {
                for y in 0..ROWS {
                    parser.advance(cw, styled_line(y).as_bytes());
                }
            }),
        },
    ]
}

fn bench(c: &mut Criterion) {
    for mut scenario in scenarios() {
        let (mut cw, mut parser) = term();
        let mut snap = Snapshot::default();
        snap.copy(&mut cw);
        snap.fill();
        snap.take_copied();

        // Rows copied per frame, over a hundred frames.
        let mut copied = 0usize;
        let mut full = 0usize;
        for i in 0..100 {
            (scenario.prepare)(&mut cw, &mut parser, i);
            if matches!(snap.copy(&mut cw), TerminalDamage::Full) {
                full += 1;
            }
            snap.fill();
            copied += snap.take_copied();
        }
        eprintln!(
            "{}: {:.1} rows copied per frame, {full} of 100 frames full",
            scenario.name,
            copied as f64 / 100.0
        );

        let mut group = c.benchmark_group(scenario.name);
        group.bench_function("under the lock", |b| {
            b.iter_custom(|iters| {
                let mut total = Duration::ZERO;
                for i in 0..iters as usize {
                    (scenario.prepare)(&mut cw, &mut parser, i);
                    let start = Instant::now();
                    snap.copy(&mut cw);
                    total += start.elapsed();
                    snap.fill();
                    snap.take_copied();
                }
                total
            })
        });
        group.bench_function("whole frame", |b| {
            b.iter_custom(|iters| {
                let mut total = Duration::ZERO;
                for i in 0..iters as usize {
                    (scenario.prepare)(&mut cw, &mut parser, i);
                    let start = Instant::now();
                    snap.copy(&mut cw);
                    snap.fill();
                    total += start.elapsed();
                    snap.take_copied();
                }
                total
            })
        });
        group.finish();
    }
}

criterion_group!(benches, bench);
criterion_main!(benches);
