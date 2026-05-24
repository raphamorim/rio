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

use crate::UnderlineShape;
use bytemuck::{Pod, Zeroable};

#[allow(dead_code)]
#[derive(Default, Clone, Copy)]
pub struct RunUnderline {
    pub enabled: bool,
    pub offset: f32,
    pub size: f32,
    pub color: [f32; 4],
    pub is_doubled: bool,
    pub shape: UnderlineShape,
}

/// Per-quad instance for instanced rendering (96 bytes).
/// One per quad — vertex shader generates 4 corners from vertex_id.
#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
pub struct QuadInstance {
    pub pos: [f32; 3],          // top-left x, y + depth (12)
    pub color: [f32; 4],        // fill / underline color (16)
    pub uv_rect: [f32; 4],      // [left, top, right, bottom] (16)
    pub layers: [i32; 2],       // [color_layer, mask_layer] (8)
    pub size: [f32; 2],         // width, height in pixels (8)
    pub corner_radii: [f32; 4], // [tl, tr, br, bl] (16)
    pub underline_style: i32,   // 0=none, 1=regular, 2=dashed, 3=dotted, 4=curly (4)
    pub clip_rect: [f32; 4],    // [x, y, w, h] physical pixels (16)
}

/// Vertex for non-quad geometry (lines, triangles, arcs).
#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub color: [f32; 4],
    pub uv: [f32; 2],
    pub layers: [i32; 2],
    pub corner_radii: [f32; 4],
    pub rect_size: [f32; 2],
    pub underline_style: i32,
    pub clip_rect: [f32; 4],
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
    instances: Vec<QuadInstance>,
    vertices: Vec<Vertex>,
    subpix: bool,
    order: u8,
}

