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
// Registrations are confined to the registration block (spec §4),
// `U+100000`–`U+1003FF`, and the glossary has one slot per block
// codepoint, so nothing is ever evicted. Cleared codepoints are
// removed immediately; re-registering a codepoint replaces its entry.
// Each
// registration costs one slot regardless of payload type: a
// `ColrV0`/`ColrV1` container with 200 inner outlines still occupies
// a single glossary slot. The `n_glyphs`
// cap inside a COLR payload (see `rio_backend::ansi::glyph_protocol::
// MAX_COLR_GLYPHS`) is a separate per-payload limit.

use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock};

pub use crate::glyph_layout::{GlyphAlignH, GlyphAlignV, GlyphLayout, GlyphSize};

/// Sentinel `font_id` returned by `FontLibraryData::find_best_font_match`
/// when the codepoint has a live registration. The rasterizer branches
/// on this value to skip the charmap/shape path and render from the
/// registry instead.
pub const CUSTOM_GLYPH_FONT_ID: usize = usize::MAX;

/// Same sentinel typed for the grid renderer's u32 atlas key. Equal to
/// `CUSTOM_GLYPH_FONT_ID as u32` on every supported target; we expose
/// it as its own const so call sites stay free of `as` casts.
pub const CUSTOM_GLYPH_FONT_ID_U32: u32 = u32::MAX;

/// The registry is keyed by the registration block (spec §4); the
/// wire parser checks the same definition.
pub use crate::glyph_namespace::{
    in_registration_block, GLOSSARY_CAPACITY, REGISTRATION_BLOCK_END,
    REGISTRATION_BLOCK_START,
};

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

/// Facts about a payload that only rendering reveals, kept with the
/// registration so they're worked out once.
#[derive(Debug, Default)]
pub struct RenderState {
    /// The payload failed to rasterise (a malformed COLR table, an empty
    /// outline), so the renderer needn't retry it every frame.
    pub unrenderable: AtomicBool,
    /// Whether the paint graph asks for the foreground colour.
    pub uses_foreground: OnceLock<bool>,
}

/// A single registered glyph. The raw payload is retained so the
/// renderer can re-rasterize at any cell size without re-transmitting.
///
/// `payload` is held behind an `Arc` so [`GlyphRegistry::get`] — and
/// every render-path lookup that goes through it — clones a refcount
/// bump rather than the underlying bytes. A single `ColrV1` payload
/// can carry up to 1024 inner outlines plus the COLR/CPAL tables
/// (megabytes worst-case); without this indirection every atlas miss
/// for that codepoint at a new size bucket would memcpy the lot.
#[derive(Debug, Clone)]
pub struct RegisteredGlyph {
    pub payload: Arc<StoredPayload>,
    pub upm: u16,
    /// Declared render span: `1` (narrow) or `2` (wide). A render-time
    /// hint only; the codepoint's logical cell width stays at system
    /// wcwidth, the renderer overflows the extra cell in pixels.
    pub width: u8,
    /// §8.5 sizing and placement.
    pub layout: GlyphLayout,
    /// Bumps on every register call (fresh OR overwrite). Part of the
    /// atlas key, so any mutation produces a fresh slot and prevents a
    /// post-clear or post-overwrite render from serving the previous
    /// rasterisation.
    pub version: u32,
    /// What the renderer has learned about this payload. Replaced on
    /// every registration, so it can never outlive the payload.
    pub render_state: Arc<RenderState>,
}

#[derive(Debug, Default)]
struct Inner {
    by_cp: FxHashMap<u32, RegisteredGlyph>,
    /// Monotonic counter stamped onto every registration's `version`.
    next_version: u32,
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
    /// `cp` is outside the registration block. Callers should catch
    /// this at the parser; this is a defence-in-depth check.
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

    /// Register a glyph at a codepoint in the registration block, with
    /// its declared render span and §8.5 layout. If the codepoint is
    /// already registered, its entry is replaced in the same slot.
    pub fn register(
        &self,
        cp: u32,
        payload: StoredPayload,
        upm: u16,
        width: u8,
        layout: GlyphLayout,
    ) -> Result<(), RegisterRejection> {
        if !in_registration_block(cp) {
            return Err(RegisterRejection::OutOfNamespace);
        }
        // Wrap once; subsequent `get` clones share this Arc.
        let payload = Arc::new(payload);
        let mut inner = self.inner.write();

        // Allocate a fresh `version` regardless of whether this is an
        // overwrite or a brand-new registration: any payload change
        // must produce a new atlas key so previously-rasterised pixels
        // for `cp` stop being served.
        let version = inner.next_version;
        inner.next_version = inner.next_version.wrapping_add(1);

        // Every block codepoint has its own slot, so registering never
        // needs to make room, and an overwrite replaces the entry.
        inner.by_cp.insert(
            cp,
            RegisteredGlyph {
                payload,
                upm,
                width,
                layout,
                version,
                render_state: Arc::default(),
            },
        );
        Ok(())
    }

