// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Braille Patterns | U+2800..=U+28FF
//! <https://en.wikipedia.org/wiki/Braille_Patterns>
//!
//! A 2×4 grid of dots; the codepoint's low byte is a bitmask of which
//! dots are set (Unicode dot order 1,2,3 = left column rows 0–2; 4,5,6 =
//! right column rows 0–2; 7,8 = bottom row left/right).
//!
//! Uses a
//! dynamic dot-sizing pass that distributes leftover pixels across dot
//! size, spacing, and margins so the dots stay crisp and evenly placed
//! at any cell size.

use super::canvas::Canvas;

pub fn draw(cp: u32, canvas: &mut Canvas) -> bool {
    if !(0x2800..=0x28FF).contains(&cp) {
        return false;
    }

    let width = canvas.width() as i32;
    let height = canvas.height() as i32;

    let mut w = (width / 4).min(height / 8);
    let mut x_spacing = width / 4;
    let mut y_spacing = height / 8;
    let mut x_margin = x_spacing / 2;
    let mut y_margin = y_spacing / 2;

    let mut x_px_left = width - 2 * x_margin - x_spacing - 2 * w;
    let mut y_px_left = height - 2 * y_margin - 3 * y_spacing - 4 * w;

    // First, try hard to ensure the dot width is non-zero.
    if x_px_left >= 2 && y_px_left >= 4 && w == 0 {
        w += 1;
        x_px_left -= 2;
        y_px_left -= 4;
    }
    // Second, prefer a non-zero margin.
    if x_px_left >= 2 && x_margin == 0 {
        x_margin = 1;
        x_px_left -= 2;
    }
    if y_px_left >= 2 && y_margin == 0 {
        y_margin = 1;
        y_px_left -= 2;
    }
    // Third, increase spacing.
    if x_px_left >= 1 {
        x_spacing += 1;
        x_px_left -= 1;
    }
    if y_px_left >= 3 {
        y_spacing += 1;
        y_px_left -= 3;
    }
    // Fourth, increase margins.
    if x_px_left >= 2 {
        x_margin += 1;
        x_px_left -= 2;
    }
    if y_px_left >= 2 {
        y_margin += 1;
        y_px_left -= 2;
    }
    // Last, increase dot width again.
    if x_px_left >= 2 && y_px_left >= 4 {
        w += 1;
    }

    let x = [x_margin, x_margin + w + x_spacing];
    let y = [
        y_margin,
        y_margin + w + y_spacing,
        y_margin + 2 * (w + y_spacing),
        y_margin + 3 * (w + y_spacing),
    ];

    let bits = (cp & 0xFF) as u8;
    // (bit mask, column index, row index) for each of the 8 dots.
    let dots = [
        (0x01u8, 0usize, 0usize), // dot 1: top-left
        (0x02, 0, 1),             // dot 2: upper-left
        (0x04, 0, 2),             // dot 3: lower-left
        (0x40, 0, 3),             // dot 7: bottom-left
        (0x08, 1, 0),             // dot 4: top-right
        (0x10, 1, 1),             // dot 5: upper-right
        (0x20, 1, 2),             // dot 6: lower-right
        (0x80, 1, 3),             // dot 8: bottom-right
    ];
    for (mask, col, row) in dots {
        if bits & mask != 0 {
            let cx = x[col];
            let cy = y[row];
            canvas.rect(cx, cy, cx + w, cy + w, 0xFF);
        }
    }
    true
}
