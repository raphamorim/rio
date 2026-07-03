# canario — Implementation Roadmap

> Extract Rio's headless VT engine from `rio-backend` into a reusable workspace (`canario`), then re-point `rio-backend` at it as a thin adapter. Verified against Rio source at HEAD. Every phase is a green-CI checkpoint.

---

## Ground-truth corrections folded in (read first)

The design doc and adversarial critique disagree on several load-bearing facts. Verified against source — these override both where they conflict:

| Claim | Verified reality | Roadmap consequence |
|---|---|---|
| `SequenceNo` "Rio's current model" | **No seqno anywhere** (`grep` = 0). Rio has only `LineDamage{line, damaged: bool}` + `TermDamageState`. | seqno is **net-new, Phase 9 stretch**, not extraction. v1 ships the boolean per-row dirty model honestly. |
| `snapshot_visible(&self)` / `damage(&self)` | Both are **`&mut self`** (`mod.rs:562`, `mod.rs:1331`). `damage()` mutates (`mark_fully_damaged`, `mem::replace(last_cursor)`). | v1 model is "take write lock briefly, copy out, release, paint." `&self` reads are a Phase 9 refactor. |
| `TerminalDamage::Partial(rows)` | **Unit variant** (`event/mod.rs:66`). Dirty set lives in `TermDamageState.lines: Vec<LineDamage>`. | Public enum stays unit + separate dirty array. No payload threading in v1. |
| `palette() -> [ColorRgb; 256]` | **`COUNT = 269`** (`config/colors/term.rs:7`) + `DIM_FACTOR` + named indices. | `palette() -> &[ColorRgb; 269]`; carry dim-color computation. |
| simd_utf8 "already pure-Rust, delete the feature narrative" | **FALSE.** `simd_utf8::validate` wraps `simdutf::validate_utf8_with_errors`; `parser/mod.rs:847` calls `simdutf::convert_utf8_to_utf32_with_errors` in the print hot path. `simdutf` is a real C++/SIMD workspace dep. | **Keep** the `simd` feature + a `std::str::from_utf8` fallback. The critique is wrong here. |
| Handler "~150 methods" | `grep -c 'fn '` = **164** (incl. private `Processor` helpers); ~149 trait methods + XTGETTCAP/terminfo + kitty-keyboard plumbing. | Carry-list must enumerate kitty-keyboard mode stack, XTGETTCAP/terminfo, OSC 8, title stack, focus, all mouse encodings. |
| Graphics severance "~30 refs" | Prod imports go through **`crate::sugarloaf` re-export** (`graphics.rs:8`), **6** in-test `use sugarloaf::ColorType` sites, plus an orphan `rio-backend/src/graphics/` dir (kitty test harness). `GraphicData` lives in `sugarloaf/src/sugarloaf/graphics.rs` (+ `GraphicDataEntry`). | Budget test-fixture migration + the re-export path + a home for `graphics/`. |
| `WindowId: Copy + Send + 'static` | Routing keys on window/route id. | Bound **`Copy + Eq + Hash + Send + 'static`** unless proven no id-keyed map exists. |
| `grid/resize.rs` "byte-for-byte" | `grid/mod.rs:522 blank_with_bg(bg: config::colors::AnsiColor)` — resize calls blanking. | "logic-for-logic, color type repointed." `grid/tests.rs` is the gating CI. |
| `canario-replay` "golden tests for free" | **No re-emission path in Rio.** | New development, **post-v1**. |

---

## Phase 0 — Scaffold (`rio-core` + empty workspace member)

**Goal:** create the crate skeleton and the *decoupling landing zone* (`rio-core`) **without moving any engine logic yet**. This phase compiles and ships against unchanged Rio.

### 0.1 Workspace registration

`Cargo.toml` (root) — add members in dependency order:

```toml
members = [
    "sugarloaf", "teletypewriter", "corcovado",
    "rio-core",          # NEW: leaf, zero engine deps
    "rio-parser",         # NEW: VT500 state machine
    "canario",                # NEW: the engine
    "rio-backend", "rio-grapheme-width", "rio-window",
    "rio-notifier", "frontends/rioterm",
]

[workspace.dependencies]
rio-core  = { path = "rio-core",  version = "0.1.0" }
rio-parser = { path = "rio-parser", version = "0.1.0" }
canario        = { path = "canario",        version = "0.1.0" }
```

