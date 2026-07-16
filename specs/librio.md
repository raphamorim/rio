# librio

Embeddable terminal libraries extracted from Rio, following the decomposition
libghostty is converging on (state library shipped first, renderer as a peer
component), with one structural difference: Rio already has a working,
winit-free GPU renderer (sugarloaf), so the renderer library ships from v0
instead of being roadmap.

Two peer libraries. The PTY/terminal core and the renderer sit at the same
level; neither depends on the other's internals.

```
┌───────────────────────────┐     ┌──────────────────────────────┐
│ librio-vt                 │     │ librio-sugarloaf             │
│ PTY + VT state            │     │ renderer (metal/vulkan/cpu)  │
│ teletypewriter            │     │ sugarloaf                    │
│ rio-backend (crosswords,  │     │ grid_emit (moved from        │
│   machine, performer)     │     │   rioterm, themed)           │
└─────────────┬─────────────┘     └──────────────┬───────────────┘
              │      render-state pull API       │
              └───────────► consumed ◄───────────┘
```

`librio-sugarloaf` is a *client* of `librio-vt`'s public render-state API —
never of its internals. This is the permanent dogfood test: if our renderer
can be built on the pull API, so can a third-party CPU renderer, a wasm
canvas, a TUI proxy, or a test harness.

Both libraries are distributed two ways:

1. **Rust crates** (crates.io: `librio-vt`, `librio-sugarloaf`) with
   idiomatic Rust APIs. The C ABI is a `capi` feature on the same crates,
   not a separate wrapper crate.
2. **C ABI / xcframework** for Swift and C consumers: hand-curated headers
   (cbindgen seeds them, never auto-committed), `module.modulemap`
   (`module RioKit`), universal static libs via `lipo`, packaged with
   `xcodebuild -create-xcframework`. Later: prebuilt per-commit artifacts so
   Swift consumers never install Rust (Ghostty ships this since 2026-04).

