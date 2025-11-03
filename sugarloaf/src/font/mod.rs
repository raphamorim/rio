pub mod constants;
mod fallbacks;
pub mod fonts;
#[cfg(not(target_arch = "wasm32"))]
pub mod loader;
pub mod metrics;
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
use crate::font_introspector::{CacheKey, FontRef, Synthesis};
use crate::layout::FragmentStyle;
use crate::SugarloafErrors;
use dashmap::DashMap;
use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

pub use crate::font_introspector::{Style, Weight};

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

        if let Some((shared_data, offset, key)) = library.get_data(&font_id) {
            let font_ref = FontRef {
                data: shared_data.as_ref(),
                offset,
                key,
            };
            let charmap = font_ref.charmap();
            let status = cluster.map(|ch| charmap.map(ch));
            if status != Status::Discard {
                *synth = font_synth;
                search_result = Some((font_id, is_emoji));
                break;
            }
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
        fragment_style: &FragmentStyle,
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

        let mut db = loader::Database::new();
        spec.additional_dirs
            .unwrap_or_default()
            .into_iter()
            .map(PathBuf::from)
            .for_each(|p| db.load_fonts_dir(p));

        match find_font(&db, spec.regular, false, false) {
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

        match find_font(&db, spec.italic, false, false) {
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

        match find_font(&db, spec.bold, false, false) {
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

        match find_font(&db, spec.bold_italic, true, false) {
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
            match find_font(
                &db,
                SugarloafFont {
                    family: fallback,
                    ..SugarloafFont::default()
                },
                true,
                false,
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

        if let Some(emoji_font) = spec.emoji {
            match find_font(&db, emoji_font, true, true) {
                FindResult::Found(data) => {
                    self.insert(data);
                }
                FindResult::NotFound(spec) => {
                    self.insert(FontData::from_slice(FONT_TWEMOJI_EMOJI, true).unwrap());
                    if !spec.is_default_family() {
                        fonts_not_fount.push(spec);
                    }
                }
            }
        } else {
            self.insert(FontData::from_slice(FONT_TWEMOJI_EMOJI, true).unwrap());
        }

        for extra_font in spec.extras {
            match find_font(
                &db,
                SugarloafFont {
                    family: extra_font.family,
                    style: extra_font.style,
                    weight: extra_font.weight,
                    width: extra_font.width,
                },
                true,
                true,
            ) {
                FindResult::Found(data) => {
                    self.insert(data);
                }
                FindResult::NotFound(spec) => {
                    fonts_not_fount.push(spec);
                }
            }
        }

        self.insert(FontData::from_slice(FONT_SYMBOLS_NERD_FONT_MONO, false).unwrap());

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
                match find_font(
                    &db,
                    SugarloafFont {
                        family: extra_font_from_symbol_map.font_family,
                        ..SugarloafFont::default()
                    },
                    true,
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
        self.insert(FontData::from_slice(FONT_CASCADIAMONO_REGULAR, false).unwrap());

        vec![]
    }
}

/// Atomically reference counted, heap allocated or memory mapped buffer.
#[derive(Clone, Debug)]
pub struct SharedData {
    inner: Arc<[u8]>,
}

impl SharedData {
    /// Creates shared data from the specified bytes.
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            inner: Arc::from(data),
        }
    }
}

impl std::ops::Deref for SharedData {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        (*self.inner).as_ref()
    }
}

impl AsRef<[u8]> for SharedData {
    fn as_ref(&self) -> &[u8] {
        (*self.inner).as_ref()
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
        is_emoji: bool,
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
        })
    }

    #[inline]
    pub fn from_slice(
        data: &[u8],
        is_emoji: bool,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let font = FontRef::from_index(data, 0).unwrap();
        let (offset, key) = (font.offset, font.key);
        // Return our struct with the original file data and copies of the
        // offset and key from the font reference
        let attributes = font.attributes();
        let style = attributes.style();
        let weight = attributes.weight();
        let stretch = attributes.stretch();
        let synth = attributes.synthesize(attributes);

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
        })
    }
}

pub type SugarloafFont = fonts::SugarloafFont;
pub type SugarloafFonts = fonts::SugarloafFonts;

#[cfg(not(target_arch = "wasm32"))]
use tracing::{info, warn};

enum FindResult {
    Found(FontData),
    NotFound(SugarloafFont),
}

#[cfg(not(target_arch = "wasm32"))]
#[inline]
fn find_font(
    db: &crate::font::loader::Database,
    font_spec: SugarloafFont,
    evictable: bool,
    is_emoji: bool,
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
                                is_emoji,
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
                            is_emoji,
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

    FontData::from_slice(font_to_load, false).unwrap()
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
    use std::io::Read;

    let cache = get_font_data_cache();

    // Check if already cached - DashMap handles concurrent access efficiently
    if let Some(cached_data) = cache.get(path) {
        return Some(cached_data.clone());
    }

    // Load from disk if not cached
    if let Ok(mut file) = std::fs::File::open(path) {
        let mut font_data = vec![];
        if file.read_to_end(&mut font_data).is_ok() {
            let shared_data = SharedData::new(font_data);
            // Use entry API to handle concurrent inserts properly
            let entry = cache
                .entry(path.clone())
                .or_insert_with(|| shared_data.clone());
            return Some(entry.clone());
        }
    }

    None
}
