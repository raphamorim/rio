#!/usr/bin/env python3
"""Rio cell-border prototype: python3 misc/scripts/cell-borders.py [--emit]."""

import argparse
import shutil
import sys
import termios
import tty

ESC = "\x1b"
BG, PANEL, TEXT, MUTED = "10151d", "212b36", "d3dae3", "8e9cac"
TEAL, RED, GOLD, BLUE = "80c5b7", "d67b88", "e4be78", "8bade2"
TOP, RIGHT, BOTTOM, LEFT, HORIZONTAL, VERTICAL = 1, 2, 4, 8, 16, 32
H_LEFT, H_RIGHT, V_TOP, V_BOTTOM = 64, 128, 256, 512


def rgb(color):
    return ";".join(str(int(color[i:i + 2], 16)) for i in (0, 2, 4))


def move(row, col):
    return f"{ESC}[{row};{col}H"


def border(row, col, mask, width=16, color=TEAL, placement=0, layer=1,
           rows=1, cols=1):
    """Paint selected strokes on each cell in a cursor-relative rectangle."""
    body = f"rio-border;1;set;{mask};{width};{color};{placement};{layer};{rows};{cols}"
    return move(row, col) + f"{ESC}_{body}{ESC}\\"


def outline(row, col, rows, cols, color=TEAL, width=16, placement=0, layer=1):
    """Four strips make an outer rectangle, without bordering its interior cells."""
    return "".join((
        border(row, col, TOP, width, color, placement, layer, cols=cols),
        border(row + rows - 1, col, BOTTOM, width, color, placement, layer, cols=cols),
        border(row, col, LEFT, width, color, placement, layer, rows=rows),
        border(row, col + cols - 1, RIGHT, width, color, placement, layer, rows=rows),
    ))



def half_outline(row, col, text_width, color=TEAL, width=16):
    """Outline a three-row block shape at the centers of its perimeter cells."""
    right = col + text_width + 1
    return "".join((
        border(row, col, H_RIGHT | V_BOTTOM, width, color),
        border(row, right, H_LEFT | V_BOTTOM, width, color),
        border(row + 2, col, H_RIGHT | V_TOP, width, color),
        border(row + 2, right, H_LEFT | V_TOP, width, color),
        border(row, col + 1, HORIZONTAL, width, color, cols=text_width),
        border(row + 2, col + 1, HORIZONTAL, width, color, cols=text_width),
        border(row + 1, col, VERTICAL, width, color),
        border(row + 1, right, VERTICAL, width, color),
    ))


