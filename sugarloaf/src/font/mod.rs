pub mod constants;
mod fallbacks;
pub mod fonts;
#[cfg(not(target_arch = "wasm32"))]
pub mod loader;

pub const FONT_ID_REGULAR: usize = 0;

use parking_lot::FairMutex;
use crate::font::constants::*;
use crate::font_introspector::proxy::CharmapProxy;
use crate::font_introspector::text::cluster::{CharCluster, Status};
use crate::font_introspector::{Attributes, CacheKey, FontRef, Synthesis};
use crate::layout::FragmentStyle;
use crate::SugarloafErrors;
use ab_glyph::FontArc;
use rustc_hash::FxHashMap;
use std::ops::{Index, IndexMut};
use std::path::PathBuf;
use std::sync::Arc;

pub use crate::font_introspector::{Style, Weight};

#[derive(Default)]
pub struct FontContext {
    cache: FxHashMap<String, usize>,
}

pub fn lookup_for_font_match(
    cluster: &mut CharCluster,
    synth: &mut Synthesis,
    library: &FontLibraryData,
    spec_font_attr_opt: Option<&(crate::font_introspector::Style, bool)>,
) -> Option<usize> {
    let mut font_id = None;
    for (current_font_id, font) in library.inner.iter().enumerate() {
        let (font, font_ref) = match font {
            FontSource::Data(font_data) => (font_data, font_data.as_ref()),
            FontSource::Standard => (&library.standard, library.standard.as_ref()),
        };

        // In this case, the font does match however
        // we need to check if is indeed a match
        if let Some(spec_font_attr) = spec_font_attr_opt {
            if font.style != spec_font_attr.0 {
                continue;
            }

            // In case bold is required
            // It follows spec on Bold (>=700)
            // https://developer.mozilla.org/en-US/docs/Web/CSS/@font-face/font-weight
            if spec_font_attr.1 && font.weight < crate::font_introspector::Weight(700) {
                continue;
            }
        }

        let charmap = font.charmap_proxy.materialize(&font_ref);
        let status = cluster.map(|ch| charmap.map(ch));
        if status != Status::Discard {
            *synth = library[current_font_id].synth;
            font_id = Some(current_font_id);
            break;
        }
    }

    // In case no font_id is found and exists a font spec requirement
    // then drop requirement and try to find something that can match.
    if font_id.is_none() && spec_font_attr_opt.is_some() {
        return lookup_for_font_match(cluster, synth, library, None);
    }

    font_id
}

impl FontContext {
    #[inline]
    pub fn map_cluster(
        &mut self,
        cluster: &mut CharCluster,
        synth: &mut Synthesis,
        library: &FontLibraryData,
        fragment_style: &FragmentStyle,
    ) -> Option<usize> {
        let is_italic = fragment_style.font_attrs.style() == Style::Italic;
        let is_bold = fragment_style.font_attrs.weight() == Weight::BOLD;

        let chars = cluster.chars();
        let is_cache_key_empty = chars.is_empty();

        let mut cache_key: String = String::default();
        let spec_font_attr = if is_bold && is_italic {
            cache_key.push('m');
            Some((Style::Italic, true))
        } else if is_bold {
            cache_key.push('b');
            Some((Style::Normal, true))
        } else if is_italic {
            cache_key.push('i');
            Some((Style::Italic, false))
        } else {
            cache_key.push('r');
            None
        };

        for c in chars.iter() {
            cache_key.push(c.ch);
        }

        if !is_cache_key_empty {
            if let Some(cached_font_id) = self.cache.get(&cache_key) {
                let cached_font_id = *cached_font_id;
                let charmap = library[cached_font_id]
                    .charmap_proxy
                    .materialize(&library[cached_font_id].as_ref());
                let status = cluster.map(|ch| charmap.map(ch));
                if status != Status::Discard {
                    *synth = library[cached_font_id].synth;
                }

                return Some(cached_font_id);
            }
        }

        if let Some(found_font_id) =
            lookup_for_font_match(cluster, synth, library, spec_font_attr.as_ref())
        {
            if !is_cache_key_empty {
                self.cache.insert(cache_key, found_font_id);
            }

            return Some(found_font_id);
        }

        Some(0)
    }

