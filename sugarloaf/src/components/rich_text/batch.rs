// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// batch.rs was originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE
//
// Eventually the file had updates to support other features like background-color,
// text color, underline color and etc.

use crate::contains_braille_dot;
use crate::DrawableChar;
use crate::UnderlineShape;
use bytemuck::{Pod, Zeroable};

#[derive(Default, Clone, Copy)]
pub struct RunUnderline {
    pub enabled: bool,
    pub offset: f32,
    pub size: f32,
    pub color: [f32; 4],
    pub is_doubled: bool,
    pub shape: UnderlineShape,
}

/// Batch geometry vertex.
#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub color: [f32; 4],
    pub uv: [f32; 2],
    pub layers: [i32; 2],
}

/// Rectangle with floating point coordinates.
#[derive(Copy, Clone, Default, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    /// Creates a new rectangle.
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Rect {
            x,
            y,
            width,
            height,
        }
    }
}

impl From<[f32; 4]> for Rect {
    fn from(v: [f32; 4]) -> Self {
        Self::new(v[0], v[1], v[2], v[3])
    }
}

#[derive(Default, Debug)]
struct Batch {
    image: Option<i32>,
    mask: Option<i32>,
    vertices: Vec<Vertex>,
    subpix: bool,
}

impl Batch {
    fn clear(&mut self) {
        self.image = None;
        self.mask = None;
        self.vertices.clear();
        self.subpix = false;
    }

    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn add_line(
        &mut self,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        width: f32,
        depth: f32,
        color: [f32; 4],
        image: Option<i32>,
        mask: Option<i32>,
        subpix: bool,
    ) -> bool {
        if !self.vertices.is_empty() && subpix != self.subpix {
            return false;
        }
        let has_image = image.is_some();
        let has_mask = mask.is_some();
        if has_image && self.image.is_some() && self.image != image {
            return false;
        }
        if has_mask && self.mask.is_some() && self.mask != mask {
            return false;
        }
        self.subpix = subpix;
        self.image = image;
        self.mask = mask;
        let layers = [self.image.unwrap_or(0), self.mask.unwrap_or(0)];

        // Calculate the direction vector of the line
        let dx = x2 - x1;
        let dy = y2 - y1;

        // Calculate the length of the line
        let line_length = (dx * dx + dy * dy).sqrt();

        // If the line has no length, don't draw anything
        if line_length < 0.001 {
            return true;
        }

        // Calculate the normalized perpendicular vector
        let nx = -dy / line_length; // Perpendicular vector x component
        let ny = dx / line_length; // Perpendicular vector y component

        // Calculate half width
        let half_width = width / 2.0;

        // Calculate the corners of the quad representing the line
        let x1_top = x1 + nx * half_width;
        let y1_top = y1 + ny * half_width;
        let x1_bottom = x1 - nx * half_width;
        let y1_bottom = y1 - ny * half_width;

        let x2_top = x2 + nx * half_width;
        let y2_top = y2 + ny * half_width;
        let x2_bottom = x2 - nx * half_width;
        let y2_bottom = y2 - ny * half_width;

        // Create vertices for the quad
        let v0 = Vertex {
            pos: [x1_top, y1_top, depth],
            color,
            uv: [0.0, 0.0],
            layers,
        };
        let v1 = Vertex {
            pos: [x2_top, y2_top, depth],
            color,
            uv: [1.0, 0.0],
            layers,
        };
        let v2 = Vertex {
            pos: [x2_bottom, y2_bottom, depth],
            color,
            uv: [1.0, 1.0],
            layers,
        };
        let v3 = Vertex {
            pos: [x1_bottom, y1_bottom, depth],
            color,
            uv: [0.0, 1.0],
            layers,
        };

        // Add vertices directly in drawing order (two triangles)
        // First triangle: v0, v1, v2
        self.vertices.push(v0);
        self.vertices.push(v1);
        self.vertices.push(v2);

        // Second triangle: v2, v3, v0
        self.vertices.push(v2);
        self.vertices.push(v3);
        self.vertices.push(v0);

        true
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn add_triangle(
        &mut self,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x3: f32,
        y3: f32,
        color: [f32; 4],
        depth: f32,
        image: Option<i32>,
        mask: Option<i32>,
        subpix: bool,
    ) -> bool {
        if !self.vertices.is_empty() && subpix != self.subpix {
            return false;
        }
        let has_image = image.is_some();
        let has_mask = mask.is_some();
        if has_image && self.image.is_some() && self.image != image {
            return false;
        }
        if has_mask && self.mask.is_some() && self.mask != mask {
            return false;
        }

        self.subpix = subpix;
        self.image = image;
        self.mask = mask;
        let layers = [self.image.unwrap_or(0), self.mask.unwrap_or(0)];

        // Create vertices for the triangle and add them directly in drawing order
        self.vertices.push(Vertex {
            pos: [x1, y1, depth],
            color,
            uv: [0.0, 0.0],
            layers,
        });
        self.vertices.push(Vertex {
            pos: [x2, y2, depth],
            color,
            uv: [1.0, 0.0],
            layers,
        });
        self.vertices.push(Vertex {
            pos: [x3, y3, depth],
            color,
            uv: [0.0, 1.0],
            layers,
        });

        true
    }

    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn add_arc(
        &mut self,
        center_x: f32,
        center_y: f32,
        radius: f32,
        start_angle_deg: f32,
        end_angle_deg: f32,
        stroke_width: f32,
        depth: f32,
        color: &[f32; 4],
        image: Option<i32>,
        mask: Option<i32>,
        subpix: bool,
    ) -> bool {
        if !self.vertices.is_empty() && subpix != self.subpix {
            return false;
        }
        let has_image = image.is_some();
        let has_mask = mask.is_some();
        if has_image && self.image.is_some() && self.image != image {
            return false;
        }
        if has_mask && self.mask.is_some() && self.mask != mask {
            return false;
        }
        self.subpix = subpix;
        self.image = image;
        self.mask = mask;
        let layers = [self.image.unwrap_or(0), self.mask.unwrap_or(0)];

        // Convert angles from degrees to radians
        let start_angle = start_angle_deg.to_radians();
        let end_angle = end_angle_deg.to_radians();

        // Number of segments to use for the arc (more segments = smoother curve)
        let segments = 16;

        // Calculate angle increment per segment
        let angle_diff = if end_angle >= start_angle {
            end_angle - start_angle
        } else {
            2.0 * std::f32::consts::PI - (start_angle - end_angle)
        };
        let angle_increment = angle_diff / segments as f32;

        // Calculate inner and outer radius for the stroke
        let inner_radius = radius - stroke_width / 2.0;
        let outer_radius = radius + stroke_width / 2.0;

        // Create vertices for the arc segments
        let mut current_angle = start_angle;

        for _ in 0..segments {
            let next_angle = current_angle + angle_increment;

            // Calculate vertex positions for current and next angle
            let inner_x1 = center_x + inner_radius * current_angle.cos();
            let inner_y1 = center_y + inner_radius * current_angle.sin();
            let outer_x1 = center_x + outer_radius * current_angle.cos();
            let outer_y1 = center_y + outer_radius * current_angle.sin();

            let inner_x2 = center_x + inner_radius * next_angle.cos();
            let inner_y2 = center_y + inner_radius * next_angle.sin();
            let outer_x2 = center_x + outer_radius * next_angle.cos();
            let outer_y2 = center_y + outer_radius * next_angle.sin();

            // Create vertex objects
            let v0 = Vertex {
                pos: [inner_x1, inner_y1, depth],
                color: *color,
                uv: [0.0, 0.0],
                layers,
            };
            let v1 = Vertex {
                pos: [inner_x2, inner_y2, depth],
                color: *color,
                uv: [0.0, 1.0],
                layers,
            };
            let v2 = Vertex {
                pos: [outer_x2, outer_y2, depth],
                color: *color,
                uv: [1.0, 1.0],
                layers,
            };
            let v3 = Vertex {
                pos: [outer_x1, outer_y1, depth],
                color: *color,
                uv: [1.0, 0.0],
                layers,
            };

            // Add vertices directly in drawing order (two triangles)
            // First triangle: v0, v1, v2
            self.vertices.push(v0);
            self.vertices.push(v1);
            self.vertices.push(v2);

            // Second triangle: v2, v3, v0
            self.vertices.push(v2);
            self.vertices.push(v3);
            self.vertices.push(v0);

            current_angle = next_angle;
        }

        true
    }

    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn add_rect(
        &mut self,
        rect: &Rect,
        depth: f32,
        color: &[f32; 4],
        coords: Option<&[f32; 4]>,
        image: Option<i32>,
        mask: Option<i32>,
        subpix: bool,
    ) -> bool {
        if !self.vertices.is_empty() && subpix != self.subpix {
            return false;
        }
        let has_image = image.is_some();
        let has_mask = mask.is_some();
        if has_image && self.image.is_some() && self.image != image {
            return false;
        }
        if has_mask && self.mask.is_some() && self.mask != mask {
            return false;
        }
        self.subpix = subpix;
        self.image = image;
        self.mask = mask;
        let layers = [self.image.unwrap_or(0), self.mask.unwrap_or(0)];
        self.push_rect(rect, depth, color, coords, layers);
        true
    }

    #[inline]
    fn push_rect(
        &mut self,
        rect: &Rect,
        depth: f32,
        color: &[f32; 4],
        coords: Option<&[f32; 4]>,
        layers: [i32; 2],
    ) {
        let x = rect.x;
        let y = rect.y;
        let w = rect.width;
        let h = rect.height;

        const DEFAULT_COORDS: [f32; 4] = [0., 0., 1., 1.];
        let coords = coords.unwrap_or(&DEFAULT_COORDS);
        let l = coords[0];
        let t = coords[1];
        let r = coords[2];
        let b = coords[3];

        // Create the four corner vertices
        let v0 = Vertex {
            pos: [x, y, depth],
            color: *color,
            uv: [l, t],
            layers,
        };
        let v1 = Vertex {
            pos: [x, y + h, depth],
            color: *color,
            uv: [l, b],
            layers,
        };
        let v2 = Vertex {
            pos: [x + w, y + h, depth],
            color: *color,
            uv: [r, b],
            layers,
        };
        let v3 = Vertex {
            pos: [x + w, y, depth],
            color: *color,
            uv: [r, t],
            layers,
        };

        // Add vertices directly in the order they'll be drawn
        // First triangle: v0, v1, v2
        self.vertices.push(v0);
        self.vertices.push(v1);
        self.vertices.push(v2);

        // Second triangle: v2, v3, v0
        self.vertices.push(v2);
        self.vertices.push(v3);
        self.vertices.push(v0);
    }

    #[inline]
    fn build_display_list(&self, list: &mut Vec<Vertex>) {
        // Since vertices are already in draw order, we can just copy them
        list.extend_from_slice(&self.vertices);
    }
}

pub struct BatchManager {
    batches: Vec<Batch>,
    opaque: Vec<Batch>,
    transparent: Vec<Batch>,
}

impl BatchManager {
    pub fn new() -> Self {
        Self {
            batches: Vec::new(),
            opaque: Vec::new(),
            transparent: Vec::new(),
        }
    }

    #[inline]
    pub fn reset(&mut self) {
        self.batches.append(&mut self.opaque);
        self.batches.append(&mut self.transparent);
        for batch in &mut self.batches {
            batch.clear();
        }
    }

