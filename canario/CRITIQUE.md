# canario — Design Critique (adversarial completeness pass)

Confirmed: `GraphicData` is a plain value type (the design's §5 Severance 3 is accurate). I now have everything needed to write a verified critique.

# Adversarial Review — canario Design Doc

Verified against Rio source at HEAD. Confidence is high where I cite file:line; flagged otherwise. Prioritized by severity.

---

## P0 — Correctness errors in the design's own claims (these break the central thesis)

### 1. The `SequenceNo` render contract does not exist in Rio — it is net-new, not "Rio's current model"
`grep -rni 'seqno\|sequenceno\|sequence_no'` across the whole crate returns **0**. The design (§3.4, §4) repeatedly presents a "monotonic `SequenceNo` stamped per-`Terminal` per `advance` *and* per-`Row`" as *validated by Rio's current model* and lists `pub fn seqno(&self)` on the public API. **Rio has no seqno of any kind.** It has only `LineDamage { line, damaged: bool }` (crosswords/mod.rs:215) and the `TermDamageState` flag set. Adopting wezterm's per-row seqno pull model is a **from-scratch feature**, not an extraction. **Fix:** either (a) drop seqno from v1 and ship Rio's boolean per-row dirty model honestly, or (b) scope it as new work with its own test burden — but stop describing it as "keep Rio's model."

### 2. `snapshot_visible` and `damage()` take `&mut self` — the "release the lock before GPU work" property is overstated
Design §3.4/§4 sketch `pub fn snapshot_visible(&self, …)` and lean the entire render-thread story on reads not holding the lock. Actual signatures:
- `pub fn snapshot_visible(&mut self, …)` (crosswords/mod.rs:1331)
- `pub fn damage(&mut self) -> TermDamage` (crosswords/mod.rs:562) — it *mutates* (`mark_fully_damaged`, `mem::replace` on `last_cursor`).

A `&mut` snapshot still requires the **write** lock; you cannot run it concurrently with the PTY thread's `advance`. The "lock released before any GPU work" claim is true only in the sense that you copy out and *then* unlock — but the design implies shared-read concurrency that the current code can't give. **Fix:** state the real model (briefly hold the write lock, copy out, unlock, paint), and if you want true `&self` reads, that's a refactor to carry on the list — `damage()`'s cursor-diff side effects must move out of the read path.

### 3. The real signature is `snapshot_visible(&mut self, damage, cols, dst: &mut Vec<Row<Square>>, style_table: &mut Vec<Style>, extras: &mut FxHashMap<u16, Extras>)` — i.e. it leaks `Row`, `Square`, `Style`, `Extras` across the API
The design hides this behind an opaque `RenderSnapshot`. That's a fine *goal*, but the snapshot copies a **per-grid** style table and extras map keyed by `u16` ids that are only meaningful relative to *that grid's* `StyleSet`/`ExtrasTable`. The host must resolve `StyleId`/`extras_id`→concrete style itself. **Fix:** the public `RenderSnapshot` must bundle the style table + extras map + the id-resolution contract, and document that ids are snapshot-local. This is more surface than "copy only dirty rows."

### 4. `TerminalDamage::Partial(dirty row set)` — the payload does not exist
Design §3.4 writes `Partial(/* dirty row set */)`. Rio's actual enum (event/mod.rs:59) is `Partial` **unit** ("At least one row changed; consult per-row dirty bits"). The dirty set lives separately in `TermDamageState.lines: Vec<LineDamage>`. Either change the design to match (unit variant + separate dirty-bit array) or commit to threading the row set into the enum (a real change touching every `TerminalDamage` site). **Fix:** pick one; the doc currently describes an API that isn't Rio's.

### 5. Palette is **269** colors, not 256
`config/colors/term.rs`: `pub const COUNT: usize = 269` (256 indexed + fg/bg/cursor/dim/bright/vi-cursor/search etc.). The design's `TerminalConfig::palette(&self) -> &[ColorRgb; 256]` and §3.5 "`[ColorRgb; N]`" with N=256 will silently lose the named/extended slots (default fg/bg, cursor, dim, bright-foreground, search-match, vi-cursor). **Fix:** `palette() -> &[ColorRgb; 269]` (or expose `term::COUNT` and the named-index constants from `rio-core`), and carry `DIM_FACTOR`/dim-color computation, which is a `TermColors` behavior the design's "config-free `[ColorRgb; N]` table" erases.

---

## P1 — Decoupling risks the design under-estimates

### 6. `simd_utf8` is **already pure-Rust** — the design's central justification for the `simd` feature flag is fictional
The research JSON and design §3.3/§5 repeatedly warn that simdutf is "a C++/SIMD dependency (build complexity, not no_std-friendly)" requiring a gated fallback. But `rio-backend/src/simd_utf8.rs` is a **hand-rolled pure-Rust** module: `std::str::from_utf8_unchecked` fast path, `compute_error_len`, no `simdutf` crate, no C++. There is no `convert_utf8_to_utf32_with_errors` call anywhere. **Fix:** delete the entire "gate simdutf behind a feature with a std fallback" narrative — it's already std-only. The `simd` feature as justified does not need to exist. (There *is* a `simd/arch` dir + `simd_base64.rs`; verify whether *those* carry portable-SIMD/`target_feature` before claiming a simd feature is needed at all.)

### 7. The graphics decoders import `sugarloaf` **inside test modules too**, and import more types than §5 lists
`ansi/graphics.rs` has `use sugarloaf::ColorType;` at **six** in-test sites (lines 711–950) plus the prod `use crate::sugarloaf::{GraphicData, GraphicId}` (graphics.rs:8 — note the `crate::sugarloaf` re-export path, a *seventh* relocation surface). `iterm2_image_protocol.rs:10` pulls `{GraphicData, GraphicId, ResizeCommand, ResizeParameter}`; `kitty_graphics_protocol.rs:5` adds `ColorType`. §5 Severance 3 lists the type set correctly but **undercounts the call sites** and misses the `crate::sugarloaf` re-export and the test-module imports. Mechanical, but "~30 references" is optimistic once you count tests. **Fix:** audit count is higher; budget the test-fixture migration explicitly.

### 8. A whole `graphics/` subsystem (`graphics/mod.rs` + `graphics/kitty/`) is missing from the crate tree
The design's §3.1 tree accounts for `ansi/*` decoders but there is a top-level `rio-backend/src/graphics/` (currently `#[cfg(test)] mod kitty` — a kitty graphics **test/integration harness**). The doc's crate tree has no home for it. Minor, but the tree claims to be the extraction map and isn't complete. **Fix:** map `graphics/` (and its kitty integration tests) into `canario-graphics` test layout.

### 9. The Handler trait is **149 methods**, and includes XTGETTCAP/terminfo + APC/glyph plumbing the design never mentions
The design says "~150 methods, almost all defaulted no-op." True count is 149 (handler.rs). But beyond the listed surface, Rio's `Handler`/`Processor` carries: `report_keyboard_mode`/`push_keyboard_mode`/`pop_keyboard_mode`/`set_keyboard_mode` (full **kitty keyboard protocol**, handler.rs:382–398), `report_version`, `set_scp`, `kitty_chunking_state_mut`, and the `Processor` privately implements **XTGETTCAP/terminfo querying** (`process_xtgettcap_request`, `decode_terminfo_value`, `get_termcap_capability`, hex codec) — a substantial sub-feature that replies with hardcoded terminfo capabilities. The design never lists XTGETTCAP and lists kitty-keyboard only obliquely. **Fix:** enumerate kitty-keyboard and XTGETTCAP as carried features; the terminfo capability table is a maintenance liability that must move with the engine and be feature-gated or pluggable.

---

## P2 — Terminal features the design under-specifies (all present in Rio, must carry over)

These are **in Rio today** (verified) and the design either omits or treats as primitives without committing to carry them:

| Feature | Evidence in Rio | Design gap |
|---|---|---|
| **Kitty keyboard protocol** | `Mode::{DISAMBIGUATE_ESC_CODES, REPORT_EVENT_TYPES, REPORT_ALTERNATE_KEYS, REPORT_ALL_KEYS_AS_ESC, REPORT_ASSOCIATED_TEXT}` (mod.rs:88–94), `KITTY_KEYBOARD_PROTOCOL` aggregate | Listed only as a `TerminalConfig::kitty_keyboard()` bool; the **mode stack** (`push/pop_keyboard_mode`) and 5 mode bits are real engine state, not a config toggle |
| **Hyperlinks (OSC 8)** | `square::Hyperlink`, `cell_hyperlink`, `cell_hyperlink_id`, hint matching `find_hyperlink_matches` (mod.rs:1279–1302) | Design mentions OSC 8 only in the comparison table; never states it's carried, never accounts for `Extras.hyperlink` in the relocated side-table |
| **Focus reporting** | `Mode::FOCUS_IN_OUT` (mod.rs:89), DEC 1004 (mode.rs:114) | Not in API sketch; host needs to know focus mode is active to send `CSI I`/`CSI O` |
| **Bracketed paste** | `Mode::BRACKETED_PASTE`, DEC 2004 | `send_paste` semantics (wrap + newline canonicalization) not in API; design says "host encodes" but paste-mode wrapping is engine policy |
| **Title stack** | `push_title`/`pop_title` (handler.rs), XTWINOPS | Not in `Alert` enum; `TitleChanged(String)` alone can't model push/pop |
| **Alternate scroll / UTF8 mouse / SGR-pixels** | `Mode::{ALTERNATE_SCROLL, UTF8_MOUSE, SGR_MOUSE, MOUSE_DRAG, MOUSE_MOTION}` | Design says "host encodes mouse" but the engine owns *which* protocol+encoding is active; the `modes()` accessor must expose all of these, not just "mouse mode" |
| **Sixel display modes** | `Mode::{SIXEL_DISPLAY, SIXEL_PRIV_PALETTE}` (mod.rs bits 28–29) | DECSDM and private-palette state unmentioned |
| **Origin mode / scroll regions / margins** | `set_scrolling_region`, `Mode::ORIGIN`, `scroll_region` field used in `snapshot_visible` | Listed in §3.5 generically; note `snapshot_visible` already special-cases `scroll_region` + `display_offset` — the snapshot is region-aware, a subtlety the opaque `RenderSnapshot` must preserve |
| **Insert mode forces full damage** | `damage()` calls `mark_fully_damaged()` when `Mode::INSERT` (mod.rs:565) | A correctness quirk the seqno model must replicate or it under-reports |

**Fix:** add an explicit "carried features" checklist to the design with each mode bit and its host-visible accessor; the `Alert` enum needs `TitlePush`/`TitlePop` (or a structured title op), and `modes()` must surface the full `Mode` bitset, not a curated subset.

---

## P3 — API ergonomics & threading gaps

### 10. PTY-response path is ambiguous: `write_pty` callback vs. engine-owns-write-side
The design says (§1 "engine owns the PTY write side only," echoing wezterm) **and** routes replies through `TerminalHost::write_pty` callback **and** `Alert::ClipboardLoad`/`ColorRequest` that the host answers by calling back `Terminal::set_color`/`paste_clipboard`. That's three mechanisms. Rio's actual model (handler_flow in survey) is: Handler fires `RioEvent::PtyWrite/ColorRequest/ClipboardLoad` carrying `Arc<dyn Fn(...)->String>` formatters; the frontend turns them into `Msg::Input` → `pty.writer()`. The design's `write_pty(&mut self, id, bytes)` is cleaner but **loses the lazy-formatter pattern** (Rio defers string formatting via `Arc<dyn Fn>` so the engine doesn't allocate replies it computes from frontend-owned data like text-area pixel size). **Fix:** decide whether replies are eager `&[u8]` (simpler, but engine must own all data needed to format, e.g. cell pixel dimensions — which today come from the frontend via `TextAreaSizeRequest`) or keep a callback-formatter. The `text_area_size_pixels`/`cells_size_pixels` handlers **need frontend-supplied dimensions** — that's a *query* the host must answer synchronously, which the one-way `write_pty` can't express.

