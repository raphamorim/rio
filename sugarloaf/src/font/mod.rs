use font_kit::source::SystemSource;
use glyph_brush::ab_glyph::{FontArc, FontVec};
use log::warn;

pub const DEFAULT_FONT_NAME: &str = "cascadiamono";

pub const FONT_CASCADIA_MONO: &[u8; 624892] =
    include_bytes!("./resources/CascadiaMono.ttf");

pub const FONT_EMOJI: &[u8; 877988] =
    include_bytes!("./resources/NotoEmoji/static/NotoEmoji-Regular.ttf");

pub struct Font {
    pub system: FontArc,
    pub symbol: FontArc,
    pub emojis: FontArc,
    pub unicode: FontArc,
}

// TODO:
// This code is quite unsafe and needs a proper refactor
// adding font load fallbacks for all categories.
//
// It will also only work on MacOS for now.

impl Font {
    // TODO: if cfg!(target_os = "macos") {
    pub fn new(font_name: String) -> Result<Font, String> {
        let font_symbols = SystemSource::new()
            .select_by_postscript_name("Apple Symbols")
            .unwrap()
            .load()
            .unwrap();
        let copied_font_symbol = font_symbols.copy_font_data();
        let Some(copied_font_symbol) = copied_font_symbol else { todo!() };
        let font_vec_symbol =
            FontVec::try_from_vec_and_index(copied_font_symbol.to_vec(), 1).unwrap();

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

        if font_name.to_lowercase() == DEFAULT_FONT_NAME {
            return Ok(Font {
                system: FontArc::try_from_slice(FONT_CASCADIA_MONO).unwrap(),
                symbol: FontArc::new(font_vec_symbol),
                emojis: FontArc::try_from_slice(FONT_EMOJI).unwrap(),
                unicode: FontArc::new(font_vec_unicode),
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
                                system: FontArc::new(font_vec_system),
                                symbol: FontArc::new(font_vec_symbol),
                                emojis: FontArc::try_from_slice(FONT_EMOJI).unwrap(),
                                unicode: FontArc::new(font_vec_unicode),
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
