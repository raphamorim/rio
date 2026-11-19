//! Profiling target: floods a headless grid with the benchmark's ANSI mix.
//! Run under `sample`/Instruments to see where escape-dense input spends
//! its time.

use rio_vt::ansi::CursorShape;
use rio_vt::crosswords::{Crosswords, CrosswordsSize};
use rio_vt::event::{VoidListener, WindowId};
use rio_vt::performer::handler::Processor;

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

fn main() {
    let bytes = ansi_mixed(4 * 1024 * 1024);
    let mut crosswords = Crosswords::new(
        CrosswordsSize::new(120, 40),
        CursorShape::Block,
        VoidListener {},
        WindowId::from(0),
        0,
        2000,
    );
    let mut processor = Processor::default();
    let start = std::time::Instant::now();
    let mut iterations = 0u32;
    while start.elapsed().as_secs() < 12 {
        processor.advance(&mut crosswords, &bytes);
        iterations += 1;
    }
    println!("{iterations} iterations");
}