### 11. `Terminal<H: TerminalHost>` keeps the viral generic the survey flagged as a pitfall — and the design acknowledges then ignores it
The alacritty survey's #1 pitfall: `<U: EventListener>` is viral through Grid owner, Processor, selection, search. The design's §3.5 fixes selection (good: operate on `&Grid<Square>`) but **keeps `Terminal<H>` generic** and routes the PTY `Machine<T, H>` generic too. Survey explicitly suggests "a non-generic core that emits into a `dyn FnMut(Event)` sink." The design doesn't engage with that tradeoff. **Fix:** decide explicitly: generic `H` (monomorphization bloat, turbofish noise, but zero-cost) vs. `Box<dyn TerminalHost>` (object-safe, matches the "set at construction" framing and wezterm's actual choice). The design *says* object-safe but *codes* generic — pick one.

### 12. `search.rs`/`vi_mode.rs` coupling to `WindowId` in tests, and `WindowId: Copy + Send + 'static` may be too weak
Survey notes `search.rs` tests construct `event::WindowId::from(0)`. The opaque `type WindowId` fixes prod, but the design's bound `Copy + Send + 'static` omits `Eq`/`Hash` — yet Rio routes events *by* window/route id and the `Machine` keys on it. If any internal map is keyed by `WindowId`, you need `Eq + Hash`. **Fix:** bound `WindowId: Copy + Eq + Hash + Send + 'static` unless you can prove no id-keyed lookup exists in the engine (the `route_id: usize` threading in PtyWrite suggests there is).

