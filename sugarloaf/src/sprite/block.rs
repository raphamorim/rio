// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Block Elements | U+2580..=U+259F
//! <https://en.wikipedia.org/wiki/Block_Elements>
//!
//! ▀▁▂▃▄▅▆▇█▉▊▋▌▍▎▏ ▐░▒▓▔▕▖▗▘▙▚▛▜▝▞▟
//!
//! Blocks are
//! aligned fractional fills; shades (░▒▓) are uniform-alpha full-cell
//! fills (the grid shader multiplies the coverage by the cell fg, giving
//! the translucent look); quadrants use complementary fraction rounding
//! so the two halves meet on the same pixel.

use super::canvas::Canvas;

// Shade alpha levels.
const LIGHT: u8 = 0x40;
const MEDIUM: u8 = 0x80;
const DARK: u8 = 0xC0;
const ON: u8 = 0xFF;

const ONE_EIGHTH: f64 = 0.125;
const ONE_QUARTER: f64 = 0.25;
const THREE_EIGHTHS: f64 = 0.375;
const HALF: f64 = 0.5;
const FIVE_EIGHTHS: f64 = 0.625;
const THREE_QUARTERS: f64 = 0.75;
const SEVEN_EIGHTHS: f64 = 0.875;

#[derive(Clone, Copy)]
enum HAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy)]
enum VAlign {
    Top,
    Middle,
    Bottom,
}

pub fn draw(cp: u32, canvas: &mut Canvas) -> bool {
    use HAlign::*;
    use VAlign::*;
    match cp {
        0x2580 => block(canvas, Center, Top, 1.0, HALF),
        0x2581 => block(canvas, Center, Bottom, 1.0, ONE_EIGHTH),
        0x2582 => block(canvas, Center, Bottom, 1.0, ONE_QUARTER),
        0x2583 => block(canvas, Center, Bottom, 1.0, THREE_EIGHTHS),
        0x2584 => block(canvas, Center, Bottom, 1.0, HALF),
        0x2585 => block(canvas, Center, Bottom, 1.0, FIVE_EIGHTHS),
        0x2586 => block(canvas, Center, Bottom, 1.0, THREE_QUARTERS),
        0x2587 => block(canvas, Center, Bottom, 1.0, SEVEN_EIGHTHS),
        0x2588 => full_shade(canvas, ON),
        0x2589 => block(canvas, Left, Middle, SEVEN_EIGHTHS, 1.0),
        0x258a => block(canvas, Left, Middle, THREE_QUARTERS, 1.0),
        0x258b => block(canvas, Left, Middle, FIVE_EIGHTHS, 1.0),
        0x258c => block(canvas, Left, Middle, HALF, 1.0),
        0x258d => block(canvas, Left, Middle, THREE_EIGHTHS, 1.0),
        0x258e => block(canvas, Left, Middle, ONE_QUARTER, 1.0),
        0x258f => block(canvas, Left, Middle, ONE_EIGHTH, 1.0),

        0x2590 => block(canvas, Right, Middle, HALF, 1.0),
        0x2591 => full_shade(canvas, LIGHT),
        0x2592 => full_shade(canvas, MEDIUM),
        0x2593 => full_shade(canvas, DARK),
        0x2594 => block(canvas, Center, Top, 1.0, ONE_EIGHTH),
        0x2595 => block(canvas, Right, Middle, ONE_EIGHTH, 1.0),
        0x2596 => quadrant(canvas, false, false, true, false),
        0x2597 => quadrant(canvas, false, false, false, true),
        0x2598 => quadrant(canvas, true, false, false, false),
        0x2599 => quadrant(canvas, true, false, true, true),
        0x259a => quadrant(canvas, true, false, false, true),
        0x259b => quadrant(canvas, true, true, true, false),
        0x259c => quadrant(canvas, true, true, false, true),
        0x259d => quadrant(canvas, false, true, false, false),
        0x259e => quadrant(canvas, false, true, true, false),
        0x259f => quadrant(canvas, false, true, true, true),

        _ => return false,
    }
    true
}

/// Fill an aligned rect covering `width_frac` x `height_frac` of the cell
/// (always fully opaque here; shades go through `full_shade`).
fn block(canvas: &mut Canvas, h: HAlign, v: VAlign, width_frac: f64, height_frac: f64) {
    let cw = canvas.width();
    let ch = canvas.height();
    let w = (cw as f64 * width_frac).round() as u32;
    let hh = (ch as f64 * height_frac).round() as u32;
    let x = match h {
        HAlign::Left => 0,
        HAlign::Right => cw - w,
        HAlign::Center => (cw - w) / 2,
    };
    let y = match v {
        VAlign::Top => 0,
        VAlign::Bottom => ch - hh,
        VAlign::Middle => (ch - hh) / 2,
    };
    canvas.rect(x as i32, y as i32, (x + w) as i32, (y + hh) as i32, ON);
}

/// Fill the whole cell with a shade alpha.
fn full_shade(canvas: &mut Canvas, shade: u8) {
    canvas.rect(0, 0, canvas.width() as i32, canvas.height() as i32, shade);
}

/// Fill any set of the four quadrants.
fn quadrant(canvas: &mut Canvas, tl: bool, tr: bool, bl: bool, br: bool) {
    if tl {
        canvas.fill_fraction(0.0, HALF, 0.0, HALF, ON);
    }
    if tr {
        canvas.fill_fraction(HALF, 1.0, 0.0, HALF, ON);
    }
    if bl {
        canvas.fill_fraction(0.0, HALF, HALF, 1.0, ON);
    }
    if br {
        canvas.fill_fraction(HALF, 1.0, HALF, 1.0, ON);
    }
}
