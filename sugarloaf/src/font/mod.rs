pub mod constants;
pub mod fonts;
#[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
pub mod linux;
#[cfg(not(target_arch = "wasm32"))]
pub mod loader;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod metrics;
pub mod nerd_font_attributes;
pub mod text_run_cache;
#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(test)]
mod cjk_metrics_tests;

pub const FONT_ID_REGULAR: usize = 0;

use crate::font::constants::*;
use crate::font::fonts::{parse_unicode, FontStyle};
use crate::font::metrics::{FaceMetrics, Metrics};
use crate::layout::SpanStyle;
use crate::SugarloafErrors;
use dashmap::DashMap;
use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use swash::text::cluster::Parser;
use swash::text::cluster::Token;
use swash::text::cluster::{CharCluster, Status};
use swash::text::Codepoint;
use swash::text::Script;
use swash::{tag_from_bytes, CacheKey, FontRef, Synthesis};

pub use swash::{Style, Weight};

/// Which font face slot a spec is being resolved for. Drives bold/italic
/// trait selection (Ghostty-style), so the user's spec doesn't need to
/// carry a CSS weight number — the slot itself encodes intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

impl Slot {
    #[inline]
    pub fn is_bold(self) -> bool {
        matches!(self, Slot::Bold | Slot::BoldItalic)
    }
    #[inline]
    pub fn is_italic(self) -> bool {
        matches!(self, Slot::Italic | Slot::BoldItalic)
    }
}

// Type alias for the font data cache to improve readability
type FontDataCache = Arc<DashMap<PathBuf, SharedData>>;

// Global font data cache to avoid reloading fonts from disk
// This cache stores font file data indexed by path, so fonts are only loaded once
// and shared across all font library instances. This significantly improves
// performance when the same font is referenced multiple times.
static FONT_DATA_CACHE: OnceLock<FontDataCache> = OnceLock::new();

fn get_font_data_cache() -> &'static FontDataCache {
    FONT_DATA_CACHE.get_or_init(|| Arc::new(DashMap::default()))
}

