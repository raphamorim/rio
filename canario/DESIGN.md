# canario — Design Document

> **canario** — Rio's headless VT terminal-engine crate. Renderer-agnostic, window-agnostic, config-agnostic. *Bytes in, terminal state out.*

---

## 1. Goal & positioning

**Elevator pitch:** *canario is a correct, fast, embeddable VT/ANSI terminal-state engine extracted from Rio — the Rust answer to `alacritty_terminal` / `wezterm-term` / `libghostty-vt` — that you feed PTY bytes and read a packed cell grid, cursor, scrollback, selection, and graphics from, with zero coupling to a GPU renderer, window system, or TOML config.*

### What canario IS

- A **headless terminal core**: a VT500 parser, a strongly-typed semantic `Handler`, and an in-memory screen model (`Grid` of packed `Square` cells, scrollback, alt-screen, modes, colors, graphics protocols).
- **Renderer-agnostic**: it produces *renderable state* and *damage*, never pixels, never glyph shaping, never a GPU buffer.
- **The pull side of the render contract**: a per-row sequence-number / damage model plus a `snapshot_visible`-style copy-out so a renderer can read what changed from out under the lock (and on another thread).
- **Optionally** the PTY *driver* (the `Machine` event loop over `teletypewriter`), behind a feature flag — but the engine core never owns the PTY read side.
- The crate Rio itself depends on, so `rio-backend` shrinks to a thin frontend adapter.

### What canario is NOT

