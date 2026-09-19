# Cell borders: experimental Rio protocol

Paint thin, independently colored lines on the edges or center of any terminal cell, including
empty cells. Text keeps its foreground and background; borders supply additional colors without
consuming character positions. This is a prototype, not a registered or interoperable standard.

## Wire format

Like Rio's [Glyph Protocol](glyph-protocol.md), this uses APC (`ESC _ … ESC \`) with an explicit
namespace. It avoids assigning experimental SGR numbers and keeps border painting separate from
text rendition. Spaces shown below separate notation only; actual fields contain no whitespace.

```text
ESC _ rio-border;1;set;MASK;WIDTH;RRGGBB;PLACEMENT;LAYER;ROWS;COLS ESC \
ESC _ rio-border;1;clear;MASK;ROWS;COLS ESC \
ESC _ rio-border;1;q ESC \
```

The query reply is `ESC _ rio-border;1;ok ESC \`. Set and clear have no reply. Clients should read
query replies while their input handler is active and fall back when no reply arrives. The demo
assumes this prototype is running and does not query.

| Field | Values |
| --- | --- |
| `MASK` | Add stroke bits, listed below; valid range `1..1023`. |
| `WIDTH` | Integer `1..32`, in 1/256 of **cell width**; minimum one physical pixel. |
| `RRGGBB` | Exactly six hexadecimal digits; opaque RGB, independent of text colors. |
| `PLACEMENT` | `0`: inside the cell; `1`: centered on the edge, spilling into neighbors. |
| `LAYER` | `0`: below text; `1`: above text. Both are above the cell background. |
| `ROWS`, `COLS` | Decimal `1..65535`; cursor-anchored rectangle, clipped to visible screen. |

Mask bits are top `1`, right `2`, bottom `4`, left `8`, center horizontal `16`, and center vertical
`32`. Four half-centerline arms are horizontal left `64`, horizontal right `128`, vertical top
`256`, and vertical bottom `512`. These run from the cell center to the named edge. Center strokes
stay centered regardless of placement. Width uses cell width for both axes. Half arms overlap
across the central stroke square, so perpendicular arms make a clean corner. Full lines and
half arms are independent slots; avoid selecting both over the same segment unless intentional.
Use mask `1023` to clear all strokes (the original `63` clears only the first six).

Set replaces only the selected strokes on **every cell** in the region; it does not draw only a
rectangle's perimeter. Each stroke retains its own width, color, placement, and layer. Use four
one-cell-thick strips to outline a panel. Clear removes only selected strokes. Neither operation
moves the cursor or changes text. Integer fields are decimal without signs.

For example, add a teal top edge to 20 cells starting at row 4, column 8:

```python
print("\x1b[4;8H\x1b_rio-border;1;set;1;16;80c5b7;0;1;1;20\x1b\\", end="")
```

Width uses one common physical scale, avoiding the unequal thickness of horizontal and vertical
Unicode eighth-blocks. Rounding and the one-pixel minimum mean several small widths can look
identical at ordinary font sizes. Corners are rectangular joins. Center horizontal and vertical
strokes make a cross for outlining quadrants; each can have a different color.

## Lifetime and prototype boundaries

Borders belong to cells and follow cell movement and scrolling. Overwriting or erasing a cell
clears its borders, including when writing a space. Paint text first, then borders. SGR reset
does not clear existing borders. There is no sticky border pen and no extra copied text.

Malformed or unknown commands are ignored atomically. The maximum command body is 256 bytes.
No arbitrary z-index, alpha, rounded corners, dashed lines, or persistence format is defined.
Layer ordering applies within the owning row. For centered strokes spilling across rows, row
render order can override the requested layer. Viewport edges clip overflow. Shared edges do not
merge styles automatically: applications should choose one owner when colors differ. Existing
text/style serialization does not preserve borders in this prototype.

## Try it

Build and launch a prototype window from the repository root:

```sh
cargo build -p rioterm
./target/debug/rio -e python3 misc/scripts/cell-borders.py
```

Alternatively, run inside the prototype Rio build, with at least 88 columns and 42 rows:

```sh
python3 misc/scripts/cell-borders.py
```

Press `b` to compare with and without borders; press `q` or Escape to exit. The demo restores the
original screen and cursor visibility on exit. It includes a dialog inspired by the supplied
screenshot, outlined and underline-only active/inactive fields, quadrant crosses, stroke widths,
placement, and text layers.
Press `p` for the placement detail view: identical buttons of three sizes compare inside and
centered strokes against contrasting cell backgrounds and adjacent text. Press `w` to cycle
stroke widths. Blue is the button's exact cell area; slate is its surroundings. Toggle borders
to inspect that boundary. Use `--placement` to start directly in this view.

Press `c` for button brightness comparisons: dim, medium, and bright borders; three fill
brightnesses; flat, raised, and pressed edges; and inline labels bordered with no padding.
Use `--colors` to start in this view. The placement view also compares unpadded `OK` labels.

Press `h` (or use `--blocks`) for buttons made with quadrant corners (`▗ ▖ ▝ ▘`) and half-block
edges (`▄ ▀ ▐ ▌`). Their borders follow the internal centerlines rather than the full-cell
perimeter. The top-left `▗` corner uses mask `640` (right horizontal arm + bottom vertical arm).
The other corners use `576`, `384`, and `320`. `half_outline(...)` in the demo shows the eight
commands needed; block foreground, label foreground, and border colors remain independent.
This view needs the rebuilt prototype with half-centerline support.

Press `j` (or use `--junctions`) for offset header, content, and status dividers. The shared
horizontal lines sit at the background transitions in `▀` cells. A horizontal line plus the top
arm (`272`) makes an upward T; horizontal plus bottom (`528`) makes a downward T. Dividers above
and below can terminate at different columns without crossing into the neighboring band.

To capture one frame without entering the alternate screen or waiting for input:

```sh
python3 misc/scripts/cell-borders.py --emit > /tmp/cell-borders.ansi
cat /tmp/cell-borders.ansi
```

The Python script also provides small `border(...)` and `outline(...)` helpers for experiments.

## Nearby prior art

[XTerm's rectangular-area controls](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)
include DECCARA for changing character attributes in a region. They provide a useful precedent
for painting existing cells, but do not supply these independent cell-edge strokes.
[Kitty's graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/)
provides image placement and z-order relative to text. This experiment uses much smaller,
cell-owned geometry instead of images. Rio's Glyph Protocol supplies the local APC precedent;
custom glyphs still occupy text positions, whereas these borders do not.

This short comparison is not an exhaustive prior-art survey or a claim of novelty.
