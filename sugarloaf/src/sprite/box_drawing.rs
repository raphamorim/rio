// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Box Drawing | U+2500..=U+257F
//! <https://en.wikipedia.org/wiki/Box_Drawing>
//!
//! ─━│┃┄┅┆┇┈┉┊┋┌┍┎┏ ┐┑┒┓└┕┖┗┘┙┚┛├┝┞┟
//! ┠┡┢┣┤┥┦┧┨┩┪┫┬┭┮┯ ┰┱┲┳┴┵┶┷┸┹┺┻┼┽┾┿
//! ╀╁╂╃╄╅╆╇╈╉╊╋╌╍╎╏ ═║╒╓╔╕╖╗╘╙╚╛╜╝╞╟
//! ╠╡╢╣╤╥╦╧╨╩╪╫╬╭╮╯ ╰╱╲╳╴╵╶╷╸╹╺╻╼╽╾╿
//!
//! Positions are computed in `u32` cell space with
//! saturating subtraction so adjacent cells' strokes land on the same
//! pixel rows/columns and join seamlessly.

use super::canvas::Canvas;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Style {
    None,
    Light,
    Heavy,
    Double,
}

const N: Style = Style::None;
const L: Style = Style::Light;
const H: Style = Style::Heavy;
const D: Style = Style::Double;

/// A traditional intersection-style box-drawing glyph: each of the four
/// edges runs from the cell border to the center in some line style.
#[derive(Clone, Copy)]
struct Lines {
    up: Style,
    right: Style,
    down: Style,
    left: Style,
}

#[inline]
const fn ln(up: Style, right: Style, down: Style, left: Style) -> Lines {
    Lines {
        up,
        right,
        down,
        left,
    }
}

#[derive(Clone, Copy)]
pub(super) enum Corner {
    Tl,
    Tr,
    Bl,
    Br,
}

/// Base "light" line thickness in pixels, derived from the cell height
/// so box strokes match Rio's underline thickness (`size_px * 0.075`,
/// and a cell is roughly `size_px * 1.2` tall, hence `cell_h / 16`).
#[inline]
pub(super) fn box_thickness(cell_h: u32) -> u32 {
    ((cell_h as f32 / 16.0).round().max(1.0)) as u32
}

#[inline]
fn light(base: u32) -> u32 {
    base.max(1)
}

#[inline]
fn heavy(base: u32) -> u32 {
    base.saturating_mul(2).max(1)
}

