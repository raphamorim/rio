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
use tracing::debug;

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
#[derive(Debug)]
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

    /// Total bytes of image data currently stored in memory
    /// Includes both pending graphics and stored Kitty images
    pub total_bytes: usize,

    /// Memory limit for graphics storage (default 320MB like Ghostty)
    /// If this is exceeded, oldest/unused images will be evicted
    pub total_limit: usize,

    /// Tracks when each graphic was added (for eviction priority)
    /// Maps GraphicId to insertion timestamp
    pub image_timestamps: FxHashMap<GraphicId, std::time::Instant>,
}

impl Default for Graphics {
    fn default() -> Self {
        Self {
            last_id: 0,
            pending: Vec::new(),
            texture_operations: Arc::new(Mutex::new(Vec::new())),
            sixel_shared_palette: None,
            cell_height: 0.0,
            cell_width: 0.0,
            sixel_parser: None,
            kitty_images: FxHashMap::default(),
            kitty_image_numbers: FxHashMap::default(),
            kitty_virtual_placements: FxHashMap::default(),
            total_bytes: 0,
            total_limit: 320 * 1024 * 1024, // 320MB like Ghostty
            image_timestamps: FxHashMap::default(),
        }
    }
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

    /// Calculate the memory size of a graphic in bytes
    fn calculate_graphic_bytes(graphic: &GraphicData) -> usize {
        graphic.pixels.len()
    }

    /// Evict images to make space for required_bytes.
    /// Returns true if enough space was freed, false otherwise.
    ///
    /// Eviction priority (like Ghostty):
    /// 1. Unused images (no active placements/references)
    /// 2. Oldest images by timestamp
    pub fn evict_images(
        &mut self,
        required_bytes: usize,
        used_ids: &std::collections::HashSet<u64>,
    ) -> bool {
        use tracing::debug;

        if self.total_bytes + required_bytes <= self.total_limit {
            return true; // No eviction needed
        }

        let bytes_to_free = (self.total_bytes + required_bytes) - self.total_limit;
        debug!("Graphics memory: need to evict {} bytes (current: {}, limit: {}, required: {})",
            bytes_to_free, self.total_bytes, self.total_limit, required_bytes);

        // Collect eviction candidates: (GraphicId, timestamp, is_used, bytes)
        let mut candidates: Vec<(GraphicId, std::time::Instant, bool, usize)> =
            Vec::new();

        // Check pending graphics
        for graphic in &self.pending {
            if let Some(&timestamp) = self.image_timestamps.get(&graphic.id) {
                let is_used = used_ids.contains(&graphic.id.0);
                let bytes = Self::calculate_graphic_bytes(graphic);
                candidates.push((graphic.id, timestamp, is_used, bytes));
            }
        }

        // Check stored kitty images
        for (&kitty_id, stored) in &self.kitty_images {
            let graphic_id = GraphicId(kitty_id as u64);
            let is_used = used_ids.contains(&graphic_id.0);
            let bytes = Self::calculate_graphic_bytes(&stored.data);
            candidates.push((graphic_id, stored.transmission_time, is_used, bytes));
        }

        if candidates.is_empty() {
            debug!("No candidates for eviction");
            return false;
        }

        // Sort by priority: unused first, then oldest first
        candidates.sort_by(|a, b| {
            match (a.2, b.2) {
                (false, true) => std::cmp::Ordering::Less, // unused < used
                (true, false) => std::cmp::Ordering::Greater, // used > unused
                _ => a.1.cmp(&b.1),                        // same usage, oldest first
            }
        });

        let mut freed_bytes = 0usize;
        let mut evicted_ids = Vec::new();

        for (graphic_id, _, is_used, bytes) in candidates {
            if freed_bytes >= bytes_to_free {
                break;
            }

            evicted_ids.push(graphic_id);
            freed_bytes += bytes;

            debug!(
                "Evicting graphic id={}, bytes={}, used={}",
                graphic_id.0, bytes, is_used
            );
        }

        // Actually remove the evicted graphics
        for id in evicted_ids {
            // Remove from pending
            self.pending.retain(|g| g.id != id);

            // Remove from kitty_images
            self.kitty_images
                .retain(|&kitty_id, _| GraphicId(kitty_id as u64) != id);

            // Remove timestamp
            self.image_timestamps.remove(&id);

            // Add to removal queue so GPU textures get cleaned up
            self.texture_operations.lock().push(id);
        }

        // Update total_bytes
        self.total_bytes = self.total_bytes.saturating_sub(freed_bytes);

        debug!(
            "Evicted {} bytes, new total: {}",
            freed_bytes, self.total_bytes
        );
        freed_bytes >= bytes_to_free
    }