/// Clears the global font data cache, forcing fonts to be reloaded from disk
/// on next access. This should be called when font configuration changes.
pub fn clear_font_data_cache() {
    if let Some(cache) = FONT_DATA_CACHE.get() {
        cache.clear();
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LookupAttrs {
    pub italic: bool,
    pub bold: bool,
}

pub fn lookup_for_font_match(
    cluster: &mut CharCluster,
    synth: &mut Synthesis,
    library: &FontLibraryData,
    spec: Option<LookupAttrs>,
) -> Option<(usize, bool)> {
    let mut search_result = None;
    let mut font_synth = Synthesis::default();

    let fonts_len: usize = library.inner.len();
    for font_id in 0..fonts_len {
        let mut is_emoji = false;

        if let Some(font) = library.inner.get(&font_id) {
            is_emoji = font.is_emoji;
            font_synth = font.synth;

            if let Some(spec) = spec {
                if spec.italic && !font.is_italic() && !font.should_italicize {
                    continue;
                }
                if spec.bold && !font.is_bold() && !font.should_embolden {
                    continue;
                }
            }
        }

        #[cfg(target_os = "macos")]
        let matched = {
            // Ask the CTFont directly whether it carries a glyph for each
            // codepoint. Avoids the `get_data` byte load — the fallback
            // walk no longer touches the font file(s) at all.
            let handle_opt = library.inner.get(&font_id).and_then(|font| {
                if let Some(path) = &font.path {
                    crate::font::macos::FontHandle::from_path(path)
                } else if let Some(bytes) = &font.data {
                    crate::font::macos::FontHandle::from_bytes(bytes.as_ref())
                } else {
                    None
                }
            });
            if let Some(handle) = handle_opt {
                let status = cluster.map(|ch| {
                    // Non-zero u16 == "has glyph"; swash's cluster.map only
                    // distinguishes zero vs non-zero, so `1` is fine as a
                    // placeholder when CTFont carries the codepoint.
                    if crate::font::macos::font_has_char(&handle, ch) {
                        1
                    } else {
                        0
                    }
                });
                status != Status::Discard
            } else {
                false
            }
        };

        #[cfg(not(target_os = "macos"))]
        let matched = {
            if let Some((shared_data, offset, key)) = library.get_data(&font_id) {
                let font_ref = FontRef {
                    data: shared_data.as_ref(),
                    offset,
                    key,
                };
                let charmap = font_ref.charmap();
                let status = cluster.map(|ch| charmap.map(ch));
                status != Status::Discard
            } else {
                false
            }
        };

        if matched {
            *synth = font_synth;
            search_result = Some((font_id, is_emoji));
            break;
        }
    }

    if search_result.is_none() && spec.is_some() {
        return lookup_for_font_match(cluster, synth, library, None);
    }

    search_result
}

#[derive(Clone)]
pub struct FontLibrary {
    pub inner: Arc<RwLock<FontLibraryData>>,
}

impl FontLibrary {
    pub fn new(spec: SugarloafFonts) -> (Self, Option<SugarloafErrors>) {
        let mut font_library = FontLibraryData::default();

        let mut sugarloaf_errors = None;

        let fonts_not_found = font_library.load(spec);
        if !fonts_not_found.is_empty() {
            sugarloaf_errors = Some(SugarloafErrors { fonts_not_found });
        }

        (
            Self {
                inner: Arc::new(RwLock::new(font_library)),
            },
            sugarloaf_errors,
        )
    }

    /// Parsed CoreText font for `font_id` — a direct read of the handle
    /// stored on the corresponding `FontData` (per-font pointer rather
    /// than a library-global cache).
    ///
    /// Clone is a cheap CF retain; callers clone out to escape the read
    /// lock scope. Returns `None` for unknown font ids or for fonts that
    /// weren't eagerly given a handle at construction (non-macOS test
    /// fonts loaded via `from_slice`).
    ///
    /// parking_lot's `RwLock` supports recursive reads, so calling this
    /// from code that already holds a read lock on `inner` is safe.
    #[cfg(target_os = "macos")]
    pub fn ct_font(&self, font_id: usize) -> Option<crate::font::macos::FontHandle> {
        self.inner
            .read()
            .inner
            .get(&font_id)
            .and_then(|f| f.handle().cloned())
    }

    /// Resolve a PostScript name back to Rio's `font_id`. Returns
    /// `None` when the library doesn't hold a font with that name.
    pub fn font_id_for_postscript_name(&self, name: &str) -> Option<usize> {
        self.inner.read().font_id_for_postscript_name(name)
    }

    /// Resolve `ch` to a Rio `(font_id, is_emoji)`, walking the
    /// registered fonts first and falling back to CoreText's cascade
    /// via `CTFontCreateForString` when no registered font carries
    /// the glyph. Discovered fonts are registered in-place so
    /// subsequent queries for the same codepoint (or any codepoint
    /// the discovered font covers) hit the registered-font walk
    /// without re-invoking CoreText.
    ///
    /// Pre-shaping resolution with lazy discovery: the shaper
    /// operates on a single font per call, and this method
    /// guarantees that font covers the codepoint, so `CTLine` never
    /// has to cascade-substitute at shape time.
    ///
    /// Returns `(0, false)` only when the platform discovery layer
    /// can't find any font for the codepoint (truly unsupported by
    /// the system) or when the library is empty. Both cases render
    /// as tofu.
    pub fn resolve_font_for_char(
        &self,
        ch: char,
        fragment_style: &SpanStyle,
    ) -> (usize, bool) {
        // Fast path: codepoint is covered by an already-registered
        // font. No locks upgraded, no FFI call. Shared across all
        // platforms — only the cascade-discovery slow path differs.
        if let Some(found) = self
            .inner
            .read()
            .find_best_font_match_strict(ch, fragment_style)
        {
            return found;
        }

        self.cascade_discover(ch, fragment_style)
            .unwrap_or((0, false))
    }

    /// Slow-path cascade discovery — register a fallback font on
    /// first hit so future queries land in the fast path. Per-platform
    /// because the underlying API differs (CoreText `CTFontCreateForString`
    /// on macOS, fontconfig `FcFontSort` on Linux, font-kit walk on
    /// Windows). Returns `None` when no system font covers `ch`.
    #[cfg(target_os = "macos")]
    fn cascade_discover(
        &self,
        ch: char,
        _fragment_style: &SpanStyle,
    ) -> Option<(usize, bool)> {
        let primary = self.ct_font(FONT_ID_REGULAR)?;
        let discovered = crate::font::macos::discover_fallback(&primary, ch)?;
        let ps_name = discovered.postscript_name();

        if let Some(found) = self.dedupe_existing(&ps_name) {
            return Some(found);
        }

        let mut lib = self.inner.write();
        if let Some(existing) = lib.font_id_for_postscript_name(&ps_name) {
            let is_emoji = lib
                .inner
                .get(&existing)
                .map(|fd| fd.is_emoji)
                .unwrap_or(false);
            return Some((existing, is_emoji));
        }
        let font_data = FontData::from_ctfont_macos(discovered);
        let is_emoji = font_data.is_emoji;
        let new_id = lib.inner.len();
        lib.insert(font_data);
        tracing::debug!(
            "CoreText cascade discovered {} for U+{:04X}, registered as font_id {}",
            ps_name,
            ch as u32,
            new_id
        );
        Some((new_id, is_emoji))
    }

    #[cfg(any(
        all(unix, not(target_os = "macos"), not(target_os = "android")),
        target_os = "windows"
    ))]
    fn cascade_discover(
        &self,
        ch: char,
        fragment_style: &SpanStyle,
    ) -> Option<(usize, bool)> {
        let primary_family = self.primary_family_name()?;
        let want_bold = fragment_style.font_attrs.weight() == swash::Weight::BOLD;
        let want_italic = fragment_style.font_attrs.style() == swash::Style::Italic;
        // Terminal — always bias toward monospace for consistent cell
        // widths, even for fallback glyphs.
        let want_mono = true;

        #[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
        let discovered = crate::font::linux::discover_fallback(
            &primary_family,
            ch,
            want_mono,
            want_bold,
            want_italic,
        )?;

        #[cfg(target_os = "windows")]
        let discovered = crate::font::windows::discover_fallback(
            &primary_family,
            ch,
            want_mono,
            want_bold,
            want_italic,
        )?;

        let (path, face_index) = discovered;
        let font_data = FontData::from_discovered_path(path, face_index).ok()?;
        let ps_name = font_data.postscript_name()?.to_string();

        if let Some(found) = self.dedupe_existing(&ps_name) {
            return Some(found);
        }

        let mut lib = self.inner.write();
        if let Some(existing) = lib.font_id_for_postscript_name(&ps_name) {
            let is_emoji = lib
                .inner
                .get(&existing)
                .map(|fd| fd.is_emoji)
                .unwrap_or(false);
            return Some((existing, is_emoji));
        }
        let is_emoji = font_data.is_emoji;
        let new_id = lib.inner.len();
        lib.insert(font_data);
        tracing::debug!(
            "system cascade discovered {} for U+{:04X}, registered as font_id {}",
            ps_name,
            ch as u32,
            new_id
        );
        Some((new_id, is_emoji))
    }

    /// Read-lock-only check whether a font with this PostScript name
    /// is already registered (e.g. by a concurrent cascade resolver).
    /// Lets the slow path skip the upgrade-to-write-lock when there's
    /// nothing to register.
    fn dedupe_existing(&self, ps_name: &str) -> Option<(usize, bool)> {
        let lib = self.inner.read();
        let existing = lib.font_id_for_postscript_name(ps_name)?;
        let is_emoji = lib
            .inner
            .get(&existing)
            .map(|fd| fd.is_emoji)
            .unwrap_or(false);
        Some((existing, is_emoji))
    }

    /// Family name of the primary font (`FONT_ID_REGULAR`) used as a
    /// hint to the platform cascade resolver. fontconfig prefers
    /// fonts matching this family when ranking codepoint-coverage
    /// candidates. Falls back to `"monospace"` when the primary
    /// hasn't been loaded yet — fontconfig's generic family alias
    /// covers the usual case.
    #[cfg(any(
        all(unix, not(target_os = "macos"), not(target_os = "android")),
        target_os = "windows"
    ))]
    fn primary_family_name(&self) -> Option<String> {
        let lib = self.inner.read();
        let primary = lib.inner.get(&FONT_ID_REGULAR)?;
        primary
            .postscript_name()
            .map(|s| s.to_string())
            .or_else(|| Some(String::from("monospace")))
    }

    /// Sorted, deduplicated list of every font family name the host
    /// system exposes. On macOS this goes straight through CoreText; on
    /// Linux and Windows it uses `font-kit`'s `SystemSource` (fontconfig
    /// on Linux, DirectWrite on Windows). `wasm32` has no system font
    /// enumeration.
    ///
    /// Intended for the command-palette "List Fonts" browser, so users
    /// can see what's installed without leaving the terminal. Does NOT
    /// currently include fonts registered through rio's
    /// `fonts.additional_dirs` config — those aren't retained on the
    /// `FontLibrary` past load, and walking the dirs again would
    /// duplicate I/O. A follow-up can widen this once `FontLibrary`
    /// keeps a `Database` alive.
    #[cfg(target_os = "macos")]
    pub fn family_names(&self) -> Vec<String> {
        crate::font::macos::all_families()
    }

    #[cfg(all(not(target_os = "macos"), not(target_arch = "wasm32")))]
    pub fn family_names(&self) -> Vec<String> {
        let source = font_kit::source::SystemSource::new();
        let mut families = source.all_families().unwrap_or_default();
        families.sort_unstable();
        families.dedup();
        families
    }

    #[cfg(target_arch = "wasm32")]
    pub fn family_names(&self) -> Vec<String> {
        Vec::new()
    }
}

impl Default for FontLibrary {
    fn default() -> Self {
        let mut font_library = FontLibraryData::default();
        let _fonts_not_found = font_library.load(SugarloafFonts::default());

        Self {
            inner: Arc::new(RwLock::new(font_library)),
        }
    }
}

pub struct SymbolMap {
    pub font_index: usize,
    pub range: Range<char>,
}

pub struct FontLibraryData {
    // Standard is fallback for everything, it is also the inner number 0
    pub inner: FxHashMap<usize, FontData>,
    pub symbol_maps: Option<Vec<SymbolMap>>,
    pub hinting: bool,
    // Cache primary font metrics for consistent cell dimensions (consistent metrics approach)
    primary_metrics_cache: FxHashMap<u32, Metrics>,
    /// PostScript-name → `font_id` lookup, populated on `insert`. Used
    /// by the cascade resolver on every platform: macOS maps CoreText's
    /// per-CTRun font back to a Rio `font_id` when the cascade-list
    /// substitution kicks in; Linux and Windows use it to dedupe a
    /// fontconfig-/font-kit-discovered fallback against fonts already
    /// in the registry.
    postscript_to_id: FxHashMap<String, usize>,
}

