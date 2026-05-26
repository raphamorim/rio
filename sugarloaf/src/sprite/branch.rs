// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Branch Drawing Characters | U+F5D0..=U+F60D (Private Use Area).
//!
//! Used to draw git-like commit graphs in the terminal. The set is built
//! from centered lines, rounded arcs, "fading" gradient lines, and a
//! center node — a circle (filled or hollow) with straight stubs out to
//! any of the four cell edges.

use super::box_drawing::{self, Corner};
use super::canvas::Canvas;
use tiny_skia::PathBuilder;

#[derive(Clone, Copy)]
enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Clone, Copy, Default)]
struct Node {
    up: bool,
    right: bool,
    down: bool,
    left: bool,
    filled: bool,
}

pub fn draw(cp: u32, canvas: &mut Canvas) -> bool {
    use Corner::{Bl, Br, Tl, Tr};
    let arc = box_drawing::arc;
    let hline = box_drawing::hline_middle_light;
    let vline = box_drawing::vline_middle_light;

    match cp {
        0xF5D0 => hline(canvas),
        0xF5D1 => vline(canvas),
        0xF5D2 => fading_line(canvas, Edge::Right),
        0xF5D3 => fading_line(canvas, Edge::Left),
        0xF5D4 => fading_line(canvas, Edge::Bottom),
        0xF5D5 => fading_line(canvas, Edge::Top),
        0xF5D6 => arc(canvas, Br),
        0xF5D7 => arc(canvas, Bl),
        0xF5D8 => arc(canvas, Tr),
        0xF5D9 => arc(canvas, Tl),
        0xF5DA => {
            vline(canvas);
            arc(canvas, Tr);
        }
        0xF5DB => {
            vline(canvas);
            arc(canvas, Br);
        }
        0xF5DC => {
            arc(canvas, Tr);
            arc(canvas, Br);
        }
        0xF5DD => {
            vline(canvas);
            arc(canvas, Tl);
        }
        0xF5DE => {
            vline(canvas);
            arc(canvas, Bl);
        }
        0xF5DF => {
            arc(canvas, Tl);
            arc(canvas, Bl);
        }
        0xF5E0 => {
            arc(canvas, Bl);
            hline(canvas);
        }
        0xF5E1 => {
            arc(canvas, Br);
            hline(canvas);
        }
        0xF5E2 => {
            arc(canvas, Br);
            arc(canvas, Bl);
        }
        0xF5E3 => {
            arc(canvas, Tl);
            hline(canvas);
        }
        0xF5E4 => {
            arc(canvas, Tr);
            hline(canvas);
        }
        0xF5E5 => {
            arc(canvas, Tr);
            arc(canvas, Tl);
        }
        0xF5E6 => {
            vline(canvas);
            arc(canvas, Tl);
            arc(canvas, Tr);
        }
        0xF5E7 => {
            vline(canvas);
            arc(canvas, Bl);
            arc(canvas, Br);
        }
        0xF5E8 => {
            hline(canvas);
            arc(canvas, Bl);
            arc(canvas, Tl);
        }
        0xF5E9 => {
            hline(canvas);
            arc(canvas, Tr);
            arc(canvas, Br);
        }
        0xF5EA => {
            vline(canvas);
            arc(canvas, Tl);
            arc(canvas, Br);
        }
        0xF5EB => {
            vline(canvas);
            arc(canvas, Tr);
            arc(canvas, Bl);
        }
        0xF5EC => {
            hline(canvas);
            arc(canvas, Tl);
            arc(canvas, Br);
        }
        0xF5ED => {
            hline(canvas);
            arc(canvas, Tr);
            arc(canvas, Bl);
        }
        0xF5EE => node(
            canvas,
            Node {
                filled: true,
                ..Node::default()
            },
        ),
        0xF5EF => node(canvas, Node::default()),
        0xF5F0 => node(
            canvas,
            Node {
                right: true,
                filled: true,
                ..Node::default()
            },
        ),
        0xF5F1 => node(
            canvas,
            Node {
                right: true,
                ..Node::default()
            },
        ),
        0xF5F2 => node(
            canvas,
            Node {
                left: true,
                filled: true,
                ..Node::default()
            },
        ),
        0xF5F3 => node(
            canvas,
            Node {
                left: true,
                ..Node::default()
            },
        ),
        0xF5F4 => node(
            canvas,
            Node {
                left: true,
                right: true,
                filled: true,
                ..Node::default()
            },
        ),
        0xF5F5 => node(
            canvas,
            Node {
                left: true,
                right: true,
                ..Node::default()
            },
        ),
        0xF5F6 => node(
            canvas,
            Node {
                down: true,
                filled: true,
                ..Node::default()
            },
        ),
        0xF5F7 => node(
            canvas,
            Node {
                down: true,
                ..Node::default()
            },
        ),
        0xF5F8 => node(
            canvas,
            Node {
                up: true,
                filled: true,
                ..Node::default()
            },
        ),
        0xF5F9 => node(
            canvas,
            Node {
                up: true,
                ..Node::default()
            },
        ),
        0xF5FA => node(
            canvas,
            Node {
                up: true,
                down: true,
                filled: true,
                ..Node::default()
            },
        ),
        0xF5FB => node(
            canvas,
            Node {
                up: true,
                down: true,
                ..Node::default()
            },
        ),
        0xF5FC => node(
            canvas,
            Node {
                right: true,
                down: true,
                filled: true,
                ..Node::default()
            },
        ),
        0xF5FD => node(
            canvas,
            Node {
                right: true,
                down: true,
                ..Node::default()
            },
        ),
        0xF5FE => node(
            canvas,
            Node {
                left: true,
                down: true,
                filled: true,
                ..Node::default()
            },
        ),
        0xF5FF => node(
            canvas,
            Node {
                left: true,
                down: true,
                ..Node::default()
            },
        ),
        0xF600 => node(
            canvas,
            Node {
                up: true,
                right: true,
                filled: true,
                ..Node::default()
            },
        ),
        0xF601 => node(
            canvas,
            Node {
                up: true,
                right: true,
                ..Node::default()
            },
        ),
        0xF602 => node(
            canvas,
            Node {
                up: true,
                left: true,
                filled: true,
                ..Node::default()
            },
        ),
        0xF603 => node(
            canvas,
            Node {
                up: true,
                left: true,
                ..Node::default()
            },
        ),
        0xF604 => node(
            canvas,
            Node {
                up: true,
                down: true,
                right: true,
                filled: true,
                ..Node::default()
            },
        ),
        0xF605 => node(
            canvas,
            Node {
                up: true,
                down: true,
                right: true,
                ..Node::default()
            },
        ),
        0xF606 => node(
            canvas,
            Node {
                up: true,
                down: true,
                left: true,
                filled: true,
                ..Node::default()
            },
        ),
        0xF607 => node(
            canvas,
            Node {
                up: true,
                down: true,
                left: true,
                ..Node::default()
            },
        ),
        0xF608 => node(
            canvas,
            Node {
                down: true,
                left: true,
                right: true,
                filled: true,
                ..Node::default()
            },
        ),
        0xF609 => node(
            canvas,
            Node {
                down: true,
                left: true,
                right: true,
                ..Node::default()
            },
        ),
        0xF60A => node(
            canvas,
            Node {
                up: true,
                left: true,
                right: true,
                filled: true,
                ..Node::default()
            },
        ),
        0xF60B => node(
            canvas,
            Node {
                up: true,
                left: true,
                right: true,
                ..Node::default()
            },
        ),
        0xF60C => node(
            canvas,
            Node {
                up: true,
                down: true,
                left: true,
                right: true,
                filled: true,
            },
        ),
        0xF60D => node(
            canvas,
            Node {
                up: true,
                down: true,
                left: true,
                right: true,
                filled: false,
            },
        ),
        _ => return false,
    }
    true
}

