//! Resize the terminal and observe the grid follow.
//!
//! Run: `cargo run -p rio-vt --example resize`

use rio_vt::ansi::CursorShape;
use rio_vt::crosswords::pos::Column;
use rio_vt::crosswords::{Crosswords, CrosswordsSize};
use rio_vt::event::{VoidListener, WindowId};
use rio_vt::performer::handler::Processor;

fn row_text(term: &Crosswords<VoidListener>, cols: usize) -> String {
    let rows = term.visible_rows();
    (0..cols).map(|x| rows[0][Column(x)].c()).collect()
}

fn main() {
    let mut term = Crosswords::new(
        CrosswordsSize::new(80, 24),
        CursorShape::Block,
        VoidListener,
        WindowId::from(0),
        0,
        1_000,
    );

    let mut parser = Processor::default();
    parser.advance(&mut term, b"the quick brown fox");

    println!(
        "before: {}x{}  row0={:?}",
        term.columns(),
        term.screen_lines(),
        row_text(&term, 19).trim_end(),
    );

    // Shrink to 40x10; the grid keeps its content.
    term.resize(CrosswordsSize::new(40, 10));

    println!(
        "after:  {}x{}  row0={:?}",
        term.columns(),
        term.screen_lines(),
        row_text(&term, 19).trim_end(),
    );
}