/// Draw the glyph for `cp` into `canvas`. Returns `false` if `cp` is not
/// a box-drawing codepoint we handle, so the caller can fall back to the
/// font.
pub fn draw(cp: u32, canvas: &mut Canvas) -> bool {
    let base = box_thickness(canvas.height());
    let l = light(base);
    let h = heavy(base);

    match cp {
        0x2500 => lc(canvas, ln(N, L, N, L)),
        0x2501 => lc(canvas, ln(N, H, N, H)),
        0x2502 => lc(canvas, ln(L, N, L, N)),
        0x2503 => lc(canvas, ln(H, N, H, N)),
        0x2504 => dash_h(canvas, 3, l, l.max(4)),
        0x2505 => dash_h(canvas, 3, h, l.max(4)),
        0x2506 => dash_v(canvas, 3, l, l.max(4)),
        0x2507 => dash_v(canvas, 3, h, l.max(4)),
        0x2508 => dash_h(canvas, 4, l, l.max(4)),
        0x2509 => dash_h(canvas, 4, h, l.max(4)),
        0x250a => dash_v(canvas, 4, l, l.max(4)),
        0x250b => dash_v(canvas, 4, h, l.max(4)),
        0x250c => lc(canvas, ln(N, L, L, N)),
        0x250d => lc(canvas, ln(N, H, L, N)),
        0x250e => lc(canvas, ln(N, L, H, N)),
        0x250f => lc(canvas, ln(N, H, H, N)),

        0x2510 => lc(canvas, ln(N, N, L, L)),
        0x2511 => lc(canvas, ln(N, N, L, H)),
        0x2512 => lc(canvas, ln(N, N, H, L)),
        0x2513 => lc(canvas, ln(N, N, H, H)),
        0x2514 => lc(canvas, ln(L, L, N, N)),
        0x2515 => lc(canvas, ln(L, H, N, N)),
        0x2516 => lc(canvas, ln(H, L, N, N)),
        0x2517 => lc(canvas, ln(H, H, N, N)),
        0x2518 => lc(canvas, ln(L, N, N, L)),
        0x2519 => lc(canvas, ln(L, N, N, H)),
        0x251a => lc(canvas, ln(H, N, N, L)),
        0x251b => lc(canvas, ln(H, N, N, H)),
        0x251c => lc(canvas, ln(L, L, L, N)),
        0x251d => lc(canvas, ln(L, H, L, N)),
        0x251e => lc(canvas, ln(H, L, L, N)),
        0x251f => lc(canvas, ln(L, L, H, N)),

        0x2520 => lc(canvas, ln(H, L, H, N)),
        0x2521 => lc(canvas, ln(H, H, L, N)),
        0x2522 => lc(canvas, ln(L, H, H, N)),
        0x2523 => lc(canvas, ln(H, H, H, N)),
        0x2524 => lc(canvas, ln(L, N, L, L)),
        0x2525 => lc(canvas, ln(L, N, L, H)),
        0x2526 => lc(canvas, ln(H, N, L, L)),
        0x2527 => lc(canvas, ln(L, N, H, L)),
        0x2528 => lc(canvas, ln(H, N, H, L)),
        0x2529 => lc(canvas, ln(H, N, L, H)),
        0x252a => lc(canvas, ln(L, N, H, H)),
        0x252b => lc(canvas, ln(H, N, H, H)),
        0x252c => lc(canvas, ln(N, L, L, L)),
        0x252d => lc(canvas, ln(N, L, L, H)),
        0x252e => lc(canvas, ln(N, H, L, L)),
        0x252f => lc(canvas, ln(N, H, L, H)),

        0x2530 => lc(canvas, ln(N, L, H, L)),
        0x2531 => lc(canvas, ln(N, L, H, H)),
        0x2532 => lc(canvas, ln(N, H, H, L)),
        0x2533 => lc(canvas, ln(N, H, H, H)),
        0x2534 => lc(canvas, ln(L, L, N, L)),
        0x2535 => lc(canvas, ln(L, L, N, H)),
        0x2536 => lc(canvas, ln(L, H, N, L)),
        0x2537 => lc(canvas, ln(L, H, N, H)),
        0x2538 => lc(canvas, ln(H, L, N, L)),
        0x2539 => lc(canvas, ln(H, L, N, H)),
        0x253a => lc(canvas, ln(H, H, N, L)),
        0x253b => lc(canvas, ln(H, H, N, H)),
        0x253c => lc(canvas, ln(L, L, L, L)),
        0x253d => lc(canvas, ln(L, L, L, H)),
        0x253e => lc(canvas, ln(L, H, L, L)),
        0x253f => lc(canvas, ln(L, H, L, H)),

        0x2540 => lc(canvas, ln(H, L, L, L)),
        0x2541 => lc(canvas, ln(L, L, H, L)),
        0x2542 => lc(canvas, ln(H, L, H, L)),
        0x2543 => lc(canvas, ln(H, L, L, H)),
        0x2544 => lc(canvas, ln(H, H, L, L)),
        0x2545 => lc(canvas, ln(L, L, H, H)),
        0x2546 => lc(canvas, ln(L, H, H, L)),
        0x2547 => lc(canvas, ln(H, H, L, H)),
        0x2548 => lc(canvas, ln(L, H, H, H)),
        0x2549 => lc(canvas, ln(H, L, H, H)),
        0x254a => lc(canvas, ln(H, H, H, L)),
        0x254b => lc(canvas, ln(H, H, H, H)),
        0x254c => dash_h(canvas, 2, l, l),
        0x254d => dash_h(canvas, 2, h, h),
        0x254e => dash_v(canvas, 2, l, h),
        0x254f => dash_v(canvas, 2, h, h),

        0x2550 => lc(canvas, ln(N, D, N, D)),
        0x2551 => lc(canvas, ln(D, N, D, N)),
        0x2552 => lc(canvas, ln(N, D, L, N)),
        0x2553 => lc(canvas, ln(N, L, D, N)),
        0x2554 => lc(canvas, ln(N, D, D, N)),
        0x2555 => lc(canvas, ln(N, N, L, D)),
        0x2556 => lc(canvas, ln(N, N, D, L)),
        0x2557 => lc(canvas, ln(N, N, D, D)),
        0x2558 => lc(canvas, ln(L, D, N, N)),
        0x2559 => lc(canvas, ln(D, L, N, N)),
        0x255a => lc(canvas, ln(D, D, N, N)),
        0x255b => lc(canvas, ln(L, N, N, D)),
        0x255c => lc(canvas, ln(D, N, N, L)),
        0x255d => lc(canvas, ln(D, N, N, D)),
        0x255e => lc(canvas, ln(L, D, L, N)),
        0x255f => lc(canvas, ln(D, L, D, N)),

        0x2560 => lc(canvas, ln(D, D, D, N)),
        0x2561 => lc(canvas, ln(L, N, L, D)),
        0x2562 => lc(canvas, ln(D, N, D, L)),
        0x2563 => lc(canvas, ln(D, N, D, D)),
        0x2564 => lc(canvas, ln(N, D, L, D)),
        0x2565 => lc(canvas, ln(N, L, D, L)),
        0x2566 => lc(canvas, ln(N, D, D, D)),
        0x2567 => lc(canvas, ln(L, D, N, D)),
        0x2568 => lc(canvas, ln(D, L, N, L)),
        0x2569 => lc(canvas, ln(D, D, N, D)),
        0x256a => lc(canvas, ln(L, D, L, D)),
        0x256b => lc(canvas, ln(D, L, D, L)),
        0x256c => lc(canvas, ln(D, D, D, D)),
        0x256d => arc(canvas, Corner::Br),
        0x256e => arc(canvas, Corner::Bl),
        0x256f => arc(canvas, Corner::Tl),

        0x2570 => arc(canvas, Corner::Tr),
        0x2571 => diagonal_ur_ll(canvas),
        0x2572 => diagonal_ul_lr(canvas),
        0x2573 => {
            diagonal_ur_ll(canvas);
            diagonal_ul_lr(canvas);
        }
        0x2574 => lc(canvas, ln(N, N, N, L)),
        0x2575 => lc(canvas, ln(L, N, N, N)),
        0x2576 => lc(canvas, ln(N, L, N, N)),
        0x2577 => lc(canvas, ln(N, N, L, N)),
        0x2578 => lc(canvas, ln(N, N, N, H)),
        0x2579 => lc(canvas, ln(H, N, N, N)),
        0x257a => lc(canvas, ln(N, H, N, N)),
        0x257b => lc(canvas, ln(N, N, H, N)),
        0x257c => lc(canvas, ln(N, H, N, L)),
        0x257d => lc(canvas, ln(L, N, H, N)),
        0x257e => lc(canvas, ln(N, L, N, H)),
        0x257f => lc(canvas, ln(H, N, L, N)),

        _ => return false,
    }
    true
}

