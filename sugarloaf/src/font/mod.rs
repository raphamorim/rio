use font_kit::source::SystemSource;
use glyph_brush::ab_glyph::{FontArc, FontVec};
use log::{info, warn};

pub const FONT_FIRAMONO: &[u8; 170204] =
    include_bytes!("./resources/FiraMono/FiraMono-Regular.ttf");

pub struct Font {
    pub arc: FontArc,
}

impl Font {
    pub fn new(font_name: String) -> Result<Font, String> {
        // let font = SystemSource::new()
        //     .select_by_postscript_name("ArialMT")
        //     .unwrap()
        //     .load()
        //     .unwrap();

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
                            let copy =
                                FontVec::try_from_vec_and_index(copied_font.to_vec(), 0);

                            return Ok(Font {
                                arc: FontArc::new(copy.unwrap()),
                            });
                        }
                    }
                }
            }
            Err(err) => {
                warn!("failed to load font {font_name} {err:?}")
            }
        };

        info!("failing back to default font");
        match FontArc::try_from_slice(FONT_FIRAMONO) {
            Ok(arc) => Ok(Font { arc }),
            Err(err_message) => Err(err_message.to_string()),
        }
    }
}