    /// Track a new graphic's memory usage and timestamp
    pub fn track_graphic(&mut self, graphic_id: GraphicId, bytes: usize) {
        self.image_timestamps
            .insert(graphic_id, std::time::Instant::now());
        self.total_bytes += bytes;
        debug!(
            "Tracked graphic id={}, bytes={}, total_bytes={}",
            graphic_id.0, bytes, self.total_bytes
        );
    }

    /// Update total_bytes when a graphic is removed
    pub fn untrack_graphic(&mut self, graphic_id: GraphicId, bytes: usize) {
        self.image_timestamps.remove(&graphic_id);
        self.total_bytes = self.total_bytes.saturating_sub(bytes);
        debug!(
            "Untracked graphic id={}, bytes={}, total_bytes={}",
            graphic_id.0, bytes, self.total_bytes
        );
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

#[test]
fn test_graphics_memory_tracking() {
    use sugarloaf::ColorType;
    let mut graphics = Graphics::default();

    // Create a small graphic (100x100 RGBA = 40,000 bytes)
    let pixels = vec![255u8; 100 * 100 * 4];
    let graphic = GraphicData {
        id: GraphicId(1),
        width: 100,
        height: 100,
        color_type: ColorType::Rgba,
        pixels,
        is_opaque: true,
        resize: None,
    };

    let bytes = Graphics::calculate_graphic_bytes(&graphic);
    assert_eq!(bytes, 40_000);

    // Track the graphic
    graphics.track_graphic(GraphicId(1), bytes);
    assert_eq!(graphics.total_bytes, 40_000);
    assert!(graphics.image_timestamps.contains_key(&GraphicId(1)));

    // Untrack the graphic
    graphics.untrack_graphic(GraphicId(1), bytes);
    assert_eq!(graphics.total_bytes, 0);
    assert!(!graphics.image_timestamps.contains_key(&GraphicId(1)));
}

#[test]
fn test_graphics_eviction_unused_first() {
    use sugarloaf::ColorType;
    let mut graphics = Graphics {
        total_limit: 100_000, // 100KB limit for testing
        ..Graphics::default()
    };

    // Add 3 graphics (50KB each = 150KB total, will exceed limit)
    let mut used_ids = std::collections::HashSet::new();

    // Graphic 1: 50KB, used
    let pixels1 = vec![255u8; 50_000];
    let graphic1 = GraphicData {
        id: GraphicId(1),
        width: 100,
        height: 125,
        color_type: ColorType::Rgba,
        pixels: pixels1.clone(),
        is_opaque: true,
        resize: None,
    };
    graphics.pending.push(graphic1);
    graphics.track_graphic(GraphicId(1), pixels1.len());
    used_ids.insert(1); // Mark as used

    std::thread::sleep(std::time::Duration::from_millis(10));

    // Graphic 2: 50KB, unused (should be evicted first)
    let pixels2 = vec![255u8; 50_000];
    let graphic2 = GraphicData {
        id: GraphicId(2),
        width: 100,
        height: 125,
        color_type: ColorType::Rgba,
        pixels: pixels2.clone(),
        is_opaque: true,
        resize: None,
    };
    graphics.pending.push(graphic2);
    graphics.track_graphic(GraphicId(2), pixels2.len());
    // Not marked as used

    // Try to add Graphic 3 (will trigger eviction)
    let pixels3_len = 50_000;
    let success = graphics.evict_images(pixels3_len, &used_ids);

    assert!(success, "Eviction should succeed");
    // Graphic 2 (unused) should be evicted, Graphic 1 (used) should remain
    assert_eq!(graphics.pending.len(), 1);
    assert_eq!(graphics.pending[0].id, GraphicId(1));
    assert!(graphics.image_timestamps.contains_key(&GraphicId(1)));
    assert!(!graphics.image_timestamps.contains_key(&GraphicId(2)));
}

#[test]
fn test_graphics_eviction_oldest_first() {
    use sugarloaf::ColorType;
    let mut graphics = Graphics {
        total_limit: 100_000, // 100KB limit
        ..Graphics::default()
    };

    let used_ids = std::collections::HashSet::new(); // No images used

    // Add 3 graphics, all unused
    // Graphic 1: oldest
    let pixels1 = vec![255u8; 50_000];
    let graphic1 = GraphicData {
        id: GraphicId(1),
        width: 100,
        height: 125,
        color_type: ColorType::Rgba,
        pixels: pixels1.clone(),
        is_opaque: true,
        resize: None,
    };
    graphics.pending.push(graphic1);
    graphics.track_graphic(GraphicId(1), pixels1.len());

    std::thread::sleep(std::time::Duration::from_millis(10));

    // Graphic 2: middle
    let pixels2 = vec![255u8; 50_000];
    let graphic2 = GraphicData {
        id: GraphicId(2),
        width: 100,
        height: 125,
        color_type: ColorType::Rgba,
        pixels: pixels2.clone(),
        is_opaque: true,
        resize: None,
    };
    graphics.pending.push(graphic2);
    graphics.track_graphic(GraphicId(2), pixels2.len());

    // Try to add Graphic 3 (will trigger eviction, oldest should go first)
    let pixels3_len = 50_000;
    let success = graphics.evict_images(pixels3_len, &used_ids);

    assert!(success);
    // Graphic 1 (oldest) should be evicted
    assert_eq!(graphics.pending.len(), 1);
    assert_eq!(graphics.pending[0].id, GraphicId(2));
}

#[test]
fn test_graphics_eviction_fails_when_not_enough_space() {
    use sugarloaf::ColorType;
    let mut graphics = Graphics {
        total_limit: 100_000, // 100KB limit
        ..Graphics::default()
    };

    let mut used_ids = std::collections::HashSet::new();

    // Add one 90KB graphic that's in use
    let pixels1 = vec![255u8; 90_000];
    let graphic1 = GraphicData {
        id: GraphicId(1),
        width: 150,
        height: 150,
        color_type: ColorType::Rgba,
        pixels: pixels1.clone(),
        is_opaque: true,
        resize: None,
    };
    graphics.pending.push(graphic1);
    graphics.track_graphic(GraphicId(1), pixels1.len());
    used_ids.insert(1); // Mark as used

    // Try to add another 90KB (total would be 180KB, exceeds limit)
    // Will evict the first one even though it's in use (like Ghostty)
    let pixels2_len = 90_000;
    let success = graphics.evict_images(pixels2_len, &used_ids);

    assert!(
        success,
        "Eviction should succeed by evicting used images if necessary"
    );
    // The used image should be evicted
    assert_eq!(graphics.pending.len(), 0);
}

#[test]
fn test_graphics_no_eviction_when_under_limit() {
    use sugarloaf::ColorType;
    let mut graphics = Graphics {
        total_limit: 200_000, // 200KB limit
        ..Graphics::default()
    };

    let used_ids = std::collections::HashSet::new();

    // Add one 50KB graphic
    let pixels1 = vec![255u8; 50_000];
    let graphic1 = GraphicData {
        id: GraphicId(1),
        width: 100,
        height: 125,
        color_type: ColorType::Rgba,
        pixels: pixels1.clone(),
        is_opaque: true,
        resize: None,
    };
    graphics.pending.push(graphic1);
    graphics.track_graphic(GraphicId(1), pixels1.len());

    // Try to add another 50KB (total 100KB, well under limit)
    let pixels2_len = 50_000;
    let success = graphics.evict_images(pixels2_len, &used_ids);

    assert!(success);
    // No eviction should occur
    assert_eq!(graphics.pending.len(), 1);
    assert_eq!(graphics.total_bytes, 50_000);
}
