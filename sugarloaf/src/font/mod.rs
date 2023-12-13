pub mod constants;
pub mod fonts;
#[cfg(not(target_arch = "wasm32"))]
pub mod loader;

pub const FONT_ID_REGULAR: usize = 0;
pub const FONT_ID_ITALIC: usize = 1;
pub const FONT_ID_BOLD: usize = 2;
pub const FONT_ID_BOLD_ITALIC: usize = 3;
pub const FONT_ID_SYMBOL: usize = 4;
pub const FONT_ID_EMOJIS: usize = 5;
pub const FONT_ID_BUILTIN: usize = 6;
pub const FONT_ID_ICONS: usize = 7;
pub const FONT_ID_UNICODE: usize = 8;
// After 8 is extra fonts

use crate::font::constants::*;
use ab_glyph::FontArc;

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
                            match FontArc::try_from_vec(font_data) {
                                Ok(arc) => {
                                    info!(
                                        "Font '{}' found in {}",
                                        family,
                                        path.display()
                                    );
                                    return (arc, false, None);
                                }
                                Err(err_message) => {
                                    warn!("Failed to load font '{family}' with style '{style}' and weight '{weight}', {err_message}");
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
    pub fn load(
        mut spec: SugarloafFonts,
        db_opt: Option<&loader::Database>,
    ) -> (bool, Vec<FontArc>, Vec<SugarloafFont>) {
        let mut fonts_not_fount: Vec<SugarloafFont> = vec![];
        let mut fonts: Vec<FontArc> = vec![];

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
        let is_regular_font_monospaced = regular.1;
        fonts.push(regular.0);
        if let Some(err) = regular.2 {
            fonts_not_fount.push(err);
        }

        let italic = find_font(db, spec.italic);
        fonts.push(italic.0);
        if let Some(err) = italic.2 {
            fonts_not_fount.push(err);
        }

        let bold = find_font(db, spec.bold);
        fonts.push(bold.0);
        if let Some(err) = bold.2 {
            fonts_not_fount.push(err);
        }

        let bold_italic = find_font(db, spec.bold_italic);
        fonts.push(bold_italic.0);
        if let Some(err) = bold_italic.2 {
            fonts_not_fount.push(err);
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
            fonts.push(font_arc_symbol);
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
            fonts.push(font_arc_symbol);
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let font_arc_symbol = FontArc::try_from_slice(FONT_DEJAVU_SANS).unwrap();
            fonts.push(font_arc_symbol);
        }

        let font_arc_emoji = FontArc::try_from_slice(FONT_EMOJI).unwrap();
        fonts.push(font_arc_emoji);

        let font_arc_builtin =
            FontArc::try_from_slice(FONT_CASCADIAMONO_REGULAR).unwrap();
        fonts.push(font_arc_builtin);

        let font_arc_icons =
            FontArc::try_from_slice(FONT_SYMBOLS_NERD_FONT_MONO).unwrap();
        fonts.push(font_arc_icons);

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
            fonts.push(font_arc_unicode);
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
            fonts.push(font_arc_unicode);

            let font_arc_unicode = find_font(
                db,
                SugarloafFont {
                    family: String::from("Microsoft JhengHei"),
                    style: None,
                    weight: None,
                },
            )
            .0;
            fonts.push(font_arc_unicode);
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let font_arc_unicode =
                FontArc::try_from_slice(FONT_UNICODE_FALLBACK).unwrap();
            fonts.push(font_arc_unicode);
        }

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
                fonts.push(extra_font_arc.0);
                if let Some(err) = extra_font_arc.2 {
                    fonts_not_fount.push(err);
                }
            }
        }

        (is_regular_font_monospaced, fonts, fonts_not_fount)
    }

    #[cfg(target_arch = "wasm32")]
    pub fn load(_font_spec: SugarloafFonts) -> (bool, Vec<FontArc>, Vec<SugarloafFont>) {
        (
            true,
            vec![
                FontArc::try_from_slice(FONT_CASCADIAMONO_REGULAR).unwrap(),
                FontArc::try_from_slice(FONT_CASCADIAMONO_ITALIC).unwrap(),
                FontArc::try_from_slice(FONT_CASCADIAMONO_BOLD).unwrap(),
                FontArc::try_from_slice(FONT_CASCADIAMONO_BOLD_ITALIC).unwrap(),
                FontArc::try_from_slice(FONT_DEJAVU_SANS).unwrap(),
                FontArc::try_from_slice(FONT_EMOJI).unwrap(),
                FontArc::try_from_slice(FONT_CASCADIAMONO_REGULAR).unwrap(),
                FontArc::try_from_slice(FONT_SYMBOLS_NERD_FONT_MONO).unwrap(),
                FontArc::try_from_slice(FONT_UNICODE_FALLBACK).unwrap(),
            ],
            vec![],
        )
    }
}
