/// High-performance character-to-string cache for terminal rendering
/// 
/// This module provides optimized character-to-string conversion for the hot path
/// in terminal rendering, avoiding repeated allocations for common ASCII characters.

use std::collections::HashMap;

/// Pre-computed ASCII character strings for fast lookup
static ASCII_STRINGS: [&str; 128] = [
    "\0", "\x01", "\x02", "\x03", "\x04", "\x05", "\x06", "\x07",
    "\x08", "\t", "\n", "\x0b", "\x0c", "\r", "\x0e", "\x0f",
    "\x10", "\x11", "\x12", "\x13", "\x14", "\x15", "\x16", "\x17",
    "\x18", "\x19", "\x1a", "\x1b", "\x1c", "\x1d", "\x1e", "\x1f",
    " ", "!", "\"", "#", "$", "%", "&", "'",
    "(", ")", "*", "+", ",", "-", ".", "/",
    "0", "1", "2", "3", "4", "5", "6", "7",
    "8", "9", ":", ";", "<", "=", ">", "?",
    "@", "A", "B", "C", "D", "E", "F", "G",
    "H", "I", "J", "K", "L", "M", "N", "O",
    "P", "Q", "R", "S", "T", "U", "V", "W",
    "X", "Y", "Z", "[", "\\", "]", "^", "_",
    "`", "a", "b", "c", "d", "e", "f", "g",
    "h", "i", "j", "k", "l", "m", "n", "o",
    "p", "q", "r", "s", "t", "u", "v", "w",
    "x", "y", "z", "{", "|", "}", "~", "\x7f",
];

/// Cache for character-to-string conversions
/// 
/// Optimized for terminal rendering where most characters are ASCII.
/// ASCII characters use a pre-computed lookup table, while Unicode
/// characters are cached in a HashMap.
pub struct CharCache {
    /// Cache for non-ASCII characters
    unicode_cache: HashMap<char, String>,
}

impl CharCache {
    /// Create a new character cache
    pub fn new() -> Self {
        Self {
            unicode_cache: HashMap::new(),
        }
    }

    /// Get string representation of a character
    /// 
    /// For ASCII characters (0-127), returns a pre-computed static string.
    /// For Unicode characters, caches the result in a HashMap.
    #[inline]
    pub fn get_str(&mut self, c: char) -> &str {
        let code = c as u32;
        if code < 128 {
            ASCII_STRINGS[code as usize]
        } else {
            self.unicode_cache.entry(c).or_insert_with(|| c.to_string())
        }
    }

    /// Clear the Unicode cache to free memory
    /// 
    /// ASCII cache is static and doesn't need clearing.
    pub fn clear_unicode_cache(&mut self) {
        self.unicode_cache.clear();
    }

    /// Get cache statistics for monitoring
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            unicode_entries: self.unicode_cache.len(),
        }
    }
}

impl Default for CharCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the character cache
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub unicode_entries: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ascii_characters() {
        let mut cache = CharCache::new();
        
        // Test common ASCII characters
        assert_eq!(cache.get_str('a'), "a");
        assert_eq!(cache.get_str(' '), " ");
        assert_eq!(cache.get_str('0'), "0");
        assert_eq!(cache.get_str('\n'), "\n");
        
        // Should have no Unicode cache entries
        assert_eq!(cache.stats().unicode_entries, 0);
    }

    #[test]
    fn test_unicode_characters() {
        let mut cache = CharCache::new();
        
        // Test Unicode characters
        assert_eq!(cache.get_str('α'), "α");
        assert_eq!(cache.get_str('β'), "β");
        assert_eq!(cache.get_str('🚀'), "🚀");
        
        // Should have Unicode cache entries
        assert_eq!(cache.stats().unicode_entries, 3);
        
        // Test cache hit
        assert_eq!(cache.get_str('α'), "α");
        assert_eq!(cache.stats().unicode_entries, 3); // No new entries
    }

    #[test]
    fn test_cache_clearing() {
        let mut cache = CharCache::new();
        
        // Add some Unicode characters
        cache.get_str('α');
        cache.get_str('β');
        assert_eq!(cache.stats().unicode_entries, 2);
        
        // Clear cache
        cache.clear_unicode_cache();
        assert_eq!(cache.stats().unicode_entries, 0);
        
        // ASCII should still work
        assert_eq!(cache.get_str('a'), "a");
    }
}