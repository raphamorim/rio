use lru::LruCache;
use std::hash::Hasher;
use std::num::NonZeroUsize;
use std::sync::Arc;
use tracing::debug;
use wyhash::WyHash;

/// Maximum number of text runs to cache
const MAX_TEXT_RUN_CACHE_SIZE: usize = 4096;

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

/// Key for text run caching - uses position-independent hash
/// This allows identical text runs at different positions to share cache entries
/// Color is NOT part of the key since we only cache shaping, not vertices
///
/// The hash includes: codepoints + relative clusters + font + size
/// Using u64 hash is more memory efficient than storing full text strings
pub type TextRunKey = u64;

/// Simplified text run cache for efficient terminal rendering
/// Caches only shaping results (glyph IDs + positions), not rendering artifacts
/// Vertices are generated on-demand from cached shaping data (fast operation)
pub struct TextRunCache {
    /// Single LRU cache for shaped text runs
    cache: LruCache<TextRunKey, CachedTextRun>,
}

impl TextRunCache {
    /// Create a new text run cache
    pub fn new() -> Self {
        Self {
            cache: LruCache::new(
                NonZeroUsize::new(MAX_TEXT_RUN_CACHE_SIZE).unwrap(),
            ),
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

/// Helper function to create a position-independent text run key
///
/// This hashes the codepoints with their relative cluster positions,
/// making the cache key independent of where the text appears.
/// This allows "hello" at column 0 and "hello" at column 50 to share
/// the same cache entry, dramatically improving cache hit rates.
///
/// Hash components (in order):
/// 1. For each codepoint: (codepoint, relative_cluster_position)
/// 2. Run length
/// 3. Font ID
/// 4. Font size (scaled)
pub fn create_text_run_key(
    text: &str,
    font_id: usize,
    font_size: f32,
) -> TextRunKey {
    let mut hasher = WyHash::with_seed(0);

    // Hash each codepoint with its relative position (0, 1, 2, ...)
    // This makes the hash position-independent
    for (cluster, ch) in text.chars().enumerate() {
        // Skip emoji/text presentation modifiers (they don't affect shaping)
        if ch == '\u{FE0E}' || ch == '\u{FE0F}' {
            continue;
        }

        hasher.write_u32(ch as u32);
        hasher.write_usize(cluster);
    }

    // Add run length to avoid collisions
    hasher.write_usize(text.chars().count());

    // Add font ID
    hasher.write_usize(font_id);

    // Add scaled font size
    let font_size_scaled = (font_size * 100.0) as u32;
    hasher.write_u32(font_size_scaled);

    hasher.finish()
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

        let key = create_text_run_key("hello world", 0, 12.0);

        let run = create_cached_text_run(vec![], 0, 12.0, false);

        // Test miss
        assert!(cache.get(&key).is_none());

        // Test insert and hit
        cache.insert(key, run.clone());
        assert!(cache.get(&key).is_some());
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_position_independence() {
        let mut cache = TextRunCache::new();

        // Same text "hello" should produce same hash regardless of position
        let key1 = create_text_run_key("hello", 0, 12.0);
        let key2 = create_text_run_key("hello", 0, 12.0);

        assert_eq!(key1, key2);

        let run = create_cached_text_run(vec![], 0, 12.0, false);
        cache.insert(key1, run);

        // Second identical text should hit the cache
        assert!(cache.get(&key2).is_some());
    }

    #[test]
    fn test_hash_consistency() {
        // Same text with same attributes should always produce same hash
        let key1 = create_text_run_key("test", 0, 12.0);
        let key2 = create_text_run_key("test", 0, 12.0);

        assert_eq!(key1, key2);

        // Different text should produce different hash
        let key3 = create_text_run_key("other", 0, 12.0);
        assert_ne!(key1, key3);

        // Different font should produce different hash
        let key4 = create_text_run_key("test", 1, 12.0);
        assert_ne!(key1, key4);

        // Different size should produce different hash
        let key5 = create_text_run_key("test", 0, 14.0);
        assert_ne!(key1, key5);
    }

    #[test]
    fn test_lru_eviction() {
        let mut cache = TextRunCache::new();
        let capacity = cache.capacity();

        // Fill cache to capacity + 1 to trigger eviction
        for i in 0..capacity + 1 {
            let key = create_text_run_key(&format!("text{i}"), 0, 12.0);
            let run = create_cached_text_run(vec![], 0, 12.0, false);
            cache.insert(key, run);
        }

        // Cache should be at capacity (LRU evicted the oldest)
        assert_eq!(cache.len(), capacity);

        // The first item should have been evicted
        let first_key = create_text_run_key("text0", 0, 12.0);
        assert!(cache.get(&first_key).is_none());

        // The last item should still be there
        let last_key = create_text_run_key(&format!("text{capacity}"), 0, 12.0);
        assert!(cache.get(&last_key).is_some());
    }

    #[test]
    fn test_cache_resize() {
        let mut cache = TextRunCache::new();

        // Fill cache
        for i in 0..10 {
            let key = create_text_run_key(&format!("text{i}"), 0, 12.0);
            let run = create_cached_text_run(vec![], 0, 12.0, false);
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

        let key = create_text_run_key("peek_test", 0, 12.0);
        let run = create_cached_text_run(vec![], 0, 12.0, false);
        cache.insert(key, run);

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
            let key = create_text_run_key(&format!("util_test{i}"), 0, 12.0);
            let run = create_cached_text_run(vec![], 0, 12.0, false);
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

        let key = create_text_run_key("test", 0, 12.0);
        let run = create_cached_text_run(vec![], 0, 12.0, false);
        cache.insert(key, run);

        assert!(!cache.is_empty());
        assert_eq!(cache.len(), 1);

        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }
}