---

## P4 — Scope realism

### 13. "Keep `resize.rs` byte-for-byte" conflicts with relocating `AnsiColor` out of `blank_with_bg`
§3.2 says keep `grid/resize.rs` byte-for-byte; §5 Severance 1 + the survey note `grid/mod.rs:522 blank_with_bg(bg: config::colors::AnsiColor)` must change its color type. Resize calls blanking. You **cannot** keep grid byte-for-byte *and* repoint the color enum — the grid touches `AnsiColor`. Minor, but "byte-for-byte" is false advertising; it's "logic-for-logic with the color type repointed." **Fix:** soften the claim and call out that `Square`'s discriminant, `blank_with_bg`, `Attr`, and `StyleSet` all need the color type swapped in lockstep (a single coordinated rename, per the survey's leaky_types).

### 14. `replay`/`contents_diff` adapter is presented as low-cost but has **no basis in Rio**
§2/§3.4 sell `canario-replay` (vt100-style `contents_formatted`/`contents_diff`) as a "first-class optional adapter… golden tests for free." Rio has **none** of this — no re-emission path exists in the source. This is a full new subsystem (escape-sequence *encoder* for the entire SGR/cursor/mode surface), not an extraction. The design's framing ("incidentally gives golden tests") badly understates it. **Fix:** mark `canario-replay` as **new development, post-v1**, not part of the extraction.

### 15. Reflow correctness is the #1 historical bug source and the test port is load-bearing — say so louder
Every surveyed engine flags reflow as the hardest code; the design correctly says "keep faithful" but buries that `grid/tests.rs` is the **only** safety net and must port intact. Given P0 #13 (resize isn't actually untouched), the reflow test suite must be the **gating CI** for the extraction. **Fix:** make "all of `grid/tests.rs` green" an explicit extraction exit-criterion.

---

## Summary of forced decisions

1. **seqno**: drop it, or scope it as new work (it does not exist in Rio). [P0]
2. **lock model**: `snapshot_visible`/`damage` are `&mut self` today — either refactor to `&self` reads or stop claiming concurrent reads. [P0]
3. **`RenderSnapshot`**: must bundle snapshot-local style table + extras map + id-resolution; can't be a thin opaque blob. [P0]
4. **`TerminalDamage::Partial`**: unit variant + side dirty array, or add the payload everywhere — pick one. [P0]
5. **palette size**: 269 + named indices + `DIM_FACTOR`, not `[ColorRgb;256]`. [P0]
6. **delete the simdutf feature narrative** — `simd_utf8.rs` is already pure-Rust. [P1]
7. **graphics severance** is wider than §5 (test-module imports + `crate::sugarloaf` re-export + a `graphics/` dir with no home). [P1]
8. **carry list** must explicitly include: kitty keyboard (mode stack + 5 bits), XTGETTCAP/terminfo table, OSC 8 hyperlinks + hint matching, title stack, focus reporting, bracketed-paste wrapping, all mouse encodings, sixel display modes, insert-mode-forces-full-damage. [P2]
9. **PTY reply mechanism**: reconcile `write_pty(bytes)` vs. the lazy `Arc<dyn Fn>` formatters and the synchronous *size queries* the host must answer. [P3]
10. **generic vs. dyn host**: the doc says object-safe but the API is generic `Terminal<H>` — resolve. `WindowId` likely needs `Eq + Hash`. [P3]
11. **`canario-replay`** is net-new, not an extraction. [P4]
12. **`grid/resize.rs` is not byte-for-byte** (color-type repoint touches it); gate the extraction on `grid/tests.rs`. [P4]

**Overall:** the *decoupling* analysis (5 severances, type relocation, trait injection) is sound and matches the source. The *render-contract* half of the design (seqno, `&self` snapshots, `Partial(rows)`, opaque `RenderSnapshot`, 256-color palette) describes a terminal engine Rio **does not currently have** while claiming to merely extract Rio — that mismatch is the document's biggest correctness risk, and it inflates "extraction" into "extraction + a new wezterm-style damage subsystem" without budgeting for it.