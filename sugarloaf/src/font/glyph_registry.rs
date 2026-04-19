// Per-terminal registry of glyphs registered over Glyph Protocol.
//
// Each Crosswords (terminal tab) owns one `GlyphRegistry` behind an
// `Arc<RwLock<_>>`. The rendering pipeline borrows it read-only when
// resolving codepoints to glyph outlines, so concurrent registrations
// from the app side never block a frame.
//
// Two tabs can register conflicting glyphs for the same codepoint —
// each tab sees only its own registry. Registrations live for the
// lifetime of the terminal session and are dropped on close.
//
// The registry holds at most 256 simultaneous entries. On the 257th
// register, the oldest entry is evicted (FIFO) to make room. Cleared
// codepoints are removed immediately; re-registering a codepoint
// overwrites the previous entry without affecting FIFO order of
// other entries.

use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Sentinel `font_id` returned by `FontLibraryData::find_best_font_match`
/// when the codepoint has a live registration. The rasterizer branches
/// on this value to skip the charmap/shape path and render from the
/// registry instead.
pub const CUSTOM_GLYPH_FONT_ID: usize = usize::MAX;

/// Maximum simultaneous registrations per session (spec §4).
pub const GLOSSARY_CAPACITY: usize = 256;

/// Is `cp` in any of the three Unicode Private Use Areas? This is the
/// check enforced by the `r` verb's parser; mirrored here so the
/// registry itself never stores a non-PUA codepoint even if a future
/// caller forgets to validate.
#[inline]
pub fn is_pua(cp: u32) -> bool {
    (0xE000..=0xF8FF).contains(&cp)
        || (0xF_0000..=0xF_FFFD).contains(&cp)
        || (0x10_0000..=0x10_FFFD).contains(&cp)
}

/// Payload retained per registration. `Glyf` is a single monochrome
/// outline; `ColrV0` and `ColrV1` carry the full colour container (a
/// table of outlines + raw OpenType `COLR`/`CPAL` bytes) so the
/// renderer can walk the paint graph at any cell size without
/// re-transmitting. The two colour variants share structure; the
/// variant tag tells the renderer which COLR table version to parse.
#[derive(Debug, Clone)]
pub enum StoredPayload {
    Glyf {
        glyf: Vec<u8>,
    },
    ColrV0 {
        glyphs: Vec<Vec<u8>>,
        colr: Vec<u8>,
        cpal: Vec<u8>,
    },
    ColrV1 {
        glyphs: Vec<Vec<u8>>,
        colr: Vec<u8>,
        cpal: Vec<u8>,
    },
}

impl StoredPayload {
    /// Constructor for the legacy monochrome-only path. Lets existing
    /// tests and call-sites keep their prior ergonomics while the new
    /// colour variants are plumbed through.
    pub fn glyf(bytes: Vec<u8>) -> Self {
        StoredPayload::Glyf { glyf: bytes }
    }
}

/// A single registered glyph. The raw payload is retained so the
/// renderer can re-rasterize at any cell size without re-transmitting.
#[derive(Debug, Clone)]
pub struct RegisteredGlyph {
    pub payload: StoredPayload,
    pub upm: u16,
    /// Stable render-side index in `0..=255`. Because codepoints are
    /// 21-bit and the renderer's glyph-id field is u16, we hand every
    /// registration a u8 slot id that fits. Indices are reused after
    /// eviction or explicit clear, so the atlas cache must be
    /// invalidated for a (slot_id, *) pair whenever that happens.
    pub index: u8,
    /// Per-registration insertion id used to order entries for FIFO
    /// eviction. A larger id means "registered later."
    pub insertion_id: u64,
}

#[derive(Debug)]
struct Inner {
    by_cp: FxHashMap<u32, RegisteredGlyph>,
    /// Reverse map: `indexed[i]` holds the codepoint currently using
    /// slot index `i`, or `None` if the slot is free.
    indexed: [Option<u32>; GLOSSARY_CAPACITY],
    /// Monotonic counter that stamps each registration so we can
    /// evict the oldest in O(n) over 256 entries.
    next_insertion: u64,
}

