// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// Unified text run manager - replaces separate line cache and shaping cache

use crate::font::text_run_cache::{
    create_cached_text_run, create_text_run_key, ShapedGlyph, TextRunCache,
};
use std::sync::Arc;
use tracing::debug;

/// Unified text run manager that handles shaping, glyph, and vertex caching
pub struct TextRunManager {
    /// Unified cache for text runs (shaping + glyphs + vertices)
    unified_cache: TextRunCache,
}

impl TextRunManager {
    pub fn new() -> Self {
        Self {
            unified_cache: TextRunCache::new(),
        }
    }

    /// Get cached shaping data for a text run
    pub fn get_cached_data(
        &mut self,
        text: &str,
        font_id: usize,
        font_size: f32,
    ) -> CacheResult {
        let key = create_text_run_key(text, font_id, font_size);

        match self.unified_cache.get(&key) {
            Some(cached_run) => CacheResult::Hit {
                glyphs: cached_run.glyphs.clone(),
                advance_width: cached_run.advance_width,
                has_emoji: cached_run.has_emoji,
                font_id: cached_run.font_id,
            },
            None => CacheResult::Miss,
        }
    }

    /// Cache shaping data for a text run
    pub fn cache_shaping_data(
        &mut self,
        text: &str,
        font_id: usize,
        font_size: f32,
        glyphs: Vec<ShapedGlyph>,
        has_emoji: bool,
    ) {
        let key = create_text_run_key(text, font_id, font_size);

        let cached_run = create_cached_text_run(glyphs, font_id, font_size, has_emoji);

        self.unified_cache.insert(key, cached_run);
    }

    /// Apply cached vertices to output, adjusting for new position
    #[cfg(test)]
    pub fn apply_cached_vertices(
        vertices_data: &[u8],
        base_position: (f32, f32),
        new_position: (f32, f32),
        output_vertices: &mut Vec<u8>,
    ) {
        let dx = new_position.0 - base_position.0;
        let dy = new_position.1 - base_position.1;

        // If there's no position change, just copy the data
        if dx == 0.0 && dy == 0.0 {
            output_vertices.extend_from_slice(vertices_data);
            return;
        }

        // Vertex structure: pos[3] + color[4] + uv[2] + layers[2] + corner_radii[4] + rect_size[2] + border_widths[4] + border_color[4]
        // pos: [f32; 3] = 12 bytes
        // color: [f32; 4] = 16 bytes
        // uv: [f32; 2] = 8 bytes
        // layers: [i32; 2] = 8 bytes
        // corner_radii: [f32; 4] = 16 bytes
        // rect_size: [f32; 2] = 8 bytes
        // border_widths: [f32; 4] = 16 bytes
        // border_color: [f32; 4] = 16 bytes
        const VERTEX_SIZE: usize = 116;

        if !vertices_data.len().is_multiple_of(VERTEX_SIZE) {
            debug!("Invalid vertex data size: {}", vertices_data.len());
            output_vertices.extend_from_slice(vertices_data);
            return;
        }

        // Reserve space for the adjusted vertices
        output_vertices.reserve(vertices_data.len());

        // Process vertices in chunks of 44 bytes
        for chunk in vertices_data.chunks_exact(VERTEX_SIZE) {
            // Deserialize position (first 12 bytes - 3 f32s)
            let x_bytes = [chunk[0], chunk[1], chunk[2], chunk[3]];
            let y_bytes = [chunk[4], chunk[5], chunk[6], chunk[7]];
            let z_bytes = [chunk[8], chunk[9], chunk[10], chunk[11]];

            let mut x = f32::from_le_bytes(x_bytes);
            let mut y = f32::from_le_bytes(y_bytes);
            let z = f32::from_le_bytes(z_bytes);

            // Apply position offset (only to x and y, leave z unchanged)
            x += dx;
            y += dy;

            // Write adjusted position
            output_vertices.extend_from_slice(&x.to_le_bytes());
            output_vertices.extend_from_slice(&y.to_le_bytes());
            output_vertices.extend_from_slice(&z.to_le_bytes());

            // Copy the rest of the vertex data unchanged (color + uv + layers)
            // color: bytes 12-27 (16 bytes)
            // uv: bytes 28-35 (8 bytes)
            // layers: bytes 36-43 (8 bytes)
            output_vertices.extend_from_slice(&chunk[12..]);
        }

        debug!("Applied cached vertices with offset ({}, {})", dx, dy);
    }

