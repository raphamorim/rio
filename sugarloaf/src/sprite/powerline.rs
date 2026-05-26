// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Powerline + Powerline Extra Symbols (geometric subset):
//! U+E0B0..=U+E0BF, U+E0D2, U+E0D4.
//!
//! We render the geometric glyphs (filled triangles, chevrons, rounded
//! caps, diagonals, trapezoid dividers) and leave the stylized ones
//! (flames, ice, etc.) to the font.

use super::box_drawing;
use super::canvas::Canvas;
use std::f32::consts::SQRT_2;
use tiny_skia::PathBuilder;

pub fn draw(cp: u32, canvas: &mut Canvas) -> bool {
    let w = canvas.width() as f32;
    let h = canvas.height() as f32;
    match cp {
        // Filled separator triangles.
        0xE0B0 => canvas.triangle(0.0, 0.0, w, h / 2.0, 0.0, h),
        0xE0B2 => canvas.triangle(w, 0.0, 0.0, h / 2.0, w, h),
        // Chevron (hollow) separators.
        0xE0B1 => chevron(canvas),
        0xE0B3 => {
            chevron(canvas);
            canvas.flip_horizontal();
        }
        // Rounded caps.
        0xE0B4 => rounded_cap_filled(canvas),
        0xE0B5 => rounded_cap_stroke(canvas),
        0xE0B6 => {
            rounded_cap_filled(canvas);
            canvas.flip_horizontal();
        }
        0xE0B7 => {
            rounded_cap_stroke(canvas);
            canvas.flip_horizontal();
        }
        // Half-cell slant triangles + their hollow diagonals.
        0xE0B8 => canvas.triangle(0.0, 0.0, w, h, 0.0, h),
        0xE0B9 => box_drawing::diagonal_ul_lr(canvas),
        0xE0BA => canvas.triangle(w, 0.0, w, h, 0.0, h),
        0xE0BB => box_drawing::diagonal_ur_ll(canvas),
        0xE0BC => canvas.triangle(0.0, 0.0, w, 0.0, 0.0, h),
        0xE0BD => box_drawing::diagonal_ur_ll(canvas),
        0xE0BE => canvas.triangle(0.0, 0.0, w, 0.0, w, h),
        0xE0BF => box_drawing::diagonal_ul_lr(canvas),
        // Trapezoid dividers.
        0xE0D2 => trapezoids(canvas),
        0xE0D4 => {
            trapezoids(canvas);
            canvas.flip_horizontal();
        }
        _ => return false,
    }
    true
}

fn chevron(canvas: &mut Canvas) {
    let w = canvas.width() as f32;
    let h = canvas.height() as f32;
    let thick = box_drawing::box_thickness(canvas.height()) as f32;
    let mut pb = PathBuilder::new();
    pb.move_to(0.0, 0.0);
    pb.line_to(w, h / 2.0);
    pb.line_to(0.0, h);
    if let Some(path) = pb.finish() {
        canvas.stroke(&path, thick);
    }
}

/// Cubic control-point coefficient for approximating a circular quarter arc.
const ARC_C: f32 = (SQRT_2 - 1.0) * 4.0 / 3.0;

fn rounded_cap_path(canvas: &Canvas, close: bool) -> Option<tiny_skia::Path> {
    let w = canvas.width() as f32;
    let h = canvas.height() as f32;
    let r = w.min(h / 2.0);
    let mut pb = PathBuilder::new();
    pb.move_to(0.0, 0.0);
    pb.cubic_to(r * ARC_C, 0.0, r, r - r * ARC_C, r, r);
    pb.line_to(r, h - r);
    pb.cubic_to(r, h - r + r * ARC_C, r * ARC_C, h, 0.0, h);
    if close {
        pb.close();
    }
    pb.finish()
}

fn rounded_cap_filled(canvas: &mut Canvas) {
    if let Some(path) = rounded_cap_path(canvas, true) {
        canvas.fill_path(&path);
    }
}

fn rounded_cap_stroke(canvas: &mut Canvas) {
    let thick = box_drawing::box_thickness(canvas.height()) as f32;
    if let Some(path) = rounded_cap_path(canvas, false) {
        canvas.stroke(&path, thick);
    }
}

fn trapezoids(canvas: &mut Canvas) {
    let w = canvas.width() as f32;
    let h = canvas.height() as f32;
    let t = box_drawing::box_thickness(canvas.height()) as f32;

    // Top piece.
    let mut pb = PathBuilder::new();
    pb.move_to(0.0, 0.0);
    pb.line_to(w, 0.0);
    pb.line_to(w / 2.0, h / 2.0 - t / 2.0);
    pb.line_to(0.0, h / 2.0 - t / 2.0);
    pb.close();
    if let Some(path) = pb.finish() {
        canvas.fill_path(&path);
    }

    // Bottom piece.
    let mut pb = PathBuilder::new();
    pb.move_to(0.0, h);
    pb.line_to(w, h);
    pb.line_to(w / 2.0, h / 2.0 + t / 2.0);
    pb.line_to(0.0, h / 2.0 + t / 2.0);
    pb.close();
    if let Some(path) = pb.finish() {
        canvas.fill_path(&path);
    }
}
