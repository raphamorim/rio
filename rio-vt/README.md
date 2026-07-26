# rio-vt

Rio's embeddable terminal core: the VT state machine, ANSI/escape parser,
grid with scrollback, selection, search, and PTY-facing event model,
extracted from [Rio](https://github.com/raphamorim/rio) as a standalone,
dependency-light crate.

It carries **no rendering, GPU, or font-shaping dependencies**. You pair
it with a renderer of your choice (Rio uses `sugarloaf`) or drive it purely
from the pulled grid state. It is the safe-Rust sibling of
[`librio-vt`](../librio/vt), which wraps this same core in a C ABI for
embedding from Swift, C, Go (cgo), and other languages.

- **Rust embedders** depend on `rio-vt` directly (this crate).
- **Non-Rust embedders** link `librio-vt` (C ABI) + its generated header.

## Add it

```toml
[dependencies]
rio-vt = "0.4"
```

`rio-vt` is lean by default. The `crosswords` VT logic, the parser, grid,
selection and search all compile with no windowing or renderer stack.

### Feature flags

| Feature | Default | Effect |
| --- | --- | --- |
| _(none)_ | ✓ | Lean core. Standalone `WindowId(u64)`, no renderer, no windowing. |
| `winit` | | Use `rio-window`'s real `WindowId` and event-loop proxy instead of the standalone id. |
| `renderer` | | Expose the app-level `RioErrorType::FontsNotFound` variant (pulls a `sugarloaf` font type). Enabled transitively by `rio-backend`'s renderer. |
| `x11` / `wayland` | | Forward the matching clipboard backend feature. |

## Quick start (headless)

Create a grid, feed it bytes through the parser, and read cells back. No
PTY, no renderer, no threads:

```rust
use rio_vt::ansi::CursorShape;
use rio_vt::crosswords::grid::Dimensions;
use rio_vt::crosswords::pos::Column;
use rio_vt::crosswords::{Crosswords, CrosswordsSize};
use rio_vt::event::{VoidListener, WindowId};
use rio_vt::performer::handler::Processor;

// An 80x24 terminal with 10k lines of scrollback.
let size = CrosswordsSize::new(80, 24);
let cols = size.columns();
let mut term = Crosswords::new(
    size,
    CursorShape::Block,
    VoidListener,          // ignore events; swap for your own EventListener
    WindowId::from(0),
    0,                     // route id
    10_000,                // scrollback limit
);

// Feed raw PTY bytes (SGR red "hello", reset, then " world").
let mut parser = Processor::default();
parser.advance(&mut term, b"\x1b[31mhello\x1b[0m world");

// Pull the visible grid and read the first row's text.
let rows = term.visible_rows();
let line: String = (0..cols).map(|x| rows[0][Column(x)].c()).collect();
assert!(line.starts_with("hello world"));
```

> The full, runnable version of this snippet lives at
> [`examples/quickstart.rs`](examples/quickstart.rs)
> (`cargo run -p rio-vt --example quickstart`).

## Receiving events

Instead of `VoidListener`, implement `EventListener` to react to what the
terminal emits (PTY writes, bell, title changes, clipboard requests, and so
on). `event()` is the only required method; override the `send_*` hooks you
care about:

```rust
use rio_vt::event::{EventListener, RioEvent, WindowId};

#[derive(Clone)]
struct MyListener;

impl EventListener for MyListener {
    // Required: return a pending event (used by pull-style consumers).
    fn event(&self) -> (Option<RioEvent>, bool) {
        (None, false)
    }

    // Optional: react to events the terminal pushes out.
    fn send_event(&self, event: RioEvent, _window: WindowId) {
        match event {
            RioEvent::PtyWrite(_route_id, text) => { /* write `text` to the PTY */ }
            RioEvent::Title(title) => { /* update the window title */ }
            RioEvent::Bell => { /* ring */ }
            _ => {}
        }
    }
}
```

## Driving a real PTY

`performer::Machine` wires a spawned PTY to the parser and the grid on a
reader thread, so bytes from the child process flow into `Crosswords`
automatically. See [`librio/vt/src/lib.rs`](../librio/vt/src/lib.rs) for a
complete, working setup (`Crosswords::new` + `Machine::new` +
`teletypewriter`), which is also the reference consumer of this crate.

## Pull-based rendering

`rio-vt` does not draw anything. A frontend reads terminal state on demand:

- `term.visible_rows()` returns the current viewport as `Vec<Row<Square>>`.
- `Square::c()` gives a cell's character; foreground/background/flags come
  from its style.
- Damage tracking (`TerminalDamage`) lets a renderer repaint only changed
  rows instead of the whole grid each frame.

For a complete pull implementation (dirty-row tracking, per-cell readout,
cursor and selection state), see
[`librio/vt/src/render_state.rs`](../librio/vt/src/render_state.rs).

## C ABI

To embed from a non-Rust UI, use [`librio-vt`](../librio/vt). It builds this
crate as a `staticlib` and exposes an engine/surface/render-state C API
(`rio_engine_new`, `rio_surface_text`, `rio_render_state_cell`, ...). Rio's
SwiftUI + Metal frontend drives it that way.

## Architecture

```
rio-graphics   image/graphic value types + glyph decoder (leaf, no GPU)
     |
   rio-vt      this crate: VT core (crosswords, ansi, performer, event, ...)
     |     \
rio-backend   librio-vt (C ABI)
     |
  rioterm     Rio's terminal app (adds the sugarloaf renderer + config)
```

`rio-backend` builds on `rio-vt` and re-exports every module at the same
path, so it stays the home of the user-facing config and the renderer glue
while this crate carries the reusable core.

## Safety

The public API of `rio-vt` is safe Rust. All `unsafe` and raw-pointer
handling lives in the separate `librio-vt` C-ABI shim.

## Provenance

Rio's `crosswords` module was originally derived from Alacritty's `term`
module and has been maintained and extended in Rio since. Licensed under MIT.
