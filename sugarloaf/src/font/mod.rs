pub mod constants;
mod fallbacks;
pub mod fonts;
#[cfg(not(target_arch = "wasm32"))]
pub mod loader;

pub const FONT_ID_REGULAR: usize = 0;
pub const FONT_ID_ITALIC: usize = 1;
pub const FONT_ID_BOLD: usize = 2;
pub const FONT_ID_BOLD_ITALIC: usize = 3;

use crate::font::constants::*;
use ab_glyph::FontArc;
use std::collections::HashMap;
use std::ops::{Index, IndexMut};
use std::sync::{Arc, RwLock};
use swash::proxy::CharmapProxy;
use swash::text::cluster::{CharCluster, Status};
use swash::{Attributes, CacheKey, Charmap, FontRef, Synthesis};

pub use swash::{Style, Weight};

#[derive(Debug)]
enum Inner {
    #[allow(unused)]
    #[cfg(not(target_arch = "wasm32"))]
    Mapped(memmap2::Mmap),
    Memory(Vec<u8>),
}

impl Inner {
    fn data(&self) -> &[u8] {
        match self {
            Self::Mapped(mmap) => mmap,
            Self::Memory(vec) => vec,
        }
    }
}

#[derive(Default)]
pub struct FontContext {
    cache: HashMap<String, usize>,
}

impl FontContext {
    #[inline]
    pub fn lookup_for_best_font(
        &mut self,
        cluster: &mut CharCluster,
        synth: &mut Synthesis,
        library: &FontLibraryData,
    ) -> Option<usize> {
        let mut font_id = None;
        for (current_font_id, font) in library.inner.iter().enumerate() {
            let charmap = font.charmap_proxy().materialize(&font.as_ref());
            let status = cluster.map(|ch| charmap.map(ch));
            if status != Status::Discard {
                *synth = library[current_font_id].synth;
                font_id = Some(current_font_id);
                break;
            }
        }

        font_id
    }

    #[inline]
    pub fn map_cluster(
        &mut self,
        cluster: &mut CharCluster,
        synth: &mut Synthesis,
        library: &FontLibraryData,
    ) -> Option<usize> {
        let mut cache_key: String = String::default();
        for c in cluster.chars().iter() {
            cache_key.push(c.ch);
        }
        let mut font_id = FONT_ID_REGULAR;
        let is_cache_key_empty = cache_key.is_empty();

        if !is_cache_key_empty {
            if let Some(cached_font_id) = self.cache.get(&cache_key) {
                font_id = *cached_font_id;
            } else if cluster.info().is_emoji() {
                if let Some(font_emoji_id) = library.inner.iter().position(|r| r.is_emoji)
                {
                    font_id = font_emoji_id;
                }
            }
        }

        let charmap = library[font_id]
            .charmap_proxy()
            .materialize(&library[font_id].as_ref());
        let status = cluster.map(|ch| charmap.map(ch));
        if status != Status::Discard {
            *synth = library[font_id].synth;
        } else {
            log::info!("looking up for best font match for {:?}", cluster.chars());
            if let Some(found_font_id) =
                self.lookup_for_best_font(cluster, synth, library)
            {
                log::info!(" -> found best font id {}", found_font_id);
                font_id = found_font_id
            } else {
                return None;
            }
        }

        if !is_cache_key_empty {
            self.cache.insert(cache_key, font_id);
        }

        Some(font_id)
    }
}

#[derive(Clone)]
pub struct FontLibrary {
    pub(super) inner: Arc<RwLock<FontLibraryData>>,
}

