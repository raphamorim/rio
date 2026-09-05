# Scale/grid desync after sleep and on hidden tabs — root-cause analysis

Analysis only, no code changes. Sources: full trace of rio + rio-window +
sugarloaf (2026-08-02), with ghostty and zed read as reference architectures.
Two user-visible symptoms: (a) wake from sleep → font/grid/scale wrong;
(b) switching to a tab that wasn't visible → font/grid/scale wrong.

## The architectural root

Rio's scale pipeline has exactly **one** entry point and **one** mutator:
`WindowEvent::ScaleFactorChanged` → `Screen::set_scale`. The live window
scale is read once at window creation and never again. Every downstream copy
(sugarloaf context scale, RootStyle, Text, per-panel ContextDimension,
per-grid scaled_margin) is only written by that mutator. So the system's
correctness rests entirely on two assumptions:

1. AppKit always delivers `windowDidChangeBackingProperties:` when anything
   scale-relevant changes, and
2. one delivered event fully invalidates every scale-derived artifact.

Both assumptions fail, in different ways, and each failure mode maps to one
of the two symptoms.

## Symptom (a): sleep/wake — the event never arrives, and the layer decays

Ranked by likely causal contribution:

1. **Numeric de-dupe swallows wake "changes"** (rio-window
   window_delegate.rs:905-909). The only macOS producer compares the new
   `backingScaleFactor` against `previous_scale_factor` and returns if equal.
   After sleep/wake AppKit can rebuild the backing store, reset layer state,
   or briefly report a different screen and settle back — the *number* ends
   up the same, so rio re-runs nothing. There is no second chance:
   `previous_scale_factor` is updated before the event is queued, `resumed()`
   is empty, no `NSApplicationDidChangeScreenParametersNotification` or
   NSWorkspace wake observer exists (one is commented out), and
   `windowDidChangeScreen:` deliberately touches only the display link.
2. **`CAMetalLayer.contentsScale` is set once at creation and never
   re-applied** (sugarloaf metal context). `rescale()` stores the float;
   `resize()` sets only `drawableSize`. If the wake sequence leaves the layer
   with a different effective contentsScale than rio believes, nothing ever
   re-asserts it — the compositor scales the texture and everything looks
   subtly wrong-sized/blurry even though rio's own math is consistent.
3. **Un-occlusion never requests a frame by itself.** `Occluded(false)` only
   flips `needs_render_after_occlusion`; that flag is consumed inside
   PTY-driven render events. An idle shell after wake produces none, so the
   first thing shown is whatever the swapchain held. Compounding it, the
   macOS redraw path is display-link-mediated and the link refuses to start
   while occluded and idle-stops itself.