def scene(enabled=True):
    output = [f"{ESC}[0m{ESC}[48;2;{rgb(BG)}m{ESC}[2J{ESC}[H"]
    borders = []

    def text(row, col, value, fg=TEXT, bg=BG):
        output.append(move(row, col) + f"{ESC}[38;2;{rgb(fg)};48;2;{rgb(bg)}m" + value)

    def fill(row, col, height, width, color):
        for y in range(row, row + height):
            text(y, col, " " * width, bg=color)

    text(1, 3, "RIO / CELL BORDERS", TEAL)
    text(1, 55, "cell edges / centerline segments", MUTED)
    text(3, 3, "Thin lines, full cells. Text keeps its own foreground.", MUTED)
    fill(5, 3, 19, 74, PANEL)
    borders.append(outline(5, 3, 19, 74, BLUE))
    text(6, 5, "Abandon this change?", TEAL, PANEL)
    text(8, 5, "qpvuntsm", "c49be0", PANEL)
    text(8, 16, "Keep focus when closing a preview", bg=PANEL)
    text(10, 5, "Changes to abandon (3 files)", bg=PANEL)
    for row, status, path, delta, color in (
        (12, "D", "README.md", "+0  -3", RED),
        (13, "M", "crates/jk-tui/src/chrome.rs", "+1  -1", GOLD),
        (14, "A", "docs/tui-design.md", "+5  -0", TEAL),
    ):
        text(row, 5, status, color, PANEL)
        text(row, 8, path, bg=PANEL)
        text(row, 64, delta, color, PANEL)
    text(16, 5, "Recover with Undo in the action menu.", MUTED, PANEL)
    for col, width, label, color, background in (
        (5, 18, "View diff", MUTED, "2b3745"),
        (26, 18, "Cancel", TEAL, "263e41"),
        (47, 28, "Abandon change", RED, "422e38"),
    ):
        fill(18, col, 3, width, background)
        text(19, col + (width - len(label)) // 2, label, color, background)
        borders.append(outline(18, col, 3, width, color))
    text(22, 5, "Tab / arrows choose    Enter activate    Esc cancel", MUTED, PANEL)

    text(25, 3, "ACTIVE FIELD", TEAL)
    text(25, 41, "INACTIVE FIELD", MUTED)
    fill(26, 3, 3, 36, PANEL)
    fill(26, 41, 3, 36, PANEL)
    text(27, 5, "Search changes...", TEXT, PANEL)
    text(27, 43, "Filter by author", MUTED, PANEL)
    borders.extend((outline(26, 3, 3, 36, TEAL), outline(26, 41, 3, 36, MUTED)))

    text(30, 3, "ACTIVE / UNDERLINE ONLY", TEAL)
    text(30, 41, "INACTIVE / UNDERLINE ONLY", MUTED)
    fill(31, 3, 3, 36, PANEL)
    fill(31, 41, 3, 36, PANEL)
    text(32, 5, "Search changes...", TEXT, PANEL)
    text(32, 43, "Filter by author", MUTED, PANEL)
    borders.extend((border(33, 3, BOTTOM, color=TEAL, cols=36),
                    border(33, 41, BOTTOM, color=MUTED, cols=36)))

    text(35, 3, "QUADRANTS", MUTED)
    text(35, 20, "WIDTH 4 / 16 / 32", MUTED)
    text(35, 46, "INSIDE / CENTERED", MUTED)
    text(35, 68, "BELOW / ABOVE", MUTED)
    # A colored center cross adds a third color to each two-color block cell.
    text(37, 4, "▚▞▚▞", BLUE, "34453f")
    text(38, 4, "▞▚▞▚", BLUE, "34453f")
    borders.append(border(37, 4, HORIZONTAL | VERTICAL, color=GOLD, rows=2, cols=4))
    borders.append(outline(37, 4, 2, 4, RED))
    for col, width in ((21, 4), (27, 16), (33, 32)):
        fill(37, col, 2, 4, PANEL)
        borders.append(outline(37, col, 2, 4, TEAL, width))
    for col, placement in ((47, 0), (57, 1)):
        fill(37, col, 2, 6, "344151")
        borders.append(outline(37, col, 2, 6, GOLD, 32, placement))
    for col, layer in ((69, 0), (76, 1)):
        text(37, col, "████", BLUE, PANEL)
        borders.append(border(37, col, HORIZONTAL, 32, RED, layer=layer, cols=4))
    text(40, 3, "b: borders  h: blocks  j: joins  p: placement  c: colors  q: quit  " +
         ("BORDERS ON" if enabled else "BORDERS OFF"), MUTED)
    # Painting is deliberately last: subsequent text writes clear cell borders.
    if enabled:
        output.extend(borders)
    output.append(f"{ESC}[0m" + move(41, 1))
    return "".join(output)



def placement_scene(enabled=True, stroke_width=32):
    """Compare placement against fixed cell boundaries at several button sizes."""
    surround, button_bg = "303a48", "305a73"
    output = [f"{ESC}[0m{ESC}[48;2;{rgb(BG)}m{ESC}[2J{ESC}[H"]
    borders = []

    def text(row, col, value, fg=TEXT, bg=BG):
        output.append(move(row, col) + f"{ESC}[38;2;{rgb(fg)};48;2;{rgb(bg)}m" + value)

    def fill(row, col, height, width, color):
        for y in range(row, row + height):
            text(y, col, " " * width, bg=color)

    text(1, 3, "CELL EDGE / PLACEMENT", TEAL)
    text(3, 3, "Same button sizes and stroke widths on both sides.", MUTED)
    for panel_col, placement, title, description in (
        (3, 0, "INSIDE", "Stroke stays on the blue fill."),
        (41, 1, "CENTERED", "Stroke straddles blue and slate."),
    ):
        text(5, panel_col, title, GOLD)
        text(6, panel_col, description, MUTED)
        for row, height, width, label, caption in (
            (10, 1, 2, "OK", "1 row / 2 columns: no padding"),
            (15, 3, 16, "Save", "3 rows / 16 columns: padded"),
            (22, 5, 28, "Apply changes", "5 rows / 28 columns: spacious"),
        ):
            text(row - 2, panel_col, caption, MUTED)
            fill(row - 1, panel_col, height + 2, 36, surround)
            col = panel_col + (36 - width) // 2
            fill(row, col, height, width, button_bg)
            text(row + height // 2, col + (width - len(label)) // 2,
                 label, TEXT, button_bg)
            # Adjacent text and the background transition expose outward spill.
            text(row - 1, col, "above", MUTED, surround)
            text(row + height, col, "below", MUTED, surround)
            text(row + height // 2, col - 2, "L>", MUTED, surround)
            text(row + height // 2, col + width, "<R", MUTED, surround)
            borders.append(outline(row, col, height, width, GOLD,
                                   stroke_width, placement))
    text(29, 3, "Blue = button cells. Slate = neighboring cells. Gold = border.", MUTED)
    text(31, 3, f"w: stroke width {stroke_width}/256 cell width (same on both sides)", GOLD)
    text(32, 3, "Small widths may both round to 1 pixel; zoom the font for a closer look.", MUTED)
    text(33, 3, "Toggle b to reveal the original blue/slate boundary.", MUTED)
    text(35, 3, "b: borders  h: blocks  j: joins  p: overview  c: colors  w: width  q: quit", MUTED)
    if enabled:
        output.extend(borders)
    output.append(f"{ESC}[0m" + move(36, 1))
    return "".join(output)



def brightness_scene(enabled=True):
    """Compare border, fill, and per-edge brightness without changing button size."""
    output = [f"{ESC}[0m{ESC}[48;2;{rgb(BG)}m{ESC}[2J{ESC}[H"]
    borders = []

    def text(row, col, value, fg=TEXT, bg=BG):
        output.append(move(row, col) + f"{ESC}[38;2;{rgb(fg)};48;2;{rgb(bg)}m" + value)

    def button(row, col, label, background):
        for y in range(row, row + 3):
            text(y, col, " " * 22, bg=background)
        text(row + 1, col + (22 - len(label)) // 2, label, bg=background)

    text(1, 3, "BUTTONS / COLOR BRIGHTNESS", TEAL)
    text(3, 3, "Same hue, size, and text color. Compare one change at a time.", MUTED)
    text(7, 3, "BORDER BRIGHTNESS / fixed fill", MUTED)
    text(16, 3, "FILL BRIGHTNESS / fixed border", MUTED)
    text(25, 3, "EDGE BRIGHTNESS / fixed fill", MUTED)
    for col, label, edge, background in (
        (3, "Dim", "3c625b", "182823"),
        (29, "Medium", "679f91", "294438"),
        (55, "Bright", "a4ebd7", "3d6553"),
    ):
        button(9, col, label, PANEL)
        borders.append(outline(9, col, 3, 22, edge))
        button(18, col, label, background)
        borders.append(outline(18, col, 3, 22, TEAL))
    for col, label, top, bottom in (
        (3, "Flat", "679f91", "679f91"),
        (29, "Raised", "a4ebd7", "3c625b"),
        (55, "Pressed", "3c625b", "a4ebd7"),
    ):
        button(27, col, label, PANEL)
        borders.extend((
            border(27, col, TOP, color=top, cols=22),
            border(27, col, LEFT, color=top, rows=3),
            border(29, col, BOTTOM, color=bottom, cols=22),
            border(27, col + 21, RIGHT, color=bottom, rows=3),
        ))
    text(13, 3, "Only the outline changes: subdued, normal, emphasized.", MUTED)
    text(22, 3, "Only the background changes; borders and labels stay constant.", MUTED)
    text(32, 3, "Different edge colors suggest depth without extra characters.", MUTED)
    text(34, 3, "NO PADDING / borders on the text cells themselves", MUTED)
    for col, label, edge in (
        (3, "Dim", "3c625b"),
        (29, "Medium", "679f91"),
        (55, "Bright", "a4ebd7"),
    ):
        text(36, col, "Choose ", MUTED)
        text(36, col + 7, label, TEXT, PANEL)
        text(36, col + 7 + len(label), " now", MUTED)
        borders.append(outline(36, col + 7, 1, len(label), edge))
    text(38, 3, "One row, exactly the label's width. No added spaces or border characters.", MUTED)
    text(40, 3, "b: borders  h: blocks  j: joins  c: overview  p: placement  q: quit", MUTED)
    if enabled:
        output.extend(borders)
    output.append(f"{ESC}[0m" + move(41, 1))
    return "".join(output)



def block_scene(enabled=True, stroke_width=16):
    """Use quadrant and half-block fills, with borders on their internal edges."""
    output = [f"{ESC}[0m{ESC}[48;2;{rgb(BG)}m{ESC}[2J{ESC}[H"]
    borders = []

    def text(row, col, value, fg=TEXT, bg=BG):
        output.append(move(row, col) + f"{ESC}[38;2;{rgb(fg)};48;2;{rgb(bg)}m" + value)

    text(1, 3, "HALF-CELL BUTTONS / CENTERLINE BORDERS", TEAL)
    text(3, 3, "Same text. Right-hand fills stop halfway through their perimeter cells.", MUTED)
    text(5, 3, "FULL-CELL PADDING", MUTED)
    text(5, 41, "HALF-BLOCKS + QUADRANTS", GOLD)
    for row, label, fill_color, edge in (
        (9, "Cancel", "294438", TEAL),
        (16, "Apply changes", "344151", BLUE),
        (23, "Filter by author", PANEL, GOLD),
    ):
        text(row - 2, 3, "Full outline", MUTED)
        text(row - 2, 41, "Centerlines follow the fill", MUTED)
        # Left: one full row above/below and two columns beside the text.
        for y in range(row, row + 3):
            text(y, 5, " " * (len(label) + 4), bg=fill_color)
        text(row + 1, 7, label, bg=fill_color)
        borders.append(outline(row, 5, 3, len(label) + 4, edge, stroke_width))
        # Right: corners fill only the inward quadrant; side cells fill one half.
        # Foreground paints blocks; label foreground and border are independent colors.
        col = 43
        text(row, col, "▗" + "▄" * len(label) + "▖", fill_color)
        text(row + 1, col, "▐", fill_color)
        text(row + 1, col + 1, label, TEXT, fill_color)
        text(row + 1, col + len(label) + 1, "▌", fill_color)
        text(row + 2, col, "▝" + "▀" * len(label) + "▘", fill_color)
        borders.append(half_outline(row, col, len(label), edge, stroke_width))
        text(row + 1, col + len(label) + 3, "adjacent text", MUTED)
    text(29, 3, "▗ ▖ ▝ ▘ shape the corners; ▄ ▀ ▐ ▌ shape the sides.", MUTED)
    text(31, 3, "Borders follow those half-cell edges, not the outer cell rectangle.", MUTED)
    text(32, 3, "Two half-length centerlines join at each corner, without cross-shaped tails.", MUTED)
    text(34, 3, f"w: width {stroke_width}/256    b: hide borders to inspect the block shapes", GOLD)
    text(40, 3, "h: overview    j: junctions    p: placement    c: colors    q / Esc: quit", MUTED)
    if enabled:
        output.extend(borders)
    output.append(f"{ESC}[0m" + move(41, 1))
    return "".join(output)



def junction_scene(enabled=True, stroke_width=16):
    """Offset header, content, and status dividers on shared half-cell boundaries."""
    output = [f"{ESC}[0m{ESC}[48;2;{rgb(BG)}m{ESC}[2J{ESC}[H"]
    borders = []
    status_bg = "263e41"

    def text(row, col, value, fg=TEXT, bg=BG):
        output.append(move(row, col) + f"{ESC}[38;2;{rgb(fg)};48;2;{rgb(bg)}m" + value)

    text(1, 3, "T-JUNCTIONS / OFFSET BAR DIVIDERS", TEAL)
    text(3, 3, "Header, content, and status sections need not share column boundaries.", MUTED)
    text(5, 3, "Each divider stops at a horizontal centerline; none crosses the next band.", MUTED)
    for row in range(8, 11):
        text(row, 3, " " * 74, bg=PANEL)
    # The shared row carries two backgrounds, split at its horizontal center.
    text(11, 3, "▀" * 74, PANEL, BG)
    text(23, 3, "▀" * 74, BG, status_bg)
    for row in range(24, 27):
        text(row, 3, " " * 74, bg=status_bg)
    text(9, 5, "PROJECT / rio", TEAL, PANEL)
    text(9, 27, "BRANCH / prototype", TEXT, PANEL)
    text(9, 53, "VIEW / changes", TEXT, PANEL)
    text(14, 5, "FILES", MUTED)
    text(16, 5, "M  cell-borders.py", TEXT)
    text(18, 5, "M  border_protocol.rs", TEXT)
    text(14, 37, "PREVIEW", MUTED)
    text(16, 37, "Header ends at 24 / 50")
    text(18, 37, "Content at 34 / 60")
    text(14, 63, "DETAILS", MUTED)
    text(16, 63, "3 changes")
    text(18, 63, "Ready", TEAL)
    text(25, 5, "NORMAL", TEAL, status_bg)
    text(25, 23, "3 files / no conflicts", TEXT, status_bg)
    text(25, 57, "Ln 12 / Col 4", TEXT, status_bg)
    borders.append(outline(8, 3, 19, 74, BLUE, stroke_width))
    for row in (11, 23):
        borders.append(border(row, 3, HORIZONTAL, stroke_width, BLUE, cols=74))
    # A full horizontal line + just one vertical arm makes a T, not a cross.
    for col in (24, 50):
        borders.append(border(8, col, VERTICAL, stroke_width, BLUE, rows=3))
        borders.append(border(11, col, HORIZONTAL | V_TOP, stroke_width, BLUE))
    for col in (34, 60):
        borders.append(border(11, col, HORIZONTAL | V_BOTTOM, stroke_width, BLUE))
        borders.append(border(12, col, VERTICAL, stroke_width, BLUE, rows=11))
        borders.append(border(23, col, HORIZONTAL | V_TOP, stroke_width, BLUE))
    for col in (20, 54):
        borders.append(border(23, col, HORIZONTAL | V_BOTTOM, stroke_width, BLUE))
        borders.append(border(24, col, VERTICAL, stroke_width, BLUE, rows=3))
    text(29, 3, "At each shared row: top arms finish one band; bottom arms start another.", MUTED)
    text(31, 3, "Upward T: horizontal + top arm (272). Downward T: horizontal + bottom (528).", MUTED)
    text(33, 3, "The ▀ cells put the background transition exactly under the shared line.", MUTED)
    text(35, 3, f"w: stroke width {stroke_width}/256    b: toggle borders", GOLD)
    text(40, 3, "j: overview    h: blocks    p: placement    c: colors    q: quit", MUTED)
    if enabled:
        output.extend(borders)
    output.append(f"{ESC}[0m" + move(41, 1))
    return "".join(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--emit", action="store_true",
                        help="write one frame; no alternate screen, input, or cleanup")
    views = parser.add_mutually_exclusive_group()
    views.add_argument("--placement", action="store_true",
                        help="start with the detailed inside/centered comparison")
    views.add_argument("--colors", action="store_true",
                       help="start with the button brightness comparison")
    views.add_argument("--blocks", action="store_true",
                       help="show quadrant/half-block shapes with centerline borders")
    views.add_argument("--junctions", action="store_true",
                       help="show offset header/status dividers joined by centerline Ts")
    args = parser.parse_args()
    view = "overview"
    if args.placement:
        view = "placement"
    elif args.colors:
        view = "colors"
    elif args.blocks:
        view = "blocks"
    elif args.junctions:
        view = "junctions"
    if args.emit:
        render = {"overview": scene, "placement": placement_scene,
                  "colors": brightness_scene, "blocks": block_scene,
                  "junctions": junction_scene}
        sys.stdout.write(render[view]())
        return
    if not sys.stdin.isatty() or not sys.stdout.isatty():
        parser.error("interactive mode needs a terminal; use --emit for a capture")
    size = shutil.get_terminal_size()
    if size.columns < 88 or size.lines < 42:
        parser.error("please resize to at least 88 columns by 42 rows (or use --emit)")
    old = termios.tcgetattr(sys.stdin.fileno())
    try:
        tty.setcbreak(sys.stdin.fileno())
        sys.stdout.write(f"{ESC}[?1049h{ESC}[?25l")
        enabled = True
        widths = (4, 16, 32)
        width_index = 2
        while True:
            if view == "placement":
                frame = placement_scene(enabled, widths[width_index])
            elif view == "junctions":
                frame = junction_scene(enabled, widths[width_index])
            elif view == "blocks":
                frame = block_scene(enabled, widths[width_index])
            elif view == "colors":
                frame = brightness_scene(enabled)
            else:
                frame = scene(enabled)
            sys.stdout.write(frame)
            sys.stdout.flush()
            key = sys.stdin.read(1)
            if key in ("q", "Q", ESC, ""):
                break
            if key.lower() == "b":
                enabled = not enabled
            elif key.lower() == "p":
                view = "overview" if view == "placement" else "placement"
            elif key.lower() == "c":
                view = "overview" if view == "colors" else "colors"
            elif key.lower() == "h":
                view = "overview" if view == "blocks" else "blocks"
            elif key.lower() == "j":
                view = "overview" if view == "junctions" else "junctions"
            elif key.lower() == "w":
                width_index = (width_index + 1) % len(widths)
    except KeyboardInterrupt:
        pass
    finally:
        termios.tcsetattr(sys.stdin.fileno(), termios.TCSADRAIN, old)
        sys.stdout.write(f"{ESC}[0m{ESC}[?25h{ESC}[?1049l")
        sys.stdout.flush()


if __name__ == "__main__":
    main()
