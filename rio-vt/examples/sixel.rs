//! Feed a Sixel image and capture the decoded graphic via the event sink.
//!
//! Received images are drained into a `RioEvent::UpdateGraphics` event
//! (the renderer picks them up there), so we install a small
//! `EventListener` that records what the terminal emits.
//!
//! Run: `cargo run -p rio-vt --example sixel`

use rio_vt::ansi::CursorShape;
use rio_vt::crosswords::{Crosswords, CrosswordsSize};
use rio_vt::event::{EventListener, RioEvent, WindowId};
use rio_vt::performer::handler::Processor;
use std::sync::{Arc, Mutex};

/// Records (id, width, height, byte-len) for every graphic the terminal emits.
#[derive(Clone, Default)]
#[allow(clippy::type_complexity)]
struct GraphicSink(Arc<Mutex<Vec<(u64, usize, usize, usize)>>>);

impl EventListener for GraphicSink {
    fn event(&self) -> (Option<RioEvent>, bool) {
        (None, false)
    }

    fn send_event(&self, event: RioEvent, _window: WindowId) {
        if let RioEvent::UpdateGraphics { queues, .. } = event {
            let mut sink = self.0.lock().unwrap();
            for g in &queues.pending {
                sink.push((g.id.get(), g.width, g.height, g.pixels.len()));
            }
        }
    }
}

fn main() {
    let sink = GraphicSink::default();

    // Image protocols place graphics relative to the cell grid, so the
    // terminal needs real per-cell pixel dimensions (a renderer sets these;
    // here we pass them explicitly: 80x24 cells of 8x16 px).
    let size = CrosswordsSize::new_with_dimensions(80, 24, 640, 384, 8, 16);
    let mut term = Crosswords::new(
        size,
        CursorShape::Block,
        sink.clone(),
        WindowId::from(0),
        0,
        1_000,
    );

    // A real 64x64 Sixel image shipped as a test fixture. In a live
    // terminal these bytes arrive from the PTY.
    let sixel = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/sixel/testimage_im6.sixel"
    ))
    .expect("fixture present");

    let mut parser = Processor::default();
    parser.advance(&mut term, &sixel);

    let graphics = sink.0.lock().unwrap();
    if graphics.is_empty() {
        println!("no sixel graphic captured");
    } else {
        for (id, w, h, bytes) in graphics.iter() {
            println!("sixel graphic: id={id} {w}x{h}px rgba_bytes={bytes}");
        }
    }
}
