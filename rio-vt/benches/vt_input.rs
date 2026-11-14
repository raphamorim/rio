// VT input throughput: where the bytes go when a program floods the
// terminal. Separates the SIMD plain-text path from escape-dense input so
// regressions in either show up on their own.

use criterion::{criterion_group, criterion_main, Criterion, Throughput};

use rio_vt::ansi::CursorShape;
use rio_vt::crosswords::{Crosswords, CrosswordsSize};
use rio_vt::event::{VoidListener, WindowId};
use rio_vt::performer::handler::Processor;

const COLS: usize = 120;
const ROWS: usize = 40;

fn term() -> Crosswords<VoidListener> {
    Crosswords::new(
        CrosswordsSize::new(COLS, ROWS),
        CursorShape::Block,
        VoidListener {},
        WindowId::from(0),
        0,
        2000,
    )
}

fn plain(total: usize) -> Vec<u8> {
    let line =
        "the quick brown fox jumps over the lazy dog 0123456789 lorem ipsum dolor sit amet\r\n";
    line.repeat(total / line.len() + 1).into_bytes()
}

/// The benchmark suite's ANSI mix: 256-color pairs, truecolor bold,
/// italic/underline toggles, resets between every word.
fn ansi_mixed(total: usize) -> Vec<u8> {
    let mut line = String::new();
    for i in 0..10 {
        line += &format!(
            "\x1b[38;5;{}m\x1b[48;5;{}mword{i}\x1b[0m ",
            (i * 37) % 256,
            (i * 53 + 8) % 256
        );
        line += &format!(
            "\x1b[1;38;2;{};{};{}mbold\x1b[0m ",
            (i * 31) % 256,
            (i * 67) % 256,
            (i * 13) % 256
        );
        line += if i % 2 == 1 {
            "\x1b[3mital\x1b[0m "
        } else {
            "\x1b[4munder\x1b[0m "
        };
    }
    line += "\r\n";
    line.repeat(total / line.len() + 1).into_bytes()
}

/// Same word rhythm, only two styles alternating: isolates SGR parse and
/// dispatch cost from style-table churn.
fn ansi_two_styles(total: usize) -> Vec<u8> {
    let mut line = String::new();
    for i in 0..30 {
        line += if i % 2 == 0 {
            "\x1b[31mword\x1b[0m "
        } else {
            "\x1b[44mword\x1b[0m "
        };
    }
    line += "\r\n";
    line.repeat(total / line.len() + 1).into_bytes()
}

/// Full-screen repaint with a distinct truecolor fg+bg pair on every
/// cell, the half-block gradient shape unthrottled fps benchmarks
/// produce. Every cell interns its own style, so this is the worst case
/// for the style table (memo thrash, table growth toward the id cap)
/// on top of SGR-dense parsing.
fn truecolor_unique(total: usize) -> Vec<u8> {
    let mut out = String::new();
    let mut frame = 0usize;
    while out.len() < total {
        for y in 0..ROWS {
            out += &format!("\x1b[{};1H", y + 1);
            for x in 0..COLS {
                let v = (x * 7 + y * 13 + frame * 31) % 253;
                let w = (x * 11 + y * 3 + frame * 17) % 251;
                out += &format!("\x1b[38;2;{v};{v};{v}m\x1b[48;2;{w};{w};{w}m\u{2580}");
            }
        }
        frame += 1;
    }
    out.into_bytes()
}

/// Escape density without SGR payloads: bare reset sequences between
/// words, measuring the state-machine and dispatch floor.
fn ansi_bare_csi(total: usize) -> Vec<u8> {
    let mut line = String::new();
    for _ in 0..30 {
        line += "\x1b[mword ";
    }
    line += "\r\n";
    line.repeat(total / line.len() + 1).into_bytes()
}

/// Sink that counts instead of mutating a grid: parser cost in isolation.
#[derive(Default)]
struct NoopPerform {
    printed: u64,
    csis: u64,
}

impl rio_vt::performer::parser::Perform for NoopPerform {
    fn print_codepoints(&mut self, codepoints: &[u32]) {
        self.printed += codepoints.len() as u64;
    }
    fn csi_dispatch(
        &mut self,
        _params: &rio_vt::performer::parser::Params,
        _intermediates: &[u8],
        _ignore: bool,
        _action: char,
    ) {
        self.csis += 1;
    }
}

fn bench(c: &mut Criterion) {
    const BYTES: usize = 4 * 1024 * 1024;
    let cases: [(&str, Vec<u8>); 5] = [
        ("plain", plain(BYTES)),
        ("ansi_mixed", ansi_mixed(BYTES)),
        ("ansi_two_styles", ansi_two_styles(BYTES)),
        ("ansi_bare_csi", ansi_bare_csi(BYTES)),
        ("truecolor_unique", truecolor_unique(BYTES)),
    ];

    let mut group = c.benchmark_group("vt_input");
    group.sample_size(20);
    for (name, bytes) in &cases {
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_function(*name, |b| {
            b.iter_batched(
                || (term(), Processor::default()),
                |(mut crosswords, mut processor)| {
                    processor.advance(&mut crosswords, bytes);
                    crosswords
                },
                criterion::BatchSize::LargeInput,
            );
        });
        // Parser in isolation: the gap to the full path above is the
        // handler + grid cost.
        group.bench_function(format!("{name}_parser_only"), |b| {
            b.iter_batched(
                || {
                    (
                        rio_vt::performer::parser::Parser::new(),
                        NoopPerform::default(),
                    )
                },
                |(mut parser, mut sink)| {
                    parser.advance(&mut sink, bytes);
                    sink
                },
                criterion::BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