    /// Clear all caches (called when fonts change)
    pub fn clear_all(&mut self) {
        self.unified_cache.clear();
        debug!("TextRunManager: Cleared unified cache due to font change");
    }
}

/// Result of a cache lookup - indicates what level of cached data is available
#[derive(Debug)]
#[allow(unused)]
/// Simplified cache result - just shaping data or miss
pub enum CacheResult {
    /// Shaping data available (glyph IDs + positions)
    Hit {
        glyphs: Arc<Vec<ShapedGlyph>>,
        advance_width: f32,
        has_emoji: bool,
        font_id: usize,
    },
    /// No cached data available
    Miss,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vertex_positioning() {
        // Create mock vertex data for one complete vertex (60 bytes)
        let mut vertices = Vec::new();

        // Position: (10.0, 20.0, 0.0)
        vertices.extend_from_slice(&10.0f32.to_le_bytes()); // x
        vertices.extend_from_slice(&20.0f32.to_le_bytes()); // y
        vertices.extend_from_slice(&0.0f32.to_le_bytes()); // z

        // Color: (1.0, 0.5, 0.0, 1.0)
        vertices.extend_from_slice(&1.0f32.to_le_bytes());
        vertices.extend_from_slice(&0.5f32.to_le_bytes());
        vertices.extend_from_slice(&0.0f32.to_le_bytes());
        vertices.extend_from_slice(&1.0f32.to_le_bytes());

        // UV: (0.5, 0.7)
        vertices.extend_from_slice(&0.5f32.to_le_bytes());
        vertices.extend_from_slice(&0.7f32.to_le_bytes());

        // Layers: (1, 2)
        vertices.extend_from_slice(&1i32.to_le_bytes());
        vertices.extend_from_slice(&2i32.to_le_bytes());

        // Border radius: 0.0
        vertices.extend_from_slice(&0.0f32.to_le_bytes());

        // Rect size: (100.0, 50.0)
        vertices.extend_from_slice(&100.0f32.to_le_bytes());
        vertices.extend_from_slice(&50.0f32.to_le_bytes());

        // Padding: 0.0
        vertices.extend_from_slice(&0.0f32.to_le_bytes());

        let mut output_vertices = Vec::new();

        TextRunManager::apply_cached_vertices(
            &vertices,
            (100.0, 200.0), // base position
            (150.0, 250.0), // new position (offset by +50, +50)
            &mut output_vertices,
        );

        // Expected: only position should be offset by (+50, +50)
        // So (10, 20, 0) becomes (60, 70, 0)
        assert_eq!(output_vertices.len(), 60);

        // Check adjusted position
        let x = f32::from_le_bytes([
            output_vertices[0],
            output_vertices[1],
            output_vertices[2],
            output_vertices[3],
        ]);
        let y = f32::from_le_bytes([
            output_vertices[4],
            output_vertices[5],
            output_vertices[6],
            output_vertices[7],
        ]);
        let z = f32::from_le_bytes([
            output_vertices[8],
            output_vertices[9],
            output_vertices[10],
            output_vertices[11],
        ]);

        assert_eq!(x, 60.0);
        assert_eq!(y, 70.0);
        assert_eq!(z, 0.0);

        // Check that color, uv, and layers are unchanged
        let color_r = f32::from_le_bytes([
            output_vertices[12],
            output_vertices[13],
            output_vertices[14],
            output_vertices[15],
        ]);
        let uv_u = f32::from_le_bytes([
            output_vertices[28],
            output_vertices[29],
            output_vertices[30],
            output_vertices[31],
        ]);
        let layer_0 = i32::from_le_bytes([
            output_vertices[36],
            output_vertices[37],
            output_vertices[38],
            output_vertices[39],
        ]);

        assert_eq!(color_r, 1.0);
        assert_eq!(uv_u, 0.5);
        assert_eq!(layer_0, 1);
    }

    #[test]
    fn test_vertex_positioning_no_offset() {
        let vertices = vec![0u8; 60]; // Mock vertex data (60 bytes)
        let mut output_vertices = Vec::new();

        TextRunManager::apply_cached_vertices(
            &vertices,
            (100.0, 200.0),
            (100.0, 200.0), // Same position - no offset
            &mut output_vertices,
        );

        assert_eq!(output_vertices, vertices);
    }
}