/// The join-aware line renderer: each edge's stroke stops at a point that
/// depends on the crossing lines, so corners/tees/crosses meet cleanly.
fn lc(canvas: &mut Canvas, lines: Lines) {
    let cw = canvas.width();
    let ch = canvas.height();
    let base = box_thickness(ch);
    let light_px = light(base);
    let heavy_px = heavy(base);

    // Horizontal stroke band edges (top/bottom y).
    let h_light_top = ch.saturating_sub(light_px) / 2;
    let h_light_bottom = h_light_top + light_px;
    let h_heavy_top = ch.saturating_sub(heavy_px) / 2;
    let h_heavy_bottom = h_heavy_top + heavy_px;
    let h_double_top = h_light_top.saturating_sub(light_px);
    let h_double_bottom = h_light_bottom + light_px;

    // Vertical stroke band edges (left/right x).
    let v_light_left = cw.saturating_sub(light_px) / 2;
    let v_light_right = v_light_left + light_px;
    let v_heavy_left = cw.saturating_sub(heavy_px) / 2;
    let v_heavy_right = v_heavy_left + heavy_px;
    let v_double_left = v_light_left.saturating_sub(light_px);
    let v_double_right = v_light_right + light_px;

    // Where the up stroke stops (its bottom edge), depending on what the
    // crossing lines look like, so corners/tees meet cleanly.
    let up_bottom = if lines.left == H || lines.right == H {
        h_heavy_bottom
    } else if lines.left != lines.right || lines.down == lines.up {
        if lines.left == D || lines.right == D {
            h_double_bottom
        } else {
            h_light_bottom
        }
    } else if lines.left == N && lines.right == N {
        h_light_bottom
    } else {
        h_light_top
    };

    let down_top = if lines.left == H || lines.right == H {
        h_heavy_top
    } else if lines.left != lines.right || lines.up == lines.down {
        if lines.left == D || lines.right == D {
            h_double_top
        } else {
            h_light_top
        }
    } else if lines.left == N && lines.right == N {
        h_light_top
    } else {
        h_light_bottom
    };

    let left_right = if lines.up == H || lines.down == H {
        v_heavy_right
    } else if lines.up != lines.down || lines.left == lines.right {
        if lines.up == D || lines.down == D {
            v_double_right
        } else {
            v_light_right
        }
    } else if lines.up == N && lines.down == N {
        v_light_right
    } else {
        v_light_left
    };

    let right_left = if lines.up == H || lines.down == H {
        v_heavy_left
    } else if lines.up != lines.down || lines.right == lines.left {
        if lines.up == D || lines.down == D {
            v_double_left
        } else {
            v_light_left
        }
    } else if lines.up == N && lines.down == N {
        v_light_left
    } else {
        v_light_right
    };

    let i = |v: u32| v as i32;

    match lines.up {
        N => {}
        L => canvas.rect(i(v_light_left), 0, i(v_light_right), i(up_bottom), 0xFF),
        H => canvas.rect(i(v_heavy_left), 0, i(v_heavy_right), i(up_bottom), 0xFF),
        D => {
            let left_bottom = if lines.left == D {
                h_light_top
            } else {
                up_bottom
            };
            let right_bottom = if lines.right == D {
                h_light_top
            } else {
                up_bottom
            };
            canvas.rect(i(v_double_left), 0, i(v_light_left), i(left_bottom), 0xFF);
            canvas.rect(
                i(v_light_right),
                0,
                i(v_double_right),
                i(right_bottom),
                0xFF,
            );
        }
    }

    match lines.right {
        N => {}
        L => canvas.rect(
            i(right_left),
            i(h_light_top),
            i(cw),
            i(h_light_bottom),
            0xFF,
        ),
        H => canvas.rect(
            i(right_left),
            i(h_heavy_top),
            i(cw),
            i(h_heavy_bottom),
            0xFF,
        ),
        D => {
            let top_left = if lines.up == D {
                v_light_right
            } else {
                right_left
            };
            let bottom_left = if lines.down == D {
                v_light_right
            } else {
                right_left
            };
            canvas.rect(i(top_left), i(h_double_top), i(cw), i(h_light_top), 0xFF);
            canvas.rect(
                i(bottom_left),
                i(h_light_bottom),
                i(cw),
                i(h_double_bottom),
                0xFF,
            );
        }
    }

    match lines.down {
        N => {}
        L => canvas.rect(i(v_light_left), i(down_top), i(v_light_right), i(ch), 0xFF),
        H => canvas.rect(i(v_heavy_left), i(down_top), i(v_heavy_right), i(ch), 0xFF),
        D => {
            let left_top = if lines.left == D {
                h_light_bottom
            } else {
                down_top
            };
            let right_top = if lines.right == D {
                h_light_bottom
            } else {
                down_top
            };
            canvas.rect(i(v_double_left), i(left_top), i(v_light_left), i(ch), 0xFF);
            canvas.rect(
                i(v_light_right),
                i(right_top),
                i(v_double_right),
                i(ch),
                0xFF,
            );
        }
    }

    match lines.left {
        N => {}
        L => canvas.rect(0, i(h_light_top), i(left_right), i(h_light_bottom), 0xFF),
        H => canvas.rect(0, i(h_heavy_top), i(left_right), i(h_heavy_bottom), 0xFF),
        D => {
            let top_right = if lines.up == D {
                v_light_left
            } else {
                left_right
            };
            let bottom_right = if lines.down == D {
                v_light_left
            } else {
                left_right
            };
            canvas.rect(0, i(h_double_top), i(top_right), i(h_light_top), 0xFF);
            canvas.rect(
                0,
                i(h_light_bottom),
                i(bottom_right),
                i(h_double_bottom),
                0xFF,
            );
        }
    }
}

