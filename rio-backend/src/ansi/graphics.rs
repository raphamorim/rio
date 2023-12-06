// graphics.rs was retired from a alacritty PR made by ayosec
// Alacritty is licensed under Apache 2.0 license.
// https://github.com/alacritty/alacritty/pull/4763/files

use crate::config::colors::ColorRgb;
use crate::crosswords::grid::Dimensions;
use parking_lot::Mutex;
use smallvec::SmallVec;
use std::mem;
use std::sync::{Arc, Weak};

#[derive(Eq, PartialEq, Clone, Debug)]
pub struct ClearSubregion {
    /// Graphics identifier.
    pub id: GraphicId,

    /// X coordinate.
    pub x: u16,

    /// Y coordinate.
    pub y: u16,
}

pub struct UpdateQueues {
    /// Graphics read from the PTY.
    pub pending: Vec<GraphicData>,

    /// Graphics removed from the grid.
    pub remove_queue: Vec<GraphicId>,

    /// Subregions in a graphic to be clear.
    pub clear_subregions: Vec<ClearSubregion>,
}

/// Max allowed dimensions (width, height) for the graphic, in pixels.
pub const MAX_GRAPHIC_DIMENSIONS: [usize; 2] = [4096, 4096];

#[derive(Clone, Debug)]
pub struct TextureRef {
    /// Graphic identifier.
    pub id: GraphicId,

    /// Width, in pixels, of the graphic.
    pub width: u16,

    /// Height, in pixels, of the graphic.
    pub height: u16,

    /// Height, in pixels, of the cell when the graphic was inserted.
    pub cell_height: usize,

    /// Queue to track removed references.
    pub texture_operations: Weak<Mutex<Vec<TextureOperation>>>,
}

impl PartialEq for TextureRef {
    fn eq(&self, t: &Self) -> bool {
        // Ignore texture_operations.
        self.id == t.id
    }
}

impl Eq for TextureRef {}

impl Drop for TextureRef {
    fn drop(&mut self) {
        if let Some(texture_operations) = self.texture_operations.upgrade() {
            texture_operations
                .lock()
                .push(TextureOperation::Remove(self.id));
        }
    }
}

/// A list of graphics in a single cell.
pub type GraphicsCell = SmallVec<[GraphicCell; 1]>;

/// Graphic data stored in a single cell.
#[derive(Clone, Debug)]
pub struct GraphicCell {
    /// Texture to draw the graphic in this cell.
    pub texture: Arc<TextureRef>,

    /// Offset in the x direction.
    pub offset_x: u16,

    /// Offset in the y direction.
    pub offset_y: u16,

    /// Queue to track empty subregions.
    pub texture_operations: Weak<Mutex<Vec<TextureOperation>>>,
}

impl PartialEq for GraphicCell {
    fn eq(&self, c: &Self) -> bool {
        // Ignore texture_operations.
        self.texture == c.texture
            && self.offset_x == c.offset_x
            && self.offset_y == c.offset_y
    }
}

impl Eq for GraphicCell {}

impl Drop for GraphicCell {
    fn drop(&mut self) {
        if let Some(texture_operations) = self.texture_operations.upgrade() {
            let tex_op = TextureOperation::ClearSubregion(ClearSubregion {
                id: self.texture.id,
                x: self.offset_x,
                y: self.offset_y,
            });

            texture_operations.lock().push(tex_op);
        }
    }
}

/// Unique identifier for every graphic added to a grid.
#[derive(Eq, PartialEq, Clone, Debug, Copy, Hash, PartialOrd, Ord)]
pub struct GraphicId(pub u64);

/// Specifies the format of the pixel data.
#[derive(Eq, PartialEq, Clone, Debug, Copy)]
pub enum ColorType {
    /// 3 bytes per pixel (red, green, blue).
    Rgb,

    /// 4 bytes per pixel (red, green, blue, alpha).
    Rgba,
}

/// Defines a single graphic read from the PTY.
#[derive(Eq, PartialEq, Clone, Debug)]
pub struct GraphicData {
    /// Graphics identifier.
    pub id: GraphicId,

    /// Width, in pixels, of the graphic.
    pub width: usize,

    /// Height, in pixels, of the graphic.
    pub height: usize,

    /// Color type of the pixels.
    pub color_type: ColorType,

    /// Pixels data.
    pub pixels: Vec<u8>,

    /// Indicate if there are no transparent pixels.
    pub is_opaque: bool,
}