impl Default for FontLibraryData {
    fn default() -> Self {
        Self {
            inner: FxHashMap::default(),
            hinting: true,
            symbol_maps: None,
            primary_metrics_cache: FxHashMap::default(),
            postscript_to_id: FxHashMap::default(),
        }
    }
}

impl FontLibraryData {
    #[inline]
    pub fn find_best_font_match(
        &self,
        ch: char,
        fragment_style: &SpanStyle,
    ) -> Option<(usize, bool)> {
        let mut synth = Synthesis::default();
        let mut char_cluster = CharCluster::new();
        let mut parser = Parser::new(
            Script::Latin,
            std::iter::once(Token {
                ch,
                offset: 0,
                len: ch.len_utf8() as u8,
                info: ch.properties().into(),
                data: 0,
            }),
        );
        if !parser.next(&mut char_cluster) {
            return Some((0, false));
        }

        // First check symbol map before lookup_for_font_match
        if let Some(symbol_maps) = &self.symbol_maps {
            for symbol_map in symbol_maps {
                if symbol_map.range.contains(&ch) {
                    return Some((symbol_map.font_index, false));
                }
            }
        }

        let italic = fragment_style.font_attrs.style() == Style::Italic;
        let bold = fragment_style.font_attrs.weight() == Weight::BOLD;
        let spec = (italic || bold).then_some(LookupAttrs { italic, bold });

        if let Some(result) =
            lookup_for_font_match(&mut char_cluster, &mut synth, self, spec)
        {
            return Some(result);
        }

        Some((0, false))
    }

    /// Like [`find_best_font_match`](Self::find_best_font_match) but
    /// returns `None` on a true miss instead of the `(0, false)`
    /// last-resort fallback. Enables callers (the lazy-discovery path
    /// on [`FontLibrary`], on every platform) to distinguish "primary
    /// font is the answer" from "nothing in the library covers this
    /// codepoint" so discovery can fire on the latter.
    #[inline]
    pub fn find_best_font_match_strict(
        &self,
        ch: char,
        fragment_style: &SpanStyle,
    ) -> Option<(usize, bool)> {
        let mut synth = Synthesis::default();
        let mut char_cluster = CharCluster::new();
        let mut parser = Parser::new(
            Script::Latin,
            std::iter::once(Token {
                ch,
                offset: 0,
                len: ch.len_utf8() as u8,
                info: ch.properties().into(),
                data: 0,
            }),
        );
        if !parser.next(&mut char_cluster) {
            return None;
        }

        if let Some(symbol_maps) = &self.symbol_maps {
            for symbol_map in symbol_maps {
                if symbol_map.range.contains(&ch) {
                    return Some((symbol_map.font_index, false));
                }
            }
        }

        let italic = fragment_style.font_attrs.style() == Style::Italic;
        let bold = fragment_style.font_attrs.weight() == Weight::BOLD;
        let spec = (italic || bold).then_some(LookupAttrs { italic, bold });

        lookup_for_font_match(&mut char_cluster, &mut synth, self, spec)
    }

    #[inline]
    pub fn insert(&mut self, font_data: FontData) {
        let id = self.inner.len();
        // Index by PS name so the cascade resolver (CoreText on macOS,
        // fontconfig on Linux, font-kit walk on Windows) can map a
        // discovered font back to a Rio `font_id`. Only paid at load
        // time. Duplicate names (same face loaded twice) resolve to
        // the first-inserted id, which is the entry the rest of the
        // library already points at — good enough for cascade mapping.
        if let Some(ps_name) = font_data.postscript_name() {
            self.postscript_to_id
                .entry(ps_name.to_string())
                .or_insert(id);
        }
        self.inner.insert(id, font_data);
    }

    /// Rio `font_id` registered for the given PostScript name, or
    /// `None` when no loaded font reports that name. Used by the
    /// cascade resolver to dedupe a discovered font against ones
    /// already in the registry.
    pub fn font_id_for_postscript_name(&self, name: &str) -> Option<usize> {
        self.postscript_to_id.get(name).copied()
    }

    #[inline]
    pub fn get(&self, font_id: &usize) -> &FontData {
        &self.inner[font_id]
    }

    pub fn get_data(&self, font_id: &usize) -> Option<(SharedData, u32, CacheKey)> {
        if let Some(font) = self.inner.get(font_id) {
            if let Some(data) = &font.data {
                return Some((data.clone(), font.offset, font.key));
            } else if let Some(path) = &font.path {
                // Load font data from cache or disk
                if let Some(raw_data) = load_from_font_source(path) {
                    return Some((raw_data, font.offset, font.key));
                }
            }
        }

        None
    }

    #[inline]
    pub fn get_mut(&mut self, font_id: &usize) -> Option<&mut FontData> {
        self.inner.get_mut(font_id)
    }

