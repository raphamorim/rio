# Glyph Protocol

**Author:** Raphael Amorim
**Year:** 2026
**Last updated:** 2026-11-18

**See also:**
- Blog post introducing the protocol and its rationale:
  <https://rapha.land/introducing-glyph-protocol-for-terminals/>
- Reference implementation: [Rio terminal](https://raphamorim.io/rio)
- Example apps (ratatui, bubbletea v2, ink) registering real Nerd Font
  outlines and colrv1/colrv0 emojis in the registration block:
  [glyph-protocol-examples](https://github.com/raphamorim/glyph-protocol-examples)

---

## Abstract

Glyph Protocol is a terminal protocol that lets applications ship
custom vector glyphs to the terminal at runtime (as `vg`, SVG path
data with fills, strokes, gradients and clips; as OpenType `glyf`,
monochrome; or as OpenType `COLR` v0, flat layered colour, and
`COLR` v1, a paint graph with gradients, transforms, and
composites) without requiring the user to install a patched font
(Nerd Fonts, Powerline, etc.). Registrations are restricted to one
reserved block of Supplementary Private Use Area-B, which no font or
convention assigns, so the protocol cannot be used to modify the
appearance of real text or of icons from an installed font.

The protocol is transported over APC (Application Program Command)
sequences. Every payload format is optional, and a terminal lists
the ones it accepts; `vg` is the recommended one, since applications
can send the SVG path data their icon sets already ship. Four verbs
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

- **Small surface.** Few verbs, four payload formats (all
  optional, negotiated through `s`), no daemons, no caches, no
  cross-session state.
- **Few new terminal dependencies.** `vg` needs a path filler with
  gradients, which any 2D graphics library has. Terminals that
  render OpenType text already link a `glyf` rasterizer, and those
  that render Apple / Google colour emoji already parse `COLR` +
  `CPAL`, so they can offer those formats from the tables their font
  stack already decodes.
- **Resolution independent.** Glyphs are vector and scale to any
  cell size.
- **Graceful degradation.** Applications detect support by sending
  `s` and watching for a reply, and send nothing else unless the
  terminal answered. Most terminals that do not implement the
  protocol discard `s`; §3.4 covers the programs that don't.
- **No override of user text.** Registrations are confined to a
  reserved block that nothing else assigns. The rendered appearance
  of `a`, `ssh`, any URL, or a Nerd Fonts icon in the user's prompt
  cannot be changed by any program that writes to the terminal.
  See §9.
- **Small on the wire.** Typical icons are 50–400 bytes of
  `glyf`, or a few hundred bytes of `vg` path text.

## 3. Transport

Glyph Protocol uses APC (Application Program Command,
`ESC _ ... ESC \`). ECMA-48 defines APC as a control string whose
content is interpreted by the program that receives it, so
everything between the introducer and the string terminator is
defined by this specification. ECMA-48 does not require a program
that doesn't implement a command to ignore it. Most terminal
emulators discard an APC they don't recognise, but some programs
give APC a meaning of their own; §3.4 says how clients detect
support so that such programs receive the probe and nothing else.

### 3.1 Identifier

Every Glyph Protocol message begins with the Unicode codepoint
**U+25A1** (WHITE SQUARE), written in the message as the
lowercase hex string `25a1`, followed by `;`. Terminals MUST
ignore any APC message whose body does not begin with `25a1;`.

Some terminals dispatch control strings the way DEC structures
them, by intermediate and final bytes like an escape sequence. To
such a dispatcher a Glyph Protocol message is `APC 2`: `2` (03/02)
is the final byte, with no intermediates, and the rest of the
identifier starts the command string, so `ESC _ 25a1 ; s ESC \`
is `APC 2` with the command string `5a1;s`. A terminal that
dispatches this way handles `APC 2` and checks that the command
string begins with `5a1;`. An `APC 2` whose command string doesn't
is not Glyph Protocol, and is treated like any unknown APC.
`2` is in the 03/00–03/15 final range that ECMA-35 leaves for
private use.

### 3.2 Framing

The general form of a Glyph Protocol message is:

```
ESC _ 25a1 ; <verb> [ ; key=value ]* [ ; <payload> ] ESC \
```

Parameter keys use lowercase ASCII. Values are lowercase hex for
codepoints, decimal for integers, base64 for binary payloads, and
lowercase ASCII names for enums (single value, or comma-separated
for lists). The `status` field's shape is verb-specific: `r` and
`c` return a `u8` (`0` = success, nonzero paired with `reason=`);
`q` returns a comma-separated coverage list (§5.2), or no `status`
and a `reason=` when it cannot answer (§5.4).

A message that carries a payload (`r`) carries it as its last
`;`-separated field, after every key; no key follows it. Terminals
MUST take the last field as the payload by position, not by its
content, since a payload can itself contain `=` (base64 padding, for
one). A payload never contains `;`: base64 has none, and a format
whose payload is text MUST exclude it.

### 3.3 Verbs

| Verb | Meaning |
|------|---------|
| `s`  | Advertise supported payload formats and optional capabilities. Doubles as a protocol-detection ping: any reply confirms Glyph Protocol; a DA1 reply with no `s` reply before it means unsupported (§3.4). |
| `q`  | Query the state of a codepoint or of a grapheme cluster. |
| `r`  | Register a glyph for a codepoint in the registration block. |
| `c`  | Clear one slot or every slot in this session's glossary. |

The `s` verb takes no parameters. The reply lists the terminal's
supported payload formats in the `fmt` key as a comma-separated
list of lowercase ASCII names, with no surrounding whitespace:

```
ESC _ 25a1 ; s ; fmt=vg,glyf,colrv0,colrv1 ESC \
```

Format names defined in v1.17:

| Name     | Format                                                   |
|----------|----------------------------------------------------------|
| `vg`     | SVG path data with fills, strokes, gradients and clips (§8.9). Recommended. |
| `glyf`   | Monochrome simple-glyph (§8.1).                          |
| `colrv0` | Layered flat colour, OpenType `COLR` v0 + `CPAL` (§8.6). |
| `colrv1` | OpenType paint graph, `COLR` v1 (§8.7).                  |

Every format is optional. A terminal lists the formats it accepts,
and SHOULD accept `vg`. Order is not significant; clients MUST treat
the value as a set. An empty `fmt=` value means the terminal
recognises Glyph Protocol but currently advertises no payload
formats, so every `r` will be rejected. Clients MUST ignore names
they do not recognise rather than failing the reply, so future
format names are forward-compatible.

A terminal MUST reject an `r` whose format it does not list with
`reason=unsupported_format` (§6.2), so a client can tell a format it
can't use from a payload that's wrong. Clients SHOULD pick a format
from the `s` reply rather than learn it from that rejection.

The reply also lists optional capabilities in the `cap` key, in the
same form:

```
ESC _ 25a1 ; s ; fmt=vg,glyf,colrv0,colrv1 ; cap=grapheme ESC \
```

Capability names defined in v1.14:

| Name       | Capability                                              |
|------------|---------------------------------------------------------|
| `grapheme` | `q` answers for a grapheme cluster, `grapheme=` (§5.3). |

A terminal with no optional capabilities MAY leave `cap` out; a
missing `cap` and an empty `cap=` mean the same. Clients MUST treat
the value as a set and ignore names they do not recognise.

### 3.4 Detecting support

Clients detect support by sending `s` followed by Primary Device
Attributes (DA1, `CSI c`):

```
ESC _ 25a1 ; s ESC \  CSI c
```

Terminals answer requests in the order they receive them, and
nearly every terminal answers DA1. If the DA1 reply arrives with
no `s` reply before it, the terminal does not implement Glyph
Protocol. This is how kitty's graphics and keyboard protocols are
detected too.

Clients MUST NOT send `q`, `r`, or `c` to a terminal that has not
answered `s`. A program that doesn't implement the protocol then
receives one short `s` message at most, instead of every
registration.

Some programs give APC a meaning of their own, and act on the `s`
message instead of discarding it:

- GNU screen shows the content of an APC in its hardstatus line,
  or as the window title when there is none.
- tmux sets the pane title from an APC it doesn't recognise, when
  `allow-set-title` is on (the default), so the title becomes
  `25a1;s`.

That is a side effect of the probe, not a failure: the multiplexer
never answers `s`, so the client concludes the protocol is
unsupported and sends nothing else. The protocol can't work through
a multiplexer as is anyway, since a registration has to reach the
terminal that draws the glyph and the multiplexer sits in between.

A client MAY wrap its messages in the multiplexer's passthrough
(for tmux, `ESC P tmux; … ESC \` with every `ESC` inside doubled,
which needs `allow-passthrough` on). It then wraps DA1 too, since
the multiplexer answers an unwrapped DA1 itself, before the outer
terminal could answer `s`. Replies from the outer terminal reach
the client through the multiplexer's key input. Through
passthrough, registrations belong to the outer terminal's session
and every pane in it shares them.

## 4. Glossary namespace

Registrations target codepoints the application picks, constrained
to the **registration block**: the 1024 codepoints
**`U+100000`–`U+1003FF`**, at the start of Supplementary Private Use
Area-B (plane 16).

Any other codepoint is rejected by `r` and `c` with
`reason=out_of_namespace`. That includes the rest of the Private Use
Areas: the Basic PUA (`U+E000`–`U+F8FF`) and Supplementary PUA-A
(`U+F0000`–`U+FFFFD`) are where Nerd Fonts, Powerline, and Font
Awesome put their icons, and a registration there would replace an
icon the user's font already draws (§9).

Nothing assigns codepoints in the block. Nerd Fonts has no glyphs in
plane 16 (its v3.5.1 glyph list stops at `U+F1AF0`), and its
maintainers have confirmed it never will: an OpenType font holds at
most 65,535 glyphs and PUA-A alone has 65,534 codepoints, so a Nerd
Font fills PUA-A before it could need PUA-B (nerd-fonts#2105).
kitty's Unicode placeholder `U+10EEEE` is outside it.

The block is exactly as large as the glossary: each terminal session
holds at most **1024 registrations**, one per codepoint in the block.
One codepoint = one slot regardless of payload format: a `fmt=colrv1`
registration with 500 inner outlines still consumes exactly one
glossary slot. Every codepoint in the block can be registered at
once, so a registration is never evicted to make room for another.

Each terminal session (tab, pane, PTY) owns its own glossary. Two
sessions can independently register `U+100000`, each pointing at a
different glyph. Registrations MUST NOT leak between sessions.

## 5. Query (`q`)

### 5.1 Request

```
ESC _ 25a1 ; q ; cp=<hex> ESC \
ESC _ 25a1 ; q ; grapheme=<hex>,<hex>,... ESC \
```

Parameters, exactly one of:

- `cp`: codepoint in hex. Any valid Unicode scalar value (not a
  surrogate). May be inside or outside the registration block, so a
  client can ask whether the user's font already has an icon (for
  example a Nerd Fonts codepoint) before registering its own.
- `grapheme`: one grapheme cluster, as 1 to 32 comma-separated hex
  scalar values in logical order. OPTIONAL for terminals; a client
  MUST NOT send it unless the `s` reply lists `grapheme` in `cap` (§3.3).
  See §5.3.

### 5.2 Response

```
ESC _ 25a1 ; q ; cp=<hex> ; status=<list> ESC \
```

`status` is a comma-separated list of coverage names: the set of
sources that can render `cp` in this session. Order is not
significant; clients MUST treat the value as a set.

| `status=` value     | Meaning |
|---------------------|---------|
| (empty)             | No font in the fallback chain renders `cp`, and the glossary has no registration for it. The cell will render as tofu. |
| `system`            | A font renders `cp`, as the terminal would currently render it in the regular face, user-configured font mappings (such as symbol maps) included. No glossary registration (or `cp` is outside the registration block). |
| `glossary`          | `cp` is in the registration block and has a live registration in this session. No system font covers it. |
| `system,glossary`   | `cp` is in the registration block, has a live registration, AND a system font also covers it. The registration takes precedence at render time. |

For codepoints outside the registration block only the empty value
and `status=system` are possible. Clients MUST ignore unknown
coverage names so future sources are forward-compatible.

### 5.3 Grapheme query

A codepoint query answers whether each codepoint has a glyph. It
cannot answer whether a cluster renders correctly: a ZWJ emoji
sequence can have every codepoint covered and still come out as
three separate people, and an emoji presentation selector has no
glyph of its own, so asking for it alone reports tofu. `grapheme`
asks about the cluster as a whole.

```
ESC _ 25a1 ; q ; grapheme=1f468,200d,1f469,200d,1f467 ESC \
ESC _ 25a1 ; q ; grapheme=1f468,200d,1f469,200d,1f467 ; status=system ESC \
```

The reply echoes `grapheme` in lowercase hex. `status` keeps the
meaning of §5.2, the set of sources that draw the cluster, with one
difference: it is empty unless the terminal would render the whole
cluster correctly, in the current mode, if it were printed at the
cursor. In particular, `status` is empty when:

- a codepoint that is not default-ignorable would draw as a missing
  glyph: the source the terminal would draw it from (the registry,
  or the font it picks for the cluster) does not have it, even if
  some other installed font does, or
- the terminal would not draw the cluster as one unit: it would split
  it across separate cells (for example because grapheme cluster
  mode 2027 is off), or
- the cluster is an emoji sequence (a ZWJ, modifier, flag, or tag
  sequence, which Unicode Technical Standard #51 says displays as a
  single image) and shaping does not turn it into one glyph, as
  when the font does not have that sequence.

Other scripts are not held to one glyph: a Devanagari conjunct or a
base with stacked marks legitimately shapes to several.

Default-ignorable codepoints (`U+200D`, the variation selectors
`U+FE00`–`U+FE0F`, tag characters, and the rest of Unicode's
`Default_Ignorable_Code_Point` set) have no glyph of their own and
are never reported as missing. They count only through their effect
on the cluster.

The client segments the text; the terminal answers for exactly the
codepoints it was given. A terminal does not re-segment `grapheme`
or report how it would cluster it. If the codepoints would not form a
single cluster in the terminal, they do not render as one unit and
`status` is empty.

`glossary` appears when the cluster contains a registered
registration-block codepoint. A registered glyph is a single
codepoint and never joins with its neighbours (§10), so a cluster
that pairs one with another visible codepoint answers empty.

The answer is for the regular face, and for the cluster printed as
plain text in the current mode. A client that places the cluster with
explicit cell widths, such as kitty's text sizing protocol (OSC 66),
decides its cells itself; the parts of the answer about coverage and
joining still apply. The answer does not report widths: a cluster's
cell width is the business of the terminal's width tables, mode 2027
and any explicit width, and a registered glyph is drawn into the
cells its character occupies (§8.5.2).

### 5.4 Errors

When a terminal cannot answer a query it replies with `reason=` and
no `status`, echoing the request's `cp` or `grapheme` when it parsed
one:

```
ESC _ 25a1 ; q ; grapheme=1f468,200d,1f469 ; reason=unsupported ESC \
```

| `reason=`         | Meaning |
|-------------------|---------|
| `unsupported`     | `grapheme` was sent but the terminal does not list `grapheme` in `cap`. |
| `malformed_query` | Neither or both of `cp` and `grapheme`, an invalid hex value, a surrogate, or more than 32 codepoints in `grapheme`. |
| `unknown`         | The terminal can't answer without a slow lookup, or is limiting how many queries it answers. |

Answering can mean running font fallback, and for `grapheme`
shaping, which can take milliseconds through fontconfig and similar.
A terminal MUST NOT let queries stall its input handling: it MAY
answer `reason=unknown` instead of looking a font up, and MAY limit
the rate at which it answers, answering the rest `reason=unknown`. A
client that gets `unknown` can ask again later, or treat the
codepoint as not covered.

Terminals written for v1.13 and earlier may ignore a `q` without
`cp` and send nothing, which is why clients check `cap` first.
Clients MUST ignore unknown `reason` values.

### 5.5 Ordering

A terminal answers `q` requests in the order it received them, so a
client can send several queries, one per cluster, without waiting
for each reply.

## 6. Register (`r`)

### 6.1 Request

```
ESC _ 25a1 ; r ; cp=<hex> ; fmt=glyf ; reply=<0|1|2> ; upm=<int> ; aw=<int> ; lh=<int> ; ymin=<int> ; width=<1|2> ; size=<mode> ; align=<h>,<v> ; pad=<t>,<r>,<b>,<l> ; <base64-payload> ESC \
```

Parameters:

- `cp`: target codepoint in hex. MUST be in the registration block
  defined in §4. Otherwise the request is rejected with
  `reason=out_of_namespace`.
- `fmt`: payload format: `vg`, `glyf`, `colrv0` or `colrv1`.
  Optional; a registration without it is `glyf`, as it was before
  formats were negotiated. A format the terminal doesn't list in `s`
  is rejected with `reason=unsupported_format`. See §8 for each
  format's wire layout.
- `reply`: reply-level control. Optional; default `1`.
  - `reply=0`: the terminal MUST NOT emit any reply for this
    registration (neither success nor failure). Intended for bulk
    fire-and-forget startup registrations that won't be read back.
  - `reply=1`: the terminal emits both success and failure replies
    (the default; equivalent to omitting the parameter).
  - `reply=2`: the terminal emits failure replies only; success
    registrations are silent. Useful for bulk registrations that
    still want to learn about the broken ones without a success
    ACK for every glyph.
  Unknown values fall back to `reply=1`.
- `vb`, `fit`: `fmt=vg` only; see §8.9. `align` takes `vg`'s own
  values there, and the keys from `upm` to `pad` below except
  `width` don't apply to `vg`.
- `upm`: units per em, the coordinate space the outline is
  authored in. Optional; default `1000`.
- `aw`: authored advance width, in upm units. The intended
  horizontal extent of the glyph, NOT the outline's bounding box.
  Optional; default `upm` when the extent is declared (§8.5.1).
- `lh`: authored line height, in upm units. The intended vertical
  extent (descender-to-ascender), NOT the outline's bounding box.
  Optional; default `upm` when the extent is declared (§8.5.1).
- `ymin`: bottom of the authored extent, in upm units: the
  descender, so `0` or negative for most glyphs. Optional; default
  `0` when the extent is declared (§8.5.1).
- `width`: OPTIONAL hint for terminals that can't size the
  character any other way: `1` (one cell) or `2` (two cells),
  default `1`. A glyph is drawn into the cells its character
  occupies (§8.5.2); where the terminal sizes the character itself,
  through an explicit width such as kitty's text sizing protocol
  (OSC 66) or its own rule for a codepoint followed by a space,
  that wins and `width` is ignored. Where it doesn't, `width=2`
  draws the glyph across its own cell plus the following one in
  pixels, while the codepoint's logical cell width stays its system
  `wcwidth`, so cursor advance, wrapping and selection don't change.
  The client MUST leave that following cell blank (emit a space
  after the codepoint) so the overflow lands on empty space.
  Clients that want larger glyphs SHOULD prefer explicit widths
  where the terminal supports them: they belong to each place the
  character is printed, not to the registration.
- `size`: scale policy. One of `height`, `advance`, `contain`,
  `cover`, `stretch`. Optional; default `contain`. See §8.5.
- `align`: placement of the scaled outline within the render span,
  as a comma-separated pair `<h>,<v>`. `<h>` is one of `start`,
  `center`, `end`; `<v>` is one of `start`, `center`, `end`,
  `baseline`. Optional; default `center,center`. See §8.5.
- `pad`: insets from the render span edges, as comma-separated
  fractions `<top>,<right>,<bottom>,<left>` (each `0.0`–`1.0`;
  top/bottom are fractions of cell height, left/right of render
  span width). Optional; default `0,0,0,0`. See §8.5.
- payload: the payload for the declared `fmt`, as the last field
  (§3.2): base64 for the binary formats, at most 32 KiB decoded, or
  the text itself for `vg`, at most 32 KiB.

An invalid value for `upm`, `width`, `aw`, `lh`, `ymin`, `size`,
`align`, or
`pad` rejects the registration with `reason=malformed_payload`.

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
failure produces any output: the registration is fire-and-forget.

Defined error codes:

| Code                    | Meaning |
|-------------------------|---------|
| `out_of_namespace`      | `cp` is not in the registration block. |
| `composite_unsupported` | Payload contains composite glyphs. |
| `hinting_unsupported`   | Payload contains hinting instructions. |
| `unsupported_format`    | `fmt` names a format the terminal does not list in `s` (§3.3). |
| `malformed_payload`     | Payload failed to parse as its format, or a sizing parameter has an invalid value. |
| `payload_too_large`     | Payload exceeds 32 KiB: decoded for the base64 formats, as sent for `vg`. |
| `outline_too_large`     | An outline (or a colour container's outlines in aggregate) exceeds the 64 KiB decoded-size budget (§8.2), or a `vg` payload exceeds one of its limits (§8.9.5). |
| `no_space`              | A memory budget the terminal enforces for registrations, per session or across sessions, is used up (§9). The registration may succeed after others are cleared, or after text using old glyphs is gone. |

When a registration has several problems, the terminal reports the
first in this order, so every terminal gives the same reason: `cp`
(`out_of_namespace`), then `fmt` (`unsupported_format`), then the
placement keys (`width`, and `upm`, `aw`, `lh`, `ymin`, `size`,
`align`, `pad` for the font formats or `vb`, `fit`, `align` for `vg`:
`malformed_payload`), then the payload's size (`payload_too_large`),
then the payload itself. Payload size comes before its content: a
base64 payload too long to decode within 32 KiB is too large whether
or not it is valid base64.

### 6.3 Overwrite

A second `r` on the same `cp` replaces the registration for text
printed after it. Terminals SHOULD bind a registration to the text
when it is printed: a cell keeps the glyph that was registered when
its codepoint was printed, so overwriting or clearing a registration
changes neither text already on screen nor text in the scrollback,
and doesn't force a redraw. Colours that use the foreground (`fg`,
palette index `0xFFFF`) still follow the cell's foreground when it is
drawn. A terminal MAY instead apply a new registration to existing
text.

Clients MUST NOT rely on either behaviour. To change what is
displayed, a client registers again and prints the codepoint again. A
theme change that only affects the foreground needs neither.

Binding at print time is what kitty measured as the better choice.
Applying a registration to existing text means that after every `r`
or `c` any cell on screen or in the scrollback may have changed, so
the terminal redraws the whole window and drops what it has cached for
the scrollback: 0.1–1.3 ms per frame against 0.005–0.016 ms for the
one line that changed, every frame, for a program animating an icon by
re-registering it. Binding costs one lookup per printed block
codepoint, which kitty's benchmarks could not tell apart from printing
any other character. It also keeps programs from changing each other's
output, and makes memory accounting exact: a glyph is in use exactly
as long as some cell refers to it (§9).

There is no eviction: the glossary has a slot for every codepoint in
the registration block (§4), so a registration stays until it is
overwritten or cleared, the terminal is reset (§6.4), or the session
ends.

### 6.4 Lifetime

Registrations live for the duration of the terminal session. A
full terminal reset (RIS, `ESC c`) MUST clear the entire glossary, so
`reset` reliably returns the user to a known state.
Registrations MUST NOT persist across terminal restarts.

A registration can outlive the program that made it. A later
program in the same session that emits a block codepoint without
registering it first gets whatever was registered there last, or
the system font, or tofu. Where the terminal binds at print time
(§6.3), registering never changes what an earlier program printed. Programs MUST register every block
codepoint they emit, rather than relying on what the session
already holds. Because the block holds only application-drawn
glyphs, a leftover registration can only affect other programs
using the block, never text or an installed font's icons.

## 7. Clear (`c`)

### 7.1 Request

```
ESC _ 25a1 ; c [ ; cp=<hex> ] [ ; reply=<0|1|2> ] ESC \
```

If `cp` is omitted, every slot in the session's glossary is
cleared. Otherwise the slot corresponding to `cp` is cleared. Like an
overwrite (§6.3), clearing applies to text printed afterwards where
the terminal binds at print time. `cp`
MUST be in the registration block; otherwise the request is
rejected with `reason=out_of_namespace`. `reply` works as it does
for `r` (§6.1): `0` sends nothing, `2` only failures, `1` (the
default) both.

### 7.2 Response

Success:

```
ESC _ 25a1 ; c ; cp=<hex> ; status=0 ESC \
ESC _ 25a1 ; c ; status=0 ESC \
```

The reply echoes `cp` when the request had one, so a client that
clears several slots can tell the replies apart; clearing everything
replies without `cp`. Clearing an empty slot is a no-op and MUST
return `status=0`.

Failure:

```
ESC _ 25a1 ; c ; cp=<hex> ; status=1 ; reason=out_of_namespace ESC \
```

### 7.3 Cache invalidation

When a slot is cleared (explicitly, via overwrite, or via a
terminal reset), text printed afterwards MUST NOT be drawn from a
rasterization of the old registration. A subsequent `r` that reuses
the codepoint MUST rasterize the new outline fresh, not serve stale
pixels. Where the terminal binds at print time, text printed before
keeps its glyph, and its rasterization stays valid as long as that
text exists.

## 8. Payload formats

A terminal accepts any of the formats below and lists them in `s`
(§3.3); none is required. `vg` (§8.9) is the recommended format:
applications send the SVG path data their icon sets and emoji
already ship, and it needs only a 2D path renderer. `glyf` and the
`COLR` formats suit terminals and applications that already work
with font tables. The placement rules of §8.5 are written for the
font formats; `vg` places its viewBox the same way with its own keys
(§8.9.3), and every format is clipped as §8.5.7 says.

### 8.1 Payload format: `glyf`

Glyph Protocol reuses the OpenType `glyf` table's simple-glyph
record as its wire format. Authoritative references:

- OpenType `glyf` specification (Microsoft Typography).
- Apple TrueType Reference Manual, Chapter 6.

### 8.2 Constraints

Terminals that accept `glyf` MUST accept the following
subset of it and MAY reject anything else with
`reason=composite_unsupported` or `reason=hinting_unsupported`:

- **Simple glyphs only** (in v1). No composite glyphs, no references
  to other glyphs. Composite support may land in a future version
  once the simple-glyph baseline is settled in the field.
- **Standard flag encoding** as defined by the OpenType spec
  (on-curve, off-curve, x-short, y-short, repeat).
- **No hinting instructions** (in v1). The `instructionLength`
  field MUST be zero. Bytecode hinting may be allowed in a future
  version once the unhinted baseline is settled; for now the
  terminal's own anti-aliasing is what users see.
- **Coordinate space** defined by `upm`. The terminal maps this
  space onto its cell at render time.
- **Decoded-size limit.** The wire form is compressed: repeat
  flags and 8-bit or omitted deltas mean one wire byte can stand
  for many decoded points. For limit accounting, one decoded
  point costs **12 bytes** (two expanded coordinates plus an
  on-curve flag, what a typical in-memory point representation
  occupies). An outline whose decoded size would exceed **64
  KiB**, i.e. more than **5,461 points**, MUST be rejected with
  `reason=outline_too_large`. The same budget applies to a
  registration as a whole: the decoded sizes of all outlines
  carried in a `colrv0`/`colrv1` container (§8.6 / §8.7) MUST sum
  to at most 64 KiB. The point count is known from
  `endPtsOfContours` before any point storage exists, so
  terminals MUST validate against the budget before allocating
  decoded-point storage. The limit is a bound on allocation, not
  a post-hoc check.

The 64 KiB budget comes from Mitchell Hashimoto's survey of real
symbol fonts for Ghostty's implementation: across Apple Symbols,
Noto, and the Nerd Fonts collection, the largest glyph decodes to
roughly 40 KiB, so every observed real glyph fits with headroom.
Without the cap, a crafted payload at the wire cap (maximally
repeat-compressed flags, omitted deltas) expands about 12×: a
64 KiB payload, the wire cap before v1.16, to ~768 KiB of decoded
points per monochrome slot, and a colour
container to many times that, turning the 1024-slot glossary
into a memory-exhaustion vector measured in hundreds of MiB per
session.

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

### 8.5 Sizing, placement, and coordinate convention

Every registered outline passes through three transforms at render
time, in order: **pad** (compute the effective render span),
**size** (pick scale factors), **align** (position the scaled
outline within the span).

#### 8.5.1 Coordinate convention

Outlines are authored in `upm`-unit space, Y-up, with `y=0` at the
baseline. Placement is computed from the **authored extent**, a
rectangle in that space:

```
x: 0 .. aw
y: ymin .. ymin + lh
```

The extent is *declared* when the registration sends any of `aw`,
`lh`, or `ymin`; the absent ones take their defaults (`upm`, `upm`,
and `0`). Terminals MUST NOT use the outline's own bounds to place a
glyph whose extent is declared: that is what stops the glyphs of a
set from shifting relative to each other.

When none of the three is sent, the author has said nothing about
metrics and the outline is the only information available, so the
extent is the outline's bounding box, computed from its points (the
bounds stored in the `glyf` header are not trusted). For
`colrv0`/`colrv1` that is
the base glyph's clip box when it has one, and its outline's bounds
otherwise.

Points outside a declared extent are allowed. They are placed by the
same transform and clipped by §8.5.7.

#### 8.5.2 Render span and padding

The render span is the rectangle of pixels the outline is scaled
into: the area the terminal renders that character into. That is,
in order:

1. the cells an explicit width gives the character where the
   terminal supports one, such as kitty's text sizing protocol
   (OSC 66), whose `w` and `s` keys can make it several cells wide
   and tall;
2. otherwise the terminal's own rule for the codepoint, such as
   kitty drawing a Private Use Area codepoint followed by a space
   across two cells;
3. otherwise `width` (§6.1) cells wide and one cell tall:

```
W = width × cell_width_px      (cell_width_px from the terminal)
H = cell_height_px
```

The span is a pixel-space quantity. In the third case `width=2`
widens the pixels the outline scales into, not the codepoint's
logical cell width (§6.1): the cursor still advances one cell and
the glyph overflows rightward into the following cell, which the
client keeps blank. If the glyph is in the grid's last column, or
the following cell is not blank, there is nowhere for the overflow
to land: the terminal MUST render it with a one-cell span.

`pad=<t>,<r>,<b>,<l>` shrinks the span:

```
W' = W × (1 − l − r)
H' = H × (1 − t − b)
```

Padding values are fractions from `0.0` to `1.0`; one outside that
range is an invalid value (§6.1). If `l + r ≥ 1` or `t + b ≥ 1` the
terminal MUST treat the request as if `pad=0,0,0,0` (no padding):
a degenerate span is not useful and suggests a client bug. The
effective span `W' × H'` is what `size` and `align` operate on.

#### 8.5.3 Size modes

Given the authored extent `aw × lh` (the outline's width and height
when the extent is not declared) and effective span `W' × H'`:

| `size`     | `sx`              | `sy`              | Aspect ratio | Notes |
|------------|-------------------|-------------------|--------------|-------|
| `height`   | `H' / lh`         | `H' / lh`         | Preserved    | Line-height drives. Horizontal extent is whatever `aw` scales to; may exceed the span. |
| `advance`  | `W' / aw`         | `W' / aw`         | Preserved    | Advance drives. Vertical extent is whatever `lh` scales to; may exceed the span. |
| `contain`  | `min(W'/aw, H'/lh)` (see below) | same | Preserved    | Default. The extent fits entirely inside the span on both axes. May leave empty space on one axis. |
| `cover`    | `max(W'/aw, H'/lh)` | same            | Preserved    | The extent fills the span on both axes. May exceed it on one axis. |
| `stretch`  | `W' / aw`         | `H' / lh`         | Not preserved | Each axis independent. Useful for box-drawing. |

When the extent is not declared, `contain` is further capped at
`min(W', H') / upm`. The outline's bounds say nothing about how big
the glyph was meant to be, so the em keeps a set in proportion: a
period stays smaller than an `M` instead of growing to fill the cell.

`contain` is the default because a client that says nothing about
sizing should get a whole glyph inside its span. For character-like
glyphs, prefer `size=advance` with `align=center,baseline`: the
advance fills the cell's width and the glyph sits on the text
baseline.

#### 8.5.4 Alignment

After scaling, the authored extent measures `(aw×sx) × (lh×sy)`
and `align` picks where it sits within the effective span.

Horizontal:

- `start`: the extent's left edge aligns with the span's left edge
  (`pad_left`).
- `center`: the extent's horizontal midpoint aligns with the span's
  horizontal midpoint.
- `end`: the extent's right edge aligns with the span's right edge
  (`W − pad_right`).

Vertical (Y-up):

- `start`: the extent's bottom (`y=ymin`) aligns with the span's
  bottom edge (`pad_bottom`).
- `center`: the extent's vertical midpoint aligns with the span's
  vertical midpoint.
- `end`: the extent's top (`y=ymin+lh`) aligns with the span's top
  edge (`H − pad_top`).
- `baseline`: `y=0` aligns with the terminal's text baseline within
  the cell. Preferred for character-like glyphs that must sit on the
  same baseline as surrounding text; descenders extend below the
  baseline naturally.

When `size=stretch`, the scaled extent exactly matches the span on
both axes, so `align` has no visible effect. When `size=cover`, the
scaled extent exceeds the span on one axis; `align` picks which edge
to anchor and therefore which part is clipped. When
`size=contain`/`height`/`advance`, the scaled extent may be smaller
than the span on at least one axis; `align` picks where the empty
space goes.

#### 8.5.5 Resolution independence

Applications MUST NOT assume a particular cell size and MUST NOT
re-register glyphs on font size change. All scaling is computed
at render time from the parameters above and the terminal's
current cell metrics.

#### 8.5.6 Coordinated sets (no scale groups)

There is no `group` parameter. A set of glyphs that must visually
align (spinner frames, progress-bar steps, a multi-glyph logo)
aligns automatically when each glyph declares the same extent (`aw`,
`lh`, `ymin`) and uses the same `size`, `align`, and `pad`, because
placement then does not depend on any glyph's own outline.
Scale-group semantics can be added later if authoring experience
shows they are genuinely needed; for v1 the burden sits with
the author, not the protocol.

#### 8.5.7 Clipping

Terminals MUST clip a registered glyph's pixels to its render span
`W × H` (before padding): the area its character occupies, every
cell of it when an explicit width makes it several cells (§8.5.2).
The application owns the grid and cannot see or repair ink that
lands in a neighbouring cell, so a glyph MUST NOT paint outside its
span. A glyph that needs more room asks for it with an explicit
width where the terminal supports one, or `width=2`.

### 8.6 Payload format: `colrv0`

`fmt=colrv0` carries a layered flat-colour glyph using the
OpenType `COLR` v0 and `CPAL` tables verbatim. The protocol wraps
those tables in a small container that also ships the simple-glyph
outlines each layer references, so a colour glyph is self-
contained: no external font needed.

**Container layout** (all integers big-endian, post-base64-decode):

```
u16     n_glyphs              # 1..=1024
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
codes as `fmt=glyf`. The decoded-size budget of §8.2 applies both
to each carried outline and to the container's outlines in
aggregate: a container whose outlines sum past 64 KiB decoded is
rejected with `reason=outline_too_large`.

### 8.7 Payload format: `colrv1`

`fmt=colrv1` shares the container layout of §8.6 but ships an
OpenType `COLR` v1 table, which adds a full paint graph: linear,
radial, and sweep gradients, affine transforms, clip boxes, and
per-layer compositing modes. `CPAL` remains valid but is optional
(v1 paints may carry sRGBA directly), so `cpal_len = 0` is
permitted and means "the COLR references no palette index."

**Paint types.** Terminals implementing `colrv1` SHOULD support
the full OpenType paint-graph vocabulary for maximum interop. A
conforming subset for low-overhead implementations is:

- Solid (direct sRGBA or palette index).
- Linear gradient.
- Radial gradient.
- Affine transforms on paint subtrees.
- `src-over` layer composite.

A terminal that can't render a paint node in a payload (a sweep
gradient, a blend mode beyond `src-over`, variations) MUST reject the
registration with `reason=malformed_payload` rather than draw a
substitute: the same payload must draw the same in every terminal
that accepts it, so a client can rely on what it registered.

**Foreground inheritance.** CPAL palette index `0xFFFF` and
v1's `PaintSolid` with the foreground sentinel MUST resolve to
the cell's current foreground colour at rasterisation time.
Terminals that cache rasterised colour glyphs MUST re-rasterise
on foreground change for any glyph whose paint graph references
`0xFFFF`.

**Security.** `cp` is still confined to the registration block,
the cell buffer is still authoritative for copy and selection, and
whatever a payload draws stays inside its character's cells. The
`COLR` parser itself is new attack surface, though: a v1 paint graph
can be deeply nested or cyclic through `PaintColrLayers` and
`PaintColrGlyph`, and parsers for it have needed hardening against
fuzzed input. Terminals MUST bound recursion depth and detect cycles,
and SHOULD use a parser that has been fuzzed.

### 8.8 Authoring

Most applications will not hand-author `COLR` bytes either.
Typical flows:

- **From an existing colour font.** Use `fontTools` to extract the
  `COLR`/`CPAL` tables for the glyphs of interest, then pack them
  with the referenced outlines into the container above.
- **From SVG.** The Skia team publishes `nanoemoji` / `maximum-color`,
  which compiles a directory of SVGs into a `COLR` v1 font; feed
  its output into the packer. Where the terminal lists `vg`, sending
  the SVG as `vg` (§8.9) needs no font tooling at all.

### 8.9 Payload format: `vg`

`fmt=vg` carries SVG's path data and paint model as text: layers of
fills, strokes and clips, painted with the foreground, a fixed colour
or a gradient. It is not an SVG document; there is no XML, no styling
and no references. A converter flattens an SVG into it (the example
repository has one).

```
ESC _ 25a1 ; r ; cp=<hex> ; fmt=vg ; vb=<x>,<y>,<w>,<h> [ ; width=<1|2> ] [ ; fit=<mode> ] [ ; align=<h>,<v> ] [ ; reply=<0|1|2> ] ; <layers> ESC \
```

#### 8.9.1 Payload

The payload is the text itself, not base64. It MUST consist of
printable ASCII (0x20–0x7E) only, and so never contains `;`, which
ends the field, or any whitespace but the space. It is at most
32 KiB.

A payload is one to 128 layers separated by `|`, drawn in order. Each
layer is a header, `:`, and SVG 2 path data:

```
fill=<rule>,<colour>[,m <a> <b> <c> <d> <e> <f>]:<path data>
stroke=<width>[/<style>[/<style>]],<colour>[,m <a> <b> <c> <d> <e> <f>]:<path data>
clip=<n>,<rule>[,m <a> <b> <c> <d> <e> <f>]:<path data>
```

- `<rule>` is `nonzero` or `evenodd`, as SVG's `fill-rule`.
- `fill` fills the path with the colour.
- `stroke` strokes it. The width is in the layer's units and MUST be
  greater than zero; through the layer's matrix, it MUST NOT exceed
  the viewBox size the limits use (§8.9.5). The styles that follow
  the width are at most one cap and one join, in either order: caps
  are `butt` and `square`, joins `bevel` and `miter` with an optional
  limit from 1 to 1000 (`miter`, `miter4` and `miter10.5` are valid;
  the default limit is 4), and `round`, the default for both, is
  whichever of the two isn't given. So `stroke=1/miter` is a round cap
  with a miter join, and `stroke=1/round/butt` a butt cap with a round
  join. Caps, joins and the miter limit mean what they mean in SVG.
- `clip` makes the path a clip, using `<rule>`, for the next `n`
  layers (1 to 127). Those layers draw only inside it. They MUST
  exist and MUST NOT be clips themselves: there is one level of
  clipping. A clip path that encloses no area hides what it covers.
- `m <a> <b> <c> <d> <e> <f>`, optional on any layer, is an affine
  matrix from the layer's coordinates to the viewBox, as SVG's
  `matrix(a b c d e f)`. The path, the stroke and any gradient on the
  layer go through it, so a non-uniform scale draws an elliptical pen
  as SVG does. Without it the layer's coordinates are the viewBox's.

A colour is one of:

- `fg`: the cell's foreground colour when the glyph is drawn, as an
  opaque colour. A terminal whose foreground colour carries an alpha
  doesn't apply it here, so a terminal can composite a glyph that
  mixes `fg` with fixed colours as `K + fg·F` at draw time and get the
  same pixels.
- `fg@<alpha>`: the foreground with alpha `<alpha>`, a number from 0
  to 1.
- `#rrggbb` or `#rrggbbaa`: a fixed sRGB colour, hex digits in either
  case, alpha not premultiplied.
- A gradient:

```
lin(<x1> <y1> <x2> <y2> [<spread>] [m <a> <b> <c> <d> <e> <f>] / <offset> <colour> / <offset> <colour> ...)
rad(<fx> <fy> <fr> <cx> <cy> <r> [<spread>] [m <a> <b> <c> <d> <e> <f>] / <offset> <colour> ...)
```

`lin` runs from (`x1`, `y1`) to (`x2`, `y2`). `rad` is a two-point
conical gradient from the focal circle (`fx`, `fy`, `fr`) to the
circle (`cx`, `cy`, `r`), which covers SVG's `radialGradient` focal
point and radius; `fr` and `r` MUST NOT be negative. `<spread>` is
`pad` (default), `reflect` or `repeat`, as SVG's `spreadMethod`. The
optional matrix maps gradient space to the layer's coordinates, as
`gradientTransform`. Colours between stops are interpolated in
non-premultiplied sRGB, as SVG does (CSS and canvas interpolate
premultiplied, which differs for stops of different alpha: halfway
from `#ff000000` to `#0000ffff` is half-transparent purple here, and
half-transparent blue there). A gradient has 2 to 16 stops, each an offset
from 0 to 1, a space, and a colour that is not itself a gradient
(`fg`, `fg@<alpha>`, `#rrggbb` or `#rrggbbaa`). Offsets MUST NOT
decrease. A gradient with no extent (its two points, or two circles,
the same) paints its last stop, as SVG does.

Every matrix, a layer's or a gradient's, MUST stretch or shrink
lengths by at most a factor of 4096: both of its singular values lie
between 1/4096 and 4096. That keeps it invertible and keeps a layer's
own numbers in a range rasterizers handle. Real SVGs need far less;
the emoji sampled for this version use 0.29 to 59.

Numbers in headers, colours and gradients, and the numbers of a
matrix, are written and separated as in path data (§8.9.2). A header
or colour that breaks these rules, a stroke too wide, or a matrix
outside its bounds rejects the registration with
`reason=malformed_payload`; the limits of §8.9.5 use
`reason=outline_too_large`.

#### 8.9.2 Path data

Path data follows the SVG 2 path grammar: every command, absolute and
relative; implicit repetition, a moveto's extra pairs being linetos;
`S` and `T` reflecting the previous control point; drawing after `Z`
starting at the subpath's start. Numbers are an optional sign, then
digits, digits with a fraction, or a fraction alone, then an optional
exponent; a `.` MUST be followed by a digit, so `1.` is not a number.
A number too large for a 32-bit float is a limit (§8.9.5), not a
syntax error: it is rejected with `reason=outline_too_large`, wherever
it appears.
Numbers are separated by spaces, at most one comma, or a sign or `.`
that can't continue the previous number. Arc flags are `0` or `1` and
need no separator. Path data MUST start with a moveto, so empty path
data (`fill=nonzero,fg:`) is malformed, though SVG accepts an empty
`d` as drawing nothing; a layer that draws nothing is written as a
moveto alone.

Arcs follow SVG 2's implementation notes: a zero radius draws a
straight line, identical endpoints draw nothing, and radii too small
for the endpoints are scaled up. Terminals expand an arc into cubic
Béziers, one for each quarter turn or part of one it sweeps (one to
four), each with the standard `4/3·tan(θ/4)` control-point distance,
so that the limits below count the same segments everywhere.

SVG renders path data up to its first error. Here any error,
anywhere in the payload, rejects the whole registration with
`reason=malformed_payload`, since parsers disagree on what partial
data draws and every terminal must draw the same thing. Path data
that only moves or closes is valid and draws nothing.

#### 8.9.3 Placement

The viewBox `vb=<x>,<y>,<w>,<h>` is required: its values are
separated by commas or spaces, `w` and `h` MUST be positive, neither
may be more than 4096 times the other, and no value's magnitude may
exceed 1,000,000. Its y axis points down, as in
SVG. It is the glyph's extent, placed in the render span of §8.5.2
the way SVG's `preserveAspectRatio` places a viewBox in a viewport:

- `fit` is `contain` (default), `cover` or `stretch`, scaling as the
  `size` modes of the same names in §8.5.3 with the viewBox's `w` and
  `h` as `aw` and `lh` (and no cap by the em).
- `align=<h>,<v>` is `start`, `center` or `end` on each axis, default
  `center,center`. Horizontally, `start` is the left; vertically,
  `start` is the top, as SVG's `xMin` and `yMin`. A viewBox has no
  baseline, so `baseline` is an invalid value.

An invalid `vb`, `fit` or `align` rejects the registration with
`reason=malformed_payload`. `width` works as for every format
(§6.1). The font keys, `upm`, `aw`, `lh`, `ymin`, `size` and `pad`,
don't apply and are ignored like any unrecognised key. The glyph is
clipped to its span (§8.5.7).

#### 8.9.4 Rendering

Layers are painted in order with `src-over` compositing, each path
filled or stroked with its colour, through its clip when one covers
it. Path coordinates go through the layer's matrix, then the
placement of §8.9.3.

`fg` and `fg@<alpha>` resolve to the cell's foreground colour when
the glyph is rasterized. Terminals that cache rasterized glyphs MUST
re-rasterize on foreground change for any glyph that uses the
foreground, like `colrv1` (§8.7). A glyph whose every colour,
gradient stops included, is the foreground MAY be kept as a coverage
mask and tinted at draw time, as `glyf` is.

#### 8.9.5 Limits

What makes a glyph slow to draw is how much path crosses the cell,
not its bytes, segments or layers, so the limits bound that. They are
measured in the *viewBox size*: the side `fit` maps onto the cell,
which is the larger of `w` and `h` for `contain` and the smaller for
`cover` and `stretch`. Measured in the larger side, a thin viewBox
under `cover` would let path data that passes every limit be millions
of times the cell's size in pixels; measured this way, the path drawn
is at most 64 times the cell's larger dimension in every mode. A
payload over any of them is rejected with `reason=outline_too_large`:

- Every point, control points included, after the layer's matrix,
  lies within one viewBox size of the viewBox.
- At most 2048 segments across all layers, clip paths included:
  movetos, lines, curves (an arc counted as its cubics, §8.9.2) and
  closes each count one. Drawing after `Z` starts a subpath with an
  implied moveto, which counts; a `Z` right after another closes
  nothing and doesn't.
- At most 64 viewBox sizes of path in total, measured in viewBox
  units after each layer's matrix: the length of every line and
  close, and of every curve's control polygon (which bounds the
  curve), plus, on a stroked layer, the stroke width for every
  segment, moveto included, for the cap or join drawn at its end.
- At most 128 layers and 16 stops per gradient.
- Every number fits a 32-bit float.

These numbers come from icon sets and emoji. Of 584 icons sampled
from nine sets, the largest needs 226 segments and 14 viewBox sizes
of path; of 600 emoji sampled from Twemoji, OpenMoji, Noto and Fluent
Flat, the largest needs 83 layers, 955 segments and 41 sizes. Under
these limits the most expensive path found takes about 1.4 ms to draw
into an 80×160 pixel cell with tiny-skia, and about 1.1 ms with
kitty's own renderer.

The limits bound path, not paint. Every layer covers its pixels once,
so the cost of filling is layers × pixels: 16 layers covering the
whole viewBox, each a radial gradient (the most the length limit
allows), take 1.6 ms at 80×160 in kitty and 60 ms at 7×7 cells. A
terminal SHOULD rasterize a glyph when it is first drawn and cache the
result (Appendix C), and MAY bound the work spent on one glyph, or the
area it rasterizes, drawing the glyph as missing past it. A converter can't save layers by merging
same-colour fills: overlapping shapes drawn in opposite directions
cancel under nonzero winding.

#### 8.9.6 Not included

Patterns, filters, masks, nested clips, dashes, blend modes, text,
and group opacity, which a converter applies per layer. None of these
come up in icon sets; among emoji, those that need blur filters
(Fluent's Color style) are out of reach, as they are for `colrv1`.

## 9. Security considerations

The core property Glyph Protocol must preserve is that **an
application cannot change how existing text, or an icon the user's
font draws, looks**. Enforcement is structural:

- Register accepts a `cp` parameter, but `cp` MUST be in the
  registration block (§4). Any other codepoint is rejected with
  `reason=out_of_namespace`.
- No font in use puts an icon in the block, so the only glyph a
  block codepoint can show is one registered in the session. That
  doesn't mean only cooperating programs emit them: block codepoints
  can appear in file names, file contents or commit messages, and
  `cat` on a file can both register a glyph (the APC is part of the
  file) and print it. Anything printed to the terminal can register
  and use these codepoints, and the protocol is designed for that.
- The rest of the Private Use Areas stays out of reach. A Nerd
  Fonts codepoint such as `U+E0A0` (the git branch icon) is drawn
  by the user's font, in their prompt and in every program, for
  the whole session. If it could be registered, a program could
  redefine it and exit, and the prompt would keep drawing that
  program's glyph after it was gone (§6.4).
- The cell buffer is authoritative. Selection, copy, search,
  hyperlinks, shell history, and any other text extraction MUST
  return the codepoint the application emitted, never the
  rendered glyph.

Without these properties, a program writing to the PTY could
register a glyph for `a` that looks like `o`, or swap the git
branch icon in the user's prompt, and mislead the reader. With
them, the worst anything printed to the terminal can do is draw
something unexpected at a block codepoint, confined to that
character's cells (§8.5.7), which copies and pastes as the Private
Use Area character it is. An untrusted file can already show
arbitrary pictures through image protocols.

Other considerations:

- **Resource bounds.** Three caps bound the glossary: 1024 slots
  per session, 32 KiB of payload per registration, and 64 KiB of
  decoded outline data per registration (§8.2). Worst case, a
  session holds 32 MiB of payload plus 64 MiB of decoded points.
  The decoded cap is load-bearing: the `glyf` wire form is
  compressed, so a crafted payload expands ~12× on decode and the
  payload cap alone bounds nothing that matters. A `vg`
  registration is held parsed, at most 2048 segments, which is the
  same order of memory. Those are per
  session, and a user with many tabs and splits multiplies them, so
  a terminal MAY also enforce budgets of its own, per session or
  across sessions, and reject registrations past them with
  `reason=no_space` (§6.2), rather than being able to hold 1024
  maximal glyphs in every session. A terminal that binds at print
  time (§6.3) knows exactly which glyphs are in use: a replaced or
  cleared registration lives on only while text printed with it does,
  so its budget can count those too and free each one, with its
  rasterizations, when the last cell using it is gone. Without that,
  a program registering and printing new glyphs in a loop would keep
  every old one alive in the scrollback.
- **Font fingerprinting.** `q` with a `cp` outside the block tells a
  program, one codepoint at a time, whether an installed font covers
  it, so a remote program can learn something about the installed
  fonts. That is the same information a program gets by printing a
  codepoint and asking where the cursor went, and terminals MAY
  rate-limit answers (§5.4).
- **Bounded drawing cost.** A glyph is rasterized while a frame is
  being built, so a payload must not be able to make that slow. The
  `vg` limits (§8.9.5) bound the path that crosses the cell, which is
  what drawing time follows, gradient overdraw included.
- **No code execution.** The `glyf` subset defined in §8.2
  excludes hinting instructions, which is the only part of
  TrueType that is executable. `vg` has no scripting, styling or
  references. Glyph Protocol is purely declarative.
- **No filesystem access.** Glyph Protocol messages do not
  reference files and MUST NOT be used to load data from disk.
- **Session isolation.** Glossaries MUST NOT leak between terminal
  tabs, windows, multiplexer panes, or PTY sessions.

## 10. Non-goals (v1)

- **No codepoints outside the registration block.** Registration is
  restricted to `U+100000`–`U+1003FF` (see §4).
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

1. Recognizes APC sequences whose body begins with `25a1;` (§3.1).
2. Implements the `s`, `q`, `r`, and `c` verbs with the semantics
   defined in this specification, and lists every accepted payload
   format by name in the `fmt=` value of the `s` reply. Every format
   is OPTIONAL and `vg` is RECOMMENDED (§8). An `r` whose format the
   terminal doesn't list, including one without `fmt` (`glyf`) when
   it doesn't list `glyf`, is rejected with
   `reason=unsupported_format`.
3. Restricts register/clear `cp` to the registration block
   `U+100000`–`U+1003FF`; rejects anything else with
   `reason=out_of_namespace`.
4. Can hold a registration for every codepoint in the block at
   once, per session.
5. For each format it accepts, accepts it as its section defines
   and enforces its limits: for `glyf` and the outlines in a
   `colrv0`/`colrv1` container, the 64 KiB decoded-size budget, per
   outline and per registration, rejecting violations with
   `reason=outline_too_large` before allocating decoded-point
   storage (§8.2); for `vg`, the limits of §8.9.5, with the same
   reason.
6. Renders registered `glyf` glyphs in the current foreground
   color; renders `colrv0`/`colrv1` glyphs using the COLR paint
   graph, resolving palette index `0xFFFF` to the current
   foreground color; renders `vg` glyphs as §8.9.4 says, resolving
   `fg` to the current foreground color.
7. Scales and positions glyphs according to `upm`, `aw`, `lh`,
   `ymin`, `width`, `size`, `align`, and `pad` as specified in §8.5,
   or for `vg` according to `vb`, `fit`, `align` and `width`
   (§8.9.3), into the area the character occupies (§8.5.2), rendering a
   `width=2` glyph across two cells of pixels where nothing else sizes
   the character while its logical cell width stays its system
   `wcwidth`, and clipping each glyph to its render span (§8.5.7).
8. Enforces the cell-buffer authority invariant in §9: selection,
   copy, and search return the raw codepoint.
9. Ignores unrecognized parameters rather than failing the
   request.
10. Answers `q` requests in order (§5.5), without letting them stall
    its input handling (§5.4). If it lists `grapheme` in the `s`
    reply's `cap`, answers grapheme queries per §5.3; otherwise
    replies to them with `reason=unsupported` (§5.4).
11. Clears the glossary on a full reset (RIS, §6.4).

A client (application) is Glyph Protocol v1 conformant if it:

1. Registers `cp` only from the registration block, and registers
   every block codepoint before emitting it (§6.4).
2. Treats a DA1 reply with no `s` reply before it as "terminal
   does not implement Glyph Protocol", and then sends no other
   Glyph Protocol message (§3.4).
3. Sends only formats the `s` reply lists, each as its section in
   §8 defines it.
4. Handles all `reason=*` error codes without crashing.
5. Leaves the cell after every `width=2` codepoint blank (emits a
   space), so the render-span overflow lands on empty space, when it
   relies on `width` rather than an explicit width (§8.5.2).
6. Sends `q` with `grapheme` only when the `s` reply lists `grapheme` in
   `cap`, and falls back to `cp` otherwise.
7. Registers again and prints again to change what is displayed, and
   relies neither on earlier output changing with a new registration
   nor on it staying the same (§6.3).

## 12. Reference implementation

The reference implementation ships in Rio terminal. The
[glyph-protocol-examples](https://github.com/raphamorim/glyph-protocol-examples)
repository has example applications and the scripts they use to
extract `glyf`, `colrv0`, and `colrv1` payloads from existing fonts,
and to convert SVG icons and emoji into `vg`.

## Appendix A. Worked example: register an icon

```python
import base64, sys
from fontTools.pens.ttGlyphPen import TTGlyphPen

# A stylised outline in glyf coordinate space (upm=1000, Y-up).
pen = TTGlyphPen(None)
# ... draw commands ...
pen.closePath()

payload = base64.b64encode(pen.glyph().compile(None)).decode("ascii")

# The terminal has already answered `s` (§3.4).
# Register at U+100000, the first codepoint of the registration
# block (U+100000-U+1003FF). No font covers the block, so the
# registration is the only source of the rendered glyph.
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

# The terminal has already answered `s` (§3.4).
# Does the user already have Nerd Fonts installed?
q(0xE0A0)
# Expected reply parsed from the PTY:
#   status=system → the user's font has the icon; emit U+E0A0 itself
#   status=       → nothing covers it; register your own outline at a
#                   block codepoint (e.g. U+100000) and emit that
```

With `cap=grapheme` in the `s` reply, a client can ask about an emoji
before using it:

```python
def q_cluster(cps: list[int]) -> None:
    hexes = ",".join(f"{cp:x}" for cp in cps)
    sys.stdout.write(f"\x1b_25a1;q;grapheme={hexes}\x1b\\")
    sys.stdout.flush()

# Family: man, ZWJ, woman, ZWJ, girl
q_cluster([0x1F468, 0x200D, 0x1F469, 0x200D, 0x1F467])
#   status=system → drawn as one joined family
#   status=       → tofu, or three separate people; use a fallback
```

## Appendix C. Implementation notes

These are not normative but reflect lessons from the first
implementations.

**Response draining.** By default `r` and `c` produce an APC reply
on the PTY. Client applications that register at startup and do not
care about the reply have three options, in order of preference:

1. Use `reply=0` on the request (§6.1, §7.1). The terminal emits
   nothing, so there's nothing to drain and nothing to leak. Best
   for bulk startup registrations, and for a `c` on exit.
2. Use `reply=2` to keep failure replies but drop success ACKs.
   Retains debuggability (you still learn about malformed payloads)
   without the success-reply noise of a 100-glyph registration.
3. Let the framework's input reader swallow the reply, which is safe only
   while that reader is alive.

The failure mode to watch for is sending `r` or `c` with `reply=1`
AFTER the framework has torn down its input reader, typically on
exit, at which point the reply arrives in the PTY but nobody reads
it, and the shell that takes over the PTY after the app exits emits
the queued bytes as visible text (`.25a1;c;status=0` or
`.25a1;r;cp=…;status=0`). For exit-time cleanup, send `c` with
`reply=0`, or skip it (registrations expire with the session anyway).

**Register cheaply, rasterize lazily.** Parsing and validating is all
a registration should cost. Rasterize when a glyph is first drawn,
not when it is registered, and cache the result keyed on the
registration, the size of the area it is drawn into, and the
foreground colour when the glyph's colours reference it. A
multi-colour glyph that uses the foreground can't be one alpha mask
tinted at draw time the way text is, so a glyph shown in many
foreground colours makes many cache entries. Drawing cost grows with
the area as well as the outline: with explicit widths a glyph can
cover up to 7×7 cells, many times the pixels of one cell.

**Practical source of `glyf` data.** Most apps will not hand-author
`glyf` bytes. The typical pipeline is:

1. Open a Nerd Font or similar icon TTF with `fontTools`.
2. For each codepoint of interest, pull the glyph record.
3. If composite, flatten via `fontTools.pens.ttGlyphPen.TTGlyphPen`.
4. Strip hinting instructions (`instructionLength := 0`).
5. Compile to bytes; base64-encode; register at a block codepoint
   of the app's choosing.

**Practical source of `vg` data.** Icon sets ship SVG, and most of
it is already `vg` but for the framing: a converter reads the SVG,
applies its transforms (or keeps them as layer matrices), resolves
styles to a fill or stroke per path and gradients to `lin`/`rad`, and
writes one layer per painted path. A path-only icon set such as
Lucide becomes one `stroke` layer per path with the SVG's own `d`
attribute as the path data.

The source codepoint in the font is irrelevant to the protocol:
applications pull outlines from a Nerd Font's codepoints (`U+E0A0`,
`U+F07B`, …) and register them in the block (`U+100000`+), which is
the only place a registration is accepted.

**Atlas cache invalidation.** The cache-invalidation rule in §7.3
applies to overwrite and reset too, not only explicit clear. Every
block codepoint keeps the same slot for the whole session, so an atlas
keyed on the codepoint or its slot alone would serve the previous
outline after a re-registration. Key it on the registration instead:
the one the cell was printed with where the terminal binds at print
time (§6.3), or a per-registration tag (a counter bumped on every
`r`) otherwise. A registration identical to the one it replaces can
reuse it, so re-sending every icon on a theme change costs almost
nothing.

## Appendix D. Change log

| Date       | Version | Notes |
|------------|---------|-------|
| 2026-04-17 | v1      | Initial release. Register accepts a client-picked `cp` restricted to PUA; 256-slot glossary with FIFO eviction; numeric `status` field; ligatures out of scope. |
| 2026-04-19 | v1.1    | Added `s` verb (support advertisement / protocol ping). |
| 2026-04-19 | v1.2    | Added `fmt=colrv0` and `fmt=colrv1` payload formats wrapping OpenType `COLR` / `CPAL` tables with sidecar `glyf` outlines. Both advertised via bits 1 and 2 of the `s` reply's `fmt=` bitfield. |
| 2026-04-19 | v1.3    | Added `reply=0|1|2` parameter to the `r` verb so bulk registrations can suppress success ACKs (`reply=2`) or go fully fire-and-forget (`reply=0`). Default `reply=1` preserves v1.0 behaviour. |
| 2026-04-19 | v1.4    | Raised the glossary capacity from 256 to 1024 simultaneous registrations per session, and raised the `n_glyphs` cap in `fmt=colrv0`/`colrv1` containers from 256 to 1024 outlines. Both bumps quadruple the worst-case memory footprint; the 64 KiB per-payload cap is unchanged. |
| 2026-04-21 | v1.5    | Added `cp=auto` to the `r` verb: the terminal allocates a free PUA codepoint (SHOULD come from PUA-B) and echoes it in the success reply so the client can emit it. Added `reason=auto_unsupported` and `reason=glossary_exhausted` error codes. `cp=auto` forces a success reply regardless of `reply=0|2` because the allocated codepoint is only carried in the reply. |
| 2026-04-23 | v1.6    | Removed `cp=auto` from the `r` verb (introduced in v1.5). Auto-allocation forced a stateful round-trip reply the client depended on to learn its codepoint, which recording tools like `asciinema` and `tee` cannot capture or replay, making `cp=auto` output impossible to reproduce from a transcript. Clients must pick their own PUA codepoint. The `auto_unsupported` and `glossary_exhausted` error codes are withdrawn. |
| 2026-04-23 | v1.7    | Added a sizing and placement model to the `r` verb: `aw` / `lh` (authored extent in upm units), `width` (Unicode/wcwidth width, `1` or `2`, authoritative), `size` (`height`/`advance`/`contain`/`cover`/`stretch`), `align` (`<h>,<v>` positioning after scale, with `v=baseline` for character-like glyphs), and `pad` (fractional insets from the render span). Pinned the coordinate convention: Y-up, `y=0` at baseline, `lh` measured descender-to-ascender (OpenType). Scale groups are intentionally omitted: coordinated sets align via matching parameters and outline geometry. |
| 2026-05-03 | v1.8    | Replaced the `s` reply's `u8` bitfield with a comma-separated list of format names (e.g. `fmt=glyf,colrv0,colrv1`). Names extend without bit-collision worries and stay readable in transcripts; an empty `fmt=` means the terminal advertises no payload formats. Unknown names MUST be ignored by clients, so future formats are forward-compatible. |
| 2026-05-03 | v1.9    | Replaced the `q` reply's `u8` two-bit `status` field with a comma-separated list of coverage names: `status=system`, `status=glossary`, `status=system,glossary`, or empty for "free". Same motivation as v1.8 for the `s` reply. The `r` and `c` replies still use `status=<u8>` for success/failure since that's a closed boolean, not an extensible set. |
| 2026-08-17 | v1.10   | Added a decoded-size budget to §8.2: 64 KiB of decoded outline data (12 bytes per point, i.e. 5,461 points), enforced per outline and per registration before point storage is allocated; violations reject with the new `reason=outline_too_large`. Closes a decode-amplification hole: repeat-compressed flags and omitted deltas let a 64 KiB wire payload expand to ~768 KiB per monochrome slot (~768 MiB per session at 1024 slots), and a colour container amplifies further via its up-to-1024 carried outlines. Requested by Mitchell Hashimoto, who hit the expansion while implementing the protocol in Ghostty; the budget follows his survey of Apple Symbols, Noto, and Nerd Fonts, where the largest real glyph decodes to ~40 KiB. |
| 2026-08-17 | v1.11   | Redefined `width` as a render-level span, matching the reference implementation (rio#1649, rio#1650): the codepoint's logical cell width always stays its system `wcwidth`, because interactive applications (shells, line editors, multiplexers) compute column counts independently and a terminal-only width override desyncs the cursor between terminal and application. A `width=2` glyph overflows into the following cell in pixels, and the client MUST leave that cell blank. v1.7's "authoritative for all layout decisions" wording is withdrawn. |
| 2026-11-08 | v1.12   | Made the authored extent explicit: added `ymin`, the extent's bottom in upm units, because `lh` alone did not say where the extent sits and implementations had to guess. Placement now uses the declared extent and never the outline's bounds; with no `aw`, `lh`, or `ymin`, the extent is the outline's bounding box and `contain` is capped by the em. A client written for v1.11 that sends `lh` for an outline with descenders must add `ymin`, since `ymin` defaults to `0`. Changed the `size` default from `height` to `contain`, so a client that says nothing about sizing gets a whole glyph inside its span. Terminals MUST clip a glyph to its render span (§8.5.7). An invalid sizing value now rejects with `reason=malformed_payload`. A `width=2` glyph with no blank following cell renders with a one-cell span. |
| 2026-11-15 | v1.13   | Restricted registration to one reserved block, `U+100000`–`U+1003FF` (the first 1024 codepoints of Supplementary PUA-B); `r` and `c` reject every other codepoint, including the rest of the Private Use Areas, with `reason=out_of_namespace`. Allowing all of PUA let a program redefine a Nerd Fonts icon such as the git branch glyph and exit, leaving the user's prompt drawing it for the rest of the session. Raised by Kovid Goyal in kitty#10872; Nerd Fonts confirmed it will not use the block (nerd-fonts#2105). The block is the size of the glossary, so FIFO eviction is removed. Clients MUST register every block codepoint they emit, since registrations outlive the program that made them. Clients registering in the Basic PUA, PUA-A, or elsewhere in PUA-B must move into the block. Removed the claim that an `svg2glyf` helper ships with the reference implementation; it was never published. |
| 2026-11-15 | v1.14   | Added the grapheme query `q;grapheme=`: one grapheme cluster as up to 32 codepoints, answered for the cluster as a whole. `status` keeps its meaning as a set of sources but is empty unless the cluster renders correctly in the current mode, so a ZWJ emoji sequence the font does not join, or a cluster the terminal would split across cells, reports empty. Default-ignorable codepoints are never reported as missing. It is optional: the `s` reply gains a `cap` key listing optional capabilities, the first being `grapheme`, and a terminal without it replies `reason=unsupported`. `q` replies can now carry `reason=` (`unsupported`, `malformed_query`) and arrive in request order. Registration stays per codepoint. Raised by Jeff Quast in contour#2019, for tofu detection in blessed and ucs-detect. |
| 2026-11-18 | v1.15   | Added a note to §3.1 on dispatch: to a terminal that dispatches control strings by intermediate and final bytes, as DEC structures them, a message is `APC 2` with the command string starting `5a1;`, and `2` is in the private-use final range; terminals now recognise a message by `25a1;`, not `25a1` alone. Corrected §3: ECMA-48 leaves an APC's content to the program that receives it and does not require a program that doesn't implement a command to ignore it, as the earlier wording claimed. Added §3.4: clients detect support with `s` followed by DA1, so a DA1 reply with no `s` reply before it means unsupported, as with kitty's graphics and keyboard protocols, and MUST NOT send `q`, `r`, or `c` to a terminal that has not answered `s`. GNU screen and tmux give APC their own meaning (the hardstatus line or window title, the pane title), so there the probe shows up as text, but it is never answered and nothing else is sent. Clients that use the multiplexer's passthrough wrap DA1 too, since the multiplexer answers an unwrapped DA1 itself. Clients that register without sending `s` first must add the probe. Wire format unchanged. |
| 2026-11-18 | v1.16   | Review from Kovid Goyal for kitty. `width` is an optional hint: a glyph is drawn into the cells its character occupies (§8.5.2), so an explicit width such as kitty's text sizing protocol (OSC 66), or a terminal's own rule for a codepoint followed by a space, wins over it; clipping is to that area. The grapheme query answers for the cluster printed as plain text; explicit widths are the client's business. `status=system` means as the terminal would currently render it in the regular face, symbol maps included. `q` may answer the new `reason=unknown` instead of stalling input on a font lookup, and terminals may rate-limit it. The payload is the last field of `r`, found by position, and never contains `;` (§3.2). `c` takes `reply=` and its reply echoes `cp`. The payload cap drops from 64 KiB to 32 KiB, and terminals may enforce a budget across sessions with the new `reason=no_space`. A full reset (RIS) MUST clear the glossary. A `colrv1` paint the terminal can't render rejects the registration instead of drawing a substitute, and §8.7 no longer claims the COLR parser adds no attack surface. §9 reasons from anything printed to the terminal being able to register and use block codepoints, and notes font fingerprinting through `q`. Appendix C has notes on lazy rasterization and cache keys. A client that sent payloads over 32 KiB, or relied on a `c` reply without `cp`, must adjust. |
| 2026-11-18 | v1.17   | Added the `vg` payload format (§8.9) and made it the recommended one: SVG path data as text, in layers of fills, strokes and one level of clips, painted with the foreground, a fixed colour, or a linear or two-point conical gradient, with an optional matrix per layer, placed by `vb`, `fit` and `align` like SVG's `preserveAspectRatio`, and bounded by how much path crosses the cell rather than by bytes. It started from Mitchell Hashimoto's and Kovid Goyal's suggestions to accept SVG. Every payload format is now optional, `glyf` included: a terminal lists the ones it accepts in `s`, and rejects an `r` for any other with the new `reason=unsupported_format`. A registration without `fmt` is still `glyf`, so existing clients keep working where `glyf` is listed. Clients send only listed formats. §8 is now "Payload formats", and `glyf`'s scope is §8.1. |
| 2026-11-19 | v1.18   | Review of v1.17 from Kovid Goyal, with measurements from kitty's implementation (kitty#10872). Terminals SHOULD bind a registration to text when it is printed, so overwriting or clearing one changes only text printed afterwards, and MAY instead apply it to existing text; clients MUST NOT rely on either and re-register and print again to change what's shown (§6.3, §7). `vg`'s limits are measured in the viewBox side `fit` maps onto the cell, the smaller one for `cover` and `stretch`, which closes a hole where a thin viewBox let path many times the cell's size through, and a viewBox's sides may differ by at most 4096 times (§8.9.3, §8.9.5). The spec now says paint cost is layers × pixels, which terminals bound by rasterizing lazily and caching and MAY cap. Gradients interpolate in non-premultiplied sRGB, as in SVG; `fg` is the opaque foreground; stroke styles come in either order, so `stroke=1/miter` is valid; empty path data is malformed; a number too large for a 32-bit float is `outline_too_large` wherever it appears. §6.2 gives the order in which a registration's problems are reported. `no_space` budgets may be per session. |