/// Centered horizontal light line (dash fallback when the cell is too
/// small to dash).
pub(super) fn hline_middle_light(canvas: &mut Canvas) {
    let ch = canvas.height();
    let l = light(box_thickness(ch));
    let top = ch.saturating_sub(l) / 2;
    canvas.rect(0, top as i32, canvas.width() as i32, (top + l) as i32, 0xFF);
}

pub(super) fn vline_middle_light(canvas: &mut Canvas) {
    let cw = canvas.width();
    let l = light(box_thickness(canvas.height()));
    let left = cw.saturating_sub(l) / 2;
    canvas.rect(
        left as i32,
        0,
        (left + l) as i32,
        canvas.height() as i32,
        0xFF,
    );
}

/// Dashed horizontal line. Half gaps on each side so a
/// tiled row of dashes stays evenly spaced.
fn dash_h(canvas: &mut Canvas, count: i32, thick_px: u32, desired_gap: u32) {
    let cw = canvas.width() as i32;
    let ch = canvas.height();
    let gap_count = count;
    if cw < count + gap_count {
        hline_middle_light(canvas);
        return;
    }
    let gap_width = desired_gap.min(canvas.width() / (2 * count as u32)) as i32;
    let total_gap_width = gap_count * gap_width;
    let total_dash_width = cw - total_gap_width;
    let dash_width = total_dash_width / count;
    let mut remaining = total_dash_width % count;

    let y = (ch.saturating_sub(thick_px) / 2) as i32;
    let mut x = gap_width / 2;
    for _ in 0..count {
        let mut x1 = x + dash_width;
        if remaining > 0 {
            remaining -= 1;
            x1 += 1;
        }
        canvas.rect(x, y, x1, y + thick_px as i32, 0xFF);
        x = x1 + gap_width;
    }
}