### 0.2 `rio-core/` (leaf — the relocation target)

```
rio-core/
├── Cargo.toml
└── src/
    ├── lib.rs        # pub mod color; pub mod graphics; pub mod geom;
    ├── color.rs      # ColorRgb, AnsiColor, NamedColor, ColorArray, NAMED INDEX consts, DIM_FACTOR
    ├── graphics.rs   # GraphicData, GraphicDataEntry, GraphicId, ColorType,
    │                 #   ResizeCommand, ResizeParameter, MAX_GRAPHIC_DIMENSIONS
    └── geom.rs       # Pos, Line, Column, Side, index newtypes (later phase)
```

`rio-core/Cargo.toml`:

```toml
[package]
name = "rio-core"
version = "0.1.0"
edition = "2021"
license = "MIT"

[dependencies]
bitflags = { workspace = true }
serde = { workspace = true, optional = true }

[features]
default = []
serde = ["dep:serde"]
```

**Decoupling move (this phase, mechanical):**
- Copy the **pure value types** out of `sugarloaf/src/sugarloaf/graphics.rs` (`GraphicData`, `GraphicDataEntry`, `GraphicId(pub u64)`, `ColorType`, `ResizeCommand`, `ResizeParameter`, `MAX_GRAPHIC_DIMENSIONS`) into `rio-core::graphics`. **Leave the GPU-side `Graphics`/texture-atlas logic in sugarloaf.** (Severance 3 — `crosswords/mod.rs:62`.)
- Copy `ColorRgb`, `AnsiColor`, `NamedColor`, `ColorArray`, the **269 named-index constants**, and `DIM_FACTOR` out of `config/colors/mod.rs` into `rio-core::color`. **Strip `to_wgpu`/`to_composition`** — those stay in `config` as inherent methods on a frontend newtype. (Severance 1 — `config/colors/mod.rs:14`.)
- In sugarloaf and config, re-export from `rio-core` (`pub use rio_core::graphics::*;` / `pub use rio_core::color::*;`) so **existing Rio code keeps compiling unchanged**. This is the keystone: the relocation is invisible until the engine moves.

**Verify:** `cargo build -p rio-core`; full `cargo build` of the workspace green (re-exports keep rioterm/sugarloaf working); `cargo test -p sugarloaf` green (graphics decode tests still pass through the re-export).

### 0.3 `rio-parser/` and `canario/` empty shells

`rio-parser` is registered now but populated in Phase 1; `canario` in Phase 2. Both start as `lib.rs` with a doc comment so the workspace resolves.

`canario/src/lib.rs` skeleton (module stubs, all `pub(crate)` empty for now):

```rust
//! canario — headless VT terminal engine. Bytes in, terminal state out.
pub mod ansi;        // control, mode, charset, glyph_protocol
pub mod handler;     // Handler trait + Processor
pub mod grid;        // Grid<Square>, Row, Storage, resize
pub mod square;      // packed Square(u64) + Extras
pub mod style;       // StyleSet / StyleId
pub mod attr;        // SGR Attr
pub mod term;        // Terminal<H> (== Crosswords) + Handler impl
pub mod host;        // TerminalHost, Alert, TerminalConfig
pub mod selection;   // #[cfg(feature="selection")]
pub mod search;      // #[cfg(feature="search")]
pub mod vi_mode;     // #[cfg(feature="vi-mode")]
pub mod codepoint_width;
#[cfg(feature = "pty")] pub use canario_pty as pty;
pub use rio_core as types;
pub use rio_parser as parser;
```

`canario/Cargo.toml` starting deps (sever sugarloaf/config/rio-window):

```toml
[dependencies]
rio-core  = { workspace = true }
rio-parser = { workspace = true }
bitflags = { workspace = true }
bytemuck = { workspace = true }
rustc-hash = { workspace = true }
smallvec = { version = "1.13.2", default-features = false }
unicode-width = { workspace = true }
parking_lot = { workspace = true }
rio-grapheme-width = { workspace = true }   # reuse: emoji::Presentation (crosswords/mod.rs:405)
# optional
regex = { workspace = true, optional = true }
onig  = { workspace = true, optional = true }
base64 = { workspace = true, optional = true }
serde = { workspace = true, optional = true }
teletypewriter = { workspace = true, optional = true }
corcovado = { workspace = true, optional = true }
simdutf = { workspace = true, optional = true }   # simd UTF-8→UTF-32 hot path

[features]
default = ["selection", "search", "vi-mode", "graphics", "simd"]
selection = []
search = ["dep:regex", "dep:onig"]
vi-mode = []
graphics = ["dep:base64"]
pty = ["dep:teletypewriter", "dep:corcovado"]
replay = []                       # post-v1, stub
simd = ["dep:simdutf", "rio-parser/simd"]
serde = ["dep:serde", "rio-core/serde"]
```

