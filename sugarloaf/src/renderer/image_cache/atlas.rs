/// Improved shelf-based atlas allocator for better space utilization
#[derive(Clone)]
pub struct AtlasAllocator {
    width: u16,
    height: u16,
    shelves: Vec<Shelf>,
    waste_limit: f32, // Maximum acceptable waste ratio per shelf
}

#[derive(Debug, Clone)]
struct Shelf {
    x: u16,      // Current x position on this shelf
    y: u16,      // Y position of this shelf
    width: u16,  // Remaining width on this shelf
    height: u16, // Height of this shelf
}

impl AtlasAllocator {
    /// Creates a new atlas with the specified dimensions.
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            shelves: Vec::new(),
            waste_limit: 0.1, // Allow 10% waste per shelf
        }
    }

    /// Allocates a rectangle in the atlas if possible. Returns the x and y
    /// coordinates of the allocated slot.
    pub fn allocate(&mut self, width: u16, height: u16) -> Option<(u16, u16)> {
        // Add padding to prevent bleeding
        let padded_width = width.saturating_add(1);
        let padded_height = height.saturating_add(1);

        // Width is a hard constraint
        if padded_width > self.width {
            return None;
        }

        // Try to find an existing shelf that can fit this rectangle
        if let Some((shelf_idx, x, y)) = self.find_best_shelf(padded_width, padded_height)
        {
            // Allocate on existing shelf
            let shelf = &mut self.shelves[shelf_idx];
            shelf.x += padded_width;
            shelf.width = shelf.width.saturating_sub(padded_width);
            return Some((x, y));
        }

        // No existing shelf can fit, try to create a new shelf
        self.create_new_shelf(padded_width, padded_height)
    }

    /// Find the best existing shelf for the given dimensions
    fn find_best_shelf(&mut self, width: u16, height: u16) -> Option<(usize, u16, u16)> {
        let mut best_shelf = None;
        let mut best_waste = f32::INFINITY;

        for (i, shelf) in self.shelves.iter().enumerate() {
            // Check if this shelf can fit the rectangle
            if shelf.width >= width && shelf.height >= height {
                // Calculate waste ratio for this allocation
                let waste_ratio = self.calculate_waste_ratio(shelf, width, height);

                // Prefer shelves with less waste, but also prefer exact height matches
                let score = if shelf.height == height {
                    waste_ratio - 1.0 // Bonus for exact height match
                } else {
                    waste_ratio
                };

                if score < best_waste && waste_ratio <= self.waste_limit {
                    best_waste = score;
                    best_shelf = Some((i, shelf.x, shelf.y));
                }
            }
        }

        best_shelf
    }

    /// Calculate waste ratio for allocating on a shelf
    fn calculate_waste_ratio(&self, shelf: &Shelf, _width: u16, height: u16) -> f32 {
        if shelf.height == 0 {
            return f32::INFINITY;
        }

        let wasted_height = shelf.height.saturating_sub(height) as f32;
        let total_height = shelf.height as f32;
        wasted_height / total_height
    }

    /// Create a new shelf for the given dimensions
    fn create_new_shelf(&mut self, width: u16, height: u16) -> Option<(u16, u16)> {
        // Find the lowest available Y position
        let y = self.find_next_y_position();

        // Check if we have enough vertical space
        if y.saturating_add(height) > self.height {
            return None;
        }

        // Create new shelf
        let shelf = Shelf {
            x: width,
            y,
            width: self.width.saturating_sub(width),
            height,
        };

        self.shelves.push(shelf);
        Some((0, y))
    }

    /// Find the next available Y position for a new shelf
    fn find_next_y_position(&self) -> u16 {
        if self.shelves.is_empty() {
            return 0;
        }

        // Find the maximum Y + height among all shelves
        self.shelves
            .iter()
            .map(|shelf| shelf.y.saturating_add(shelf.height))
            .max()
            .unwrap_or(0)
    }

    /// Deallocation is a no-op — the shelf packer doesn't track
    /// freed rectangles. Called from [`ImageCache::deallocate`],
    /// which is itself used by graphic-cache eviction. The atlas
    /// will reclaim the space on the next full-clear.
    pub fn deallocate(&mut self, _x: u16, _y: u16, _width: u16) {}

    /// Expand the atlas bounds in place, preserving every existing
    /// shelf's allocated region. Every current shelf gains
    /// `(new_width - old_width)` of remaining horizontal room (its
    /// `x` stays, but the trailing free space now stretches to the
    /// new right edge). New shelves created after this call can be
    /// placed below the current tallest shelf, consuming the extra
    /// vertical room.
    ///
    /// Panics if the new bounds are smaller than the current ones —
    /// shrinking would orphan live allocations.
    pub fn resize(&mut self, new_width: u16, new_height: u16) {
        assert!(
            new_width >= self.width && new_height >= self.height,
            "AtlasAllocator::resize can only grow ({}x{} -> {}x{})",
            self.width,
            self.height,
            new_width,
            new_height
        );
        let width_diff = new_width - self.width;
        if width_diff != 0 {
            for shelf in &mut self.shelves {
                shelf.width = shelf.width.saturating_add(width_diff);
            }
        }
        self.width = new_width;
        self.height = new_height;
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_lets_allocations_use_new_room() {
        // Fill a small atlas completely, then resize and verify the
        // fresh space actually accepts allocations. This is the bug
        // that caused "works for a bit, then freezes" on zoom: cloning
        // the allocator carries the old width/height, so after grow the
        // allocator still refused anything past the original footprint.
        let mut atlas = AtlasAllocator::new(64, 64);
        // Fill the first shelf (64 wide, each alloc 32+1 padded).
        assert!(atlas.allocate(30, 10).is_some());
        assert!(atlas.allocate(30, 10).is_some());
        // Second shelf.
        assert!(atlas.allocate(60, 50).is_some());
        // Nothing fits now — height exhausted (10 + 50 = 60, one pad row each).
        assert!(atlas.allocate(10, 10).is_none());

        atlas.resize(128, 128);
        // Resized atlas should have room for new allocations below the
        // existing shelves.
        let pos = atlas.allocate(10, 10);
        assert!(pos.is_some(), "resize didn't open up new room");
    }

    #[test]
    fn resize_extends_existing_shelf_remaining_width() {
        // An existing shelf with leftover horizontal room should see
        // that leftover stretch to the new atlas width after resize.
        let mut atlas = AtlasAllocator::new(64, 64);
        atlas.allocate(30, 10).unwrap();
        // Shelf is now at x=31, width remaining = 33 (64 - 31).
        atlas.resize(128, 64);
        // After resize, that shelf should still be at x=31, but have
        // width remaining = 97 (128 - 31). A 60-wide allocation that
        // didn't fit in the old 33 must now fit.
        let pos = atlas.allocate(60, 10);
        assert!(pos.is_some(), "existing shelf didn't extend on resize");
    }

    #[test]
    #[should_panic(expected = "can only grow")]
    fn resize_rejects_shrink() {
        let mut atlas = AtlasAllocator::new(128, 128);
        atlas.resize(64, 64);
    }

    #[test]
    fn test_basic_allocation() {
        let mut atlas = AtlasAllocator::new(100, 100);

        // First allocation should succeed
        let pos1 = atlas.allocate(10, 10);
        assert_eq!(pos1, Some((0, 0)));

        // Second allocation on same shelf
        let pos2 = atlas.allocate(10, 10);
        assert_eq!(pos2, Some((11, 0))); // +1 for padding

        // Third allocation should fit on same shelf
        let pos3 = atlas.allocate(10, 10);
        assert_eq!(pos3, Some((22, 0)));
    }

    #[test]
    fn test_new_shelf_creation() {
        let mut atlas = AtlasAllocator::new(100, 100);

        // Fill first shelf
        let pos1 = atlas.allocate(50, 10);
        assert_eq!(pos1, Some((0, 0)));

        let pos2 = atlas.allocate(49, 10); // Should fit on same shelf (50 + 49 + 2 padding = 101 > 100)
        assert_eq!(pos2, Some((0, 11))); // New shelf
    }

    #[test]
    fn test_height_matching() {
        let mut atlas = AtlasAllocator::new(100, 100);

        // Create a shelf with height 20
        let pos1 = atlas.allocate(30, 20);
        assert_eq!(pos1, Some((0, 0)));

        // This should prefer the existing shelf (exact height match)
        let pos2 = atlas.allocate(30, 20);
        assert_eq!(pos2, Some((31, 0)));

        // Different height should create new shelf
        let pos3 = atlas.allocate(30, 10);
        assert_eq!(pos3, Some((0, 21)));
    }

    #[test]
    fn test_oversized_allocation() {
        let mut atlas = AtlasAllocator::new(100, 100);

        // Too wide should fail
        let pos = atlas.allocate(101, 10);
        assert_eq!(pos, None);

        // Too tall should fail
        let pos = atlas.allocate(10, 101);
        assert_eq!(pos, None);
    }

    #[test]
    fn test_atlas_full() {
        let mut atlas = AtlasAllocator::new(20, 20);

        // Fill the atlas
        let pos1 = atlas.allocate(19, 19); // +1 padding = 20x20
        assert_eq!(pos1, Some((0, 0)));

        // Should fail - no space left
        let pos2 = atlas.allocate(1, 1);
        assert_eq!(pos2, None);
    }

}
