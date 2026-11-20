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
// codepoint, so nothing is ever evicted.
//
// A registration is bound to text when it is printed (spec §6.3): the
// terminal asks [`GlyphRegistry::bind`] for the codepoint's current
// version and stores it in the cell, and the renderer draws the cell
// from that version. Re-registering or clearing a codepoint changes
// which version later text gets; a version text was printed with is
// *retired*, kept until the terminal's mark-and-sweep finds no cell
// using it ([`GlyphRegistry::retain_retired`]). One never printed is
// dropped at once. Each
// registration costs one slot regardless of payload type: a
// `ColrV0`/`ColrV1` container with 200 inner outlines still occupies
// a single glossary slot. The `n_glyphs`
// cap inside a COLR payload (see `rio_backend::ansi::glyph_protocol::
// MAX_COLR_GLYPHS`) is a separate per-payload limit.

use parking_lot::RwLock;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
#[derive(Debug, Clone, PartialEq)]
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
    /// `fmt=vg`: layers of fills, strokes and clips over SVG path
    /// data, validated at registration with arcs already expanded (see
    /// [`crate::glyph_vector`]).
    Vector {
        glyph: crate::glyph_vector::VectorGlyph,
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
    /// Unique per registration (an identical re-registration keeps the
    /// one it would replace). What a cell stores when it is printed,
    /// and part of the atlas key, so a new registration never serves
    /// the previous one's pixels.
    pub version: u32,
    /// Some text was printed with this registration, so it has to
    /// outlive being replaced or cleared.
    pub bound: Arc<AtomicBool>,
    /// What the renderer has learned about this payload. Replaced on
    /// every registration, so it can never outlive the payload.
    pub render_state: Arc<RenderState>,
}

#[derive(Debug, Default)]
struct Inner {
    /// Each codepoint's current registration, as a version.
    current: FxHashMap<u32, u32>,
    /// Every registration still reachable: the current ones, and the
    /// retired ones text may still show.
    by_version: FxHashMap<u32, RegisteredGlyph>,
    /// Versions in `by_version` that are no longer current.
    retired: FxHashSet<u32>,
    /// Monotonic counter stamped onto every registration's `version`.
    next_version: u32,
}

impl RegisteredGlyph {
    /// Registering this again changes nothing.
    fn is_same(
        &self,
        payload: &StoredPayload,
        upm: u16,
        width: u8,
        layout: &GlyphLayout,
    ) -> bool {
        self.upm == upm
            && self.width == width
            && self.layout == *layout
            && *self.payload == *payload
    }
}

impl Inner {
    /// `version` is no longer current: keep it while text may show it,
    /// drop it if no text ever did.
    fn retire(&mut self, version: u32) {
        let bound = self
            .by_version
            .get(&version)
            .is_some_and(|g| g.bound.load(Ordering::Relaxed));
        if bound {
            self.retired.insert(version);
        } else {
            self.by_version.remove(&version);
        }
    }
}

/// Most retired registrations a registry holds before the terminal
/// must reclaim them or refuse new ones with `no_space` (spec §9). The
/// same as the glossary, so live and retired glyphs together stay
/// within twice what the block can hold at once.
pub const MAX_RETIRED: usize = GLOSSARY_CAPACITY;

/// The public registry handle. Cloned cheaply via `Arc`.
#[derive(Debug, Clone)]
pub struct GlyphRegistry {
    inner: Arc<RwLock<Inner>>,
    /// Unique for every registry and never reused. Versions are only
    /// unique within one registry, so a cache shared by several
    /// sessions keys on this too.
    id: u64,
}

