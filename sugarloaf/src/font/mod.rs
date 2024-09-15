pub mod constants;
mod fallbacks;
pub mod fonts;
#[cfg(not(target_arch = "wasm32"))]
pub mod loader;

pub const FONT_ID_REGULAR: usize = 0;

use crate::font::constants::*;
use crate::font_introspector::text::cluster::Parser;
use crate::font_introspector::text::cluster::Token;
use crate::font_introspector::text::cluster::{CharCluster, Status};
use crate::font_introspector::text::Codepoint;
use crate::font_introspector::text::Script;
use crate::font_introspector::{Attributes, CacheKey, FontRef, Synthesis};
use crate::layout::FragmentStyle;
use crate::SugarloafErrors;
use ab_glyph::FontArc;
use parking_lot::FairMutex;
use rustc_hash::FxHashMap;
// use std::ops::{Index, IndexMut};
use std::path::PathBuf;
use std::sync::Arc;

pub use crate::font_introspector::{Style, Weight};

pub fn lookup_for_font_match(
    cluster: &mut CharCluster,
    synth: &mut Synthesis,
    library: &mut FontLibraryData,
    spec_font_attr_opt: Option<&(crate::font_introspector::Style, bool)>,
) -> Option<(usize, bool)> {
    let mut font_id = None;
    for (current_font_id, font) in library.inner.iter() {
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

        let charmap = font.as_ref().charmap();
        let status = cluster.map(|ch| charmap.map(ch));
        if status != Status::Discard {
            let current_font_id = *current_font_id;
            *synth = library.get(&current_font_id).synth;
            font_id = Some((current_font_id, library.get(&current_font_id).is_emoji));
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

#[derive(Clone)]
pub struct FontLibrary {
    pub inner: Arc<FairMutex<FontLibraryData>>,
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

pub struct FontLibraryData {
    pub ui: FontArc,
    // Standard is fallback for everything, it is also the inner number 0
    pub inner: FxHashMap<usize, FontData>,
}

impl Default for FontLibraryData {
    fn default() -> Self {
        Self {
            ui: FontArc::try_from_slice(FONT_CASCADIAMONO_REGULAR).unwrap(),
            inner: FxHashMap::default(),
        }
    }
}

impl FontLibraryData {
    #[inline]
    pub fn find_best_font_match(
        &mut self,
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
    pub fn insert(&mut self, font_data: FontData) -> usize {
        let id = self.inner.len();
        self.inner.insert(id, font_data);
        id
    }

    pub fn get(&mut self, font_id: &usize) -> &FontData {
        println!("font_id required {}", font_id);
        &self.inner[font_id]
    }

    pub fn get_mut(&mut self, font_id: &usize) -> Option<&mut FontData> {
        self.inner.get_mut(font_id)
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
        if let Some(font_data) = self.inner.get_mut(&font_id) {
            if let Some(loaded_font_data) = load_from_font_source(&path) {
                *font_data = loaded_font_data;
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
                self.insert(data);
            }
            FindResult::NotFound(spec) => {
                self.insert(load_fallback_from_memory(&spec));
                if !spec.is_default_family() {
                    fonts_not_fount.push(spec);
                }
            }
        }

        match find_font(&db, spec.italic) {
            FindResult::Found(data) => {
                self.insert(data);
            }
            FindResult::NotFound(spec) => {
                self.insert(load_fallback_from_memory(&spec));
                if !spec.is_default_family() {
                    fonts_not_fount.push(spec);
                }
            }
        }

        match find_font(&db, spec.bold) {
            FindResult::Found(data) => {
                self.insert(data);
            }
            FindResult::NotFound(spec) => {
                self.insert(load_fallback_from_memory(&spec));
                if !spec.is_default_family() {
                    fonts_not_fount.push(spec);
                }
            }
        }

        match find_font(&db, spec.bold_italic) {
            FindResult::Found(data) => {
                self.insert(data);
            }
            FindResult::NotFound(spec) => {
                self.insert(load_fallback_from_memory(&spec));
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
                    self.insert(data);
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
                    let id = self.insert(data);
                    if let Some(ref mut font_ref) = self.inner.get_mut(&id) {
                        font_ref.is_emoji = true;
                    }
                }
                FindResult::NotFound(spec) => {
                    let id =
                        self.insert(FontData::from_slice(FONT_TWEMOJI_EMOJI).unwrap());
                    if let Some(ref mut font_ref) = self.inner.get_mut(&id) {
                        font_ref.is_emoji = true;
                    }
                    if !spec.is_default_family() {
                        fonts_not_fount.push(spec);
                    }
                }
            }
        } else {
            let id = self.insert(FontData::from_slice(FONT_TWEMOJI_EMOJI).unwrap());
            if let Some(ref mut font_ref) = self.inner.get_mut(&id) {
                font_ref.is_emoji = true;
            }
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
                    self.insert(data);
                }
                FindResult::NotFound(spec) => {
                    fonts_not_fount.push(spec);
                }
            }
        }

        self.insert(FontData::from_slice(FONT_SYMBOLS_NERD_FONT_MONO).unwrap());

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

// impl Index<usize> for FontLibraryData {
//     type Output = FontData;

//     fn index(&self, index: usize) -> &Self::Output {
//         match &self.inner[index] {
//             FontSource::Data(font_ref) => font_ref,
//             FontSource::Standard => &self.standard,
//         }
//     }
// }

// impl IndexMut<usize> for FontLibraryData {
//     fn index_mut(&mut self, index: usize) -> &mut FontData {
//         match &mut self.inner[index] {
//             FontSource::Data(font_ref) => font_ref,
//             FontSource::Standard => &mut self.standard,
//         }
//     }
// }

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
