use glyph_brush::ab_glyph::FontArc;

pub mod text;
mod shared;

// load_font (Path)
// render_row (Row<Square>)
// render_string (String)

pub struct Renderer {
    pub brush: text::GlyphBrush<()>
}


impl Renderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Result<Renderer, String> {
        //     FontArc::try_from_slice(shared::FONT_NOVAMONO)?

        match FontArc::try_from_slice(shared::FONT_FIRAMONO) {
            Ok(font_data) => {
                let brush = text::GlyphBrushBuilder::using_font(font_data).build(&device, format);
                Ok(
                    Renderer {
                        brush,
                    }
                )
            },
            Err(err_message) => {
                Err(format!("Renderer error: could not initialize font {:?}", err_message))
            }
        }
    }

    // pub fn load_font(&self, font_path: String) {
    //     self.font = ab_glyph::FontArc::try_from_slice(font_path)?;
    // }

    // pub fn set_font(font_path: String) -> RetType {
    //     let font = match config.style.font {
    //         config::Font::Firamono => {
    //             ab_glyph::FontArc::try_from_slice(font_path)?
    //         }
    //         config::Font::Novamono => {
    //             ab_glyph::FontArc::try_from_slice(font_path)?
    //         }
    //     };

    //     let text_brush = GlyphBrushBuilder::using_font(font).build(&device, format);

    //     let cache = Cache::new(&device, 1024, 1024);
    // }
}