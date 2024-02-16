pub mod constants;
pub mod fonts;
#[cfg(not(target_arch = "wasm32"))]
pub mod loader;

pub const FONT_ID_REGULAR: usize = 0;
pub const FONT_ID_ITALIC: usize = 1;
pub const FONT_ID_BOLD: usize = 2;
pub const FONT_ID_BOLD_ITALIC: usize = 3;
pub const FONT_ID_UNICODE: usize = 4;
pub const FONT_ID_SYMBOL: usize = 5;
pub const FONT_ID_EMOJIS: usize = 6;
pub const FONT_ID_EMOJIS_NATIVE: usize = 7;
pub const FONT_ID_ICONS: usize = 8;
pub const FONT_ID_BUILTIN: usize = 9;
// After 8 is extra fonts

use crate::font::constants::*;
use ab_glyph::FontArc;
use fnv::FnvHashMap;
use std::ops::Index;
use std::ops::IndexMut;
use swash::proxy::CharmapProxy;
use swash::text::cluster::{CharCluster, Status};
use swash::{Attributes, CacheKey, Charmap, FontRef, Synthesis};

pub use swash::{Style, Weight};

#[derive(Default)]
pub struct FontLibrary {
    pub inner: Vec<FontData>,
    cache: FnvHashMap<char, usize>,
}

impl FontLibrary {
    #[inline]
    pub fn font_arcs(&self) -> Vec<ab_glyph::FontArc> {
        self.inner
            .iter()
            .map(|x| x.arc.clone())
            .collect::<Vec<ab_glyph::FontArc>>()
    }

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

