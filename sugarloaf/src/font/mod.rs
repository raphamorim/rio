pub mod constants;
mod fallbacks;
pub mod fonts;
#[cfg(not(target_arch = "wasm32"))]
pub mod loader;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod metrics;
pub mod nerd_font_attributes;
pub mod text_run_cache;

#[cfg(test)]
mod cjk_metrics_tests;

pub const FONT_ID_REGULAR: usize = 0;

use crate::font::constants::*;
use crate::font::fonts::{parse_unicode, SugarloafFontStyle, SugarloafFontWidth};
use crate::font::metrics::{FaceMetrics, Metrics};
use crate::font_introspector::text::cluster::Parser;
use crate::font_introspector::text::cluster::Token;
use crate::font_introspector::text::cluster::{CharCluster, Status};
use crate::font_introspector::text::Codepoint;
use crate::font_introspector::text::Script;
use crate::font_introspector::{tag_from_bytes, CacheKey, FontRef, Synthesis};
use crate::layout::SpanStyle;
use crate::SugarloafErrors;
use dashmap::DashMap;
use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

pub use crate::font_introspector::{Style, Weight};

/// Cross-platform shim: non-macOS threads `&loader::Database` through to
/// `find_font`; macOS drops it since CoreText handles matching directly and
/// we never build a Database there. The macro lets call sites stay uniform
/// (`try_find_font!(&db, spec, evict)`) even though `db` doesn't exist on
/// macOS — macOS expansion simply discards that token.
#[cfg(target_os = "macos")]
macro_rules! try_find_font {
    ($_db:expr, $spec:expr, $evictable:expr) => {{
        find_font($spec, $evictable)
    }};
}

