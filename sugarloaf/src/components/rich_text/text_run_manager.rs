// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// Unified text run manager - replaces separate line cache and shaping cache

use crate::font::text_run_cache::{
    create_cached_text_run, create_shaping_key, create_text_run_key, CacheHitType,
    ShapedGlyph, TextRunCache,
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

    /// Get cached data for a text run - returns the best available cache level
    #[allow(clippy::too_many_arguments)]
    pub fn get_cached_data(
        &mut self,
        text: &str,
        font_id: usize,
        font_size: f32,
        color: Option<[f32; 4]>,
    ) -> CacheResult {
        let key = create_text_run_key(text, font_id, font_size, color);

        match self.unified_cache.get(&key) {
            Some(CacheHitType::FullRender(cached_run)) => CacheResult::FullRender {
                glyphs: cached_run.glyphs.clone(),
                vertices: cached_run.vertices.clone().unwrap(),
                base_position: cached_run.base_position.unwrap(),
                advance_width: cached_run.advance_width,
                has_emoji: cached_run.has_emoji,
                font_id: cached_run.font_id,
            },
            Some(CacheHitType::ShapingOnly(cached_run)) => CacheResult::ShapingOnly {
                glyphs: cached_run.glyphs.clone(),
                shaping_features: cached_run.shaping_features.clone(),
                advance_width: cached_run.advance_width,
                has_emoji: cached_run.has_emoji,
                font_id: cached_run.font_id,
            },
            Some(CacheHitType::GlyphsOnly(cached_run)) => CacheResult::GlyphsOnly {
                glyphs: cached_run.glyphs.clone(),
                advance_width: cached_run.advance_width,
                has_emoji: cached_run.has_emoji,
                font_id: cached_run.font_id,
            },
            None => CacheResult::Miss,
        }
    }

    /// Cache shaping data for a text run (first level of caching)
    #[allow(clippy::too_many_arguments)]
    pub fn cache_shaping_data(
        &mut self,
        text: &str,
        font_id: usize,
        font_size: f32,
        glyphs: Vec<ShapedGlyph>,
        has_emoji: bool,
        shaping_features: Option<Vec<u8>>,
    ) {
        let key = create_shaping_key(text, font_id, font_size);

        let cached_run = create_cached_text_run(
            glyphs,
            font_id,
            font_size,
            has_emoji,
            shaping_features,
            None, // No vertices yet
            None, // No base position yet
            None, // No color yet
        );

        self.unified_cache.insert(key, cached_run);
    }

    /// Apply cached vertices to output, adjusting for new position
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

        // Vertex structure: pos[3] + color[4] + uv[2] + layers[2] = 11 * 4 bytes = 44 bytes
        // pos: [f32; 3] = 12 bytes
        // color: [f32; 4] = 16 bytes
        // uv: [f32; 2] = 8 bytes
        // layers: [i32; 2] = 8 bytes
        const VERTEX_SIZE: usize = 44;

        if vertices_data.len() % VERTEX_SIZE != 0 {
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
pub enum CacheResult {
    /// Full render data available (glyphs + vertices + shaping)
    FullRender {
        glyphs: Arc<Vec<ShapedGlyph>>,
        vertices: Arc<Vec<u8>>,
        base_position: (f32, f32),
        advance_width: f32,
        has_emoji: bool,
        font_id: usize,
    },
    /// Shaping and glyph data available (can skip shaping)
    ShapingOnly {
        glyphs: Arc<Vec<ShapedGlyph>>,
        shaping_features: Option<Arc<Vec<u8>>>,
        advance_width: f32,
        has_emoji: bool,
        font_id: usize,
    },
    /// Only basic glyph data available (need to re-shape)
    GlyphsOnly {
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
        // Create mock vertex data for one complete vertex
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

        let mut output_vertices = Vec::new();

        TextRunManager::apply_cached_vertices(
            &vertices,
            (100.0, 200.0), // base position
            (150.0, 250.0), // new position (offset by +50, +50)
            &mut output_vertices,
        );

        // Expected: only position should be offset by (+50, +50)
        // So (10, 20, 0) becomes (60, 70, 0)
        assert_eq!(output_vertices.len(), 44);

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
        let vertices = vec![0u8; 44]; // Mock vertex data (44 bytes)
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
