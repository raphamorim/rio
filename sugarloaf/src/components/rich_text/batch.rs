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

use bytemuck::{Pod, Zeroable};

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
    fn build_display_list(&self, list: &mut DisplayList) {
        // Since vertices are already in draw order, we can just copy them
        list.vertices.extend_from_slice(&self.vertices);
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
    pub fn build_display_list(&self, list: &mut DisplayList) {
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

/// Resources and commands for drawing a composition.
#[derive(Default, Debug, Clone)]
pub struct DisplayList {
    pub vertices: Vec<Vertex>,
}

impl DisplayList {
    /// Creates a new empty display list.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears the display list.
    #[inline]
    pub fn clear(&mut self) {
        self.vertices.clear();
    }
}