    /// Get font metrics for rich text rendering (consistent metrics approach)
    ///
    /// Primary font determines cell dimensions for all fonts to ensure consistent
    /// baseline alignment across different scripts (Latin, CJK, emoji, etc.).
    ///
    /// # Arguments
    /// * `font_id` - The font to get metrics for
    /// * `font_size` - The font size in pixels
    ///
    /// # Returns
    /// A tuple of (width, height, line_height) for the font, or None if the font
    /// cannot be found or metrics cannot be calculated.
    ///
    /// # Implementation Notes
    /// - Primary font metrics are cached for performance
    /// - Secondary fonts inherit cell dimensions from primary font
    /// - This ensures CJK characters don't appear "higher" than Latin text
    pub fn get_font_metrics(
        &mut self,
        font_id: &usize,
        font_size: f32,
    ) -> Option<(f32, f32, f32)> {
        let size_key = (font_size * 100.0) as u32;

        // First, ensure we have primary font metrics
        let primary_metrics =
            if let Some(cached) = self.primary_metrics_cache.get(&size_key) {
                *cached
            } else {
                let primary_font = self.inner.get_mut(&FONT_ID_REGULAR)?;
                let primary_metrics = primary_font.get_metrics(font_size, None)?;
                self.primary_metrics_cache.insert(size_key, primary_metrics);
                primary_metrics
            };

        match font_id {
            &FONT_ID_REGULAR => {
                // Primary font uses its own metrics
                Some(primary_metrics.for_rich_text())
            }
            _ => {
                // Secondary fonts use primary font's cell dimensions
                let font = self.inner.get_mut(font_id)?;
                font.get_rich_text_metrics(font_size, Some(&primary_metrics))
            }
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn load(&mut self, mut spec: SugarloafFonts) -> Vec<SugarloafFont> {
        // Configure hinting through spec
        self.hinting = spec.hinting;

        let mut fonts_not_fount: Vec<SugarloafFont> = vec![];

        // If fonts.family does exist it will overwrite all families
        if let Some(font_family_overwrite) = spec.family {
            font_family_overwrite.clone_into(&mut spec.regular.family);
            font_family_overwrite.clone_into(&mut spec.bold.family);
            font_family_overwrite.clone_into(&mut spec.bold_italic.family);
            font_family_overwrite.clone_into(&mut spec.italic.family);
        }

        // On macOS we resolve fonts through CoreText (see `find_font` below)
        // and never touch `loader::Database`, so skip its construction entirely
        // — `SystemSource::new` walks the full CoreText font list on init, which
        // is wasted work when we're about to do the same thing ourselves.
        #[cfg(not(target_os = "macos"))]
        let mut db = loader::Database::new();

        let additional_dirs = spec.additional_dirs.unwrap_or_default();
        for dir in additional_dirs.into_iter().map(PathBuf::from) {
            #[cfg(target_os = "macos")]
            crate::font::macos::register_fonts_in_dir(&dir);
            #[cfg(not(target_os = "macos"))]
            db.load_fonts_dir(dir);
        }

        #[cfg(target_os = "macos")]
        let resolve = |spec: SugarloafFont, slot: Slot, evictable: bool| {
            find_font(spec, slot, evictable)
        };
        #[cfg(not(target_os = "macos"))]
        let resolve = |spec: SugarloafFont, slot: Slot, evictable: bool| {
            find_font(&db, spec, slot, evictable)
        };

        let regular_index = self.len();
        match resolve(spec.regular, Slot::Regular, false) {
            FindResult::Found(data) => {
                self.insert(data);
            }
            FindResult::NotFound(spec) => {
                if !spec.is_default_family() {
                    fonts_not_fount.push(spec.to_owned());
                }

                self.insert(load_fallback_from_memory(Slot::Regular));
            }
        }

        for (slot, slot_spec, evictable) in [
            (Slot::Italic, spec.italic, false),
            (Slot::Bold, spec.bold, false),
            (Slot::BoldItalic, spec.bold_italic, true),
        ] {
            if slot_spec.style.is_disabled() {
                let reg = self.inner.get(&regular_index).cloned();
                match reg {
                    Some(data) => self.insert(data),
                    None => self.insert(load_fallback_from_memory(Slot::Regular)),
                }
                continue;
            }

            match resolve(slot_spec, slot, evictable) {
                FindResult::Found(data) => {
                    self.insert(data);
                }
                FindResult::NotFound(spec) => {
                    if !spec.is_default_family() {
                        fonts_not_fount.push(spec);
                    } else {
                        self.insert(load_fallback_from_memory(slot));
                    }
                }
            }
        }

        // On macOS, append CoreText's default cascade list for the primary
        // font. Dynamic fallback: we let CoreText name every font it would
        // normally fall back to (emoji, CJK, symbols, script typefaces) so
        // users get the same coverage as any other macOS app.
        //
        // Critically, each cascade entry is constructed via `from_path_macos`
        // — CoreText opens the file on demand, Rio never reads the bytes.
        // This keeps us from pulling the 200 MB Apple Color Emoji file into
        // `FONT_DATA_CACHE`.
        #[cfg(target_os = "macos")]
        {
            let primary_handle = self.inner.get(&FONT_ID_REGULAR).and_then(|f| {
                if let Some(path) = &f.path {
                    crate::font::macos::FontHandle::from_path(path)
                } else if let Some(bytes) = &f.data {
                    crate::font::macos::FontHandle::from_bytes(bytes.as_ref())
                } else {
                    None
                }
            });
            if let Some(primary_handle) = primary_handle {
                let default_spec = SugarloafFont::default();
                for path in crate::font::macos::default_cascade_list(&primary_handle) {
                    if let Ok(font_data) =
                        FontData::from_path_macos(path, Slot::Regular, &default_spec)
                    {
                        self.insert(font_data);
                    }
                }
            }
        }

        // TODO: Currently, it will naively just extend fonts from symbol_map
        // without even look if the font has been loaded before.
        // Considering that we drop the font data that's inactive should be ok but
        // it will cost a bit more time to initialize.
        //
        // Considering we receive via config
        // [{ start = "2297", end = "2299", font-family = "Cascadia Code NF" },
        //  { start = "2296", end = "2297", font-family = "Cascadia Code NF" }]
        //
        // Will become:
        // [{ start = "2297", end = "2299", font_index = Some(1) },
        //  { start = "2296", end = "2297", font_index = Some(1) }]
        //
        // TODO: We should have a new symbol map internally
        // { range = '2296'..'2297', font_index = Some(1) }]
        if let Some(symbol_map) = spec.symbol_map {
            let mut symbol_maps = Vec::default();
            for extra_font_from_symbol_map in symbol_map {
                match resolve(
                    SugarloafFont {
                        family: extra_font_from_symbol_map.font_family,
                        ..SugarloafFont::default()
                    },
                    Slot::Regular,
                    true,
                ) {
                    FindResult::Found(data) => {
                        if let Some(start) =
                            parse_unicode(&extra_font_from_symbol_map.start)
                        {
                            if let Some(end) =
                                parse_unicode(&extra_font_from_symbol_map.end)
                            {
                                self.insert(data);

                                symbol_maps.push(SymbolMap {
                                    range: start..end,
                                    font_index: self.len() - 1,
                                });

                                continue;
                            }
                        }

                        warn!("symbol-map: Failed to parse start and end values");
                    }
                    FindResult::NotFound(spec) => {
                        fonts_not_fount.push(spec);
                    }
                }
            }

            self.symbol_maps = Some(symbol_maps);
        }

        if spec.disable_warnings_not_found {
            vec![]
        } else {
            fonts_not_fount
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn load(&mut self, _font_spec: SugarloafFonts) -> Vec<SugarloafFont> {
        self.insert(FontData::from_slice(FONT_CASCADIAMONO_NF_REGULAR).unwrap());

        vec![]
    }
}

/// Font byte storage. Three variants so each load path pays the smallest
/// cost it can:
///
/// - [`Heap`](Self::Heap): Arc-shared `[u8]` on the heap. Fallback path
///   for bytes we genuinely own (tests, `from_slice`).
/// - [`Static`](Self::Static): a reference into `'static` data. Bundled
///   fonts use this so their bytes stay in the binary's `.rodata` instead
///   of being copied.
/// - [`Mmap`](Self::Mmap): memory-mapped file. Non-mac file reads use this
///   so the kernel backs the bytes with the font file and only pages in
///   what's actually touched. A 100 MB emoji font costs maybe 1 MB of
///   resident RAM instead of 100.
///
/// Clone is atomic-refcount on [`Heap`]/[`Mmap`] and a pointer copy on
/// [`Static`]; all three are effectively free.
#[derive(Clone, Debug)]
pub enum SharedData {
    Heap(Arc<[u8]>),
    Static(&'static [u8]),
    #[cfg(not(target_arch = "wasm32"))]
    Mmap(Arc<memmap2::Mmap>),
}

impl SharedData {
    /// Wrap an owned byte buffer. Used for ad-hoc / test loads; production
    /// font paths prefer [`from_static`](Self::from_static) or
    /// [`from_mmap`](Self::from_mmap).
    pub fn new(data: Vec<u8>) -> Self {
        Self::Heap(Arc::from(data))
    }

    /// Reference `'static` bytes. Zero-copy — bytes stay wherever they are
    /// (typically the binary's `.rodata` for bundled fonts).
    pub const fn from_static(data: &'static [u8]) -> Self {
        Self::Static(data)
    }

    /// Wrap a memory-mapped file. The `Arc<Mmap>` keeps the mapping alive
    /// until every `SharedData` referencing it drops.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_mmap(mmap: memmap2::Mmap) -> Self {
        Self::Mmap(Arc::new(mmap))
    }

    /// `true` when this `SharedData` references the binary's `.rodata`.
    /// Callers (the CoreText path) use this to pick a no-copy
    /// `CFDataCreateWithBytesNoCopy` when true.
    pub const fn is_static(&self) -> bool {
        matches!(self, Self::Static(_))
    }
}

impl std::ops::Deref for SharedData {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Heap(a) => a,
            Self::Static(s) => s,
            #[cfg(not(target_arch = "wasm32"))]
            Self::Mmap(m) => m.as_ref(),
        }
    }
}

impl AsRef<[u8]> for SharedData {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Heap(a) => a,
            Self::Static(s) => s,
            #[cfg(not(target_arch = "wasm32"))]
            Self::Mmap(m) => m.as_ref(),
        }
    }
}

