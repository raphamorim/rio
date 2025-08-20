use lru::LruCache;
use std::num::NonZeroUsize;
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
    pub font_id: usize,
    /// Font size (as integer to avoid float precision issues)
    pub font_size_scaled: u32,
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

/// High-performance unified text run cache using LRU eviction
/// Combines shaping cache, glyph cache, and vertex cache into a single efficient structure
/// Uses two LRU caches: one for full render data (with color) and one for shaping data (without color)
pub struct TextRunCache {
    /// Primary cache with color information for vertex caching
    cache_with_color: LruCache<TextRunKey, CachedTextRun>,
    /// Secondary cache without color for shaping-only lookups
    cache_without_color: LruCache<TextRunKey, CachedTextRun>,
}

impl TextRunCache {
    /// Create a new unified text run cache
    pub fn new() -> Self {
        Self {
            cache_with_color: LruCache::new(
                NonZeroUsize::new(MAX_TEXT_RUN_CACHE_SIZE * 2).unwrap(),
            ),
            cache_without_color: LruCache::new(
                NonZeroUsize::new(MAX_TEXT_RUN_CACHE_SIZE).unwrap(),
            ),
        }
    }

    /// Get a cached text run with optional vertex data matching
    /// Returns different cache hit types based on what data is available
    pub fn get(&mut self, key: &TextRunKey) -> Option<CacheHitType<'_>> {
        // First try exact match (including color for vertex cache)
        if let Some(cached_run) = self.cache_with_color.get(key) {
            // Check what type of cache hit this is
            if cached_run.vertices.is_some()
                && cached_run.cached_color.is_some()
                && key.color.is_some()
            {
                return Some(CacheHitType::FullRender(cached_run));
            } else if cached_run.shaping_features.is_some() {
                return Some(CacheHitType::ShapingOnly(cached_run));
            } else {
                return Some(CacheHitType::GlyphsOnly(cached_run));
            }
        }

        // Try partial match without color (for shaping cache hit)
        if key.color.is_some() {
            let mut key_without_color = key.clone();
            key_without_color.color = None;

            if let Some(cached_run) = self.cache_without_color.get(&key_without_color) {
                return Some(CacheHitType::ShapingOnly(cached_run));
            }
        }

        None
    }

    /// Insert a shaped text run into the cache with optional render data
    pub fn insert(&mut self, key: TextRunKey, run: CachedTextRun) {
        // Insert into primary cache only if key has color
        if key.color.is_some() {
            self.cache_with_color.put(key.clone(), run.clone());
        }

        // Always insert into secondary cache without color for shaping-only lookups
        let mut key_without_color = key;
        key_without_color.color = None;
        self.cache_without_color.put(key_without_color, run);
    }

    /// Insert or update vertex data for an existing text run
    pub fn update_vertices(
        &mut self,
        key: &TextRunKey,
        vertices: Vec<u8>,
        base_position: (f32, f32),
        color: [f32; 4],
    ) -> bool {
        if let Some(cached_run) = self.cache_with_color.get_mut(key) {
            cached_run.vertices = Some(Arc::new(vertices));
            cached_run.base_position = Some(base_position);
            cached_run.cached_color = Some(color);
            return true;
        }
        false
    }

    /// Clear the cache (called when fonts change)
    pub fn clear(&mut self) {
        self.cache_with_color.clear();
        self.cache_without_color.clear();
        debug!("UnifiedTextRunCache cleared due to font change");
    }

    /// Check if cache capacity is reached
    pub fn is_full(&self) -> bool {
        self.cache_with_color.len() >= self.cache_with_color.cap().get()
    }

    /// Get current cache utilization (0.0 to 1.0)
    pub fn utilization(&self) -> f64 {
        self.cache_with_color.len() as f64 / self.cache_with_color.cap().get() as f64
    }

    /// Resize the caches (useful for dynamic adjustment)
    pub fn resize(&mut self, new_capacity: usize) {
        let new_cap = NonZeroUsize::new(new_capacity).unwrap();
        self.cache_with_color.resize(new_cap);

        let shaping_cap = NonZeroUsize::new(new_capacity / 2).unwrap();
        self.cache_without_color.resize(shaping_cap);
    }

    /// Get the current capacity of the primary cache
    pub fn capacity(&self) -> usize {
        self.cache_with_color.cap().get()
    }

    /// Get the current length of the primary cache
    pub fn len(&self) -> usize {
        self.cache_with_color.len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache_with_color.is_empty()
    }

    /// Peek at an entry without updating LRU order
    pub fn peek(&self, key: &TextRunKey) -> Option<CacheHitType<'_>> {
        // First try exact match (including color for vertex cache)
        if let Some(cached_run) = self.cache_with_color.peek(key) {
            // Check what type of cache hit this is
            if cached_run.vertices.is_some()
                && cached_run.cached_color.is_some()
                && key.color.is_some()
            {
                return Some(CacheHitType::FullRender(cached_run));
            } else if cached_run.shaping_features.is_some() {
                return Some(CacheHitType::ShapingOnly(cached_run));
            } else {
                return Some(CacheHitType::GlyphsOnly(cached_run));
            }
        }

        // Try partial match without color (for shaping cache hit)
        if key.color.is_some() {
            let mut key_without_color = key.clone();
            key_without_color.color = None;

            if let Some(cached_run) = self.cache_without_color.peek(&key_without_color) {
                return Some(CacheHitType::ShapingOnly(cached_run));
            }
        }

        None
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
    font_id: usize,
    font_size: f32,
    color: Option<[f32; 4]>,
) -> TextRunKey {
    TextRunKey {
        text: text.to_string(),
        font_id,
        // Scale font size to avoid float precision issues
        font_size_scaled: (font_size * 100.0) as u32,
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
pub fn create_shaping_key(text: &str, font_id: usize, font_size: f32) -> TextRunKey {
    create_text_run_key(text, font_id, font_size, None)
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
