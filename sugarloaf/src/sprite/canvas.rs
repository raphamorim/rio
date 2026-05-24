// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! A tiny R8 (single-channel alpha) drawing surface for built-in sprite
//! glyphs. Box-drawing and friends are mostly axis-aligned rectangles,
//! which we fill directly into the buffer; the few curved / diagonal
//! glyphs (arcs, diagonals, later powerline curves) are rasterized with
//! anti-aliasing via `tiny_skia` and max-composited back into the R8
//! buffer.
//!
//! It emits
//! the grayscale coverage bytes the grid atlas expects (`RasterizedGlyph`
//! `bytes`, R8 row-major, no stride).

use tiny_skia::{FillRule, LineCap, Paint, Path, PathBuilder, Pixmap, Stroke, Transform};

pub struct Canvas {
    w: u32,
    h: u32,
    /// R8 coverage, row-major, length `w * h`.
    data: Vec<u8>,
}

impl Canvas {
    pub fn new(w: u32, h: u32) -> Self {
        Self {
            w,
            h,
            data: vec![0u8; (w as usize) * (h as usize)],
        }
    }

    #[inline]
    pub fn width(&self) -> u32 {
        self.w
    }

    #[inline]
    pub fn height(&self) -> u32 {
        self.h
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.data
    }

    /// Fill the half-open rect `[x0, x1) x [y0, y1)` with `shade`
    /// (max-blended, so overlapping fills don't darken). Coordinates are
    /// clamped to the canvas.
    pub fn rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, shade: u8) {
        let x0 = x0.clamp(0, self.w as i32);
        let x1 = x1.clamp(0, self.w as i32);
        let y0 = y0.clamp(0, self.h as i32);
        let y1 = y1.clamp(0, self.h as i32);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        for y in y0..y1 {
            let row = (y as usize) * (self.w as usize);
            let s = row + x0 as usize;
            let e = row + x1 as usize;
            for px in &mut self.data[s..e] {
                if shade > *px {
                    *px = shade;
                }
            }
        }
    }

    /// Fill the rect between fractional cell lines (each in `0.0..=1.0`).
    /// `x0`/`y0` use "min" rounding and `x1`/`y1` "max" rounding, which
    /// are complementary so abutting fills share
    /// an exact edge — what lets quadrants / sextants / octants tile.
    pub fn fill_fraction(&mut self, x0: f64, x1: f64, y0: f64, y1: f64, shade: u8) {
        let (w, h) = (self.w, self.h);
        self.rect(
            frac_min(x0, w),
            frac_min(y0, h),
            frac_max(x1, w),
            frac_max(y1, h),
            shade,
        );
    }

    /// Stroke a straight line from `(x0, y0)` to `(x1, y1)` with the
    /// given pixel width, anti-aliased.
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32) {
        let mut pb = PathBuilder::new();
        pb.move_to(x0, y0);
        pb.line_to(x1, y1);
        if let Some(path) = pb.finish() {
            self.stroke(&path, width);
        }
    }

    /// Stroke a path with butt caps and the given width, anti-aliased.
    pub fn stroke(&mut self, path: &Path, width: f32) {
        let Some(mut pixmap) = Pixmap::new(self.w, self.h) else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color_rgba8(255, 255, 255, 255);
        paint.anti_alias = true;
        let stroke = Stroke {
            width,
            line_cap: LineCap::Butt,
            ..Stroke::default()
        };
        pixmap.stroke_path(path, &paint, &stroke, Transform::identity(), None);
        self.composite(&pixmap);
    }

    /// Fill a closed path (non-zero winding), anti-aliased.
    pub fn fill_path(&mut self, path: &Path) {
        let Some(mut pixmap) = Pixmap::new(self.w, self.h) else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color_rgba8(255, 255, 255, 255);
        paint.anti_alias = true;
        pixmap.fill_path(path, &paint, FillRule::Winding, Transform::identity(), None);
        self.composite(&pixmap);
    }

    /// Fill the triangle with vertices `(x0,y0) (x1,y1) (x2,y2)`, anti-aliased.
    pub fn triangle(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, x2: f32, y2: f32) {
        let mut pb = PathBuilder::new();
        pb.move_to(x0, y0);
        pb.line_to(x1, y1);
        pb.line_to(x2, y2);
        pb.close();
        if let Some(path) = pb.finish() {
            self.fill_path(&path);
        }
    }

    /// Set a single pixel's coverage (max-blended). Out-of-bounds is a
    /// no-op. Used by the fading-line branch glyphs.
    pub fn pixel(&mut self, x: i32, y: i32, a: u8) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return;
        }
        let i = (y as u32 * self.w + x as u32) as usize;
        if a > self.data[i] {
            self.data[i] = a;
        }
    }

    /// Mirror the buffer left-to-right (used by the flipped powerline
    /// variants).
    pub fn flip_horizontal(&mut self) {
        let w = self.w as usize;
        for row in self.data.chunks_mut(w) {
            row.reverse();
        }
    }

    /// Max-composite a white-on-transparent `tiny_skia` pixmap's alpha
    /// channel into our R8 buffer. White premultiplied means
    /// `alpha == coverage`, so we just read the alpha byte.
    fn composite(&mut self, pixmap: &Pixmap) {
        for (dst, src) in self.data.iter_mut().zip(pixmap.pixels()) {
            let a = src.alpha();
            if a > *dst {
                *dst = a;
            }
        }
    }
}

/// Cell coordinate for a fraction used as a min (left/top) edge.
#[inline]
fn frac_min(frac: f64, size: u32) -> i32 {
    let s = size as f64;
    (s - ((1.0 - frac) * s).round()) as i32
}

/// Cell coordinate for a fraction used as a max (right/bottom) edge.
#[inline]
fn frac_max(frac: f64, size: u32) -> i32 {
    (frac * size as f64).round() as i32
}