4. **Drawable-acquisition failure right after wake silently eats a frame.**
   The Metal path returns without discarding/resetting and without re-arming
   a redraw — after per-panel damage was already consumed. That content is
   lost until new damage arrives. (The wgpu path resets; Metal doesn't.)
   This is almost certainly issue #1506's mechanism.
5. **The startup synthesized scale event is dead code** — initialized-equal
   then guarded-equal — so a window created on a non-primary-scale monitor
   during a weird wake state never gets a correcting event either.

## Symptom (b): hidden tabs — dimensions update, pixels don't

The intuitive theory ("background tabs miss the resize fan-out") is wrong:
`Screen::set_scale` and `resize_all_grids` iterate every tab, update every
panel's dimensions, reflow every terminal, and SIGWINCH every PTY. The
staleness is render-side — dimensions are right, the *sprites* are wrong:

1. **Nothing converts "scale changed" into full terminal damage.** The
   rescale path only sets a dirty flag; `Crosswords::resize` early-returns
   without damage when cols/lines are unchanged; the per-panel GridRenderer
   `resize` early-returns when cols/rows are unchanged and never sets
   `needs_full_rebuild`. Damage then merges to Partial/Noop, so resident
   glyph quads keep the old `size_bucket` (old font_px) while the uniforms
   (cell_size, projection) already use the new metrics. That mismatch *is*
   the "font/grid off" frame, and it can hit the active tab too whenever a
   DPI change preserves cols×rows.
2. **The tab-switch safety net is broken by a key collision.** The renderer
   detects "active pane changed" by comparing taffy `NodeId`s — but each tab
   owns its own taffy tree, and identically-shaped trees produce identical
   NodeIds. Switching tabs usually compares equal, so the force-full-damage
   path never fires for the newly shown tab, which then presents GPU buffers
   rasterized under the old scale.
3. **Damage events can't reach background tabs at all**:
   `get_by_route_id` searches only the current tab, so
   `TerminalDamaged`/`RenderRoute`/`UpdateGraphics` for a background pane
   no-op. Whatever a hidden tab missed, it stays missed until its own PTY
   speaks again.
4. **Tab switching performs zero reconciliation** — visibility toggling and
   a dirty mark on the focused pane only. No dimension check, no scale
   comparison, no grid-renderer ensure.
5. Secondary but real: **`ContextGrid.scale` is written once at creation and
   never updated** (paddings, gaps, border hit-boxes stay at creation-time
   DPI for every tab), and **a new tab inherits panel-sized dimensions and
   double-subtracts the margin**, so it's born slightly short until the next
   window resize.

## How ghostty and zed avoid this class of bug

Two opposite but internally consistent strategies:

- **Ghostty: per-surface observers + idempotent push.** Every surface (each
  tab is a real NSWindow) owns its own `viewDidChangeBackingProperties` and
  `didChangeScreen` observers and its own stored scale. Screen change
  *deliberately re-fires* the backing-properties path "just in case the
  scale differs" (their issue 2731 — the exact de-dupe trap rio has). A
  scale change re-derives the framebuffer size itself from the cached
  logical size instead of waiting for a layout pass. Becoming visible
  triggers an immediate unconditional updateFrame+drawFrame. All entry
  points early-return on no-change, which makes redundant re-firing free —
  that's the property that lets them re-sync aggressively.
- **Zed: no per-tab pixel state to go stale.** One `Window.scale_factor`,
  re-pulled wholesale from the platform on resize, move, screen change,
  backing change, *and window activation* (an accidental wake-recovery
  path rio lacks). Hidden tabs are simply not laid out; the terminal's
  alacritty grid is resized from prepaint on the first frame after it
  becomes visible, with a diff in `set_size` making reconciliation free.
  Glyph caches key on scale, so a DPI change produces new atlas entries
  rather than stale ones.

Rio sits in the worst quadrant today: ghostty-style long-lived per-context
caches, but zed-style single-point event delivery — cached state everywhere,
with only one fragile trigger allowed to invalidate it.

## What this implies (direction, not code)

1. **Stop trusting single delivery.** Re-read the live window scale and
   compare against the stored one at cheap, natural checkpoints: window
   focus/activation, occlusion clearing, tab switch, and screen-change. If
   different, run the existing rescale path. This one habit (zed's
   activation re-pull, ghostty's re-fire-on-screen-change) collapses most of
   symptom (a) regardless of which AppKit notification got coalesced.
2. **Make staleness impossible to present, not just unlikely.** The renderer
   should treat scaled font metrics as part of the damage key: if a panel's
   `scaled_font_size`/cell metrics differ from what its GPU buffers were
   built with, that's full damage by definition — independent of cols/rows
   comparisons or event ordering. Equivalently: key the "active changed"
   check on route_id rather than cross-tree NodeIds, and let rescale mark
   full terminal damage explicitly.
3. **Re-assert `contentsScale` whenever scale is applied** (and on
   screen-change/wake checkpoints), instead of assuming the layer still
   holds its creation-time value.
4. **Un-occlusion and drawable-failure must both re-arm a redraw** on their
   own, without waiting for PTY traffic; the Metal drawable-failure path
   must stop consuming damage it didn't present (#1506).
5. **Fix the delivery layer while at it**: register the screen-parameters
   observer, update `previous_scale_factor` only after successful dispatch,
   and un-dead the startup synthesized event.
6. Longer term, pick a side of the ghostty/zed dichotomy for per-tab state:
   either every scale-derived cache gets an owner that re-validates it on
   visibility (ghostty), or per-tab pixel state shrinks until the first
   rendered frame after a switch derives everything live (zed). The current
   hybrid is what makes these bugs recur (see the eight prior scale-fix
   commits in the log).

Related open issues this likely closes or explains: #1506 (wrong rendering
after sleep), the quake-window mixed-DPI residue (positioned while hidden;
backing-properties may not fire for ordered-out windows), and #1680's HiDPI
blurriness reports where users toggled displays.