    #[inline]
    #[allow(unused)]
    pub fn add_polygon(
        &mut self,
        points: &[(f32, f32)], // Array of (x,y) points defining the polygon
        depth: f32,
        color: [f32; 4],
    ) {
        // Need at least 3 points to form a polygon
        if points.len() < 3 {
            return;
        }

        // Use triangulation by fan method
        // This works well for convex shapes and some concave shapes,
        // but more complex triangulation would be needed for highly concave polygons
        let first_point = points[0];

        for i in 1..points.len() - 1 {
            self.add_triangle(
                first_point.0,
                first_point.1, // First vertex
                points[i].0,
                points[i].1, // Second vertex
                points[i + 1].0,
                points[i + 1].1, // Third vertex
                depth,
                color,
            );
        }
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn add_triangle(
        &mut self,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x3: f32,
        y3: f32,
        depth: f32,
        color: [f32; 4],
    ) {
        // Enable subpixel positioning for curved shapes
        let subpix = true;

        let transparent = color[3] != 1.0;
        if transparent {
            for batch in &mut self.transparent {
                if batch.add_triangle(
                    x1, y1, x2, y2, x3, y3, color, depth, None, None, subpix,
                ) {
                    return;
                }
            }
        } else {
            for batch in &mut self.opaque {
                if batch.add_triangle(
                    x1, y1, x2, y2, x3, y3, color, depth, None, None, subpix,
                ) {
                    return;
                }
            }
        }
        self.alloc_batch(transparent)
            .add_triangle(x1, y1, x2, y2, x3, y3, color, depth, None, None, subpix);
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn add_line(
        &mut self,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        width: f32,
        depth: f32,
        color: [f32; 4],
    ) {
        let transparent = color[3] != 1.0;
        if transparent {
            for batch in &mut self.transparent {
                if batch.add_line(
                    x1, y1, x2, y2, width, depth, color, None,  // image
                    None,  // mask
                    false, // subpix
                ) {
                    return;
                }
            }
        } else {
            for batch in &mut self.opaque {
                if batch.add_line(
                    x1, y1, x2, y2, width, depth, color, None,  // image
                    None,  // mask
                    false, // subpix
                ) {
                    return;
                }
            }
        }
        self.alloc_batch(transparent).add_line(
            x1, y1, x2, y2, width, depth, color, None,  // image
            None,  // mask
            false, // subpix
        );
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn add_arc(
        &mut self,
        center_x: f32,
        center_y: f32,
        radius: f32,
        start_angle_deg: f32,
        end_angle_deg: f32,
        stroke_width: f32,
        depth: f32,
        color: &[f32; 4],
    ) {
        let transparent = color[3] != 1.0;
        if transparent {
            for batch in &mut self.transparent {
                if batch.add_arc(
                    center_x,
                    center_y,
                    radius,
                    start_angle_deg,
                    end_angle_deg,
                    stroke_width,
                    depth,
                    color,
                    None,  // image
                    None,  // mask
                    false, // subpix
                ) {
                    return;
                }
            }
        } else {
            for batch in &mut self.opaque {
                if batch.add_arc(
                    center_x,
                    center_y,
                    radius,
                    start_angle_deg,
                    end_angle_deg,
                    stroke_width,
                    depth,
                    color,
                    None,  // image
                    None,  // mask
                    false, // subpix
                ) {
                    return;
                }
            }
        }
        self.alloc_batch(transparent).add_arc(
            center_x,
            center_y,
            radius,
            start_angle_deg,
            end_angle_deg,
            stroke_width,
            depth,
            color,
            None,  // image
            None,  // mask
            false, // subpix
        );
    }

    #[inline]
    pub fn add_antialiased_polygon(
        &mut self,
        points: &[(f32, f32)],
        depth: f32,
        color: [f32; 4],
    ) {
        // Need at least 3 points to form a polygon
        if points.len() < 3 {
            return;
        }

        // For a crisp-free appearance, we'll use a multi-pass approach
        // First, draw the main polygon at full opacity
        let first_point = points[0];

        // For curved shapes, use an optimized triangulation
        for i in 1..points.len() - 1 {
            let p1 = first_point;
            let p2 = points[i];
            let p3 = points[i + 1];

            // Add triangle with correct subpixel rendering
            self.add_triangle(p1.0, p1.1, p2.0, p2.1, p3.0, p3.1, depth, color);
        }

        // Now add anti-aliasing at the edges
        // Create a slightly larger stroke around the polygon with semi-transparency
        if points.len() >= 4 {
            // Draw anti-aliased edges
            // Create a slightly transparent version of the color for edge blending
            let depth = depth - 0.0001;
            let edge_color = [
                color[0],
                color[1],
                color[2],
                color[3] * 0.5, // Half opacity for smooth blending
            ];

            // Edge width for anti-aliasing (typically 0.5-1.0 pixels)
            let edge_width = 0.5;

            // Process each edge of the polygon
            for i in 0..points.len() {
                let p1 = points[i];
                let p2 = points[(i + 1) % points.len()];

                // Calculate edge vector
                let edge_x = p2.0 - p1.0;
                let edge_y = p2.1 - p1.1;

                // Calculate normalized perpendicular vector for edge expansion
                let edge_length = (edge_x * edge_x + edge_y * edge_y).sqrt();
                if edge_length > 0.001 {
                    let norm_x = -edge_y / edge_length;
                    let norm_y = edge_x / edge_length;

                    // Create expanded edge quad points
                    let q1 = (p1.0, p1.1);
                    let q2 = (p2.0, p2.1);
                    let q3 = (p2.0 + norm_x * edge_width, p2.1 + norm_y * edge_width);
                    let q4 = (p1.0 + norm_x * edge_width, p1.1 + norm_y * edge_width);

                    // Draw the quad as two triangles with transparency for anti-aliasing
                    self.add_triangle(
                        q1.0, q1.1, q2.0, q2.1, q3.0, q3.1, depth, edge_color,
                    );

                    self.add_triangle(
                        q1.0, q1.1, q3.0, q3.1, q4.0, q4.1, depth, edge_color,
                    );
                }
            }
        }
    }

    #[inline]
    pub fn add_mask_rect(
        &mut self,
        rect: &Rect,
        depth: f32,
        color: &[f32; 4],
        coords: &[f32; 4],
        subpix: bool,
    ) {
        for batch in &mut self.transparent {
            if batch.add_rect(rect, depth, color, Some(coords), None, Some(1), subpix) {
                return;
            }
        }
        self.alloc_batch(true).add_rect(
            rect,
            depth,
            color,
            Some(coords),
            None,
            Some(1),
            subpix,
        );
    }

    #[inline]
    pub fn add_image_rect(
        &mut self,
        rect: &Rect,
        depth: f32,
        color: &[f32; 4],
        coords: &[f32; 4],
        has_alpha: bool,
    ) {
        let transparent = has_alpha || color[3] != 1.0;
        if transparent {
            for batch in &mut self.transparent {
                if batch.add_rect(rect, depth, color, Some(coords), Some(1), None, false)
                {
                    return;
                }
            }
        } else {
            for batch in &mut self.opaque {
                if batch.add_rect(rect, depth, color, Some(coords), Some(1), None, false)
                {
                    return;
                }
            }
        }
        self.alloc_batch(transparent).add_rect(
            rect,
            depth,
            color,
            Some(coords),
            Some(1),
            None,
            false,
        );
    }

    #[inline]
    pub fn add_rect(&mut self, rect: &Rect, depth: f32, color: &[f32; 4]) {
        let transparent = color[3] != 1.0;
        if transparent {
            for batch in &mut self.transparent {
                if batch.add_rect(rect, depth, color, None, None, None, false) {
                    return;
                }
            }
        } else {
            for batch in &mut self.opaque {
                if batch.add_rect(rect, depth, color, None, None, None, false) {
                    return;
                }
            }
        }
        self.alloc_batch(transparent)
            .add_rect(rect, depth, color, None, None, None, false);
    }

    #[inline]
    pub fn build_display_list(&self, list: &mut Vec<Vertex>) {
        for batch in &self.opaque {
            if batch.vertices.is_empty() {
                continue;
            }
            batch.build_display_list(list);
        }
        for batch in &self.transparent {
            if batch.vertices.is_empty() {
                continue;
            }
            batch.build_display_list(list);
        }
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn draw_drawable_character(
        &mut self,
        x: f32,
        y: f32,
        advance: f32,
        character: DrawableChar,
        color: [f32; 4],
        depth: f32,
        line_height: f32,
    ) {
        let half_size = advance / 2.0;
        let stroke = f32::clamp(line_height / 10., 1.0, 6.0).round();
        let center_x = x + half_size;
        let center_y = y + (line_height / 2.0);
        let line_width = advance;

        match character {
            DrawableChar::Horizontal => {
                let rect = Rect {
                    x,
                    y: center_y - (stroke / 2.0),
                    width: line_width,
                    height: stroke,
                };
                self.add_rect(&rect, depth, &color);
            }
            DrawableChar::DoubleHorizontal => {
                // Calculate spacing between the two horizontal lines
                let gap = stroke * 1.5;

                // Top horizontal line
                let top_rect = Rect {
                    x,
                    y: center_y - gap,
                    width: line_width,
                    height: stroke,
                };

                // Bottom horizontal line
                let bottom_rect = Rect {
                    x,
                    y: center_y + gap - stroke,
                    width: line_width,
                    height: stroke,
                };

                // Draw both rectangles
                self.add_rect(&top_rect, depth, &color);
                self.add_rect(&bottom_rect, depth, &color);
            }
            DrawableChar::HeavyHorizontal => {
                let heavy_stroke = stroke * 2.0;
                let rect = Rect {
                    x,
                    y: center_y - heavy_stroke / 2.0,
                    width: line_width,
                    height: heavy_stroke,
                };
                self.add_rect(&rect, depth, &color);
            }
            DrawableChar::Vertical => {
                let rect = Rect {
                    x: center_x - (stroke / 2.0),
                    y,
                    width: stroke,
                    height: line_height,
                };
                self.add_rect(&rect, depth, &color);
            }
            DrawableChar::DoubleVertical => {
                let gap = stroke * 1.5;

                // Left vertical line
                let left_rect = Rect {
                    x: center_x - gap,
                    y,
                    width: stroke,
                    height: line_height,
                };

                // Right vertical line
                let right_rect = Rect {
                    x: center_x + gap - stroke,
                    y,
                    width: stroke,
                    height: line_height,
                };

                // Draw both rectangles
                self.add_rect(&left_rect, depth, &color);
                self.add_rect(&right_rect, depth, &color);
            }
            DrawableChar::HeavyVertical => {
                let heavy_stroke = stroke * 2.0;
                let rect = Rect {
                    x: center_x - heavy_stroke / 2.0,
                    y,
                    width: heavy_stroke,
                    height: line_height,
                };
                self.add_rect(&rect, depth, &color);
            }
            DrawableChar::DoubleCross => {
                let gap = stroke * 1.5;

                // Vertical double lines
                let top_left_vertical_rect = Rect {
                    x: center_x - gap,
                    y,
                    width: stroke,
                    height: (line_height / 2.0) - gap,
                };

                let top_right_vertical_rect = Rect {
                    x: center_x + gap - stroke,
                    y,
                    width: stroke,
                    height: (line_height / 2.0) - gap,
                };

                let bottom_left_vertical_rect = Rect {
                    x: center_x - gap,
                    y: center_y + gap,
                    width: stroke,
                    height: (line_height / 2.0) - gap,
                };

                let bottom_right_vertical_rect = Rect {
                    x: center_x + gap - stroke,
                    y: center_y + gap,
                    width: stroke,
                    height: (line_height / 2.0) - gap,
                };

                // Horizontal double lines
                let left_top_horizontal_rect = Rect {
                    x,
                    y: center_y - gap,
                    width: (line_width - stroke) / 2.0,
                    height: stroke,
                };

                let left_bottom_horizontal_rect = Rect {
                    x,
                    y: center_y + gap - stroke,
                    width: (line_width - stroke) / 2.0,
                    height: stroke,
                };

                let right_top_horizontal_rect = Rect {
                    x: center_x + gap - stroke,
                    y: center_y - gap,
                    width: line_width / 2.0,
                    height: stroke,
                };

                let right_bottom_horizontal_rect = Rect {
                    x: center_x + gap - stroke,
                    y: center_y + gap - stroke,
                    width: line_width / 2.0,
                    height: stroke,
                };

                // Draw all rectangles
                self.add_rect(&top_left_vertical_rect, depth, &color);
                self.add_rect(&top_right_vertical_rect, depth, &color);
                self.add_rect(&bottom_left_vertical_rect, depth, &color);
                self.add_rect(&bottom_right_vertical_rect, depth, &color);
                self.add_rect(&left_top_horizontal_rect, depth, &color);
                self.add_rect(&left_bottom_horizontal_rect, depth, &color);
                self.add_rect(&right_top_horizontal_rect, depth, &color);
                self.add_rect(&right_bottom_horizontal_rect, depth, &color);
            }
            DrawableChar::DoubleVerticalRight => {
                let gap = stroke * 1.5;

                // Vertical double lines
                let left_vertical_rect = Rect {
                    x: center_x - gap,
                    y,
                    width: stroke,
                    height: line_height,
                };

                let top_right_vertical_rect = Rect {
                    x: center_x + gap - stroke,
                    y,
                    width: stroke,
                    height: (line_height / 2.) - (gap - stroke),
                };

                let bottom_right_vertical_rect = Rect {
                    x: center_x + gap - stroke,
                    y: center_y + gap - stroke,
                    width: stroke,
                    height: (line_height / 2.) - (gap - stroke),
                };

                // Horizontal double lines going right from center
                let top_horizontal_rect = Rect {
                    x: center_x + gap,
                    y: center_y - gap,
                    width: (line_width / 2.0) - gap, // Right half
                    height: stroke,
                };

                let bottom_horizontal_rect = Rect {
                    x: center_x + gap,
                    y: center_y + gap - stroke,
                    width: (line_width / 2.0) - gap, // Right half
                    height: stroke,
                };

                // Draw all rectangles
                self.add_rect(&left_vertical_rect, depth, &color);
                self.add_rect(&top_right_vertical_rect, depth, &color);
                self.add_rect(&bottom_right_vertical_rect, depth, &color);
                self.add_rect(&top_horizontal_rect, depth, &color);
                self.add_rect(&bottom_horizontal_rect, depth, &color);
            }
            DrawableChar::DoubleVerticalLeft => {
                let gap = stroke * 1.5;
                // Right vertical line - split into top and bottom portions
                let right_top_vertical_rect = Rect {
                    x: center_x - gap,
                    y,
                    width: stroke,
                    height: (line_height / 2.) - (gap - stroke),
                };
                let right_bottom_vertical_rect = Rect {
                    x: center_x - gap,
                    y: center_y + gap - stroke,
                    width: stroke,
                    height: (line_height / 2.) - (gap - stroke),
                };
                // Left vertical line - full height
                let left_vertical_rect = Rect {
                    x: center_x + gap - stroke,
                    y,
                    width: stroke,
                    height: line_height,
                };
                // Horizontal lines going left from center
                let top_horizontal_rect = Rect {
                    x,
                    y: center_y - gap,
                    width: (line_width / 2.0) - gap, // Left half
                    height: stroke,
                };
                let bottom_horizontal_rect = Rect {
                    x,
                    y: center_y + gap - stroke,
                    width: (line_width / 2.0) - gap, // Left half
                    height: stroke,
                };
                // Draw all rectangles
                self.add_rect(&left_vertical_rect, depth, &color);
                self.add_rect(&right_top_vertical_rect, depth, &color);
                self.add_rect(&right_bottom_vertical_rect, depth, &color);
                self.add_rect(&top_horizontal_rect, depth, &color);
                self.add_rect(&bottom_horizontal_rect, depth, &color);
            }
            DrawableChar::DoubleHorizontalDown => {
                let gap = stroke * 1.5;

                // Horizontal double lines
                let top_horizontal_rect = Rect {
                    x,
                    y: center_y - gap,
                    width: line_width,
                    height: stroke,
                };

                let left_bottom_horizontal_rect = Rect {
                    x,
                    y: center_y + gap - stroke,
                    width: (line_width / 2.0) - (gap - stroke),
                    height: stroke,
                };

                let right_bottom_horizontal_rect = Rect {
                    x: center_x + gap - stroke,
                    y: center_y + gap - stroke,
                    width: (line_width / 2.0) - (gap - stroke),
                    height: stroke,
                };

                // Vertical double lines going down from center
                let left_vertical_rect = Rect {
                    x: center_x - gap,
                    y: center_y + gap,
                    width: stroke,
                    height: (line_height / 2.0) - gap, // Bottom half
                };

                let right_vertical_rect = Rect {
                    x: center_x + gap - stroke,
                    y: center_y + gap,
                    width: stroke,
                    height: (line_height / 2.0) - gap, // Bottom half
                };

                // Draw all rectangles
                self.add_rect(&top_horizontal_rect, depth, &color);
                self.add_rect(&left_bottom_horizontal_rect, depth, &color);
                self.add_rect(&right_bottom_horizontal_rect, depth, &color);
                self.add_rect(&left_vertical_rect, depth, &color);
                self.add_rect(&right_vertical_rect, depth, &color);
            }
            // ╦ ╩
            DrawableChar::DoubleHorizontalUp => {
                let gap = stroke * 1.5;

                // Horizontal double lines
                let bottom_horizontal_rect = Rect {
                    x,
                    y: center_y + gap - stroke,
                    width: line_width,
                    height: stroke,
                };

                let left_top_horizontal_rect = Rect {
                    x,
                    y: center_y - gap,
                    width: (line_width / 2.0) - (gap - stroke),
                    height: stroke,
                };

                let right_top_horizontal_rect = Rect {
                    x: center_x + gap - stroke,
                    y: center_y - gap,
                    width: (line_width / 2.0) - (gap - stroke),
                    height: stroke,
                };

                // Vertical double lines going down from center
                let left_vertical_rect = Rect {
                    x: center_x - gap,
                    y,
                    width: stroke,
                    height: (line_height / 2.0) - gap, // Bottom half
                };

                let right_vertical_rect = Rect {
                    x: center_x + gap - stroke,
                    y,
                    width: stroke,
                    height: (line_height / 2.0) - gap, // Bottom half
                };

                // Draw all rectangles
                self.add_rect(&bottom_horizontal_rect, depth, &color);
                self.add_rect(&left_top_horizontal_rect, depth, &color);
                self.add_rect(&right_top_horizontal_rect, depth, &color);
                self.add_rect(&left_vertical_rect, depth, &color);
                self.add_rect(&right_vertical_rect, depth, &color);
            }
            DrawableChar::VerticalDoubleAndHorizontalSingle => {
                let gap = stroke * 1.5;

                // Vertical double lines
                let left_vertical_rect = Rect {
                    x: center_x - gap,
                    y,
                    width: stroke,
                    height: line_height,
                };

                let right_vertical_rect = Rect {
                    x: center_x + gap - stroke,
                    y,
                    width: stroke,
                    height: line_height,
                };

                // Horizontal single line
                let horiz_rect = Rect {
                    x,
                    y: center_y - (stroke / 2.0),
                    width: line_width,
                    height: stroke,
                };

                // Draw all rectangles
                self.add_rect(&left_vertical_rect, depth, &color);
                self.add_rect(&right_vertical_rect, depth, &color);
                self.add_rect(&horiz_rect, depth, &color);
            }
            DrawableChar::DownDoubleAndRightSingle => {
                let gap = stroke * 1.5;

                // Vertical double lines going down from center
                let left_vertical_rect = Rect {
                    x: center_x - gap,
                    y: center_y + (gap - stroke),
                    width: stroke,
                    height: (line_height / 2.0) - (gap - stroke), // Bottom half
                };

                let right_vertical_rect = Rect {
                    x: center_x + gap - stroke,
                    y: center_y + (gap - stroke),
                    width: stroke,
                    height: (line_height / 2.0) - (gap - stroke), // Bottom half
                };

                // Horizontal single line going right from center
                let horiz_rect = Rect {
                    x: center_x - gap,
                    y: center_y - (stroke / 2.0),
                    width: (line_width / 2.0) + gap, // Right half
                    height: stroke,
                };

                // Draw all rectangles
                self.add_rect(&left_vertical_rect, depth, &color);
                self.add_rect(&right_vertical_rect, depth, &color);
                self.add_rect(&horiz_rect, depth, &color);
            }
            DrawableChar::DownDoubleAndLeftSingle => {
                let gap = stroke * 1.5;

                // Vertical double lines going down from center
                let left_vertical_rect = Rect {
                    x: center_x - gap,
                    y: center_y + (gap - stroke),
                    width: stroke,
                    height: (line_height / 2.0) - (gap - stroke), // Bottom half
                };

                let right_vertical_rect = Rect {
                    x: center_x + gap - stroke,
                    y: center_y + (gap - stroke),
                    width: stroke,
                    height: (line_height / 2.0) - (gap - stroke), // Bottom half
                };

                // Horizontal single line going left from center
                let horiz_rect = Rect {
                    x,
                    y: center_y - (stroke / 2.0),
                    width: (line_width / 2.0) + gap, // Left half
                    height: stroke,
                };

                // Draw all rectangles
                self.add_rect(&left_vertical_rect, depth, &color);
                self.add_rect(&right_vertical_rect, depth, &color);
                self.add_rect(&horiz_rect, depth, &color);
            }
            DrawableChar::VerticalDoubleAndRightSingle => {
                let gap = stroke * 1.5;

                // Vertical double lines
                let left_vertical_rect = Rect {
                    x: center_x - gap,
                    y,
                    width: stroke,
                    height: line_height,
                };

                let right_vertical_rect = Rect {
                    x: center_x + gap - stroke,
                    y,
                    width: stroke,
                    height: line_height,
                };

                // Horizontal single line going right from center
                let horiz_rect = Rect {
                    x: center_x + gap,
                    y: center_y - (stroke / 2.0),
                    width: (line_width / 2.0) - gap, // Right half
                    height: stroke,
                };

                // Draw all rectangles
                self.add_rect(&left_vertical_rect, depth, &color);
                self.add_rect(&right_vertical_rect, depth, &color);
                self.add_rect(&horiz_rect, depth, &color);
            }
            DrawableChar::VerticalDoubleAndLeftSingle => {
                let gap = stroke * 1.5;

                // Vertical double lines
                let left_vertical_rect = Rect {
                    x: center_x - gap,
                    y,
                    width: stroke,
                    height: line_height,
                };

                let right_vertical_rect = Rect {
                    x: center_x + gap - stroke,
                    y,
                    width: stroke,
                    height: line_height,
                };

                // Horizontal single line going left from center
                let horiz_rect = Rect {
                    x,
                    y: center_y - (stroke / 2.0),
                    width: (line_width / 2.0) - gap, // Left half
                    height: stroke,
                };

                // Draw all rectangles
                self.add_rect(&left_vertical_rect, depth, &color);
                self.add_rect(&right_vertical_rect, depth, &color);
                self.add_rect(&horiz_rect, depth, &color);
            }
            DrawableChar::VerticalSingleAndRightDouble => {
                // Calculate spacing between the double horizontal lines
                let gap = stroke * 1.5;

                // Vertical single line
                let vertical_rect = Rect {
                    x: center_x - (stroke / 2.0),
                    y,
                    width: stroke,
                    height: line_height,
                };

                // Horizontal double lines going right from center
                let top_horizontal_rect = Rect {
                    x: center_x,
                    y: center_y - gap,
                    width: line_width / 2.0, // Right half
                    height: stroke,
                };

                let bottom_horizontal_rect = Rect {
                    x: center_x,
                    y: center_y + gap - stroke,
                    width: line_width / 2.0, // Right half
                    height: stroke,
                };

                // Draw all rectangles
                self.add_rect(&vertical_rect, depth, &color);
                self.add_rect(&top_horizontal_rect, depth, &color);
                self.add_rect(&bottom_horizontal_rect, depth, &color);
            }
            DrawableChar::VerticalSingleAndLeftDouble => {
                // Calculate spacing between the double horizontal lines
                let gap = stroke * 1.5;

                // Vertical single line
                let vertical_rect = Rect {
                    x: center_x - (stroke / 2.0),
                    y,
                    width: stroke,
                    height: line_height,
                };

                // Horizontal double lines going left from center
                let top_horizontal_rect = Rect {
                    x,
                    y: center_y - gap,
                    width: line_width / 2.0, // Left half
                    height: stroke,
                };

                let bottom_horizontal_rect = Rect {
                    x,
                    y: center_y + gap - stroke,
                    width: line_width / 2.0, // Left half
                    height: stroke,
                };

                // Draw all rectangles
                self.add_rect(&vertical_rect, depth, &color);
                self.add_rect(&top_horizontal_rect, depth, &color);
                self.add_rect(&bottom_horizontal_rect, depth, &color);
            }
            DrawableChar::DownSingleAndRightDouble => {
                // Calculate spacing between the double horizontal lines
                let gap = stroke * 1.5;

                // Vertical single line going down from center
                let vertical_rect = Rect {
                    x: center_x - (stroke / 2.0),
                    y: center_y - gap,
                    width: stroke,
                    height: (line_height / 2.0) + gap, // Bottom half
                };

                // Horizontal double lines going right from center
                let top_horizontal_rect = Rect {
                    x: center_x,
                    y: center_y - gap,
                    width: line_width / 2.0, // Right half
                    height: stroke,
                };

                let bottom_horizontal_rect = Rect {
                    x: center_x,
                    y: center_y + gap - stroke,
                    width: line_width / 2.0, // Right half
                    height: stroke,
                };

                // Draw all rectangles
                self.add_rect(&vertical_rect, depth, &color);
                self.add_rect(&top_horizontal_rect, depth, &color);
                self.add_rect(&bottom_horizontal_rect, depth, &color);
            }
            DrawableChar::DownSingleAndLeftDouble => {
                // Calculate spacing between the double horizontal lines
                let gap = stroke * 1.5;

                // Vertical single line going down from center
                let vertical_rect = Rect {
                    x: center_x - (stroke / 2.0),
                    y: center_y - gap,
                    width: stroke,
                    height: (line_height / 2.0) + gap, // Bottom half
                };

                // Horizontal double lines going left from center
                let top_horizontal_rect = Rect {
                    x,
                    y: center_y - gap,
                    width: line_width / 2.0, // Left half
                    height: stroke,
                };

                let bottom_horizontal_rect = Rect {
                    x,
                    y: center_y + gap - stroke,
                    width: line_width / 2.0, // Left half
                    height: stroke,
                };

                // Draw all rectangles
                self.add_rect(&vertical_rect, depth, &color);
                self.add_rect(&top_horizontal_rect, depth, &color);
                self.add_rect(&bottom_horizontal_rect, depth, &color);
            }
            DrawableChar::HeavyDownAndRight => {
                let heavy_stroke = stroke * 2.0;

                // Heavy vertical line going down from center
                let vertical_rect = Rect {
                    x: center_x - heavy_stroke / 2.0,
                    y: center_y - heavy_stroke / 2.0,
                    width: heavy_stroke,
                    height: (line_height / 2.0) + heavy_stroke / 2.0, // Bottom half
                };

                // Heavy horizontal line going right from center
                let horizontal_rect = Rect {
                    x: center_x,
                    y: center_y - heavy_stroke / 2.0,
                    width: line_width / 2.0, // Right half
                    height: heavy_stroke,
                };

                // Draw both rectangles
                self.add_rect(&vertical_rect, depth, &color);
                self.add_rect(&horizontal_rect, depth, &color);
            }
            DrawableChar::HeavyDownAndLeft => {
                let heavy_stroke = stroke * 2.0;

                // Heavy vertical line going down from center
                let vertical_rect = Rect {
                    x: center_x - heavy_stroke / 2.0,
                    y: center_y - heavy_stroke / 2.0,
                    width: heavy_stroke,
                    height: (line_height / 2.0) + heavy_stroke / 2.0, // Bottom half
                };

                // Heavy horizontal line going left from center
                let horizontal_rect = Rect {
                    x,
                    y: center_y - heavy_stroke / 2.0,
                    width: line_width / 2.0, // Left half
                    height: heavy_stroke,
                };

                // Draw both rectangles
                self.add_rect(&vertical_rect, depth, &color);
                self.add_rect(&horizontal_rect, depth, &color);
            }
            DrawableChar::HeavyUpAndRight => {
                let heavy_stroke = stroke * 2.0;

                // Heavy vertical line going up from center
                let vertical_rect = Rect {
                    x: center_x - heavy_stroke / 2.0,
                    y,
                    width: heavy_stroke,
                    height: (line_height / 2.0) + heavy_stroke / 2.0, // Top half
                };

                // Heavy horizontal line going right from center
                let horizontal_rect = Rect {
                    x: center_x,
                    y: center_y - heavy_stroke / 2.0,
                    width: line_width / 2.0, // Right half
                    height: heavy_stroke,
                };

                // Draw both rectangles
                self.add_rect(&vertical_rect, depth, &color);
                self.add_rect(&horizontal_rect, depth, &color);
            }
            DrawableChar::HeavyUpAndLeft => {
                let heavy_stroke = stroke * 2.0;

                // Heavy vertical line going up from center
                let vertical_rect = Rect {
                    x: center_x - heavy_stroke / 2.0,
                    y,
                    width: heavy_stroke,
                    height: (line_height / 2.0) + heavy_stroke / 2.0, // Top half
                };

                // Heavy horizontal line going left from center
                let horizontal_rect = Rect {
                    x,
                    y: center_y - heavy_stroke / 2.0,
                    width: line_width / 2.0, // Left half
                    height: heavy_stroke,
                };

                // Draw both rectangles
                self.add_rect(&vertical_rect, depth, &color);
                self.add_rect(&horizontal_rect, depth, &color);
            }
            DrawableChar::HeavyVerticalAndRight => {
                let heavy_stroke = stroke * 2.0;

                // Heavy vertical line
                let vertical_rect = Rect {
                    x: center_x - heavy_stroke / 2.0,
                    y,
                    width: heavy_stroke,
                    height: line_height,
                };

                // Heavy horizontal line going right from center
                let horizontal_rect = Rect {
                    x: center_x,
                    y: center_y - heavy_stroke / 2.0,
                    width: line_width / 2.0, // Right half
                    height: heavy_stroke,
                };

                // Draw both rectangles
                self.add_rect(&vertical_rect, depth, &color);
                self.add_rect(&horizontal_rect, depth, &color);
            }
            DrawableChar::HeavyVerticalAndLeft => {
                let heavy_stroke = stroke * 2.0;

                // Heavy vertical line
                let vertical_rect = Rect {
                    x: center_x - heavy_stroke / 2.0,
                    y,
                    width: heavy_stroke,
                    height: line_height,
                };

                // Heavy horizontal line going left from center
                let horizontal_rect = Rect {
                    x,
                    y: center_y - heavy_stroke / 2.0,
                    width: line_width / 2.0, // Left half
                    height: heavy_stroke,
                };

                // Draw both rectangles
                self.add_rect(&vertical_rect, depth, &color);
                self.add_rect(&horizontal_rect, depth, &color);
            }
            DrawableChar::HeavyHorizontalAndDown => {
                let heavy_stroke = stroke * 2.0;

                // Heavy horizontal line
                let horizontal_rect = Rect {
                    x,
                    y: center_y - heavy_stroke / 2.0,
                    width: line_width,
                    height: heavy_stroke,
                };

                // Heavy vertical line going down from center
                let vertical_rect = Rect {
                    x: center_x - heavy_stroke / 2.0,
                    y: center_y,
                    width: heavy_stroke,
                    height: line_height / 2.0, // Bottom half
                };

                // Draw both rectangles
                self.add_rect(&horizontal_rect, depth, &color);
                self.add_rect(&vertical_rect, depth, &color);
            }
            DrawableChar::HeavyHorizontalAndUp => {
                let heavy_stroke = stroke * 2.0;

                // Heavy horizontal line
                let horizontal_rect = Rect {
                    x,
                    y: center_y - heavy_stroke / 2.0,
                    width: line_width,
                    height: heavy_stroke,
                };

                // Heavy vertical line going up from center
                let vertical_rect = Rect {
                    x: center_x - heavy_stroke / 2.0,
                    y,
                    width: heavy_stroke,
                    height: line_height / 2.0, // Top half
                };

                // Draw both rectangles
                self.add_rect(&horizontal_rect, depth, &color);
                self.add_rect(&vertical_rect, depth, &color);
            }
            DrawableChar::HeavyCross => {
                let heavy_stroke = stroke * 2.0;

                // Heavy vertical line
                let vertical_rect = Rect {
                    x: center_x - heavy_stroke / 2.0,
                    y,
                    width: heavy_stroke,
                    height: line_height,
                };

                // Heavy horizontal line
                let horizontal_rect = Rect {
                    x,
                    y: center_y - heavy_stroke / 2.0,
                    width: line_width,
                    height: heavy_stroke,
                };

                // Draw both rectangles
                self.add_rect(&vertical_rect, depth, &color);
                self.add_rect(&horizontal_rect, depth, &color);
            }
            DrawableChar::LightDownAndHeavyRight => {
                // Light vertical line going down from center
                let vertical_rect = Rect {
                    x: center_x - (stroke / 2.0),
                    y: center_y - stroke,
                    width: stroke,
                    height: (line_height / 2.0) + stroke, // Bottom half
                };

                // Heavy horizontal line going right from center
                let heavy_stroke = stroke * 2.0;
                let horizontal_rect = Rect {
                    x: center_x,
                    y: center_y - heavy_stroke / 2.0,
                    width: line_width / 2.0, // Right half
                    height: heavy_stroke,
                };

                // Draw both rectangles
                self.add_rect(&vertical_rect, depth, &color);
                self.add_rect(&horizontal_rect, depth, &color);
            }
            DrawableChar::LightDownAndHeavyLeft => {
                // Light vertical line going down from center
                let vertical_rect = Rect {
                    x: center_x - (stroke / 2.0),
                    y: center_y - stroke,
                    width: stroke,
                    height: (line_height / 2.0) + stroke, // Bottom half
                };

                // Heavy horizontal line going left from center
                let heavy_stroke = stroke * 2.0;
                let horizontal_rect = Rect {
                    x,
                    y: center_y - heavy_stroke / 2.0,
                    width: line_width / 2.0, // Left half
                    height: heavy_stroke,
                };

                // Draw both rectangles
                self.add_rect(&vertical_rect, depth, &color);
                self.add_rect(&horizontal_rect, depth, &color);
            }
            DrawableChar::HeavyDownAndLightRight => {
                // Heavy vertical line going down from center
                let heavy_stroke = stroke * 2.0;
                let vertical_rect = Rect {
                    x: center_x - heavy_stroke / 2.0,
                    y: center_y - (stroke / 2.0),
                    width: heavy_stroke,
                    height: (line_height / 2.0) + (stroke / 2.0), // Bottom half
                };

                // Light horizontal line going right from center
                let horizontal_rect = Rect {
                    x: center_x,
                    y: center_y - (stroke / 2.0),
                    width: line_width / 2.0, // Right half
                    height: stroke,
                };

                // Draw both rectangles
                self.add_rect(&vertical_rect, depth, &color);
                self.add_rect(&horizontal_rect, depth, &color);
            }
            DrawableChar::HeavyDownAndLightLeft => {
                // Heavy vertical line going down from center
                let heavy_stroke = stroke * 2.0;
                let vertical_rect = Rect {
                    x: center_x - heavy_stroke / 2.0,
                    y: center_y - (stroke / 2.0),
                    width: heavy_stroke,
                    height: (line_height / 2.0) + (stroke / 2.0), // Bottom half
                };

                // Light horizontal line going left from center
                let horizontal_rect = Rect {
                    x,
                    y: center_y - (stroke / 2.0),
                    width: line_width / 2.0, // Left half
                    height: stroke,
                };

                // Draw both rectangles
                self.add_rect(&vertical_rect, depth, &color);
                self.add_rect(&horizontal_rect, depth, &color);
            }
            DrawableChar::LightUpAndHeavyRight => {
                // Light vertical line going up from center
                let vertical_rect = Rect {
                    x: center_x - (stroke / 2.0),
                    y,
                    width: stroke,
                    height: (line_height / 2.0) + stroke, // Top half
                };

                // Heavy horizontal line going right from center
                let heavy_stroke = stroke * 2.0;
                let horizontal_rect = Rect {
                    x: center_x,
                    y: center_y - heavy_stroke / 2.0,
                    width: line_width / 2.0, // Right half
                    height: heavy_stroke,
                };

                // Draw both rectangles
                self.add_rect(&vertical_rect, depth, &color);
                self.add_rect(&horizontal_rect, depth, &color);
            }
            DrawableChar::LightUpAndHeavyLeft => {
                // Light vertical line going up from center
                let vertical_rect = Rect {
                    x: center_x - (stroke / 2.0),
                    y,
                    width: stroke,
                    height: (line_height / 2.0) + stroke, // Top half
                };

                // Heavy horizontal line going left from center
                let heavy_stroke = stroke * 2.0;
                let horizontal_rect = Rect {
                    x,
                    y: center_y - heavy_stroke / 2.0,
                    width: line_width / 2.0, // Left half
                    height: heavy_stroke,
                };

                // Draw both rectangles
                self.add_rect(&vertical_rect, depth, &color);
                self.add_rect(&horizontal_rect, depth, &color);
            }
            DrawableChar::HeavyUpAndLightRight => {
                // Heavy vertical line going up from center
                let heavy_stroke = stroke * 2.0;
                let vertical_rect = Rect {
                    x: center_x - heavy_stroke / 2.0,
                    y,
                    width: heavy_stroke,
                    height: (line_height / 2.0) + (stroke / 2.0), // Top half
                };

                // Light horizontal line going right from center
                let horizontal_rect = Rect {
                    x: center_x,
                    y: center_y - (stroke / 2.0),
                    width: line_width / 2.0, // Right half
                    height: stroke,
                };

                // Draw both rectangles
                self.add_rect(&vertical_rect, depth, &color);
                self.add_rect(&horizontal_rect, depth, &color);
            }
            DrawableChar::HeavyUpAndLightLeft => {
                // Heavy vertical line going up from center
                let heavy_stroke = stroke * 2.0;
                let vertical_rect = Rect {
                    x: center_x - heavy_stroke / 2.0,
                    y,
                    width: heavy_stroke,
                    height: (line_height / 2.0) + (stroke / 2.0), // Top half
                };

                // Light horizontal line going left from center
                let horizontal_rect = Rect {
                    x,
                    y: center_y - (stroke / 2.0),
                    width: line_width / 2.0, // Left half
                    height: stroke,
                };

                // Draw both rectangles
                self.add_rect(&vertical_rect, depth, &color);
                self.add_rect(&horizontal_rect, depth, &color);
            }
            DrawableChar::UpperOneQuarterBlock => {
                // Upper One Quarter Block (▀) - fills top 1/4 of the cell
                let block_height = line_height / 4.0;
                let block_rect = Rect {
                    x,
                    y,
                    width: line_width,
                    height: block_height,
                };
                self.add_rect(&block_rect, depth, &color);
            }
            DrawableChar::LowerFiveEighthsBlock => {
                // Lower Five Eighths Block (▅) - fills bottom 5/8 of the cell
                let block_height = (line_height * 5.0) / 8.0;
                let block_rect = Rect {
                    x,
                    y: y + line_height - block_height,
                    width: line_width,
                    height: block_height,
                };
                self.add_rect(&block_rect, depth, &color);
            }
            DrawableChar::LowerThreeQuartersBlock => {
                // Lower Three Quarters Block (▆) - fills bottom 3/4 of the cell
                let block_height = (line_height * 3.0) / 4.0;
                let block_rect = Rect {
                    x,
                    y: y + line_height - block_height,
                    width: line_width,
                    height: block_height,
                };
                self.add_rect(&block_rect, depth, &color);
            }
            DrawableChar::LowerSevenEighthsBlock => {
                // Lower Seven Eighths Block (▇) - fills bottom 7/8 of the cell
                let block_height = (line_height * 7.0) / 8.0;
                let block_rect = Rect {
                    x,
                    y: y + line_height - block_height,
                    width: line_width,
                    height: block_height,
                };
                self.add_rect(&block_rect, depth, &color);
            }
            DrawableChar::QuadrantUpperRightAndLowerLeft => {
                // QuadrantUpperRightAndLowerLeft (▟) - fills upper right and lower left quadrants
                let upper_right_rect = Rect {
                    x: center_x,
                    y,
                    width: line_width / 2.0,
                    height: line_height,
                };
                let lower_left_rect = Rect {
                    x,
                    y: center_y,
                    width: line_width / 2.0,
                    height: line_height / 2.0,
                };
                self.add_rect(&upper_right_rect, depth, &color);
                self.add_rect(&lower_left_rect, depth, &color);
            }
            DrawableChar::QuadrantUpperRightAndLowerRight => {
                // QuadrantUpperRightAndLowerRight (▙) - fills upper right and lower right quadrants
                let upper_left_rect = Rect {
                    x,
                    y,
                    width: line_width / 2.0,
                    height: line_height,
                };
                let lower_right_rect = Rect {
                    x: center_x,
                    y: center_y,
                    width: line_width / 2.0,
                    height: line_height / 2.0,
                };
                self.add_rect(&upper_left_rect, depth, &color);
                self.add_rect(&lower_right_rect, depth, &color);
            }
            DrawableChar::QuadrantUpperLeftAndLowerLeft => {
                let upper_left_rect = Rect {
                    x,
                    y,
                    width: line_width / 2.0,
                    height: line_height / 2.0,
                };
                let lower_right_rect = Rect {
                    x: center_x,
                    y: center_y,
                    width: line_width / 2.0,
                    height: line_height / 2.0,
                };
                self.add_rect(&upper_left_rect, depth, &color);
                self.add_rect(&lower_right_rect, depth, &color);
            }
            DrawableChar::QuadrantUpperLeftAndUpperRight => {
                // QuadrantUpperLeftAndUpperRight (▀) - fills upper half of the cell
                let upper_rect = Rect {
                    x,
                    y,
                    width: line_width,
                    height: line_height / 2.0,
                };
                self.add_rect(&upper_rect, depth, &color);
            }
            DrawableChar::QuadrantUpperLeftAndLowerRight => {
                let upper_right_rect = Rect {
                    x: center_x,
                    y,
                    width: line_width / 2.0,
                    height: line_height / 2.0,
                };
                let lower_left_rect = Rect {
                    x,
                    y: center_y,
                    width: line_width / 2.0,
                    height: line_height / 2.0,
                };
                self.add_rect(&upper_right_rect, depth, &color);
                self.add_rect(&lower_left_rect, depth, &color);
            }
            DrawableChar::DiagonalRisingBar => {
                // DiagonalRisingBar (╱) - diagonal line from bottom-left to top-right
                // We'll approximate this with a rotated rectangle
                // let diagonal_width =
                // (line_width * line_width + line_height * line_height).sqrt();
                // let diagonal_height = stroke;

                // Calculate the angle of rotation in radians
                // let angle = (line_height / line_width).atan();

                // Calculate the offset to center the rotated rectangle
                // let offset_x = (diagonal_width - line_width) / 2.0;
                // let offset_y = (diagonal_height - line_height) / 2.0;

                // Create a path for the diagonal line
                let path = vec![
                    (x, y + line_height),          // bottom-left
                    (x + stroke, y + line_height), // bottom-left + stroke width
                    (x + line_width, y),           // top-right
                    (x + line_width - stroke, y),  // top-right - stroke width
                ];

                self.add_polygon(&path, depth, color);
            }
            DrawableChar::DiagonalFallingBar => {
                // DiagonalFallingBar (╲) - diagonal line from top-left to bottom-right
                // We'll approximate this with a rotated rectangle
                // let diagonal_width =
                //     (line_width * line_width + line_height * line_height).sqrt();
                // let diagonal_height = stroke;

                // // Calculate the angle of rotation in radians
                // let angle = (line_height / line_width).atan();

                // Create a path for the diagonal line
                let path = vec![
                    (x, y),                                     // top-left
                    (x + stroke, y),                            // top-left + stroke width
                    (x + line_width, y + line_height),          // bottom-right
                    (x + line_width - stroke, y + line_height), // bottom-right - stroke width
                ];

                self.add_polygon(&path, depth, color);
            }
            DrawableChar::DiagonalCross => {
                // DiagonalCross (╳) - combination of rising and falling diagonals
                // Create paths for both diagonals
                let rising_path = vec![
                    (x, y + line_height),          // bottom-left
                    (x + stroke, y + line_height), // bottom-left + stroke width
                    (x + line_width, y),           // top-right
                    (x + line_width - stroke, y),  // top-right - stroke width
                ];

                let falling_path = vec![
                    (x, y),                                     // top-left
                    (x + stroke, y),                            // top-left + stroke width
                    (x + line_width, y + line_height),          // bottom-right
                    (x + line_width - stroke, y + line_height), // bottom-right - stroke width
                ];

                self.add_polygon(&rising_path, depth, color);
                self.add_polygon(&falling_path, depth, color);
            }
            DrawableChar::LowerOneEighthBlock => {
                // Lower One Eighth Block (▁) - fills bottom 1/8 of the cell
                let block_height = line_height / 8.0;
                let block_rect = Rect {
                    x,
                    y: y + line_height - block_height, // Position at bottom 1/8
                    width: line_width,
                    height: block_height,
                };
                self.add_rect(&block_rect, depth, &color);
            }
            DrawableChar::LowerOneQuarterBlock => {
                // Lower One Quarter Block (▂) - fills bottom 1/4 of the cell
                let block_height = line_height / 4.0;
                let block_rect = Rect {
                    x,
                    y: y + line_height - block_height, // Position at bottom 1/4
                    width: line_width,
                    height: block_height,
                };
                self.add_rect(&block_rect, depth, &color);
            }
            DrawableChar::LowerThreeEighthsBlock => {
                // Lower Three Eighths Block (▃) - fills bottom 3/8 of the cell
                let block_height = (line_height * 3.0) / 8.0;
                let block_rect = Rect {
                    x,
                    y: y + line_height - block_height, // Position at bottom 3/8
                    width: line_width,
                    height: block_height,
                };
                self.add_rect(&block_rect, depth, &color);
            }

            DrawableChar::LeftOneQuarterBlock => {
                // Left One Quarter Block (▎) - fills left 1/4 of the cell
                let block_width = line_width / 4.0;
                let block_rect = Rect {
                    x,
                    y,
                    width: block_width,
                    height: line_height,
                };
                self.add_rect(&block_rect, depth, &color);
            }
            DrawableChar::LeftThreeEighthsBlock => {
                // Left Three Eighths Block (▍) - fills left 3/8 of the cell
                let block_width = (line_width * 3.0) / 8.0;
                let block_rect = Rect {
                    x,
                    y,
                    width: block_width,
                    height: line_height,
                };
                self.add_rect(&block_rect, depth, &color);
            }
            DrawableChar::LeftThreeQuartersBlock => {
                // Left Three Quarters Block (▊) - fills left 3/4 of the cell
                let block_width = (line_width * 3.0) / 4.0;
                let block_rect = Rect {
                    x,
                    y,
                    width: block_width,
                    height: line_height,
                };
                self.add_rect(&block_rect, depth, &color);
            }
            DrawableChar::RightOneQuarterBlock => {
                // Right One Quarter Block (▕) - fills right 1/4 of the cell
                let block_width = line_width / 4.0;
                let block_rect = Rect {
                    x: x + line_width - block_width, // Position at right 1/4
                    y,
                    width: block_width,
                    height: line_height,
                };
                self.add_rect(&block_rect, depth, &color);
            }

            DrawableChar::RightThreeEighthsBlock => {
                // Right Three Eighths Block (🮈) - fills right 3/8 of the cell
                let block_width = (line_width * 3.0) / 8.0;
                let block_rect = Rect {
                    x: x + line_width - block_width, // Position at right 3/8
                    y,
                    width: block_width,
                    height: line_height,
                };
                self.add_rect(&block_rect, depth, &color);
            }
            DrawableChar::RightThreeQuartersBlock => {
                // Right Three Quarters Block (🮊) - fills right 3/4 of the cell
                let block_width = (line_width * 3.0) / 4.0;
                let block_rect = Rect {
                    x: x + line_width - block_width, // Position at right 3/4
                    y,
                    width: block_width,
                    height: line_height,
                };
                self.add_rect(&block_rect, depth, &color);
            }
            DrawableChar::UpperOneEighthBlock => {
                // Upper One Eighth Block (▔) - fills top 1/8 of the cell
                let block_height = line_height / 8.0;
                let block_rect = Rect {
                    x,
                    y, // Position at top
                    width: line_width,
                    height: block_height,
                };
                self.add_rect(&block_rect, depth, &color);
            }
            DrawableChar::UpperThreeEighthsBlock => {
                // Upper Three Eighths Block (🮃) - fills top 3/8 of the cell
                let block_height = (line_height * 3.0) / 8.0;
                let block_rect = Rect {
                    x,
                    y, // Position at top
                    width: line_width,
                    height: block_height,
                };
                self.add_rect(&block_rect, depth, &color);
            }
            DrawableChar::UpperThreeQuartersBlock => {
                // Upper Three Quarters Block (🮅) - fills top 3/4 of the cell
                let block_height = (line_height * 3.0) / 4.0;
                let block_rect = Rect {
                    x,
                    y, // Position at top
                    width: line_width,
                    height: block_height,
                };
                self.add_rect(&block_rect, depth, &color);
            }
            DrawableChar::QuadrantUpperLeft => {
                let rect = Rect {
                    x,
                    y: center_y - line_height / 2.0,
                    width: line_width / 2.0,
                    height: line_height / 2.0,
                };
                self.add_rect(&rect, depth, &color);
            }
            DrawableChar::QuadrantUpperRight => {
                let rect = Rect {
                    x: x + line_width / 2.0,
                    y: center_y - line_height / 2.0,
                    width: line_width / 2.0,
                    height: line_height / 2.0,
                };
                self.add_rect(&rect, depth, &color);
            }
            DrawableChar::QuadrantLowerLeft => {
                let rect = Rect {
                    x,
                    y: center_y,
                    width: line_width / 2.0,
                    height: line_height / 2.0,
                };
                self.add_rect(&rect, depth, &color);
            }
            DrawableChar::QuadrantLowerRight => {
                let rect = Rect {
                    x: x + line_width / 2.0,
                    y: center_y,
                    width: line_width / 2.0,
                    height: line_height / 2.0,
                };
                self.add_rect(&rect, depth, &color);
            }
            DrawableChar::UpperHalf => {
                let rect = Rect {
                    x,
                    y: center_y - line_height / 2.0,
                    width: line_width,
                    height: line_height / 2.0,
                };
                self.add_rect(&rect, depth, &color);
            }
            DrawableChar::LowerHalf => {
                let rect = Rect {
                    x,
                    y: center_y,
                    width: line_width,
                    height: line_height / 2.0,
                };
                self.add_rect(&rect, depth, &color);
            }
            DrawableChar::LeftHalf => {
                let rect = Rect {
                    x,
                    y: center_y - line_height / 2.0,
                    width: line_width / 2.0,
                    height: line_height,
                };
                self.add_rect(&rect, depth, &color);
            }
            DrawableChar::RightHalf => {
                let rect = Rect {
                    x: x + line_width / 2.0,
                    y: center_y - line_height / 2.0,
                    width: line_width / 2.0,
                    height: line_height,
                };
                self.add_rect(&rect, depth, &color);
            }
            DrawableChar::DownDoubleAndHorizontalSingle => {
                // Calculate spacing between the two vertical lines
                let gap = stroke * 1.5; // Adjust this value as needed

                // Left vertical line - goes all the way down
                let left_rect = Rect {
                    x: center_x - gap,
                    y: center_y,
                    width: stroke,
                    height: line_height / 2.0, // Only the bottom half
                };

                // Right vertical line - goes all the way down
                let right_rect = Rect {
                    x: center_x + gap - stroke,
                    y: center_y,
                    width: stroke,
                    height: line_height / 2.0, // Only the bottom half
                };

                // Horizontal single line
                let horiz_rect = Rect {
                    x,
                    y: center_y - (stroke / 2.0),
                    width: line_width,
                    height: stroke,
                };

                // Draw all rectangles
                self.add_rect(&left_rect, depth, &color);
                self.add_rect(&right_rect, depth, &color);
                self.add_rect(&horiz_rect, depth, &color);
            }
            DrawableChar::DownSingleAndHorizontalDouble => {
                // Calculate spacing between the double horizontal lines
                let gap = stroke * 1.5; // Adjust this value as needed

                // Single vertical line going down from center
                let vertical_rect = Rect {
                    x: center_x - (stroke / 2.0),
                    y: center_y + gap,
                    width: stroke,
                    height: (line_height / 2.0) - gap, // Bottom half
                };

                // Double horizontal lines
                let top_horizontal_rect = Rect {
                    x,
                    y: center_y - gap,
                    width: line_width,
                    height: stroke,
                };

                let bottom_horizontal_rect = Rect {
                    x,
                    y: center_y + gap - stroke,
                    width: line_width,
                    height: stroke,
                };

                // Draw all rectangles
                self.add_rect(&vertical_rect, depth, &color);
                self.add_rect(&top_horizontal_rect, depth, &color);
                self.add_rect(&bottom_horizontal_rect, depth, &color);
            }
            DrawableChar::DoubleUpAndRight => {
                // Calculate spacing between the double lines
                let gap = stroke * 1.5; // Adjust this value as needed

                // Vertical double lines going up from center
                let left_vertical_rect = Rect {
                    x: center_x - gap,
                    y,
                    width: stroke,
                    height: line_height / 2.0, // Top half
                };

                let right_vertical_rect = Rect {
                    x: center_x + gap - stroke,
                    y,
                    width: stroke,
                    height: line_height / 2.0, // Top half
                };

                // Horizontal double lines going right from center
                let top_horizontal_rect = Rect {
                    x: center_x,
                    y: center_y - gap,
                    width: line_width / 2.0, // Right half
                    height: stroke,
                };

                let bottom_horizontal_rect = Rect {
                    x: center_x,
                    y: center_y + gap - stroke,
                    width: line_width / 2.0, // Right half
                    height: stroke,
                };

                // Draw all rectangles
                self.add_rect(&left_vertical_rect, depth, &color);
                self.add_rect(&right_vertical_rect, depth, &color);
                self.add_rect(&top_horizontal_rect, depth, &color);
                self.add_rect(&bottom_horizontal_rect, depth, &color);
            }
            DrawableChar::DoubleUpAndLeft => {
                // Calculate spacing between the double lines
                let gap = stroke * 1.5; // Adjust this value as needed

                // Vertical double lines going up from center
                let left_vertical_rect = Rect {
                    x: center_x - gap,
                    y,
                    width: stroke,
                    height: line_height / 2.0, // Top half
                };

                let right_vertical_rect = Rect {
                    x: center_x + gap - stroke,
                    y,
                    width: stroke,
                    height: line_height / 2.0, // Top half
                };

                // Horizontal double lines going left from center
                let top_horizontal_rect = Rect {
                    x,
                    y: center_y - gap,
                    width: line_width / 2.0, // Left half
                    height: stroke,
                };

                let bottom_horizontal_rect = Rect {
                    x,
                    y: center_y + gap - stroke,
                    width: line_width / 2.0, // Left half
                    height: stroke,
                };

                // Draw all rectangles
                self.add_rect(&left_vertical_rect, depth, &color);
                self.add_rect(&right_vertical_rect, depth, &color);
                self.add_rect(&top_horizontal_rect, depth, &color);
                self.add_rect(&bottom_horizontal_rect, depth, &color);
            }
            DrawableChar::UpSingleAndRightDouble => {
                // Calculate spacing between the double horizontal lines
                let gap = stroke * 1.5; // Adjust this value as needed

                // Single vertical line going up from center
                let vertical_rect = Rect {
                    x: center_x - (stroke / 2.0),
                    y,
                    width: stroke,
                    height: line_height / 2.0, // Top half
                };

                // Double horizontal lines going right from center
                let top_horizontal_rect = Rect {
                    x: center_x,
                    y: center_y - gap,
                    width: line_width / 2.0, // Right half
                    height: stroke,
                };

                let bottom_horizontal_rect = Rect {
                    x: center_x,
                    y: center_y + gap - stroke,
                    width: line_width / 2.0, // Right half
                    height: stroke,
                };

                // Draw all rectangles
                self.add_rect(&vertical_rect, depth, &color);
                self.add_rect(&top_horizontal_rect, depth, &color);
                self.add_rect(&bottom_horizontal_rect, depth, &color);
            }
            DrawableChar::UpSingleAndLeftDouble => {
                // Calculate spacing between the double horizontal lines
                let gap = stroke * 1.5; // Adjust this value as needed

                // Single vertical line going up from center
                let vertical_rect = Rect {
                    x: center_x - (stroke / 2.0),
                    y,
                    width: stroke,
                    height: line_height / 2.0, // Top half
                };

                // Double horizontal lines going left from center
                let top_horizontal_rect = Rect {
                    x,
                    y: center_y - gap,
                    width: line_width / 2.0, // Left half
                    height: stroke,
                };

                let bottom_horizontal_rect = Rect {
                    x,
                    y: center_y + gap - stroke,
                    width: line_width / 2.0, // Left half
                    height: stroke,
                };

                // Draw all rectangles
                self.add_rect(&vertical_rect, depth, &color);
                self.add_rect(&top_horizontal_rect, depth, &color);
                self.add_rect(&bottom_horizontal_rect, depth, &color);
            }
            DrawableChar::VerticalSingleAndHorizontalDouble => {
                // Calculate spacing between the double horizontal lines
                let gap = stroke * 1.5; // Adjust this value as needed

                // Single vertical line going through the full height
                let vertical_rect = Rect {
                    x: center_x - (stroke / 2.0),
                    y,
                    width: stroke,
                    height: line_height,
                };

                // Double horizontal lines going across the full width
                let top_horizontal_rect = Rect {
                    x,
                    y: center_y - gap,
                    width: line_width,
                    height: stroke,
                };

                let bottom_horizontal_rect = Rect {
                    x,
                    y: center_y + gap - stroke,
                    width: line_width,
                    height: stroke,
                };

                // Draw all rectangles
                self.add_rect(&vertical_rect, depth, &color);
                self.add_rect(&top_horizontal_rect, depth, &color);
                self.add_rect(&bottom_horizontal_rect, depth, &color);
            }
            DrawableChar::LightShade => {
                // For light shade (25% filled), create a sparse dot pattern
                // (░)
                let dot_size = stroke;
                let cols = 4;
                let rows = 8;
                let cell_width = line_width / cols as f32;
                let cell_height = line_height / rows as f32;

                for j in 0..rows {
                    for i in 0..cols {
                        // Place dots in alternating positions:
                        // If row is even (0, 2), place dots at even columns (0, 2)
                        // If row is odd (1, 3), place dots at odd columns (1, 3)
                        if (j % 2 == 0 && i % 2 == 0) || (j % 2 == 1 && i % 2 == 1) {
                            let dot_x =
                                x + i as f32 * cell_width + (cell_width - dot_size) / 2.0;
                            let dot_y = center_y - line_height / 2.0
                                + j as f32 * cell_height
                                + (cell_height - dot_size) / 2.0;

                            let rect = Rect {
                                x: dot_x,
                                y: dot_y,
                                width: dot_size,
                                height: dot_size,
                            };
                            self.add_rect(&rect, depth, &color);
                        }
                    }
                }
            }
            DrawableChar::MediumShade => {
                // For medium shade (50% filled), create a denser pattern
                // (▒)
                let dot_size = stroke;
                let cols = 4;
                let rows = 8;
                let cell_width = line_width / cols as f32;
                let cell_height = line_height / rows as f32;

                // First layer - same as light shade
                for j in 0..rows {
                    for i in 0..cols {
                        if (j % 2 == 0 && i % 2 == 0) || (j % 2 == 1 && i % 2 == 1) {
                            let dot_x =
                                x + i as f32 * cell_width + (cell_width - dot_size) / 2.0;
                            let dot_y = center_y - line_height / 2.0
                                + j as f32 * cell_height
                                + (cell_height - dot_size) / 2.0;
                            let rect = Rect {
                                x: dot_x,
                                y: dot_y,
                                width: dot_size,
                                height: dot_size,
                            };
                            self.add_rect(&rect, depth, &color);
                        }
                    }
                }

                // Second layer - offset pattern at half the size for medium shade
                let small_dot_size = dot_size * 0.75;
                for j in 0..rows {
                    for i in 0..cols {
                        if (j % 2 == 1 && i % 2 == 0) || (j % 2 == 0 && i % 2 == 1) {
                            let dot_x = x
                                + i as f32 * cell_width
                                + (cell_width - small_dot_size) / 2.0;
                            let dot_y = center_y - line_height / 2.0
                                + j as f32 * cell_height
                                + (cell_height - small_dot_size) / 2.0;
                            let rect = Rect {
                                x: dot_x,
                                y: dot_y,
                                width: small_dot_size,
                                height: small_dot_size,
                            };
                            self.add_rect(&rect, depth, &color);
                        }
                    }
                }
            }
            DrawableChar::DarkShade => {
                // For dark shade (75% filled)
                // (▓)
                let dot_size = stroke;
                let cols = 4;
                let rows = 8;
                let cell_width = line_width / cols as f32;
                let cell_height = line_height / rows as f32;

                // Base layer - fill the entire rectangle with a semi-transparent color
                let rect = Rect {
                    x,
                    y: center_y - line_height / 2.0,
                    width: line_width,
                    height: line_height,
                };
                let base_color = [
                    color[0] * 0.6,
                    color[1] * 0.6,
                    color[2] * 0.6,
                    color[3] * 0.6,
                ];
                self.add_rect(&rect, depth + 0.0001, &base_color);

                // Add dots everywhere
                for j in 0..rows {
                    for i in 0..cols {
                        let dot_x =
                            x + i as f32 * cell_width + (cell_width - dot_size) / 2.0;
                        let dot_y = center_y - line_height / 2.0
                            + j as f32 * cell_height
                            + (cell_height - dot_size) / 2.0;
                        let rect = Rect {
                            x: dot_x,
                            y: dot_y,
                            width: dot_size,
                            height: dot_size,
                        };
                        self.add_rect(&rect, depth, &color);

                        // Skip some dots to create tiny gaps (only in a few positions)
                        if j % 4 == 0 && i % 4 == 0 {
                            // This creates small gaps in a regular pattern
                            continue;
                        }
                    }
                }
            }
            DrawableChar::FullBlock => {
                let rect = Rect {
                    x,
                    y: center_y - line_height / 2.0,
                    width: line_width,
                    height: line_height,
                };
                self.add_rect(&rect, depth, &color);
            }
            DrawableChar::Cross => {
                // Horizontal part
                let rect_h = Rect {
                    x,
                    y: center_y - stroke / 2.0,
                    width: line_width,
                    height: stroke,
                };
                self.add_rect(&rect_h, depth, &color);

                // Vertical part
                let rect_v = Rect {
                    x: center_x - stroke / 2.0,
                    y,
                    width: stroke,
                    height: line_height,
                };
                self.add_rect(&rect_v, depth, &color);
            }
            DrawableChar::TopRight => {
                // Horizontal part (from center to right)
                let vertical_rect = Rect {
                    x: center_x - stroke / 2.0,
                    y,
                    width: stroke,
                    height: (line_height / 2.0) + (stroke / 2.0),
                };
                self.add_rect(&vertical_rect, depth, &color);

                // Horizontal line from left to center
                let horizontal_rect = Rect {
                    x: center_x,
                    y: center_y - stroke / 2.0,
                    width: line_width / 2.0,
                    height: stroke,
                };
                self.add_rect(&horizontal_rect, depth, &color);
            }
            DrawableChar::TopLeft => {
                let vertical_rect = Rect {
                    x: center_x - stroke / 2.0,
                    y,
                    width: stroke,
                    height: (line_height / 2.0) + (stroke / 2.0),
                };
                self.add_rect(&vertical_rect, depth, &color);

                // Horizontal line from left to center
                let horizontal_rect = Rect {
                    x,
                    y: center_y - stroke / 2.0,
                    width: half_size,
                    height: stroke,
                };
                self.add_rect(&horizontal_rect, depth, &color);
            }
            DrawableChar::BottomRight => {
                // Horizontal part (from center to right)
                let rect_h = Rect {
                    x: center_x,
                    y: center_y - stroke / 2.0,
                    width: half_size,
                    height: stroke,
                };
                self.add_rect(&rect_h, depth, &color);

                // Vertical part (from center to bottom)
                let rect_v = Rect {
                    x: center_x - stroke / 2.0,
                    y: center_y,
                    width: stroke,
                    height: line_height / 2.0,
                };
                self.add_rect(&rect_v, depth, &color);
            }
            DrawableChar::BottomLeft => {
                // Horizontal part (from left to center)
                let rect_h = Rect {
                    x,
                    y: center_y - stroke / 2.0,
                    width: half_size,
                    height: stroke,
                };
                self.add_rect(&rect_h, depth, &color);

                // Vertical part (from center to bottom)
                let rect_v = Rect {
                    x: center_x - stroke / 2.0,
                    y: center_y,
                    width: stroke,
                    height: line_height / 2.0,
                };
                self.add_rect(&rect_v, depth, &color);
            }
            DrawableChar::ArcTopLeft => {
                // Arc corner at bottom-right (╯)
                // Vertical line from top to center
                let radius = line_width / 4.0;
                let vertical_rect = Rect {
                    x: center_x - stroke / 2.0,
                    y,
                    width: stroke,
                    height: (line_height / 2.0) - radius,
                };
                self.add_rect(&vertical_rect, depth, &color);

                // Horizontal line from left to center
                let horizontal_rect = Rect {
                    x,
                    y: center_y - stroke / 2.0,
                    width: (line_width / 2.0) - radius,
                    height: stroke,
                };
                self.add_rect(&horizontal_rect, depth, &color);

                // Arc in the bottom-left quarter (connecting horizontal and vertical lines)
                self.add_arc(
                    center_x - radius,
                    center_y - radius,
                    line_width / 4.0, // Smaller radius for better appearance
                    0.0,              // Start angle
                    90.0,             // End angle (quarter circle)
                    stroke,
                    depth,
                    &color,
                );
            }
            DrawableChar::ArcBottomRight => {
                // Arc corner at top-left (┌)
                // Vertical line from center to bottom
                let radius = line_width / 4.0;
                let vertical_rect = Rect {
                    x: center_x - stroke / 2.0,
                    y: center_y + radius,
                    width: stroke,
                    height: (line_height / 2.0) - radius,
                };
                self.add_rect(&vertical_rect, depth, &color);
                // Horizontal line from center to right
                let horizontal_rect = Rect {
                    x: center_x + radius,
                    y: center_y - stroke / 2.0,
                    width: (line_width / 2.0) - radius,
                    height: stroke,
                };
                self.add_rect(&horizontal_rect, depth, &color);
                // Arc in the top-left quarter (connecting horizontal and vertical lines)
                self.add_arc(
                    center_x + radius,
                    center_y + radius,
                    radius, // Smaller radius for better appearance
                    180.0,  // Start angle
                    270.0,  // End angle (quarter circle)
                    stroke,
                    depth,
                    &color,
                );
            }

            DrawableChar::ArcBottomLeft => {
                // Arc corner at top-right (┐)
                // Vertical line from center to bottom
                let radius = line_width / 4.0;
                let vertical_rect = Rect {
                    x: center_x - stroke / 2.0,
                    y: center_y + radius,
                    width: stroke,
                    height: (line_height / 2.0) - radius,
                };
                self.add_rect(&vertical_rect, depth, &color);
                // Horizontal line from left to center
                let horizontal_rect = Rect {
                    x,
                    y: center_y - stroke / 2.0,
                    width: center_x - radius - x,
                    height: stroke,
                };
                self.add_rect(&horizontal_rect, depth, &color);
                // Arc in the top-right quarter (connecting horizontal and vertical lines)
                self.add_arc(
                    center_x - radius,
                    center_y + radius,
                    radius, // Smaller radius for better appearance
                    270.0,  // Start angle
                    360.0,  // End angle (quarter circle)
                    stroke,
                    depth,
                    &color,
                );
            }
            DrawableChar::ArcTopRight => {
                // Arc corner at bottom-left (╰)
                // Vertical line from top to center
                let radius = line_width / 4.0;
                let vertical_rect = Rect {
                    x: center_x - stroke / 2.0,
                    y,
                    width: stroke,
                    height: center_y - radius - y,
                };
                self.add_rect(&vertical_rect, depth, &color);
                // Horizontal line from center to right
                let horizontal_rect = Rect {
                    x: center_x + radius,
                    y: center_y - stroke / 2.0,
                    width: (line_width / 2.0) - radius,
                    height: stroke,
                };
                self.add_rect(&horizontal_rect, depth, &color);
                // Arc in the bottom-right quarter (connecting horizontal and vertical lines)
                self.add_arc(
                    center_x + radius,
                    center_y - radius,
                    radius, // Smaller radius for better appearance
                    90.0,   // Start angle
                    180.0,  // End angle (quarter circle)
                    stroke,
                    depth,
                    &color,
                );
            }
            DrawableChar::VerticalRight => {
                // Vertical line
                let rect_v = Rect {
                    x: center_x - stroke / 2.0,
                    y,
                    width: stroke,
                    height: line_height,
                };
                self.add_rect(&rect_v, depth, &color);

                // Horizontal line (from center to right)
                let rect_h = Rect {
                    x: center_x + (stroke / 2.0),
                    y: center_y - stroke / 2.0,
                    width: half_size - (stroke / 2.0),
                    height: stroke,
                };
                self.add_rect(&rect_h, depth, &color);
            }
            DrawableChar::VerticalLeft => {
                // Vertical line
                let rect_v = Rect {
                    x: center_x - stroke / 2.0,
                    y,
                    width: stroke,
                    height: line_height,
                };
                self.add_rect(&rect_v, depth, &color);

                // Horizontal line (from left to center)
                let rect_h = Rect {
                    x,
                    y: center_y - stroke / 2.0,
                    width: half_size - (stroke / 2.0),
                    height: stroke,
                };
                self.add_rect(&rect_h, depth, &color);
            }
            DrawableChar::HorizontalDown => {
                // Horizontal line
                let rect_h = Rect {
                    x,
                    y: center_y - stroke / 2.0,
                    width: advance,
                    height: stroke,
                };
                self.add_rect(&rect_h, depth, &color);

                // Vertical line (from center to bottom)
                let rect_v = Rect {
                    x: center_x - stroke / 2.0,
                    y: center_y,
                    width: stroke,
                    height: line_height / 2.0,
                };
                self.add_rect(&rect_v, depth, &color);
            }
            DrawableChar::HorizontalUp => {
                // Horizontal line
                let rect_h = Rect {
                    x,
                    y: center_y - stroke / 2.0,
                    width: advance,
                    height: stroke,
                };
                self.add_rect(&rect_h, depth, &color);

                // Vertical line (from center to top)
                let rect_v = Rect {
                    x: center_x - stroke / 2.0,
                    y,
                    width: stroke,
                    height: line_height / 2.0,
                };
                self.add_rect(&rect_v, depth, &color);
            }
            DrawableChar::PowerlineLeftSolid => {
                // PowerlineLeftSolid - solid triangle pointing left
                // Creates a filled triangle pointing to the left
                self.add_triangle(
                    x + line_width,
                    y, // Top-right (x1, y1)
                    x + line_width,
                    y + line_height, // Bottom-right (x2, y2)
                    x,
                    y + line_height / 2.0, // Middle-left (x3, y3)
                    depth,
                    color,
                );
            }
            DrawableChar::PowerlineRightSolid => {
                // PowerlineRightSolid - solid triangle pointing right
                // Creates a filled triangle pointing to the right
                self.add_triangle(
                    x,
                    y, // Top-left (x1, y1)
                    x,
                    y + line_height, // Bottom-left (x2, y2)
                    x + line_width,
                    y + line_height / 2.0, // Middle-right (x3, y3)
                    depth,
                    color,
                );
            }
            DrawableChar::PowerlineLeftHollow => {
                // PowerlineLeftHollow - hollow triangle pointing left

                // Define stroke width for the hollow triangle outline
                let stroke_width = line_width * 0.1; // Adjust as needed for desired thickness

                // Top edge: from top-right to middle-left
                self.add_line(
                    x + line_width,
                    y, // Start point (top-right)
                    x,
                    y + line_height / 2.0, // End point (middle-left)
                    stroke_width,
                    depth,
                    color,
                );

                // Bottom edge: from middle-left to bottom-right
                self.add_line(
                    x,
                    y + line_height / 2.0, // Start point (middle-left)
                    x + line_width,
                    y + line_height, // End point (bottom-right)
                    stroke_width,
                    depth,
                    color,
                );

                // // Right edge: from bottom-right to top-right
                // self.add_line(
                //     x + line_width,
                //     y + line_height, // Start point (bottom-right)
                //     x + line_width,
                //     y, // End point (top-right)
                //     stroke_width,
                //     depth,
                //     &color,
                // );
            }
            DrawableChar::PowerlineRightHollow => {
                // PowerlineRightHollow - hollow triangle pointing right

                // Define stroke width for the hollow triangle outline
                let stroke_width = line_width * 0.1; // Adjust as needed for desired thickness

                // Top edge: from top-left to middle-right
                self.add_line(
                    x,
                    y, // Start point (top-left)
                    x + line_width,
                    y + line_height / 2.0, // End point (middle-right)
                    stroke_width,
                    depth,
                    color,
                );

                // Bottom edge: from middle-right to bottom-left
                self.add_line(
                    x + line_width,
                    y + line_height / 2.0, // Start point (middle-right)
                    x,
                    y + line_height, // End point (bottom-left)
                    stroke_width,
                    depth,
                    color,
                );

                // Left edge: from bottom-left to top-left
                // self.add_line(
                //     x,
                //     y + line_height, // Start point (bottom-left)
                //     x,
                //     y, // End point (top-left)
                //     stroke_width,
                //     depth,
                //     &color,
                // );
            }
            DrawableChar::PowerlineCurvedLeftSolid => {
                // Number of segments to create a smooth curve
                let segments = 60;
                // Create points for the polygon
                let mut points = Vec::with_capacity(segments + 2);
                // Add the right side points first (straight edge)
                points.push((x + line_width, y)); // Top-right
                points.push((x + line_width, y + line_height)); // Bottom-right

                // Create the curved left side (half oval)
                for i in (0..=segments).rev() {
                    // Draw from bottom to top
                    let t = i as f32 / segments as f32; // Parameter from 0 to 1

                    // For a half oval, we use the parametric equation of an ellipse
                    // The horizontal radius is line_width
                    // The vertical radius is line_height/2

                    // Calculate y position (moving from bottom to top)
                    let y_pos = y + line_height * (1.0 - t);

                    // Calculate x position using the ellipse formula x = a * sqrt(1 - (y/b)²)
                    // Where a is the horizontal radius and b is the vertical radius
                    // We need to normalize y to be between -1 and 1 for the calculation
                    let normalized_y = 2.0 * t - 1.0;

                    // Calculate the x position based on the ellipse equation
                    let x_pos = x + line_width
                        - (line_width * (1.0 - normalized_y * normalized_y).sqrt());

                    points.push((x_pos, y_pos));
                }

                // Draw the filled polygon with all our points
                self.add_antialiased_polygon(&points, depth, color);
            }
            DrawableChar::PowerlineCurvedRightSolid => {
                // Use even higher segment count for ultra-smooth curve
                let segments = 180;
                let mut points = Vec::with_capacity(segments + 2);

                // Start with straight edge (left side)
                points.push((x, y)); // Top-left
                points.push((x, y + line_height)); // Bottom-left

                // Create an even distribution of points along the curve
                // Use a distribution that concentrates more points in the curved areas
                for i in 0..=segments {
                    // Use sine-based parameterization for better point distribution
                    // This gives more points where curvature is highest
                    let angle = std::f32::consts::PI * i as f32 / segments as f32;
                    let t = (1.0 - angle.cos()) / 2.0; // Smoother distribution between 0-1

                    // Calculate y position from bottom to top
                    let y_pos = y + line_height * (1.0 - t);

                    // Calculate x position using ellipse formula
                    // Using sine distribution gives better antialiasing at critical curve points
                    let normalized_y = 2.0 * t - 1.0;

                    // Ensure x coordinates exactly match the mathematical approach in left curve
                    // but mirrored for right side
                    let x_pos =
                        x + line_width * (1.0 - normalized_y * normalized_y).sqrt();

                    // Add a tiny adjustment to ensure perfect pixel alignment
                    let x_adj = x_pos + 0.001; // Subpixel adjustment can help with antialiasing

                    points.push((x_adj, y_pos));
                }

                // Draw the filled polygon with antialiasing
                self.add_antialiased_polygon(&points, depth, color);
            }
            DrawableChar::PowerlineCurvedLeftHollow => {
                // Number of segments to create a smooth curve
                let segments = 30;
                let line_thickness = stroke / 2.;

                // Draw the vertical line on the right side
                // self.add_line(
                //     x + line_width, y,
                //     x + line_width, y + line_height,
                //     line_thickness, depth, color
                // );

                // Draw the curved left side from top to bottom
                for i in 0..segments {
                    let t1 = i as f32 / segments as f32;
                    let t2 = (i + 1) as f32 / segments as f32;

                    // Calculate positions
                    let y1 = y + line_height * t1;
                    let y2 = y + line_height * t2;

                    // Calculate x positions
                    let normalized_t1 = 2.0 * t1 - 1.0;
                    let normalized_t2 = 2.0 * t2 - 1.0;

                    let x_factor1 = f32::sqrt(1.0 - normalized_t1 * normalized_t1);
                    let x_factor2 = f32::sqrt(1.0 - normalized_t2 * normalized_t2);

                    let x1 = x + (line_width * (1.0 - x_factor1));
                    let x2 = x + (line_width * (1.0 - x_factor2));

                    // Draw segment of the curve
                    self.add_line(x1, y1, x2, y2, line_thickness, depth, color);
                }

                // Calculate endpoints for top and bottom
                let top_normalized_t = -1.0; // t=0 gives normalized_t = -1
                let bottom_normalized_t = 1.0; // t=1 gives normalized_t = 1

                let top_x_factor = f32::sqrt(1.0 - top_normalized_t * top_normalized_t);
                let bottom_x_factor =
                    f32::sqrt(1.0 - bottom_normalized_t * bottom_normalized_t);

                let top_x = x + (line_width * (1.0 - top_x_factor));
                let bottom_x = x + (line_width * (1.0 - bottom_x_factor));

                // Draw the horizontal line at the top
                self.add_line(top_x, y, x + line_width, y, line_thickness, depth, color);

                // Draw the horizontal line at the bottom
                self.add_line(
                    bottom_x,
                    y + line_height,
                    x + line_width,
                    y + line_height,
                    line_thickness,
                    depth,
                    color,
                );
            }
            DrawableChar::PowerlineCurvedRightHollow => {
                // Number of segments to create a smooth curve
                let segments = 30;
                let line_thickness = stroke / 2.;

                // Draw the vertical line on the left side
                // self.add_line(
                //     x,
                //     y,
                //     x,
                //     y + line_height,
                //     line_thickness,
                //     depth,
                //     color,
                // );

                // Draw the curved right side from top to bottom
                for i in 0..segments {
                    let t1 = i as f32 / segments as f32;
                    let t2 = (i + 1) as f32 / segments as f32;

                    // Calculate positions
                    let y1 = y + line_height * t1;
                    let y2 = y + line_height * t2;

                    // Calculate x positions - flipped from left version
                    let normalized_t1 = 2.0 * t1 - 1.0;
                    let normalized_t2 = 2.0 * t2 - 1.0;

                    let x_factor1 = f32::sqrt(1.0 - normalized_t1 * normalized_t1);
                    let x_factor2 = f32::sqrt(1.0 - normalized_t2 * normalized_t2);

                    // For right curve, we add the factor instead of subtracting
                    let x1 = x + (line_width * x_factor1);
                    let x2 = x + (line_width * x_factor2);

                    // Draw segment of the curve
                    self.add_line(x1, y1, x2, y2, line_thickness, depth, color);
                }

                // Calculate endpoints for top and bottom
                let top_normalized_t = -1.0; // t=0 gives normalized_t = -1
                let bottom_normalized_t = 1.0; // t=1 gives normalized_t = 1

                let top_x_factor = f32::sqrt(1.0 - top_normalized_t * top_normalized_t);
                let bottom_x_factor =
                    f32::sqrt(1.0 - bottom_normalized_t * bottom_normalized_t);

                // For right curve, we add the factor instead of subtracting
                let top_x = x + (line_width * top_x_factor);
                let bottom_x = x + (line_width * bottom_x_factor);

                // Draw the horizontal line at the top
                self.add_line(x, y, top_x, y, line_thickness, depth, color);

                // Draw the horizontal line at the bottom
                self.add_line(
                    x,
                    y + line_height,
                    bottom_x,
                    y + line_height,
                    line_thickness,
                    depth,
                    color,
                );
            }
            DrawableChar::PowerlineLowerLeftTriangle => {
                // POWERLINE_EXTRA_LOWER_LEFT_TRIANGLE - solid triangle pointing to lower left
                self.add_triangle(
                    x,
                    y + line_height, // Bottom-left (x1, y1)
                    x + line_width,
                    y + line_height, // Bottom-right (x2, y2)
                    x,
                    y, // Top-left (x3, y3)
                    depth,
                    color,
                );
            }
            DrawableChar::PowerlineBackslashSeparator => {
                // POWERLINE_EXTRA_BACKSLASH_SEPARATOR - diagonal line from top right to bottom left
                let stroke_width = line_width * 0.1; // Adjust thickness as needed
                self.add_line(
                    x + line_width,
                    y, // Start point (top-right)
                    x,
                    y + line_height, // End point (bottom-left)
                    stroke_width,
                    depth,
                    color,
                );
            }
            DrawableChar::PowerlineLowerRightTriangle => {
                // POWERLINE_EXTRA_LOWER_RIGHT_TRIANGLE - solid triangle pointing to lower right
                self.add_triangle(
                    x,
                    y + line_height, // Bottom-left (x1, y1)
                    x + line_width,
                    y + line_height, // Bottom-right (x2, y2)
                    x + line_width,
                    y, // Top-right (x3, y3)
                    depth,
                    color,
                );
            }
            DrawableChar::PowerlineForwardslashSeparator => {
                // POWERLINE_EXTRA_FORWARDSLASH_SEPARATOR - diagonal line from top left to bottom right
                let stroke_width = line_width * 0.1; // Adjust thickness as needed
                self.add_line(
                    x,
                    y, // Start point (top-left)
                    x + line_width,
                    y + line_height, // End point (bottom-right)
                    stroke_width,
                    depth,
                    color,
                );
            }
            DrawableChar::PowerlineUpperLeftTriangle => {
                // POWERLINE_EXTRA_UPPER_LEFT_TRIANGLE - solid triangle pointing to upper left
                self.add_triangle(
                    x,
                    y, // Top-left (x1, y1)
                    x + line_width,
                    y, // Top-right (x2, y2)
                    x,
                    y + line_height, // Bottom-left (x3, y3)
                    depth,
                    color,
                );
            }
            DrawableChar::PowerlineForwardslashSeparatorRedundant => {
                // This appears to be another forward slash separator (redundant)
                // Using same implementation as PowerlineForwardslashSeparator
                let stroke_width = line_width * 0.1; // Adjust thickness as needed
                self.add_line(
                    x,
                    y, // Start point (top-left)
                    x + line_width,
                    y + line_height, // End point (bottom-right)
                    stroke_width,
                    depth,
                    color,
                );
            }
            DrawableChar::PowerlineUpperRightTriangle => {
                // POWERLINE_EXTRA_UPPER_RIGHT_TRIANGLE - solid triangle pointing to upper right
                self.add_triangle(
                    x,
                    y, // Top-left (x1, y1)
                    x + line_width,
                    y, // Top-right (x2, y2)
                    x + line_width,
                    y + line_height, // Bottom-right (x3, y3)
                    depth,
                    color,
                );
            }
            DrawableChar::PowerlineBackslashSeparatorRedundant => {
                // This appears to be another backslash separator (redundant)
                // Using same implementation as PowerlineBackslashSeparator
                let stroke_width = line_width * 0.1; // Adjust thickness as needed
                self.add_line(
                    x + line_width,
                    y, // Start point (top-right)
                    x,
                    y + line_height, // End point (bottom-left)
                    stroke_width,
                    depth,
                    color,
                );
            }
            DrawableChar::HorizontalLightDash => {
                // ┄ - Light dashed horizontal line with 3 dashes
                let dash_count = 3;
                let total_space = dash_count - 1;
                let dash_width =
                    (line_width - (total_space as f32 * stroke)) / dash_count as f32;

                for i in 0..dash_count {
                    let dash_x = x + (i as f32) * (dash_width + stroke);
                    let rect = Rect {
                        x: dash_x,
                        y: center_y - (stroke / 2.0),
                        width: dash_width,
                        height: stroke,
                    };
                    self.add_rect(&rect, depth, &color);
                }
            }
            DrawableChar::HorizontalHeavyDash => {
                // ┅ - Heavy dashed horizontal line with 3 dashes
                let heavy_stroke = stroke * 1.8;
                let dash_count = 3;
                let total_space = dash_count - 1;
                let dash_width =
                    (line_width - (total_space as f32 * stroke)) / dash_count as f32;

                for i in 0..dash_count {
                    let dash_x = x + (i as f32) * (dash_width + stroke);
                    let rect = Rect {
                        x: dash_x,
                        y: center_y - heavy_stroke / 2.0,
                        width: dash_width,
                        height: heavy_stroke,
                    };
                    self.add_rect(&rect, depth, &color);
                }
            }
            DrawableChar::HorizontalLightDoubleDash => {
                // ┈ - Light double-dashed horizontal line with 4 dashes
                let dash_count = 4;
                let total_space = dash_count - 1;
                let dash_width =
                    (line_width - (total_space as f32 * stroke)) / dash_count as f32;

                for i in 0..dash_count {
                    let dash_x = x + (i as f32) * (dash_width + stroke);
                    let rect = Rect {
                        x: dash_x,
                        y: center_y - (stroke / 2.0),
                        width: dash_width,
                        height: stroke,
                    };
                    self.add_rect(&rect, depth, &color);
                }
            }
            DrawableChar::HorizontalHeavyDoubleDash => {
                // ┉ - Heavy double-dashed horizontal line with 4 dashes
                let heavy_stroke = stroke * 1.8;
                let dash_count = 4;
                let total_space = dash_count - 1;
                let dash_width =
                    (line_width - (total_space as f32 * stroke)) / dash_count as f32;

                for i in 0..dash_count {
                    let dash_x = x + (i as f32) * (dash_width + stroke);
                    let rect = Rect {
                        x: dash_x,
                        y: center_y - heavy_stroke / 2.0,
                        width: dash_width,
                        height: heavy_stroke,
                    };
                    self.add_rect(&rect, depth, &color);
                }
            }
            DrawableChar::HorizontalLightTripleDash => {
                // ╌ - Light triple-dashed horizontal line with 2 dashes
                let dash_count = 2;
                let total_space = dash_count - 1;
                let dash_width =
                    (line_width - (total_space as f32 * stroke)) / dash_count as f32;

                for i in 0..dash_count {
                    let dash_x = x + (i as f32) * (dash_width + stroke);
                    let rect = Rect {
                        x: dash_x,
                        y: center_y - (stroke / 2.0),
                        width: dash_width,
                        height: stroke,
                    };
                    self.add_rect(&rect, depth, &color);
                }
            }
            DrawableChar::HorizontalHeavyTripleDash => {
                // ╍ - Heavy triple-dashed horizontal line with 2 dashes
                let heavy_stroke = stroke * 1.8;
                let dash_count = 2;
                let total_space = dash_count - 1;
                let dash_width =
                    (line_width - (total_space as f32 * stroke)) / dash_count as f32;

                for i in 0..dash_count {
                    let dash_x = x + (i as f32) * (dash_width + stroke);
                    let rect = Rect {
                        x: dash_x,
                        y: center_y - heavy_stroke / 2.0,
                        width: dash_width,
                        height: heavy_stroke,
                    };
                    self.add_rect(&rect, depth, &color);
                }
            }
            DrawableChar::VerticalLightDash => {
                // ┆ - Light dashed vertical line with 3 dashes
                let dash_count = 3;
                let total_space = dash_count - 1;
                let dash_height =
                    (line_height - (total_space as f32 * stroke)) / dash_count as f32;

                for i in 0..dash_count {
                    let dash_y = y + (i as f32) * (dash_height + stroke);
                    let rect = Rect {
                        x: center_x - (stroke / 2.0),
                        y: dash_y,
                        width: stroke,
                        height: dash_height,
                    };
                    self.add_rect(&rect, depth, &color);
                }
            }
            DrawableChar::VerticalHeavyDash => {
                // ┇ - Heavy dashed vertical line with 3 dashes
                let heavy_stroke = stroke * 1.8;
                let dash_count = 3;
                let total_space = dash_count - 1;
                let dash_height =
                    (line_height - (total_space as f32 * stroke)) / dash_count as f32;

                for i in 0..dash_count {
                    let dash_y = y + (i as f32) * (dash_height + stroke);
                    let rect = Rect {
                        x: center_x - heavy_stroke / 2.0,
                        y: dash_y,
                        width: heavy_stroke,
                        height: dash_height,
                    };
                    self.add_rect(&rect, depth, &color);
                }
            }
            DrawableChar::VerticalLightDoubleDash => {
                // ┊ - Light double-dashed vertical line with 4 dashes
                let dash_count = 4;
                let total_space = dash_count - 1;
                let dash_height =
                    (line_height - (total_space as f32 * stroke)) / dash_count as f32;

                for i in 0..dash_count {
                    let dash_y = y + (i as f32) * (dash_height + stroke);
                    let rect = Rect {
                        x: center_x - (stroke / 2.0),
                        y: dash_y,
                        width: stroke,
                        height: dash_height,
                    };
                    self.add_rect(&rect, depth, &color);
                }
            }
            DrawableChar::VerticalHeavyDoubleDash => {
                // ┋ - Heavy double-dashed vertical line with 4 dashes
                let heavy_stroke = stroke * 1.8;
                let dash_count = 4;
                let total_space = dash_count - 1;
                let dash_height =
                    (line_height - (total_space as f32 * stroke)) / dash_count as f32;

                for i in 0..dash_count {
                    let dash_y = y + (i as f32) * (dash_height + stroke);
                    let rect = Rect {
                        x: center_x - heavy_stroke / 2.0,
                        y: dash_y,
                        width: heavy_stroke,
                        height: dash_height,
                    };
                    self.add_rect(&rect, depth, &color);
                }
            }
            DrawableChar::VerticalLightTripleDash => {
                // ╎ - Light triple-dashed vertical line with 2 dashes
                let dash_count = 2;
                let total_space = dash_count - 1;
                let dash_height =
                    (line_height - (total_space as f32 * stroke)) / dash_count as f32;

                for i in 0..dash_count {
                    let dash_y = y + (i as f32) * (dash_height + stroke);
                    let rect = Rect {
                        x: center_x - (stroke / 2.0),
                        y: dash_y,
                        width: stroke,
                        height: dash_height,
                    };
                    self.add_rect(&rect, depth, &color);
                }
            }
            DrawableChar::VerticalHeavyTripleDash => {
                // ╏ - Heavy triple-dashed vertical line with 2 dashes
                let heavy_stroke = stroke * 1.8;
                let dash_count = 2;
                let total_space = dash_count - 1;
                let dash_height =
                    (line_height - (total_space as f32 * stroke)) / dash_count as f32;

                for i in 0..dash_count {
                    let dash_y = y + (i as f32) * (dash_height + stroke);
                    let rect = Rect {
                        x: center_x - heavy_stroke / 2.0,
                        y: dash_y,
                        width: heavy_stroke,
                        height: dash_height,
                    };
                    self.add_rect(&rect, depth, &color);
                }
            }
            // Separated Quadrants (slightly smaller with some padding)
            DrawableChar::SeparatedQuadrantUpperLeft => {
                // Separated upper left quadrant (🬓)
                let padding = line_width / 15.0;
                let quadrant_rect = Rect {
                    x: x + padding,
                    y: y + padding,
                    width: (line_width / 2.0) - (2.0 * padding),
                    height: (line_height / 2.0) - (2.0 * padding),
                };
                self.add_rect(&quadrant_rect, depth, &color);
            }
            DrawableChar::SeparatedQuadrantUpperRight => {
                // Separated upper right quadrant (🬔)
                let padding = line_width / 15.0;
                let quadrant_rect = Rect {
                    x: center_x + padding,
                    y: y + padding,
                    width: (line_width / 2.0) - (2.0 * padding),
                    height: (line_height / 2.0) - (2.0 * padding),
                };
                self.add_rect(&quadrant_rect, depth, &color);
            }
            DrawableChar::SeparatedQuadrantLowerLeft => {
                // Separated lower left quadrant (🬕)
                let padding = line_width / 15.0;
                let quadrant_rect = Rect {
                    x: x + padding,
                    y: center_y + padding,
                    width: (line_width / 2.0) - (2.0 * padding),
                    height: (line_height / 2.0) - (2.0 * padding),
                };
                self.add_rect(&quadrant_rect, depth, &color);
            }
            DrawableChar::SeparatedQuadrantLowerRight => {
                // Separated lower right quadrant (🬖)
                let padding = line_width / 15.0;
                let quadrant_rect = Rect {
                    x: center_x + padding,
                    y: center_y + padding,
                    width: (line_width / 2.0) - (2.0 * padding),
                    height: (line_height / 2.0) - (2.0 * padding),
                };
                self.add_rect(&quadrant_rect, depth, &color);
            }
            // Braille patterns
            DrawableChar::BrailleBlank => {
                // No dots to draw
            }
            DrawableChar::Braille(braille_dots) => {
                // Use stroke as the dot size base
                let dot_size = (stroke * 1.2).round();

                // Calculate cell dimensions
                let cell_width = advance;
                let cell_height = line_height;

                // Define the grid - 2×4 layout
                let grid_columns = 2;
                let grid_rows = 4;

                // Calculate single cell dimensions
                let cell_width_unit = cell_width / grid_columns as f32;
                let cell_height_unit = cell_height / grid_rows as f32;

                // Function to calculate dot position based on grid coordinates
                let get_dot_position = |col: usize, row: usize| -> (f32, f32) {
                    let dot_x =
                        x + (col as f32 * cell_width_unit) + (cell_width_unit / 2.0)
                            - (dot_size / 2.0);
                    let dot_y =
                        y + (row as f32 * cell_height_unit) + (cell_height_unit / 2.0)
                            - (dot_size / 2.0);
                    (dot_x, dot_y)
                };

                // Dot 1 (top-left): position [0,0]
                if contains_braille_dot(&braille_dots, 1) {
                    let (dot_x, dot_y) = get_dot_position(0, 0);
                    let dot_rect = Rect {
                        x: dot_x,
                        y: dot_y,
                        width: dot_size,
                        height: dot_size,
                    };
                    self.add_rect(&dot_rect, depth, &color);
                }

                // Dot 2 (middle-top-left): position [0,1]
                if contains_braille_dot(&braille_dots, 2) {
                    let (dot_x, dot_y) = get_dot_position(0, 1);
                    let dot_rect = Rect {
                        x: dot_x,
                        y: dot_y,
                        width: dot_size,
                        height: dot_size,
                    };
                    self.add_rect(&dot_rect, depth, &color);
                }

                // Dot 3 (middle-bottom-left): position [0,2]
                if contains_braille_dot(&braille_dots, 3) {
                    let (dot_x, dot_y) = get_dot_position(0, 2);
                    let dot_rect = Rect {
                        x: dot_x,
                        y: dot_y,
                        width: dot_size,
                        height: dot_size,
                    };
                    self.add_rect(&dot_rect, depth, &color);
                }

                // Dot 7 (bottom-left): position [0,3]
                if contains_braille_dot(&braille_dots, 7) {
                    let (dot_x, dot_y) = get_dot_position(0, 3);
                    let dot_rect = Rect {
                        x: dot_x,
                        y: dot_y,
                        width: dot_size,
                        height: dot_size,
                    };
                    self.add_rect(&dot_rect, depth, &color);
                }

                // Right column
                // Dot 4 (top-right): position [1,0]
                if contains_braille_dot(&braille_dots, 4) {
                    let (dot_x, dot_y) = get_dot_position(1, 0);
                    let dot_rect = Rect {
                        x: dot_x,
                        y: dot_y,
                        width: dot_size,
                        height: dot_size,
                    };
                    self.add_rect(&dot_rect, depth, &color);
                }

                // Dot 5 (middle-top-right): position [1,1]
                if contains_braille_dot(&braille_dots, 5) {
                    let (dot_x, dot_y) = get_dot_position(1, 1);
                    let dot_rect = Rect {
                        x: dot_x,
                        y: dot_y,
                        width: dot_size,
                        height: dot_size,
                    };
                    self.add_rect(&dot_rect, depth, &color);
                }

                // Dot 6 (middle-bottom-right): position [1,2]
                if contains_braille_dot(&braille_dots, 6) {
                    let (dot_x, dot_y) = get_dot_position(1, 2);
                    let dot_rect = Rect {
                        x: dot_x,
                        y: dot_y,
                        width: dot_size,
                        height: dot_size,
                    };
                    self.add_rect(&dot_rect, depth, &color);
                }

                // Dot 8 (bottom-right): position [1,3]
                if contains_braille_dot(&braille_dots, 8) {
                    let (dot_x, dot_y) = get_dot_position(1, 3);
                    let dot_rect = Rect {
                        x: dot_x,
                        y: dot_y,
                        width: dot_size,
                        height: dot_size,
                    };
                    self.add_rect(&dot_rect, depth, &color);
                }
            }
            DrawableChar::Octant(pattern) => {
                // Octants are on a 2×4 grid:
                // ╭───┬───╮
                // │ 0 │ 1 │
                // ├───┼───┤
                // │ 2 │ 3 │
                // ├───┼───┤
                // │ 4 │ 5 │
                // ├───┼───┤
                // │ 6 │ 7 │
                // ╰───┴───╯

                let cell_width = line_width / 2.0; // 2 columns
                let cell_height = line_height / 4.0; // 4 rows

                // Loop through each bit in the pattern
                for i in 0..8 {
                    // Check if this octant should be filled
                    if pattern & (1 << i) != 0 {
                        // Calculate the octant position
                        let row = i / 2; // integer division gives row (0-3)
                        let col = i % 2; // modulo gives column (0-1)

                        // Calculate the rectangle for this octant
                        let octant_rect = Rect {
                            x: x + (col as f32 * cell_width),
                            y: y + (row as f32 * cell_height),
                            width: cell_width,
                            height: cell_height,
                        };

                        self.add_rect(&octant_rect, depth, &color);
                    }
                }
            }
            DrawableChar::Sextant(pattern) => {
                // Sextants are on a 2×3 grid:
                // ╭───┬───╮
                // │ 0 │ 1 │
                // ├───┼───┤
                // │ 2 │ 3 │
                // ├───┼───┤
                // │ 4 │ 5 │
                // ╰───┴───╯

                let cell_width = line_width / 2.0; // 2 columns
                let cell_height = line_height / 3.0; // 3 rows

                // Unicode Block Sextant mapping:
                // The Unicode codepoints U+1FB00 to U+1FB3F represent different sextant combinations
                // The pattern value is the offset from U+1FB00, which encodes which sextants are filled

                // Loop through each bit in the pattern
                for i in 0..6 {
                    // Check if this sextant should be filled
                    if pattern & (1 << i) != 0 {
                        // Calculate the sextant position
                        let row = i / 2; // integer division gives row (0-2)
                        let col = i % 2; // modulo gives column (0-1)

                        // Calculate the rectangle for this sextant
                        let sextant_rect = Rect {
                            x: x + (col as f32 * cell_width),
                            y: y + (row as f32 * cell_height),
                            width: cell_width,
                            height: cell_height,
                        };

                        self.add_rect(&sextant_rect, depth, &color);
                    }
                }
            }
        }
    }

    #[inline]
    pub fn draw_underline(
        &mut self,
        underline: &RunUnderline,
        x: f32,
        advance: f32,
        baseline: f32,
        depth: f32,
        line_height: f32,
    ) {
        if underline.enabled {
            let ux = x;
            // Position underline below baseline by adding the calculated offset
            // This ensures proper underline placement in the descent area
            let uy = baseline + underline.offset;

            let end = x + advance;
            if ux < end {
                match underline.shape {
                    UnderlineShape::Regular => {
                        self.add_rect(
                            &Rect::new(ux, uy, end - ux, underline.size),
                            depth,
                            &underline.color,
                        );
                        if underline.is_doubled {
                            // Position the second underline with a gap equal to thickness
                            // First line is at uy, gap of underline.size, then second line
                            self.add_rect(
                                &Rect::new(
                                    ux,
                                    uy + (underline.size * 2.0),
                                    end - ux,
                                    underline.size,
                                ),
                                depth,
                                &underline.color,
                            );
                        }
                    }
                    UnderlineShape::Dashed => {
                        let mut start = ux;
                        while start < end {
                            start = start.min(end);
                            self.add_rect(
                                &Rect::new(start, uy, 6.0, underline.size),
                                depth,
                                &underline.color,
                            );
                            start += 8.0;
                        }
                    }
                    UnderlineShape::Dotted => {
                        let mut start = ux;
                        while start < end {
                            start = start.min(end);
                            self.add_rect(
                                &Rect::new(start, uy, 2.0, underline.size),
                                depth,
                                &underline.color,
                            );
                            start += 4.0;
                        }
                    }
                    UnderlineShape::Curly => {
                        // Create smooth curly underlines using triangles to form thick curved segments
                        let wave_amplitude = (line_height / 12.).clamp(0.9, 1.8); // Slightly reduced amplitude
                        let wave_frequency = 8.0; // pixels per complete wave cycle
                        let thickness = (line_height / 16.).clamp(1.0, 2.0);

                        let mut x = ux;
                        let step_size: f32 = 0.8; // Larger steps for triangle segments

                        while x < end - step_size {
                            let progress1 = (x - ux) / wave_frequency;
                            let progress2 = ((x + step_size) - ux) / wave_frequency;

                            let wave_phase1 = progress1 * std::f32::consts::PI * 2.0;
                            let wave_phase2 = progress2 * std::f32::consts::PI * 2.0;

                            // Calculate Y positions for current and next points
                            let y1 = uy + wave_phase1.sin() * wave_amplitude;
                            let y2 = uy + wave_phase2.sin() * wave_amplitude;

                            // Create thick line segment using two triangles (quad)
                            let half_thickness = thickness * 0.5;

                            // Top triangle of the quad
                            self.add_triangle(
                                x,
                                y1 - half_thickness, // top-left
                                x + step_size,
                                y2 - half_thickness, // top-right
                                x,
                                y1 + half_thickness, // bottom-left
                                depth,
                                underline.color,
                            );

                            // Bottom triangle of the quad
                            self.add_triangle(
                                x + step_size,
                                y2 - half_thickness, // top-right
                                x + step_size,
                                y2 + half_thickness, // bottom-right
                                x,
                                y1 + half_thickness, // bottom-left
                                depth,
                                underline.color,
                            );

                            x += step_size;
                        }
                    }
                }
            }
        }
    }

    #[inline]
    fn alloc_batch(&mut self, transparent: bool) -> &mut Batch {
        let batch = self.batches.pop().unwrap_or_default();
        if transparent {
            self.transparent.push(batch);
            self.transparent.last_mut().unwrap()
        } else {
            self.opaque.push(batch);
            self.opaque.last_mut().unwrap()
        }
    }
}
