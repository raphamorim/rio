#![allow(clippy::uninlined_format_args)]

use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tracing::debug;

/// Maximum number of text runs to cache
const MAX_TEXT_RUN_CACHE_SIZE: usize = 256;

/// A unified cached text run containing shaping cache, glyphs, and render data
/// This replaces separate line cache and shaping cache with a single comprehensive cache
#[derive(Clone, Debug)]
pub struct CachedTextRun {
    /// The shaped glyph data with positioning
    pub glyphs: Arc<Vec<ShapedGlyph>>,
    /// Font ID used for shaping
    pub font_id: usize,
    /// Whether this run contains emoji
    pub has_emoji: bool,
    /// Total advance width of the run
    pub advance_width: f32,
    /// Cached shaping features for this font/text combination (stored as bytes)
    pub shaping_features: Option<Arc<Vec<u8>>>,
    /// Pre-rendered vertices ready for GPU (position-relative) - stored as bytes
    pub vertices: Option<Arc<Vec<u8>>>,
    /// Base position used when vertices were captured (for repositioning)
    pub base_position: Option<(f32, f32)>,
    /// Text color used for vertex generation (affects caching)
    pub cached_color: Option<[f32; 4]>,
    /// Font size used for this cache entry
    pub font_size: f32,
    /// Creation timestamp for LRU eviction
    pub created_at: u64,
}

/// A shaped glyph with comprehensive positioning and rendering information
#[derive(Clone, Debug)]
pub struct ShapedGlyph {
    /// Glyph ID in the font
    pub glyph_id: u32,
    /// X advance
    pub x_advance: f32,
    /// Y advance  
    pub y_advance: f32,
    /// X offset
    pub x_offset: f32,
    /// Y offset
    pub y_offset: f32,
    /// Cluster index (for ligatures)
    pub cluster: u32,
    /// Cached atlas coordinates for this glyph (if rendered)
    pub atlas_coords: Option<(f32, f32, f32, f32)>, // (u, v, width, height)
    /// Atlas layer index
    pub atlas_layer: Option<u32>,
}