    #[inline]
    pub fn find_font_by_character(
        &mut self,
        character: char,
        synth: &mut Synthesis,
        library: &FontLibraryData,
        fragment_style: &FragmentStyle,
    ) -> Option<usize> {
        let is_italic = fragment_style.font_attrs.style() == Style::Italic;
        let is_bold = fragment_style.font_attrs.weight() == Weight::BOLD;

        let mut cache_key: String = String::default();
        if is_bold && is_italic {
            cache_key.push('m');
        } else if is_bold {
            cache_key.push('b');
        } else if is_italic {
            cache_key.push('i');
        } else {
            cache_key.push('r');
        };

        cache_key.push(character);

        if let Some(cached_font_id) = self.cache.get(&cache_key) {
            *synth = library[*cached_font_id].synth;
            return Some(*cached_font_id);
        }

        None
    }
}

#[derive(Clone)]
pub struct FontLibrary {
    pub(super) inner: Arc<FairMutex<FontLibraryData>>,
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
                inner: Arc::new(FairMutex::new(font_library)),
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
            inner: Arc::new(FairMutex::new(font_library)),
        }
    }
}

pub enum FontSource {
    Standard,
    Data(FontData),
}

pub struct FontLibraryData {
    pub ui: FontArc,
    // Standard is fallback for everything, it is also the inner number 0
    pub standard: FontData,
    pub inner: Vec<FontSource>,
}

impl Default for FontLibraryData {
    fn default() -> Self {
        Self {
            ui: FontArc::try_from_slice(FONT_CASCADIAMONO_REGULAR).unwrap(),
            standard: FontData::from_slice(FONT_CASCADIAMONO_REGULAR).unwrap(),
            inner: vec![],
        }
    }
}