#[derive(Clone)]
pub struct FontData {
    // Full content of the font file
    data: Option<SharedData>,
    path: Option<PathBuf>,
    // Offset to the table directory
    offset: u32,
    // Cache key
    pub key: CacheKey,
    pub weight: swash::Weight,
    pub style: swash::Style,
    pub stretch: swash::Stretch,
    pub synth: Synthesis,
    pub should_embolden: bool,
    pub should_italicize: bool,
    pub is_emoji: bool,
    // Cached metrics per font size (per-font caching)
    metrics_cache: FxHashMap<u32, Metrics>,
    /// Parsed CoreText handle, constructed once at `FontData` creation
    /// and cloned out via CF refcount on every access. Per-font pointer
    /// rather than a library-global cache. `Clone` of `FontHandle` is an
    /// atomic retain, so handing it out to the shape/raster/charmap paths
    /// is effectively free.
    #[cfg(target_os = "macos")]
    handle: Option<crate::font::macos::FontHandle>,
    /// PostScript name extracted at construction so the cross-platform
    /// `font_id_for_postscript_name` lookup can avoid reparsing the
    /// font file. Used by both the macOS CoreText cascade resolver and
    /// the Linux/Windows fontconfig/font-kit cascade resolver to map
    /// a discovered font back to a Rio `font_id`. `None` only for
    /// fonts where the PS name couldn't be parsed (rare — corrupt
    /// font, or zero-name TTF).
    postscript_name: Option<String>,
}

impl PartialEq for FontData {
    fn eq(&self, other: &Self) -> bool {
        // self.data == other.data &&
        self.key == other.key
        // self.offset == other.offset && self.key == other.key
    }
}

impl FontData {
    #[inline]
    pub fn is_bold(&self) -> bool {
        self.weight >= Weight(700)
    }

    #[inline]
    pub fn is_italic(&self) -> bool {
        self.style == Style::Italic
    }

    /// Get font data reference
    pub fn data(&self) -> &Option<SharedData> {
        &self.data
    }

    /// On-disk path the font was loaded from, if any. Embedded fonts
    /// (bundled `&[u8]` constants) have no path.
    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    /// The parsed CoreText handle, or `None` if this font was constructed
    /// via a path that doesn't run on macOS. Access is a direct field read
    /// (no map lookup); callers clone the handle (cheap CF retain) to
    /// escape the lock scope.
    #[cfg(target_os = "macos")]
    pub fn handle(&self) -> Option<&crate::font::macos::FontHandle> {
        self.handle.as_ref()
    }

    /// PostScript name extracted at load time (`name` table ID 6).
    /// `None` only for fonts whose name table couldn't be parsed.
    /// Used to map a discovered font path back to a Rio `font_id` in
    /// the cross-platform cascade resolver.
    pub fn postscript_name(&self) -> Option<&str> {
        self.postscript_name.as_deref()
    }

    /// Get font offset
    pub fn offset(&self) -> u32 {
        self.offset
    }

    /// Get or calculate metrics for a given font size (consistent metrics approach)
    /// For primary font: calculate natural metrics with CJK measurement
    /// For secondary fonts: use primary font's cell dimensions with CJK measurement
    pub fn get_metrics(
        &mut self,
        font_size: f32,
        primary_metrics: Option<&Metrics>,
    ) -> Option<Metrics> {
        let size_key = (font_size * 100.0) as u32; // Use scaled int as key

        if let Some(cached) = self.metrics_cache.get(&size_key) {
            return Some(*cached);
        }

        // macOS path-only (or handle-only) fonts: metrics come straight
        // from CoreText. This fires for every cascade-list fallback, any
        // user font discovered through `find_font_path`, AND any font
        // registered at runtime via `from_ctfont_macos` (lazy cascade
        // discovery) — none of which have `data` set. Prefer the stored
        // CTFont handle when present (cheap CF retain); otherwise
        // rebuild it from the path.
        #[cfg(target_os = "macos")]
        if self.data.is_none() {
            let handle = if let Some(h) = self.handle.as_ref() {
                h.clone()
            } else {
                self.path
                    .as_ref()
                    .and_then(|p| crate::font::macos::FontHandle::from_path(p))?
            };
            let font_metrics = crate::font::macos::design_unit_metrics(&handle);
            let scaled_metrics = font_metrics.scale(font_size);
            let face_metrics = FaceMetrics {
                cell_width: scaled_metrics.max_width as f64,
                ascent: scaled_metrics.ascent as f64,
                descent: scaled_metrics.descent as f64,
                line_gap: scaled_metrics.leading as f64,
                underline_position: Some(scaled_metrics.underline_offset as f64),
                underline_thickness: Some(scaled_metrics.stroke_size as f64),
                strikethrough_position: Some(scaled_metrics.strikeout_offset as f64),
                strikethrough_thickness: Some(scaled_metrics.stroke_size as f64),
                cap_height: Some(scaled_metrics.cap_height as f64),
                ex_height: Some(scaled_metrics.x_height as f64),
                ic_width: crate::font::macos::cjk_ic_width(&handle).map(|u| {
                    // design units → pixels at font_size
                    u * font_size as f64 / scaled_metrics.units_per_em as f64
                }),
            };
            let metrics = if let Some(primary) = primary_metrics {
                Metrics::calc_with_primary_cell_dimensions(face_metrics, primary)
            } else {
                Metrics::calc(face_metrics)
            };
            self.metrics_cache.insert(size_key, metrics);
            return Some(metrics);
        }

        // Calculate metrics if not cached
        if let Some(ref data) = self.data {
            let font_ref = swash::FontRef {
                data: data.as_ref(),
                offset: self.offset,
                key: self.key,
            };

            let scaled_metrics = font_ref.metrics(&[]).scale(font_size);

            // Use the unified method that always includes CJK measurement
            let face_metrics = FaceMetrics::from_font(&font_ref, &scaled_metrics);

            // Calculate metrics using consistent approach
            let metrics = if let Some(primary) = primary_metrics {
                // Secondary font: use primary font's cell dimensions
                Metrics::calc_with_primary_cell_dimensions(face_metrics, primary)
            } else {
                // Primary font: calculate natural metrics
                Metrics::calc(face_metrics)
            };

            // Cache the result
            self.metrics_cache.insert(size_key, metrics);
            Some(metrics)
        } else {
            None
        }
    }

    /// Get metrics for rich text rendering
    pub fn get_rich_text_metrics(
        &mut self,
        font_size: f32,
        primary_metrics: Option<&Metrics>,
    ) -> Option<(f32, f32, f32)> {
        self.get_metrics(font_size, primary_metrics)
            .map(|m| m.for_rich_text())
    }

