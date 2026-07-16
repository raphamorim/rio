# librio-vt

Embeddable terminal core extracted from [Rio](https://github.com/raphamorim/rio):
PTY spawning, VT parsing, terminal state, and a host-pulled render-state API
with per-row dirty tracking. No drawing or windowing code — bring your own
renderer, or pair it with `librio-sugarloaf`.

Status: public alpha. No API stability promise; versioned independently of
the Rio terminal.

```rust
use librio_vt::{Engine, RenderState, SurfaceDelegate, SurfaceDesc};
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

A C ABI is available behind the `capi` feature; see `librio/include` in the
Rio repository for the curated headers and the `RioKit.xcframework` packaging.
