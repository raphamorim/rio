# rio-vt vs ghostty: closing the benchmark gaps

Deep analysis of every workload where rio-vt loses in ../rio-vt-benchmark, with
profile evidence and ranked plans. Sources analyzed: local rio-vt 0.5.2 and
../ghostty (b14d92383, close to the libghostty-rs pin). Profiles taken with
macOS `sample`/samply on Apple Silicon against the exact benchmark corpora.

## The gaps (criterion medians, 2026-07-31 run)

| workload | rio-vt | ghostty | others | gap |
|---|---|---|---|---|
| unicode_wide | 245 MiB/s | 597 | alacritty 338 | 2.4x |
| ascii_plain | 829 MiB/s | 1538 | | 1.9x |
| scroll_storm | 267 MiB/s | 477 | | 1.8x |
| mixed (process) | 287 MiB/s | 342 | | 1.2x |
| sgr_churn | 230 MiB/s | 233 | alacritty 331, vt100 325 | 1.4x vs alacritty |
| resize_reflow | 1006 µs | 415 µs | vt100 108 (clips, no reflow) | 2.4x |

rio-vt wins: alt_screen_redraw (565 vs 529), serialize (4.4 µs vs 12.5), plain
resize (5.1 µs vs 70 — ghostty always rewrites pages cell-by-cell; rio's
no-reflow fast paths skip that entirely).

## Root causes (cross-cutting)

Five patterns explain all six gaps. Ghostty's core wins not on the inner loops
(its per-cell fill is the same packed-u64 OR-store rio already does) but on
driving every per-line and per-run *fixed* cost to near zero.

1. **O(rows) bookkeeping on every scroll.** Each linefeed marks 24 rows dirty
   through per-line ring-index math, runs a second 24-iteration damage loop,
   resets 80 cells ignoring `occ`, and moves an `Option<Selection>` even when
   `None`. Ghostty: one memset, one 8-byte-row-header rotate, `page.dirty =
   true`. This taxes ascii_plain (~30% of its time) and unicode_wide too, not
   just scroll_storm (~55%).
