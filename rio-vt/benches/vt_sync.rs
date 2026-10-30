// Synchronized output (mode 2026) throughput: full-screen redraws
// bracketed by BSU/ESU, the shape TUIs like Claude Code emit on every
// frame, against the same frames without the brackets.

use criterion::{criterion_group, criterion_main, Criterion, Throughput};

use rio_vt::ansi::CursorShape;
use rio_vt::crosswords::{Crosswords, CrosswordsSize};
use rio_vt::event::{VoidListener, WindowId};
use rio_vt::performer::handler::Processor;

const COLS: usize = 120;
const ROWS: usize = 40;
const READ: usize = 4096;

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

fn frames(total: usize, synced: bool) -> Vec<u8> {
    let mut out = String::new();
    let mut frame = 0usize;
    while out.len() < total {
        if synced {
            out += "\x1b[?2026h";
        }
        out += "\x1b[H\x1b[2J";
        for y in 0..ROWS {
            out += &format!(
                "\x1b[{};1H\x1b[38;5;{}m{:<width$}\x1b[0m",
                y + 1,
                (y * 7 + frame) % 256,
                format!(
                    "frame {frame} line {y} the quick brown fox jumps over the lazy dog"
                ),
                width = COLS - 1
            );
        }
        if synced {
            out += "\x1b[?2026l";
        }
        frame += 1;
    }
    out.into_bytes()
}

fn bench(c: &mut Criterion) {
    const BYTES: usize = 4 * 1024 * 1024;
    let synced = frames(BYTES, true);
    let unsynced = frames(BYTES, false);

    let mut group = c.benchmark_group("vt_sync");
    group.sample_size(20);

    let cases: [(&str, &[u8], usize); 3] = [
        ("sync_frames_4k_reads", &synced, READ),
        ("sync_frames_one_write", &synced, synced.len()),
        ("unsynced_frames_4k_reads", &unsynced, READ),
    ];
    for (name, bytes, chunk) in cases {
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_function(name, |b| {
            b.iter_batched(
                || (term(), Processor::default()),
                |(mut crosswords, mut processor)| {
                    for read in bytes.chunks(chunk) {
                        processor.advance(&mut crosswords, read);
                    }
                    crosswords
                },
                criterion::BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
