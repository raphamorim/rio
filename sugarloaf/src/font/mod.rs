pub mod constants;

use crate::font::constants::*;
#[cfg(not(target_arch = "wasm32"))]
use font_kit::{properties::Style, source::SystemSource};
use glyph_brush::ab_glyph::FontArc;
#[cfg(not(target_arch = "wasm32"))]
use glyph_brush::ab_glyph::FontVec;
#[cfg(not(target_arch = "wasm32"))]
use log::warn;

#[derive(Debug, Clone)]
pub struct ComposedFontArc {
    pub is_monospace: bool,
    pub regular: FontArc,
    pub bold: FontArc,
    pub italic: FontArc,
    pub bold_italic: FontArc,
    pub extra_light: FontArc,
    pub extra_light_italic: FontArc,
    pub light: FontArc,
    pub light_italic: FontArc,
    pub semi_bold: FontArc,
    pub semi_bold_italic: FontArc,
    pub semi_light: FontArc,
    pub semi_light_italic: FontArc,
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

#[cfg(not(target_arch = "wasm32"))]
fn find_monospace_variant(font_name: String) -> Option<font_kit::font::Font> {
    if let Ok(system_fonts) =
        SystemSource::new().select_family_by_name(&(font_name + " mono"))
    {
        let fonts = system_fonts.fonts();
        if !fonts.is_empty() {
            for font in fonts.iter() {
                let font = font.load();
                if let Ok(font) = font {
                    let is_monospace = font.is_monospace();
                    if is_monospace {
                        return Some(font);
                    }
                }
            }
        }
    }

    None
}

impl Font {
    // TODO: Refactor multiple unwraps in this code
    // TODO: Use FontAttributes bold and italic
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(font_name: String) -> Font {
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

        let mut text_fonts = ComposedFontArc {
            is_monospace: true,
            bold: FontArc::try_from_slice(FONT_CASCADIAMONO_BOLD).unwrap(),
            bold_italic: FontArc::try_from_slice(FONT_CASCADIAMONO_BOLD_ITALIC).unwrap(),
            extra_light: FontArc::try_from_slice(FONT_CASCADIAMONO_EXTRA_LIGHT).unwrap(),
            extra_light_italic: FontArc::try_from_slice(
                FONT_CASCADIAMONO_EXTRA_LIGHT_ITALIC,
            )
            .unwrap(),
            italic: FontArc::try_from_slice(FONT_CASCADIAMONO_ITALIC).unwrap(),
            light: FontArc::try_from_slice(FONT_CASCADIAMONO_LIGHT).unwrap(),
            light_italic: FontArc::try_from_slice(FONT_CASCADIAMONO_LIGHT_ITALIC)
                .unwrap(),
            regular: FontArc::try_from_slice(FONT_CASCADIAMONO_REGULAR).unwrap(),
            semi_bold: FontArc::try_from_slice(FONT_CASCADIAMONO_SEMI_BOLD).unwrap(),
            semi_bold_italic: FontArc::try_from_slice(FONT_CASCADIAMONO_SEMI_BOLD_ITALIC)
                .unwrap(),
            semi_light: FontArc::try_from_slice(FONT_CASCADIAMONO_SEMI_LIGHT).unwrap(),
            semi_light_italic: FontArc::try_from_slice(
                FONT_CASCADIAMONO_SEMI_LIGHT_ITALIC,
            )
            .unwrap(),
        };

        let is_default_font = font_name.to_lowercase() == DEFAULT_FONT_NAME;
        if !is_default_font {
            if let Ok(system_fonts) =
                SystemSource::new().select_family_by_name(&font_name)
            {
                let fonts = system_fonts.fonts();
                let mut has_variant = true;
                if !fonts.is_empty() {
                    for font in fonts.iter() {
                        let font = font.load();
                        if let Ok(font) = font {
                            let meta = font.properties();
                            let is_monospace = font.is_monospace();
                            if has_variant {
                                if let Some(_monospaced_font) =
                                    find_monospace_variant(font_name.to_string())
                                {
                                    warn!("using a monospaced variant from the font: {font_name}\n");
                                    return Font::new(font_name + " mono");
                                } else {
                                    has_variant = false;
                                }
                            }

                            match meta.style {
                                Style::Normal => {
                                    //TODO: Find a way to use struct Weight
                                    match meta.weight.0.round() as i32 {
                                        //NORMAL
                                        300 | 400 | 500 => {
                                            if let Some(font_arc) =
                                                font_arc_from_font(font)
                                            {
                                                text_fonts.regular = font_arc;
                                                text_fonts.is_monospace = is_monospace;
                                            }
                                        }
                                        //BOLD
                                        600 | 700 | 800 => {
                                            if let Some(font_arc) =
                                                font_arc_from_font(font)
                                            {
                                                text_fonts.bold = font_arc;
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                Style::Italic => {
                                    match meta.weight.0.round() as i32 {
                                        //NORMAL
                                        400 => {
                                            if let Some(font_arc) =
                                                font_arc_from_font(font)
                                            {
                                                text_fonts.italic = font_arc;
                                            }
                                        }
                                        //BOLD
                                        700 => {
                                            if let Some(font_arc) =
                                                font_arc_from_font(font)
                                            {
                                                text_fonts.bold_italic = font_arc;
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                _ => {}
                            }
                        }
                    }

                    if !text_fonts.is_monospace {
                        warn!("using a non monospaced font: {font_name}\nSugarloaf will do the best can do for render it although please consider use a monospaced font");
                    }

                    return Font {
                        text: text_fonts,
                        symbol: font_arc_symbol,
                        emojis: FontArc::try_from_slice(FONT_EMOJI).unwrap(),
                        unicode: font_arc_unicode,
                        icons: FontArc::try_from_slice(FONT_SYMBOLS_NERD_FONT_MONO)
                            .unwrap(),
                    };
                }
            }

            warn!("failed to load font {font_name}");
        }

        Font {
            text: text_fonts,
            symbol: font_arc_symbol,
            emojis: FontArc::try_from_slice(FONT_EMOJI).unwrap(),
            unicode: font_arc_unicode,
            icons: FontArc::try_from_slice(FONT_SYMBOLS_NERD_FONT_MONO).unwrap(),
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn new(_font_name: String) -> Font {
        let font_arc_unicode = FontArc::try_from_slice(FONT_UNICODE_FALLBACK).unwrap();
        let font_arc_symbol = FontArc::try_from_slice(FONT_DEJAVU_SANS).unwrap();

        Font {
            text: ComposedFontArc {
                is_monospace: true,
                bold: FontArc::try_from_slice(FONT_CASCADIAMONO_BOLD).unwrap(),
                bold_italic: FontArc::try_from_slice(FONT_CASCADIAMONO_BOLD_ITALIC)
                    .unwrap(),
                extra_light: FontArc::try_from_slice(FONT_CASCADIAMONO_EXTRA_LIGHT)
                    .unwrap(),
                extra_light_italic: FontArc::try_from_slice(
                    FONT_CASCADIAMONO_EXTRA_LIGHT_ITALIC,
                )
                .unwrap(),
                italic: FontArc::try_from_slice(FONT_CASCADIAMONO_ITALIC).unwrap(),
                light: FontArc::try_from_slice(FONT_CASCADIAMONO_LIGHT).unwrap(),
                light_italic: FontArc::try_from_slice(FONT_CASCADIAMONO_LIGHT_ITALIC)
                    .unwrap(),
                regular: FontArc::try_from_slice(FONT_CASCADIAMONO_REGULAR).unwrap(),
                semi_bold: FontArc::try_from_slice(FONT_CASCADIAMONO_SEMI_BOLD).unwrap(),
                semi_bold_italic: FontArc::try_from_slice(
                    FONT_CASCADIAMONO_SEMI_BOLD_ITALIC,
                )
                .unwrap(),
                semi_light: FontArc::try_from_slice(FONT_CASCADIAMONO_SEMI_LIGHT)
                    .unwrap(),
                semi_light_italic: FontArc::try_from_slice(
                    FONT_CASCADIAMONO_SEMI_LIGHT_ITALIC,
                )
                .unwrap(),
            },
            symbol: font_arc_symbol,
            emojis: FontArc::try_from_slice(FONT_EMOJI).unwrap(),
            unicode: font_arc_unicode,
            icons: FontArc::try_from_slice(FONT_SYMBOLS_NERD_FONT_MONO).unwrap(),
        }
    }

    // pub async fn all() -> AllSystemFonts {
    //     let source = SystemSource::new();
    //     source.all_fonts().unwrap_or(vec![])
    // }

    // pub fn search_font_by_content(content: char, all_fonts: &AllSystemFonts) -> Option<FontArc> {
    //     let mut fonts_found = vec![];
    //     for system_font in all_fonts.iter() {
    //         if let Ok(font) = system_font.load() {
    //             match font.glyph_for_char(content) {
    //                 Some(f) => {
    //                     println!("{:?}", f);
    //                     fonts_found.push(font);
    //                 }
    //                 None => {
    //                     continue;
    //                 }
    //             }
    //         }
    //     }

    //     log::info!("{content:?} is available in {fonts_found:?}");
    //     None
    // }
}
