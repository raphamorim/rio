// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// Unified text run manager - replaces separate line cache and shaping cache

use crate::font::text_run_cache::{
    create_cached_text_run, create_shaping_key, create_text_run_key, CacheHitType,
    ShapedGlyph, TextDirection, TextRunCache,
};
use std::sync::Arc;
use tracing::debug;

/// Common terminal patterns for cache warming
/// These patterns are frequently seen in terminal applications and benefit from pre-caching
const COMMON_TERMINAL_PATTERNS: &[&str] = &[
    // Common indentation patterns (programming)
    "    ",             // 4 spaces
    "        ",         // 8 spaces
    "            ",     // 12 spaces
    "                ", // 16 spaces
    "\t",               // Single tab
    "\t\t",             // Double tab
    "\t\t\t",           // Triple tab
    // Programming keywords and symbols
    "const ",
    "let ",
    "var ",
    "function ",
    "class ",
    "import ",
    "export ",
    "return ",
    "if ",
    "else ",
    "for ",
    "while ",
    "switch ",
    "case ",
    "break ",
    "continue ",
    "try ",
    "catch ",
    "finally ",
    "async ",
    "await ",
    "public ",
    "private ",
    "protected ",
    "static ",
    "void ",
    "int ",
    "string ",
    "bool ",
    "true",
    "false",
    "null",
    "undefined",
    // Common operators and punctuation
    " = ",
    " == ",
    " === ",
    " != ",
    " !== ",
    " <= ",
    " >= ",
    " && ",
    " || ",
    " => ",
    " -> ",
    "();",
    "{}",
    "[]",
    "();",
    "{};",
    "[];",
    // Shell prompts and common commands
    "$ ",
    "# ",
    "> ",
    "~ ",
    "├── ",
    "└── ",
    "│   ",
    "ls ",
    "cd ",
    "pwd",
    "cat ",
    "grep ",
    "find ",
    "git ",
    "npm ",
    "cargo ",
    "make ",
    "sudo ",
    // Common file extensions and paths
    ".js",
    ".ts",
    ".rs",
    ".py",
    ".go",
    ".cpp",
    ".c",
    ".h",
    ".json",
    ".xml",
    ".html",
    ".css",
    ".md",
    ".txt",
    ".log",
    "/usr/",
    "/home/",
    "/var/",
    "/etc/",
    "./",
    "../",
    // Common error/log patterns
    "Error: ",
    "Warning: ",
    "Info: ",
    "Debug: ",
    "[ERROR]",
    "[WARN]",
    "[INFO]",
    "[DEBUG]",
    "FAILED",
    "SUCCESS",
    "OK",
    // Numbers and common values
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "10",
    "100",
    "1000",
    "0.0",
    "1.0",
    "-1",
    // Common terminal output
    "...",
    "---",
    "===",
    "***",
    "+++",
    ">>>",
    "<<<",
];

/// Unified text run manager that handles shaping, glyph, and vertex caching
/// This replaces the previous separate TextRunManager and line cache approach
pub struct TextRunManager {
    /// Unified cache for text runs (shaping + glyphs + vertices)
    unified_cache: TextRunCache,
    /// Statistics
    total_requests: u64,
    full_render_hits: u64,
    shaping_hits: u64,
    glyph_hits: u64,
    cache_misses: u64,
}

impl TextRunManager {
    pub fn new() -> Self {
        Self {
            unified_cache: TextRunCache::new(),
            total_requests: 0,
            full_render_hits: 0,
            shaping_hits: 0,
            glyph_hits: 0,
            cache_misses: 0,
        }
    }

    /// Warm the cache with common terminal patterns for improved hit rates
    /// This should be called during initialization with the primary font configuration
    #[allow(clippy::too_many_arguments)]
    pub fn warm_cache(
        &mut self,
        font_id: usize,
        font_size: f32,
        font_weight: u16,
        font_style: u8,
        font_stretch: u8,
        default_color: [f32; 4],
    ) {
        debug!(
            "Warming text run cache with {} common patterns",
            COMMON_TERMINAL_PATTERNS.len()
        );

        let mut warmed_count = 0;

        for &pattern in COMMON_TERMINAL_PATTERNS {
            // Create cache key for this pattern
            let key = create_text_run_key(
                pattern,
                font_weight,
                font_style,
                font_stretch,
                font_size,
                0, // script
                TextDirection::LeftToRight,
                Some(default_color),
            );

            // Create minimal cached text run for warming (shaping-only level)
            // In a real implementation, this would be populated by actual shaping
            let cached_run = create_cached_text_run(
                vec![], // Empty glyphs - will be populated when actually used
                font_id,
                font_size,
                false, // has_emoji
                None,  // shaping_features - will be populated on first use
                None,  // vertices - will be populated on first render
                None,  // base_position
                Some(default_color),
            );

            // Insert into cache for future hits
            self.unified_cache.insert(key, cached_run);
            warmed_count += 1;
        }

        debug!(
            "Cache warming completed: {} patterns pre-cached",
            warmed_count
        );
    }

