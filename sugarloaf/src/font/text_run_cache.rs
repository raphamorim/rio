use std::hash::Hasher;
use std::mem::MaybeUninit;
use std::sync::Arc;
use tracing::debug;
use wyhash::WyHash;

/// Number of buckets (power of 2 for fast modulo via bitmask)
const NUM_BUCKETS: usize = 256;
/// Entries per bucket (N-way associative)
const BUCKET_SIZE: usize = 8;

/// A simplified cached text run containing only essential shaping data
#[derive(Clone, Debug)]
pub struct CachedTextRun {
    pub glyphs: Arc<Vec<ShapedGlyph>>,
    pub font_id: usize,
    pub has_emoji: bool,
    pub advance_width: f32,
    pub font_size: f32,
}

/// A shaped glyph with positioning information
#[derive(Clone, Debug)]
pub struct ShapedGlyph {
    pub glyph_id: u32,
    pub x_advance: f32,
    pub y_advance: f32,
    pub x_offset: f32,
    pub y_offset: f32,
    pub cluster: u32,
}

pub type TextRunKey = u64;

struct Entry {
    key: u64,
    value: CachedTextRun,
}

/// Move entries using memmove for overlapping regions.
#[inline]
unsafe fn move_entries(items: *mut Entry, dst: usize, src: usize, count: usize) {
    if count == 0 {
        return;
    }
    std::ptr::copy(items.add(src), items.add(dst), count);
}

/// Rotate the item at `pos` to the end of `items[0..len]`.
/// `[A B C* D E]` with pos=2 → `[A B D E C]`
/// Uses memmove + tmp var for efficiency.
#[inline]
unsafe fn rotate_to_end(items: *mut Entry, pos: usize, len: usize) {
    if pos >= len - 1 {
        return;
    }
    let tmp = std::ptr::read(items.add(pos));
    move_entries(items, pos, pos + 1, len - pos - 1);
    std::ptr::write(items.add(len - 1), tmp);
}

/// Rotate a new item into the end, evicting and returning slot 0.
/// `[A B C D] + new` → `[B C D new]`, returns A
/// Evicts slot 0 (oldest) and appends new entry at the end.
#[inline]
unsafe fn rotate_in(items: *mut Entry, len: usize, new: Entry) -> Entry {
    let removed = std::ptr::read(items);
    move_entries(items, 0, 1, len - 1);
    std::ptr::write(items.add(len - 1), new);
    removed
}

/// Fixed-size hash table with N-way associative buckets.
/// - On get: rotate found item to end (LRU within bucket)
/// - On put to full bucket: rotate in new, evict oldest (slot 0)
/// - No Option wrapping: lengths array guards access
/// - Key is used directly as hash (no double-hashing)
pub struct TextRunCache {
    buckets: Box<[[MaybeUninit<Entry>; BUCKET_SIZE]; NUM_BUCKETS]>,
    lengths: [u8; NUM_BUCKETS],
}

impl TextRunCache {
    pub fn new() -> Self {
        Self {
            // SAFETY: MaybeUninit doesn't require initialization
            buckets: unsafe {
                Box::new(
                    MaybeUninit::<
                        [[MaybeUninit<Entry>; BUCKET_SIZE]; NUM_BUCKETS],
                    >::uninit()
                    .assume_init(),
                )
            },
            lengths: [0; NUM_BUCKETS],
        }
    }

    #[inline]
    fn bucket_ptr_mut(&mut self, idx: usize) -> *mut Entry {
        self.buckets[idx].as_mut_ptr() as *mut Entry
    }

    #[inline]
    pub fn get(&mut self, key: &TextRunKey) -> Option<&CachedTextRun> {
        let idx = (*key as usize) & (NUM_BUCKETS - 1);
        let len = self.lengths[idx] as usize;
        let ptr = self.bucket_ptr_mut(idx);

        let mut i = len;
        while i > 0 {
            i -= 1;
            // SAFETY: i < len, and slots 0..len are initialized
            if unsafe { (*ptr.add(i)).key == *key } {
                unsafe { rotate_to_end(ptr, i, len) };
                return Some(unsafe { &(*ptr.add(len - 1)).value });
            }
        }
        None
    }

    #[inline]
    pub fn insert(&mut self, key: TextRunKey, value: CachedTextRun) {
        let idx = (key as usize) & (NUM_BUCKETS - 1);
        let len = self.lengths[idx] as usize;
        let ptr = self.bucket_ptr_mut(idx);

        // Check if key already exists — update and promote
        for i in 0..len {
            if unsafe { (*ptr.add(i)).key == key } {
                // Drop old value, write new, rotate to end
                unsafe {
                    std::ptr::drop_in_place(&mut (*ptr.add(i)).value);
                    std::ptr::write(ptr.add(i), Entry { key, value });
                    rotate_to_end(ptr, i, len);
                }
                return;
            }
        }

        // Space available — append
        if len < BUCKET_SIZE {
            self.buckets[idx][len] = MaybeUninit::new(Entry { key, value });
            self.lengths[idx] += 1;
            return;
        }

        // Full — rotate in new, evict oldest
        let evicted = unsafe { rotate_in(ptr, BUCKET_SIZE, Entry { key, value }) };
        drop(evicted);
    }

