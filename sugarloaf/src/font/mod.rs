pub mod constants;
pub mod fonts;
pub mod ligatures;

use crate::font::constants::*;
use crate::font::fonts::*;
#[cfg(not(target_arch = "wasm32"))]
use glyph_brush::ab_glyph::FontArc;

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
    db: &fontdb::Database,
    font_spec: SugarloafFont,
    fallback: Option<SugarloafFont>,
) -> (FontArc, bool) {
    let weight = font_spec.weight.unwrap_or(400);
    let style = font_spec
        .style
        .to_owned()
        .unwrap_or(String::from("normal"))
        .to_lowercase();

    if font_spec.is_default_family() {
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

        return (FontArc::try_from_slice(font_to_load).unwrap(), true);
    }

    let family = font_spec.family.to_string();
    info!("Font search: family '{family}' with style '{style}' and weight '{weight}'");

    let query_style = match style.as_str() {
        "italic" => fontdb::Style::Italic,
        _ => fontdb::Style::Normal,
    };

    let query = fontdb::Query {
        families: &[fontdb::Family::Name(&family), fontdb::Family::Monospace],
        weight: fontdb::Weight(weight),
        style: query_style,
        ..fontdb::Query::default()
    };

    match db.query(&query) {
        Some(id) => {
            if let Some((src, _index)) = db.face_source(id) {
                if let fontdb::Source::File(ref path) = &src {
                    if let Ok(bytes) = std::fs::read(path) {
                        match FontArc::try_from_vec(bytes.to_vec()) {
                            Ok(arc) => {
                                info!("Font '{}' found in {}", family, path.display());
                                return (arc, false);
                            }
                            Err(_) => {
                                return (
                                    FontArc::try_from_slice(
                                        constants::FONT_CASCADIAMONO_REGULAR,
                                    )
                                    .unwrap(),
                                    true,
                                );
                            }
                        }
                    }
                }
            }
        }
        None => {
            warn!("Font '{}' not found.", family);
        }
    }

    // let mut nearest_font_weight = None;
    // if let Ok(system_fonts) = SystemSource::new().select_family_by_name(&family) {
    //     let fonts = system_fonts.fonts();
    //     // let mut has_variant = true;

    //     if !fonts.is_empty() {
    //         for font in fonts.iter() {
    //             let font = font.load();
    //             if let Ok(font) = font {
    //                 let meta = font.properties();
    //                 let is_monospace = font.is_monospace();
    //                 // TODO: Look for variants
    //                 // if has_variant {
    //                 //     if let Some(_monospaced_font) =
    //                 //         find_monospace_variant(font_spec.family.to_string())
    //                 //     {
    //                 //         warn!("using a monospaced variant from the font\n");
    //                 //         let try_to_find_fonts = SugarloafFont {
    //                 //             family: family + "mono",
    //                 //             ..font_spec
    //                 //         };

    //                 //         return Font::new(try_to_find_fonts);
    //                 //     } else {
    //                 //         has_variant = false;
    //                 //     }
    //                 // }

    //                 if meta.style != font_spec_style {
    //                     continue;
    //                 }

    //                 if meta.weight.0 != weight_f32 {
    //                     // TODO: Improve nearest logic
    //                     let is_both_light = weight_f32 <= 300. && meta.weight.0 <= 300.;
    //                     let is_both_bold = weight_f32 >= 700. && meta.weight.0 >= 700.;
    //                     let is_both_regular = weight_f32 < 700.
    //                         && meta.weight.0 < 700.
    //                         && weight_f32 > 300.
    //                         && meta.weight.0 > 300.;

    //                     if is_both_light || is_both_bold || is_both_regular {
    //                         nearest_font_weight = Some(meta.weight.0);
    //                     }

    //                     continue;
    //                 }

    //                 if let Some(font_arc) = font_arc_from_font(font) {
    //                     info!("sugarloaf: OK font found '{family}' with style '{style}' and weight '{weight}'");
    //                     return (font_arc, is_monospace);
    //                 }
    //             }
    //         }
    //     }
    // }

    warn!("sugarloaf: failed to load font '{family}' with style '{style}' and weight '{weight}'");
    // if let Some(nearest) = nearest_font_weight {
    //     warn!(
    //         "sugarloaf: falling back to nearest font weight found is {:?}",
    //         nearest
    //     );
    //     find_font(
    //         SugarloafFont {
    //             weight: Some(nearest as u32),
    //             ..font_spec
    //         },
    //         fallback,
    //     )
    // } else {
    find_font(db, fallback.unwrap_or_else(default_font_regular), None)
    // }
}

impl Font {
    // TODO: Refactor multiple unwraps in this code
    // TODO: Use FontAttributes bold and italic
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(mut font_spec: SugarloafFonts) -> Font {
        let font_arc_unicode;
        let font_arc_symbol;

        let mut db = fontdb::Database::new();
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
                Some(default_font_regular()),
            );

            font_arc_unicode = find_font(
                &db,
                SugarloafFont {
                    family: String::from("Arial Unicode MS"),
                    style: None,
                    weight: None,
                },
                Some(default_font_regular()),
            );
        }

        #[cfg(not(target_os = "macos"))]
        {
            font_arc_unicode = FontArc::try_from_slice(FONT_UNICODE_FALLBACK).unwrap();
            font_arc_symbol = FontArc::try_from_slice(FONT_DEJAVU_SANS).unwrap();
        }

        let regular = find_font(&db, font_spec.regular, Some(default_font_regular()));
        let bold = find_font(&db, font_spec.bold, Some(default_font_bold()));
        let bold_italic =
            find_font(&db, font_spec.bold_italic, Some(default_font_bold_italic()));
        let italic = find_font(&db, font_spec.italic, Some(default_font_italic()));

        Font {
            text: ComposedFontArc {
                is_monospace: regular.1,
                regular: regular.0,
                bold: bold.0,
                bold_italic: bold_italic.0,
                italic: italic.0,
            },
            symbol: font_arc_symbol.0,
            emojis: FontArc::try_from_slice(FONT_EMOJI).unwrap(),
            unicode: font_arc_unicode.0,
            icons: FontArc::try_from_slice(FONT_SYMBOLS_NERD_FONT_MONO).unwrap(),
            breadcrumbs: FontArc::try_from_slice(FONT_CASCADIAMONO_REGULAR).unwrap(),
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn new(_font_spec: SugarloafFonts) -> Font {
        let font_arc_unicode = FontArc::try_from_slice(FONT_UNICODE_FALLBACK).unwrap();
        let font_arc_symbol = FontArc::try_from_slice(FONT_DEJAVU_SANS).unwrap();

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
        }
    }
}