    /// Clear one codepoint. No-op if nothing was registered.
    pub fn clear_one(&self, cp: u32) {
        self.inner.write().by_cp.remove(&cp);
    }

    /// Drop every registration.
    pub fn clear_all(&self) {
        self.inner.write().by_cp.clear();
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
    fn overwriting_a_registration_clears_its_failure() {
        use std::sync::atomic::Ordering;

        let r = GlyphRegistry::new();
        r.register(0x10_0001, glyf(vec![1]), 1000, 1, GlyphLayout::default())
            .unwrap();
        r.get(0x10_0001)
            .unwrap()
            .render_state
            .unrenderable
            .store(true, Ordering::Relaxed);
        assert!(r
            .get(0x10_0001)
            .unwrap()
            .render_state
            .unrenderable
            .load(Ordering::Relaxed));

        r.register(0x10_0001, glyf(vec![2]), 1000, 1, GlyphLayout::default())
            .unwrap();
        assert!(!r
            .get(0x10_0001)
            .unwrap()
            .render_state
            .unrenderable
            .load(Ordering::Relaxed));
    }

    #[test]
    fn register_overwrite_bumps_version_for_atlas_busting() {
        let r = GlyphRegistry::new();
        r.register(0x10_0000, glyf(vec![0xAA]), 1000, 1, GlyphLayout::default())
            .unwrap();
        let v_first = r.get(0x10_0000).unwrap().version;

        r.register(0x10_0000, glyf(vec![0xBB]), 1000, 1, GlyphLayout::default())
            .unwrap();
        let v_second = r.get(0x10_0000).unwrap().version;

        assert_ne!(
            v_first, v_second,
            "overwrite must produce a new version so atlas re-rasterises"
        );
    }

    #[test]
    fn clear_then_reregister_yields_new_version() {
        let r = GlyphRegistry::new();
        r.register(0x10_0000, glyf(vec![0xAA]), 1000, 1, GlyphLayout::default())
            .unwrap();
        let v_first = r.get(0x10_0000).unwrap().version;

        r.clear_one(0x10_0000);
        r.register(0x10_0000, glyf(vec![0xBB]), 1000, 1, GlyphLayout::default())
            .unwrap();
        let v_second = r.get(0x10_0000).unwrap().version;

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
        r.register(
            0x10_0000,
            glyf(vec![1, 2, 3]),
            1000,
            1,
            GlyphLayout::default(),
        )
        .unwrap();
        let g = r.get(0x10_0000).unwrap();
        assert_glyf_bytes(&g.payload, &[1, 2, 3]);
        assert_eq!(g.upm, 1000);
        assert!(r.contains(0x10_0000));
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn register_rejects_outside_the_block() {
        let r = GlyphRegistry::new();
        for cp in [0x61, 0xE0A0, 0xF_0000, 0x10_0400] {
            assert_eq!(
                r.register(cp, glyf(vec![1]), 1000, 1, GlyphLayout::default()),
                Err(RegisterRejection::OutOfNamespace)
            );
        }
        assert!(r.is_empty());
    }

    #[test]
    fn register_overwrites_in_place() {
        let r = GlyphRegistry::new();
        r.register(0x10_0000, glyf(vec![1]), 1000, 1, GlyphLayout::default())
            .unwrap();
        r.register(0x10_0001, glyf(vec![2]), 1000, 1, GlyphLayout::default())
            .unwrap();
        let v_before = r.get(0x10_0000).unwrap().version;
        // Overwrite the first entry: new payload and version, the other
        // entry untouched.
        r.register(0x10_0000, glyf(vec![9]), 2048, 1, GlyphLayout::default())
            .unwrap();
        let a = r.get(0x10_0000).unwrap();
        let b = r.get(0x10_0001).unwrap();
        assert_glyf_bytes(&a.payload, &[9]);
        assert_eq!(a.upm, 2048);
        assert_ne!(a.version, v_before);
        assert_glyf_bytes(&b.payload, &[2]);
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn clear_one_removes_registration() {
        let r = GlyphRegistry::new();
        r.register(0x10_0000, glyf(vec![1]), 1000, 1, GlyphLayout::default())
            .unwrap();
        r.register(0x10_0001, glyf(vec![2]), 1000, 1, GlyphLayout::default())
            .unwrap();
        r.clear_one(0x10_0000);
        assert!(!r.contains(0x10_0000));
        assert!(r.contains(0x10_0001));
    }

    #[test]
    fn clear_one_unknown_is_noop() {
        let r = GlyphRegistry::new();
        r.clear_one(0x10_0000);
        assert!(r.is_empty());
    }

    #[test]
    fn clear_all_drops_everything() {
        let r = GlyphRegistry::new();
        r.register(0x10_0000, glyf(vec![1]), 1000, 1, GlyphLayout::default())
            .unwrap();
        r.register(0x10_0001, glyf(vec![2]), 1000, 1, GlyphLayout::default())
            .unwrap();
        r.clear_all();
        assert!(r.is_empty());
    }

    #[test]
    fn whole_block_fits_and_nothing_is_evicted() {
        let r = GlyphRegistry::new();
        for cp in REGISTRATION_BLOCK_START..=REGISTRATION_BLOCK_END {
            r.register(cp, glyf(vec![cp as u8]), 1000, 1, GlyphLayout::default())
                .unwrap();
        }
        assert_eq!(r.len(), GLOSSARY_CAPACITY);
        assert!(r.contains(REGISTRATION_BLOCK_START));

        // Overwriting at capacity replaces in place.
        r.register(0x10_0000, glyf(vec![0xAB]), 1000, 1, GlyphLayout::default())
            .unwrap();
        assert_eq!(r.len(), GLOSSARY_CAPACITY);
        assert_glyf_bytes(&r.get(0x10_0000).unwrap().payload, &[0xAB]);
        assert!(
            (REGISTRATION_BLOCK_START..=REGISTRATION_BLOCK_END).all(|cp| r.contains(cp))
        );
    }

    #[test]
    fn registry_is_arc_shareable() {
        let r1 = GlyphRegistry::new();
        let r2 = r1.clone();
        r1.register(0x10_0000, glyf(vec![1]), 1000, 1, GlyphLayout::default())
            .unwrap();
        assert!(r2.contains(0x10_0000));
    }

    #[test]
    fn register_stores_and_overwrites_layout() {
        let r = GlyphRegistry::new();
        let layout = GlyphLayout {
            aw: Some(350),
            size: GlyphSize::Advance,
            ..Default::default()
        };
        r.register(0x10_0001, glyf(vec![1]), 1000, 1, layout)
            .unwrap();
        assert_eq!(r.get(0x10_0001).unwrap().layout, layout);
        r.register(0x10_0001, glyf(vec![2]), 1000, 1, GlyphLayout::default())
            .unwrap();
        assert_eq!(r.get(0x10_0001).unwrap().layout, GlyphLayout::default());
    }

    #[test]
    fn register_stores_render_hint() {
        let r = GlyphRegistry::new();
        r.register(0x10_0000, glyf(vec![1]), 1000, 1, GlyphLayout::default())
            .unwrap(); // default narrow
        assert_eq!(r.get(0x10_0000).unwrap().width, 1);

        r.register(0x10_0001, glyf(vec![2]), 1000, 2, GlyphLayout::default())
            .unwrap();
        assert_eq!(r.get(0x10_0001).unwrap().width, 2);

        // Overwrite narrows it back; the stored render hint follows.
        r.register(0x10_0001, glyf(vec![3]), 1000, 1, GlyphLayout::default())
            .unwrap();
        assert_eq!(r.get(0x10_0001).unwrap().width, 1);
    }

    #[test]
    fn register_stores_colrv0_payload() {
        let r = GlyphRegistry::new();
        let colr = vec![0xC0, 0x00, 0x01];
        let cpal = vec![0xCA, 0xFE];
        r.register(
            0x10_0000,
            StoredPayload::ColrV0 {
                glyphs: vec![vec![0xA], vec![0xB, 0xC]],
                colr: colr.clone(),
                cpal: cpal.clone(),
            },
            1024,
            1,
            GlyphLayout::default(),
        )
        .unwrap();
        match &*r.get(0x10_0000).unwrap().payload {
            StoredPayload::ColrV0 {
                glyphs,
                colr: c,
                cpal: p,
            } => {
                assert_eq!(glyphs.len(), 2);
                assert_eq!(c, &colr);
                assert_eq!(p, &cpal);
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
            1,
            GlyphLayout::default(),
        )
        .unwrap();
        assert!(matches!(
            &*r.get(0x100000).unwrap().payload,
            StoredPayload::ColrV1 { .. }
        ));
    }

    #[test]
    fn overwrite_replaces_payload_across_formats() {
        // Re-registering the same codepoint with a different format
        // should swap the stored payload.
        let r = GlyphRegistry::new();
        r.register(
            0x10_0000,
            glyf(vec![1, 2, 3]),
            1000,
            1,
            GlyphLayout::default(),
        )
        .unwrap();
        r.register(
            0x10_0000,
            StoredPayload::ColrV0 {
                glyphs: vec![vec![0]],
                colr: vec![0x00; 4],
                cpal: vec![],
            },
            1000,
            1,
            GlyphLayout::default(),
        )
        .unwrap();
        let g = r.get(0x10_0000).unwrap();
        assert_eq!(r.len(), 1);
        assert!(matches!(&*g.payload, StoredPayload::ColrV0 { .. }));
    }
}