/// A centered line whose alpha ramps from 0 to full toward `to`.
fn fading_line(canvas: &mut Canvas, to: Edge) {
    let w = canvas.width();
    let h = canvas.height();
    let thick = box_drawing::box_thickness(h);
    let h_top = h.saturating_sub(thick) / 2;
    let h_bottom = h_top + thick;
    let v_left = w.saturating_sub(thick) / 2;
    let v_right = v_left + thick;

    let (mut color, inc) = match to {
        Edge::Top => (0.0_f64, 255.0 / h as f64),
        Edge::Bottom => (255.0, -255.0 / h as f64),
        Edge::Left => (0.0, 255.0 / w as f64),
        Edge::Right => (255.0, -255.0 / w as f64),
    };

    match to {
        Edge::Top | Edge::Bottom => {
            for y in 0..h {
                let a = color.round().clamp(0.0, 255.0) as u8;
                for x in v_left..v_right {
                    canvas.pixel(x as i32, y as i32, a);
                }
                color += inc;
            }
        }
        Edge::Left | Edge::Right => {
            for x in 0..w {
                let a = color.round().clamp(0.0, 255.0) as u8;
                for y in h_top..h_bottom {
                    canvas.pixel(x as i32, y as i32, a);
                }
                color += inc;
            }
        }
    }
}

/// A center circle (filled or hollow) with straight stubs out to each
/// requested edge. The circle center is offset to align with box-drawing
/// line positions.
fn node(canvas: &mut Canvas, n: Node) {
    let w = canvas.width();
    let h = canvas.height();
    let thick = box_drawing::box_thickness(h);
    let ft = thick as f32;

    let h_top = h.saturating_sub(thick) / 2;
    let h_bottom = h_top + thick;
    let v_left = w.saturating_sub(thick) / 2;
    let v_right = v_left + thick;

    let cx = v_left as f32 + ft / 2.0;
    let cy = h_top as f32 + ft / 2.0;
    let r = cx.min(cy).min((w as f32 - cx).min(h as f32 - cy));

    // Edge stubs run from the cell border up to the circle.
    if n.up {
        canvas.rect(
            v_left as i32,
            0,
            v_right as i32,
            (cy - r + ft / 2.0).ceil() as i32,
            0xFF,
        );
    }
    if n.right {
        canvas.rect(
            (cx + r - ft / 2.0).floor() as i32,
            h_top as i32,
            w as i32,
            h_bottom as i32,
            0xFF,
        );
    }
    if n.down {
        canvas.rect(
            v_left as i32,
            (cy + r - ft / 2.0).floor() as i32,
            v_right as i32,
            h as i32,
            0xFF,
        );
    }
    if n.left {
        canvas.rect(
            0,
            h_top as i32,
            (cx - r + ft / 2.0).ceil() as i32,
            h_bottom as i32,
            0xFF,
        );
    }

    let mut pb = PathBuilder::new();
    if n.filled {
        pb.push_circle(cx, cy, r);
        if let Some(path) = pb.finish() {
            canvas.fill_path(&path);
        }
    } else {
        pb.push_circle(cx, cy, r - ft / 2.0);
        if let Some(path) = pb.finish() {
            canvas.stroke(&path, ft);
        }
    }
}