impl Default for Inner {
    fn default() -> Self {
        Self {
            by_cp: FxHashMap::default(),
            indexed: [None; GLOSSARY_CAPACITY],
            next_insertion: 0,
        }
    }
}

/// The public registry handle. Cloned cheaply via `Arc`.
#[derive(Debug, Clone, Default)]
pub struct GlyphRegistry {
    inner: Arc<RwLock<Inner>>,
}

/// Reasons a register call is rejected by the registry itself (as
/// opposed to parse-time rejections the dispatcher handles).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisterRejection {
    /// `cp` is not in any PUA range. Callers should catch this at the
    /// parser; this is a defence-in-depth check.
    OutOfNamespace,
}

impl GlyphRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a glyph at a PUA codepoint.
    ///
    /// If the codepoint is already registered, the outline is replaced
    /// and the existing insertion order and slot index are preserved.
    ///
    /// If the glossary is full (256 entries) and `cp` is NOT already
    /// registered, the oldest entry is evicted to make room. Returns
    /// `Some(evicted_cp)` in that case so the caller can invalidate
    /// its render cache for that codepoint.
    pub fn register(
        &self,
        cp: u32,
        payload: StoredPayload,
        upm: u16,
    ) -> Result<Option<u32>, RegisterRejection> {
        if !is_pua(cp) {
            return Err(RegisterRejection::OutOfNamespace);
        }
        let mut inner = self.inner.write();

        // Overwrite path: reuse the existing slot index. Insertion id
        // is preserved so overwrite does not refresh eviction order.
        if let Some(existing) = inner.by_cp.get_mut(&cp) {
            existing.payload = payload;
            existing.upm = upm;
            return Ok(None);
        }

        // Fresh insertion. Find a free slot; if none, evict the
        // oldest entry and take its slot.
        let (slot_index, evicted) = match inner.indexed.iter().position(|s| s.is_none()) {
            Some(i) => (i as u8, None),
            None => {
                let (evict_cp, evict_entry) = inner
                    .by_cp
                    .iter()
                    .min_by_key(|(_, v)| v.insertion_id)
                    .map(|(cp, v)| (*cp, v.clone()))
                    .expect("capacity full but by_cp empty");
                let freed_index = evict_entry.index;
                inner.by_cp.remove(&evict_cp);
                inner.indexed[freed_index as usize] = None;
                (freed_index, Some(evict_cp))
            }
        };

        let id = inner.next_insertion;
        inner.next_insertion = inner.next_insertion.wrapping_add(1);
        inner.indexed[slot_index as usize] = Some(cp);
        inner.by_cp.insert(
            cp,
            RegisteredGlyph {
                payload,
                upm,
                index: slot_index,
                insertion_id: id,
            },
        );
        Ok(evicted)
    }

    /// Clear one codepoint. No-op if nothing was registered.
    pub fn clear_one(&self, cp: u32) {
        let mut inner = self.inner.write();
        if let Some(entry) = inner.by_cp.remove(&cp) {
            inner.indexed[entry.index as usize] = None;
        }
    }

    /// Drop every registration and free every slot index.
    pub fn clear_all(&self) {
        let mut inner = self.inner.write();
        inner.by_cp.clear();
        inner.indexed = [None; GLOSSARY_CAPACITY];
    }

    /// Recover the codepoint that a render-side slot index points at.
    /// Used by the rasterizer to look up the outline from an atlas key.
    pub fn cp_for_index(&self, index: u8) -> Option<u32> {
        self.inner.read().indexed[index as usize]
    }

    /// Look up a registration.
    pub fn get(&self, cp: u32) -> Option<RegisteredGlyph> {
        self.inner.read().by_cp.get(&cp).cloned()
    }

    /// True iff `cp` has a live custom registration.
    pub fn contains(&self, cp: u32) -> bool {
        self.inner.read().by_cp.contains_key(&cp)
    }

    /// Live registration count. Exposed for tests and telemetry.
    pub fn len(&self) -> usize {
        self.inner.read().by_cp.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pua_check_matches_spec() {
        assert!(is_pua(0xE000));
        assert!(is_pua(0xE0A0));
        assert!(is_pua(0xF8FF));
        assert!(is_pua(0xF_0000));
        assert!(is_pua(0x10_0000));
        assert!(!is_pua(0x61)); // 'a'
        assert!(!is_pua(0x1F600)); // emoji
    }

    fn glyf(bytes: Vec<u8>) -> StoredPayload {
        StoredPayload::glyf(bytes)
    }

    fn assert_glyf_bytes(p: &StoredPayload, expected: &[u8]) {
        match p {
            StoredPayload::Glyf { glyf } => assert_eq!(glyf, expected),
            other => panic!("expected Glyf, got {:?}", other),
        }
    }

    #[test]
    fn register_and_lookup() {
        let r = GlyphRegistry::new();
        assert_eq!(r.register(0xE0A0, glyf(vec![1, 2, 3]), 1000).unwrap(), None);
        let g = r.get(0xE0A0).unwrap();
        assert_glyf_bytes(&g.payload, &[1, 2, 3]);
        assert_eq!(g.upm, 1000);
        assert!(r.contains(0xE0A0));
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn register_rejects_non_pua() {
        let r = GlyphRegistry::new();
        assert_eq!(
            r.register(0x61, glyf(vec![1]), 1000),
            Err(RegisterRejection::OutOfNamespace)
        );
        assert!(r.is_empty());
    }

    #[test]
    fn register_overwrites_preserving_insertion_order_and_index() {
        let r = GlyphRegistry::new();
        r.register(0xE0A0, glyf(vec![1]), 1000).unwrap();
        r.register(0xE0A1, glyf(vec![2]), 1000).unwrap();
        let idx_before = r.get(0xE0A0).unwrap().index;
        // Overwrite the first entry — index and insertion order stable.
        r.register(0xE0A0, glyf(vec![9]), 2048).unwrap();
        let a = r.get(0xE0A0).unwrap();
        let b = r.get(0xE0A1).unwrap();
        assert_glyf_bytes(&a.payload, &[9]);
        assert_eq!(a.upm, 2048);
        assert_eq!(a.index, idx_before);
        assert!(a.insertion_id < b.insertion_id);
    }

    #[test]
    fn distinct_registrations_get_distinct_indices() {
        let r = GlyphRegistry::new();
        r.register(0xE0A0, glyf(vec![1]), 1000).unwrap();
        r.register(0xE0A1, glyf(vec![2]), 1000).unwrap();
        r.register(0xE0A2, glyf(vec![3]), 1000).unwrap();
        let i0 = r.get(0xE0A0).unwrap().index;
        let i1 = r.get(0xE0A1).unwrap().index;
        let i2 = r.get(0xE0A2).unwrap().index;
        assert_ne!(i0, i1);
        assert_ne!(i1, i2);
        assert_ne!(i0, i2);
        assert_eq!(r.cp_for_index(i0), Some(0xE0A0));
        assert_eq!(r.cp_for_index(i1), Some(0xE0A1));
    }

    #[test]
    fn cleared_slot_is_reused_by_next_registration() {
        let r = GlyphRegistry::new();
        r.register(0xE0A0, glyf(vec![1]), 1000).unwrap();
        let old_index = r.get(0xE0A0).unwrap().index;
        r.clear_one(0xE0A0);
        assert_eq!(r.cp_for_index(old_index), None);
        r.register(0xE0A1, glyf(vec![2]), 1000).unwrap();
        assert_eq!(r.get(0xE0A1).unwrap().index, old_index);
    }

    #[test]
    fn clear_one_removes_registration() {
        let r = GlyphRegistry::new();
        r.register(0xE0A0, glyf(vec![1]), 1000).unwrap();
        r.register(0xE0A1, glyf(vec![2]), 1000).unwrap();
        r.clear_one(0xE0A0);
        assert!(!r.contains(0xE0A0));
        assert!(r.contains(0xE0A1));
    }

    #[test]
    fn clear_one_unknown_is_noop() {
        let r = GlyphRegistry::new();
        r.clear_one(0xE0A0);
        assert!(r.is_empty());
    }

    #[test]
    fn clear_all_drops_everything() {
        let r = GlyphRegistry::new();
        r.register(0xE0A0, glyf(vec![1]), 1000).unwrap();
        r.register(0xE0A1, glyf(vec![2]), 1000).unwrap();
        r.clear_all();
        assert!(r.is_empty());
    }

    #[test]
    fn fifo_eviction_on_capacity() {
        let r = GlyphRegistry::new();
        // Fill the glossary using contiguous PUA codepoints.
        for i in 0..GLOSSARY_CAPACITY as u32 {
            r.register(0xE000 + i, glyf(vec![i as u8]), 1000).unwrap();
        }
        assert_eq!(r.len(), GLOSSARY_CAPACITY);

        // 257th register evicts the oldest (U+E000) to make room.
        let evicted = r.register(0xE500, glyf(vec![0xFF]), 1000).unwrap();
        assert_eq!(evicted, Some(0xE000));
        assert!(!r.contains(0xE000));
        assert!(r.contains(0xE500));
        assert_eq!(r.len(), GLOSSARY_CAPACITY);
    }

    #[test]
    fn overwrite_at_capacity_does_not_evict() {
        let r = GlyphRegistry::new();
        for i in 0..GLOSSARY_CAPACITY as u32 {
            r.register(0xE000 + i, glyf(vec![i as u8]), 1000).unwrap();
        }
        // Overwriting an existing codepoint MUST NOT evict.
        let evicted = r.register(0xE000, glyf(vec![0xAB]), 1000).unwrap();
        assert_eq!(evicted, None);
        assert_eq!(r.len(), GLOSSARY_CAPACITY);
        assert_glyf_bytes(&r.get(0xE000).unwrap().payload, &[0xAB]);
    }

    #[test]
    fn registry_is_arc_shareable() {
        let r1 = GlyphRegistry::new();
        let r2 = r1.clone();
        r1.register(0xE0A0, glyf(vec![1]), 1000).unwrap();
        assert!(r2.contains(0xE0A0));
    }

    // ----- colour payload storage round-trip --------------------------

    #[test]
    fn register_stores_colrv0_payload() {
        let r = GlyphRegistry::new();
        let colr = vec![0xC0, 0x00, 0x01];
        let cpal = vec![0xCA, 0xFE];
        r.register(
            0xE0A0,
            StoredPayload::ColrV0 {
                glyphs: vec![vec![0xA], vec![0xB, 0xC]],
                colr: colr.clone(),
                cpal: cpal.clone(),
            },
            1024,
        )
        .unwrap();
        match r.get(0xE0A0).unwrap().payload {
            StoredPayload::ColrV0 {
                glyphs,
                colr: c,
                cpal: p,
            } => {
                assert_eq!(glyphs.len(), 2);
                assert_eq!(c, colr);
                assert_eq!(p, cpal);
            }
            other => panic!("expected ColrV0, got {:?}", other),
        }
    }

    #[test]
    fn register_stores_colrv1_payload() {
        let r = GlyphRegistry::new();
        r.register(
            0x100000,
            StoredPayload::ColrV1 {
                glyphs: vec![vec![0xDE, 0xAD]],
                colr: vec![0x01; 12],
                cpal: vec![],
            },
            2048,
        )
        .unwrap();
        assert!(matches!(
            r.get(0x100000).unwrap().payload,
            StoredPayload::ColrV1 { .. }
        ));
    }

    #[test]
    fn overwrite_replaces_payload_across_formats() {
        // Re-registering the same codepoint with a different format
        // should swap the stored payload while preserving index and
        // insertion order (FIFO eviction invariant).
        let r = GlyphRegistry::new();
        r.register(0xE0A0, glyf(vec![1, 2, 3]), 1000).unwrap();
        let idx = r.get(0xE0A0).unwrap().index;
        r.register(
            0xE0A0,
            StoredPayload::ColrV0 {
                glyphs: vec![vec![0]],
                colr: vec![0x00; 4],
                cpal: vec![],
            },
            1000,
        )
        .unwrap();
        let g = r.get(0xE0A0).unwrap();
        assert_eq!(g.index, idx);
        assert!(matches!(g.payload, StoredPayload::ColrV0 { .. }));
    }
}
