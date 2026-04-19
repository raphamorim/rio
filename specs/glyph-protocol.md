# Glyph Protocol

**Author:** Hugo Raphael Vianna Amorim
**Location:** Uppsala, Sweden
**Year:** 2026
**Last updated:** 2026-04-17

**See also:**
- Blog post introducing the protocol and its rationale:
  <https://rapha.land/introducing-glyph-protocol-for-terminals/>
- Reference implementation: [Rio terminal](https://raphamorim.io/rio)
- Example apps (ratatui, bubbletea v2, ink) registering real Nerd Font
  outlines at empty PUA-B slots:
  [glyph-protocol-examples](https://github.com/raphamorim/glyph-protocol-examples)

---

## Abstract

Glyph Protocol is a terminal protocol that lets applications ship
custom vector glyphs to the terminal at runtime without requiring
the user to install a patched font (Nerd Fonts, Powerline, etc.).
Registrations are restricted to the Unicode Private Use Areas —
ranges the user never types and existing text never contains — so
the protocol cannot be used to modify the appearance of real text.

The protocol is transported over APC (Application Program Command)
sequences. The default payload is the OpenType `glyf` simple-glyph
record for monochrome icons; colour icons ride OpenType `COLR` v0
(layered flat colour) or `COLR` v1 (full paint graph). Four verbs
are defined: support-negotiation (`s`), query (`q`), register
(`r`), and clear (`c`).

## 1. Motivation

Terminal applications today rely on out-of-band font distribution
to render non-ASCII iconography. The dominant workflow is:

1. Application author picks codepoints in the Unicode Private Use
   Area.
2. User installs a multi-megabyte patched font that maps those
   codepoints to glyphs.
3. User switches their terminal's font to the patched font.
4. Application emits the codepoint and hopes the mapping is correct.

This workflow has three structural problems:

- **Distribution cost.** Users carry megabytes of glyphs they
  never see.
- **Coupling.** Adding a new icon requires the entire font
  ecosystem to update. Application authors are locked into a fixed
  PUA allocation.
- **Invisible failure.** An application cannot tell whether a
  given codepoint will render; it can only emit it and accept the
  result.

Glyph Protocol moves glyph ownership from the font file to the
application, and gives applications a way to ask the terminal what
it can render before it renders it.

## 2. Design goals

- **Small surface.** Four verbs, three payload formats (one
  required, two optional), no daemons, no caches, no cross-session
  state.
- **Zero new terminal dependencies.** Every terminal that renders
  text already links a `glyf` rasterizer.
- **Resolution independent.** Glyphs are vector and scale to any
  cell size.
- **Graceful degradation.** Terminals that do not implement the
  protocol ignore the APC message. Applications detect support by
  sending a query and watching for a reply.
- **No override of user text.** Registrations are confined to PUA
  codepoints — ranges no user types and no pre-existing text
  contains. The rendered appearance of `a`, `ssh`, or any URL
  cannot be changed by any program that writes to the terminal.
  See §9.
- **Small on the wire.** Typical icons are 150–400 bytes of
  `glyf`, 2–3× smaller than the equivalent SVG.

## 3. Transport

Glyph Protocol uses APC (Application Program Command,
`ESC _ ... ESC \`). APC is specified for application-defined
commands; terminals that do not implement a particular APC command
are required to ignore it, making APC safer than OSC for
introducing new protocols.

### 3.1 Identifier

Every Glyph Protocol message begins with the Unicode codepoint
**U+25A1** (WHITE SQUARE), written in the message as the
lowercase hex string `25a1`. Terminals MUST ignore any APC
message whose body does not begin with this identifier.

### 3.2 Framing

The general form of a Glyph Protocol message is:

```
ESC _ 25a1 ; <verb> [ ; key=value ]* [ ; <payload> ] ESC \
```

Parameter keys use lowercase ASCII. Values are lowercase hex for
codepoints, decimal for integers, base64 for binary payloads, and
decimal u8 values for the `status` field of every response.

### 3.3 Verbs

| Verb | Meaning |
|------|---------|
| `s`  | Advertise supported payload formats. Doubles as a protocol-detection ping — any reply confirms Glyph Protocol; a timeout means unsupported. |
| `q`  | Query the state of a codepoint. |
| `r`  | Register a glyph for a PUA codepoint. |
| `c`  | Clear one slot or every slot in this session's glossary. |

The `s` verb takes no parameters and returns a decimal `u8`
bitfield under the `fmt` key:

| Bit | `fmt=` value | Format name |
|-----|--------------|-------------|
| 0   | 1            | `glyf` (monochrome simple-glyph; §8). |
| 1   | 2            | `colrv0` (layered flat colour; §8.6). |
| 2   | 4            | `colrv1` (OpenType paint graph; §8.7). |

Further bits are reserved. Clients treat unknown bits as
unsupported and ignore them. A terminal that advertises only
`fmt=1` (monochrome) and receives an `r` with `fmt=colrv0` /
`fmt=colrv1` MUST reject the registration (`reason=malformed_payload`
is acceptable for parser-level rejection). Clients SHOULD check
the `s` reply before emitting colour registrations so they can
fall back to a monochrome `fmt=glyf` without making a doomed
round-trip.

## 4. Glossary namespace

Registrations target codepoints the application picks, constrained
to the three Unicode Private Use Areas:

| Range                        | Plane | Common use |
|------------------------------|-------|------------|
| `U+E000`–`U+F8FF`             | BMP   | Basic PUA. Nerd Fonts, Powerline, Font Awesome all live here. |
| `U+F0000`–`U+FFFFD`           | 15    | Supplementary PUA-A. Nerd Fonts v3 Material icons live here. |
| `U+100000`–`U+10FFFD`         | 16    | Supplementary PUA-B. No common convention — clean space for apps that want it. |

Any other codepoint — ASCII, Latin-1, CJK, emoji, control chars —
is rejected by `r` and `c` with `reason=out_of_namespace`.

Each terminal session holds at most **256 simultaneous
registrations**. When the glossary is full and a new `r` arrives,
the terminal evicts the oldest registration in FIFO order; the new
registration succeeds. Applications that cannot tolerate silent
eviction SHOULD query their codepoint with `q` before emitting.

Each terminal session (tab, pane, PTY) owns its own glossary. Two
sessions can independently register `U+E0A0`, each pointing at a
different glyph. Registrations MUST NOT leak between sessions.

## 5. Query (`q`)

### 5.1 Request

```
ESC _ 25a1 ; q ; cp=<hex> ESC \
```

Parameters:

- `cp` — codepoint in hex. Any valid Unicode scalar value (not a
  surrogate). May be inside or outside PUA.

### 5.2 Response

```
ESC _ 25a1 ; q ; cp=<hex> ; status=<u8> ESC \
```

`status` is a decimal u8 encoding a two-bit field:

| Value | State       | Meaning |
|-------|-------------|---------|
| `0`   | `free`      | No font in the fallback chain renders `cp`, and the glossary has no registration for it. The cell will render as tofu. |
| `1`   | `system`    | Some font in the fallback chain renders `cp`. No glossary registration (or `cp` is outside PUA). |
| `2`   | `glossary`  | `cp` is in PUA and has a live registration in this session. No system font covers it. |
| `3`   | `both`      | `cp` is in PUA, has a live registration, AND a system font also covers it. The registration shadows the system font at render time. |

Bit 0 = system coverage, bit 1 = glossary coverage.

For non-PUA codepoints only `0` and `1` are possible.

## 6. Register (`r`)

### 6.1 Request

```
ESC _ 25a1 ; r ; cp=<hex> ; fmt=glyf ; reply=<0|1|2> ; upm=<int> ; <base64-payload> ESC \
```

Parameters:

- `cp` — target codepoint in hex. MUST be in one of the PUA ranges
  defined in §4. Otherwise the request is rejected with
  `reason=out_of_namespace`.
- `fmt` — payload format. One of `glyf`, `colrv0`, `colrv1`.
  Optional; `glyf` is the default. See §8 for each format's wire
  layout.
- `reply` — reply-level control. Optional; default `1`.
  - `reply=0` — the terminal MUST NOT emit any reply for this
    registration (neither success nor failure). Intended for bulk
    fire-and-forget startup registrations that won't be read back.
  - `reply=1` — the terminal emits both success and failure replies
    (the default; equivalent to omitting the parameter).
  - `reply=2` — the terminal emits failure replies only; success
    registrations are silent. Useful for bulk registrations that
    still want to learn about the broken ones without a success
    ACK for every glyph.
  Unknown values fall back to `reply=1`.
- `upm` — units per em, the coordinate space the outline is
  authored in. Optional; default `1000`.
- payload — base64-encoded payload for the declared `fmt`.

### 6.2 Response

Replies are gated by the request's `reply` parameter (§6.1).
For `reply=1` (the default), successful registrations emit:

```
ESC _ 25a1 ; r ; cp=<hex> ; status=0 ESC \
```

`cp` is echoed from the request.

Failures, when not suppressed by `reply=0`, emit:

```
ESC _ 25a1 ; r ; cp=<hex> ; status=<nonzero u8> ; reason=<code> ESC \
```

At `reply=2`, successful registrations are silent; failures still
emit the error reply above. At `reply=0`, neither success nor
failure produces any output — the registration is fire-and-forget.

Defined error codes:

| Code                    | Meaning |
|-------------------------|---------|
| `out_of_namespace`      | `cp` is not in any PUA range. |
| `composite_unsupported` | Payload contains composite glyphs. |
| `hinting_unsupported`   | Payload contains hinting instructions. |
| `malformed_payload`     | Payload failed to parse as `glyf`. |
| `payload_too_large`     | Payload exceeds 64 KiB post-base64-decode. |

### 6.3 Overwrite and eviction

A second `r` on the same `cp` overwrites the first. This is how
applications update a glyph or react to theme changes.

When the glossary already holds 256 registrations and the new `r`
is for a `cp` that is NOT already registered, the terminal evicts
the oldest registration (FIFO) to make room. Eviction silently
invalidates the evicted codepoint: subsequent emissions fall
through to the system font (or tofu). Applications SHOULD query
before emitting if they cannot tolerate silent eviction.

### 6.4 Lifetime

Registrations live for the duration of the terminal session. A
terminal reset (e.g. `ESC c`) MAY clear the entire glossary.
Registrations MUST NOT persist across terminal restarts.

## 7. Clear (`c`)

### 7.1 Request

```
ESC _ 25a1 ; c [ ; cp=<hex> ] ESC \
```

If `cp` is omitted, every slot in the session's glossary is
cleared. Otherwise the slot corresponding to `cp` is cleared. `cp`
MUST be in a PUA range; otherwise the request is rejected with
`reason=out_of_namespace`.

### 7.2 Response

Success:

```
ESC _ 25a1 ; c ; status=0 ESC \
```

Clearing an empty slot is a no-op and MUST return `status=0`.

Failure:

```
ESC _ 25a1 ; c ; status=1 ; reason=out_of_namespace ESC \
```

### 7.3 Cache invalidation

When a slot is cleared (explicitly, via overwrite, or via
eviction), the terminal MUST invalidate any rasterization cached
for that codepoint. A subsequent `r` that reuses the codepoint
MUST rasterize the new outline fresh, not serve stale pixels.

## 8. Payload format: `glyf`

### 8.1 Scope

Glyph Protocol reuses the OpenType `glyf` table's simple-glyph
record as its wire format. Authoritative references:

- OpenType `glyf` specification (Microsoft Typography).
- Apple TrueType Reference Manual, Chapter 6.

### 8.2 Constraints

Terminals implementing Glyph Protocol MUST accept the following
subset of `glyf` and MAY reject anything else with
`reason=composite_unsupported` or `reason=hinting_unsupported`:

- **Simple glyphs only.** No composite glyphs, no references to
  other glyphs.
- **Standard flag encoding** as defined by the OpenType spec
  (on-curve, off-curve, x-short, y-short, repeat).
- **No hinting instructions.** The `instructionLength` field MUST
  be zero.
- **Coordinate space** defined by `upm`. The terminal maps this
  space onto its cell at render time.

### 8.3 Contour semantics

A `glyf` record stores a glyph as a set of closed contours. Each
contour is a sequence of points; each point carries a single
on-curve/off-curve flag bit. Contour walking follows standard
TrueType semantics:

- Two on-curve points in a row → straight line.
- An off-curve point between two on-curve points → quadratic
  Bézier with the off-curve point as the control point.
- Two off-curve points in a row → an implied on-curve point at
  their midpoint.

### 8.4 Color

`glyf` outlines carry no color. Terminals MUST render them in the
current foreground color. For colored icons see the `colrv0` and
`colrv1` formats in §8.6 / §8.7.

### 8.5 Scaling

The `upm` value defines the glyph's authoring coordinate space.
The terminal maps that space onto its cell at render time.
Applications MUST NOT assume a particular cell size and MUST NOT
re-register glyphs on font size change.

### 8.6 Payload format: `colrv0`

`fmt=colrv0` carries a layered flat-colour glyph using the
OpenType `COLR` v0 and `CPAL` tables verbatim. The protocol wraps
those tables in a small container that also ships the simple-glyph
outlines each layer references, so a colour glyph is self-
contained: no external font needed.

**Container layout** (all integers big-endian, post-base64-decode):

```
u16     n_glyphs              # 1..=256
repeat n_glyphs:
  u16   glyf_len
  glyf_len bytes              # simple-glyph record, §8.2 subset
u16     colr_len              # > 0
colr_len bytes                # OpenType COLR v0 table
u16     cpal_len              # may be 0 (see below)
cpal_len bytes                # OpenType CPAL table (required for v0)
```

`GlyphId` values in the `COLR` table resolve to indices into the
outline array (glyph 0 is the base glyph rendered when the
terminal emits `cp`). `paletteIndex` values in the `COLR` layer
records resolve to entries in the CPAL colour records array, in
standard OpenType order (one record = one BGRA quadruple).
`paletteIndex = 0xFFFF` MUST be rendered as the current foreground
colour, per the OpenType spec.

**Rendering rules.**

- Layers composite in painter order (first layer painted first).
- Per-layer colours come from CPAL; `0xFFFF` means foreground.
- `COLR` v0 defines no transforms or compositing modes beyond
  `src-over`, so terminals MAY implement v0 in one pass with no
  graphics-state stack.

**Validation.** Terminals SHOULD validate the wrapped `COLR` and
`CPAL` tables using an OpenType parser (e.g. `ttf-parser`); a
`COLR` table that fails to parse SHOULD be rejected with
`reason=malformed_payload`. Every carried outline MUST satisfy the
`glyf` simple-glyph subset of §8.2; violations use the same error
codes as `fmt=glyf`.

### 8.7 Payload format: `colrv1`

`fmt=colrv1` shares the container layout of §8.6 but ships an
OpenType `COLR` v1 table, which adds a full paint graph: linear,
radial, and sweep gradients, affine transforms, clip boxes, and
per-layer compositing modes. `CPAL` remains valid but is optional
— v1 paints may carry sRGBA directly — so `cpal_len = 0` is
permitted and means "the COLR references no palette index."

**Paint types.** Terminals implementing `colrv1` SHOULD support
the full OpenType paint-graph vocabulary for maximum interop. A
conforming subset for low-overhead implementations is:

- Solid (direct sRGBA or palette index).
- Linear gradient.
- Radial gradient.
- Affine transforms on paint subtrees.
- `src-over` layer composite.

Terminals MAY render unsupported paint nodes (sweep gradients,
blend modes beyond `src-over`, variations) using a reasonable
fallback — typically the paint subtree's first solid colour —
rather than rejecting the registration.

**Foreground inheritance.** CPAL palette index `0xFFFF` and
v1's `PaintSolid` with the foreground sentinel MUST resolve to
the cell's current foreground colour at rasterisation time.
Terminals that cache rasterised colour glyphs MUST re-rasterise
on foreground change for any glyph whose paint graph references
`0xFFFF`.

**Security.** The colour formats add no new attack surface beyond
§9: `cp` is still PUA-only, the cell buffer is still authoritative
for copy/selection, and registrations are still session-scoped.
A malformed `COLR` is a rendering error, not an injection vector
— the rendered pixels can only affect cells the client itself
emits at a PUA codepoint.

### 8.8 Authoring

Most applications will not hand-author `COLR` bytes either.
Typical flows:

- **From an existing colour font.** Use `fontTools` to extract the
  `COLR`/`CPAL` tables for the glyphs of interest, then pack them
  with the referenced outlines into the container above.
- **From SVG.** The Skia team publishes `nanoemoji` / `maximum-color`,
  which compiles a directory of SVGs into a `COLR` v1 font; feed
  its output into the packer.

Rio ships an `svg2colr` helper alongside `svg2glyf` for this flow.

## 9. Security considerations

The core property Glyph Protocol must preserve is that **an
application cannot change how existing text looks**. Enforcement
is structural:

- Register accepts a `cp` parameter, but `cp` MUST be in one of the
  three Unicode Private Use Areas (§4). Any non-PUA codepoint is
  rejected with `reason=out_of_namespace`.
- Users never type PUA codepoints. No pre-existing text —
  filenames, URLs, commands, variable names, log lines — contains
  them. A program that registers a glyph can only affect how PUA
  codepoints render, and PUA codepoints only appear in text the
  same application (or another one opting into the same
  convention) has deliberately emitted.
- The cell buffer is authoritative. Selection, copy, search,
  hyperlinks, shell history, and any other text extraction MUST
  return the codepoint the application emitted, never the
  rendered glyph.

Without these properties, a program writing to the PTY could
register a glyph for `a` that looks like `o` and mislead the
reader. With them, the worst a program can do is render a
weird-looking character at a PUA codepoint the user never types
and the cell buffer honestly reports.

Other considerations:

- **Resource bounds.** The 256-slot cap and 64 KiB per-payload cap
  give a hard upper bound of 16 MiB on the glossary's memory
  footprint per session.
- **No code execution.** The `glyf` subset defined in §8.2
  excludes hinting instructions, which is the only part of
  TrueType that is executable. Glyph Protocol is purely
  declarative.
- **No filesystem access.** Glyph Protocol messages do not
  reference files and MUST NOT be used to load data from disk.
- **Session isolation.** Glossaries MUST NOT leak between terminal
  tabs, windows, multiplexer panes, or PTY sessions.

## 10. Non-goals (v1)

- **No non-PUA codepoints.** Registration is restricted to the
  three PUA ranges — see §4.
- **No ligatures.** Registration applies to a single codepoint.
  Sequence-keyed substitution is out of scope; programming
  ligatures are already handled by OpenType fonts.
- **No persistence across sessions.** Glyphs are shipped fresh on
  each run.
- **No cross-application sharing.** Each terminal session owns its
  glossary. No IPC, no daemon.
- **No bitmap colour glyphs.** Colour is delivered via `colrv0`
  and `colrv1` (§8.6 / §8.7), which are vector. `CBDT`/`sbix`/
  `SVG ` tables are explicitly out of scope so resolution
  independence is preserved.
- **No subpixel positioning control.** The terminal's normal cell
  positioning applies.
- **No bitmap payloads.** Vector only, to preserve resolution
  independence.

## 11. Conformance

A terminal emulator is Glyph Protocol v1 conformant if it:

1. Recognizes the `25a1` identifier in APC sequences.
2. Implements the `s`, `q`, `r`, and `c` verbs with the semantics
   defined in this specification, and advertises every accepted
   payload format via the `fmt=` bitfield in the `s` reply.
3. Restricts register/clear `cp` to the three PUA ranges; rejects
   anything else with `reason=out_of_namespace`.
4. Holds at most 256 simultaneous registrations per session and
   evicts in FIFO order when full.
5. Accepts the `glyf` simple-glyph subset defined in §8.2. The
   `colrv0` and `colrv1` formats are OPTIONAL; terminals that
   accept them MUST set the corresponding bit in the `s` reply.
6. Renders registered `glyf` glyphs in the current foreground
   color; renders `colrv0`/`colrv1` glyphs using the COLR paint
   graph, resolving palette index `0xFFFF` to the current
   foreground color.
7. Scales glyphs according to `upm` and the current cell size.
8. Enforces the cell-buffer authority invariant in §9: selection,
   copy, and search return the raw codepoint.
9. Ignores unrecognized parameters rather than failing the
   request.

A client (application) is Glyph Protocol v1 conformant if it:

1. Emits `cp` only from the three PUA ranges.
2. Treats query timeout as "terminal does not implement Glyph
   Protocol."
3. Emits only the `glyf` subset defined in §8.2.
4. Handles all `reason=*` error codes without crashing.

## 12. Reference implementation

The reference implementation ships in Rio terminal. A companion
helper, `svg2glyf`, ships alongside to convert existing SVG assets
to the accepted `glyf` subset at build time.

## Appendix A. Worked example: register an icon in empty PUA

```python
import base64, sys
from fontTools.pens.ttGlyphPen import TTGlyphPen

# A stylised outline in glyf coordinate space (upm=1000, Y-up).
pen = TTGlyphPen(None)
# ... draw commands ...
pen.closePath()

payload = base64.b64encode(pen.glyph().compile(None)).decode("ascii")

# Register at U+100000 — the first codepoint of Supplementary PUA-B.
# No known font covers this range, so the registration is the sole
# source of the rendered glyph and the demo is unambiguous.
sys.stdout.write(
    f"\x1b_25a1;r;cp=100000;upm=1000;{payload}\x1b\\"
)
sys.stdout.flush()

# From now on, U+100000 renders our outline.
sys.stdout.write(f"icon: {chr(0x100000)}\n")
```

## Appendix B. Worked example: query before registering

```python
import sys

def q(cp: int) -> None:
    sys.stdout.write(f"\x1b_25a1;q;cp={cp:x}\x1b\\")
    sys.stdout.flush()

# Does the user already have Nerd Fonts installed?
q(0xE0A0)
# Expected reply parsed from the PTY:
#   status=1 → system font covers it; don't register, just emit cp
#   status=0 → nothing covers it; register and emit
```

## Appendix C. Implementation notes

These are not normative but reflect lessons from the first
implementations.

**Response draining.** `r` and `c` always produce an APC reply on the
PTY. Client applications that register at startup and do not care
about the reply should either (a) read and discard it, or (b) accept
that the response will be delivered. In practice, TUI frameworks
(ratatui, bubbletea, ink) consume stdin through their input reader;
APC replies are parsed and silently dropped alongside non-keyboard
bytes. The failure mode to watch for is sending `r` or `c` AFTER the
framework has torn down its input reader — typically on exit — at
which point the reply arrives in the PTY but nobody reads it, and
the shell that takes over the PTY after the app exits emits the
queued bytes as visible text (`.25a1;c;status=0`). Either skip
cleanup on exit (registrations expire with the session anyway) or
send the cleanup command while the framework is still running and
let its input reader swallow the reply.

**Practical source of `glyf` data.** Most apps will not hand-author
`glyf` bytes. The typical pipeline is:

1. Open a Nerd Font or similar icon TTF with `fontTools`.
2. For each codepoint of interest, pull the glyph record.
3. If composite, flatten via `fontTools.pens.ttGlyphPen.TTGlyphPen`.
4. Strip hinting instructions (`instructionLength := 0`).
5. Compile to bytes; base64-encode; register at a codepoint of the
   app's choosing.

Because the source codepoint in the font is irrelevant to the
protocol, applications commonly pull outlines from a Nerd Font's
basic-PUA codepoints (`U+E0A0`, `U+F07B`, …) and register them at
Supplementary PUA-B slots (`U+100000`+) — that way the rendered glyph
is unambiguously from the registration, not from a system font that
happens to cover the same codepoint.

**Atlas cache invalidation.** The cache-invalidation rule in §7.3
applies to overwrite and eviction too, not only explicit clear.
Implementations that key their glyph atlas on some stable slot id
(rather than `cp`) must ensure the slot id is either released on
clear/evict or paired with a per-registration invalidation tag, so
that a subsequent register reusing the id rasterizes fresh bytes
rather than serving a stale bitmap.

## Appendix D. Change log

| Date       | Version | Notes |
|------------|---------|-------|
| 2026-04-17 | v1      | Initial release. Register accepts a client-picked `cp` restricted to PUA; 256-slot glossary with FIFO eviction; numeric `status` field; ligatures out of scope. |
| 2026-04-19 | v1.1    | Added `s` verb (support advertisement / protocol ping). |
| 2026-04-19 | v1.2    | Added `fmt=colrv0` and `fmt=colrv1` payload formats wrapping OpenType `COLR` / `CPAL` tables with sidecar `glyf` outlines. Both advertised via bits 1 and 2 of the `s` reply's `fmt=` bitfield. |
| 2026-04-19 | v1.3    | Added `reply=0|1|2` parameter to the `r` verb so bulk registrations can suppress success ACKs (`reply=2`) or go fully fire-and-forget (`reply=0`). Default `reply=1` preserves v1.0 behaviour. |
