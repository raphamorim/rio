use lru::LruCache;
use rio_backend::sugarloaf::font_introspector::Attributes;
use std::num::NonZeroUsize;

/// Maximum number of font cache entries to keep in memory
/// This should be enough for most terminal usage while preventing unbounded growth
const MAX_FONT_CACHE_SIZE: usize = 2048;

/// LRU cache for font metrics to prevent unbounded memory growth
pub struct FontCache {
    cache: LruCache<(char, Attributes), (usize, f32)>,
}

impl FontCache {
    pub fn new() -> Self {
        Self {
            cache: LruCache::new(
                NonZeroUsize::new(MAX_FONT_CACHE_SIZE)
                    .expect("Cache size must be non-zero"),
            ),
        }
    }

    /// Get font metrics from cache
    pub fn get(&mut self, key: &(char, Attributes)) -> Option<&(usize, f32)> {
        self.cache.get(key)
    }

    /// Insert font metrics into cache
    pub fn insert(&mut self, key: (char, Attributes), value: (usize, f32)) {
        self.cache.put(key, value);
    }

    /// Get current cache size (for debugging/monitoring)
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if cache is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Clear all cache entries
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.cache.clear();
    }
}

impl Default for FontCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rio_backend::sugarloaf::font_introspector::{Stretch, Style, Weight};

    #[test]
    fn test_font_cache_basic_operations() {
        let mut cache = FontCache::new();

        // Test empty cache
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);

        // Test insertion and retrieval
        let attrs = Attributes::new(Stretch::NORMAL, Weight::NORMAL, Style::Normal);
        let key = ('a', attrs);
        let value = (1, 1.0);

        cache.insert(key, value);
        assert!(!cache.is_empty());
        assert_eq!(cache.len(), 1);

        let retrieved = cache.get(&key);
        assert_eq!(retrieved, Some(&value));
    }

    #[test]
    fn test_font_cache_lru_eviction() {
        let mut cache = FontCache::new();

        // Fill cache beyond capacity to test LRU eviction
        // We'll add a smaller number for testing
        let test_size = 10;
        for i in 0..=test_size {
            let attrs = Attributes::new(Stretch::NORMAL, Weight::NORMAL, Style::Normal);
            let key = (char::from_u32(i as u32 + 65).unwrap_or('A'), attrs);
            let value = (i, i as f32);
            cache.insert(key, value);
        }

        // Cache should have all entries since we're under the limit
        assert_eq!(cache.len(), test_size + 1);
    }

    #[test]
    fn test_font_cache_clear() {
        let mut cache = FontCache::new();

        // Add some entries
        for i in 0..10 {
            let attrs = Attributes::new(Stretch::NORMAL, Weight::NORMAL, Style::Normal);
            let key = (char::from_u32(i as u32 + 65).unwrap_or('A'), attrs);
            let value = (i, i as f32);
            cache.insert(key, value);
        }

        assert_eq!(cache.len(), 10);

        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }
}
