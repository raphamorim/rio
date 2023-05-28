use font_kit::source::SystemSource;
use glyph_brush::ab_glyph::{FontArc, FontVec};
use log::warn;

pub const DEFAULT_FONT_NAME: &str = "cascadiamono";

pub const FONT_CASCADIAMONO_REGULAR: &[u8; 308212] =
    include_bytes!("./resources/CascadiaMono/CascadiaMonoPL-Regular.otf");

pub const FONT_CASCADIAMONO_BOLD: &[u8; 312976] =
    include_bytes!("./resources/CascadiaMono/CascadiaMonoPL-Bold.otf");

pub const FONT_CASCADIAMONO_ITALIC: &[u8; 191296] =
    include_bytes!("./resources/CascadiaMono/CascadiaMonoPL-Italic.otf");

pub const FONT_CASCADIAMONO_BOLD_ITALIC: &[u8; 193360] =
    include_bytes!("./resources/CascadiaMono/CascadiaMonoPL-BoldItalic.otf");

pub const FONT_EMOJI: &[u8; 877988] =
    include_bytes!("./resources/NotoEmoji/static/NotoEmoji-Regular.ttf");

#[cfg(not(target_os = "macos"))]
pub const FONT_DEJAVU_MONO: &[u8; 340712] =
    include_bytes!("./resources/DejaVuSansMono.ttf");

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
}

impl Font {
    pub fn new(font_name: String) -> Result<Font, String> {
        // TODO:
        // This code is quite unsafe and needs a proper refactor
        // adding font load fallbacks for all categories.

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

            // TODO: Load native emojis
            // let font_emojis = SystemSource::new()
            //     .select_by_postscript_name("Apple Color Emoji")
            //     .unwrap()
            //     .load()
            //     .unwrap();
            // let copied_font_emojis = font_emojis.copy_font_data();
            // let Some(copied_font_emojis) = copied_font_emojis else { todo!() };
            // let font_vec_emojis = FontVec::try_from_vec_and_index(copied_font_emojis.to_vec(), 2).unwrap();

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
            font_arc_unicode = FontArc::try_from_slice(FONT_DEJAVU_MONO).unwrap();
            font_arc_symbol = FontArc::try_from_slice(FONT_DEJAVU_MONO).unwrap();
        }

        if font_name.to_lowercase() == DEFAULT_FONT_NAME {
            return Ok(Font {
                text: ComposedFontArc {
                    regular: FontArc::try_from_slice(FONT_CASCADIAMONO_REGULAR).unwrap(),
                    bold: FontArc::try_from_slice(FONT_CASCADIAMONO_BOLD).unwrap(),
                    italic: FontArc::try_from_slice(FONT_CASCADIAMONO_ITALIC).unwrap(),
                    bold_italic: FontArc::try_from_slice(FONT_CASCADIAMONO_BOLD_ITALIC)
                        .unwrap(),
                },
                symbol: font_arc_symbol,
                emojis: FontArc::try_from_slice(FONT_EMOJI).unwrap(),
                unicode: font_arc_unicode,
            });
        }

        let system_fonts = SystemSource::new().select_family_by_name(&font_name);
        match system_fonts {
            Ok(system_fonts) => {
                let fonts = system_fonts.fonts();
                if !fonts.is_empty() {
                    let first_font = fonts[0].load();
                    if let Ok(font) = first_font {
                        let copied_font = font.copy_font_data();
                        if copied_font.is_some() {
                            let Some(copied_font) = copied_font else { todo!() };
                            let font_vec_system =
                                FontVec::try_from_vec_and_index(copied_font.to_vec(), 0)
                                    .unwrap();

                            return Ok(Font {
                                text: ComposedFontArc {
                                    regular: FontArc::new(font_vec_system),
                                    bold: FontArc::try_from_slice(FONT_CASCADIAMONO_BOLD)
                                        .unwrap(),
                                    italic: FontArc::try_from_slice(
                                        FONT_CASCADIAMONO_ITALIC,
                                    )
                                    .unwrap(),
                                    bold_italic: FontArc::try_from_slice(
                                        FONT_CASCADIAMONO_BOLD_ITALIC,
                                    )
                                    .unwrap(),
                                },
                                symbol: font_arc_symbol,
                                emojis: FontArc::try_from_slice(FONT_EMOJI).unwrap(),
                                unicode: font_arc_unicode,
                            });
                        }
                    }
                }

                Err("failed to load font".to_string())
            }
            Err(err) => {
                warn!("failed to load font {font_name} {err:?}");
                Err(err.to_string())
            }
        }
    }
}
