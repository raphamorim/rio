// mod frames;
mod shared;
mod text;

use crate::crosswords::grid::row::Row;
use crate::crosswords::pos;
use crate::crosswords::square::{Flags, Square};
use crate::layout::Style;
use colors::{
    term::{List, TermColors},
    AnsiColor, NamedColor,
};
use config::Config;
use glyph_brush::ab_glyph::FontArc;
use glyph_brush::{OwnedSection, OwnedText};
use std::rc::Rc;

#[derive(Default)]
struct Cursor {
    position: (pos::Column, pos::Line),
    content: char,
}

pub struct Renderer {
    pub brush: text::GlyphBrush<()>,
    pub config: Rc<Config>,
    cursor: Cursor,
    colors: List,
    // fps: frames::Counter,
}

impl Renderer {
    pub fn new(
        device: wgpu::Device,
        format: wgpu::TextureFormat,
        config: &Rc<Config>,
    ) -> Result<Renderer, String> {
        match FontArc::try_from_slice(shared::FONT_FIRAMONO) {
            Ok(font_data) => {
                let brush =
                    text::GlyphBrushBuilder::using_font(font_data).build(&device, format);
                // let fps = frames::Counter::new();
                let term_colors = TermColors::default();
                let colors = List::from(&term_colors);
                Ok(Renderer {
                    colors,
                    cursor: Cursor {
                        content: config.cursor,
                        position: (pos::Column(0), pos::Line(0)),
                    },
                    brush,
                    config: config.clone(),
                    // fps,
                })
            }
            Err(err_message) => Err(format!(
                "Renderer error: could not initialize font {err_message:?}"
            )),
        }
    }

    #[inline]
    fn process_row(&self, square: &Square, style: Style) -> OwnedText {
        let content: String = square.c.to_string();
        let flags = square.flags;

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
            AnsiColor::Spec(_rgb) => self.config.colors.foreground,
            AnsiColor::Indexed(index) => {
                let index = match (flags & Flags::DIM_BOLD, index) {
                    (Flags::DIM, 8..=15) => index as usize - 8,
                    (Flags::DIM, 0..=7) => NamedColor::DimBlack as usize + index as usize,
                    _ => index as usize,
                };

                self.colors[index]
            }
        };

        #[allow(unused)]
        let bg = match square.bg {
            AnsiColor::Spec(_rgb) => self.config.colors.foreground,
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
            AnsiColor::Indexed(idx) => self.colors[idx as usize],
        };

        // println!("{:?}", bg);

        OwnedText::new(content)
            .with_color(fg_color)
            .with_scale(style.text_scale)
    }

    pub fn set_cursor(&mut self, cursor: (pos::Column, pos::Line)) -> &mut Self {
        self.cursor.position = cursor;
        self
    }

    #[inline]
    fn render_row(
        &mut self,
        row: &Row<Square>,
        style: Style,
        line_height: f32,
        has_cursor: bool,
    ) {
        let mut row_text: Vec<OwnedText> = vec![];
        let columns: usize = row.len();
        for column in 0..columns {
            let square = &row.inner[column];
            let text = self.process_row(square, style);

            if has_cursor && column == self.cursor.position.0 {
                row_text.push(
                    OwnedText::new(self.cursor.content)
                        .with_color(self.config.colors.cursor)
                        .with_scale(style.text_scale),
                )
            } else {
                row_text.push(text);
            }

            // Render last column and break row
            if column == (columns - 1) {
                self.brush.queue(&OwnedSection {
                    screen_position: (
                        style.screen_position.0,
                        style.screen_position.1 + line_height,
                    ),
                    bounds: style.bounds,
                    text: row_text,
                    layout: glyph_brush::Layout::default_single_line()
                        .v_align(glyph_brush::VerticalAlign::Bottom),
                });

                break;
            }
        }
    }

    #[inline]
    pub fn term(&mut self, rows: Vec<Row<Square>>, style: Style) {
        let mut line_height: f32 = 0.0;
        let cursor_row = self.cursor.position.1;
        for (i, row) in rows.iter().enumerate() {
            self.render_row(row, style, line_height, cursor_row == i);
            line_height += style.text_scale;
        }
    }

    pub fn draw_queued(
        &mut self,
        device: &wgpu::Device,
        staging_belt: &mut wgpu::util::StagingBelt,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        size: (u32, u32),
    ) {
        let _ =
            self.brush
                .draw_queued(device, staging_belt, encoder, view, (size.0, size.1));
    }

    // pub fn topbar(&mut self, command: String) {
    //     let fps_text = if self.config.developer.enable_fps_counter {
    //         format!(" fps_{:?}", self.fps.tick())
    //     } else {
    //         String::from("")
    //     };

    //     self.brush.queue(Section {
    //         screen_position: self.styles.tabs_active.screen_position,
    //         bounds: self.styles.tabs_active.bounds,
    //         text: vec![
    //             Text::new(&command)
    //                 .with_color(self.config.colors.tabs_active)
    //                 .with_scale(self.styles.tabs_active.text_scale),
    //             Text::new("■ vim ■ zsh ■ docker")
    //                 .with_color([0.89020, 0.54118, 0.33725, 1.0])
    //                 .with_scale(self.styles.tabs_active.text_scale),
    //             Text::new(&fps_text)
    //                 .with_color(self.config.colors.foreground)
    //                 .with_scale(self.styles.tabs_active.text_scale),
    //         ],
    //         layout: glyph_brush::Layout::default_single_line(),
    //         // ..Section::default() // .line_breaker(glyph_brush::BuiltInLineBreaker::UNi)
    //         // .v_align(glyph_brush::VerticalAlign::Center)
    //         // .h_align(glyph_brush::HorizontalAlign::Left)
    //     });

    //     // self.brush.queue(Section {
    //     //     screen_position: ((self.size.width as f32 - 20.0) * scale, (8.0 * scale)),
    //     //     bounds: (
    //     //         (self.size.width as f32) - (40.0 * scale),
    //     //         (self.size.height as f32) * scale,
    //     //     ),
    //     //     text: vec![Text::new("■ vim ■ zsh ■ docker")
    //     //         //(157,165,237)
    //     //         .with_color([0.89020, 0.54118, 0.33725, 1.0])
    //     //         .with_scale(14.0 * scale)],
    //     //     layout: glyph_brush::Layout::default()
    //     //         // .line_breaker(glyph_brush::BuiltInLineBreaker::UNi)
    //     //         // .v_align(glyph_brush::VerticalAlign::Center)
    //     //         .h_align(glyph_brush::HorizontalAlign::Right),
    //     //     ..Section::default()
    //     // });
    // }
}
