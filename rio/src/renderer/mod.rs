mod frames;
mod shared;
mod text;

use std::rc::Rc;
use colors::{AnsiColor, NamedColor};
use config::Config;
use crate::crosswords::{row::Row, square::Square};
use glyph_brush::ab_glyph::FontArc;
use glyph_brush::{OwnedSection, OwnedText, Section, Text};

pub struct Style {
    pub screen_position: (f32, f32),
    pub bounds: (f32, f32),
    pub text_scale: f32,
}

pub struct RendererStyles {
    pub scale: f32,
    pub font_size: f32,
    pub term: Style,
    pub tabs_active: Style,
    pub width: u32,
    pub height: u32,
}

impl RendererStyles {
    pub fn new(scale: f32, width: u32, height: u32, font_size: f32) -> RendererStyles {
        Self::mount_styles(scale, width, height, font_size)
    }

    #[inline]
    fn mount_styles(
        scale: f32,
        width: u32,
        height: u32,
        font_size: f32,
    ) -> RendererStyles {
        let yspacing = 30.0;
        let width_f32 = width as f32;
        let height_f32 = height as f32;
        RendererStyles {
            height,
            width,
            scale,
            font_size,
            term: Style {
                screen_position: (10.0 * scale, (yspacing * scale)),
                bounds: (width_f32 - ((font_size + 5.0) * scale), height_f32 * scale),
                text_scale: font_size * scale,
            },
            tabs_active: Style {
                screen_position: (80.0 * scale, (8.0 * scale)),
                bounds: (width_f32 - (40.0 * scale), height_f32 * scale),
                text_scale: 15.0 * scale,
            },
        }
    }

    pub fn refresh_styles(&mut self, width: u32, height: u32, scale: f32) {
        *self = Self::mount_styles(scale, width, height, self.font_size);
    }
}

pub struct Renderer {
    pub brush: text::GlyphBrush<()>,
    pub config: Rc<Config>,
    styles: RendererStyles,
    /// This field is used if monochrome is true, so skip color processing per squares
    fps: frames::Counter,
}

impl Renderer {
    pub fn new(
        device: wgpu::Device,
        format: wgpu::TextureFormat,
        config: &Rc<Config>,
        styles: RendererStyles,
    ) -> Result<Renderer, String> {
        match FontArc::try_from_slice(shared::FONT_FIRAMONO) {
            Ok(font_data) => {
                let brush =
                    text::GlyphBrushBuilder::using_font(font_data).build(&device, format);
                let fps = frames::Counter::new();
                Ok(Renderer {
                    brush,
                    config: config.clone(),
                    styles,
                    fps,
                })
            }
            Err(err_message) => Err(format!(
                "Renderer error: could not initialize font {err_message:?}"
            )),
        }
    }

    pub fn refresh_styles(&mut self, width: u32, height: u32, scale: f32) {
        self.styles.refresh_styles(width, height, scale);
    }

    pub fn get_current_scale(&self) -> f32 {
        self.styles.scale
    }

    #[inline]
    fn process_row(&self, square: &Square) -> OwnedText {
        let content: String = square.c.to_string();

        // println!("{:?}", square.fg);

        let fg_color = match square.fg {
            AnsiColor::Named(NamedColor::Black) => self.config.colors.black,
            AnsiColor::Named(NamedColor::Background) => self.config.colors.background.0,
            AnsiColor::Named(NamedColor::Blue) => self.config.colors.blue,
            AnsiColor::Named(NamedColor::LightBlack) => self.config.colors.light_black,
            AnsiColor::Named(NamedColor::LightBlue) => self.config.colors.light_blue,
            AnsiColor::Named(NamedColor::LightCyan) => self.config.colors.light_cyan,
            AnsiColor::Named(NamedColor::LightForeground) => {
                self.config.colors.light_foreground
            }
            AnsiColor::Named(NamedColor::LightGreen) => self.config.colors.light_green,
            AnsiColor::Named(NamedColor::LightMagenta) => {
                self.config.colors.light_magenta
            }
            AnsiColor::Named(NamedColor::LightRed) => self.config.colors.light_red,
            AnsiColor::Named(NamedColor::LightWhite) => self.config.colors.light_white,
            AnsiColor::Named(NamedColor::LightYellow) => self.config.colors.light_yellow,
            AnsiColor::Named(NamedColor::Cursor) => self.config.colors.cursor,
            AnsiColor::Named(NamedColor::Cyan) => self.config.colors.cyan,
            AnsiColor::Named(NamedColor::DimBlack) => self.config.colors.dim_black,
            AnsiColor::Named(NamedColor::DimBlue) => self.config.colors.dim_blue,
            AnsiColor::Named(NamedColor::DimCyan) => self.config.colors.dim_cyan,
            AnsiColor::Named(NamedColor::DimForeground) => {
                self.config.colors.dim_foreground
            }
            AnsiColor::Named(NamedColor::DimGreen) => self.config.colors.dim_green,
            AnsiColor::Named(NamedColor::DimMagenta) => self.config.colors.dim_magenta,
            AnsiColor::Named(NamedColor::DimRed) => self.config.colors.dim_red,
            AnsiColor::Named(NamedColor::DimWhite) => self.config.colors.dim_white,
            AnsiColor::Named(NamedColor::DimYellow) => self.config.colors.dim_yellow,
            AnsiColor::Named(NamedColor::Foreground) => self.config.colors.foreground,
            AnsiColor::Named(NamedColor::Green) => self.config.colors.green,
            AnsiColor::Named(NamedColor::Magenta) => self.config.colors.magenta,
            AnsiColor::Named(NamedColor::Red) => self.config.colors.red,
            AnsiColor::Named(NamedColor::White) => self.config.colors.white,
            AnsiColor::Named(NamedColor::Yellow) => self.config.colors.yellow,
        };

        OwnedText::new(content)
            .with_color(fg_color)
            .with_scale(self.styles.term.text_scale)
    }