impl Default for GlyphRegistry {
    fn default() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        Self {
            inner: Arc::default(),
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        }
    }
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

    /// This registry's identity, shared by its clones. See the field.
    #[inline]
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Register a glyph at a codepoint in the registration block, with
    /// its declared render span and §8.5 layout. If the codepoint is
    /// already registered, the new registration applies to text printed
    /// from now on; one identical to the current registration keeps it,
    /// so re-sending a set of icons costs nothing.
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
        let mut inner = self.inner.write();
        if inner
            .current
            .get(&cp)
            .and_then(|version| inner.by_version.get(version))
            .is_some_and(|existing| existing.is_same(&payload, upm, width, &layout))
        {
            return Ok(());
        }

        // A fresh `version` for every registration that changes
        // something: it is what new text binds to, and it keys the atlas,
        // so the previous registration's pixels are never served for it.
        let version = inner.next_version;
        inner.next_version = inner.next_version.wrapping_add(1);
        inner.by_version.insert(
            version,
            RegisteredGlyph {
                payload: Arc::new(payload),
                upm,
                width,
                layout,
                version,
                bound: Arc::default(),
                render_state: Arc::default(),
            },
        );
        if let Some(old) = inner.current.insert(cp, version) {
            inner.retire(old);
        }
        Ok(())
    }

    /// Whether this is already the current registration at `cp`, so
    /// registering it again keeps it and adds nothing.
    pub fn is_current(
        &self,
        cp: u32,
        payload: &StoredPayload,
        upm: u16,
        width: u8,
        layout: &GlyphLayout,
    ) -> bool {
        let inner = self.inner.read();
        inner
            .current
            .get(&cp)
            .and_then(|version| inner.by_version.get(version))
            .is_some_and(|existing| existing.is_same(payload, upm, width, layout))
    }

    /// Clear one codepoint for text printed from now on. No-op if
    /// nothing was registered.
    pub fn clear_one(&self, cp: u32) {
        let mut inner = self.inner.write();
        if let Some(old) = inner.current.remove(&cp) {
            inner.retire(old);
        }
    }

    /// Clear every codepoint for text printed from now on.
    pub fn clear_all(&self) {
        let mut inner = self.inner.write();
        let old: Vec<u32> = inner.current.drain().map(|(_, v)| v).collect();
        for version in old {
            inner.retire(version);
        }
    }

    /// Drop everything, retired registrations included. For a full
    /// reset, which also erases all the text that could show them.
    pub fn reset(&self) {
        let mut inner = self.inner.write();
        inner.current.clear();
        inner.by_version.clear();
        inner.retired.clear();
    }

    /// The registration text printed now at `cp` binds to, if any: its
    /// version, for the cell to keep. Marks it as shown by text, so it
    /// survives being replaced.
    pub fn bind(&self, cp: u32) -> Option<u32> {
        let inner = self.inner.read();
        let version = *inner.current.get(&cp)?;
        if let Some(glyph) = inner.by_version.get(&version) {
            glyph.bound.store(true, Ordering::Relaxed);
        }
        Some(version)
    }

    /// The registration a cell was printed with.
    pub fn get_version(&self, version: u32) -> Option<RegisteredGlyph> {
        self.inner.read().by_version.get(&version).cloned()
    }

    /// The current registration at `cp`: what text printed now gets.
    pub fn get(&self, cp: u32) -> Option<RegisteredGlyph> {
        let inner = self.inner.read();
        let version = inner.current.get(&cp)?;
        inner.by_version.get(version).cloned()
    }

    /// True iff `cp` has a current registration.
    pub fn contains(&self, cp: u32) -> bool {
        self.inner.read().current.contains_key(&cp)
    }

    /// Current registration count. Exposed for tests and telemetry.
    pub fn len(&self) -> usize {
        self.inner.read().current.len()
    }

    /// Replaced or cleared registrations still kept for text.
    pub fn retired_len(&self) -> usize {
        self.inner.read().retired.len()
    }

    /// Keep only the retired registrations `in_use` says some cell
    /// still shows; the terminal decides with a mark-and-sweep of its
    /// cells. Current registrations are untouched.
    pub fn retain_retired(&self, in_use: impl Fn(u32) -> bool) {
        let mut inner = self.inner.write();
        let dead: Vec<u32> = inner
            .retired
            .iter()
            .copied()
            .filter(|&version| !in_use(version))
            .collect();
        for version in dead {
            inner.retired.remove(&version);
            inner.by_version.remove(&version);
        }
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

    fn reg(r: &GlyphRegistry, cp: u32, byte: u8) -> u32 {
        r.register(cp, glyf(vec![byte]), 1000, 1, GlyphLayout::default())
            .unwrap();
        r.get(cp).unwrap().version
    }

    #[test]
    fn a_printed_registration_outlives_being_replaced() {
        // Spec §6.3: text keeps the registration it was printed with.
        let r = GlyphRegistry::new();
        let first = reg(&r, 0x10_0000, 1);
        assert_eq!(r.bind(0x10_0000), Some(first));
        let second = reg(&r, 0x10_0000, 2);
        assert_ne!(first, second);
        // New text gets the new one; the printed one is still drawable.
        assert_eq!(r.bind(0x10_0000), Some(second));
        assert_glyf_bytes(&r.get_version(first).unwrap().payload, &[1]);
        assert_eq!(r.retired_len(), 1);
        // Cleared, new text gets nothing; printed text keeps its glyph.
        r.clear_one(0x10_0000);
        assert_eq!(r.bind(0x10_0000), None);
        assert!(r.get_version(second).is_some());
        assert_eq!(r.retired_len(), 2);
        assert!(r.is_empty());
    }

    #[test]
    fn a_registration_never_printed_goes_when_replaced() {
        let r = GlyphRegistry::new();
        let first = reg(&r, 0x10_0000, 1);
        reg(&r, 0x10_0000, 2);
        assert!(r.get_version(first).is_none());
        assert_eq!(r.retired_len(), 0);
        let current = r.get(0x10_0000).unwrap().version;
        r.clear_all();
        assert!(r.get_version(current).is_none());
    }

    #[test]
    fn an_identical_registration_keeps_the_current_one() {
        // Re-sending the same icon (a theme change, a restart) costs
        // nothing and doesn't retire what text shows.
        let r = GlyphRegistry::new();
        let first = reg(&r, 0x10_0000, 1);
        r.bind(0x10_0000);
        assert_eq!(reg(&r, 0x10_0000, 1), first);
        assert_eq!(r.retired_len(), 0);
        // Any difference is a new registration: payload, upm, width or
        // layout.
        assert_ne!(reg(&r, 0x10_0000, 9), first);
        let v = r.get(0x10_0000).unwrap().version;
        r.register(0x10_0000, glyf(vec![9]), 2048, 1, GlyphLayout::default())
            .unwrap();
        assert_ne!(r.get(0x10_0000).unwrap().version, v);
        let v = r.get(0x10_0000).unwrap().version;
        r.register(0x10_0000, glyf(vec![9]), 2048, 2, GlyphLayout::default())
            .unwrap();
        assert_ne!(r.get(0x10_0000).unwrap().version, v);
    }

    #[test]
    fn retain_retired_drops_what_no_text_shows() {
        let r = GlyphRegistry::new();
        let a = reg(&r, 0x10_0000, 1);
        r.bind(0x10_0000);
        let b = reg(&r, 0x10_0000, 2);
        r.bind(0x10_0000);
        reg(&r, 0x10_0000, 3);
        assert_eq!(r.retired_len(), 2);
        r.retain_retired(|version| version == b);
        assert!(r.get_version(a).is_none());
        assert!(r.get_version(b).is_some());
        assert_eq!(r.retired_len(), 1);
        // Current registrations are never swept.
        r.retain_retired(|_| false);
        assert!(r.contains(0x10_0000));
        assert_eq!(r.retired_len(), 0);
    }

    #[test]
    fn reset_drops_retired_registrations_too() {
        let r = GlyphRegistry::new();
        let a = reg(&r, 0x10_0000, 1);
        r.bind(0x10_0000);
        reg(&r, 0x10_0000, 2);
        r.reset();
        assert!(r.is_empty());
        assert_eq!(r.retired_len(), 0);
        assert!(r.get_version(a).is_none());
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
