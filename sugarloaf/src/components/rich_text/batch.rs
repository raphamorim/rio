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
    indices: Vec<u32>,
    subpix: bool,
}

impl Batch {
    fn clear(&mut self) {
        self.image = None;
        self.mask = None;
        self.vertices.clear();
        self.indices.clear();
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
        let verts = [
            // First vertex (top left)
            Vertex {
                pos: [x1_top, y1_top, depth],
                color,
                uv: [0.0, 0.0],
                layers,
            },
            // Second vertex (top right)
            Vertex {
                pos: [x2_top, y2_top, depth],
                color: color,
                uv: [1.0, 0.0],
                layers,
            },
            // Third vertex (bottom right)
            Vertex {
                pos: [x2_bottom, y2_bottom, depth],
                color: color,
                uv: [1.0, 1.0],
                layers,
            },
            // Fourth vertex (bottom left)
            Vertex {
                pos: [x1_bottom, y1_bottom, depth],
                color: color,
                uv: [0.0, 1.0],
                layers,
            },
        ];

        // Add the vertices to the batch
        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&verts);

        // Add indices for two triangles forming the quad
        self.indices.extend_from_slice(&[
            base,
            base + 1,
            base + 2,
            base + 2,
            base + 3,
            base,
        ]);

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

        let verts = [
            // First vertex
            Vertex {
                pos: [x1, y1, depth],
                color,
                uv: [0.0, 0.0],
                layers,
            },
            // Second vertex
            Vertex {
                pos: [x2, y2, depth],
                color,
                uv: [0.0, 1.0],
                layers,
            },
            // Third vertex
            Vertex {
                pos: [x3, y3, depth],
                color,
                uv: [1.0, 0.0],
                layers,
            },
        ];

        // Add the vertices to the batch
        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&verts);

        // Add indices for the triangle
        self.indices.extend_from_slice(&[
            base,     // First vertex
            base + 1, // Second vertex
            base + 2, // Third vertex
        ]);

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

            // Create a quad (two triangles) for this segment
            let verts = [
                // Inner point at current angle
                Vertex {
                    pos: [inner_x1, inner_y1, depth],
                    color: *color,
                    uv: [0.0, 0.0],
                    layers,
                },
                // Inner point at next angle
                Vertex {
                    pos: [inner_x2, inner_y2, depth],
                    color: *color,
                    uv: [0.0, 1.0],
                    layers,
                },
                // Outer point at next angle
                Vertex {
                    pos: [outer_x2, outer_y2, depth],
                    color: *color,
                    uv: [1.0, 1.0],
                    layers,
                },
                // Outer point at current angle
                Vertex {
                    pos: [outer_x1, outer_y1, depth],
                    color: *color,
                    uv: [1.0, 0.0],
                    layers,
                },
            ];

            let base = self.vertices.len() as u32;
            self.vertices.extend_from_slice(&verts);

            // Add indices for two triangles forming a quad
            self.indices.extend_from_slice(&[
                base,
                base + 1,
                base + 2,
                base + 2,
                base + 3,
                base,
            ]);

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
        let verts = [
            Vertex {
                pos: [x, y, depth],
                color: *color,
                uv: [l, t],
                layers,
            },
            Vertex {
                pos: [x, y + h, depth],
                color: *color,
                uv: [l, b],
                layers,
            },
            Vertex {
                pos: [x + w, y + h, depth],
                color: *color,
                uv: [r, b],
                layers,
            },
            Vertex {
                pos: [x + w, y, depth],
                color: *color,
                uv: [r, t],
                layers,
            },
        ];
        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&verts);
        self.indices.extend_from_slice(&[
            base,
            base + 1,
            base + 2,
            base + 2,
            base,
            base + 3,
        ]);
    }

    #[inline]
    fn build_display_list(&self, list: &mut DisplayList) {
        let first_vertex = list.vertices.len() as u32;
        list.vertices.extend_from_slice(&self.vertices);
        list.indices
            .extend(self.indices.iter().map(|i| *i + first_vertex));
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
        let transparent = color[3] != 1.0;
        if transparent {
            for batch in &mut self.transparent {
                if batch.add_triangle(
                    x1, y1, x2, y2, x3, y3, color, depth, None,  // image
                    None,  // mask
                    false, // subpix
                ) {
                    return;
                }
            }
        } else {
            for batch in &mut self.opaque {
                if batch.add_triangle(
                    x1, y1, x2, y2, x3, y3, color, depth, None,  // image
                    None,  // mask
                    false, // subpix
                ) {
                    return;
                }
            }
        }
        self.alloc_batch(transparent).add_triangle(
            x1, y1, x2, y2, x3, y3, color, depth, None,  // image
            None,  // mask
            false, // subpix
        );
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
    pub indices: Vec<u32>,
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
        self.indices.clear();
    }
}