    #[inline]
    pub fn from_data(
        data: SharedData,
        path: PathBuf,
        evictable: bool,
        slot: Slot,
        font_spec: &SugarloafFont,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let font = FontRef::from_index(&data, 0)
            .ok_or_else(|| format!("Failed to load font from path: {:?}", path))?;
        let (offset, key) = (font.offset, font.key);

        let attributes = font.attributes();
        let style = attributes.style();
        let weight = attributes.weight();

        let (should_embolden, should_italicize) = synth_decisions(
            slot,
            font_spec,
            weight >= Weight(700),
            style == Style::Italic,
        );

        let stretch = attributes.stretch();
        let synth = attributes.synthesize(attributes);
        let is_emoji = has_color_tables(&font);
        let postscript_name = parse_postscript_name(&data);

        let data = (!evictable).then_some(data);

        Ok(Self {
            data,
            offset,
            should_italicize,
            should_embolden,
            key,
            synth,
            style,
            weight,
            stretch,
            path: Some(path),
            is_emoji,
            metrics_cache: FxHashMap::default(),
            // `from_data` is the non-macOS code path — macOS goes through
            // `from_path_macos` or `from_static_slice`, both of which
            // populate `handle` themselves. Leave it unset here; if
            // anything on mac does route through here, the `ct_font()`
            // fallback rebuilds from bytes/path on demand.
            #[cfg(target_os = "macos")]
            handle: None,
            postscript_name,
        })
    }

    /// macOS-only: construct a `FontData` straight from a file path, with
    /// attributes read through CoreText. Never loads the font bytes.
    ///
    /// CoreText reads the file itself, so Rio's `FONT_DATA_CACHE` never
    /// ends up holding hundreds of MB of Apple Color Emoji / CJK font
    /// bytes.
    /// macOS-only: wrap a CTFont discovered at runtime (e.g. via
    /// `CTFontCreateForString` lazy cascade) into a `FontData` with no
    /// backing path or bytes. Metrics, rasterization and PS-name
    /// lookups all go through the handle directly — there's nothing
    /// for `get_data` / `get_metrics` to fall back to besides the
    /// stored CTFont.
    ///
    /// Weight/italic/stretch are left at defaults because lazy-cascade
    /// fonts are picked by CoreText based on script coverage rather
    /// than style matching; the primary font's style already dictated
    /// what was searched. Callers should not treat these fields as
    /// authoritative.
    #[cfg(target_os = "macos")]
    pub fn from_ctfont_macos(handle: crate::font::macos::FontHandle) -> Self {
        let attrs = crate::font::macos::font_attributes(&handle);
        let style = if attrs.is_italic {
            swash::Style::Italic
        } else {
            swash::Style::Normal
        };
        let weight = swash::Weight(attrs.weight);
        let postscript_name = Some(handle.postscript_name());
        Self {
            data: None,
            path: None,
            offset: 0,
            key: CacheKey::new(),
            weight,
            style,
            stretch: swash::Stretch::NORMAL,
            synth: Synthesis::default(),
            should_embolden: false,
            should_italicize: false,
            is_emoji: attrs.is_color,
            metrics_cache: FxHashMap::default(),
            handle: Some(handle),
            postscript_name,
        }
    }

    #[cfg(target_os = "macos")]
    pub fn from_path_macos(
        path: PathBuf,
        slot: Slot,
        font_spec: &SugarloafFont,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let handle = crate::font::macos::FontHandle::from_path(&path)
            .ok_or_else(|| format!("CoreText refused {}", path.display()))?;
        let attrs = crate::font::macos::font_attributes(&handle);

        let style = if attrs.is_italic {
            swash::Style::Italic
        } else {
            swash::Style::Normal
        };
        let weight = swash::Weight(attrs.weight);

        let (should_embolden, should_italicize) =
            synth_decisions(slot, font_spec, attrs.is_bold, attrs.is_italic);

        let postscript_name = Some(handle.postscript_name());
        Ok(Self {
            data: None,
            path: Some(path),
            offset: 0,
            key: CacheKey::new(),
            weight,
            style,
            stretch: swash::Stretch::NORMAL,
            synth: Synthesis::default(),
            should_embolden,
            should_italicize,
            is_emoji: attrs.is_color,
            metrics_cache: FxHashMap::default(),
            handle: Some(handle),
            postscript_name,
        })
    }

    /// Load a bundled font whose bytes live in `.rodata` (anything from
    /// `include_bytes!` / `font!`).
    ///
    /// The bytes stay where they already are — no `.to_vec()` copy onto
    /// the heap, no second copy into a CoreFoundation buffer. On macOS we
    /// also eagerly construct the CTFont via `CFDataCreateWithBytesNoCopy`
    /// + `kCFAllocatorNull` and cache it on `FontData.handle`.
    #[inline]
    pub fn from_static_slice(
        data: &'static [u8],
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let font = FontRef::from_index(data, 0).unwrap();
        let (offset, key) = (font.offset, font.key);
        let attributes = font.attributes();
        let style = attributes.style();
        let weight = attributes.weight();
        let stretch = attributes.stretch();
        let synth = attributes.synthesize(attributes);
        let is_emoji = has_color_tables(&font);
        let postscript_name = parse_postscript_name(data);

        #[cfg(target_os = "macos")]
        let handle = crate::font::macos::FontHandle::from_static_bytes(data);

        Ok(Self {
            data: Some(SharedData::from_static(data)),
            offset,
            key,
            synth,
            style,
            should_embolden: false,
            should_italicize: false,
            weight,
            stretch,
            path: None,
            is_emoji,
            metrics_cache: FxHashMap::default(),
            #[cfg(target_os = "macos")]
            handle,
            postscript_name,
        })
    }

    /// Legacy constructor kept for tests and any caller that only has a
    /// non-static slice — copies the bytes into an owned `Vec<u8>`.
    /// Production code should use [`from_static_slice`] for bundled fonts
    /// and [`from_data`] for path-loaded ones.
    #[inline]
    pub fn from_slice(data: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        let font = FontRef::from_index(data, 0).unwrap();
        let (offset, key) = (font.offset, font.key);
        let attributes = font.attributes();
        let style = attributes.style();
        let weight = attributes.weight();
        let stretch = attributes.stretch();
        let synth = attributes.synthesize(attributes);
        let is_emoji = has_color_tables(&font);

        let postscript_name = parse_postscript_name(data);
        Ok(Self {
            data: Some(SharedData::new(data.to_vec())),
            offset,
            key,
            synth,
            style,
            should_embolden: false,
            should_italicize: false,
            weight,
            stretch,
            path: None,
            is_emoji,
            metrics_cache: FxHashMap::default(),
            #[cfg(target_os = "macos")]
            handle: None,
            postscript_name,
        })
    }