impl GraphicData {
    /// Check if the image may contain transparent pixels. If it returns
    /// `false`, it is guaranteed that there are no transparent pixels.
    #[inline]
    pub fn maybe_transparent(&self) -> bool {
        !self.is_opaque && self.color_type == ColorType::Rgba
    }

    /// Check if all pixels under a region are opaque.
    ///
    /// If the region exceeds the boundaries of the image it is considered as
    /// not filled.
    pub fn is_filled(&self, x: usize, y: usize, width: usize, height: usize) -> bool {
        // If there are pixels outside the picture we assume that the region is
        // not filled.
        if x + width >= self.width || y + height >= self.height {
            return false;
        }

        // Don't check actual pixels if the image does not contain an alpha
        // channel.
        if !self.maybe_transparent() {
            return true;
        }

        debug_assert!(self.color_type == ColorType::Rgba);

        for offset_y in y..y + height {
            let offset = offset_y * self.width * 4;
            let row = &self.pixels[offset..offset + width * 4];

            if row.chunks_exact(4).any(|pixel| pixel.last() != Some(&255)) {
                return false;
            }
        }

        true
    }
}

#[derive(Debug)]
pub enum TextureOperation {
    /// Remove a texture from the GPU.
    Remove(GraphicId),

    /// Clear a subregion.
    ClearSubregion(ClearSubregion),
}

/// Track changes in the grid to add or to remove graphics.
#[derive(Clone, Debug, Default)]
pub struct Graphics {
    /// Last generated identifier.
    pub last_id: u64,

    /// New graphics, received from the PTY.
    pub pending: Vec<GraphicData>,

    /// Graphics removed from the grid.
    pub texture_operations: Arc<Mutex<Vec<TextureOperation>>>,

    /// Shared palette for Sixel graphics.
    pub sixel_shared_palette: Option<Vec<ColorRgb>>,

    /// Cell height in pixels.
    pub cell_height: f32,

    /// Cell width in pixels.
    pub cell_width: f32,
}

impl Graphics {
    /// Create a new instance, and initialize it with the dimensions of the
    /// window.
    pub fn new<S: Dimensions>(size: &S) -> Self {
        let mut graphics = Graphics::default();
        graphics.resize(size);
        graphics
    }

    /// Generate a new graphic identifier.
    pub fn next_id(&mut self) -> GraphicId {
        self.last_id += 1;
        GraphicId(self.last_id)
    }

    /// Get queues to update graphics in the grid.
    ///
    /// If all queues are empty, it returns `None`.
    pub fn take_queues(&mut self) -> Option<UpdateQueues> {
        let texture_operations = {
            let mut queue = self.texture_operations.lock();
            if queue.is_empty() {
                Vec::new()
            } else {
                mem::take(&mut *queue)
            }
        };

        if texture_operations.is_empty() && self.pending.is_empty() {
            return None;
        }

        let mut remove_queue = Vec::new();
        let mut clear_subregions = Vec::new();

        for operation in texture_operations {
            match operation {
                TextureOperation::Remove(id) => remove_queue.push(id),
                TextureOperation::ClearSubregion(cs) => clear_subregions.push(cs),
            }
        }

        Some(UpdateQueues {
            pending: mem::take(&mut self.pending),
            remove_queue,
            clear_subregions,
        })
    }

    /// Update cell dimensions.
    pub fn resize<S: Dimensions>(&mut self, size: &S) {
        self.cell_height = size.square_height();
        self.cell_width = size.square_width();
    }
}

#[test]
fn check_opaque_region() {
    let graphic = GraphicData {
        id: GraphicId(0),
        width: 10,
        height: 10,
        color_type: ColorType::Rgb,
        pixels: vec![255; 10 * 10 * 3],
        is_opaque: true,
    };

    assert!(graphic.is_filled(1, 1, 3, 3));
    assert!(!graphic.is_filled(8, 8, 10, 10));

    let pixels = {
        // Put a transparent 3x3 box inside the picture.
        let mut data = vec![255; 10 * 10 * 4];
        for y in 3..6 {
            let offset = y * 10 * 4;
            data[offset..offset + 3 * 4].fill(0);
        }
        data
    };

    let graphic = GraphicData {
        id: GraphicId(0),
        pixels,
        width: 10,
        height: 10,
        color_type: ColorType::Rgba,
        is_opaque: false,
    };

    assert!(graphic.is_filled(0, 0, 3, 3));
    assert!(!graphic.is_filled(1, 1, 4, 4));
}
