// Benchmarks rio-vt against the vt100 crate on the exact "headless
// reconnect-snapshot" workload: feed PTY output into a screen, then serialize
// the visible screen back to ANSI (`contents_formatted`).
//
// Run: cargo bench -p rio-vt

use criterion::{
    black_box, criterion_group, criterion_main, BatchSize, Criterion, Throughput,
};
use rio_vt::ansi::CursorShape;
use rio_vt::crosswords::{Crosswords, CrosswordsSize};
use rio_vt::event::{VoidListener, WindowId};
use rio_vt::performer::handler::Processor;

const ROWS: u16 = 24;
const COLS: u16 = 80;

/// Representative program output: colored `ls`-style lines, plain text, and
/// occasional full-screen redraws (clear + home + header).
fn corpus() -> Vec<u8> {
    let mut out = Vec::new();
    for i in 0..3000u32 {
        out.extend_from_slice(b"\x1b[1;34mdir\x1b[0m  \x1b[32mfile");
        out.extend_from_slice(i.to_string().as_bytes());
        out.extend_from_slice(b".rs\x1b[0m  \x1b[90msome regular output text here\x1b[0m\r\n");
        if i % 40 == 0 {
            out.extend_from_slice(b"\x1b[2J\x1b[H\x1b[1;33m== section header ==\x1b[0m\r\n");
        }
    }
    out
}

fn new_rio() -> (Crosswords<VoidListener>, Processor) {
    let term = Crosswords::new(
        CrosswordsSize::new_with_dimensions(
            COLS as usize,
            ROWS as usize,
            COLS as u32 * 8,
            ROWS as u32 * 16,
            8,
            16,
        ),
        CursorShape::Block,
        VoidListener,
        WindowId::from(0),
        0,
        0,
    );
    (term, Processor::default())
}

fn bench_process(c: &mut Criterion) {
    let data = corpus();
    let mut group = c.benchmark_group("process");
    group.throughput(Throughput::Bytes(data.len() as u64));

    group.bench_function("rio-vt", |b| {
        b.iter_batched(
            new_rio,
            |(mut term, mut parser)| parser.advance(&mut term, &data),
            BatchSize::SmallInput,
        )
    });
    group.bench_function("vt100", |b| {
        b.iter_batched(
            || vt100::Parser::new(ROWS, COLS, 0),
            |mut parser| parser.process(&data),
            BatchSize::SmallInput,
        )
    });
    group.finish();
}

fn bench_snapshot(c: &mut Criterion) {
    let data = corpus();

    // Pre-fill both screens once; the snapshot is read-only.
    let (mut rio_term, mut rio_parser) = new_rio();
    rio_parser.advance(&mut rio_term, &data);
    let mut vt = vt100::Parser::new(ROWS, COLS, 0);
    vt.process(&data);

    let mut group = c.benchmark_group("contents_formatted");
    group.bench_function("rio-vt", |b| {
        b.iter(|| black_box(rio_term.contents_formatted()))
    });
    group.bench_function("vt100", |b| {
        b.iter(|| black_box(vt.screen().contents_formatted()))
    });
    group.finish();
}

criterion_group!(benches, bench_process, bench_snapshot);
criterion_main!(benches);