    #[inline]
    pub fn lookup_for_best_font(
        &mut self,
        cluster: &mut CharCluster,
        synth: &mut Synthesis,
    ) -> Option<usize> {
        let mut font_id = None;
        for (current_font_id, font) in self.inner.iter().enumerate() {
            let charmap = font.charmap_proxy().materialize(&font.as_ref());
            let status = cluster.map(|ch| charmap.map(ch));
            if status != Status::Discard {
                *synth = self.inner[current_font_id].synth;
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
    ) -> Option<usize> {
        let chars = cluster.chars();
        let mut font_id = FONT_ID_REGULAR;
        if let Some(cached_font_id) = self.cache.get(&chars[0].ch) {
            font_id = *cached_font_id;
        } else if cluster.info().is_emoji() {
            font_id = FONT_ID_EMOJIS_NATIVE;
        }

        let charmap = self.inner[font_id]
            .charmap_proxy()
            .materialize(&self.inner[font_id].as_ref());
        let status = cluster.map(|ch| charmap.map(ch));
        if status != Status::Discard {
            *synth = self.inner[font_id].synth;
        } else {
            log::info!("looking up for best font match for {:?}", cluster.chars());
            if let Some(found_font_id) = self.lookup_for_best_font(cluster, synth) {
                log::info!(" -> found best font id {}", found_font_id);
                font_id = found_font_id
            } else {
                return None;
            }
        }

        let chars = cluster.chars();
        if !chars.is_empty() {
            self.cache.insert(chars[0].ch, font_id);
        }
        Some(font_id)
    }
}

impl Index<usize> for FontLibrary {
    type Output = FontData;

    fn index(&self, index: usize) -> &Self::Output {
        &self.inner[index]
    }
}

impl IndexMut<usize> for FontLibrary {
    fn index_mut(&mut self, index: usize) -> &mut FontData {
        &mut self.inner[index]
    }
}

#[derive(Clone)]
pub struct FontData {
    // Full content of the font file
    data: Vec<u8>,
    // Offset to the table directory
    offset: u32,
    // Cache key
    key: CacheKey,
    // Arc
    arc: ab_glyph::FontArc,

    charmap_proxy: CharmapProxy,
    pub synth: Synthesis,
}

impl PartialEq for FontData {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data && self.offset == other.offset && self.key == other.key
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

        let arc = FontArc::try_from_vec(data.clone()).unwrap();
        let attributes = font.attributes();
        let synth = attributes.synthesize(attributes);

        Ok(Self {
            data,
            offset,
            key,
            arc,
            charmap_proxy,
            synth,
        })
    }

    #[inline]
    pub fn from_slice(data: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        let font = FontRef::from_index(data, 0).unwrap();
        let (offset, key) = (font.offset, font.key);
        // Return our struct with the original file data and copies of the
        // offset and key from the font reference

        let arc = FontArc::try_from_vec(data.to_vec()).unwrap();
        let attributes = font.attributes();
        let synth = attributes.synthesize(attributes);

        Ok(Self {
            data: data.to_vec(),
            offset,
            key,
            arc,
            charmap_proxy: CharmapProxy::from_font(&font),
            synth,
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

    #[inline]
    pub fn map_cluster(&mut self, cluster: &mut CharCluster, synth: &mut Synthesis) {
        let charmap = self.charmap_proxy.materialize(&self.as_ref());
        let status = cluster.map(|ch| charmap.map(ch));
        if status != Status::Discard {
            *synth = self.synth;
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

pub struct Font {
    pub text: ComposedFontArc,
    pub symbol: FontArc,
    pub emojis: FontArc,
    pub unicode: FontArc,
    pub icons: FontArc,
    pub breadcrumbs: FontArc,
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
                                    println!(
                                        "Font '{}' found in {}",
                                        family,
                                        path.display()
                                    );
                                    return (d, false, None);
                                }
                                Err(err_message) => {
                                    println!("Failed to load font '{family}' with style '{style}' and weight '{weight}', {err_message}");
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

    let font_to_load = match style.as_str() {
        "italic" => match weight {
            100 => constants::FONT_CASCADIAMONO_EXTRA_LIGHT_ITALIC,
            200 => constants::FONT_CASCADIAMONO_LIGHT_ITALIC,
            300 => constants::FONT_CASCADIAMONO_SEMI_LIGHT_ITALIC,
            400 => constants::FONT_CASCADIAMONO_ITALIC,
            500 => constants::FONT_CASCADIAMONO_ITALIC,
            600 => constants::FONT_CASCADIAMONO_SEMI_BOLD_ITALIC,
            700 => constants::FONT_CASCADIAMONO_SEMI_BOLD_ITALIC,
            800 => constants::FONT_CASCADIAMONO_BOLD_ITALIC,
            900 => constants::FONT_CASCADIAMONO_BOLD_ITALIC,
            _ => constants::FONT_CASCADIAMONO_ITALIC,
        },
        _ => match weight {
            100 => constants::FONT_CASCADIAMONO_EXTRA_LIGHT,
            200 => constants::FONT_CASCADIAMONO_LIGHT,
            300 => constants::FONT_CASCADIAMONO_SEMI_LIGHT,
            400 => constants::FONT_CASCADIAMONO_REGULAR,
            500 => constants::FONT_CASCADIAMONO_REGULAR,
            600 => constants::FONT_CASCADIAMONO_SEMI_BOLD,
            700 => constants::FONT_CASCADIAMONO_SEMI_BOLD,
            800 => constants::FONT_CASCADIAMONO_BOLD,
            900 => constants::FONT_CASCADIAMONO_BOLD,
            _ => constants::FONT_CASCADIAMONO_REGULAR,
        },
    };

    (FontData::from_slice(font_to_load).unwrap(), true, not_found)
}

impl Font {
    // TODO: Refactor multiple unwraps in this code
    // TODO: Use FontAttributes bold and italic
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load(
        mut spec: SugarloafFonts,
        db_opt: Option<&loader::Database>,
    ) -> (FontLibrary, Vec<SugarloafFont>) {
        let mut fonts_not_fount: Vec<SugarloafFont> = vec![];
        let mut fonts: FontLibrary = FontLibrary::default();

        // If fonts.family does exist it will overwrite all families
        if let Some(font_family_overwrite) = spec.family {
            spec.regular.family = font_family_overwrite.to_owned();
            spec.bold.family = font_family_overwrite.to_owned();
            spec.bold_italic.family = font_family_overwrite.to_owned();
            spec.italic.family = font_family_overwrite.to_owned();
        }

        let mut font_database;
        let db: &loader::Database;

        if let Some(db_ref) = db_opt {
            db = db_ref;
        } else {
            font_database = loader::Database::new();
            font_database.load_system_fonts();
            db = &font_database;
        }

        let regular = find_font(db, spec.regular);
        fonts.insert(regular.0);
        if let Some(err) = regular.2 {
            fonts_not_fount.push(err);
        }

        let italic = find_font(db, spec.italic);
        fonts.insert(italic.0);
        if let Some(err) = italic.2 {
            fonts_not_fount.push(err);
        }

        let bold = find_font(db, spec.bold);
        fonts.insert(bold.0);
        if let Some(err) = bold.2 {
            fonts_not_fount.push(err);
        }

        let bold_italic = find_font(db, spec.bold_italic);
        fonts.insert(bold_italic.0);
        if let Some(err) = bold_italic.2 {
            fonts_not_fount.push(err);
        }

        #[cfg(target_os = "macos")]
        {
            let font_arc_unicode = find_font(
                db,
                SugarloafFont {
                    family: String::from("Arial Unicode MS"),
                    style: None,
                    weight: None,
                },
            )
            .0;
            fonts.insert(font_arc_unicode);
        }

        #[cfg(target_os = "windows")]
        {
            // Lucida Sans Unicode
            let font_arc_unicode = find_font(
                db,
                SugarloafFont {
                    family: String::from("Lucida Sans Unicode"),
                    style: None,
                    weight: None,
                },
            )
            .0;
            fonts.insert(font_arc_unicode);

            let font_arc_unicode = find_font(
                db,
                SugarloafFont {
                    family: String::from("Microsoft JhengHei"),
                    style: None,
                    weight: None,
                },
            )
            .0;
            fonts.insert(font_arc_unicode);
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let font_arc_unicode = FontData::from_slice(FONT_UNICODE_FALLBACK).unwrap();
            fonts.insert(font_arc_unicode);
        }

        #[cfg(target_os = "macos")]
        {
            let font_arc_symbol = find_font(
                db,
                SugarloafFont {
                    family: String::from("Apple Symbols"),
                    style: None,
                    weight: None,
                },
            )
            .0;
            fonts.insert(font_arc_symbol);
        }

        #[cfg(target_os = "windows")]
        {
            let font_arc_symbol = find_font(
                db,
                SugarloafFont {
                    family: String::from("Symbol"),
                    style: None,
                    weight: None,
                },
            )
            .0;
            fonts.insert(font_arc_symbol);
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let font_arc_symbol = FontData::from_slice(FONT_DEJAVU_SANS).unwrap();
            fonts.insert(font_arc_symbol);
        }

        let font_arc_emoji = FontData::from_slice(FONT_EMOJI).unwrap();
        fonts.insert(font_arc_emoji);

        let font_arc_emoji_native = find_font(
            db,
            SugarloafFont {
                family: String::from("Apple Color Emoji"),
                style: None,
                weight: None,
            },
        )
        .0;
        fonts.insert(font_arc_emoji_native);

        let font_arc_icons = FontData::from_slice(FONT_SYMBOLS_NERD_FONT_MONO).unwrap();
        fonts.insert(font_arc_icons);

        let font_arc_builtin = FontData::from_slice(FONT_CASCADIAMONO_REGULAR).unwrap();
        fonts.insert(font_arc_builtin);

        if !spec.extras.is_empty() {
            for extra_font in spec.extras {
                let extra_font_arc = find_font(
                    db,
                    SugarloafFont {
                        family: extra_font.family,
                        style: extra_font.style,
                        weight: extra_font.weight,
                    },
                );
                fonts.insert(extra_font_arc.0);
                if let Some(err) = extra_font_arc.2 {
                    fonts_not_fount.push(err);
                }
            }
        }

        (fonts, fonts_not_fount)
    }

    #[cfg(target_arch = "wasm32")]
    pub fn load(_font_spec: SugarloafFonts) -> (FontLibrary, Vec<SugarloafFont>) {
        (
            vec![
                FontData::from_slice(FONT_CASCADIAMONO_REGULAR).unwrap(),
                FontData::from_slice(FONT_CASCADIAMONO_ITALIC).unwrap(),
                FontData::from_slice(FONT_CASCADIAMONO_BOLD).unwrap(),
                FontData::from_slice(FONT_CASCADIAMONO_BOLD_ITALIC).unwrap(),
                FontData::from_slice(FONT_UNICODE_FALLBACK).unwrap(),
                FontData::from_slice(FONT_DEJAVU_SANS).unwrap(),
                FontData::from_slice(FONT_EMOJI).unwrap(),
                FontData::from_slice(FONT_SYMBOLS_NERD_FONT_MONO).unwrap(),
                FontData::from_slice(FONT_CASCADIAMONO_REGULAR).unwrap(),
            ],
            vec![],
        )
    }
}