2. **Early-exit scalar scans that never vectorize.** Two hot loops carry
   comments claiming LLVM auto-vectorizes them; it does not (early-exit
   iterator loops can't). Ghostty hand-vectorizes both of its equivalents and
   says so in comments. ~46% of ascii_plain time.
3. **Heap allocation on hot paths.** A `Vec<Option<Attr>>` per SGR sequence
   (~10 ns, the bigger half of the sgr gap); `split_off`/`append`/`resize_with`
   realloc churn per row during reflow (~20-30k heap calls per resize
   iteration vs ghostty's ~50 pooled page ops).
4. **Batching exists for ASCII but not for wide chars or non-ASCII decode.**
   `input_str` has a good bulk path; `input_codepoints` writes wide chars one
   codepoint at a time (~4 cursor resolutions, 2 template builds, 2 atlas
   checks per char), and the parser splits multi-byte runs at every ASCII
   byte, so simdutf gets 9-24-byte FFI calls (4 per line) instead of 4KB
   chunks. ~80% of the unicode gap.
5. **Per-event tracing checks and eager style interning.** `debug!`/`trace!`
   atomics run per CR and per LF; every SGR attribute does a full
   FxHashMap hash+probe re-intern even when the style is unchanged. Ghostty
   has the same interning weakness (hence its identical 233 on sgr_churn) and
   a TODO acknowledging the fix rio should also make.

## Validation status

Two of the plans were already validated end-to-end on scratch copies of
rio-vt (repo untouched, all 401 unit tests pass on the patched copies):

- ascii/scroll patch set: ascii_plain **884 → 1720 MiB/s** (past ghostty's
  1538), pure-scroll 62 → 110 MiB/s, mixed 385 → 433 (loop-mode harness).
- scroll patch set (independent agent, same three scroll fixes plus
  occ-bounded reset): scroll_storm **273 → ~530 MiB/s** (past ghostty's 477),
  LF cost 27.6 → 8.0 ns.

The sgr plan is validated by microbenchmark (per-fix ns costs measured); the
unicode and reflow plans are measured decompositions with projected gains.

---

## Phase 1 — validated quick wins (ascii_plain, scroll_storm, mixed, + partial sgr)

**Status: IMPLEMENTED (2026-07-31)** in rio-vt. Criterion medians after
(vs the same-day 4-engine run): ascii_plain 829 → 2090 MiB/s, scroll_storm
267 → 743, mixed 287 → 417, alt_screen_redraw 565 → 841, sgr_churn 230 → 315
— every parse workload except unicode_wide now beats ghostty; serialize and
resize unchanged. 402/402 rio-vt tests, rio-backend/librio suites, and the
render-parity binary all pass. Item 6 (`is_ascii` pass) was skipped: it needs
a new entry on the now-public Handler trait for ~3%.

Implementation notes vs the plan below: the `Row::grow` occ mitigation
regressed resize_reflow 2x (the added branch stops `resize_with` from
lowering to a memset-like extend in the reflow loop) and was replaced by a
two-ended tail probe in `Row::reset` — grow's mixed tail is always
old-fill-then-defaults, so probing `inner[occ]` and `inner[len-1]` is exact.

Everything here is small, local, and already proven on a scratch copy.

1. **Branchless wide-cleanup scan** — `crosswords/mod.rs:3203`. Replace
   `cells.iter().any(|c| c.needs_wide_cleanup())` with a `fold(false, |a, c|
   a | c.needs_wide_cleanup())` (or 8-cell OR blocks against the wide mask).
   Measured: ascii +30% alone. One line, no semantic change.
2. **Vectorize the printable-run end scan** — `performer/parser/mod.rs:800-804`.
   SWAR u64 scan (stop byte = `<0x20 | ==0x7F | >=0x80` via wrapping-sub
   masks, 8 B/iter) or NEON/SSE2. Fix the false "LLVM auto-vectorizes"
   comment. Measured: print path +27%, compounds with #1.
3. **Kill per-scroll O(rows) bookkeeping** (both agents converged here;
   measured ascii +27%, crlf-only +45%, LF 27.6 → 8.0 ns):
   - Full-screen scroll region → `mark_fully_damaged()` once instead of the
     24x `damage_line` loop (`crosswords/mod.rs:1226-1229`);
     `scroll_down_relative` already does this (mod.rs:1187), and
     `peek_damage_event` prefers full damage, so semantics are unchanged.
   - Skip the per-row `dirty = true` loop (`grid/mod.rs:426-431`) when full
     damage is marked — `snapshot_visible`'s `needs_full` branch copies every
     row regardless. For partial regions (DECSTBM) add
     `Storage::mark_lines_dirty(start, end)`: resolve the ring offset once,
     mark at most two contiguous slices, instead of 24x `compute_index`.
     Contract to document/debug_assert: full-screen scroll implies full
     damage (verify librio/canario snapshot consumers go through
     `snapshot_visible`).
   - `Row::reset` (`grid/row.rs:189-208`): occ-bounded reset — probe the last
     cell; if resetting it is a no-op, only reset `inner[..occ]` (occ==0 for
     recycled blank rows → free). Also build the proto cell once and use
     `fill` so the full-reset path lowers to a vector fill. Watch the occ
     invariant soft spots: `Row::grow` (row.rs:153-159) appends defaults
     without touching occ, and `resize.rs:190` pushes to `inner` directly;
     either fix `grow` or gate the fast path on the default style id.
4. **Selection guard + tracing gates** (measured ascii +18%, crlf 90 → 110):
   - `mod.rs:1208-1211`: only `take()` the selection when `is_some()` — every
     scroll currently moves a large `Option<Selection>` out and back.
   - Feature-gate the per-CR/per-LF `debug!`/`trace!` callsites
     (mod.rs:1198, mod.rs:3665, handler.rs:931) — e.g. tracing
     `release_max_level_info` for benches/release. Worth ~5% on mixed even
     after the other patches.
5. **Kill the per-SGR-sequence Vec** — `performer/handler.rs:1516-1602`
   (caller at 1335). Dispatch attrs directly to the handler like vte 0.15
   does, or use `ArrayVec<Option<Attr>, 32>` (params cap at 32). Microbench:
   9.7 ns/seq, i.e. ~+20-25% on sgr_churn alone. Near-zero risk.
6. **Drop the redundant `is_ascii` pass** — `mod.rs:3146`. The parser only
   calls `print_str` with proven printable-ASCII bytes; add an internal
   `input_ascii_run` entry with a debug_assert. ~3%.

Projected phase-1 results: ascii_plain ~1.7 GiB/s (beats ghostty), scroll_storm
~530 MiB/s (beats ghostty), mixed +10-15% (parity or better), sgr_churn ~275.

## Phase 2 — unicode_wide batching + sgr parity

**Status: IMPLEMENTED (2026-07-31).** unicode_wide 243 → 723 MiB/s (3.0x),
past ghostty's 685; every parse workload now beats the field. Landed as:
run-classified `input_codepoints` with bulk narrow and wide-pair writers
(`write_narrow_run` / `write_wide_run`, scalar `write_codepoint_cell`
fallback for placeholders/cleanup/edges), width table extended over plane 1
with a hoistable reference (`width_table()` + `width_in`), and chunked
whole-run decode: `ground_dispatch` feeds simdutf up to 4KB per call
(never splitting a sequence) and controls are split out in codepoint space
by `dispatch_codepoints`. Two deliberate semantic notes: a non-ASCII
charset now falls back to the scalar path in `input_codepoints` (ASCII can
ride in decode runs and must charset-map), and a standalone C1 byte inside
a decode run now executes (previously U+FFFD mid-run, execute at run
lead — unified to execute everywhere). unicode_wide render-parity vs vt100
passes. The two resize parity-harness failures predate all of this (alpha.4
fails them too: rio reflows, vt100 clips).

unicode_wide (measured decomposition: ~185 ns/line in per-codepoint wide
writes, ~58 ns in fragmented simdutf FFI, ~30 ns scroll — phase 1 already
takes the scroll share):

1. **Batch wide-char writes in `input_codepoints`** (`mod.rs:3057-3138`).
   Mirror the ASCII bulk path: hoist template/charset/atlas/columns; classify
   maximal same-width runs; precompute wide and spacer u64s once; store pairs
   into the row slice after one branchless cleanup scan; cursor advance +
   `damage_line` once per run. `Square` is `repr(transparent)` u64 — this is
   exactly ghostty's `printSliceFill` shape (Terminal.zig:727-731). Fallbacks:
   end-of-row wide wrap (LeadingSpacer), first cell needing wide cleanup,
   INSERT mode (already falls back). Projected: 220 → 380-450 MiB/s alone.
2. **Decode whole ground runs in one simdutf call** — `parser/mod.rs:793-832`.
   Extend the decode run to the next C0 control instead of the next ASCII
   byte (printable ASCII decodes fine as UTF-8); route cp<0x20 to `execute`
   in codepoint space (`dispatch_codepoints`, parser/mod.rs:894). Keeps the
   pure-ASCII `print_str` path untouched. Kills the 4-tiny-FFI-calls-per-line
   shape (58 → ~20-38 ns/line) and the per-space `input_str` preamble.
   Careful: bare C1 run-lead execute semantics (parser/mod.rs:812-819).
3. **Full-plane static width LUT** — replace `codepoint_width.rs`'s
   OnceLock+BMP table with a build-time 3-stage table covering all planes
   (ghostty's `unicode/lut.zig` model). Removes the atomic load per lookup
   and the scalar `UnicodeWidthChar` fallback for emoji. Enabler for #1's
   run classifier.

sgr_churn to parity:

4. **256-slot direct-mapped style memo** in front of `StyleSet::intern`
   (`style.rs:146-161`): fold the style to an index, 16-byte compare, fall
   through to FxHashMap on miss. Safe (style ids are never invalidated; no
   compaction exists). Measured 0.7 vs 5.0 ns. +10-13%.
5. **Unpacked cursor style + skip no-op re-interns** (`grid/mod.rs:656-660`):
   keep a shadow `Style` beside the template id; early-return when an SGR
   doesn't change it. Sync at set_template_style, cursor save/restore, grid
   swap. Small here, bigger on real repeated-SGR traffic.

Projected: unicode_wide ~450-550 MiB/s (near ghostty's 597), sgr_churn ~330
(parity with alacritty/vt100).

## Phase 3 — reflow streaming rewrite (resize_reflow)

Profile: resize time is almost entirely `RawVec::reserve`→realloc churn +
`Row::shrink`'s per-row alloc+memmove + two O(n) ring rotates and two
`reverse()` passes. Cells move 2-3x; ~20-30k heap calls per iteration vs
ghostty's ~50 pooled page ops (its ReflowCursor writes each cell once into
pool-backed pages, trims trailing blanks, materializes blank rows lazily).

1. **Streaming `shrink_columns`** (`grid/resize.rs:293-507`): walk wrapped
   runs oldest-first, `extend_from_slice` occupied slices into output rows
   pre-allocated at exact width; recycle consumed source-row Vecs as output
   buffers (shrink: old cap ≥ new width → steady-state alloc-free, one move
   per cell). Kills `split_off` allocs (row.rs:170), `append_front` full-row
   memcpy (row.rs:278), `resize_with` tails. Hairiest part of the file: wide
   spacers (resize.rs:404-433), cursor reflow (474-495), display_offset,
   reflow_remap trackers must be preserved. Est. 40-60% off the shrink half.
2. **Streaming `grow_columns`** (resize.rs:95-282): same shape; push freed
   source Vecs into the storage recycle pool so the next shrink is free.
   Eliminates the per-merge realloc, `drain` memmove, and the third-copy
   `Row::grow` pass. Est. 30-50% off the grow half.
3. **Open the recycle pool to column reflow** (`storage.rs:62`, cap
   `MAX_CACHE_SIZE=1000` at storage.rs:14): byte-budgeted cap; a resize
   out-and-back then cycles the same buffers with zero malloc. Trade-off:
   retained memory — add idle shrink or byte cap.
4. **Copy only occupied cells** (`row.occ`) and replace `is_clear()` full
   scans with occ==0 checks; blank tails become memset.
5. **Reflow only wrapped runs**: rows with `occ <= columns` that don't
   continue a wrap move as 40-byte Row structs (zero cell movement). Neutral
   on this all-wrapped benchmark; collapses realistic resizes toward the
   existing 5 µs fast path.
6. **Kill the O(n) shuffles**: iterate the ring via the zero offset and emit
   in storage order (no `rotate_left` rezero at storage.rs:360, no
   `reverse()`); keep a persistent scratch Vec on Grid. Also grow cols before
   rows like ghostty so new rows aren't created at the old width.

Projected: at or below ghostty's 415 µs (rio has no per-cell style re-intern
to pay during reflow, unlike ghostty's style-set re-adds), while keeping the
no-scrollback fast path that already beats ghostty 14x.

## Phase 4 — structural (optional, larger blast radius)

- **Damage as a bitset**: `LineDamage {line, damaged}` is 16 bytes with a
  redundant field (mod.rs:213-219); a u64 bitmask makes partial-region scroll
  damage one OR. Longer term, a scroll-delta-aware damage model lets the
  renderer blit instead of full-copy.
- **O(1) scroll**: move `dirty` out of `Row` (per-grid bitmask), pooled
  pre-cleared rows so the recycled-row reset disappears (post-phase-1 residue
  is ~8 ns/scroll vs ghostty's counter bump; crlf-only 110 MiB/s shows the
  remaining headroom).
- **Bulk CSI param consumption** à la ghostty's `consumeCsiParams`
  (stream.zig:661-752): consume digit runs with parser state in locals
  instead of per-byte `change_state`→`advance_csi_param` (~6-8% of mixed).
- **Deferred style interning** (intern in `cell_template()` behind a dirty
  flag): collapses multi-attr sequences (4 interns → 1); every reader of
  `cursor.template.style_id()` needs a flush — do after phase 2's memo.
  Ghostty has a TODO for exactly this.

## Not worth touching (measured)

The cell fill itself (~3%), the memchr ESC scan (~1%), `Square::from_template`
(already one OR-store), `Storage::swap` (5-qword copy), ring rotation
(already an offset bump, cheaper than ghostty's header memmove).

## Follow-up optimizations (2026-07-31, same day)

After the phase-1 review, three more changes landed in rio-vt:

1. `Handler::input_ascii_str` — a provided trait method carrying the
   parser's printable-ASCII guarantee across the Handler boundary, so
   `Crosswords` skips the redundant `is_ascii` re-scan (`print_str` was
   the second-hottest line in the print path). ascii +13%.
2. A direct-mapped 1024-slot memo of candidate ids in front of
   `StyleSet::intern` (candidates verified against `styles[id]`, so stale
   slots miss, never lie). Break-even on the adversarial 256-color-cycling
   corpus, a win for real repeated-style traffic.
3. `advance_csi_param_run` — a CsiParam run consumer in the parser that
   accumulates digit runs into a local instead of paying state dispatch and
   `self.param` load/store per byte. sgr_churn +21%, putting it past
   alacritty (349 vs 331 MiB/s).

Also found: **rio-vt and libghostty-vt cannot be linked into one binary** —
both bundle simdutf (different versions, different compilers), and the
linker mixes the two C++ builds' objects, crashing with SIGBUS inside
`convert_utf8_to_utf32_with_errors` (this had silently killed the
benchmark's unicode_wide group all along). The benchmark repo now isolates
ghostty benches in separate binaries behind a `ghostty` feature. If librio
ever links against something bundling simdutf, the same applies; a real fix
needs symbol hiding or prefixing in the simdutf crate's build.

## Reproducing

- Benchmarks: `cargo bench` in ../rio-vt-benchmark (now includes ghostty via
  libghostty-vt; needs zig 0.16).
- Profiles: harness at the session scratchpad `rio-prof/` (modes: ascii,
  unicode, scroll, sgr, reflow), profiled with `sample <pid> 10` or samply.
- Validated patched scratch copies of rio-vt (with the phase-1 changes) were
  left in the session scratchpad (`rio-copy/`, `riocopy/`); they pass the
  full rio-vt test suite.

---

## Phase 3 — concretized plan (2026-08-01, from ghostty + rio source dossiers)

Baseline today: resize_reflow **763 µs** (tree includes the WIP shrink tail-buffer
pool in resize.rs/row.rs), ghostty 415 µs, alacritty 2.92 ms. Target ≤415 µs.

### Why rio can beat 415 µs, not just match it

Ghostty's reflow (~1,785 lines) is one streaming pass writing each cell once —
but ~60% of that implementation is capacity management for per-page interned
side tables: every styled cell pays a hash+probe re-intern into the destination
page's RefCountedSet (addWithId same-id fast path only holds while pages map
1:1), every grapheme/hyperlink does trial alloc/free probes, and style-set
overflow triggers increaseCapacity = full re-clone of the destination page
(PageList.zig:1770-1810, 1909-1932; their own TODO at page.zig:994-997
questions addWithId). Rio's global style table means style_id survives reflow
untouched — none of that cost exists. Ghostty also sweeps O(tracked_pins) per
written cell (PageList.zig:1374-1386); rio's remap is positional and selection
is dropped on column resize. The only thing ghostty does better is the shape:
stream once, write once, recycle in lockstep.

### Existing assets

- **Dangling streaming core**: commit 6c5aec06d3 (chain: 94d946a1, 8ced3237,
  1cd2c875, 6c5aec06) has `grid/reflow.rs` — 256 lines, reflow-cursor model,
  oldest-first streaming, trailing-blank trim, free-list recycling, 6 parity
  tests. Cell-level only: cursor/remap/display_offset not wired. Reachable via
  stash@{0}'s parent; recover with `git checkout 6c5aec06d3 -- rio-vt/src/crosswords/grid/reflow.rs`.
- **WIP in tree**: shrink tail-buffer pool (Row::shrink_into +
  append_front_from + pool in shrink_columns). Superseded by the rewrite but
  commit it first as a fallback increment.

### Steps

1. ~~WIP pool~~ (dropped — superseded by the rewrite; the uncommitted
   resize.rs/row.rs changes stay uncommitted and get replaced).
2. **Resurrect reflow.rs** and adopt the three ghostty semantics it lacks:
   direction-agnostic single pass (grow and shrink through one code path),
   deferred blank-row counter (never materialize trailing blank rows), and
   wide-char-at-last-column = emit LeadingSpacer, wrap, retry same cell.
   Trailing-blank trim stays gated on !wrapline with overrides for
   semantic-prompt rows and blanks left of the cursor (ghostty
   PageList.zig:1288-1340 is the reference).
3. **Wire the bookkeeping** the core deliberately skipped, porting the exact
   contracts from resize.rs: cursor mapping incl. should_wrap unwind/rewind at
   entry/exit for BOTH cursors (ghostty forgets the live cursor's
   pending_wrap — Screen.zig:1957 fixes only the saved one; don't copy that),
   display_offset (+1 per row inserted above, −1 per row removed above, clamp
   to history at end), reflow_remap trackers with the consumed = columns −
   displaced accounting (tests.rs:374 and :407 are the guards), occ
   maintenance (fix Row::grow's occ soft spot while there), and cap
   truncation advancing total_lines_scrolled.
4. **Fix the metadata loss as part of the rewrite**: current Row::from_vec
   hardcodes semantic_prompt=None / kitty_virtual_placeholder=false /
   has_extras=true, so shrink-created rows lose OSC 133 marks and placeholder
   flags today. The streaming core should copy row metadata from the source
   row it's consuming (ghostty's copyRowMetadata carries semantic_prompt; it
   re-derives the kitty flag per cell). Add mod.rs tests for both across a
   reflow.
5. **Recycle in lockstep**: consumed source-row Vecs feed new destination rows
   (shrink is naturally self-sufficient: old cap ≥ new width); grow's freed
   buffers and cap-truncated rows push into Storage::free (byte-budgeted cap
   replacing the count cap); Row::new filler and refill_to pull from it.
   Steady state: allocation count ≈ wrap-count delta, peak ≈ max(old,new).
6. **Kill the O(rows) shuffles**: emit in ring order via the zero offset (no
   reverse(), no rotate_left rezero in take_all), persistent scratch Vec on
   Grid. Invert resize order ghostty-style: grow = cols then rows, shrink =
   rows then cols (dispatcher rationale at PageList.zig:1001-1024) so filler
   rows are born at the new width.
7. **Keep both fast paths** (5.1 µs plain resize beats ghostty 14x) and add
   the doc-item-5 degeneration: unwrapped rows with occ ≤ columns move as Row
   structs, zero cell movement — realistic resizes collapse toward the fast
   path even when a few wrapped runs exist.

### Test plan

Existing: 8 unit tests in grid/tests.rs (incl. the two remap guards), storage
pool tests, parity harness (the two resize scenarios fail by design vs vt100 —
not a signal). Add: metadata propagation (step 4), occ invariant checks,
grow len==1 spacer-only merge, both-cursor pending-wrap across resize, blank-
row laziness (trailing blank rows never materialize), and a proptest:
random grid → reflow W1→W2→W1 → equal modulo trailing blanks, occ bounds
hold, wrapline/spacer pairing valid. Port ghostty's edge-case list (71 resize
tests in PageList.zig) as a checklist, not verbatim.

### Gates

resize_reflow ≤415 µs (stretch: ≤350 on the styled corpus, where ghostty pays
re-interns and rio doesn't); plain resize ≤5.5 µs unchanged; all parse
workloads unchanged; 402+ rio-vt tests green; librio/canario suites green.

### Risks

- remap trackers are the tightest coupling (settlement order vs push order).
- Row/Storage::swap 5-qword debug_assert — don't add Row fields.
- Behavior deltas from step 4 are visible (prompt-nav after resize) — note in
  changelog as a fix.
- Blank-row laziness changes total_lines/history counts in edge cases — vt100
  parity harness will show diffs; adjudicate deliberately.

### Phase 3 status (2026-08-01, paused)

Streaming rewrite DONE on branch `reflow-streaming` (worktree ../rio-reflow,
commit a19670604e): grow+shrink unified in grid/reflow.rs (~350 lines), legacy
slow paths deleted, anchors replace incremental cursor/viewport/remap
bookkeeping, Storage::free is a persistent buffer pool, row metadata
(semantic_prompt/kitty flag) now survives reflow. 413/413 tests, clippy clean.

Numbers: plain resize 5.1 → 4.0 µs; reflow warm (repeated drags, pool cycling)
467 µs/cycle; criterion cold 1006 → 763 µs vs ghostty 415. Profile: remaining
cold gap is ~7-10k per-row Vec regrowths on first widen (~300 µs, structural)
+ ~18 ns/row span-loop overhead. The structural fix is the contiguous-page
experiment in stash@{1} (grid/page.rs, 326 lines, validated in isolation) —
flat Vec<Square> per page, row-major, would put cold reflow under ghostty.

stash@{0} = superseded shrink tail-buffer pool (resize.rs/row.rs WIP).
rio-vt-benchmark/Cargo.toml restored to `=0.5.3` (was temporarily pointed at
the worktree while iterating).