    pub fn term(&mut self, rows: Vec<Row<Square>>) {
        let mut line_height: f32 = 1.0;
        for row in rows {
            let mut row_text: Vec<OwnedText> = vec![];
            let columns: usize = row.len();
            for column in 0..columns {
                let square = &row.inner[column];
                let text = self.process_row(square);
                row_text.push(text);
                // for c in square.zerowidth().into_iter().flatten() {
                //     text.push(*c);
                // }

                // Render last column and break row
                if column == (columns - 1) {
                    self.brush.queue(&OwnedSection {
                        screen_position: (
                            self.styles.term.screen_position.0,
                            self.styles.term.screen_position.1 + line_height,
                        ),
                        bounds: self.styles.term.bounds,
                        text: row_text,
                        layout: glyph_brush::Layout::default_single_line(),
                    });

                    line_height += self.styles.term.text_scale;
                    row_text = vec![];
                }
            }
        }
    }

    pub fn draw_queued(&mut self, device: &wgpu::Device, staging_belt: &mut wgpu::util::StagingBelt, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        self.brush
            .draw_queued(
                &device,
                staging_belt,
                encoder,
                view,
                (self.styles.width, self.styles.height),
            );
    }

    pub fn topbar(&mut self, command: String) {
        let fps_text = if self.config.advanced.enable_fps_counter {
            format!(" fps_{:?}", self.fps.tick())
        } else {
            String::from("")
        };

        self.brush.queue(Section {
            screen_position: self.styles.tabs_active.screen_position,
            bounds: self.styles.tabs_active.bounds,
            text: vec![
                Text::new(&command)
                    .with_color(self.config.colors.tabs_active)
                    .with_scale(self.styles.tabs_active.text_scale),
                Text::new("■ vim ■ zsh ■ docker")
                    .with_color([0.89020, 0.54118, 0.33725, 1.0])
                    .with_scale(self.styles.tabs_active.text_scale),
                Text::new(&fps_text)
                    .with_color(self.config.colors.foreground)
                    .with_scale(self.styles.tabs_active.text_scale),
            ],
            layout: glyph_brush::Layout::default_single_line(),
            // ..Section::default() // .line_breaker(glyph_brush::BuiltInLineBreaker::UNi)
            // .v_align(glyph_brush::VerticalAlign::Center)
            // .h_align(glyph_brush::HorizontalAlign::Left)
        });

        // self.brush.queue(Section {
        //     screen_position: ((self.size.width as f32 - 20.0) * scale, (8.0 * scale)),
        //     bounds: (
        //         (self.size.width as f32) - (40.0 * scale),
        //         (self.size.height as f32) * scale,
        //     ),
        //     text: vec![Text::new("■ vim ■ zsh ■ docker")
        //         //(157,165,237)
        //         .with_color([0.89020, 0.54118, 0.33725, 1.0])
        //         .with_scale(14.0 * scale)],
        //     layout: glyph_brush::Layout::default()
        //         // .line_breaker(glyph_brush::BuiltInLineBreaker::UNi)
        //         // .v_align(glyph_brush::VerticalAlign::Center)
        //         .h_align(glyph_brush::HorizontalAlign::Right),
        //     ..Section::default()
        // });
    }
}