    pub fn clear(&mut self) {
        for i in 0..NUM_BUCKETS {
            let len = self.lengths[i] as usize;
            let ptr = self.bucket_ptr_mut(i);
            for j in 0..len {
                unsafe { std::ptr::drop_in_place(ptr.add(j)) };
            }
            self.lengths[i] = 0;
        }
        debug!("TextRunCache cleared");
    }
}

impl Drop for TextRunCache {
    fn drop(&mut self) {
        for i in 0..NUM_BUCKETS {
            let len = self.lengths[i] as usize;
            let ptr = self.bucket_ptr_mut(i);
            for j in 0..len {
                unsafe { std::ptr::drop_in_place(ptr.add(j)) };
            }
        }
    }
}

impl Default for TextRunCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to create a position-independent text run key
pub fn create_text_run_key(text: &str, font_id: usize, font_size: f32) -> TextRunKey {
    let mut hasher = WyHash::with_seed(0);

    for (cluster, ch) in text.chars().enumerate() {
        if ch == '\u{FE0E}' || ch == '\u{FE0F}' {
            continue;
        }
        hasher.write_u32(ch as u32);
        hasher.write_usize(cluster);
    }

    hasher.write_usize(text.chars().count());
    hasher.write_usize(font_id);
    hasher.write_u32((font_size * 100.0) as u32);

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
    fn test_basic_insert_and_get() {
        let mut cache = TextRunCache::new();
        let key = create_text_run_key("hello world", 0, 12.0);
        let run = create_cached_text_run(vec![], 0, 12.0, false);

        assert!(cache.get(&key).is_none());
        cache.insert(key, run);
        assert!(cache.get(&key).is_some());
    }

    #[test]
    fn test_position_independence() {
        let mut cache = TextRunCache::new();
        let key1 = create_text_run_key("hello", 0, 12.0);
        let key2 = create_text_run_key("hello", 0, 12.0);
        assert_eq!(key1, key2);

        cache.insert(key1, create_cached_text_run(vec![], 0, 12.0, false));
        assert!(cache.get(&key2).is_some());
    }

    #[test]
    fn test_hash_consistency() {
        let key1 = create_text_run_key("test", 0, 12.0);
        let key2 = create_text_run_key("test", 0, 12.0);
        assert_eq!(key1, key2);

        assert_ne!(key1, create_text_run_key("other", 0, 12.0));
        assert_ne!(key1, create_text_run_key("test", 1, 12.0));
        assert_ne!(key1, create_text_run_key("test", 0, 14.0));
    }

    #[test]
    fn test_bucket_eviction() {
        let mut cache = TextRunCache::new();

        let base_key: u64 = 42;
        for i in 0..=BUCKET_SIZE {
            let key = base_key + (i as u64 * NUM_BUCKETS as u64);
            cache.insert(key, create_cached_text_run(vec![], 0, 12.0, false));
        }

        assert!(cache.get(&base_key).is_none());
        let last = base_key + (BUCKET_SIZE as u64 * NUM_BUCKETS as u64);
        assert!(cache.get(&last).is_some());
    }

    #[test]
    fn test_lru_promotion() {
        let mut cache = TextRunCache::new();

        let base_key: u64 = 42;
        for i in 0..BUCKET_SIZE {
            let key = base_key + (i as u64 * NUM_BUCKETS as u64);
            cache.insert(key, create_cached_text_run(vec![], 0, 12.0, false));
        }

        // Access first item (promotes to most-recent)
        assert!(cache.get(&base_key).is_some());

        // Insert new — should evict second item (now oldest), not first
        let new_key = base_key + (BUCKET_SIZE as u64 * NUM_BUCKETS as u64);
        cache.insert(new_key, create_cached_text_run(vec![], 0, 12.0, false));

        assert!(cache.get(&base_key).is_some());
        let second = base_key + NUM_BUCKETS as u64;
        assert!(cache.get(&second).is_none());
    }

    #[test]
    fn test_clear() {
        let mut cache = TextRunCache::new();

        for i in 0..10 {
            let key = create_text_run_key(&format!("text{i}"), 0, 12.0);
            cache.insert(key, create_cached_text_run(vec![], 0, 12.0, false));
        }

        cache.clear();
        let key = create_text_run_key("text0", 0, 12.0);
        assert!(cache.get(&key).is_none());
    }
}
