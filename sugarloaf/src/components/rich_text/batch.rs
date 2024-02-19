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

use crate::components::rich_text::image_cache::TextureId;
use bytemuck::{Pod, Zeroable};

/// Batch geometry vertex.
#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
pub struct Vertex {
    pub pos: [f32; 4],
    pub color: [f32; 4],
    pub uv: [f32; 2],
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
    image: Option<TextureId>,
    mask: Option<TextureId>,
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
    fn add_rect(
        &mut self,
        rect: &Rect,
        depth: f32,
        color: &[f32; 4],
        coords: Option<&[f32; 4]>,
        image: Option<TextureId>,
        mask: Option<TextureId>,
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
        let flags = match (has_image, has_mask) {
            (true, true) => {
                self.image = image;
                self.mask = mask;
                3.
            }
            (true, false) => {
                self.image = image;
                1.
            }
            (false, true) => {
                self.mask = mask;
                2.
            }
            _ => 0.,
        };
        self.push_rect(rect, depth, flags, color, coords);
        true
    }

    fn push_rect(
        &mut self,
        rect: &Rect,
        depth: f32,
        flags: f32,
        color: &[f32; 4],
        coords: Option<&[f32; 4]>,
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
                pos: [x, y, depth, flags],
                color: *color,
                uv: [l, t],
            },
            Vertex {
                pos: [x, y + h, depth, flags],
                color: *color,
                uv: [l, b],
            },
            Vertex {
                pos: [x + w, y + h, depth, flags],
                color: *color,
                uv: [r, b],
            },
            Vertex {
                pos: [x + w, y, depth, flags],
                color: *color,
                uv: [r, t],
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

    fn build_display_list(&self, list: &mut DisplayList) {
        let first_vertex = list.vertices.len() as u32;
        let first_index = list.indices.len() as u32;
        list.vertices.extend_from_slice(&self.vertices);
        list.indices
            .extend(self.indices.iter().map(|i| *i + first_vertex));
        if let Some(tex) = self.mask {
            list.commands.push(Command::BindTexture(0, tex));
        }
        if let Some(tex) = self.image {
            list.commands.push(Command::BindTexture(1, tex));
        }
        list.commands.push(Command::Draw {
            start: first_index,
            count: self.indices.len() as u32,
        });
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

    pub fn reset(&mut self) {
        self.batches.append(&mut self.opaque);
        self.batches.append(&mut self.transparent);
        for batch in &mut self.batches {
            batch.clear();
        }
    }

    pub fn add_mask_rect(
        &mut self,
        rect: &Rect,
        depth: f32,
        color: &[f32; 4],
        coords: &[f32; 4],
        mask: TextureId,
        subpix: bool,
    ) {
        for batch in &mut self.transparent {
            if batch.add_rect(rect, depth, color, Some(coords), None, Some(mask), subpix)
            {
                return;
            }
        }
        self.alloc_batch(true).add_rect(
            rect,
            depth,
            color,
            Some(coords),
            None,
            Some(mask),
            subpix,
        );
    }

    pub fn add_image_rect(
        &mut self,
        rect: &Rect,
        depth: f32,
        color: &[f32; 4],
        coords: &[f32; 4],
        image: TextureId,
        has_alpha: bool,
    ) {
        let transparent = has_alpha || color[3] != 1.0;
        if transparent {
            for batch in &mut self.transparent {
                if batch.add_rect(
                    rect,
                    depth,
                    color,
                    Some(coords),
                    Some(image),
                    None,
                    false,
                ) {
                    return;
                }
            }
        } else {
            for batch in &mut self.opaque {
                if batch.add_rect(
                    rect,
                    depth,
                    color,
                    Some(coords),
                    Some(image),
                    None,
                    false,
                ) {
                    return;
                }
            }
        }
        self.alloc_batch(transparent).add_rect(
            rect,
            depth,
            color,
            Some(coords),
            Some(image),
            None,
            false,
        );
    }

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

    pub fn build_display_list(&self, list: &mut DisplayList) {
        list.commands.push(Command::BindPipeline(Pipeline::Opaque));
        for batch in &self.opaque {
            if batch.vertices.is_empty() {
                continue;
            }
            batch.build_display_list(list);
        }
        let mut mode = Pipeline::Opaque;
        for batch in &self.transparent {
            if batch.vertices.is_empty() {
                continue;
            }
            let batch_mode = if batch.subpix {
                Pipeline::Subpixel
            } else {
                Pipeline::Transparent
            };
            if mode != batch_mode {
                mode = batch_mode;
                list.commands.push(Command::BindPipeline(batch_mode));
            }
            batch.build_display_list(list);
        }
    }

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
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    commands: Vec<Command>,
}

impl DisplayList {
    /// Creates a new empty display list.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the buffered vertices for the display list.
    #[inline]
    pub fn vertices(&self) -> &[Vertex] {
        &self.vertices
    }

    /// Returns the buffered indices for the display list.
    #[inline]
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    /// Returns the sequence of display commands.
    #[inline]
    pub fn commands(&self) -> &[Command] {
        &self.commands
    }

    /// Clears the display list.
    #[inline]
    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
        self.commands.clear();
    }
}

/// Command in a display list.
#[derive(Copy, Clone, Debug)]
pub enum Command {
    /// Bind a texture at the specified slot.
    BindTexture(u32, TextureId),
    /// Switch to the specified render mode.
    BindPipeline(Pipeline),
    /// Draw the specified range of indexed triangles.
    Draw { start: u32, count: u32 },
}

/// Pipelines used by a display list.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Pipeline {
    Opaque,
    Transparent,
    Subpixel,
}
