// What a terminal server pays to hand a new client the session: a full
// snapshot (`FormatOptions::snapshot`) of a 120 by 40 terminal with 10k
// lines of styled history. Variants: OSC 8 links on every line, a color
// per cell (the most SGR a snapshot writes), an alt-screen program over
// the history, the visible screen alone, and the plain text of it all.

use criterion::{criterion_group, criterion_main, Criterion};

use rio_vt::ansi::CursorShape;
use rio_vt::crosswords::formatter::FormatOptions;
use rio_vt::crosswords::{Crosswords, CrosswordsSize};
use rio_vt::event::{VoidListener, WindowId};
use rio_vt::performer::handler::Processor;

const COLS: usize = 120;
const ROWS: usize = 40;
const HISTORY: usize = 10_000;

type Term = Crosswords<VoidListener>;

fn term(lines: impl Fn(usize) -> String) -> Term {
    let mut t = Crosswords::new(
        CrosswordsSize::new(COLS, ROWS),
        CursorShape::Block,
        VoidListener,
        WindowId::from(0),
        0,
        HISTORY,
    );
    let mut p = Processor::default();
    let mut data = String::new();
    for i in 0..HISTORY + ROWS {
        data.push_str(&lines(i));
    }
    p.advance(&mut t, data.as_bytes());
    t
}

fn styled(i: usize) -> String {
    format!(
        "\x1b[3{}mline {i:05} \x1b[0m{}\r\n",
        i % 8,
        "lorem ipsum dolor sit amet ".repeat(3)
    )
}

fn linked(i: usize) -> String {
    format!(
        "see \x1b]8;;https://example.com/{i}\x1b\\example.com/{i}\x1b]8;;\x1b\\ and \x1b[1mbold\x1b[0m\r\n"
    )
}

/// Every cell its own color, as `lolcat` or a gradient draws: the most SGR
/// a snapshot writes.
fn rainbow(i: usize) -> String {
    let mut line = String::new();
    for x in 0..COLS {
        line.push_str(&format!(
            "\x1b[38;5;{}m{}",
            (i + x) % 256,
            (b'a' + (x % 26) as u8) as char
        ));
    }
    line.push_str("\x1b[0m\r\n");
    line
}

fn bench(c: &mut Criterion) {
    let mut g = c.benchmark_group("snapshot");

    let t = term(styled);
    eprintln!("styled: {} bytes", t.snapshot_formatted().len());
    g.bench_function("styled_10k", |b| b.iter(|| t.snapshot_formatted()));
    g.bench_function("plain_10k", |b| {
        b.iter(|| t.format(FormatOptions::plain().with_scrollback(true)))
    });

    let t = term(linked);
    eprintln!("linked: {} bytes", t.snapshot_formatted().len());
    g.bench_function("linked_10k", |b| b.iter(|| t.snapshot_formatted()));

    let t = term(rainbow);
    eprintln!("rainbow: {} bytes", t.snapshot_formatted().len());
    g.bench_function("rainbow_10k", |b| b.iter(|| t.snapshot_formatted()));

    let mut t = term(styled);
    let mut p = Processor::default();
    p.advance(&mut t, b"\x1b[?1049h\x1b[H\x1b[2J");
    for r in 0..ROWS {
        p.advance(
            &mut t,
            format!("\x1b[{};1H\x1b[44m{}\x1b[0m", r + 1, "x".repeat(COLS)).as_bytes(),
        );
    }
    g.bench_function("alt_over_10k", |b| b.iter(|| t.snapshot_formatted()));

    let t = term(|i| if i < 5 { styled(i) } else { String::new() });
    g.bench_function("screen_only", |b| b.iter(|| t.snapshot_formatted()));
    g.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
