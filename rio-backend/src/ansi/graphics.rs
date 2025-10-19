// graphics.rs was retired from a alacritty PR made by ayosec
// Alacritty is licensed under Apache 2.0 license.
// https://github.com/alacritty/alacritty/pull/4763/files

use crate::ansi::sixel;
use crate::config::colors::ColorRgb;
use crate::crosswords::grid::Dimensions;
use crate::sugarloaf::{GraphicData, GraphicId};
use parking_lot::Mutex;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::mem;
use std::sync::{Arc, Weak};

#[derive(Debug, Clone)]
pub struct UpdateQueues {
    /// Graphics read from the PTY.
    pub pending: Vec<GraphicData>,

    /// Graphics removed from the grid.
    pub remove_queue: Vec<GraphicId>,
}

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

    /// Queue to track removed textures.
    pub texture_operations: Weak<Mutex<Vec<GraphicId>>>,
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
            texture_operations.lock().push(self.id);
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

    /// Queue to track removed textures.
    pub texture_operations: Weak<Mutex<Vec<GraphicId>>>,
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
            texture_operations.lock().push(self.texture.id);
        }
    }
}

/// Kitty graphics Unicode placeholder character
pub const KITTY_PLACEHOLDER: char = '\u{10EEEE}';

/// Stored image data for Kitty graphics protocol
#[derive(Debug, Clone, PartialEq)]
pub struct StoredImage {
    pub data: GraphicData,
    #[allow(dead_code)]
    pub transmission_time: std::time::Instant,
}

/// Virtual placement metadata for Kitty graphics protocol
/// Stored separately from direct graphics in cells
#[derive(Debug, Clone, PartialEq)]
pub struct VirtualPlacement {
    pub image_id: u32,
    pub placement_id: u32,
    pub columns: u32,
    pub rows: u32,
    pub x: u32,
    pub y: u32,
}

/// Track changes in the grid to add or to remove graphics.
#[derive(Debug, Default)]
pub struct Graphics {
    /// Last generated identifier.
    pub last_id: u64,

    /// New graphics, received from the PTY.
    pub pending: Vec<GraphicData>,

    /// Graphics removed from the grid.
    pub texture_operations: Arc<Mutex<Vec<GraphicId>>>,

    /// Shared palette for Sixel graphics.
    pub sixel_shared_palette: Option<Vec<ColorRgb>>,

    /// Cell height in pixels.
    pub cell_height: f32,

    /// Cell width in pixels.
    pub cell_width: f32,

    /// Current Sixel parser.
    pub sixel_parser: Option<Box<sixel::Parser>>,

    /// Kitty graphics: Cache of transmitted images (by image_id)
    /// Allows placing the same image multiple times without re-transmission
    pub kitty_images: FxHashMap<u32, StoredImage>,

    /// Kitty graphics: Image number to ID mapping (for I= parameter)
    /// Maps image number to the most recently transmitted image with that number
    pub kitty_image_numbers: FxHashMap<u32, u32>,

    /// Kitty graphics: Virtual placements (when U=1)
    /// Key is (image_id, placement_id), value is placement metadata
    pub kitty_virtual_placements: FxHashMap<(u32, u32), VirtualPlacement>,
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
    pub fn has_pending_updates(&self) -> bool {
        !self.pending.is_empty() || !self.texture_operations.lock().is_empty()
    }

    pub fn take_queues(&mut self) -> Option<UpdateQueues> {
        let remove_queue = {
            let mut queue = self.texture_operations.lock();
            if queue.is_empty() {
                Vec::new()
            } else {
                mem::take(&mut *queue)
            }
        };

        if remove_queue.is_empty() && self.pending.is_empty() {
            return None;
        }

        Some(UpdateQueues {
            pending: mem::take(&mut self.pending),
            remove_queue,
        })
    }

    /// Update cell dimensions.
    pub fn resize<S: Dimensions>(&mut self, size: &S) {
        self.cell_height = size.square_height();
        self.cell_width = size.square_width();
    }

    /// Store a kitty graphics image for later placement
    pub fn store_kitty_image(
        &mut self,
        image_id: u32,
        image_number: Option<u32>,
        data: GraphicData,
    ) {
        self.kitty_images.insert(
            image_id,
            StoredImage {
                data,
                transmission_time: std::time::Instant::now(),
            },
        );

        // Update image number mapping if provided
        if let Some(number) = image_number {
            self.kitty_image_numbers.insert(number, image_id);
        }
    }

    /// Get a stored kitty graphics image by ID
    pub fn get_kitty_image(&self, image_id: u32) -> Option<&StoredImage> {
        self.kitty_images.get(&image_id)
    }

    /// Get a stored kitty graphics image by number (I= parameter)
    /// Returns the most recently transmitted image with that number
    pub fn get_kitty_image_by_number(&self, image_number: u32) -> Option<&StoredImage> {
        self.kitty_image_numbers
            .get(&image_number)
            .and_then(|id| self.kitty_images.get(id))
    }

    /// Delete kitty graphics images
    pub fn delete_kitty_images(
        &mut self,
        predicate: impl Fn(&u32, &StoredImage) -> bool,
    ) {
        self.kitty_images.retain(|id, img| !predicate(id, img));
        // Clean up stale number mappings
        self.kitty_image_numbers
            .retain(|_, id| self.kitty_images.contains_key(id));
    }
}

#[test]
fn check_opaque_region() {
    use sugarloaf::ColorType;
    let graphic = GraphicData {
        id: GraphicId(0),
        width: 10,
        height: 10,
        color_type: ColorType::Rgb,
        pixels: vec![255; 10 * 10 * 3],
        is_opaque: true,
        resize: None,
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
        resize: None,
    };

    assert!(graphic.is_filled(0, 0, 3, 3));
    assert!(!graphic.is_filled(1, 1, 4, 4));
}
