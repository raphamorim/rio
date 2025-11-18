use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Arc;
use tracing::debug;

/// Maximum number of text runs to cache
/// Increased to 8192 to maximize cache hits for terminal text
/// Terminal displays are highly repetitive - same text renders frame after frame
/// Text shaping can account for 90%+ of frame time, so caching is critical
/// Larger cache reduces cache thrashing when scrolling through large buffers
const MAX_TEXT_RUN_CACHE_SIZE: usize = 8192;

/// A simplified cached text run containing only essential shaping data
/// Cache shaping results (expensive), not rendering artifacts (cheap)
/// Vertices and atlas lookups are generated on-demand (cheap compared to shaping)
#[derive(Clone, Debug)]
pub struct CachedTextRun {
    /// The shaped glyph data with positioning (just glyph IDs and advances)
    pub glyphs: Arc<Vec<ShapedGlyph>>,
    /// Font ID used for shaping
    pub font_id: usize,
    /// Whether this run contains emoji
    pub has_emoji: bool,
    /// Total advance width of the run
    pub advance_width: f32,
    /// Font size used for this cache entry
    pub font_size: f32,
}

/// A shaped glyph with positioning information
/// Atlas coordinates are looked up on-demand (already cached in glyph atlas)
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
}

/// Key for text run caching - includes all factors that affect shaping
/// Color is NOT part of the key since we only cache shaping, not vertices
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextRunKey {
    /// The text content
    pub text: String,
    /// Font family/style attributes
    pub font_id: usize,
    /// Font size (as integer to avoid float precision issues)
    pub font_size_scaled: u32,
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

/// Statistics for cache performance monitoring
#[derive(Default, Debug, Clone)]
pub struct CacheStats {
    pub full_render_hits: usize,
    pub shaping_hits: usize,
    pub glyph_hits: usize,
    pub misses: usize,
}

impl CacheStats {
    pub fn total_hits(&self) -> usize {
        self.full_render_hits + self.shaping_hits + self.glyph_hits
    }

    pub fn total_requests(&self) -> usize {
        self.total_hits() + self.misses
    }

    pub fn hit_rate(&self) -> f64 {
        let total = self.total_requests();
        if total == 0 {
            0.0
        } else {
            (self.total_hits() as f64 / total as f64) * 100.0
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Simplified text run cache for efficient terminal rendering
/// Caches only shaping results (glyph IDs + positions), not rendering artifacts
/// Vertices are generated on-demand from cached shaping data (fast operation)
pub struct TextRunCache {
    /// Single LRU cache for shaped text runs
    cache: LruCache<TextRunKey, CachedTextRun>,
    /// Statistics for monitoring cache performance
    stats: CacheStats,
}

impl TextRunCache {
    /// Create a new text run cache
    pub fn new() -> Self {
        Self {
            cache: LruCache::new(
                NonZeroUsize::new(MAX_TEXT_RUN_CACHE_SIZE).unwrap(),
            ),
            stats: CacheStats::default(),
        }
    }

    /// Get a cached shaped text run
    /// Returns a reference to the cached shaping data if found
    pub fn get(&mut self, key: &TextRunKey) -> Option<&CachedTextRun> {
        self.cache.get(key)
    }

    /// Insert a shaped text run into the cache
    pub fn insert(&mut self, key: TextRunKey, run: CachedTextRun) {
        self.cache.put(key, run);
    }

    /// Clear the cache (called when fonts change)
    pub fn clear(&mut self) {
        self.cache.clear();
        debug!("TextRunCache cleared due to font change");
    }

    /// Check if cache capacity is reached
    pub fn is_full(&self) -> bool {
        self.cache.len() >= self.cache.cap().get()
    }

    /// Get current cache utilization (0.0 to 1.0)
    pub fn utilization(&self) -> f64 {
        self.cache.len() as f64 / self.cache.cap().get() as f64
    }

    /// Get cache statistics
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    /// Reset cache statistics
    pub fn reset_stats(&mut self) {
        self.stats.reset();
    }

    /// Resize the cache (useful for dynamic adjustment)
    pub fn resize(&mut self, new_capacity: usize) {
        let new_cap = NonZeroUsize::new(new_capacity).unwrap();
        self.cache.resize(new_cap);
    }

    /// Get the current capacity
    pub fn capacity(&self) -> usize {
        self.cache.cap().get()
    }

    /// Get the current length
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Peek at an entry without updating LRU order
    pub fn peek(&self, key: &TextRunKey) -> Option<&CachedTextRun> {
        self.cache.peek(key)
    }
}

impl Default for TextRunCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to create a text run key
pub fn create_text_run_key(
    text: &str,
    font_id: usize,
    font_size: f32,
) -> TextRunKey {
    TextRunKey {
        text: text.to_string(),
        font_id,
        // Scale font size to avoid float precision issues
        font_size_scaled: (font_size * 100.0) as u32,
    }
}

/// Helper function to create a cached text run from shaped glyphs
pub fn create_cached_text_run(
    glyphs: Vec<ShapedGlyph>,
    font_id: usize,
    font_size: f32,
    has_emoji: bool,
) -> CachedTextRun {
    let advance_width = glyphs.iter().map(|g| g.x_advance).sum();

    CachedTextRun {
        glyphs: Arc::new(glyphs),
        font_id,
        has_emoji,
        advance_width,
        font_size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unified_text_run_cache_basic() {
        let mut cache = TextRunCache::new();

        let key = create_text_run_key("hello world", 0, 12.0, Some([1.0, 1.0, 1.0, 1.0]));

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
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_shaping_cache_fallback() {
        let mut cache = TextRunCache::new();

        // Insert with shaping data only (no color)
        let shaping_key = create_shaping_key("hello", 0, 12.0);

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
        let render_key =
            create_text_run_key("hello", 0, 12.0, Some([1.0, 0.0, 0.0, 1.0]));

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
    }

    #[test]
    fn test_vertex_cache_update() {
        let mut cache = TextRunCache::new();

        let key = create_text_run_key("test", 0, 12.0, Some([1.0, 1.0, 1.0, 1.0]));

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
    fn test_lru_eviction() {
        let mut cache = TextRunCache::new();
        let capacity = cache.capacity();

        // Fill cache to capacity + 1 to trigger eviction
        for i in 0..capacity + 1 {
            let key = create_text_run_key(
                &format!("text{i}"),
                0,
                12.0,
                Some([1.0, 1.0, 1.0, 1.0]),
            );

            let run =
                create_cached_text_run(vec![], 0, 12.0, false, None, None, None, None);
            cache.insert(key, run);
        }

        // Cache should be at capacity (LRU evicted the oldest)
        assert_eq!(cache.len(), capacity);

        // The first item should have been evicted
        let first_key = create_text_run_key("text0", 0, 12.0, Some([1.0, 1.0, 1.0, 1.0]));
        assert!(cache.get(&first_key).is_none());

        // The last item should still be there
        let last_key = create_text_run_key(
            &format!("text{capacity}"),
            0,
            12.0,
            Some([1.0, 1.0, 1.0, 1.0]),
        );
        assert!(cache.get(&last_key).is_some());
    }

    #[test]
    fn test_cache_resize() {
        let mut cache = TextRunCache::new();

        // Fill cache
        for i in 0..10 {
            let key = create_text_run_key(
                &format!("text{i}"),
                0,
                12.0,
                Some([1.0, 1.0, 1.0, 1.0]),
            );

            let run =
                create_cached_text_run(vec![], 0, 12.0, false, None, None, None, None);
            cache.insert(key, run);
        }

        // Resize to smaller capacity
        let new_capacity = 5;
        cache.resize(new_capacity);

        assert_eq!(cache.capacity(), new_capacity);
        assert!(cache.len() <= new_capacity);
    }

    #[test]
    fn test_peek_functionality() {
        let mut cache = TextRunCache::new();

        let key = create_text_run_key("peek_test", 0, 12.0, Some([1.0, 1.0, 1.0, 1.0]));

        let run = create_cached_text_run(vec![], 0, 12.0, false, None, None, None, None);
        cache.insert(key.clone(), run);

        // Test that peek works
        assert!(cache.peek(&key).is_some());

        // Test that get also works
        assert!(cache.get(&key).is_some());
    }

    #[test]
    fn test_utilization() {
        let mut cache = TextRunCache::new();

        assert_eq!(cache.utilization(), 0.0);

        // Add some items
        for i in 0..5 {
            let key = create_text_run_key(
                &format!("util_test{i}"),
                0,
                12.0,
                Some([1.0, 1.0, 1.0, 1.0]),
            );

            let run =
                create_cached_text_run(vec![], 0, 12.0, false, None, None, None, None);
            cache.insert(key, run);
        }

        let utilization = cache.utilization();
        assert!(utilization > 0.0);
        assert!(utilization <= 1.0);
    }

    #[test]
    fn test_cache_empty_and_clear() {
        let mut cache = TextRunCache::new();

        assert!(cache.is_empty());

        let key = create_text_run_key("test", 0, 12.0, Some([1.0, 1.0, 1.0, 1.0]));

        let run = create_cached_text_run(vec![], 0, 12.0, false, None, None, None, None);
        cache.insert(key, run);

        assert!(!cache.is_empty());
        assert_eq!(cache.len(), 1);

        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }
}