- **No renderer.** No `sugarloaf`, no `wgpu`, no glyph shaping, no font rasterization. Glyph-protocol outline decoding is reached through an injected trait, never owned.
- **No window/event-loop coupling.** No `rio_window`, no `WindowId`, no `EventLoopProxy` in the engine. `WindowId` becomes an opaque associated type on the host trait.
- **No config crate.** No serde TOML, no `config::Colors`, no `ColorBuilder`. The engine indexes a plain `[ColorRgb; N]` palette the host fills in.
- **No clipboard/OS integration.** No `copypasta`, no `raw_window_handle`. The engine *reports* clipboard intent through a callback; the host does the OS work.
- **No selection policy / vi-mode policy / search UI.** It exposes the *primitives* (selection geometry, regex search, stable row anchors); the frontend owns gestures and key bindings.
- **No async runtime.** No tokio. Blocking I/O only (consistent with Rio's house rules). The PTY driver uses `corcovado` (mio fork) polling on a dedicated thread.

### Who consumes it

| Consumer | Uses |
|---|---|
| **Rio** | full engine + PTY driver + graphics + damage snapshot, rendered by sugarloaf |
| **TUI multiplexers / proxies** (tmux/zellij-style) | parser + grid + scrollback + a re-emission/diff adapter to drive a downstream real terminal |
| **Headless test harnesses** | `Terminal::new` + `advance(bytes)` + assert on cells/cursor — esctest/vttest-style conformance, fuzzing, golden snapshots |
| **Record/replay & session save** | serialize screen/scrollback state; replay byte logs deterministically |
| **CI log viewers / embedders** | a *minimal* feature set (no graphics, no search) that pays nothing for what it doesn't use |

---

## 2. Comparative landscape

| | **vt100** | **alacritty_terminal** | **wezterm-term** | **libghostty-vt** | **xterm.js core** | **→ canario** |
|---|---|---|---|---|---|---|
| **Scope** | parser + screen, output-only, no input | parser (vte) + headless engine + opt PTY loop | headless engine + input encoding + PTY write | full headless engine + RenderState | engine (`common`) + opt renderer | headless engine + input encoding + opt PTY driver |
| **Memory model** | `Vec<Row>` + `VecDeque` scrollback; 32 B inline cell, style by value | `Grid<T>` ring buffer, `zero`-rotation; 24 B Arc-extra cell | `VecDeque<Line>` unified; `TeenyString` u64 cell + boxed FatAttrs | **paged** mmap'd blocks, offset-addressed; 8 B cell + per-page ref-counted side tables | `CircularList<BufferLine>`; flat `Uint32Array` 12 B/cell + sparse side maps | **Rio's `Grid<Square>` ring buffer; 8 B packed `Square` + per-grid `StyleSet`/`ExtrasTable`** (paged is a deferred option) |
| **Parser style** | `vte` (low) + `Perform` impl (high) | `vte` state machine + `vte::ansi` `Handler` | typed-AST `Action` + `Performer` apply | comptime-Handler `Stream`, vt100.net table | Uint16 table + `InputHandler`, registrable hooks | **Rio's forked VT500 state machine + `Handler` trait + `Processor` (sync-update aware)** |
| **Host integration** | `Callbacks` trait, no-op blanket impl | `EventListener::send_event` (viral generic) | small object-safe traits set post-construction (`Clipboard`, `AlertHandler`, …) + `Arc<dyn Config>` | embedder-set C function pointers + `sys.*` hooks | DI container + `onData`/event emitters | **object-safe `TerminalHost` trait set, opaque `WindowId` assoc type; `Arc<dyn TerminalConfig>` w/ generation counter** |
| **Graphics protocols** | none | none upstream (Rio added all) | sixel + iTerm2 + kitty (feature-gated) | kitty (first-class) + sixel via DCS | none in core (addons) | **sixel + iTerm2 + kitty (+ glyph protocol), feature-gated; payloads renderer-agnostic** |
| **Reuse mechanism** | crates.io, monolithic, no features | crates.io, feature-gated layers, generic listener | crates.io, layered building-block crates, trait objects | Zig module + C-ABI + WASM | npm `@xterm/headless` + addons | **crates.io, feature-gated; trait-object host; C-ABI/WASM as future targets** |
| **Reflow on resize** | ✗ (caused mprocs fork) | ✓ | ✓ | ✓ (hardest code) | ✓ | **✓ — Rio already has `grid/resize.rs`; keep it faithful** |
| **Damage model** | clone-and-diff | per-line bounds + `damage()` iterator | per-line `SequenceNo` pull model | layered dirty bits + `RenderState` snapshot | dirty row-range | **per-row dirty + `TerminalDamage` enum + `snapshot_visible` copy-out** |

### What canario borrows from each

- **From alacritty_terminal (its lineage):** the `Grid<T>` ring buffer with **O(1) `zero`-rotation scroll**, the two-layer parser/`Handler` split, `StableRowIndex`-style anchors. This is the base — Rio already forked it.
- **From libghostty-vt:** the **tiny 8-byte cell + ref-counted, deduplicated per-grid side tables** (Rio's `Square` + `StyleSet`/`ExtrasTable` already realize this), the **`ContentTag` bg-only fast path**, layered **false-positive-only dirty tracking**, the **persistent `RenderState`/`snapshot_visible`** boundary instead of per-frame clone, and **side-effects-as-callbacks**. The paged-memory model is borrowed as a *deferred* option, not v1.
- **From wezterm-term:** the **per-line `SequenceNo` pull damage model** as the *primary* render contract; **small, object-safe, post-construction host traits** carrying a rich `Alert` enum rather than a trait-per-event; **`Arc<dyn Config>` with a `generation()` counter**; compile-time-distinct **index newtypes** (`PhysRowIndex`/`VisibleRowIndex`/`StableRowIndex`); engine owns the **PTY write side only**.
- **From vt100:** the **escape-code re-emission / `contents_diff`** path as a *first-class optional adapter* (the multiplexer/proxy use case and free golden tests); the **never-silently-drop-unknown-sequences** principle (forward to host hooks).
- **From xterm.js:** **ship and test the headless target first**; the **registrable parser-hook extension model** (`register_csi/osc/dcs/esc`) as the *non-fork* extension seam; **decode UTF-8 → codepoints before the state machine** (Rio's `simd_utf8` already does this); coarse **row-range damage is enough**.

---

## 3. Architecture

### 3.1 Crate tree

canario ships as a small **workspace**, not one monolith, so a minimal embedder pays nothing and the heavy/leaky value types live in one place:

```
canario/                         (workspace)
├── rio-core/               # leaf crate, zero engine deps — the "decoupling landing zone"
│   ├── color.rs                 # ColorRgb, AnsiColor, NamedColor  (moved OUT of config::colors)
│   ├── graphics.rs              # GraphicData, GraphicId, ColorType, ResizeCommand,
│   │                            #   ResizeParameter, MAX_GRAPHIC_DIMENSIONS (moved OUT of sugarloaf)
│   └── geom.rs                  # Pos, Line, Column, index newtypes
│
├── rio-parser/              # the VT500 state machine, alone (xterm.js/wezterm pattern)
│   ├── parser/{mod.rs,params.rs}# forked vte state machine — ZERO deps but std::str + simd_utf8
│   └── lib.rs                   # Parser, advance, advance_until_terminated
│
├── canario/                     # THE ENGINE (the crate everyone depends on)
│   ├── ansi/                    # control.rs, mode.rs, charset.rs, glyph_protocol.rs
│   ├── handler.rs               # `Handler` trait (the command surface) + `Processor` (sync-aware)
│   ├── osc.rs                   # OSC dispatcher
│   ├── grid/                    # mod.rs, row.rs, storage.rs (ring buffer), resize.rs (reflow), tests.rs
│   ├── square.rs                # packed Square(u64) + Extras side-table
│   ├── style.rs                 # StyleSet / StyleId  (interned styles)
│   ├── attr.rs                  # SGR Attr
│   ├── term/                    # mod.rs = `Terminal<H>` (== Rio's Crosswords) + Handler impl
│   │   ├── colors.rs            # TermColors as a config-free [ColorRgb; N] table
│   │   ├── modes.rs             # ANSI + DEC-private modes, charsets, saved-cursor, tabstops
│   │   └── damage.rs            # TerminalDamage, per-row dirty, snapshot_visible
│   ├── selection.rs             # selection geometry (primitives only)
│   ├── search.rs                # regex/semantic/bracket search
│   ├── vi_mode.rs               # vi cursor primitives
│   ├── host.rs                  # TerminalHost trait, Alert enum, TerminalConfig trait
│   └── codepoint_width.rs       # unicode-width table
│
├── canario-graphics/            # feature: sixel, iterm2, kitty, kitty_virtual decoders
│                                #   (depend on rio-core::graphics, NOT sugarloaf)
├── canario-pty/                 # feature: Machine event loop over teletypewriter + corcovado
└── canario-replay/              # feature: contents_formatted / contents_diff re-emission adapter (vt100-style)
```

**Cargo features on `canario`:**
`graphics` (pulls `canario-graphics`), `pty` (pulls `canario-pty`), `search`, `vi-mode`, `selection`, `replay`, `serde` (serialize cells/grid for session save & mux transport), `simd` (simd_utf8 fast path, with a pure-Rust `std::str::from_utf8` fallback for exotic targets). A CI-log-viewer embedder builds with **none** of these.

### 3.2 Screen/grid memory model — **decision: keep Rio's ring-buffer `Grid<Square>` for v1; design for a paged backend later**

**Recommendation: ship v1 on Rio's existing alacritty-derived ring buffer (`Grid<Square>` over `Storage<Square>`), and treat the libghostty paged model as a *post-1.0* internal swap, not a v1 goal.**

Justification:

- **The ring buffer is already battle-tested in Rio.** `grid/{row,storage,resize}.rs` are ~100% pure generic and move cleanly (per the coupling map). Re-deriving reflow and wide-char/soft-wrap bookkeeping from scratch is the #1 bug source in every emulator surveyed; keep Rio's `resize.rs` **byte-for-byte** and port its `tests.rs`.
- **Rio's `Square` already wins the cell-size battle.** The packed 8-byte `Square(u64)` + per-grid `StyleSet`/`ExtrasTable` + `ContentTag` bg-only fast path is *exactly* libghostty's "tiny cell + ref-counted side tables" lesson, achieved without paging. We get Ghostty's cache footprint without Ghostty's offset-addressing complexity (4 GB page cap, base-pointer threading, pin-garbage tracking, `verifyIntegrity` debug-only invariants).
- **O(1) scroll is preserved.** `Storage::rotate` does modular arithmetic on `zero`; do **not** regress to `rotate_left`. Keep the lazy-grow / non-truncating-shrink scrollback and the `truncate()`-before-`PartialEq` precondition (needed for serde session save).
- **Paged memory is a real future win** (cheap scrollback eviction, serializable pages, render-thread snapshots) but it's a large, invariant-heavy rewrite. The `Grid<T>` is generic over the cell *and* its storage shape enough that we can introduce a `trait GridBackend` seam **after** the public API is stable, swapping the ring buffer for pages without touching `Handler` or the public surface. Don't block the extraction on it.

The two grids (primary + `inactive_grid` for alt-screen) and `swap_alt()` stay. Alt-screen has scrollback disabled; tests must cover swap-in/swap-out without losing or sharing primary scrollback.

### 3.3 Parser + Handler split

Keep Rio's existing **three-layer** shape, promote it to public:

1. **`rio-parser`** — the forked VT500 state machine. Zero terminal semantics. Decodes UTF-8 → `u32` codepoints *before* the table (Rio's `simdutf::convert_utf8_to_utf32_with_errors`, feature-gated with a std fallback). Exposes the **bulk hooks** `print_str` / `print_codepoints` (default impls fall back to per-char) so a whole printable run is one batched cell-write. Inline OSC buffer (2048) with overflow spill.
2. **`Handler` trait** (~150 methods, almost all defaulted no-op) — the semantic command surface: `input`/`input_codepoints`, `goto`, `terminal_attribute(Attr)`, `clear`, `scroll`, `set_title`, `insert_graphic(GraphicData)`, `glyph_register`, mode set/reset, color set/query, clipboard, etc. **Never silently drop** an unrecognized CSI/OSC — forward it through a defaulted `unhandled_*` hook so a host can extend without forking (vt100/xterm.js lesson; the mprocs fork is the cautionary tale).
3. **`Processor`** — wraps the parser, drives a `Performer` adapter that translates parser events into `Handler` calls, and owns **synchronized-update (DEC 2026) buffering**: `advance`, `advance_sync`/`stop_sync`/`sync_timeout`/`sync_bytes_count`. This stays on the public surface — required to avoid tearing with modern TUIs.

The single concrete `Handler` impl is `Terminal<H: TerminalHost>` (== Rio's `Crosswords`). The clean seam every consumer touches is **`Processor::advance(&mut impl Handler, &[u8])`**.

> We keep Rio's raw-`Handler`-params style rather than adopting wezterm's pre-parsed typed-`Action` AST: it's what Rio has, the perf is good, and the bulk-codepoint path matters more than param ergonomics. The registrable-hook extension model (xterm.js) is added *on top* via the `unhandled_*` forwarding, not by rewriting the dispatch.

### 3.4 Damage / diff model

**Primary contract: per-row dirty flag + a coarse `TerminalDamage` enum + a `snapshot_visible` copy-out** (Rio's current model, validated by libghostty's `RenderState` and wezterm's seqno pull model).

```rust
pub enum TerminalDamage { Noop, Full, Partial(/* dirty row set */), CursorOnly }
```

- Each `Row` carries `dirty: bool` (Rio's storage-layer addition over upstream). Mutation sets it; `snapshot_visible(&damage, cols, dst, style_table, extras)` copies **only dirty rows** plus a style-table snapshot + extras map into a render-owned buffer — **so the terminal lock is released before any GPU work**. This is the key property that lets a render thread run without holding the PTY `FairMutex`.
- A **monotonic `SequenceNo`** (wezterm's lesson) is stamped per-`Terminal` per `advance` *and* per-`Row`, so a renderer that remembers its last-seen seqno can ask "what changed?" without diffing. The whole-terminal seqno alone is *not* enough granularity — track per-row.
- Dirty tracking is **false-positive-only, never false-negative** (libghostty rule): a sticky per-row "styled" hint may over-report, never under-report.
- The vt100-style **clone-and-diff is rejected as the primary path** (allocation-heavy, doesn't scale). It survives *only* as the optional `canario-replay` adapter for the multiplexer/proxy use case, where `contents_diff(&prev)` against a snapshot emits escape sequences to drive a downstream terminal — and incidentally gives golden/snapshot tests for free.

Tradeoff to **document explicitly** (it's a known Rio gap): damage is **row-granular, not column-granular**. Rio reduced alacritty's `LineDamageBounds{left,right}` to a boolean. A single dirty cell repaints its whole row. Fine for the snapshot approach; embedders wanting per-cell GPU diffs keep their own shadow grid.

### 3.5 Scrollback, selection, modes

- **Scrollback:** ring-buffer `Storage`, byte/line-budgeted, `display_offset` viewport, lazy-grow / non-truncating-shrink. `StableRowIndex` (monotonic-from-top, advances as lines purge) is the anchor handed to the host for selection/scroll/search that must survive eviction.
- **Selection:** geometry **primitives only** (`selection.rs`), feature-gated. The engine reports selection ranges and `selection_to_string`; gestures, highlight rendering, and copy live in the frontend. Crucially, **decouple selection from the `<H>` generic** — Rio's `selection.rs` is generic over `Crosswords<U>` *purely to reach the grid*, transitively dragging in the event+window chain. Make selection operate on `&Grid<Square>` so it carries no host type.
- **Modes:** one **`TerminalModes` struct** holding ANSI modes, DEC-private modes (app cursor/keypad, bracketed paste, origin, insert, wraparound, mouse-reporting mode+encoding), G0–G3 charsets, saved-cursor (DECSC/DECRC), tabstops, protected attrs — all in *one* well-defined place so `reset()`/soft-reset (DECSTR) are correct and modes don't leak across the parser/buffer boundary (xterm.js pitfall). Modes are **queryable** on `Terminal`; the engine reports which mouse protocol/encoding is active, the **host encodes the actual mouse/key event**.

### 3.6 Graphics protocols

All graphics live in `canario-graphics` behind the `graphics` feature. The decoders (sixel, iterm2, kitty, kitty_virtual, glyph protocol) are **engine logic** but currently import `sugarloaf` graphics value types. The fix is **type relocation, not redesign**:

- `GraphicData`, `GraphicId`, `ColorType`, `ResizeCommand`, `ResizeParameter`, `MAX_GRAPHIC_DIMENSIONS` move **out of sugarloaf into `rio-core::graphics`** (they have no GPU/serde logic — `GraphicId` is a plain `struct GraphicId(u64)`).
- Decoded image **pixel payloads stay renderer-agnostic**: `GraphicData` is a plain `{ id, color_type, width, height, pixels: Vec<u8> }` value type. The engine *produces* it and hands it to the host via `Handler::insert_graphic` / an `Alert::Graphics` event; the host uploads it to a texture. Large pixel blobs are **never** folded into the cell or grid memory (libghostty lesson) — the cell's `Extras` side-table holds only a small `GraphicId` reference.
- The **Glyph Protocol** outline decode (Rio calls `sugarloaf::font::glyf_decode` directly) is reached through an injected **`GlyphDecode` trait** the frontend implements; the engine never owns the sugarloaf font type (see §5 blockers).

---

## 4. Public API sketch

```rust
// ───────────────────────── host integration (the decoupling boundary) ─────────────────────────

/// Everything the engine needs from its embedder. Object-safe; methods defaulted no-op.
/// One trait, set at construction — modeled on wezterm's small-trait-set, but folded into one
/// object so `Terminal` is not generic-viral the way alacritty's `EventListener` is.
pub trait TerminalHost {
    /// Opaque routing/window identity — canario never names rio_window::WindowId.
    type WindowId: Copy + Send + 'static;

    /// Engine → program reply bytes (DA/DSR, color queries, bracketed paste, mouse/key reports).
    /// The host writes these to the PTY. The engine owns the WRITE side conceptually but does no I/O.
    fn write_pty(&mut self, _id: Self::WindowId, _bytes: &[u8]) {}

    /// Out-of-band side effects, all through ONE rich enum (bell, title, cwd, progress,
    /// clipboard store/load, color request, cursor-blink change, desktop notification, damaged).
    fn alert(&mut self, _id: Self::WindowId, _alert: Alert) {}

    /// Decoded graphic ready for the host to upload to a texture (feature = "graphics").
    fn insert_graphic(&mut self, _id: Self::WindowId, _g: rio_core::GraphicData) {}

    /// Glyph-protocol outline decode is injected — engine never owns the font type.
    fn decode_glyph(&mut self, _payload: &[u8]) -> Result<GlyphHandle, GlyphReject> { Err(GlyphReject::Unsupported) }
}

pub enum Alert {
    Bell,
    TitleChanged(String),
    CurrentDirChanged(String),
    ClipboardStore { kind: ClipboardKind, data: String },
    ClipboardLoad  { kind: ClipboardKind },          // host replies via Terminal::paste_clipboard
    ColorRequest(usize),                             // host replies via Terminal::set_color
    Progress(ProgressReport),
    CursorBlinkingChanged,
    Damaged,                                          // lightweight render nudge
    Notification { title: String, body: String },
}

/// Config as a single trait with defaulted methods + a generation counter (wezterm pattern).
/// New knobs are added as defaulted methods — non-breaking.
pub trait TerminalConfig: Send + Sync {
    fn generation(&self) -> u64 { 0 }
    fn scrollback_lines(&self) -> usize { 10_000 }
    fn palette(&self) -> &[rio_core::ColorRgb; 256];
    fn unicode_version(&self) -> u8 { 9 }
    fn kitty_keyboard(&self) -> bool { false }
    // ... all defaulted; engine re-reads lazily and flushes caches when generation bumps.
}

// ───────────────────────── the engine ─────────────────────────

pub struct Terminal<H: TerminalHost> { /* grid: Grid<Square>, modes, colors, damage, host, ... */ }

impl<H: TerminalHost> Terminal<H> {
    pub fn new(dims: Dimensions, config: Arc<dyn TerminalConfig>, host: H, id: H::WindowId) -> Self;

    // ── feed: bytes in ──
    pub fn advance(&mut self, bytes: &[u8]);                 // wraps Processor::advance over self
    pub fn perform_actions(&mut self, actions: &[Action]);  // skip the byte parser (mux/testing)

    // ── read: state out ──
    pub fn grid(&self) -> &Grid<Square>;
    pub fn cursor(&self) -> CursorState;                    // pos, shape, visibility, blink, seqno
    pub fn modes(&self) -> &TerminalModes;
    pub fn colors(&self) -> &TermColors;                    // config-free [ColorRgb; N]
    pub fn title(&self) -> Option<&str>;
    pub fn current_dir(&self) -> Option<&str>;

    // ── damage / render contract (pull model) ──
    pub fn seqno(&self) -> SequenceNo;
    pub fn damage(&self) -> TerminalDamage;                 // Noop | Full | Partial(rows) | CursorOnly
    pub fn reset_damage(&mut self);
    /// Copy only dirty rows + a style snapshot into a render-owned buffer; lock released after.
    pub fn snapshot_visible(&self, dmg: &TerminalDamage, dst: &mut RenderSnapshot);

    // ── control ──
    pub fn resize(&mut self, dims: Dimensions);             // reflows wrapped lines (grid/resize.rs)
    pub fn scroll_display(&mut self, scroll: Scroll);
    pub fn reset(&mut self);                                // RIS; soft_reset() for DECSTR

    // ── host replies back into the engine ──
    pub fn set_color(&mut self, index: usize, color: rio_core::ColorRgb);
    pub fn paste_clipboard(&mut self, kind: ClipboardKind, data: &str);

    // ── input encoding (engine encodes; host delivers raw events & writes the bytes) ──
    #[cfg(feature = "selection")] pub fn selection(&self) -> Option<&Selection>;
    #[cfg(feature = "search")]    pub fn search(&self, regex: &str, dir: Direction) -> Option<Match>;
}

// ───────────────────────── PTY driver (feature = "pty", optional) ─────────────────────────

/// The Machine event loop, lifted verbatim from performer/mod.rs but generic over the host trait.
/// teletypewriter + corcovado move into canario-pty unchanged (they carry no sugarloaf/config deps).
#[cfg(feature = "pty")]
pub struct Machine<T: teletypewriter::EventedPty, H: TerminalHost> { /* pty: T, term: FairMutex<Terminal<H>> */ }

#[cfg(feature = "pty")]
impl<T: teletypewriter::EventedPty, H: TerminalHost> Machine<T, H> {
    pub fn spawn(self);  // dedicated thread: corcovado::Poll loop → pty_read → lock → term.advance(buf)
}
```

**Consumer drive loop (Rio or a TUI):**

```rust
let term = Terminal::new(dims, config, my_host, window_id);
// PTY thread (feature="pty"): Machine::spawn pumps pty bytes → term.advance(...) under FairMutex,
//   and fires Alert::Damaged once per batch (guarded by an in-flight flag).
// Render thread: on Damaged, lock briefly → term.snapshot_visible(&term.damage(), &mut snap)
//   → unlock → paint snap with sugarloaf. Reads NEVER hold the lock across GPU work.
// Input: host gets a click/key → encodes per term.modes() → host.write_pty(bytes).
```

**Headless test harness (no features):**

```rust
let mut t = Terminal::new(Dimensions::new(24, 80), Arc::new(TestConfig), VoidHost, ());
t.advance(b"\x1b[31mhello\x1b[0m");
assert_eq!(t.grid()[Line(0)][Column(0)].fg(), AnsiColor::Named(NamedColor::Red));
```

`VoidHost` (the `()`-style no-op `TerminalHost` with `WindowId = ()`) is provided for tests, fuzzing, and batch use — the canario analog of wezterm's `VoidListener` and vt100's `()` blanket impl.

---

## 5. Dependency & decoupling policy

The coupling map confirms there is **no cyclic crate dependency** (sugarloaf does not depend on rio-backend). This is a **type-relocation + trait-injection** problem, mechanical but wide (~30 in-tree `sugarloaf::` / `config::colors::` references). Five severances, each tied to a confirmed blocker:

### Severance 1 — Colors out of `config`
`config::colors::mod.rs:14` defines `pub type ColorWGPU = sugarloaf::Color` and `ColorRgb::to_wgpu()`/`to_composition()` return sugarloaf types — *verified in source*. Yet `AnsiColor`/`NamedColor`/`ColorRgb` are referenced by `attr.rs:1`, `style.rs`, `handler.rs:6`, `osc.rs:13`, `sixel.rs`, `kitty_virtual.rs`, `grid/mod.rs:522`, and the `Square` discriminant.
→ **Move the pure color value types into `rio-core::color`. Strip the `to_wgpu`/`to_composition` helpers from the engine-facing surface** (they're frontend concerns; the frontend keeps its `ColorWGPU` alias and converts from `rio_core::ColorRgb`).

### Severance 2 — `TermColors` off `config::Colors`
`Crosswords.colors: TermColors` (`mod.rs:437`, verified) → `config/colors/term.rs` pulls `config::Colors` + `ColorBuilder`. The runtime palette the engine indexes every cell against is defined in terms of the serde config struct.
→ **Reduce `TermColors` to a config-free `[ColorRgb; N]` table** the frontend fills via `TerminalConfig::palette()`. The engine never sees serde or `ColorBuilder`.

### Severance 3 — Graphics value types out of `sugarloaf`
`crosswords/mod.rs:62` `use sugarloaf::{GraphicData, MAX_GRAPHIC_DIMENSIONS}` (verified), `handler.rs:13`, `square.rs:204` (`Extras.graphic` → `GraphicCell.texture: Arc<TextureRef{ id: sugarloaf::GraphicId }>`), and all four graphics decoders.
→ **Relocate `GraphicData`/`GraphicId`/`ColorType`/`ResizeCommand`/`ResizeParameter`/`MAX_GRAPHIC_DIMENSIONS` into `rio-core::graphics`.** They have no GPU logic. The `Square` packed word itself is already clean — only the `Extras` side-table leaks, and it only needs the relocated `GraphicId`.

### Severance 4 — `GlyphRegistry` out of the core struct (the heaviest blocker)
`Crosswords.glyph_registry: Option<sugarloaf::font::glyph_registry::GlyphRegistry>` (`mod.rs:449`, verified) and `RioEvent::GlyphProtocolInstalled{ registry: GlyphRegistry }` (`event/mod.rs:97`) put a sugarloaf **font** type (backed by `glyf_decode`/`colr_raster` → `ttf-parser`/`tiny-skia`) inside the engine struct *and* its public event enum. `glyph_register` (`mod.rs:4114`) calls `sugarloaf::font::glyf_decode` directly.
→ **Remove the field. Decode glyph-protocol payloads through the injected `TerminalHost::decode_glyph` trait method**, returning an opaque `GlyphHandle` the engine stores by id. The engine names no sugarloaf font type. The event variant carries the opaque handle, not `GlyphRegistry`.

### Severance 5 — `WindowId` / `EventListener` off `rio_window`
`event/mod.rs:10,18,20` make `WindowId = rio_window::window::WindowId` and wrap `EventLoopProxy`. `WindowId` is threaded through `Crosswords` (`mod.rs:453`, verified), `Machine` (`mod.rs:72`), every `send_event`, and even `search.rs` tests.
→ **Make `WindowId` an opaque associated type on `TerminalHost`** (`type WindowId`). Replace `event_proxy: U` + `RioEvent::send_event` with `host: H` + `host.alert(...)` / `host.write_pty(...)`. Split the fat `RioEvent` (which leaks `WindowId`, `route_id`, `sugarloaf::GlyphRegistry`, `UpdateQueues`, `UpdateFontSize`, `ToggleFullScreen`, `CreateNativeTab`) into a small terminal-semantic `Alert`; **all UI/window events stay in the frontend.** `VoidListener` already proves the trait can be frontend-free; the `WindowId` alias was the only blocker.

### What stays in the frontend (`rio-backend`/`rioterm`)
The serde TOML `config/*` (incl. `config::Colors` and the `ColorWGPU = sugarloaf::Color` alias), the `copypasta` OS clipboard impl, `raw_window_handle`, `rio_window`, the sugarloaf font machinery (`GlyphRegistry`/`glyf_decode`/`colr_raster`), and `lib.rs:16 pub use sugarloaf`. `rio-backend` becomes a thin adapter: it implements `TerminalHost` and `TerminalConfig`, maps `Alert` → `RioEvent`, and converts `rio_core::ColorRgb` → `ColorWGPU` at the render boundary.

### Resulting dependency set
`canario` depends on `{ rio-core, rio-parser, bitflags, bytemuck, rustc-hash, smallvec, unicode-width, parking_lot }` + optionally `{ regex/onig (search), base64 (glyph), teletypewriter, corcovado (pty), serde }`. It depends on **none of** sugarloaf, config, rio-window, copypasta.

### no_std / wasm / C-ABI ambitions
- **no_std:** *not* a v1 goal (the engine uses `Vec`/`VecDeque`/`parking_lot`/`std::io::Write` and the PTY driver is inherently OS-coupled). But `rio-core` and `rio-parser` should be **kept `alloc`-only-capable** (no `std` in their hot paths) so a future no_std parser-only build is feasible — wezterm proves the building-block-crate split makes this cheap.
- **wasm / C-ABI:** future targets, not v1 — but learn libghostty's forward-compat discipline *now* so we don't repaint ourselves into a corner: keep the public surface **accessor-method-shaped, not field-layout-shaped**; keep host integration through **object-safe traits** (already C-ABI-friendly: opaque handle + function pointers map cleanly); gate the `simd_utf8` C++/SIMD dependency behind a feature with a pure-Rust fallback so wasm/cross builds stay green. A `canario-c` crate (opaque `CanarioTerminal` handle, get/get-multi accessor enums, sized-struct init) is the eventual bridge for non-Rust embedders and the multiplexer ecosystem.

---

## 6. Naming & identity

**canario** — Portuguese for *canary*. It continues Rio's tradition of naming crates after Rio de Janeiro / Brazilian landmarks and symbols:

- **sugarloaf** (Pão de Açúcar) — the GPU renderer
- **corcovado** — the mio fork driving the event loop (the mountain of Christ the Redeemer)
- **crosswords** — the in-tree grid engine (the wordplay name for the cell grid)
- **teletypewriter** — the PTY layer

The canary fits the family on two levels. Literally, the *canário* is a small Brazilian songbird — the Brazilian national football team is *a Canarinho*, "the little canary," for its yellow shirts; it slots naturally beside Sugarloaf and Corcovado as a piece of Rio/Brazil iconography. Figuratively, a **canary** is the *first thing into the mine* — the small, sensitive, correctness-focused core that surfaces VT problems before they reach the renderer. A terminal *engine* whose entire job is to be *correct* and to *signal* (damage, alerts, replies) is exactly a canary. The headless conformance/fuzz target reinforces it: canario is the canary that catches escape-sequence bugs in CI before they ship.

**Internal naming continuity:** the extracted grid engine **keeps the name `Crosswords` internally** (as `term::Terminal == Crosswords`) so blame/history and Rio's vocabulary survive the move; the public type is `canario::Terminal` for outside consumers. `canario-pty` keeps `teletypewriter` + `corcovado` by name. The new leaf crate is **`rio-core`** (deliberately plain — it's the boring landing zone for relocated value types, and boring is correct for the crate everything depends on).

> One-line identity for the README: **canario — the songbird at the bottom of Rio's stack: a correct, fast, embeddable VT terminal engine that turns bytes into terminal state, and nothing more.**