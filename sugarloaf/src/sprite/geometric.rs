// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Geometric Shapes (corner-triangle subset): filled U+25E2..=U+25E5
//! (◢◣◤◥) and outlined U+25F8..=U+25FA + U+25FF (◸◹◺◿). Only the corner
//! triangles are drawable as sprites; the rest of the block stays with
//! the font.

use super::box_drawing;
use super::canvas::Canvas;
use tiny_skia::PathBuilder;

#[derive(Clone, Copy)]
enum Corner {
    Tl,
    Tr,
    Bl,
    Br,
}

pub fn draw(cp: u32, canvas: &mut Canvas) -> bool {
    match cp {
        0x25E2 => filled(canvas, Corner::Br),
        0x25E3 => filled(canvas, Corner::Bl),
        0x25E4 => filled(canvas, Corner::Tl),
        0x25E5 => filled(canvas, Corner::Tr),
        0x25F8 => outline(canvas, Corner::Tl),
        0x25F9 => outline(canvas, Corner::Tr),
        0x25FA => outline(canvas, Corner::Bl),
        0x25FF => outline(canvas, Corner::Br),
        _ => return false,
    }
    true
}

/// The three triangle vertices for a corner, covering that diagonal half
/// of the cell.
fn verts(canvas: &Canvas, corner: Corner) -> [(f32, f32); 3] {
    let w = canvas.width() as f32;
    let h = canvas.height() as f32;
    match corner {
        Corner::Tl => [(0.0, 0.0), (0.0, h), (w, 0.0)],
        Corner::Tr => [(0.0, 0.0), (w, h), (w, 0.0)],
        Corner::Bl => [(0.0, 0.0), (0.0, h), (w, h)],
        Corner::Br => [(0.0, h), (w, h), (w, 0.0)],
    }
}

fn filled(canvas: &mut Canvas, corner: Corner) {
    let v = verts(canvas, corner);
    canvas.triangle(v[0].0, v[0].1, v[1].0, v[1].1, v[2].0, v[2].1);
}

fn outline(canvas: &mut Canvas, corner: Corner) {
    let v = verts(canvas, corner);
    let thick = box_drawing::box_thickness(canvas.height()) as f32;
    let mut pb = PathBuilder::new();
    pb.move_to(v[0].0, v[0].1);
    pb.line_to(v[1].0, v[1].1);
    pb.line_to(v[2].0, v[2].1);
    pb.close();
    if let Some(path) = pb.finish() {
        canvas.stroke(&path, thick);
    }
}
