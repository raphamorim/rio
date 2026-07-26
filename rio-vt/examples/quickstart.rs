use rio_vt::ansi::CursorShape;
use rio_vt::crosswords::grid::Dimensions;
use rio_vt::crosswords::pos::Column;
use rio_vt::crosswords::{Crosswords, CrosswordsSize};
use rio_vt::event::{VoidListener, WindowId};
use rio_vt::performer::handler::Processor;

fn main() {
    let size = CrosswordsSize::new(80, 24);
    let cols = size.columns();
    let mut term = Crosswords::new(
        size,
        CursorShape::Block,
        VoidListener,
        WindowId::from(0),
        0,
        10_000,
    );

    let mut parser = Processor::default();
    parser.advance(&mut term, b"\x1b[31mhello\x1b[0m world");

    let rows = term.visible_rows();
    let line: String = (0..cols).map(|x| rows[0][Column(x)].c()).collect();
    assert!(line.starts_with("hello world"));
    println!("ok: {:?}", &line[..11]);
}
