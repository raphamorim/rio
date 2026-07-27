# librio

`librio` is an embeddable terminal library extracted from Rio: the PTY and VT
state core, usable from Rust, Swift, and C without pulling in a windowing
stack.

It began as a research exercise studying two prior art decompositions — the
libghostty split (a terminal-state library shipped first, renderer as a later
peer) and [libvterm](https://www.leonerd.org.uk/code/libvterm/) — then kept the
parts that fit Rio's existing engine and dropped the rest.

The current scope is a single library, **`librio-vt`** — PTY + VT state with a
host-pulled render-state API. A renderer peer built on sugarloaf is a possible
later addition but is out of scope here.

Distribution, two ways:

1. **Rust crate** (`librio-vt` on crates.io) with an idiomatic Rust API. The C
   ABI is a `capi` feature on the same crate, not a separate wrapper crate.
2. **C ABI / xcframework** for Swift and C consumers: hand-curated headers
   (cbindgen seeds them, never auto-committed), `module.modulemap`
   (`module RioKit`), universal static libs via `lipo`, packaged with
   `xcodebuild -create-xcframework`.

Versioning: `librio-vt` versions independently of Rio releases; no API
stability promise until tagged.

## librio-vt

Owns the PTY (`teletypewriter`), the VT machine (`Machine` / `performer`), and
the grid (`Crosswords`). Knows nothing about pixels: the host pulls a
render-state snapshot with per-row dirty flags (which the host resets) and
draws it however it likes. This pull model is the permanent dogfood test — if
our own renderer can be built on it, so can a third-party CPU renderer, a wasm
canvas, a TUI proxy, or a test harness.

The callback surface is deliberately small: `userdata` + a `wakeup_cb` + a
single enum-tagged `action_cb` + clipboard and close-surface callbacks. Adding
a new action never changes the ABI struct. Input is a per-surface set of entry
points: key (returns a consumed bool), text, preedit (IME), mouse
button/pos/scroll, set_size, set_content_scale, set_focus, set_occlusion.

### Rust API (primary)

```rust
pub struct Engine { /* shared config, action fan-out */ }
pub struct Surface { /* one PTY + grid; owns the IO thread */ }
pub struct RenderState { /* snapshot + dirty rows */ }

pub trait SurfaceDelegate: Send + Sync {
    fn wakeup(&self);                                  // any thread; schedule only
    fn action(&self, surface_id: SurfaceId, action: Action);
    fn clipboard_write(&self, kind: ClipboardKind, text: &str);
    fn close_surface(&self, surface_id: SurfaceId);
}

pub enum Action {
    SetTitle(String),
    RingBell,
    ChildExited(i32),
    CursorShape(CursorShape),
    // grows freely
}

impl Surface {
    pub fn new(engine: &Engine, desc: SurfaceDesc) -> Result<Surface>;
    pub fn key(&self, event: KeyEvent) -> bool;        // true = consumed
    pub fn text(&self, s: &str);
    pub fn preedit(&self, s: &str);
    pub fn mouse_button(&self, ...); pub fn mouse_pos(&self, ...);
    pub fn scroll(&self, dx: f64, dy: f64);
    pub fn set_size(&self, cols: u16, rows: u16, px_w: u32, px_h: u32);
    pub fn set_content_scale(&self, scale: f64);
    pub fn set_focus(&self, focused: bool);
    pub fn set_occlusion(&self, occluded: bool);
}

impl RenderState {
    pub fn new(surface: &Surface) -> RenderState;
    pub fn update(&mut self);                          // snapshot under the terminal lock
    pub fn dims(&self) -> (u16, u16);
    pub fn row_dirty(&self, row: u16) -> bool;
    pub fn reset_dirty(&mut self);
    pub fn cell(&self, row: u16, col: u16) -> Cell;    // materialized view
    pub fn row_cells(&self, row: u16) -> &[Cell];
    pub fn cursor(&self) -> Cursor;
    pub fn selection(&self) -> Option<SelectionRange>;
    pub fn palette(&self) -> &Palette;
}
```

### C ABI (feature `capi`)

```c
typedef struct rio_engine        rio_engine_t;
typedef struct rio_config        rio_config_t;
typedef struct rio_surface       rio_surface_t;
typedef struct rio_render_state  rio_render_state_t;

typedef enum {
  RIO_ACTION_SET_TITLE, RIO_ACTION_RING_BELL,
  RIO_ACTION_CHILD_EXITED, RIO_ACTION_CURSOR_SHAPE,
} rio_action_tag_t;
typedef struct { rio_action_tag_t tag; /* tagged union payload */ } rio_action_s;

typedef struct {
  void *userdata;
  void (*wakeup_cb)(void *ud);                    /* any thread; schedule only */
  void (*action_cb)(void *ud, rio_surface_t*, rio_action_s);
  void (*clipboard_write_cb)(void *ud, int kind, const char *utf8);
  void (*close_surface_cb)(void *ud, rio_surface_t*);
} rio_runtime_config_s;

rio_engine_t  *rio_engine_new(const rio_runtime_config_s*, const rio_config_t*);
void           rio_engine_free(rio_engine_t*);

rio_surface_t *rio_surface_new(rio_engine_t*, const rio_surface_config_s*);
void           rio_surface_free(rio_surface_t*);
bool rio_surface_key(rio_surface_t*, rio_key_event_s);   /* true = consumed */
void rio_surface_text(rio_surface_t*, const char*, uintptr_t);
void rio_surface_preedit(rio_surface_t*, const char*, uintptr_t);
void rio_surface_mouse_button(rio_surface_t*, rio_mouse_event_s);
void rio_surface_mouse_pos(rio_surface_t*, double x, double y);
void rio_surface_scroll(rio_surface_t*, double dx, double dy);
void rio_surface_set_size(rio_surface_t*, uint16_t cols, uint16_t rows,
                          uint32_t px_w, uint32_t px_h);
void rio_surface_set_content_scale(rio_surface_t*, double);
void rio_surface_set_focus(rio_surface_t*, bool);
void rio_surface_set_occlusion(rio_surface_t*, bool);

rio_render_state_t *rio_render_state_new(rio_surface_t*);
void rio_render_state_free(rio_render_state_t*);
void rio_render_state_update(rio_render_state_t*);
bool rio_render_state_row_dirty(const rio_render_state_t*, uint16_t row);
void rio_render_state_reset_dirty(rio_render_state_t*);
void rio_render_state_dims(const rio_render_state_t*, uint16_t *cols, uint16_t *rows);
const rio_cell_s *rio_render_state_row(const rio_render_state_t*, uint16_t row,
                                       uintptr_t *out_len);
rio_cursor_s rio_render_state_cursor(const rio_render_state_t*);
```

Every `extern "C"` entry point wraps its body in `catch_unwind` — a Rust panic
must never unwind into Swift/C (UB).

Threading contract (identical to today's rioterm split): the surface owns one
PTY IO thread; `wakeup_cb`/`action_cb` may fire on it and must only
flag/schedule. All `render_state_*` calls and input calls are host-thread safe
(input posts to the IO channel; render-state update takes the FairMutex).
