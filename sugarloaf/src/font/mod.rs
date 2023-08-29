pub mod constants;
pub mod fonts;

use crate::font::constants::*;
use crate::font::fonts::*;
#[cfg(not(target_arch = "wasm32"))]
use font_kit::{properties::Style, source::SystemSource};
use glyph_brush::ab_glyph::FontArc;
#[cfg(not(target_arch = "wasm32"))]
use glyph_brush::ab_glyph::FontVec;
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
}

#[cfg(not(target_arch = "wasm32"))]
fn font_arc_from_font(font: font_kit::font::Font) -> Option<FontArc> {
    let copied_font = font.copy_font_data();
    Some(FontArc::new(
        FontVec::try_from_vec_and_index(copied_font?.to_vec(), 0).unwrap(),
    ))
}

// #[cfg(not(target_arch = "wasm32"))]
// fn find_monospace_variant(font_name: String) -> Option<font_kit::font::Font> {
//     if let Ok(system_fonts) =
//         SystemSource::new().select_family_by_name(&(font_name + " mono"))
//     {
//         let fonts = system_fonts.fonts();
//         if !fonts.is_empty() {
//             for font in fonts.iter() {
//                 let font = font.load();
//                 if let Ok(font) = font {
//                     let is_monospace = font.is_monospace();
//                     if is_monospace {
//                         return Some(font);
//                     }
//                 }
//             }
//         }
//     }

//     None
// }

#[cfg(not(target_arch = "wasm32"))]
#[inline]
fn find_font(
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
    info!("sugarloaf: search font '{family}' with style '{style}' and weight '{weight}'");

    let weight_f32 = weight as f32;
    let font_spec_style = match style.as_str() {
        "italic" => Style::Italic,
        _ => Style::Normal,
    };
    let mut nearest_font_weight = None;
    if let Ok(system_fonts) = SystemSource::new().select_family_by_name(&family) {
        let fonts = system_fonts.fonts();
        // let mut has_variant = true;

        if !fonts.is_empty() {
            for font in fonts.iter() {
                let font = font.load();
                if let Ok(font) = font {
                    let meta = font.properties();
                    let is_monospace = font.is_monospace();
                    // TODO: Look for variants
                    // if has_variant {
                    //     if let Some(_monospaced_font) =
                    //         find_monospace_variant(font_spec.family.to_string())
                    //     {
                    //         warn!("using a monospaced variant from the font\n");
                    //         let try_to_find_fonts = SugarloafFont {
                    //             family: family + "mono",
                    //             ..font_spec
                    //         };

                    //         return Font::new(try_to_find_fonts);
                    //     } else {
                    //         has_variant = false;
                    //     }
                    // }

                    if meta.style != font_spec_style {
                        continue;
                    }

                    if meta.weight.0 != weight_f32 {
                        // TODO: Improve nearest logic
                        let is_both_light = weight_f32 <= 300. && meta.weight.0 <= 300.;
                        let is_both_bold = weight_f32 >= 700. && meta.weight.0 >= 700.;
                        let is_both_regular = weight_f32 < 700.
                            && meta.weight.0 < 700.
                            && weight_f32 > 300.
                            && meta.weight.0 > 300.;

                        if is_both_light || is_both_bold || is_both_regular {
                            nearest_font_weight = Some(meta.weight.0);
                        }

                        continue;
                    }

                    if let Some(font_arc) = font_arc_from_font(font) {
                        info!("sugarloaf: OK font found '{family}' with style '{style}' and weight '{weight}'");
                        return (font_arc, is_monospace);
                    }
                }
            }
        }
    }

    warn!("sugarloaf: failed to load font '{family}' with style '{style}' and weight '{weight}'");
    if let Some(nearest) = nearest_font_weight {
        warn!(
            "sugarloaf: falling back to nearest font weight found is {:?}",
            nearest
        );
        find_font(
            SugarloafFont {
                weight: Some(nearest as u32),
                ..font_spec
            },
            fallback,
        )
    } else {
        find_font(fallback.unwrap_or_else(default_font_regular), None)
    }
}

impl Font {
    // TODO: Refactor multiple unwraps in this code
    // TODO: Use FontAttributes bold and italic
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(font_spec: SugarloafFonts) -> Font {
        let font_arc_unicode;
        let font_arc_symbol;

        #[cfg(target_os = "macos")]
        {
            let font_symbols = SystemSource::new()
                .select_by_postscript_name("Apple Symbols")
                .unwrap()
                .load()
                .unwrap();
            let copied_font_symbol = font_symbols.copy_font_data();
            let Some(copied_font_symbol) = copied_font_symbol else { todo!() };
            let font_vec_symbol =
                FontVec::try_from_vec_and_index(copied_font_symbol.to_vec(), 1).unwrap();
            font_arc_symbol = FontArc::new(font_vec_symbol);

            let font_unicode = SystemSource::new()
                .select_by_postscript_name("Arial Unicode MS")
                .unwrap()
                .load()
                .unwrap();
            let copied_font_unicode = font_unicode.copy_font_data();
            let Some(copied_font_unicode) = copied_font_unicode else { todo!() };
            let font_vec_unicode =
                FontVec::try_from_vec_and_index(copied_font_unicode.to_vec(), 3).unwrap();
            font_arc_unicode = FontArc::new(font_vec_unicode);
        }

        #[cfg(not(target_os = "macos"))]
        {
            font_arc_unicode = FontArc::try_from_slice(FONT_UNICODE_FALLBACK).unwrap();
            font_arc_symbol = FontArc::try_from_slice(FONT_DEJAVU_SANS).unwrap();
        }

        let regular = find_font(font_spec.regular, Some(default_font_regular()));
        let bold = find_font(font_spec.bold, Some(default_font_bold()));
        let bold_italic =
            find_font(font_spec.bold_italic, Some(default_font_bold_italic()));
        let italic = find_font(font_spec.italic, Some(default_font_italic()));

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
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn new(_font_name: Fonts) -> Font {
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
        }
    }
}