> **NOT depended on by `canario`:** sugarloaf, config, rio-window, copypasta, raw-window-handle, image_rs, wgpu, toml, dirs. Confirmed against `rio-backend/Cargo.toml`.

**Exit criteria Phase 0:** workspace builds; `rio-core` is the single home for the relocated value types; Rio still green via re-exports; **zero engine logic moved**.

---

## Phase 1 — Extract the VT parser (`rio-parser`)

**Scope:** the forked vte VT500 state machine, standalone. This is the lowest-risk move (the survey confirms `performer/parser/{mod.rs,params.rs}` is "ZERO external deps").

**Files moved:** `rio-backend/src/performer/parser/{mod.rs, params.rs}` → `rio-parser/src/parser/{mod.rs, params.rs}` + `rio-parser/src/lib.rs` (`pub use parser::{Parser, ...}`).

**Decoupling:**
- The **only** non-std dep is the simd UTF-8 path. `parser/mod.rs:724` calls `crate::simd_utf8::validate`; `parser/mod.rs:847` calls `simdutf::convert_utf8_to_utf32_with_errors`. **Move `simd_utf8.rs` into `rio-parser`** behind the `simd` feature, and add a pure-Rust fallback module (`std::str::from_utf8` + a scalar `validate`) selected when `simd` is off. (Corrects critique #6 — the C++ dep is real and in the hot path.)
- Parser exposes bulk hooks `print_str`/`print_codepoints` with per-char default fallbacks (already present).

**Public API delta:** new crate surface `rio_parser::{Parser, advance, advance_until_terminated, Perform-equivalent}`. The `Handler` trait does **not** live here — parser is semantics-free.

`rio-parser/Cargo.toml`:

```toml
[dependencies]
simdutf = { workspace = true, optional = true }
[features]
default = ["simd"]
simd = ["dep:simdutf"]
```

**Verify:**
- Port the parser's in-module tests verbatim.
- Add a `simd`-off CI job; assert byte-identical parse output between `simd` and fallback on a fixture corpus (vttest dumps + a UTF-8 edge-case file: split multibyte across `advance` boundaries, overlong, lone surrogates).
- `rio-backend` not yet repointed (parser still lives in two places temporarily — or: immediately repoint `performer/mod.rs` to `rio_parser::Parser` and delete the in-tree copy; preferred, smaller blast radius).

---

## Phase 2 — Extract `ansi/*` primitives + the `Handler` trait + `Processor`

**Scope:** the semantic command surface, decoupled from sugarloaf graphics + config colors. **No grid yet** — `Handler` is a trait; the concrete impl moves in Phase 4.

**Files moved into `canario/`:**
- `ansi/{mod.rs, control.rs, mode.rs, charset.rs, glyph_protocol.rs}` → `canario/src/ansi/`. (Survey: "ZERO sugarloaf/config/window deps" except value types.)
- `performer/handler.rs` → `canario/src/handler.rs` (the `Handler` trait + `Processor`).
- `performer/osc.rs` → `canario/src/osc.rs`.
- `codepoint_width.rs` → `canario/src/codepoint_width.rs` (pure, `unicode_width` only).

**Decoupling (cite coupling points):**
- `handler.rs:13 use sugarloaf::GraphicData` → `use rio_core::graphics::GraphicData`.
- `handler.rs:6 / osc.rs:13 config::colors::{AnsiColor,ColorRgb,NamedColor}` → `rio_core::color::*`.
- `glyph_protocol.rs` keeps `base64` (gated by `graphics`).

**Carry-list made explicit in `Handler` (per critique P2 — these are real engine state, not config toggles):**
- **kitty keyboard protocol:** the `Mode::{DISAMBIGUATE_ESC_CODES, REPORT_EVENT_TYPES, REPORT_ALTERNATE_KEYS, REPORT_ALL_KEYS_AS_ESC, REPORT_ASSOCIATED_TEXT}` bits + the **mode stack** (`push/pop/set/report_keyboard_mode`, `handler.rs:382–398`). Carried as engine state; `TerminalConfig::kitty_keyboard()` only gates the *initial* enablement.
- **XTGETTCAP / terminfo:** `Processor::process_xtgettcap_request` + `decode_terminfo_value` + `get_termcap_capability` + hex codec. Move with the engine; **feature-gate behind `xtgettcap`** (default-on) and document the hardcoded capability table as a maintenance liability + make it pluggable via a `TerminalConfig::termcap(name) -> Option<&str>` hook.
- `report_version`, `set_scp`, `kitty_chunking_state_mut`.

**Public API delta:** `canario::handler::{Handler, Processor}` public. `Processor::advance(&mut impl Handler, &[u8])` is the universal seam. Synchronized-update (DEC 2026) buffering (`advance_sync`/`stop_sync`/`sync_timeout`) stays public.

**Verify:** `Handler` trait compiles against a `VoidHandler` test double (all no-op defaults). Port `handler.rs`/`osc.rs` unit tests. The 6 in-test `use sugarloaf::ColorType` sites in `ansi/graphics.rs` are repointed in Phase 3 (graphics), not here.

---

## Phase 3 — Graphics decoders (`graphics` feature, in-crate module)

**Scope:** sixel, iterm2, kitty, kitty_virtual, plus the `graphics.rs` cell-side types — all renderer-agnostic.

**Files moved into `canario/src/ansi/`** (gated `#[cfg(feature="graphics")]`):
- `ansi/{graphics.rs, sixel.rs, iterm2_image_protocol.rs, kitty_graphics_protocol.rs, kitty_virtual.rs}`.
- The orphan `rio-backend/src/graphics/` (kitty test harness) → `canario/tests/kitty/` (integration tests).

**Decoupling (the wider-than-§5 audit — verified counts):**
- `graphics.rs:8 use crate::sugarloaf::{GraphicData, GraphicId}` → `rio_core::graphics::*`. (Note: this was the **re-export path**, an extra relocation surface.)
- `graphics.rs` **6 in-test** `use sugarloaf::ColorType` (lines 711, 757, 791, 852, 909, 950) → `rio_core::graphics::ColorType`.
- `iterm2_image_protocol.rs:10` `{GraphicData, GraphicId, ResizeCommand, ResizeParameter}`; `kitty_graphics_protocol.rs:5` adds `ColorType`; `sixel.rs:29` `{ColorType, GraphicData, GraphicId, MAX_GRAPHIC_DIMENSIONS}` → all `rio_core::graphics::*`.
- **Cell side-table leak (`square.rs:204`, `graphics.rs:31`):** `TextureRef.id: GraphicId`, `GraphicsCell = SmallVec<[GraphicCell;1]>`, `GraphicCell.texture: Arc<TextureRef>`. The `GraphicId` is now `rio_core`; the `Extras.graphic` side-table is renderer-agnostic. **Decoded pixel blobs stay out of the cell** — `Extras` holds only the small `GraphicId` reference (libghostty lesson, already true in Rio).

**Public API delta:** `Handler::insert_graphic(GraphicData)` carries the `rio_core` payload; the host uploads to a texture. `Alert::Graphics`/`UpdateGraphics` carries the payload, never a GPU handle.

**Verify:** port all graphics decode tests (now in-crate); golden-compare decoded `GraphicData.pixels` against fixtures for sixel/iterm2/kitty; `cargo build -p canario --no-default-features` (no `graphics`) compiles (the feature truly pays nothing).

---

## Phase 4 — Extract the grid + `Crosswords` screen model (the core)

**Scope:** the heart. `Grid<Square>` ring buffer + the `Crosswords` engine + its `Handler` impl, decoupled from config/event/window.

**Files moved into `canario/`:**
- `crosswords/grid/{mod.rs, row.rs, storage.rs, resize.rs, tests.rs}` → `canario/src/grid/`. (`storage.rs`/`row.rs`/`resize.rs` are pure generic — verified: `storage.rs` imports only `std` + `super::Row` + `Line`.)
- `crosswords/{pos.rs, style.rs, attr.rs, square.rs}` → `canario/src/`.
- `crosswords/mod.rs` (`Crosswords<U>`) → `canario/src/term/mod.rs`. **Keep the type name `Crosswords` internally**; expose `pub type Terminal<H> = Crosswords<H>` (or rename the host generic).
- New: `canario/src/term/{colors.rs, modes.rs, damage.rs}`.

**Decoupling moves (the five severances land here):**

| Coupling point | Move |
|---|---|
| `grid/mod.rs:522 blank_with_bg(bg: config::colors::AnsiColor)` | → `rio_core::color::AnsiColor`. **This is why resize is "logic-for-logic," not byte-for-byte** — resize → blanking → AnsiColor. |
| `style.rs:12`, `attr.rs:1` (style table/`Attr` on `config::colors`) | → `rio_core::color::{AnsiColor, NamedColor}`. Coordinated rename across `Square` discriminant + `StyleSet` + `Attr` in lockstep. |
| `mod.rs:437 colors: TermColors` → `config::Colors`+`ColorBuilder` (Severance 2) | **Reduce `TermColors` to a config-free `[ColorRgb; 269]`** in `term/colors.rs`, with `DIM_FACTOR` + named-index constants from `rio-core`. Frontend fills it via `TerminalConfig::palette()`. **Carry the dim-color computation** (critique #5). |
| `mod.rs:449 glyph_registry: Option<sugarloaf::...GlyphRegistry>` (Severance 4, heaviest) | **Delete the field.** Decode via injected `TerminalHost::decode_glyph(payload) -> Result<GlyphHandle, GlyphReject>`. `glyph_register` (`mod.rs:4114`) calls the host, not `sugarloaf::font::glyf_decode`. Store opaque `GlyphHandle` by id. |
| `mod.rs:43-44 event::{WindowId, EventListener, RioEvent}` + `event_proxy: U` (`mod.rs:435`) + `window_id` (`mod.rs:453`) (Severance 5) | Replace with `host: H: TerminalHost` + `H::WindowId` assoc type. ~40 `send_event` sites → `host.alert(...)`/`host.write_pty(...)`. |
| `mod.rs:62 sugarloaf::{GraphicData, MAX_GRAPHIC_DIMENSIONS}` | → `rio_core::graphics::*`. |

**Damage model — ship Rio's *actual* model (critique P0 #1–4):**
- `term/damage.rs`: keep `TerminalDamage { Noop, Full, Partial, CursorOnly }` (**`Partial` is a unit variant**) + `TermDamageState.lines: Vec<LineDamage{line, damaged: bool}>`.
- `damage()` and `snapshot_visible()` stay **`&mut self`** (verified `mod.rs:562`, `mod.rs:1331`). Document the real lock model: *briefly hold the write lock → copy out → release → paint.* No `&self` concurrent-read claim.
- Preserve the **insert-mode-forces-full-damage** quirk (`mod.rs:565`: `Mode::INSERT` → `mark_fully_damaged()`).
- `snapshot_visible` signature **leaks** `&mut Vec<Row<Square>>`, `&mut Vec<Style>`, `&mut FxHashMap<u16, Extras>`. Wrap in a public **`RenderSnapshot`** struct that **bundles the snapshot-local style table + extras map + id-resolution contract**, and document that `StyleId`/`extras_id` are **snapshot-grid-local** (critique P0 #3). Preserve `scroll_region`/`display_offset` awareness (it special-cases both).

**Modes (`term/modes.rs`) — one struct, full bitset accessor (critique P2):** ANSI + DEC-private modes, G0–G3 charsets, DECSC/DECRC saved cursor, tabstops, protected attrs. `modes()` exposes the **full `Mode` bitset**, not a curated subset: mouse (`SGR_MOUSE`, `UTF8_MOUSE`, `ALTERNATE_SCROLL`, `MOUSE_DRAG`, `MOUSE_MOTION`), `FOCUS_IN_OUT`, `BRACKETED_PASTE`, `ORIGIN`, `SIXEL_DISPLAY`, `SIXEL_PRIV_PALETTE`, kitty-keyboard bits. `reset()`/`soft_reset()` (DECSTR) correct. Two grids (primary + `inactive_grid`) + `swap_alt()` preserved; alt-screen scrollback disabled.

**Hyperlinks (OSC 8) carried:** `square::Hyperlink`, `cell_hyperlink`, `cell_hyperlink_id`, `find_hyperlink_matches` (`mod.rs:1279`). `Extras.hyperlink` accounted for in the relocated side-table.

**Public API delta:** `Terminal<H: TerminalHost>` with `new/advance/grid/cursor/modes/colors/title/current_dir/damage/reset_damage/snapshot_visible/resize/scroll_display/reset/set_color/paste_clipboard`. `WindowId: Copy + Eq + Hash + Send + 'static` (critique #12 — routing keys on it).

**Verify (the gate):**
- **`grid/tests.rs` is the gating CI** — all green is the Phase-4 exit criterion (reflow is the #1 historical bug source).
- Port `crosswords/mod.rs` tests + `square.rs`/`style.rs`/`attr.rs` tests.
- Alt-screen swap-in/swap-out: primary scrollback not lost/shared.
- Add a `VoidHost` (`WindowId=()`, all no-op) + headless harness: `t.advance(b"\x1b[31mhi\x1b[0m"); assert fg == Red`.

---

## Phase 5 — Selection, search, vi-mode (feature-gated, host-free)

**Scope:** geometry/search primitives, decoupled from the `<H>` generic.

**Files moved:** `selection.rs`, `crosswords/search.rs`, `crosswords/vi_mode.rs` → `canario/src/`.

**Decoupling:**
- `selection.rs:18-19` is generic over `Crosswords<U>` **purely to reach the grid** → make selection operate on `&Grid<Square>` so it carries **no host type** (survey + design §3.5).
- `search.rs:4,753,770` tests construct `event::VoidListener` + `event::WindowId::from(0)` → repoint to `canario` `VoidHost`/`()`.

**Public API delta:** `#[cfg(feature="selection")] Terminal::selection()`; `#[cfg(feature="search")] Terminal::search(regex, dir)`. `StableRowIndex` anchors survive eviction.

**Verify:** port selection/search/vi-mode tests; build matrix `--no-default-features --features <each>` individually.

---

## Phase 6 — Host integration traits (`host.rs`) + reconcile the reply path

**Scope:** finalize `TerminalHost`, `Alert`, `TerminalConfig`. Resolve the three reply mechanisms (critique P3 #10).

**Files created:** `canario/src/host.rs`.

**Reply-path decision (must resolve — Rio uses lazy `Arc<dyn Fn>` formatters today):**
- **Eager `write_pty(&mut self, id, bytes: &[u8])`** for replies the engine can format from owned data (DA/DSR, color queries, bracketed-paste, mouse/key reports).
- **Synchronous *query* methods** for replies needing frontend-only data — `text_area_size_pixels`/`cells_size_pixels` need cell pixel dimensions the frontend owns (today via `TextAreaSizeRequest`). One-way `write_pty` **cannot** express this. Add to `TerminalConfig`: `fn cell_size_pixels(&self) -> (u16,u16)` / `fn text_area_size_pixels(&self) -> (u16,u16)`, queried synchronously. This eliminates the `Arc<dyn Fn>` formatter pattern for the size case.
- **Callback-back-into-engine** for async OS round-trips: `Alert::ClipboardLoad`/`ColorRequest` → host answers via `Terminal::paste_clipboard`/`set_color`.

**`Alert` enum (expanded per critique P2):**
```rust
pub enum Alert {
    Bell,
    TitleChanged(String), TitlePush(String), TitlePop,   // XTWINOPS title stack
    CurrentDirChanged(String),
    ClipboardStore { kind, data }, ClipboardLoad { kind },
    ColorRequest(usize),
    Progress(ProgressReport),
    CursorBlinkingChanged,
    Graphics(UpdateQueues),       // renderer-agnostic payload
    GlyphInstalled(GlyphHandle),  // opaque, not GlyphRegistry
    Damaged,
    Notification { title, body },
}
```

**`TerminalConfig`:** `generation()` counter (lazy cache flush on bump), `scrollback_lines`, `palette() -> &[ColorRgb; 269]`, `unicode_version`, `kitty_keyboard`, `cell_size_pixels`, `termcap(name)`. All defaulted.

**generic vs dyn (critique #11 — resolve explicitly):** ship **generic `Terminal<H: TerminalHost>`** (zero-cost, matches `Machine<T,H>`), but keep `TerminalHost` **object-safe** so a future `Terminal<Box<dyn TerminalHost>>` is a non-breaking option. Document the tradeoff; don't claim object-safe while only offering generic.

**Verify:** `rioterm`-shaped mock host implements the trait end-to-end in a test; assert DA/DSR/color-query replies land in `write_pty`; assert size query answered synchronously.

---

## Phase 7 — PTY driver (`pty` feature)

**Scope:** the `Machine` event loop, generic over the host trait.

**Files moved:** `performer/mod.rs` (`Machine`) + `event/sync.rs` (`FairMutex`) → `canario/src/pty/` (or sub-crate `canario-pty`), `#[cfg(feature="pty")]`.

**Decoupling:**
- `performer/mod.rs:8 event::{EventListener, Msg, WindowId}` → `H: TerminalHost` + `H::WindowId`.
- `Machine<T: teletypewriter::EventedPty, U>` → `Machine<T, H>`. `teletypewriter` + `corcovado` move into canario's optional dep set **unchanged** (verified: no sugarloaf/config/window deps).
- `RioEvent::TerminalDamaged/Render` (`mod.rs:227,365,402`) → `host.alert(Alert::Damaged)`, guarded by the existing `damage_event_in_flight` flag.
- `ChildEvent::Exited` → `term.exit()` + `Alert::Damaged`.

**Public API delta:** `#[cfg(feature="pty")] Machine::spawn()` — dedicated thread, `corcovado::Poll` loop → `pty_read` → lock `FairMutex<Terminal<H>>` → `Processor::advance`.

**Verify:** integration test with a fake `EventedPty` feeding a byte log; assert grid state + that `Alert::Damaged` fires once per batch.

---

## Phase 8 — Re-point `rio-backend` as a thin adapter

**Scope:** `rio-backend` deletes the moved modules and depends on `canario`. **rioterm must not change behavior.**

**Migration moves:**
- `rio-backend/Cargo.toml`: add `canario = { workspace = true, features = [...] }`; the `sugarloaf` re-exports of graphics/color types now forward to `rio-core`.
- **Adapter layer in `rio-backend`:**
  - `impl canario::TerminalHost for RioHost`: `WindowId = rio_window::window::WindowId`; `alert(Alert)` → maps to existing `RioEvent` variants; `write_pty` → `Msg::Input` → existing channel; `decode_glyph` → `sugarloaf::font::glyf_decode` + `GlyphRegistry`.
  - `impl canario::TerminalConfig for RioConfigAdapter`: `palette()` from `config::Colors`/`ColorBuilder`; converts `config::Colors` → `[ColorRgb; 269]`.
  - `ColorWGPU = sugarloaf::Color` + `to_wgpu`/`to_composition` **stay in config** as conversions from `rio_core::ColorRgb` at the render boundary.
- `rio-backend/src/lib.rs:16 pub use sugarloaf` stays (frontend concern).
- Delete: `performer/*`, `crosswords/*`, `ansi/*`, `selection.rs`, `codepoint_width.rs`, `event/sync.rs`, `simd_utf8.rs`, `graphics/` from rio-backend (now in canario). Keep `event/mod.rs` (`RioEvent`, window glue), `config/*`, `clipboard.rs` OS impl.
- `event/mod.rs:97 GlyphProtocolInstalled{registry: GlyphRegistry}` → carries the opaque handle on the canario side; rio-backend's `RioEvent` keeps `GlyphRegistry` in its own adapter event.

**Public API delta:** `rio-backend` re-exports `pub use canario::{Terminal as Crosswords, ...}` so `rioterm` import paths barely move; provide `pub use` shims for `Crosswords`, `Grid`, `Square`, `TerminalDamage` at old paths.

**Verify (the big gate):**
- Full `cargo build` + `cargo test` of the **entire workspace** green.
- `cargo build -p rioterm --features=wgpu` per target (respect the `wgpu` feature-gates `update_filters` release rule).
- Manual smoke: launch rioterm, run vttest, sixel/kitty image demo, OSC 8 hyperlink, kitty-keyboard app (e.g. neovim), selection/copy, scrollback, alt-screen swap, sync-update TUI (no tearing).
- Run the linux adaptive-theme + macOS titlebar paths unchanged (no engine touch).

---

## Phase 9 — Post-v1 / stretch (each independent, none gate the extraction)

1. **seqno render contract (net-new).** Add monotonic `SequenceNo` per-`Terminal`-per-`advance` + per-`Row`; `Terminal::seqno()`; wezterm-style pull. Requires moving `damage()`'s cursor-diff side effects out of the read path so reads become **`&self`** → true cross-thread shared reads (render thread without the write lock). Own test burden. *This is the half of the design doc Rio doesn't have yet — scoped honestly here.*
2. **`canario-replay` (net-new).** `contents_formatted`/`contents_diff` escape-sequence **encoder** for the SGR/cursor/mode surface (vt100-style). Drives the tmux/zellij-style mux use case; gives golden snapshot tests. Full new subsystem, not extraction.
3. **Column-granular damage.** Restore alacritty's `LineDamageBounds{left,right}` that Rio reduced to a boolean, for embedders wanting per-cell GPU diffs.
4. **`canario-c` (libcanario).** Opaque `CanarioTerminal` handle + get/get-multi accessor enums + sized-struct init, modeled on libghostty-vt's C-ABI. Object-safe `TerminalHost` maps to function pointers cleanly.
5. **wasm target.** `@canario/headless`-style; ensure `simd` C++ dep is fully feature-gated off and the std-fallback parser path stays green on `wasm32`.
6. **no_std core.** Keep `rio-core` + `rio-parser` `alloc`-only (no `std` in hot paths) so a parser-only no_std build is feasible. Engine core stays `std` (Vec/VecDeque/parking_lot).
7. **Paged memory backend.** Introduce `trait GridBackend` *after* the public API is stable; swap the ring buffer for libghostty-style pages without touching `Handler` or the public surface. Big invariant-heavy rewrite; deferred.
8. **Publish to crates.io:** `rio-core`, `rio-parser`, `canario`, `canario-pty` at `0.1.0` once Phase 8 ships and the API has baked one Rio release.

---

## Risks & sequencing

**Hard dependency order:** 0 (`rio-core`) → 1 (parser) → 2 (Handler/ansi) → 3 (graphics) → 4 (grid/Crosswords) → 5 (selection/search) → 6 (host traits) → 7 (pty) → 8 (rio-backend adapter). Phase 9 items are independent and unordered.

**What can break rioterm (and the mitigation):**
- **Phase 4 (highest risk).** Reflow + alt-screen + damage. Mitigation: `grid/tests.rs` green is a **hard gate**; the color-type repoint (`AnsiColor` through `blank_with_bg`/`Attr`/`StyleSet`/`Square` discriminant) is one coordinated rename — do it atomically, not piecemeal.
- **Phase 6/8 reply path.** The `Arc<dyn Fn>` → `write_pty` + synchronous-query swap can drop a reply (e.g. `text_area_size`) if the size-query hook isn't wired. Mitigation: explicit test asserting every Rio `RioEvent::*Request` reply variant has a canario equivalent that round-trips.
- **simd fallback divergence.** Mitigation: CI job comparing `simd` vs fallback byte-identical on a UTF-8 edge corpus (Phase 1 gate). **Keep the feature** — the C++ `simdutf` dep is real and in the print hot path (corrects the critique).
- **Glyph protocol (Severance 4).** Removing `glyph_registry` from the struct and routing through `decode_glyph` changes the kitty-graphics-with-glyph path. Mitigation: notcurses-demo + glyph-protocol fixtures in Phase 8 smoke (per the kitty-graphics-perf memory note).

**Test gates per phase (CI must be green to merge):**
- P0: workspace builds, sugarloaf graphics tests pass through re-export.
- P1: parser tests + simd/fallback parity corpus.
- P2: `Handler` against `VoidHandler`; osc/handler unit tests; XTGETTCAP table tests.
- P3: graphics decode goldens; `--no-default-features` (no graphics) builds.
- P4: **all `grid/tests.rs`**; Crosswords tests; alt-screen swap; insert-mode-full-damage quirk.
- P5: selection/search/vi-mode tests; per-feature build matrix.
- P6: mock-host reply round-trip (DA/DSR/color/size).
- P7: fake-PTY byte-log integration.
- P8: **full workspace `cargo test`** + per-target `rioterm --features=wgpu` build + manual vttest/sixel/kitty/OSC8/kitty-keyboard/sync-update smoke.

**Extraction exit criterion (v1 done):** `rio-backend` Cargo.toml lists **none of** sugarloaf/config/rio-window/copypasta in `canario`'s dependency closure; rioterm builds and behaves identically; `grid/tests.rs` + full conformance suite green; canario builds standalone with `--no-default-features` and with each feature in isolation.