Versioning: both libraries version independently of Rio releases, explicit
alpha, no API stability promise until tagged (libghostty's posture).

## Research grounding (2026-07, adversarially verified)

- `ghostty.h` is explicitly *not* public libghostty — build.zig calls it
  internal glue for the macOS app; the artifact installs as
  `ghostty-internal`. Lesson: expect v0 to be canario-shaped glue; plan for
  a clean-slate revision, don't promise stability.
- The only shipped standalone component is `libghostty-vt`: VT parsing +
  terminal state + a host-pulled render-state API (per-row dirty flags the
  host resets). No drawing code; the consumer renders.
- Full libghostty's roadmap adds component libraries: keyboard encoding,
  GPU rendering ("provide us with an OpenGL or Metal surface and we'll take
  care of the rest"), host-toolkit wrappers.
- Callback design worth copying verbatim: not per-event named callbacks but
  `userdata + wakeup_cb + ONE enum-tagged action_cb + clipboard callbacks +
  close_surface_cb`. Adding an action never changes the ABI struct.
- Input entry points worth copying: per-surface key (returns consumed bool),
  text, preedit (IME), mouse button/pos/scroll, set_size, set_content_scale,
  set_focus, set_occlusion.

## librio-vt

Owns the PTY (`teletypewriter`), the VT machine (`rio-backend` `Machine` /
`performer`), and the grid (`Crosswords`). Knows nothing about pixels.

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

Every `extern "C"` entry point wraps its body in `catch_unwind` — a Rust
panic must never unwind into Swift/C (UB).

Threading contract (identical to today's rioterm split): the surface owns
one PTY IO thread; `wakeup_cb`/`action_cb` may fire on it and must only
flag/schedule. All `render_state_*` calls and input calls are host-thread
safe (input posts to the IO channel; render-state update takes the
FairMutex).

## librio-sugarloaf

Owns sugarloaf and the grid translation (`grid_emit`, moved out of rioterm
with a theme struct replacing `&Renderer`). Consumes `RenderState` through
its public API only.

### Rust API

```rust
pub struct Renderer { /* sugarloaf + grid renderer + font library */ }

impl Renderer {
    pub fn new_metal(layer: *mut c_void, config: RendererConfig) -> Result<Renderer>;
    pub fn new_cpu(config: RendererConfig) -> Result<Renderer>;   // sugarloaf cpu backend
    pub fn draw(&mut self, state: &RenderState);       // host display-link tick
    pub fn resize(&mut self, px_w: u32, px_h: u32);
    pub fn set_scale(&mut self, scale: f32);
    pub fn set_theme(&mut self, theme: &Theme);
    pub fn set_font_size(&mut self, pts: f32);
    pub fn cell_metrics(&self) -> (f32, f32);          // host derives cols/rows
}
```

### C ABI

```c
typedef struct rio_renderer rio_renderer_t;

rio_renderer_t *rio_renderer_new_metal(void *ca_metal_layer,
                                       const rio_renderer_config_s*);
void rio_renderer_free(rio_renderer_t*);
void rio_renderer_draw(rio_renderer_t*, rio_render_state_t*);
void rio_renderer_resize(rio_renderer_t*, uint32_t px_w, uint32_t px_h);
void rio_renderer_set_scale(rio_renderer_t*, float);
void rio_renderer_set_font_size(rio_renderer_t*, float);
void rio_renderer_cell_metrics(const rio_renderer_t*, float *w, float *h);
```

Requires one sugarloaf addition: `MetalContext::from_layer` (configure a
caller-owned CAMetalLayer instead of attaching a new one to an NSView).

Frame pacing stays with the host — no library render thread. This matches
libghostty-vt's shipped pull model and sugarloaf's FRAMES_IN_FLIGHT
backpressure design, and deviates deliberately from full-Ghostty's internal
renderer thread.

## How canario consumes it

Canario links the xcframework (both libs) behind its existing seam:

1. **Engine**: one `rio_engine_t` per process, created at app launch with
   callbacks that trampoline into Swift closures. `wakeup_cb` sets an atomic
   dirty flag per surface and unpauses that surface's display link.
   `action_cb(SET_TITLE)` updates the panel's title in the sidebar (replaces
   the "Panel N" placeholder). `CHILD_EXITED`/`close_surface_cb` remove the
   panel through the existing `closePanel` path.
2. **Panel = surface + renderer + render state**: `SurfaceRegistry` (today a
   `[UUID: MetalHostView]`) becomes `[UUID: PanelSession]` where
   `PanelSession` owns the long-lived `MetalHostView`, a `rio_surface_t`,
   a `rio_render_state_t`, and a `rio_renderer_t` bound to the view's
   `CAMetalLayer`. Creating a panel (⌘D/⌘⇧D) spawns a real shell; closing
   frees surface+renderer.
3. **Render loop**: `MetalHostView` gains a `DisplayLinkDriver`
   (`NSView.displayLink`). Tick: if dirty →
   `rio_render_state_update` → `rio_renderer_draw` → reset dirty; pause
   after N clean ticks; resume on wakeup/focus (the same idle-stop pattern
   Rio's own window_delegate uses). Resize path already computes
   backing-pixel sizes → `rio_surface_set_size` + `rio_renderer_resize`;
   cols/rows derived from `rio_renderer_cell_metrics`.
4. **Input**: `MetalHostView` implements `NSTextInputClient`
   (keyDown → `interpretKeyEvents`): committed text → `rio_surface_text`,
   marked text → `rio_surface_preedit`, raw keys → `rio_surface_key`
   honoring the consumed bool (unconsumed keys fall through to SwiftUI
   shortcuts). Focus/occlusion forwarded from window state.
5. **Everything else stays**: sidebar tree, folders, drag-reorder, column
   grid, weights, accent border — none of it knows the black rect learned
   to run a shell.

## Implementation plan

Ordered so each phase is independently verifiable; canario never blocks on
a later phase.

- **P0 — decouple rio-backend from winit** (~4 files): make
  `event/mod.rs` `WindowId` an opaque u64, feature-gate `EventProxy`, the
  `From<EventPayload>` impl, and the config/window + config/theme
  conversion impls. Gate: rio-backend builds without rio-window; rioterm
  unchanged.
- **P1 — `librio/vt` crate, Rust API**: workspace member wrapping the
  ContextManager::create_context recipe (Crosswords::new →
  create_pty_with_spawn → Machine::new/spawn → Messenger) behind
  `Engine`/`Surface`/`SurfaceDelegate`; `RenderState` over
  `snapshot_visible` + damage bits; key encoding extracted from rioterm's
  bindings into a winit-free `KeyEvent` encoder (the hardest single item —
  Ghostty lists keyboard encoding among its component libraries for a
  reason). Gate: a headless Rust test drives a real shell (`echo hi`),
  observes cells + dirty rows through `RenderState`, receives
  `Action::SetTitle`.
- **P2 — `librio/vt` C ABI** (`capi` feature): handles via
  `Box::into_raw`, `catch_unwind` on every entry point, hand-curated
  `include/librio-vt.h` + modulemap, `staticlib` crate-type. Gate: a tiny C
  program (compiled in CI) spawns a shell and prints the grid.
- **P3 — `librio/sugarloaf` crate**: `MetalContext::from_layer` in
  sugarloaf; move/port `grid_emit` with a theme struct; `Renderer::draw`
  consuming `RenderState` (public API only). Gate: a Rust example app
  (winit or raw AppKit shim) renders a live shell — first visible librio
  terminal, before canario is touched.
- **P4 — C ABI for the renderer + packaging**: `librio-sugarloaf.h`;
  Makefile targets `librio` / `librio-xcframework`
  (cargo per-arch → lipo → `xcodebuild -create-xcframework` →
  `target/RioKit.xcframework` containing both libs + headers + modulemap).
  Gate: `make librio-xcframework` produces an artifact Xcode can import.
- **P5 — canario integration**: `PanelSession` in the registry, display-link
  driver, `NSTextInputClient` input path, action→sidebar-title wiring.
  Gate: typing `ls` into a canario panel shows real output; splits run
  independent shells; closing a panel kills its shell.
- **P6 — distribution polish** (later): publish `librio-vt` /
  `librio-sugarloaf` to crates.io (alpha), CI job building the xcframework
  per commit, minisign-signed prebuilt artifacts.

Explicitly deferred from v0: splits *inside* a surface (canario composes
surfaces instead), search/hints overlays, kitty/sixel graphics forwarding,
scrollback introspection API, selection API (first planned breaking
addition — same gap libghostty-vt has), config-file loading in
`rio_config_t` (starts as a programmatic struct; "load rio config.toml"
is a convenience to add later).