/// Key for text run caching - includes all factors that affect shaping and rendering
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextRunKey {
    /// The text content
    pub text: String,
    /// Font family/style attributes
    pub font_attrs: FontAttributes,
    /// Font size (as integer to avoid float precision issues)
    pub font_size_scaled: u32,
    /// Script/language for shaping
    pub script: u32,
    /// Text direction
    pub direction: TextDirection,
    /// Color (for vertex caching) - optional to allow shaping-only cache hits
    pub color: Option<[u32; 4]>, // Scaled to avoid float precision issues
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FontAttributes {
    pub weight: u16,
    pub style: u8, // 0=normal, 1=italic, 2=oblique
    pub stretch: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TextDirection {
    LeftToRight,
    RightToLeft,
}

/// High-performance unified text run cache using hash table with bucket chaining
/// Combines shaping cache, glyph cache, and vertex cache into a single efficient structure
/// This replaces the previous separate line cache and shaping cache approach
pub struct TextRunCache {
    /// Hash table with bucket chaining (256 buckets, 8 items per bucket)
    /// Each entry stores both hashes to avoid recomputation during fallback lookup
    buckets: Vec<Vec<(u64, u64, TextRunKey, CachedTextRun)>>, // (hash_with_color, hash_without_color, key, run)
    /// Total number of cached items
    item_count: usize,
    /// Cache hit/miss statistics
    hits: u64,
    misses: u64,
    /// Vertex cache specific statistics
    vertex_hits: u64,
    vertex_misses: u64,
    /// Shaping cache specific statistics  
    shaping_hits: u64,
    shaping_misses: u64,
    /// Global timestamp for LRU tracking
    current_timestamp: u64,
}

impl TextRunCache {
    /// Create a new unified text run cache
    pub fn new() -> Self {
        Self {
            buckets: (0..256).map(|_| Vec::with_capacity(8)).collect::<Vec<_>>(),
            item_count: 0,
            hits: 0,
            misses: 0,
            vertex_hits: 0,
            vertex_misses: 0,
            shaping_hits: 0,
            shaping_misses: 0,
            current_timestamp: 0,
        }
    }

    /// Get a cached text run with optional vertex data matching
    /// Returns different cache hit types based on what data is available
    pub fn get(&mut self, key: &TextRunKey) -> Option<CacheHitType> {
        let hash_with_color = self.hash_key(key);
        let bucket_index = (hash_with_color as usize) % 256;
        let bucket = &self.buckets[bucket_index];

        // First try exact match (including color for vertex cache)
        for (
            stored_hash_with_color,
            _stored_hash_without_color,
            stored_key,
            cached_run,
        ) in bucket
        {
            if *stored_hash_with_color == hash_with_color && stored_key == key {
                self.hits += 1;

                // Check what type of cache hit this is
                if cached_run.vertices.is_some()
                    && cached_run.cached_color.is_some()
                    && key.color.is_some()
                {
                    self.vertex_hits += 1;
                    return Some(CacheHitType::FullRender(cached_run));
                } else if cached_run.shaping_features.is_some() {
                    self.shaping_hits += 1;
                    return Some(CacheHitType::ShapingOnly(cached_run));
                } else {
                    return Some(CacheHitType::GlyphsOnly(cached_run));
                }
            }
        }

        // Try partial match without color (for shaping cache hit)
        if key.color.is_some() {
            let key_without_color = TextRunKey {
                color: None,
                ..key.clone()
            };
            let hash_without_color = self.hash_key(&key_without_color);
            let bucket_index_without_color = (hash_without_color as usize) % 256;
            let bucket_without_color = &self.buckets[bucket_index_without_color];

            for (
                _stored_hash_with_color,
                stored_hash_without_color,
                stored_key,
                cached_run,
            ) in bucket_without_color
            {
                if *stored_hash_without_color == hash_without_color
                    && stored_key == &key_without_color
                {
                    self.hits += 1;
                    self.shaping_hits += 1;
                    return Some(CacheHitType::ShapingOnly(cached_run));
                }
            }
        }

        self.misses += 1;
        None
    }

    /// Insert a shaped text run into the cache with optional render data
    pub fn insert(&mut self, key: TextRunKey, run: CachedTextRun) {
        self.current_timestamp += 1;
        let mut run_with_timestamp = run;
        run_with_timestamp.created_at = self.current_timestamp;

        let hash_with_color = self.hash_key(&key);

        // Pre-compute hash without color for faster fallback lookups
        let key_without_color = TextRunKey {
            color: None,
            ..key.clone()
        };
        let hash_without_color = self.hash_key(&key_without_color);

        let bucket_index = (hash_with_color as usize) % 256;
        let bucket = &mut self.buckets[bucket_index];

        // Check if key already exists and update
        for (stored_hash_with_color, stored_hash_without_color, stored_key, cached_run) in
            bucket.iter_mut()
        {
            if *stored_hash_with_color == hash_with_color && stored_key == &key {
                *cached_run = run_with_timestamp;
                // Update the hash_without_color in case it changed
                *stored_hash_without_color = hash_without_color;
                return;
            }
        }

        // Add new entry if bucket has space
        if bucket.len() < 8 {
            bucket.push((hash_with_color, hash_without_color, key, run_with_timestamp));
            self.item_count += 1;
        } else {
            // Replace oldest entry (LRU eviction)
            let oldest_index = bucket
                .iter()
                .enumerate()
                .min_by_key(|(_, (_, _, _, run))| run.created_at)
                .map(|(i, _)| i)
                .unwrap_or(0);

            bucket[oldest_index] =
                (hash_with_color, hash_without_color, key, run_with_timestamp);
        }

        // Log cache statistics periodically
        if self.item_count % 100 == 0 {
            let hit_rate = if self.hits + self.misses > 0 {
                (self.hits as f64) / ((self.hits + self.misses) as f64) * 100.0
            } else {
                0.0
            };
            debug!(
                "UnifiedTextRunCache: {} items, {:.1}% hit rate ({} hits, {} misses), vertex: {}/{}, shaping: {}/{}",
                self.item_count, hit_rate, self.hits, self.misses,
                self.vertex_hits, self.vertex_misses, self.shaping_hits, self.shaping_misses
            );
        }
    }

    /// Insert or update vertex data for an existing text run
    pub fn update_vertices(
        &mut self,
        key: &TextRunKey,
        vertices: Vec<u8>,
        base_position: (f32, f32),
        color: [f32; 4],
    ) -> bool {
        let hash_with_color = self.hash_key(key);
        let bucket_index = (hash_with_color as usize) % 256;
        let bucket = &mut self.buckets[bucket_index];

        for (
            stored_hash_with_color,
            _stored_hash_without_color,
            stored_key,
            cached_run,
        ) in bucket.iter_mut()
        {
            if *stored_hash_with_color == hash_with_color && stored_key == key {
                cached_run.vertices = Some(Arc::new(vertices));
                cached_run.base_position = Some(base_position);
                cached_run.cached_color = Some(color);
                cached_run.created_at = self.current_timestamp;
                self.current_timestamp += 1;
                return true;
            }
        }
        false
    }

    /// Clear the cache (called when fonts change)
    pub fn clear(&mut self) {
        for bucket in &mut self.buckets {
            bucket.clear();
        }
        self.item_count = 0;
        self.current_timestamp = 0;
        debug!("UnifiedTextRunCache cleared due to font change");
    }

    /// Get cache statistics
    pub fn stats(&self) -> (usize, u64, u64, f64, u64, u64, u64, u64) {
        let hit_rate = if self.hits + self.misses > 0 {
            (self.hits as f64) / ((self.hits + self.misses) as f64) * 100.0
        } else {
            0.0
        };
        (
            self.item_count,
            self.hits,
            self.misses,
            hit_rate,
            self.vertex_hits,
            self.vertex_misses,
            self.shaping_hits,
            self.shaping_misses,
        )
    }

    /// Hash a text run key efficiently using FxHasher
    fn hash_key(&self, key: &TextRunKey) -> u64 {
        let mut hasher = FxHasher::default();
        key.hash(&mut hasher);
        hasher.finish()
    }

    /// Check if cache is getting full and needs cleanup
    pub fn needs_cleanup(&self) -> bool {
        self.item_count > MAX_TEXT_RUN_CACHE_SIZE * 2
    }

    /// Perform cache cleanup by removing least recently used entries
    pub fn cleanup(&mut self) {
        if !self.needs_cleanup() {
            return;
        }

        let mut removed = 0;
        for bucket in &mut self.buckets {
            if bucket.len() > 4 {
                // Sort by timestamp and keep only the 4 most recent entries
                bucket.sort_by_key(|(_, _, _, run)| std::cmp::Reverse(run.created_at));
                let old_len = bucket.len();
                bucket.truncate(4);
                removed += old_len - 4;
            }
        }

        self.item_count -= removed;
        debug!("UnifiedTextRunCache cleanup: removed {} entries", removed);
    }
}

/// Different types of cache hits based on available data
#[derive(Debug)]
pub enum CacheHitType<'a> {
    /// Full render data available (glyphs + vertices + shaping)
    FullRender(&'a CachedTextRun),
    /// Only shaping and glyph data available
    ShapingOnly(&'a CachedTextRun),
    /// Only basic glyph data available
    GlyphsOnly(&'a CachedTextRun),
}

impl Default for TextRunCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to create a text run key from common parameters
#[allow(clippy::too_many_arguments)]
pub fn create_text_run_key(
    text: &str,
    font_weight: u16,
    font_style: u8,
    font_stretch: u8,
    font_size: f32,
    script: u32,
    direction: TextDirection,
    color: Option<[f32; 4]>,
) -> TextRunKey {
    TextRunKey {
        text: text.to_string(),
        font_attrs: FontAttributes {
            weight: font_weight,
            style: font_style,
            stretch: font_stretch,
        },
        // Scale font size to avoid float precision issues
        font_size_scaled: (font_size * 100.0) as u32,
        script,
        direction,
        // Scale color to avoid float precision issues
        color: color.map(|c| {
            [
                (c[0] * 1000.0) as u32,
                (c[1] * 1000.0) as u32,
                (c[2] * 1000.0) as u32,
                (c[3] * 1000.0) as u32,
            ]
        }),
    }
}

/// Helper function to create a shaping-only key (without color)
pub fn create_shaping_key(
    text: &str,
    font_weight: u16,
    font_style: u8,
    font_stretch: u8,
    font_size: f32,
    script: u32,
    direction: TextDirection,
) -> TextRunKey {
    create_text_run_key(
        text,
        font_weight,
        font_style,
        font_stretch,
        font_size,
        script,
        direction,
        None,
    )
}

/// Helper function to create a cached text run with comprehensive data
#[allow(clippy::too_many_arguments)]
pub fn create_cached_text_run(
    glyphs: Vec<ShapedGlyph>,
    font_id: usize,
    font_size: f32,
    has_emoji: bool,
    shaping_features: Option<Vec<u8>>,
    vertices: Option<Vec<u8>>,
    base_position: Option<(f32, f32)>,
    color: Option<[f32; 4]>,
) -> CachedTextRun {
    let advance_width = glyphs.iter().map(|g| g.x_advance).sum();

    CachedTextRun {
        glyphs: Arc::new(glyphs),
        font_id,
        has_emoji,
        advance_width,
        shaping_features: shaping_features.map(Arc::new),
        vertices: vertices.map(Arc::new),
        base_position,
        cached_color: color,
        font_size,
        created_at: 0, // Will be set by cache
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unified_text_run_cache_basic() {
        let mut cache = TextRunCache::new();

        let key = create_text_run_key(
            "hello world",
            400,
            0,
            5,
            12.0,
            0,
            TextDirection::LeftToRight,
            Some([1.0, 1.0, 1.0, 1.0]),
        );

        let run = create_cached_text_run(
            vec![],
            0,
            12.0,
            false,
            None,
            None,
            None,
            Some([1.0, 1.0, 1.0, 1.0]),
        );

        // Test miss
        assert!(cache.get(&key).is_none());

        // Test insert and hit
        cache.insert(key.clone(), run.clone());
        assert!(cache.get(&key).is_some());

        let (items, hits, misses, _, _, _, _, _) = cache.stats();
        assert_eq!(items, 1);
        assert_eq!(hits, 1);
        assert_eq!(misses, 1);
    }

    #[test]
    fn test_shaping_cache_fallback() {
        let mut cache = TextRunCache::new();

        // Insert with shaping data only (no color)
        let shaping_key =
            create_shaping_key("hello", 400, 0, 5, 12.0, 0, TextDirection::LeftToRight);

        let run = create_cached_text_run(
            vec![],
            0,
            12.0,
            false,
            Some(vec![1, 2, 3]), // Non-empty shaping features to trigger ShapingOnly
            None,
            None,
            None,
        );

        cache.insert(shaping_key, run);

        // Try to get with color - should hit shaping cache
        let render_key = create_text_run_key(
            "hello",
            400,
            0,
            5,
            12.0,
            0,
            TextDirection::LeftToRight,
            Some([1.0, 0.0, 0.0, 1.0]),
        );

        if let Some(hit_type) = cache.get(&render_key) {
            match hit_type {
                CacheHitType::ShapingOnly(_) => {
                    // Expected - we got shaping data without vertex data
                }
                CacheHitType::GlyphsOnly(_) => {
                    // Also acceptable if no shaping features
                }
                _ => panic!("Expected shaping-only or glyphs-only cache hit"),
            }
        } else {
            panic!("Expected cache hit");
        }

        let (_, hits, misses, _, _, _, shaping_hits, _) = cache.stats();
        assert_eq!(hits, 1);
        assert_eq!(misses, 0); // No misses - insert doesn't count as miss
        assert_eq!(shaping_hits, 1);
    }

    #[test]
    fn test_vertex_cache_update() {
        let mut cache = TextRunCache::new();

        let key = create_text_run_key(
            "test",
            400,
            0,
            5,
            12.0,
            0,
            TextDirection::LeftToRight,
            Some([1.0, 1.0, 1.0, 1.0]),
        );

        let run = create_cached_text_run(vec![], 0, 12.0, false, None, None, None, None);

        cache.insert(key.clone(), run);

        // Update with vertex data
        let vertices = vec![];
        let updated =
            cache.update_vertices(&key, vertices, (10.0, 20.0), [1.0, 1.0, 1.0, 1.0]);
        assert!(updated);

        // Should now get full render cache hit
        if let Some(hit_type) = cache.get(&key) {
            match hit_type {
                CacheHitType::FullRender(cached_run) => {
                    assert!(cached_run.vertices.is_some());
                    assert_eq!(cached_run.base_position, Some((10.0, 20.0)));
                }
                _ => panic!("Expected full render cache hit"),
            }
        } else {
            panic!("Expected cache hit");
        }
    }

    #[test]
    fn test_cache_bucket_overflow_with_lru() {
        let mut cache = TextRunCache::new();

        // Fill a bucket beyond capacity with timestamped entries
        for i in 0..10 {
            let key = create_text_run_key(
                &format!("text{}", i),
                400,
                0,
                5,
                12.0,
                0,
                TextDirection::LeftToRight,
                None,
            );

            let run =
                create_cached_text_run(vec![], 0, 12.0, false, None, None, None, None);

            cache.insert(key, run);
        }

        // Should not exceed bucket capacity
        assert!(cache.buckets.iter().all(|bucket| bucket.len() <= 8));

        // Verify LRU behavior by checking that newer entries are preserved
        let recent_key = create_text_run_key(
            "text9",
            400,
            0,
            5,
            12.0,
            0,
            TextDirection::LeftToRight,
            None,
        );
        assert!(cache.get(&recent_key).is_some());
    }

    #[test]
    fn test_performance_optimizations() {
        // Test 1: TextRunCache with FxHasher and double hash avoidance
        let mut cache = TextRunCache::new();

        // Test cache misses
        for i in 0..100 {
            let key = create_text_run_key(
                &format!("test text {}", i),
                400,
                0,
                5,
                12.0,
                0,
                TextDirection::LeftToRight,
                Some([1.0, 1.0, 1.0, 1.0]),
            );

            // Try lookup (will miss initially)
            let result = cache.get(&key);
            assert!(result.is_none(), "Expected cache miss for new key");
        }

        // Test 2: Verify cache statistics after misses
        let (items, hits, misses, hit_rate, _, _, _, _) = cache.stats();
        assert_eq!(items, 0); // No items inserted, only lookups
        assert_eq!(hits, 0);
        assert_eq!(misses, 100);
        assert_eq!(hit_rate, 0.0);

        // Test 3: Insert some items and verify structure
        for i in 0..10 {
            let key = create_text_run_key(
                &format!("cached text {}", i),
                400,
                0,
                5,
                12.0,
                0,
                TextDirection::LeftToRight,
                Some([1.0, 1.0, 1.0, 1.0]),
            );

            let run = create_cached_text_run(
                vec![],
                0,
                12.0,
                false,
                None,
                None,
                None,
                Some([1.0, 1.0, 1.0, 1.0]),
            );

            cache.insert(key, run);
        }

        // Test 4: Verify cache hits work correctly
        for i in 0..10 {
            let key = create_text_run_key(
                &format!("cached text {}", i),
                400,
                0,
                5,
                12.0,
                0,
                TextDirection::LeftToRight,
                Some([1.0, 1.0, 1.0, 1.0]),
            );

            let result = cache.get(&key);
            assert!(result.is_some(), "Expected cache hit for cached text {}", i);
        }

        // Test 5: Verify improved statistics
        let (items, hits, misses, hit_rate, _, _, _, _) = cache.stats();
        assert_eq!(items, 10);
        assert_eq!(hits, 10);
        assert_eq!(misses, 100); // Previous misses still count
        assert!(hit_rate > 0.0);

        // Test 6: Verify double hash optimization - fallback lookup without color
        let key_with_color = create_text_run_key(
            "fallback test",
            400,
            0,
            5,
            12.0,
            0,
            TextDirection::LeftToRight,
            Some([1.0, 0.0, 0.0, 1.0]),
        );

        let key_without_color = create_text_run_key(
            "fallback test",
            400,
            0,
            5,
            12.0,
            0,
            TextDirection::LeftToRight,
            None,
        );

        // Insert shaping-only data (no color)
        let run = create_cached_text_run(
            vec![],
            0,
            12.0,
            false,
            Some(vec![1, 2, 3]), // Has shaping features
            None,
            None,
            None,
        );
        cache.insert(key_without_color, run);

        // Should find shaping data when looking up with color
        let result = cache.get(&key_with_color);
        assert!(
            result.is_some(),
            "Should find shaping data via fallback lookup"
        );

        match result.unwrap() {
            CacheHitType::ShapingOnly(_) => {
                // Expected - found shaping data without vertex data
            }
            _ => panic!("Expected ShapingOnly cache hit type"),
        }
    }
}