impl FontLibrary {
    pub fn new(spec: SugarloafFonts) -> Self {
        let mut font_library = FontLibraryData::default();
        // let mut sugarloaf_errors = None;

        // let (font_library, fonts_not_found) = loader;

        // if !fonts_not_found.is_empty() {
        //     sugarloaf_errors = Some(SugarloafErrors { fonts_not_found });
        // }

        let _fonts_not_found = font_library.load(spec);

        Self {
            inner: Arc::new(RwLock::new(font_library)),
        }
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

pub struct FontLibraryData {
    pub main: FontArc,
    pub inner: Vec<FontData>,
    db: loader::Database,
}

impl Default for FontLibraryData {
    fn default() -> Self {
        let mut db = loader::Database::new();
        db.load_system_fonts();
        Self {
            db,
            main: FontArc::try_from_slice(FONT_CASCADIAMONO_REGULAR).unwrap(),
            inner: vec![],
        }
    }
}

impl FontLibraryData {
    #[inline]
    pub fn insert(&mut self, font_data: FontData) {
        self.inner.push(font_data);
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
    pub fn font_by_id(&self, font_id: usize) -> FontRef {
        self.inner[font_id].as_ref()
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

        let regular = find_font(&self.db, spec.regular);
        self.inner.push(regular.0);
        if let Some(err) = regular.2 {
            fonts_not_fount.push(err);
        }

        let italic = find_font(&self.db, spec.italic);
        self.inner.push(italic.0);
        if let Some(err) = italic.2 {
            fonts_not_fount.push(err);
        }

        let bold = find_font(&self.db, spec.bold);
        self.inner.push(bold.0);
        if let Some(err) = bold.2 {
            fonts_not_fount.push(err);
        }

        let bold_italic = find_font(&self.db, spec.bold_italic);
        self.inner.push(bold_italic.0);
        if let Some(err) = bold_italic.2 {
            fonts_not_fount.push(err);
        }

        for fallback in fallbacks::external_fallbacks() {
            let is_emoji = fallback.contains("emoji");
            let mut font_data = find_font(
                &self.db,
                SugarloafFont {
                    family: fallback,
                    ..SugarloafFont::default()
                },
            );
            // Hacky way to declare emojis
            if is_emoji {
                font_data.0.is_emoji = true;
            }
            self.inner.push(font_data.0);
            if let Some(err) = font_data.2 {
                fonts_not_fount.push(err);
            }
        }

        if !spec.extras.is_empty() {
            for extra_font in spec.extras {
                let extra_font_arc = find_font(
                    &self.db,
                    SugarloafFont {
                        family: extra_font.family,
                        style: extra_font.style,
                        weight: extra_font.weight,
                    },
                );
                self.inner.push(extra_font_arc.0);
                if let Some(err) = extra_font_arc.2 {
                    fonts_not_fount.push(err);
                }
            }
        }

        self.inner
            .push(FontData::from_slice(FONT_SYMBOLS_NERD_FONT_MONO).unwrap());

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
        &self.inner[index]
    }
}

impl IndexMut<usize> for FontLibraryData {
    fn index_mut(&mut self, index: usize) -> &mut FontData {
        &mut self.inner[index]
    }
}

/// Atomically reference counted, heap allocated or memory mapped buffer.
#[derive(Clone, Debug)]
#[repr(transparent)]
pub struct SharedData {
    inner: Arc<Inner>,
}

impl SharedData {
    /// Creates shared data from the specified bytes.
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            inner: Arc::new(Inner::Memory(data)),
        }
    }

    /// Returns the underlying bytes of the data.
    pub fn as_bytes(&self) -> &[u8] {
        self.inner.data()
    }

    /// Returns the number of strong references to the data.
    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.inner)
    }
}

impl std::ops::Deref for SharedData {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.inner.data()
    }
}

impl AsRef<[u8]> for SharedData {
    fn as_ref(&self) -> &[u8] {
        self.inner.data()
    }
}

#[derive(Clone)]
pub struct FontData {
    // Full content of the font file
    data: SharedData,
    // Offset to the table directory
    offset: u32,
    // Cache key
    key: CacheKey,
    charmap_proxy: CharmapProxy,

    pub is_emoji: bool,
    pub weight: swash::Weight,
    pub style: swash::Style,
    pub stretch: swash::Stretch,
    pub synth: Synthesis,
}

impl PartialEq for FontData {
    fn eq(&self, other: &Self) -> bool {
        // self.data == other.data && self.offset == other.offset &&
        self.key == other.key
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
            is_emoji: false,
            key,
            charmap_proxy,
            synth,
            style,
            weight,
            stretch,
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
            is_emoji: false,
            key,
            charmap_proxy,
            synth,
            style,
            weight,
            stretch,
        })
    }

    // As a convenience, you may want to forward some methods.
    #[inline]
    pub fn attributes(&self) -> Attributes {
        self.as_ref().attributes()
    }

    #[inline]
    pub fn charmap(&self) -> Charmap {
        self.as_ref().charmap()
    }

    #[inline]
    pub fn charmap_proxy(&self) -> CharmapProxy {
        self.charmap_proxy
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
use log::{info, warn};

#[derive(Debug, Clone)]
pub struct ComposedFontArc {
    pub regular: FontArc,
    pub bold: FontArc,
    pub italic: FontArc,
    pub bold_italic: FontArc,
}

#[cfg(not(target_arch = "wasm32"))]
#[inline]
fn find_font(
    db: &crate::font::loader::Database,
    font_spec: SugarloafFont,
) -> (FontData, bool, Option<SugarloafFont>) {
    use std::io::Read;

    let weight = font_spec.weight.unwrap_or(400);
    let style = font_spec
        .style
        .to_owned()
        .unwrap_or(String::from("normal"))
        .to_lowercase();

    let mut not_found = None;

    if !font_spec.is_default_family() {
        let family = font_spec.family.to_string();
        info!(
            "Font search: family '{family}' with style '{style}' and weight '{weight}'"
        );

        let query_style = match style.as_str() {
            "italic" => crate::font::loader::Style::Italic,
            _ => crate::font::loader::Style::Normal,
        };

        let query = crate::font::loader::Query {
            families: &[crate::font::loader::Family::Name(&family)],
            weight: crate::font::loader::Weight(weight),
            style: query_style,
            ..crate::font::loader::Query::default()
        };

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
                                    log::info!(
                                        "Font '{}' found in {}",
                                        family,
                                        path.display()
                                    );
                                    return (d, false, None);
                                }
                                Err(err_message) => {
                                    log::info!("Failed to load font '{family}' with style '{style}' and weight '{weight}', {err_message}");
                                    return (
                                        FontData::from_slice(
                                            constants::FONT_CASCADIAMONO_REGULAR,
                                        )
                                        .unwrap(),
                                        true,
                                        Some(font_spec),
                                    );
                                }
                            }
                        }
                    }
                }
            }
            None => {
                not_found = Some(font_spec);
                warn!("Failed to find font '{family}' with style '{style}' and weight '{weight}'");
            }
        }
    }

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

    (FontData::from_slice(font_to_load).unwrap(), true, not_found)
}
