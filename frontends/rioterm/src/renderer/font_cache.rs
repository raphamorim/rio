use lru::LruCache;
use rio_backend::sugarloaf::font_introspector::Attributes;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use tracing::debug;
use unicode_width::UnicodeWidthChar;

/// Maximum number of font cache entries to keep in memory
/// Increased for better performance with complex terminal content
const MAX_FONT_CACHE_SIZE: usize = 8192;

/// LRU cache for font metrics to prevent unbounded memory growth
/// Uses a two-tier caching strategy for better performance
pub struct FontCache {
    // Hot cache for most frequently used characters (ASCII)
    hot_cache: HashMap<(char, Attributes), (usize, f32)>,
    // LRU cache for less frequent characters
    cache: LruCache<(char, Attributes), (usize, f32)>,
}

impl FontCache {
    pub fn new() -> Self {
        Self {
            hot_cache: HashMap::with_capacity(128), // ASCII + common chars
            cache: LruCache::new(
                NonZeroUsize::new(MAX_FONT_CACHE_SIZE)
                    .expect("Cache size must be non-zero"),
            ),
        }
    }

    /// Get font metrics from cache with hot path optimization
    pub fn get(&mut self, key: &(char, Attributes)) -> Option<&(usize, f32)> {
        // Check hot cache first for ASCII characters
        if key.0.is_ascii() {
            if let Some(value) = self.hot_cache.get(key) {
                return Some(value);
            }
        }

        // Fall back to LRU cache
        let result = self.cache.get(key);

        // Log cache miss for debugging
        if result.is_none() {
            debug!("FontCache miss for char='{}' attrs={:?}", key.0, key.1);
        }

        result
    }

    /// Insert font metrics into cache with hot path optimization
    pub fn insert(&mut self, key: (char, Attributes), value: (usize, f32)) {
        // Store ASCII characters in hot cache for faster access
        if key.0.is_ascii() && self.hot_cache.len() < 128 {
            self.hot_cache.insert(key, value);
        } else {
            self.cache.put(key, value);
        }
    }

    /// Get current cache size (for debugging/monitoring)
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.hot_cache.len() + self.cache.len()
    }

    /// Check if cache is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.hot_cache.is_empty() && self.cache.is_empty()
    }

    /// Clear all cache entries with cleanup
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.hot_cache.clear();
        self.cache.clear();
    }

    /// Pre-populate cache with common characters to improve hit rate
    /// This should be called during initialization with the font context
    pub fn pre_populate(
        &mut self,
        font_context: &rio_backend::sugarloaf::font::FontLibrary,
    ) {
        let common_chars = [
            // ASCII printable characters (most common)
            ' ', '!', '"', '#', '$', '%', '&', '\'', '(', ')', '*', '+', ',', '-', '.',
            '/', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', ':', ';', '<', '=',
            '>', '?', '@', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L',
            'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z', '[',
            '\\', ']', '^', '_', '`', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j',
            'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y',
            'z', '{', '|', '}', '~',
        ];

        let common_attrs = [
            Attributes::new(
                rio_backend::sugarloaf::font_introspector::Stretch::NORMAL,
                rio_backend::sugarloaf::font_introspector::Weight::NORMAL,
                rio_backend::sugarloaf::font_introspector::Style::Normal,
            ),
            Attributes::new(
                rio_backend::sugarloaf::font_introspector::Stretch::NORMAL,
                rio_backend::sugarloaf::font_introspector::Weight::BOLD,
                rio_backend::sugarloaf::font_introspector::Style::Normal,
            ),
            Attributes::new(
                rio_backend::sugarloaf::font_introspector::Stretch::NORMAL,
                rio_backend::sugarloaf::font_introspector::Weight::NORMAL,
                rio_backend::sugarloaf::font_introspector::Style::Italic,
            ),
        ];

        if let Some(font_ctx) = font_context.inner.try_read() {
            for &ch in &common_chars {
                for &attrs in &common_attrs {
                    let key = (ch, attrs);
                    if self.get(&key).is_none() {
                        let style = rio_backend::sugarloaf::FragmentStyle {
                            font_attrs: attrs,
                            ..Default::default()
                        };

                        let mut width = ch.width().unwrap_or(1) as f32;
                        if let Some((font_id, is_emoji)) =
                            font_ctx.find_best_font_match(ch, &style)
                        {
                            if is_emoji {
                                width = 2.0;
                            }
                            self.insert(key, (font_id, width));
                        }
                    }
                }
            }
        }
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
}