/// Dashed vertical line. One full gap at the bottom so
/// stacked dashes tile cleanly.
fn dash_v(canvas: &mut Canvas, count: i32, thick_px: u32, desired_gap: u32) {
    let cw = canvas.width();
    let ch = canvas.height() as i32;
    let gap_count = count;
    if ch < count + gap_count {
        vline_middle_light(canvas);
        return;
    }
    let gap_height = desired_gap.min(canvas.height() / (2 * count as u32)) as i32;
    let total_gap_height = gap_count * gap_height;
    let total_dash_height = ch - total_gap_height;
    let dash_height = total_dash_height / count;
    let mut remaining = total_dash_height % count;

    let x = (cw.saturating_sub(thick_px) / 2) as i32;
    let mut y = 0;
    for _ in 0..count {
        let mut y1 = y + dash_height;
        if remaining > 0 {
            remaining -= 1;
            y1 += 1;
        }
        canvas.rect(x, y, x + thick_px as i32, y1, 0xFF);
        y = y1 + gap_height;
    }
}

/// Rounded corner (light thickness only,
/// which is all U+256D..2570 need).
pub(super) fn arc(canvas: &mut Canvas, corner: Corner) {
    use tiny_skia::PathBuilder;

    let cw = canvas.width();
    let ch = canvas.height();
    let thick = light(box_thickness(ch));
    let fw = cw as f32;
    let fh = ch as f32;
    let ft = thick as f32;
    let center_x = (cw.saturating_sub(thick) / 2) as f32 + ft / 2.0;
    let center_y = (ch.saturating_sub(thick) / 2) as f32 + ft / 2.0;
    let r = fw.min(fh) / 2.0;
    // Fraction away from the center to place the cubic control points.
    let s = 0.25_f32;

    let mut pb = PathBuilder::new();
    match corner {
        Corner::Tl => {
            pb.move_to(center_x, 0.0);
            pb.line_to(center_x, center_y - r);
            pb.cubic_to(
                center_x,
                center_y - s * r,
                center_x - s * r,
                center_y,
                center_x - r,
                center_y,
            );
            pb.line_to(0.0, center_y);
        }
        Corner::Tr => {
            pb.move_to(center_x, 0.0);
            pb.line_to(center_x, center_y - r);
            pb.cubic_to(
                center_x,
                center_y - s * r,
                center_x + s * r,
                center_y,
                center_x + r,
                center_y,
            );
            pb.line_to(fw, center_y);
        }
        Corner::Bl => {
            pb.move_to(center_x, fh);
            pb.line_to(center_x, center_y + r);
            pb.cubic_to(
                center_x,
                center_y + s * r,
                center_x - s * r,
                center_y,
                center_x - r,
                center_y,
            );
            pb.line_to(0.0, center_y);
        }
        Corner::Br => {
            pb.move_to(center_x, fh);
            pb.line_to(center_x, center_y + r);
            pb.cubic_to(
                center_x,
                center_y + s * r,
                center_x + s * r,
                center_y,
                center_x + r,
                center_y,
            );
            pb.line_to(fw, center_y);
        }
    }
    if let Some(path) = pb.finish() {
        canvas.stroke(&path, ft);
    }
}

/// '╱' upper-right to lower-left. Reused by powerline
/// diagonals (U+E0B9/BB/BD/BF).
pub(super) fn diagonal_ur_ll(canvas: &mut Canvas) {
    let fw = canvas.width() as f32;
    let fh = canvas.height() as f32;
    let slope_x = (fw / fh).min(1.0);
    let slope_y = (fh / fw).min(1.0);
    let thick = light(box_thickness(canvas.height())) as f32;
    canvas.line(
        fw + 0.5 * slope_x,
        -0.5 * slope_y,
        -0.5 * slope_x,
        fh + 0.5 * slope_y,
        thick,
    );
}

/// '╲' upper-left to lower-right. Reused by powerline diagonals.
pub(super) fn diagonal_ul_lr(canvas: &mut Canvas) {
    let fw = canvas.width() as f32;
    let fh = canvas.height() as f32;
    let slope_x = (fw / fh).min(1.0);
    let slope_y = (fh / fw).min(1.0);
    let thick = light(box_thickness(canvas.height())) as f32;
    canvas.line(
        -0.5 * slope_x,
        -0.5 * slope_y,
        fw + 0.5 * slope_x,
        fh + 0.5 * slope_y,
        thick,
    );
}