impl FontLibraryData {
    #[inline]
    pub fn insert(&mut self, font_data: FontData) {
        self.inner.push(FontSource::Data(font_data));
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[inline]
    pub fn upsert(&mut self, font_id: usize, path: PathBuf) {
        if let Some(font_data) = self.inner.get_mut(font_id) {
            if let Some(loaded_font_data) = load_from_font_source(&path) {
                *font_data = FontSource::Data(loaded_font_data);
            };
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn load(&mut self, mut spec: SugarloafFonts) -> Vec<SugarloafFont> {
        let mut fonts_not_fount: Vec<SugarloafFont> = vec![];

        // If fonts.family does exist it will overwrite all families
        if let Some(font_family_overwrite) = spec.family {
            font_family_overwrite.clone_into(&mut spec.regular.family);
            font_family_overwrite.clone_into(&mut spec.bold.family);
            font_family_overwrite.clone_into(&mut spec.bold_italic.family);
            font_family_overwrite.clone_into(&mut spec.italic.family);
        }

        let mut db = loader::Database::new();
        db.load_system_fonts();

        match find_font(&db, spec.regular) {
            FindResult::Found(data) => {
                self.standard = data;
                self.inner = vec![FontSource::Standard];
            }
            FindResult::NotFound(spec) => {
                self.standard = load_fallback_from_memory(&spec);
                self.inner = vec![FontSource::Standard];
                if !spec.is_default_family() {
                    fonts_not_fount.push(spec);
                }
            }
        }

        match find_font(&db, spec.italic) {
            FindResult::Found(data) => {
                self.inner.push(FontSource::Data(data));
            }
            FindResult::NotFound(spec) => {
                self.inner
                    .push(FontSource::Data(load_fallback_from_memory(&spec)));
                if !spec.is_default_family() {
                    fonts_not_fount.push(spec);
                }
            }
        }

        match find_font(&db, spec.bold) {
            FindResult::Found(data) => {
                self.inner.push(FontSource::Data(data));
            }
            FindResult::NotFound(spec) => {
                self.inner
                    .push(FontSource::Data(load_fallback_from_memory(&spec)));
                if !spec.is_default_family() {
                    fonts_not_fount.push(spec);
                }
            }
        }

        match find_font(&db, spec.bold_italic) {
            FindResult::Found(data) => {
                self.inner.push(FontSource::Data(data));
            }
            FindResult::NotFound(spec) => {
                self.inner
                    .push(FontSource::Data(load_fallback_from_memory(&spec)));
                if !spec.is_default_family() {
                    fonts_not_fount.push(spec);
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
            ) {
                FindResult::Found(data) => {
                    self.inner.push(FontSource::Data(data));
                }
                FindResult::NotFound(spec) => {
                    // Fallback should not add errors
                    tracing::info!("{:?}", spec);
                }
            }
        }

        if let Some(emoji_font) = spec.emoji {
            match find_font(&db, emoji_font) {
                FindResult::Found(data) => {
                    self.inner.push(FontSource::Data(data));
                }
                FindResult::NotFound(spec) => {
                    self.inner.push(FontSource::Data(
                        FontData::from_slice(FONT_TWEMOJI_EMOJI).unwrap(),
                    ));
                    if !spec.is_default_family() {
                        fonts_not_fount.push(spec);
                    }
                }
            }
        } else {
            self.inner.push(FontSource::Data(
                FontData::from_slice(FONT_TWEMOJI_EMOJI).unwrap(),
            ));
        }
        // Set last font as emoji
        if let Some(FontSource::Data(ref mut font_ref)) = self.inner.last_mut() {
            font_ref.is_emoji = true;
        }

        for extra_font in spec.extras {
            match find_font(
                &db,
                SugarloafFont {
                    family: extra_font.family,
                    style: extra_font.style,
                    weight: extra_font.weight,
                },
            ) {
                FindResult::Found(data) => {
                    self.inner.push(FontSource::Data(data));
                }
                FindResult::NotFound(spec) => {
                    fonts_not_fount.push(spec);
                }
            }
        }

        self.inner.push(FontSource::Data(
            FontData::from_slice(FONT_SYMBOLS_NERD_FONT_MONO).unwrap(),
        ));

        if let Some(ui_spec) = spec.ui {
            match find_font(&db, ui_spec) {
                FindResult::Found(data) => {
                    self.ui = FontArc::try_from_vec(data.data.to_vec()).unwrap();
                }
                FindResult::NotFound(spec) => {
                    fonts_not_fount.push(spec);
                }
            }
        }

        fonts_not_fount
    }

    #[cfg(target_arch = "wasm32")]
    pub fn load(&mut self, _font_spec: SugarloafFonts) -> Vec<SugarloafFont> {
        self.inner
            .insert(FontData::from_slice(FONT_CASCADIAMONO_REGULAR).unwrap());

        vec![]
    }
}

impl Index<usize> for FontLibraryData {
    type Output = FontData;

    fn index(&self, index: usize) -> &Self::Output {
        match &self.inner[index] {
            FontSource::Data(font_ref) => font_ref,
            FontSource::Standard => &self.standard,
        }
    }
}

impl IndexMut<usize> for FontLibraryData {
    fn index_mut(&mut self, index: usize) -> &mut FontData {
        match &mut self.inner[index] {
            FontSource::Data(font_ref) => font_ref,
            FontSource::Standard => &mut self.standard,
        }
    }
}

/// Atomically reference counted, heap allocated or memory mapped buffer.
#[derive(Clone)]
pub struct SharedData {
    inner: Arc<dyn AsRef<[u8]> + Send + Sync>,
}

impl SharedData {
    /// Creates shared data from the specified bytes.
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            inner: Arc::new(data),
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
    data: SharedData,
    // Offset to the table directory
    offset: u32,
    // Cache key
    pub key: CacheKey,
    pub charmap_proxy: CharmapProxy,
    pub weight: crate::font_introspector::Weight,
    pub style: crate::font_introspector::Style,
    pub stretch: crate::font_introspector::Stretch,
    pub synth: Synthesis,
    pub is_emoji: bool,
}

impl PartialEq for FontData {
    fn eq(&self, other: &Self) -> bool {
        // self.data == other.data &&
        self.key == other.key
        // self.offset == other.offset && self.key == other.key
    }
}

impl<'a> From<&'a FontData> for FontRef<'a> {
    fn from(f: &'a FontData) -> FontRef<'a> {
        f.as_ref()
    }
}

impl FontData {
    #[inline]
    pub fn from_data(data: Vec<u8>) -> Result<Self, Box<dyn std::error::Error>> {
        let font = FontRef::from_index(&data, 0).unwrap();
        let charmap_proxy = CharmapProxy::from_font(&font.clone());
        let (offset, key) = (font.offset, font.key);

        // Return our struct with the original file data and copies of the
        // offset and key from the font reference
        let attributes = font.attributes();
        let style = attributes.style();
        let weight = attributes.weight();
        let stretch = attributes.stretch();
        let synth = attributes.synthesize(attributes);

        Ok(Self {
            data: SharedData::new(data),
            offset,
            key,
            charmap_proxy,
            synth,
            style,
            weight,
            stretch,
            is_emoji: false,
        })
    }

    #[inline]
    pub fn from_slice(data: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        let font = FontRef::from_index(data, 0).unwrap();
        let charmap_proxy = CharmapProxy::from_font(&font);
        let (offset, key) = (font.offset, font.key);
        // Return our struct with the original file data and copies of the
        // offset and key from the font reference
        let attributes = font.attributes();
        let style = attributes.style();
        let weight = attributes.weight();
        let stretch = attributes.stretch();
        let synth = attributes.synthesize(attributes);

        Ok(Self {
            data: SharedData::new(data.to_vec()),
            offset,
            key,
            charmap_proxy,
            synth,
            style,
            weight,
            stretch,
            is_emoji: false,
        })
    }

    // As a convenience, you may want to forward some methods.
    #[inline]
    pub fn attributes(&self) -> Attributes {
        self.as_ref().attributes()
    }

    // Create the transient font reference for accessing this crate's
    // functionality.
    #[inline]
    pub fn as_ref(&self) -> FontRef {
        // Note that you'll want to initialize the struct directly here as
        // using any of the FontRef constructors will generate a new key which,
        // while completely safe, will nullify the performance optimizations of
        // the caching mechanisms used in this crate.
        FontRef {
            data: &self.data,
            offset: self.offset,
            key: self.key,
        }
    }
}

pub type SugarloafFont = fonts::SugarloafFont;
pub type SugarloafFonts = fonts::SugarloafFonts;

#[cfg(not(target_arch = "wasm32"))]
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct ComposedFontArc {
    pub regular: FontArc,
    pub bold: FontArc,
    pub italic: FontArc,
    pub bold_italic: FontArc,
}

enum FindResult {
    Found(FontData),
    NotFound(SugarloafFont),
}

#[cfg(not(target_arch = "wasm32"))]
#[inline]
fn find_font(db: &crate::font::loader::Database, font_spec: SugarloafFont) -> FindResult {
    use std::io::Read;

    if !font_spec.is_default_family() {
        let family = font_spec.family.to_string();
        let mut query = crate::font::loader::Query {
            families: &[crate::font::loader::Family::Name(&family)],
            ..crate::font::loader::Query::default()
        };

        if let Some(weight) = font_spec.weight {
            query.weight = crate::font::loader::Weight(weight);
        }

        if let Some(ref style) = font_spec.style {
            let query_style = match style.to_lowercase().as_str() {
                "italic" => crate::font::loader::Style::Italic,
                _ => crate::font::loader::Style::Normal,
            };

            query.style = query_style;
        }

        info!("Font search: '{query:?}'");

        match db.query(&query) {
            Some(id) => {
                if let Some((crate::font::loader::Source::File(ref path), _index)) =
                    db.face_source(id)
                {
                    if let Ok(mut file) = std::fs::File::open(path) {
                        let mut font_data = vec![];
                        if file.read_to_end(&mut font_data).is_ok() {
                            match FontData::from_data(font_data) {
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
    let style = font_spec.style.to_owned().unwrap_or("regular".to_string());
    let weight = font_spec.weight.unwrap_or(400);

    let font_to_load = match (weight, style.as_str()) {
        (100, "italic") => constants::FONT_CASCADIAMONO_EXTRA_LIGHT_ITALIC,
        (200, "italic") => constants::FONT_CASCADIAMONO_LIGHT_ITALIC,
        (300, "italic") => constants::FONT_CASCADIAMONO_SEMI_LIGHT_ITALIC,
        (400, "italic") => constants::FONT_CASCADIAMONO_ITALIC,
        (500, "italic") => constants::FONT_CASCADIAMONO_ITALIC,
        (600, "italic") => constants::FONT_CASCADIAMONO_SEMI_BOLD_ITALIC,
        (700, "italic") => constants::FONT_CASCADIAMONO_SEMI_BOLD_ITALIC,
        (800, "italic") => constants::FONT_CASCADIAMONO_BOLD_ITALIC,
        (900, "italic") => constants::FONT_CASCADIAMONO_BOLD_ITALIC,
        (_, "italic") => constants::FONT_CASCADIAMONO_ITALIC,
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

    FontData::from_slice(font_to_load).unwrap()
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
fn load_from_font_source(path: &PathBuf) -> Option<FontData> {
    use std::io::Read;

    if let Ok(mut file) = std::fs::File::open(path) {
        let mut font_data = vec![];
        if file.read_to_end(&mut font_data).is_ok() {
            match FontData::from_data(font_data) {
                Ok(d) => {
                    return Some(d);
                }
                Err(err_message) => {
                    tracing::info!("Failed to load font from source {err_message}");
                    return None;
                }
            }
        }
    }

    None
}
