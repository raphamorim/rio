pub mod constants;
mod fallbacks;
pub mod fonts;
#[cfg(not(target_arch = "wasm32"))]
pub mod loader;

pub const FONT_ID_REGULAR: usize = 0;

use crate::font::constants::*;
use crate::font::fonts::{SugarloafFontStyle, SugarloafFontWidth};
use crate::font_introspector::text::cluster::Parser;
use crate::font_introspector::text::cluster::Token;
use crate::font_introspector::text::cluster::{CharCluster, Status};
use crate::font_introspector::text::Codepoint;
use crate::font_introspector::text::Script;
use crate::font_introspector::{CacheKey, FontRef, Synthesis};
use crate::layout::FragmentStyle;
use crate::SugarloafErrors;
use ab_glyph::FontArc;
use lru::LruCache;
use parking_lot::FairMutex;
use rustc_hash::FxHashMap;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;

pub use crate::font_introspector::{Style, Weight};

pub fn lookup_for_font_match(
    cluster: &mut CharCluster,
    synth: &mut Synthesis,
    library: &mut FontLibraryData,
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

        if let Some(data) = library.get_data(&font_id) {
            let charmap = data.charmap();
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
    pub stash: LruCache<usize, SharedData>,
    pub hinting: bool,
}

impl Default for FontLibraryData {
    fn default() -> Self {
        Self {
            ui: FontArc::try_from_slice(FONT_CASCADIAMONO_REGULAR).unwrap(),
            inner: FxHashMap::default(),
            stash: LruCache::new(NonZeroUsize::new(2).unwrap()),
            hinting: true,
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
    pub fn insert(&mut self, font_data: FontData) {
        self.inner.insert(self.inner.len(), font_data);
    }

    #[inline]
    pub fn get(&mut self, font_id: &usize) -> &FontData {
        &self.inner[font_id]
    }

    pub fn get_data<'a>(&'a mut self, font_id: &usize) -> Option<FontRef<'a>> {
        if let Some(font) = self.inner.get(font_id) {
            match &font.data {
                Some(data) => {
                    return Some(FontRef {
                        data: data.as_ref(),
                        offset: font.offset,
                        key: font.key,
                    })
                }
                None => {
                    if !self.stash.contains(font_id) {
                        if let Some(path) = &font.path {
                            if let Some(raw_data) = load_from_font_source(path) {
                                self.stash.put(*font_id, SharedData::new(raw_data));
                            }
                        }
                    }
                }
            }

            if let Some(data) = self.stash.get(font_id) {
                return Some(FontRef {
                    data: data.as_ref(),
                    offset: font.offset,
                    key: font.key,
                });
            }
        };

        None
    }

    #[inline]
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
        db.load_system_fonts();

        match find_font(&db, spec.regular, false, false) {
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

        if let Some(ui_spec) = spec.ui {
            match find_font(&db, ui_spec, false, false) {
                FindResult::Found(data) => {
                    self.ui = FontArc::try_from_vec(data.data.unwrap().to_vec()).unwrap();
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
            .insert(FontData::from_slice(FONT_CASCADIAMONO_REGULAR, false).unwrap());

        vec![]
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
    pub fn from_data(
        data: Vec<u8>,
        path: PathBuf,
        evictable: bool,
        is_emoji: bool,
        font_spec: &SugarloafFont,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let font = FontRef::from_index(&data, 0).unwrap();
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

        let data = if evictable {
            None
        } else {
            Some(SharedData::new(data))
        };

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
        })
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
fn find_font(
    db: &crate::font::loader::Database,
    font_spec: SugarloafFont,
    evictable: bool,
    is_emoji: bool,
) -> FindResult {
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
                if let Some((crate::font::loader::Source::File(ref path), _index)) =
                    db.face_source(id)
                {
                    if let Ok(mut file) = std::fs::File::open(path) {
                        let mut font_data = vec![];
                        if file.read_to_end(&mut font_data).is_ok() {
                            match FontData::from_data(
                                font_data,
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
fn load_from_font_source(path: &PathBuf) -> Option<Vec<u8>> {
    use std::io::Read;

    if let Ok(mut file) = std::fs::File::open(path) {
        let mut font_data = vec![];
        if file.read_to_end(&mut font_data).is_ok() {
            return Some(font_data);
        }
    }

    None
}