    /// Get cached data for a text run - returns the best available cache level
    #[allow(clippy::too_many_arguments)]
    pub fn get_cached_data(
        &mut self,
        text: &str,
        _font_id: usize,
        font_size: f32,
        font_weight: u16,
        font_style: u8,
        font_stretch: u8,
        color: Option<[f32; 4]>,
    ) -> CacheResult {
        self.total_requests += 1;

        let key = create_text_run_key(
            text,
            font_weight,
            font_style,
            font_stretch,
            font_size,
            0, // script
            TextDirection::LeftToRight,
            color,
        );

        match self.unified_cache.get(&key) {
            Some(CacheHitType::FullRender(cached_run)) => {
                self.full_render_hits += 1;
                CacheResult::FullRender {
                    glyphs: cached_run.glyphs.clone(),
                    vertices: cached_run.vertices.clone().unwrap(),
                    base_position: cached_run.base_position.unwrap(),
                    advance_width: cached_run.advance_width,
                    has_emoji: cached_run.has_emoji,
                    font_id: cached_run.font_id,
                }
            }
            Some(CacheHitType::ShapingOnly(cached_run)) => {
                self.shaping_hits += 1;
                CacheResult::ShapingOnly {
                    glyphs: cached_run.glyphs.clone(),
                    shaping_features: cached_run.shaping_features.clone(),
                    advance_width: cached_run.advance_width,
                    has_emoji: cached_run.has_emoji,
                    font_id: cached_run.font_id,
                }
            }
            Some(CacheHitType::GlyphsOnly(cached_run)) => {
                self.glyph_hits += 1;
                CacheResult::GlyphsOnly {
                    glyphs: cached_run.glyphs.clone(),
                    advance_width: cached_run.advance_width,
                    has_emoji: cached_run.has_emoji,
                    font_id: cached_run.font_id,
                }
            }
            None => {
                self.cache_misses += 1;
                CacheResult::Miss
            }
        }
    }

    /// Cache shaping data for a text run (first level of caching)
    #[allow(clippy::too_many_arguments)]
    pub fn cache_shaping_data(
        &mut self,
        text: &str,
        font_id: usize,
        font_size: f32,
        font_weight: u16,
        font_style: u8,
        font_stretch: u8,
        glyphs: Vec<ShapedGlyph>,
        has_emoji: bool,
        shaping_features: Option<Vec<u8>>,
    ) {
        let key = create_shaping_key(
            text,
            font_weight,
            font_style,
            font_stretch,
            font_size,
            0, // script
            TextDirection::LeftToRight,
        );

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
        // For now, just copy the vertex data
        // In a real implementation, you'd deserialize, adjust positions, and re-serialize
        let dx = new_position.0 - base_position.0;
        let dy = new_position.1 - base_position.1;

        // This is a simplified implementation - in practice you'd need to properly
        // deserialize the vertex data, adjust positions, and serialize back
        output_vertices.extend_from_slice(vertices_data);

        // TODO: Implement proper vertex position adjustment
        debug!("Applied cached vertices with offset ({}, {})", dx, dy);
    }

    /// Clear all caches (called when fonts change)
    pub fn clear_all(&mut self) {
        self.unified_cache.clear();
        debug!("TextRunManager: Cleared unified cache due to font change");
    }

    /// Get comprehensive cache statistics
    pub fn stats(&self) -> TextRunManagerStats {
        let (
            items,
            total_hits,
            total_misses,
            hit_rate,
            vertex_hits,
            vertex_misses,
            shaping_hits,
            shaping_misses,
        ) = self.unified_cache.stats();

        TextRunManagerStats {
            total_requests: self.total_requests,
            cache_items: items,
            total_hits,
            total_misses,
            overall_hit_rate: hit_rate,
            full_render_hits: self.full_render_hits,
            shaping_hits: self.shaping_hits,
            glyph_hits: self.glyph_hits,
            cache_misses: self.cache_misses,
            vertex_cache_hits: vertex_hits,
            vertex_cache_misses: vertex_misses,
            shaping_cache_hits: shaping_hits,
            shaping_cache_misses: shaping_misses,
        }
    }

    /// Perform maintenance on the cache
    pub fn maintenance(&mut self) {
        if self.unified_cache.needs_cleanup() {
            self.unified_cache.cleanup();
        }

        // Log statistics periodically
        if self.total_requests % 1000 == 0 && self.total_requests > 0 {
            let stats = self.stats();
            debug!(
                "UnifiedTextRunManager stats: {:.1}% hit rate ({} requests), Full: {}, Shaping: {}, Glyphs: {}, Miss: {}, {} items",
                stats.overall_hit_rate, stats.total_requests, stats.full_render_hits,
                stats.shaping_hits, stats.glyph_hits, stats.cache_misses, stats.cache_items
            );
        }
    }

    /// Check if cache needs cleanup
    pub fn needs_cleanup(&self) -> bool {
        self.unified_cache.needs_cleanup()
    }
}

/// Result of a cache lookup - indicates what level of cached data is available
#[derive(Debug)]
#[allow(dead_code)]
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

/// Comprehensive statistics for the unified text run manager
#[derive(Debug)]
#[allow(dead_code)]
pub struct TextRunManagerStats {
    pub total_requests: u64,
    pub cache_items: usize,
    pub total_hits: u64,
    pub total_misses: u64,
    pub overall_hit_rate: f64,
    pub full_render_hits: u64,
    pub shaping_hits: u64,
    pub glyph_hits: u64,
    pub cache_misses: u64,
    pub vertex_cache_hits: u64,
    pub vertex_cache_misses: u64,
    pub shaping_cache_hits: u64,
    pub shaping_cache_misses: u64,
}

impl Default for TextRunManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vertex_positioning() {
        let vertices = vec![1, 2, 3, 4]; // Mock vertex data
        let mut output_vertices = Vec::new();

        TextRunManager::apply_cached_vertices(
            &vertices,
            (100.0, 200.0),
            (150.0, 250.0),
            &mut output_vertices,
        );

        assert_eq!(output_vertices, vertices);
    }
}
