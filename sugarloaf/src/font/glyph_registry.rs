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
// The registry holds at most 1024 simultaneous entries. On the 1025th
// register, the oldest entry is evicted (FIFO) to make room. Cleared
// codepoints are removed immediately; re-registering a codepoint
// overwrites the previous entry without affecting FIFO order of
// other entries. Each registration costs one slot regardless of
// payload type — a `ColrV0`/`ColrV1` container with 200 inner
// outlines still occupies a single glossary slot. The `n_glyphs`
// cap inside a COLR payload (see `rio_backend::ansi::glyph_protocol::
// MAX_COLR_GLYPHS`) is a separate per-payload limit.

use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Sentinel `font_id` returned by `FontLibraryData::find_best_font_match`
/// when the codepoint has a live registration. The rasterizer branches
/// on this value to skip the charmap/shape path and render from the
/// registry instead.
pub const CUSTOM_GLYPH_FONT_ID: usize = usize::MAX;

/// Same sentinel typed for the grid renderer's u32 atlas key. Equal to
/// `CUSTOM_GLYPH_FONT_ID as u32` on every supported target; we expose
/// it as its own const so call sites stay free of `as` casts.
pub const CUSTOM_GLYPH_FONT_ID_U32: u32 = u32::MAX;

/// Pack a `(codepoint, version)` pair into the 32-bit `glyph_id` field
/// the grid atlas uses. Each register/clear bumps the registration's
/// `version`, so re-registering the same codepoint produces a fresh
/// atlas key and never serves stale rasterisation. PUA codepoints fit
/// in 21 bits; the remaining 11 bits give 2048 versions before
/// wraparound — large enough that an unrelated collision is
/// astronomical for any realistic register/clear cadence.
#[inline]
pub fn pack_atlas_glyph_id(codepoint: u32, version: u32) -> u32 {
    ((version & 0x7FF) << 21) | (codepoint & 0x1F_FFFF)
}

/// Maximum simultaneous registrations per session (spec §4). Each
/// registration is one slot regardless of payload type.
pub const GLOSSARY_CAPACITY: usize = 1024;

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
    /// Stable render-side index in `0..GLOSSARY_CAPACITY`. The
    /// renderer's glyph-id field is u16, so the slot id fits directly.
    /// Indices are reused after eviction or explicit clear, so the
    /// atlas cache must be invalidated for a (slot_id, *) pair
    /// whenever that happens.
    pub index: u16,
    /// Per-registration insertion id used to order entries for FIFO
    /// eviction. A larger id means "registered later."
    pub insertion_id: u64,
    /// Bumps on every register call (fresh OR overwrite). The atlas
    /// key for a custom glyph is `(CUSTOM_FONT_ID, pack(cp, version),
    /// size)`, so any mutation produces a fresh slot and prevents a
    /// post-clear or post-overwrite render from serving the previous
    /// rasterisation.
    pub version: u32,
}

#[derive(Debug)]
struct Inner {
    by_cp: FxHashMap<u32, RegisteredGlyph>,
    /// Reverse map: `indexed[i]` holds the codepoint currently using
    /// slot index `i`, or `None` if the slot is free.
    indexed: [Option<u32>; GLOSSARY_CAPACITY],
    /// Monotonic counter that stamps each registration so we can
    /// evict the oldest in O(n) over `GLOSSARY_CAPACITY` entries.
    next_insertion: u64,
    /// Monotonic counter stamped onto every registration's `version`.
    /// Wraps around (≈4 billion before wrap) — collisions only matter
    /// modulo the 11-bit window the atlas key uses, which is checked
    /// in `pack_atlas_glyph_id`.
    next_version: u32,
}

impl Default for Inner {
    fn default() -> Self {
        Self {
            by_cp: FxHashMap::default(),
            indexed: [None; GLOSSARY_CAPACITY],
            next_insertion: 0,
            next_version: 0,
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

    /// `true` if `self` and `other` share the same underlying `Arc`.
    /// Used by [`FontLibrary::attach_glyph_registry`] to skip the
    /// write lock when the same registry is being re-attached on
    /// every frame.
    #[inline]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// Register a glyph at a PUA codepoint.
    ///
    /// If the codepoint is already registered, the outline is replaced
    /// and the existing insertion order and slot index are preserved.
    ///
    /// If the glossary is full (`GLOSSARY_CAPACITY` entries) and `cp` is NOT already
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

        // Allocate a fresh `version` regardless of whether this is an
        // overwrite or a brand-new registration: any payload change
        // must produce a new atlas key so previously-rasterised pixels
        // for `cp` stop being served.
        let version = inner.next_version;
        inner.next_version = inner.next_version.wrapping_add(1);

        // Overwrite path: reuse the existing slot index. Insertion id
        // is preserved so overwrite does not refresh eviction order.
        if let Some(existing) = inner.by_cp.get_mut(&cp) {
            existing.payload = payload;
            existing.upm = upm;
            existing.version = version;
            return Ok(None);
        }

        // Fresh insertion. Find a free slot; if none, evict the
        // oldest entry and take its slot.
        let (slot_index, evicted) = match inner.indexed.iter().position(|s| s.is_none()) {
            Some(i) => (i as u16, None),
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
                version,
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
    pub fn cp_for_index(&self, index: u16) -> Option<u32> {
        self.inner
            .read()
            .indexed
            .get(index as usize)
            .copied()
            .flatten()
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

    #[test]
    fn pack_atlas_glyph_id_is_unique_per_codepoint_and_version() {
        // Distinct codepoints with the same version → distinct keys.
        assert_ne!(
            pack_atlas_glyph_id(0xE0A0, 0),
            pack_atlas_glyph_id(0xE0A1, 0)
        );
        // Distinct versions with the same codepoint → distinct keys
        // (this is the property that fixes the stale-atlas bug).
        assert_ne!(
            pack_atlas_glyph_id(0xE0A0, 0),
            pack_atlas_glyph_id(0xE0A0, 1)
        );
        // Wrap-around at the 11-bit version window. Documented
        // limitation: register/clear/re-register more than 2048
        // times for the same cp would alias. Acceptable for any
        // realistic cadence.
        assert_eq!(
            pack_atlas_glyph_id(0xE0A0, 0),
            pack_atlas_glyph_id(0xE0A0, 0x800)
        );
    }

    #[test]
    fn register_overwrite_bumps_version_for_atlas_busting() {
        let r = GlyphRegistry::new();
        r.register(0xE0A0, glyf(vec![0xAA]), 1000).unwrap();
        let v_first = r.get(0xE0A0).unwrap().version;

        r.register(0xE0A0, glyf(vec![0xBB]), 1000).unwrap();
        let v_second = r.get(0xE0A0).unwrap().version;

        assert_ne!(
            v_first, v_second,
            "overwrite must produce a new version so atlas re-rasterises"
        );
    }

    #[test]
    fn clear_then_reregister_yields_new_version() {
        let r = GlyphRegistry::new();
        r.register(0xE0A0, glyf(vec![0xAA]), 1000).unwrap();
        let v_first = r.get(0xE0A0).unwrap().version;

        r.clear_one(0xE0A0);
        r.register(0xE0A0, glyf(vec![0xBB]), 1000).unwrap();
        let v_second = r.get(0xE0A0).unwrap().version;

        assert_ne!(
            v_first, v_second,
            "register-after-clear must produce a new version"
        );
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

        // The next register evicts the oldest (U+E000) to make room.
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
