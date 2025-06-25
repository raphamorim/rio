/// Improved shelf-based atlas allocator for better space utilization
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

    /// Clear all allocations and reset the atlas
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.shelves.clear();
    }

    /// Check if the atlas is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.shelves.is_empty()
    }

    /// Get the atlas dimensions
    #[allow(dead_code)]
    pub fn dimensions(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    /// Deallocates a rectangle (simplified - in practice this is complex)
    pub fn deallocate(&mut self, _x: u16, _y: u16, _width: u16) {
        // For now, we don't implement deallocation as it's complex
        // In a full implementation, you'd need to track allocated rectangles
        // and merge adjacent free spaces when deallocating

        // This is acceptable for a terminal where glyphs are rarely deallocated
        // and the atlas is cleared periodically
    }

    /// Get atlas utilization statistics
    #[allow(dead_code)]
    pub fn utilization(&self) -> AtlasStats {
        let total_area = (self.width as u32) * (self.height as u32);
        let used_area = self.calculate_used_area();

        AtlasStats {
            total_area,
            used_area,
            utilization_ratio: used_area as f32 / total_area as f32,
            num_shelves: self.shelves.len(),
        }
    }

    #[allow(dead_code)]
    fn calculate_used_area(&self) -> u32 {
        let next_y = self.find_next_y_position();
        (self.width as u32) * (next_y as u32)
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct AtlasStats {
    pub total_area: u32,
    pub used_area: u32,
    pub utilization_ratio: f32,
    pub num_shelves: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_clear() {
        let mut atlas = AtlasAllocator::new(100, 100);

        atlas.allocate(10, 10);
        assert!(!atlas.is_empty());

        atlas.clear();
        assert!(atlas.is_empty());

        // Should be able to allocate again after clear
        let pos = atlas.allocate(10, 10);
        assert_eq!(pos, Some((0, 0)));
    }

    #[test]
    fn test_utilization_stats() {
        let mut atlas = AtlasAllocator::new(100, 100);

        let stats = atlas.utilization();
        assert_eq!(stats.total_area, 10000);
        assert_eq!(stats.used_area, 0);
        assert_eq!(stats.utilization_ratio, 0.0);
        assert_eq!(stats.num_shelves, 0);

        atlas.allocate(50, 20);
        let stats = atlas.utilization();
        assert_eq!(stats.used_area, 2100); // 100 * 21 (20 + 1 padding)
        assert_eq!(stats.utilization_ratio, 0.21);
        assert_eq!(stats.num_shelves, 1);
    }
}