impl Batch {
    fn clear(&mut self) {
        self.image = None;
        self.mask = None;
        self.instances.clear();
        self.vertices.clear();
        self.subpix = false;
        self.order = 0;
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.instances.is_empty() && self.vertices.is_empty()
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
        if !self.is_empty() && subpix != self.subpix {
            return false;
        }
        if !self.is_empty() && self.image != image {
            return false;
        }
        if !self.is_empty() && self.mask != mask {
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
            corner_radii: [0.0; 4],
            rect_size: [0.0, 0.0],
            underline_style: 0,
            clip_rect: [0.0; 4],
        };
        let v1 = Vertex {
            pos: [x2_top, y2_top, depth],
            color,
            uv: [1.0, 0.0],
            layers,
            corner_radii: [0.0; 4],
            rect_size: [0.0, 0.0],
            underline_style: 0,
            clip_rect: [0.0; 4],
        };
        let v2 = Vertex {
            pos: [x2_bottom, y2_bottom, depth],
            color,
            uv: [1.0, 1.0],
            layers,
            corner_radii: [0.0; 4],
            rect_size: [0.0, 0.0],
            underline_style: 0,
            clip_rect: [0.0; 4],
        };
        let v3 = Vertex {
            pos: [x1_bottom, y1_bottom, depth],
            color,
            uv: [0.0, 1.0],
            layers,
            corner_radii: [0.0; 4],
            rect_size: [0.0, 0.0],
            underline_style: 0,
            clip_rect: [0.0; 4],
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
        if !self.is_empty() && subpix != self.subpix {
            return false;
        }
        if !self.is_empty() && self.image != image {
            return false;
        }
        if !self.is_empty() && self.mask != mask {
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
            corner_radii: [0.0; 4],
            rect_size: [0.0, 0.0],
            underline_style: 0,
            clip_rect: [0.0; 4],
        });
        self.vertices.push(Vertex {
            pos: [x2, y2, depth],
            color,
            uv: [1.0, 0.0],
            layers,
            corner_radii: [0.0; 4],
            rect_size: [0.0, 0.0],
            underline_style: 0,
            clip_rect: [0.0; 4],
        });
        self.vertices.push(Vertex {
            pos: [x3, y3, depth],
            color,
            uv: [0.0, 1.0],
            layers,
            corner_radii: [0.0; 4],
            rect_size: [0.0, 0.0],
            underline_style: 0,
            clip_rect: [0.0; 4],
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
        if !self.is_empty() && subpix != self.subpix {
            return false;
        }
        if !self.is_empty() && self.image != image {
            return false;
        }
        if !self.is_empty() && self.mask != mask {
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
                corner_radii: [0.0; 4],
                rect_size: [0.0, 0.0],
                underline_style: 0,
                clip_rect: [0.0; 4],
            };
            let v1 = Vertex {
                pos: [inner_x2, inner_y2, depth],
                color: *color,
                uv: [0.0, 1.0],
                layers,
                corner_radii: [0.0; 4],
                rect_size: [0.0, 0.0],
                underline_style: 0,
                clip_rect: [0.0; 4],
            };
            let v2 = Vertex {
                pos: [outer_x2, outer_y2, depth],
                color: *color,
                uv: [1.0, 1.0],
                layers,
                corner_radii: [0.0; 4],
                rect_size: [0.0, 0.0],
                underline_style: 0,
                clip_rect: [0.0; 4],
            };
            let v3 = Vertex {
                pos: [outer_x1, outer_y1, depth],
                color: *color,
                uv: [1.0, 0.0],
                layers,
                corner_radii: [0.0; 4],
                rect_size: [0.0, 0.0],
                underline_style: 0,
                clip_rect: [0.0; 4],
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
    fn rect(
        &mut self,
        rect: &Rect,
        depth: f32,
        color: &[f32; 4],
        coords: Option<&[f32; 4]>,
        image: Option<i32>,
        mask: Option<i32>,
        subpix: bool,
        clip_rect: [f32; 4],
    ) -> bool {
        if !self.is_empty() && subpix != self.subpix {
            return false;
        }
        if !self.is_empty() && self.image != image {
            return false;
        }
        if !self.is_empty() && self.mask != mask {
            return false;
        }
        self.subpix = subpix;
        self.image = image;
        self.mask = mask;
        let layers = [self.image.unwrap_or(0), self.mask.unwrap_or(0)];
        self.push_rect(rect, depth, color, coords, layers, [0.0; 4], 0, clip_rect);
        true
    }

    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn rounded_rect(
        &mut self,
        rect: &Rect,
        depth: f32,
        color: &[f32; 4],
        coords: Option<&[f32; 4]>,
        image: Option<i32>,
        mask: Option<i32>,
        subpix: bool,
        corner_radii: [f32; 4],
        clip_rect: [f32; 4],
    ) -> bool {
        if !self.is_empty() && subpix != self.subpix {
            return false;
        }
        if !self.is_empty() && self.image != image {
            return false;
        }
        if !self.is_empty() && self.mask != mask {
            return false;
        }
        self.subpix = subpix;
        self.image = image;
        self.mask = mask;
        let layers = [self.image.unwrap_or(0), self.mask.unwrap_or(0)];
        self.push_rect(
            rect,
            depth,
            color,
            coords,
            layers,
            corner_radii,
            0,
            clip_rect,
        );
        true
    }

    /// Add an underline quad with GPU pattern rendering
    /// underline_style: 1 = regular, 2 = dashed, 3 = dotted, 4 = curly
    /// thickness: The actual line thickness (passed in corner_radii.x)
    #[inline]
    #[allow(dead_code)]
    fn underline(
        &mut self,
        rect: &Rect,
        depth: f32,
        color: &[f32; 4],
        underline_style: i32,
        thickness: f32,
        clip_rect: [f32; 4],
    ) -> bool {
        if !self.is_empty() && self.subpix {
            return false;
        }
        if !self.is_empty() && self.image.is_some() {
            return false;
        }
        if !self.is_empty() && self.mask.is_some() {
            return false;
        }
        self.subpix = false;
        self.image = None;
        self.mask = None;
        let layers = [0, 0];
        let corner_radii = [thickness, 0.0, 0.0, 0.0];
        self.push_rect(
            rect,
            depth,
            color,
            None,
            layers,
            corner_radii,
            underline_style,
            clip_rect,
        );
        true
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn push_rect(
        &mut self,
        rect: &Rect,
        depth: f32,
        color: &[f32; 4],
        coords: Option<&[f32; 4]>,
        layers: [i32; 2],
        corner_radii: [f32; 4],
        underline_style: i32,
        clip_rect: [f32; 4],
    ) {
        const DEFAULT_COORDS: [f32; 4] = [0., 0., 1., 1.];
        let coords = coords.unwrap_or(&DEFAULT_COORDS);

        self.instances.push(QuadInstance {
            pos: [rect.x, rect.y, depth],
            color: *color,
            uv_rect: *coords,
            layers,
            size: [rect.width, rect.height],
            corner_radii,
            underline_style,
            clip_rect,
        });
    }

    #[inline]
    fn build_display_list(
        &self,
        inst_list: &mut Vec<QuadInstance>,
        vert_list: &mut Vec<Vertex>,
        cmds: &mut Vec<DrawCmd>,
    ) {
        let color_layer = self.image.unwrap_or(0);
        let mask_layer = self.mask.unwrap_or(0);

        if !self.instances.is_empty() {
            let offset = inst_list.len() as u32;
            inst_list.extend_from_slice(&self.instances);
            cmds.push(DrawCmd::Instanced {
                offset,
                count: self.instances.len() as u32,
                color_layer,
                mask_layer,
            });
        }

        if !self.is_empty() {
            let offset = vert_list.len() as u32;
            vert_list.extend_from_slice(&self.vertices);
            cmds.push(DrawCmd::Vertices {
                offset,
                count: self.vertices.len() as u32,
                color_layer,
                mask_layer,
            });
        }
    }
}

/// Draw command emitted by the batch system.
// Fields are read by the wgpu/metal recorders; on Linux+no-wgpu only
// the Vulkan recorder runs and it reads them via `Debug` formatting in
// trace logs but not via field access — silence dead_code there.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum DrawCmd {
    /// Instanced quad draw (one instance per quad, 4 verts from vertex_id).
    Instanced {
        offset: u32,
        count: u32,
        color_layer: i32,
        mask_layer: i32,
    },
    /// Triangle-list draw for non-quad geometry (lines, triangles, arcs).
    Vertices {
        offset: u32,
        count: u32,
        color_layer: i32,
        mask_layer: i32,
    },
}

pub struct BatchManager {
    /// Pool of reusable batches
    pool: Vec<Batch>,
    /// Active batches (single list, sorted by draw order)
    active: Vec<Batch>,
    /// Current clip rect applied to all new vertices [x, y, w, h].
    /// [0,0,0,0] means no clipping.
    pub clip_rect: [f32; 4],
}

impl BatchManager {
    pub fn new() -> Self {
        Self {
            pool: Vec::new(),
            active: Vec::new(),
            clip_rect: [0.0; 4],
        }
    }

    #[inline]
    pub fn reset(&mut self) {
        self.pool.append(&mut self.active);
        for batch in &mut self.pool {
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
        let subpix = true;
        for batch in self.active.iter_mut() {
            if batch.order == 0
                && batch.add_triangle(
                    x1, y1, x2, y2, x3, y3, color, depth, None, None, subpix,
                )
            {
                return;
            }
        }
        self.alloc_batch(0)
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
        order: u8,
    ) {
        for batch in self.active.iter_mut() {
            if batch.order == order
                && batch.add_line(x1, y1, x2, y2, width, depth, color, None, None, false)
            {
                return;
            }
        }
        self.alloc_batch(order)
            .add_line(x1, y1, x2, y2, width, depth, color, None, None, false);
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
        for batch in self.active.iter_mut() {
            if batch.order == 0
                && batch.add_arc(
                    center_x,
                    center_y,
                    radius,
                    start_angle_deg,
                    end_angle_deg,
                    stroke_width,
                    depth,
                    color,
                    None,
                    None,
                    false,
                )
            {
                return;
            }
        }
        self.alloc_batch(0).add_arc(
            center_x,
            center_y,
            radius,
            start_angle_deg,
            end_angle_deg,
            stroke_width,
            depth,
            color,
            None,
            None,
            false,
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
    pub fn add_image_rect(
        &mut self,
        rect: &Rect,
        depth: f32,
        color: &[f32; 4],
        coords: &[f32; 4],
        atlas_layer: i32,
    ) {
        let cr = self.clip_rect;
        for batch in self.active.iter_mut() {
            if batch.order == 0
                && batch.rect(
                    rect,
                    depth,
                    color,
                    Some(coords),
                    Some(atlas_layer),
                    None,
                    false,
                    cr,
                )
            {
                return;
            }
        }
        self.alloc_batch(0).rect(
            rect,
            depth,
            color,
            Some(coords),
            Some(atlas_layer),
            None,
            false,
            cr,
        );
    }

    #[inline]
    pub fn rect(&mut self, rect: &Rect, depth: f32, color: &[f32; 4], order: u8) {
        let cr = self.clip_rect;
        for batch in self.active.iter_mut() {
            if batch.order == order
                && batch.rect(rect, depth, color, None, None, None, false, cr)
            {
                return;
            }
        }
        self.alloc_batch(order)
            .rect(rect, depth, color, None, None, None, false, cr);
    }

    /// Add an underline quad with GPU pattern rendering
    #[allow(dead_code)]
    #[inline]
    pub fn underline(
        &mut self,
        rect: &Rect,
        depth: f32,
        color: &[f32; 4],
        underline_style: i32,
        thickness: f32,
        order: u8,
    ) {
        let cr = self.clip_rect;
        for batch in self.active.iter_mut() {
            if batch.order == order
                && batch.underline(rect, depth, color, underline_style, thickness, cr)
            {
                return;
            }
        }
        self.alloc_batch(order).underline(
            rect,
            depth,
            color,
            underline_style,
            thickness,
            cr,
        );
    }

    /// Add a rounded rectangle with uniform corner radius (no border)
    #[inline]
    pub fn rounded_rect(
        &mut self,
        rect: &Rect,
        depth: f32,
        color: &[f32; 4],
        corner_radius: f32,
        order: u8,
    ) {
        let cr = self.clip_rect;
        let corner_radii = [corner_radius; 4];
        for batch in self.active.iter_mut() {
            if batch.order == order
                && batch.rounded_rect(
                    rect,
                    depth,
                    color,
                    None,
                    None,
                    None,
                    false,
                    corner_radii,
                    cr,
                )
            {
                return;
            }
        }
        self.alloc_batch(order).rounded_rect(
            rect,
            depth,
            color,
            None,
            None,
            None,
            false,
            corner_radii,
            cr,
        );
    }

    /// Add a quad with per-corner radii
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn quad(
        &mut self,
        rect: &Rect,
        depth: f32,
        background_color: &[f32; 4],
        corner_radii: [f32; 4],
        order: u8,
    ) {
        let cr = self.clip_rect;
        for batch in self.active.iter_mut() {
            if batch.order == order
                && batch.rounded_rect(
                    rect,
                    depth,
                    background_color,
                    None,
                    None,
                    None,
                    false,
                    corner_radii,
                    cr,
                )
            {
                return;
            }
        }
        self.alloc_batch(order).rounded_rect(
            rect,
            depth,
            background_color,
            None,
            None,
            None,
            false,
            corner_radii,
            cr,
        );
    }

    #[inline]
    pub fn build_display_list(
        &mut self,
        instances: &mut Vec<QuadInstance>,
        vertices: &mut Vec<Vertex>,
        cmds: &mut Vec<DrawCmd>,
    ) {
        // Sort batches by draw order (painter's algorithm)
        // Secondary sort: unmasked batches (backgrounds) before masked batches (text)
        // This ensures backgrounds render before text at the same draw order level
        self.active.sort_by_key(|b| (b.order, b.mask.is_some()));

        for batch in &self.active {
            if batch.instances.is_empty() && batch.vertices.is_empty() {
                continue;
            }
            batch.build_display_list(instances, vertices, cmds);
        }
    }

    #[allow(dead_code)]
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
            let end = x + advance;
            if ux >= end {
                return;
            }

            let width = end - ux;

            // Map UnderlineShape to underline_style:
            // 1 = regular, 2 = dashed, 3 = dotted, 4 = curly
            let (underline_style, rect_height, rect_y, thickness) = match underline.shape
            {
                UnderlineShape::Regular => {
                    let uy = baseline + underline.offset;
                    (1, underline.size, uy, underline.size)
                }
                UnderlineShape::Dashed => {
                    let uy = baseline + underline.offset;
                    (2, underline.size, uy, underline.size)
                }
                UnderlineShape::Dotted => {
                    let uy = baseline + underline.offset;
                    (3, underline.size, uy, underline.size)
                }
                UnderlineShape::Curly => {
                    // Curly underline needs extra height for the wave amplitude
                    // thickness is the actual line width, rect_height includes wave amplitude
                    let stroke = (line_height / 16.).clamp(1.0, 2.0);
                    let wave_amplitude = stroke * 0.8; // WAVE_HEIGHT_RATIO
                    let total_height = stroke + wave_amplitude * 2.0;
                    let uy = baseline + underline.offset - wave_amplitude;
                    (4, total_height, uy, stroke)
                }
            };

            self.underline(
                &Rect::new(ux, rect_y, width, rect_height),
                depth,
                &underline.color,
                underline_style,
                thickness,
                0,
            );

            // Handle doubled underlines (only for regular style)
            // Second line is placed with a small gap (1px) below the first
            if underline.is_doubled && matches!(underline.shape, UnderlineShape::Regular)
            {
                let gap = 1.0;
                let second_y = rect_y + underline.size + gap;
                self.underline(
                    &Rect::new(ux, second_y, width, underline.size),
                    depth,
                    &underline.color,
                    1,
                    underline.size,
                    0,
                );
            }
        }
    }

    #[inline]
    fn alloc_batch(&mut self, order: u8) -> &mut Batch {
        let mut batch = self.pool.pop().unwrap_or_default();
        batch.order = order;
        self.active.push(batch);
        self.active.last_mut().unwrap()
    }
}
