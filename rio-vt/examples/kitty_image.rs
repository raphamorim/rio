//! Transmit an image with the Kitty graphics protocol and capture it.
//!
//! Kitty images are delivered through `RioEvent::UpdateGraphics` in the
//! `pending_images` queue (keyed by the client's image id), separate from
//! the sixel/iTerm2 `pending` queue.
//!
//! Run: `cargo run -p rio-vt --example kitty_image`

use rio_vt::ansi::CursorShape;
use rio_vt::crosswords::{Crosswords, CrosswordsSize};
use rio_vt::event::{EventListener, RioEvent, WindowId};
use rio_vt::performer::handler::Processor;
use std::sync::{Arc, Mutex};

/// Records (image_id, width, height, byte-len) for every kitty image emitted.
#[derive(Clone, Default)]
struct ImageSink(Arc<Mutex<Vec<(u32, usize, usize, usize)>>>);

impl EventListener for ImageSink {
    fn event(&self) -> (Option<RioEvent>, bool) {
        (None, false)
    }

    fn send_event(&self, event: RioEvent, _window: WindowId) {
        if let RioEvent::UpdateGraphics { queues, .. } = event {
            let mut sink = self.0.lock().unwrap();
            for (image_id, g) in &queues.pending_images {
                sink.push((*image_id, g.width, g.height, g.pixels.len()));
            }
        }
    }
}

fn main() {
    let sink = ImageSink::default();

    let size = CrosswordsSize::new_with_dimensions(80, 24, 640, 384, 8, 16);
    let mut term = Crosswords::new(
        size,
        CursorShape::Block,
        sink.clone(),
        WindowId::from(0),
        0,
        1_000,
    );

    // A 2x2 solid-red RGBA image, base64-encoded, transmitted with the
    // Kitty graphics protocol:
    //   f=32  -> 32-bit RGBA pixels
    //   s,v   -> source width/height in pixels
    //   a=T   -> transmit AND display
    // The payload is `\x1b_G<control data>;<base64 pixels>\x1b\\`.
    let kitty = b"\x1b_Gf=32,s=2,v=2,a=T;/wAA//8AAP//AAD//wAA/w==\x1b\\";

    let mut parser = Processor::default();
    parser.advance(&mut term, kitty);

    let images = sink.0.lock().unwrap();
    if images.is_empty() {
        println!("no kitty image captured");
    } else {
        for (id, w, h, bytes) in images.iter() {
            println!("kitty image: id={id} {w}x{h}px rgba_bytes={bytes}");
        }
    }
}
