//! Make a selection over grid content and read it back as text.
//!
//! Run: `cargo run -p rio-vt --example selection`

use rio_vt::ansi::CursorShape;
use rio_vt::crosswords::pos::{Column, Line, Pos, Side};
use rio_vt::crosswords::{Crosswords, CrosswordsSize};
use rio_vt::event::{VoidListener, WindowId};
use rio_vt::performer::handler::Processor;
use rio_vt::selection::{Selection, SelectionType};

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
    parser.advance(&mut term, b"hello world");

    // Select columns 0..=4 of the first row ("hello").
    let mut selection = Selection::new(
        SelectionType::Simple,
        Pos::new(Line(0), Column(0)),
        Side::Left,
    );
    selection.update(Pos::new(Line(0), Column(4)), Side::Right);
    term.selection = Some(selection);

    match term.selection_to_string() {
        Some(text) => println!("selection: {text:?}"),
        None => println!("empty selection"),
    }
}