    /// Build a `FontData` from a path discovered at runtime by the
    /// Linux/Windows cascade resolver. The font bytes are mmapped (cached
    /// in `FONT_DATA_CACHE`), parsed via swash to extract attributes,
    /// and `is_emoji` is auto-detected from color tables. Mirrors
    /// `from_path_macos` in shape but uses the cross-platform swash/
    /// ttf-parser stack instead of CoreText.
    ///
    /// `face_index` lets us address fonts inside a TTC/OTC collection
    /// (Noto Sans CJK ships as a single .ttc with separate faces for
    /// SC/TC/JP/KR — fontconfig returns the right index per language tag).
    #[cfg(any(
        all(unix, not(target_os = "macos"), not(target_os = "android")),
        target_os = "windows"
    ))]
    pub fn from_discovered_path(
        path: PathBuf,
        face_index: u32,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let data = load_from_font_source(&path).ok_or_else(|| {
            format!("failed to load discovered font: {}", path.display())
        })?;
        let font = FontRef::from_index(&data, face_index as usize).ok_or_else(|| {
            format!(
                "failed to parse discovered font {} face {}",
                path.display(),
                face_index
            )
        })?;
        let (offset, key) = (font.offset, font.key);
        let attributes = font.attributes();
        let style = attributes.style();
        let weight = attributes.weight();
        let stretch = attributes.stretch();
        let synth = attributes.synthesize(attributes);
        let is_emoji = has_color_tables(&font);
        let postscript_name = parse_postscript_name(&data);

        Ok(Self {
            data: Some(data),
            offset,
            should_italicize: false,
            should_embolden: false,
            key,
            synth,
            style,
            weight,
            stretch,
            path: Some(path),
            is_emoji,
            metrics_cache: FxHashMap::default(),
            postscript_name,
        })
    }
}

/// Extract the PostScript name (`name` table ID 6) from font bytes.
/// Used to populate `FontData.postscript_name` so the cross-platform
/// resolver can map a discovered font path back to a Rio `font_id`
/// without re-parsing. Falls back to the family name (ID 1) if the
/// PS name is missing — a font without a usable name can't participate
/// in the cascade-mapping anyway, so `None` is fine.
fn parse_postscript_name(data: &[u8]) -> Option<String> {
    let face = ttf_parser::Face::parse(data, 0).ok()?;
    face.names()
        .into_iter()
        .find(|n| n.name_id == ttf_parser::name_id::POST_SCRIPT_NAME && n.is_unicode())
        .and_then(|n| n.to_string())
        .or_else(|| {
            face.names()
                .into_iter()
                .find(|n| n.name_id == ttf_parser::name_id::FAMILY && n.is_unicode())
                .and_then(|n| n.to_string())
        })
}

/// Auto-detect emoji-ness from SFNT color tables (COLR, CBDT, CBLC, SBIX).
/// Used to guard against Nerd Font families being mis-flagged as emoji,
/// so a real emoji font gets the wide-cell/color-atlas treatment while
/// icon fonts stay single-cell.
fn has_color_tables(font: &FontRef<'_>) -> bool {
    font.table(tag_from_bytes(b"COLR")).is_some()
        || font.table(tag_from_bytes(b"CBDT")).is_some()
        || font.table(tag_from_bytes(b"CBLC")).is_some()
        || font.table(tag_from_bytes(b"sbix")).is_some()
}

pub type SugarloafFont = fonts::SugarloafFont;
pub type SugarloafFonts = fonts::SugarloafFonts;

#[cfg(not(target_arch = "wasm32"))]
use tracing::{info, warn};

enum FindResult {
    Found(FontData),
    NotFound(SugarloafFont),
}

/// Whether to apply faux-bold / faux-italic on top of the matched face.
/// Synth fires only when the slot's bold/italic intent isn't already
/// satisfied by the matched face, and never when the user pinned an
/// explicit `style = "..."` Named override (an exact face was asked for).
#[inline]
fn synth_decisions(
    slot: Slot,
    font_spec: &SugarloafFont,
    matched_is_bold: bool,
    matched_is_italic: bool,
) -> (bool, bool) {
    let allowed = !matches!(font_spec.style, FontStyle::Named(_));
    let embolden = allowed && slot.is_bold() && !matched_is_bold;
    let italicize = allowed && slot.is_italic() && !matched_is_italic;
    (embolden, italicize)
}

#[cfg(target_os = "macos")]
#[inline]
fn find_font(font_spec: SugarloafFont, slot: Slot, evictable: bool) -> FindResult {
    if font_spec.is_default_family() {
        return FindResult::NotFound(font_spec);
    }

    let family = font_spec.family.to_string();
    let style_name = font_spec.style.name();
    let bold = slot.is_bold();
    let italic = slot.is_italic();

    info!(
        "Font search (CoreText): family='{family}' bold={bold} italic={italic} style={:?}",
        style_name
    );

    let Some(path) =
        crate::font::macos::find_font_path(&family, bold, italic, style_name)
    else {
        warn!("CoreText found no match for family='{family}'");
        return FindResult::NotFound(font_spec);
    };

    // Path-based load: never reads bytes. `evictable` is ignored on the
    // macOS path since `FontData.data` is always `None` here — there's
    // nothing to evict.
    let _ = evictable;
    match FontData::from_path_macos(path.clone(), slot, &font_spec) {
        Ok(d) => {
            info!("Font '{family}' matched via CoreText at {}", path.display());
            FindResult::Found(d)
        }
        Err(e) => {
            warn!("Failed to open font '{family}' via CoreText: {e}");
            FindResult::NotFound(font_spec)
        }
    }
}

#[cfg(all(not(target_os = "macos"), not(target_arch = "wasm32")))]
#[inline]
fn find_font(
    db: &crate::font::loader::Database,
    font_spec: SugarloafFont,
    slot: Slot,
    evictable: bool,
) -> FindResult {
    if !font_spec.is_default_family() {
        let family = font_spec.family.to_string();
        let mut query = crate::font::loader::Query {
            families: &[crate::font::loader::Family::Name(&family)],
            ..crate::font::loader::Query::default()
        };

        query.weight = if slot.is_bold() {
            crate::font::loader::Weight::BOLD
        } else {
            crate::font::loader::Weight::NORMAL
        };

        query.style = if slot.is_italic() {
            crate::font::loader::Style::Italic
        } else {
            crate::font::loader::Style::Normal
        };

        info!(
            "Font search: '{query:?}' style_override={:?}",
            font_spec.style.name()
        );

        match db.query(&query) {
            Some(id) => {
                match db.face_source(id) {
                    Some((crate::font::loader::Source::File(ref path), _index)) => {
                        // File source - load from path
                        if let Some(font_data_arc) =
                            load_from_font_source(&path.to_path_buf())
                        {
                            match FontData::from_data(
                                font_data_arc,
                                path.to_path_buf(),
                                evictable,
                                &font_spec,
                            ) {
                                Ok(d) => {
                                    tracing::info!(
                                        "Font '{}' found in {}",
                                        family,
                                        path.display()
                                    );
                                    return FindResult::Found(d);
                                }
                                Err(err_message) => {
                                    tracing::info!(
                                        "Failed to load font '{query:?}', {err_message}"
                                    );
                                    return FindResult::NotFound(font_spec);
                                }
                            }
                        }
                    }
                    Some((crate::font::loader::Source::Binary(font_data), _index)) => {
                        // Binary source - use data directly
                        tracing::debug!(
                            "Using binary font data, {} bytes",
                            font_data.len()
                        );
                        // Convert Arc<Vec<u8>> to SharedData
                        match FontData::from_data(
                            font_data,
                            std::path::PathBuf::from(&family),
                            evictable,
                            &font_spec,
                        ) {
                            Ok(d) => {
                                tracing::info!("Font '{}' loaded from memory", family);
                                return FindResult::Found(d);
                            }
                            Err(err_message) => {
                                tracing::info!(
                                    "Failed to load font '{query:?}' from memory, {err_message}"
                                );
                                return FindResult::NotFound(font_spec);
                            }
                        }
                    }
                    None => {
                        tracing::warn!("face_source returned None for font ID");
                    }
                }
            }
            None => {
                warn!("Failed to find font '{query:?}'");
            }
        }
    }

    FindResult::NotFound(font_spec)
}