#[cfg(not(target_os = "macos"))]
macro_rules! try_find_font {
    ($db:expr, $spec:expr, $evictable:expr) => {{
        find_font($db, $spec, $evictable)
    }};
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

pub fn lookup_for_font_match(
    cluster: &mut CharCluster,
    synth: &mut Synthesis,
    library: &FontLibraryData,
    spec_font_attr_opt: Option<&(crate::font_introspector::Style, bool)>,
) -> Option<(usize, bool)> {
    let mut search_result = None;
    let mut font_synth = Synthesis::default();

    let fonts_len: usize = library.inner.len();
    for font_id in 0..fonts_len {
        let mut is_emoji = false;

        if let Some(font) = library.inner.get(&font_id) {
            is_emoji = font.is_emoji;
            font_synth = font.synth;

            // In this case, the font does match however
            // we need to check if is indeed a match
            if let Some(spec_font_attr) = spec_font_attr_opt {
                let style_is_different = font.style != spec_font_attr.0;
                let is_italic = spec_font_attr.0 == Style::Italic;
                if style_is_different && is_italic && !font.should_italicize {
                    continue;
                }

                // In case bold is required
                // It follows spec on Bold (>=700)
                // https://developer.mozilla.org/en-US/docs/Web/CSS/@font-face/font-weight
                let weight_is_different = spec_font_attr.1 && font.weight < Weight(700);
                if weight_is_different && !font.should_embolden {
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

    // In case no font_id is found and exists a font spec requirement
    // then drop requirement and try to find something that can match.
    if search_result.is_none() && spec_font_attr_opt.is_some() {
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
}

impl Default for FontLibraryData {
    fn default() -> Self {
        Self {
            inner: FxHashMap::default(),
            hinting: true,
            symbol_maps: None,
            primary_metrics_cache: FxHashMap::default(),
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

        let is_italic = fragment_style.font_attrs.style() == Style::Italic;
        let is_bold = fragment_style.font_attrs.weight() == Weight::BOLD;

        let spec_font_attr = if is_bold && is_italic {
            Some((Style::Italic, true))
        } else if is_bold {
            Some((Style::Normal, true))
        } else if is_italic {
            Some((Style::Italic, false))
        } else {
            None
        };

        if let Some(result) = lookup_for_font_match(
            &mut char_cluster,
            &mut synth,
            self,
            spec_font_attr.as_ref(),
        ) {
            return Some(result);
        }

        Some((0, false))
    }

    #[inline]
    pub fn insert(&mut self, font_data: FontData) {
        self.inner.insert(self.inner.len(), font_data);
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

        match try_find_font!(&db, spec.regular, false) {
            FindResult::Found(data) => {
                self.insert(data);
            }
            FindResult::NotFound(spec) => {
                if !spec.is_default_family() {
                    fonts_not_fount.push(spec.to_owned());
                }

                // The first font should always have a fallback
                self.insert(load_fallback_from_memory(&spec));
            }
        }

        match try_find_font!(&db, spec.italic, false) {
            FindResult::Found(data) => {
                self.insert(data);
            }
            FindResult::NotFound(spec) => {
                if !spec.is_default_family() {
                    fonts_not_fount.push(spec);
                } else {
                    self.insert(load_fallback_from_memory(&spec));
                }
            }
        }

        match try_find_font!(&db, spec.bold, false) {
            FindResult::Found(data) => {
                self.insert(data);
            }
            FindResult::NotFound(spec) => {
                if !spec.is_default_family() {
                    fonts_not_fount.push(spec);
                } else {
                    self.insert(load_fallback_from_memory(&spec));
                }
            }
        }

        match try_find_font!(&db, spec.bold_italic, true) {
            FindResult::Found(data) => {
                self.insert(data);
            }
            FindResult::NotFound(spec) => {
                if !spec.is_default_family() {
                    fonts_not_fount.push(spec);
                } else {
                    self.insert(load_fallback_from_memory(&spec));
                }
            }
        }

        for fallback in fallbacks::external_fallbacks() {
            match try_find_font!(
                &db,
                SugarloafFont {
                    family: fallback,
                    ..SugarloafFont::default()
                },
                true
            ) {
                FindResult::Found(data) => {
                    self.insert(data);
                }
                FindResult::NotFound(spec) => {
                    // Fallback should not add errors
                    tracing::info!("{:?}", spec);
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
                    if let Ok(font_data) = FontData::from_path_macos(path, &default_spec)
                    {
                        self.insert(font_data);
                    }
                }
            }
        }

        // User-configured fallbacks run before the bundled emoji / Nerd Font
        // slices so a color emoji family dropped into `extras` (e.g.
        // `extras = [{family = "Apple Color Emoji"}]`) takes priority over
        // the bundled Twemoji. Emoji-ness is auto-detected inside `FontData::
        // from_data` via `has_color_tables` (COLR/CBDT/CBLC/sbix), so real
        // emoji families get the wide-cell / color-atlas treatment while
        // Nerd Font families stay single-cell.
        //
        // On macOS the CoreText cascade list inserted above already includes
        // emoji, CJK, symbols, and every other system-suggested fallback —
        // `font.extras` is redundant there and would only duplicate or
        // compete with the cascade order. Skipped entirely.
        #[cfg(target_os = "macos")]
        let _ = spec.extras;
        #[cfg(not(target_os = "macos"))]
        for extra_font in spec.extras {
            match try_find_font!(
                &db,
                SugarloafFont {
                    family: extra_font.family,
                    style: extra_font.style,
                    weight: extra_font.weight,
                    width: extra_font.width,
                },
                true
            ) {
                FindResult::Found(data) => {
                    self.insert(data);
                }
                FindResult::NotFound(spec) => {
                    fonts_not_fount.push(spec);
                }
            }
        }

        // macOS finds Apple Color Emoji through `fallbacks::external_fallbacks`
        // above, so skip embedding Twemoji there.
        #[cfg(not(target_os = "macos"))]
        self.insert(FontData::from_static_slice(FONT_TWEMOJI_EMOJI).unwrap());
        self.insert(FontData::from_static_slice(FONT_SYMBOLS_NERD_FONT_MONO).unwrap());

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
                match try_find_font!(
                    &db,
                    SugarloafFont {
                        family: extra_font_from_symbol_map.font_family,
                        ..SugarloafFont::default()
                    },
                    true
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
        self.insert(FontData::from_static_slice(FONT_CASCADIAMONO_REGULAR).unwrap());

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
    pub weight: crate::font_introspector::Weight,
    pub style: crate::font_introspector::Style,
    pub stretch: crate::font_introspector::Stretch,
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
}

impl PartialEq for FontData {
    fn eq(&self, other: &Self) -> bool {
        // self.data == other.data &&
        self.key == other.key
        // self.offset == other.offset && self.key == other.key
    }
}

impl FontData {
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

        // macOS path-only fonts: metrics come straight from CoreText. This
        // is the branch that fires for every cascade-list fallback and any
        // user font discovered through `find_font_path` on mac — none of
        // which have `data` set. Requires a CTFont (from path or bytes) and
        // bypasses font_introspector entirely.
        #[cfg(target_os = "macos")]
        if self.data.is_none() {
            let handle = self
                .path
                .as_ref()
                .and_then(|p| crate::font::macos::FontHandle::from_path(p))?;
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
            let font_ref = crate::font_introspector::FontRef {
                data: data.as_ref(),
                offset: self.offset,
                key: self.key,
            };

            let font_metrics =
                crate::font_introspector::Metrics::from_font(&font_ref, &[]);
            let scaled_metrics = font_metrics.scale(font_size);

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
        font_spec: &SugarloafFont,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let font = FontRef::from_index(&data, 0)
            .ok_or_else(|| format!("Failed to load font from path: {:?}", path))?;
        let (offset, key) = (font.offset, font.key);

        // Return our struct with the original file data and copies of the
        // offset and key from the font reference
        let attributes = font.attributes();
        let style = attributes.style();
        let weight = attributes.weight();

        let should_italicize =
            font_spec.style == SugarloafFontStyle::Italic && style != Style::Italic;

        let should_embolden = font_spec.weight >= Some(700) && weight < Weight(700);

        let stretch = attributes.stretch();
        let synth = attributes.synthesize(attributes);
        let is_emoji = has_color_tables(&font);

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
        })
    }

    /// macOS-only: construct a `FontData` straight from a file path, with
    /// attributes read through CoreText. Never loads the font bytes.
    ///
    /// CoreText reads the file itself, so Rio's `FONT_DATA_CACHE` never
    /// ends up holding hundreds of MB of Apple Color Emoji / CJK font
    /// bytes.
    #[cfg(target_os = "macos")]
    pub fn from_path_macos(
        path: PathBuf,
        font_spec: &SugarloafFont,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let handle = crate::font::macos::FontHandle::from_path(&path)
            .ok_or_else(|| format!("CoreText refused {}", path.display()))?;
        let attrs = crate::font::macos::font_attributes(&handle);

        let style = if attrs.is_italic {
            crate::font_introspector::Style::Italic
        } else {
            crate::font_introspector::Style::Normal
        };
        let weight = crate::font_introspector::Weight(attrs.weight);

        let should_italicize =
            font_spec.style == SugarloafFontStyle::Italic && !attrs.is_italic;
        let should_embolden = font_spec.weight >= Some(700) && attrs.weight < 700;

        Ok(Self {
            data: None,
            path: Some(path),
            offset: 0,
            key: CacheKey::new(),
            weight,
            style,
            stretch: crate::font_introspector::Stretch::NORMAL,
            synth: Synthesis::default(),
            should_embolden,
            should_italicize,
            is_emoji: attrs.is_color,
            metrics_cache: FxHashMap::default(),
            handle: Some(handle),
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
        })
    }
}

/// Auto-detect emoji-ness from SFNT color tables (COLR, CBDT, CBLC, SBIX).
/// Used to guard
/// against Nerd Font families being mis-flagged as emoji when loaded via the
/// `extras` config slot (`is_emoji` is wired per-load-site, but a real emoji
/// font in that slot would still need the wide-cell/color-atlas treatment).
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

#[cfg(target_os = "macos")]
#[inline]
fn find_font(font_spec: SugarloafFont, evictable: bool) -> FindResult {
    if font_spec.is_default_family() {
        return FindResult::NotFound(font_spec);
    }

    let family = font_spec.family.to_string();
    let weight = font_spec.weight.unwrap_or(400);
    let italic = font_spec.style == SugarloafFontStyle::Italic;
    let stretch = map_stretch_macos(&font_spec.width);

    info!("Font search (CoreText): family='{family}' weight={weight} italic={italic}");

    let Some(path) = crate::font::macos::find_font_path(&family, weight, italic, stretch)
    else {
        warn!("CoreText found no match for family='{family}'");
        return FindResult::NotFound(font_spec);
    };

    // Path-based load: never reads bytes. `evictable` is ignored on the
    // macOS path since `FontData.data` is always `None` here — there's
    // nothing to evict.
    let _ = evictable;
    match FontData::from_path_macos(path.clone(), &font_spec) {
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

#[cfg(target_os = "macos")]
fn map_stretch_macos(width: &Option<SugarloafFontWidth>) -> crate::font::macos::Stretch {
    use crate::font::macos::Stretch;
    match width {
        Some(SugarloafFontWidth::UltraCondensed) => Stretch::UltraCondensed,
        Some(SugarloafFontWidth::ExtraCondensed) => Stretch::ExtraCondensed,
        Some(SugarloafFontWidth::Condensed) => Stretch::Condensed,
        Some(SugarloafFontWidth::SemiCondensed) => Stretch::SemiCondensed,
        Some(SugarloafFontWidth::Normal) | None => Stretch::Normal,
        Some(SugarloafFontWidth::SemiExpanded) => Stretch::SemiExpanded,
        Some(SugarloafFontWidth::Expanded) => Stretch::Expanded,
        Some(SugarloafFontWidth::ExtraExpanded) => Stretch::ExtraExpanded,
        Some(SugarloafFontWidth::UltraExpanded) => Stretch::UltraExpanded,
    }
}

#[cfg(all(not(target_os = "macos"), not(target_arch = "wasm32")))]
#[inline]
fn find_font(
    db: &crate::font::loader::Database,
    font_spec: SugarloafFont,
    evictable: bool,
) -> FindResult {
    if !font_spec.is_default_family() {
        let family = font_spec.family.to_string();
        let mut query = crate::font::loader::Query {
            families: &[crate::font::loader::Family::Name(&family)],
            ..crate::font::loader::Query::default()
        };

        if let Some(weight) = font_spec.weight {
            query.weight = crate::font::loader::Weight(weight);
        }

        if let Some(ref width) = font_spec.width {
            query.stretch = match width {
                SugarloafFontWidth::UltraCondensed => {
                    crate::font::loader::Stretch::UltraCondensed
                }
                SugarloafFontWidth::ExtraCondensed => {
                    crate::font::loader::Stretch::ExtraCondensed
                }
                SugarloafFontWidth::Condensed => crate::font::loader::Stretch::Condensed,
                SugarloafFontWidth::SemiCondensed => {
                    crate::font::loader::Stretch::SemiCondensed
                }
                SugarloafFontWidth::Normal => crate::font::loader::Stretch::Normal,
                SugarloafFontWidth::SemiExpanded => {
                    crate::font::loader::Stretch::SemiExpanded
                }
                SugarloafFontWidth::Expanded => crate::font::loader::Stretch::Expanded,
                SugarloafFontWidth::ExtraExpanded => {
                    crate::font::loader::Stretch::ExtraExpanded
                }
                SugarloafFontWidth::UltraExpanded => {
                    crate::font::loader::Stretch::UltraExpanded
                }
            };
        }

        query.style = match font_spec.style {
            SugarloafFontStyle::Italic => crate::font::loader::Style::Italic,
            _ => crate::font::loader::Style::Normal,
        };

        info!("Font search: '{query:?}'");

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

fn load_fallback_from_memory(font_spec: &SugarloafFont) -> FontData {
    let style = &font_spec.style;
    let weight = font_spec.weight.unwrap_or(400);

    let font_to_load = match (weight, style) {
        (100, SugarloafFontStyle::Italic) => {
            constants::FONT_CASCADIAMONO_EXTRA_LIGHT_ITALIC
        }
        (200, SugarloafFontStyle::Italic) => constants::FONT_CASCADIAMONO_LIGHT_ITALIC,
        (300, SugarloafFontStyle::Italic) => {
            constants::FONT_CASCADIAMONO_SEMI_LIGHT_ITALIC
        }
        (400, SugarloafFontStyle::Italic) => constants::FONT_CASCADIAMONO_ITALIC,
        (500, SugarloafFontStyle::Italic) => constants::FONT_CASCADIAMONO_ITALIC,
        (600, SugarloafFontStyle::Italic) => {
            constants::FONT_CASCADIAMONO_SEMI_BOLD_ITALIC
        }
        (700, SugarloafFontStyle::Italic) => {
            constants::FONT_CASCADIAMONO_SEMI_BOLD_ITALIC
        }
        (800, SugarloafFontStyle::Italic) => constants::FONT_CASCADIAMONO_BOLD_ITALIC,
        (900, SugarloafFontStyle::Italic) => constants::FONT_CASCADIAMONO_BOLD_ITALIC,
        (_, SugarloafFontStyle::Italic) => constants::FONT_CASCADIAMONO_ITALIC,
        (100, _) => constants::FONT_CASCADIAMONO_EXTRA_LIGHT,
        (200, _) => constants::FONT_CASCADIAMONO_LIGHT,
        (300, _) => constants::FONT_CASCADIAMONO_SEMI_LIGHT,
        (400, _) => constants::FONT_CASCADIAMONO_REGULAR,
        (500, _) => constants::FONT_CASCADIAMONO_REGULAR,
        (600, _) => constants::FONT_CASCADIAMONO_SEMI_BOLD,
        (700, _) => constants::FONT_CASCADIAMONO_SEMI_BOLD,
        (800, _) => constants::FONT_CASCADIAMONO_BOLD,
        (900, _) => constants::FONT_CASCADIAMONO_BOLD,
        (_, _) => constants::FONT_CASCADIAMONO_REGULAR,
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
    // font_introspector's charmap / metrics queries actually touch, so a
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
