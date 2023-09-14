pub mod constants;
pub mod loader;
pub mod fonts;
// pub mod ligatures;

pub const FONT_ID_REGULAR: usize = 0;
pub const FONT_ID_ITALIC: usize = 1;
pub const FONT_ID_BOLD: usize = 2;
pub const FONT_ID_BOLD_ITALIC: usize = 3;
pub const FONT_ID_SYMBOL: usize = 4;
pub const FONT_ID_EMOJIS: usize = 5;
pub const FONT_ID_UNICODE: usize = 6;
pub const FONT_ID_ICONS: usize = 7;
pub const FONT_ID_BUILTIN: usize = 8;

use crate::font::constants::*;
use glyph_brush::ab_glyph::FontArc;

pub type SugarloafFont = fonts::SugarloafFont;
pub type SugarloafFonts = fonts::SugarloafFonts;

#[cfg(not(target_arch = "wasm32"))]
use log::{info, warn};

#[derive(Debug, Clone)]
pub struct ComposedFontArc {
    pub is_monospace: bool,
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
) -> (FontArc, bool, Option<SugarloafFont>) {
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
                if let Some((crate::font::loader::Source::File(ref path), _index)) = db.face_source(id)
                {
                    if let Ok(bytes) = std::fs::read(path) {
                        match FontArc::try_from_vec(bytes.to_vec()) {
                            Ok(arc) => {
                                warn!("Font '{}' found in {}", family, path.display());
                                return (arc, false, None);
                            }
                            Err(_) => {
                                warn!("Failed to load font '{family}' with style '{style}' and weight '{weight}'");
                                return (
                                    FontArc::try_from_slice(
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

    (
        FontArc::try_from_slice(font_to_load).unwrap(),
        true,
        not_found,
    )
}

impl Font {
    // TODO: Refactor multiple unwraps in this code
    // TODO: Use FontAttributes bold and italic
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(mut font_spec: SugarloafFonts) -> (Font, Vec<SugarloafFont>) {
        let mut fonts_not_fount: Vec<SugarloafFont> = vec![];

        let font_arc_unicode;
        let font_arc_symbol;

        let mut db = crate::font::loader::Database::new();
        db.load_system_fonts();
        db.set_serif_family("Times New Roman");
        db.set_sans_serif_family("Arial");
        db.set_cursive_family("Comic Sans MS");
        db.set_fantasy_family("Impact");
        db.set_monospace_family("Courier New");

        // If fonts.family does exist it will overwrite all families
        if let Some(font_family_overwrite) = font_spec.family {
            font_spec.regular.family = font_family_overwrite.to_owned();
            font_spec.bold.family = font_family_overwrite.to_owned();
            font_spec.bold_italic.family = font_family_overwrite.to_owned();
            font_spec.italic.family = font_family_overwrite.to_owned();
        }

        #[cfg(target_os = "macos")]
        {
            font_arc_symbol = find_font(
                &db,
                SugarloafFont {
                    family: String::from("Apple Symbols"),
                    style: None,
                    weight: None,
                },
            )
            .0;

            font_arc_unicode = find_font(
                &db,
                SugarloafFont {
                    family: String::from("Arial Unicode MS"),
                    style: None,
                    weight: None,
                },
            )
            .0;
        }

        #[cfg(not(target_os = "macos"))]
        {
            font_arc_unicode = FontArc::try_from_slice(FONT_UNICODE_FALLBACK).unwrap();
            font_arc_symbol = FontArc::try_from_slice(FONT_DEJAVU_SANS).unwrap();
        }

        let regular = find_font(&db, font_spec.regular);
        if let Some(err) = regular.2 {
            fonts_not_fount.push(err);
        }

        let bold = find_font(&db, font_spec.bold);
        if let Some(err) = bold.2 {
            fonts_not_fount.push(err);
        }

        let bold_italic = find_font(&db, font_spec.bold_italic);
        if let Some(err) = bold_italic.2 {
            fonts_not_fount.push(err);
        }

        let italic = find_font(&db, font_spec.italic);
        if let Some(err) = italic.2 {
            fonts_not_fount.push(err);
        }

        (
            Font {
                text: ComposedFontArc {
                    is_monospace: regular.1,
                    regular: regular.0,
                    bold: bold.0,
                    bold_italic: bold_italic.0,
                    italic: italic.0,
                },
                symbol: font_arc_symbol,
                emojis: FontArc::try_from_slice(FONT_EMOJI).unwrap(),
                unicode: font_arc_unicode,
                icons: FontArc::try_from_slice(FONT_SYMBOLS_NERD_FONT_MONO).unwrap(),
                breadcrumbs: FontArc::try_from_slice(FONT_CASCADIAMONO_REGULAR).unwrap(),
            },
            fonts_not_fount,
        )
    }

    #[cfg(target_arch = "wasm32")]
    pub fn new(_font_spec: SugarloafFonts) -> (Font, Vec<SugarloafFont>) {
        let font_arc_unicode = FontArc::try_from_slice(FONT_UNICODE_FALLBACK).unwrap();
        let font_arc_symbol = FontArc::try_from_slice(FONT_DEJAVU_SANS).unwrap();

        (
            Font {
                text: ComposedFontArc {
                    is_monospace: true,
                    bold: FontArc::try_from_slice(FONT_CASCADIAMONO_BOLD).unwrap(),
                    bold_italic: FontArc::try_from_slice(FONT_CASCADIAMONO_BOLD_ITALIC)
                        .unwrap(),
                    italic: FontArc::try_from_slice(FONT_CASCADIAMONO_ITALIC).unwrap(),
                    regular: FontArc::try_from_slice(FONT_CASCADIAMONO_REGULAR).unwrap(),
                },
                symbol: font_arc_symbol,
                emojis: FontArc::try_from_slice(FONT_EMOJI).unwrap(),
                unicode: font_arc_unicode,
                icons: FontArc::try_from_slice(FONT_SYMBOLS_NERD_FONT_MONO).unwrap(),
                breadcrumbs: FontArc::try_from_slice(FONT_CASCADIAMONO_REGULAR).unwrap(),
            },
            vec![],
        )
    }
}
