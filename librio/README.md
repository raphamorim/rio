# librio

Embeddable terminal core extracted from [Rio](https://github.com/raphamorim/rio):
PTY spawning, VT parsing, terminal state, and a host-pulled render-state API
with per-row dirty tracking. No drawing or windowing code — bring your own
renderer.

`librio` wraps [`rio-vt`](../rio-vt) as a C ABI for non-Rust consumers (Swift,
C, and anything that speaks C). It is **not published to crates.io**: it lives
in the Rio source tree and ships as the `RioKit.xcframework` static library in
Rio's GitHub releases. If you write Rust, depend on `rio-vt` directly.

The Rust surface it exposes over the C boundary:

```rust
use librio::{Engine, RenderState, SurfaceDelegate, SurfaceDesc};
use std::sync::Arc;

struct Delegate;
impl SurfaceDelegate for Delegate {
    fn wakeup(&self, _surface: usize) { /* schedule a render */ }
}

let engine = Engine::new(Arc::new(Delegate));
let surface = engine.create_surface(&SurfaceDesc::default())?;
let mut state = RenderState::new(&surface);
surface.text("ls\r");
state.update();
```

The C ABI is compiled by default (`crate-type = ["staticlib"]`); see
`librio/include` for the curated header and modulemap, and the `Makefile`
`librio-xcframework` target for packaging.