fn load_fallback_from_memory(slot: Slot) -> FontData {
    let font_to_load = match slot {
        Slot::Regular => constants::FONT_CASCADIAMONO_NF_REGULAR,
        Slot::Bold => constants::FONT_CASCADIAMONO_BOLD,
        Slot::Italic => constants::FONT_CASCADIAMONO_ITALIC,
        Slot::BoldItalic => constants::FONT_CASCADIAMONO_BOLD_ITALIC,
    };

    FontData::from_static_slice(font_to_load).unwrap()
}

#[allow(dead_code)]
fn find_font_path(
    db: &crate::font::loader::Database,
    font_family: String,
) -> Option<PathBuf> {
    info!("Font path search: family '{font_family}'");

    let query = crate::font::loader::Query {
        families: &[crate::font::loader::Family::Name(&font_family)],
        ..crate::font::loader::Query::default()
    };

    if let Some(id) = db.query(&query) {
        if let Some((crate::font::loader::Source::File(ref path), _index)) =
            db.face_source(id)
        {
            return Some(path.to_path_buf());
        }
    }

    None
}

#[cfg(not(target_arch = "wasm32"))]
fn load_from_font_source(path: &PathBuf) -> Option<SharedData> {
    let cache = get_font_data_cache();

    // Check if already cached - DashMap handles concurrent access efficiently
    if let Some(cached_data) = cache.get(path) {
        return Some(cached_data.clone());
    }

    // Memory-map the file rather than reading it into a `Vec<u8>`. The
    // kernel backs the bytes with the font file and only pages in what
    // swash's charmap / metrics queries actually touch, so a
    // large fallback (e.g. a CJK font, an emoji file) costs negligible
    // resident RAM instead of its full on-disk size. Mmap is unsafe
    // because the file can change underneath us or the mapping can fault;
    // for read-only font files this is the universally-accepted trade-off
    // (same as font-kit and FreeType).
    let file = std::fs::File::open(path).ok()?;
    let mmap = unsafe { memmap2::Mmap::map(&file).ok()? };
    let shared_data = SharedData::from_mmap(mmap);
    let entry = cache
        .entry(path.clone())
        .or_insert_with(|| shared_data.clone());
    Some(entry.clone())
}

#[cfg(all(test, target_os = "macos"))]
mod postscript_resolver_tests {
    use super::*;

    /// End-to-end: insert a bundled font into a bare `FontLibraryData`
    /// and verify the PostScript-name resolver returns the font_id we
    /// just assigned. This is the bridge the macOS shaper's cascade-run
    /// resolver walks over — if `insert` stops populating the map (e.g.
    /// `handle()` returns `None` on a refactor), the shape path
    /// silently falls back to primary instead of returning the right
    /// font for a substituted run.
    #[test]
    fn insert_populates_postscript_lookup() {
        // Read the PS name straight from the handle so the test doesn't
        // hardcode a value that changes if the bundled font is updated.
        let handle = crate::font::macos::FontHandle::from_static_bytes(
            FONT_CASCADIAMONO_NF_REGULAR,
        )
        .expect("parse CascadiaMono");
        let ps_name = handle.postscript_name();

        let mut lib = FontLibraryData::default();
        let font_data = FontData::from_static_slice(FONT_CASCADIAMONO_NF_REGULAR)
            .expect("load CascadiaMono");
        lib.insert(font_data);

        assert_eq!(
            lib.font_id_for_postscript_name(&ps_name),
            Some(0),
            "inserted PS name '{ps_name}' should resolve to font_id 0"
        );
        assert_eq!(
            lib.font_id_for_postscript_name("not-a-real-font"),
            None,
            "unknown PS names must return None, not a stale hit"
        );
    }

    /// `insert` keys on the handle's current PS name, so a second
    /// insert of the same face must not overwrite the first's id —
    /// otherwise later lookups would return a stale id pointing at a
    /// now-shifted slot. The rest of the library keys on the first
    /// id, so first-wins is the correct policy.
    #[test]
    fn duplicate_insert_keeps_first_id() {
        let handle = crate::font::macos::FontHandle::from_static_bytes(
            FONT_CASCADIAMONO_NF_REGULAR,
        )
        .expect("parse CascadiaMono");
        let ps_name = handle.postscript_name();

        let mut lib = FontLibraryData::default();
        lib.insert(
            FontData::from_static_slice(FONT_CASCADIAMONO_NF_REGULAR).expect("load a"),
        );
        lib.insert(
            FontData::from_static_slice(FONT_CASCADIAMONO_NF_REGULAR).expect("load b"),
        );
        assert_eq!(
            lib.font_id_for_postscript_name(&ps_name),
            Some(0),
            "second insert of same face must not clobber the first's font_id"
        );
    }

    /// Build a tiny `FontLibrary` that contains only CascadiaMono as
    /// font_id=0 — i.e. no cascade fallbacks registered. Then ask it to
    /// resolve a CJK codepoint CascadiaMono can't render. The lazy-
    /// discovery path should call `CTFontCreateForString`, register the
    /// discovered font under a new id, and return that id.
    #[test]
    fn resolve_font_for_char_lazy_discovers_cascade_font() {
        use crate::SpanStyle;
        use std::sync::Arc;

        let mut data = FontLibraryData::default();
        data.insert(
            FontData::from_static_slice(FONT_CASCADIAMONO_NF_REGULAR).expect("load"),
        );
        let lib = FontLibrary {
            inner: Arc::new(parking_lot::RwLock::new(data)),
        };
        let starting_len = lib.inner.read().inner.len();

        let style = SpanStyle::default();
        // U+6C34 ('水') — not in CascadiaMono. Library has no fallback
        // registered, so the pre-resolve walk returns None and the
        // discovery path has to fire.
        let (font_id, _is_emoji) = lib.resolve_font_for_char('\u{6C34}', &style);

        assert_ne!(
            font_id, 0,
            "lazy discovery should register a new font_id distinct from primary"
        );
        assert!(
            font_id < lib.inner.read().inner.len(),
            "returned font_id should index into the library"
        );
        assert_eq!(
            lib.inner.read().inner.len(),
            starting_len + 1,
            "lazy discovery should have registered exactly one new font"
        );
    }

    /// Two queries for codepoints that cascade to the same system font
    /// (both CJK ideographs) must reuse the same `font_id` — the
    /// postscript-name check under the write lock prevents double
    /// registration so each face is stored at most once.
    #[test]
    fn resolve_font_for_char_reuses_discovered_font() {
        use crate::SpanStyle;
        use std::sync::Arc;

        let mut data = FontLibraryData::default();
        data.insert(
            FontData::from_static_slice(FONT_CASCADIAMONO_NF_REGULAR).expect("load"),
        );
        let lib = FontLibrary {
            inner: Arc::new(parking_lot::RwLock::new(data)),
        };
        let style = SpanStyle::default();

        // Both codepoints should cascade to the same system CJK font on
        // any stock macOS install.
        let (id_a, _) = lib.resolve_font_for_char('\u{6C34}', &style);
        let len_after_first = lib.inner.read().inner.len();
        let (id_b, _) = lib.resolve_font_for_char('\u{6728}', &style);
        let len_after_second = lib.inner.read().inner.len();

        assert_eq!(
            id_a, id_b,
            "two CJK codepoints from the same cascade font should reuse the same font_id"
        );
        assert_eq!(
            len_after_first, len_after_second,
            "the second resolve must not register a duplicate font"
        );
    }
}
