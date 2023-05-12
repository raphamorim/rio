use crate::crosswords::grid::row::Row;
use crate::crosswords::pos;
use crate::crosswords::square::{Flags, Square};
use colors::{
    term::{List, TermColors},
    AnsiColor, Colors, NamedColor,
};
use config::Config;
use std::rc::Rc;
use sugarloaf::core::{Sugar, SugarStack};
use sugarloaf::Sugarloaf;

#[derive(Default)]
struct Cursor {
    position: (pos::Column, pos::Line),
    content: char,
    #[allow(dead_code)]
    hidden: bool,
}

pub struct State {
    named_colors: Colors,
    cursor: Cursor,
    colors: List,
}

// TODO: Finish from
impl From<Square> for Sugar {
    #[inline]
    fn from(square: Square) -> Sugar {
        Sugar {
            content: square.c,
            foreground_color: [0.0, 0.0, 0.0, 1.0],
            background_color: [0.0, 0.0, 0.0, 1.0],
        }
    }
}

impl State {
    pub fn new(config: &Rc<Config>) -> State {
        let term_colors = TermColors::default();
        let colors = List::from(&term_colors);

        State {
            colors,
            named_colors: config.colors,
            cursor: Cursor {
                content: config.cursor,
                position: (pos::Column(0), pos::Line(0)),
                hidden: false,
            },
        }
    }

    // TODO: Square.into()
    #[inline]
    fn create_sugar_from_square(&self, square: &Square) -> Sugar {
        let flags = square.flags;

        let foreground_color = match square.fg {
            AnsiColor::Named(NamedColor::Black) => self.named_colors.black,
            AnsiColor::Named(NamedColor::Background) => self.named_colors.background.0,
            AnsiColor::Named(NamedColor::Blue) => self.named_colors.blue,
            AnsiColor::Named(NamedColor::LightBlack) => self.named_colors.light_black,
            AnsiColor::Named(NamedColor::LightBlue) => self.named_colors.light_blue,
            AnsiColor::Named(NamedColor::LightCyan) => self.named_colors.light_cyan,
            AnsiColor::Named(NamedColor::LightForeground) => {
                self.named_colors.light_foreground
            }
            AnsiColor::Named(NamedColor::LightGreen) => self.named_colors.light_green,
            AnsiColor::Named(NamedColor::LightMagenta) => self.named_colors.light_magenta,
            AnsiColor::Named(NamedColor::LightRed) => self.named_colors.light_red,
            AnsiColor::Named(NamedColor::LightWhite) => self.named_colors.light_white,
            AnsiColor::Named(NamedColor::LightYellow) => self.named_colors.light_yellow,
            AnsiColor::Named(NamedColor::Cursor) => self.named_colors.cursor,
            AnsiColor::Named(NamedColor::Cyan) => self.named_colors.cyan,
            AnsiColor::Named(NamedColor::DimBlack) => self.named_colors.dim_black,
            AnsiColor::Named(NamedColor::DimBlue) => self.named_colors.dim_blue,
            AnsiColor::Named(NamedColor::DimCyan) => self.named_colors.dim_cyan,
            AnsiColor::Named(NamedColor::DimForeground) => {
                self.named_colors.dim_foreground
            }
            AnsiColor::Named(NamedColor::DimGreen) => self.named_colors.dim_green,
            AnsiColor::Named(NamedColor::DimMagenta) => self.named_colors.dim_magenta,
            AnsiColor::Named(NamedColor::DimRed) => self.named_colors.dim_red,
            AnsiColor::Named(NamedColor::DimWhite) => self.named_colors.dim_white,
            AnsiColor::Named(NamedColor::DimYellow) => self.named_colors.dim_yellow,
            AnsiColor::Named(NamedColor::Foreground) => self.named_colors.foreground,
            AnsiColor::Named(NamedColor::Green) => self.named_colors.green,
            AnsiColor::Named(NamedColor::Magenta) => self.named_colors.magenta,
            AnsiColor::Named(NamedColor::Red) => self.named_colors.red,
            AnsiColor::Named(NamedColor::White) => self.named_colors.white,
            AnsiColor::Named(NamedColor::Yellow) => self.named_colors.yellow,
            AnsiColor::Spec(_rgb) => self.named_colors.foreground,
            AnsiColor::Indexed(index) => {
                let index = match (flags & Flags::DIM_BOLD, index) {
                    (Flags::DIM, 8..=15) => index as usize - 8,
                    (Flags::DIM, 0..=7) => NamedColor::DimBlack as usize + index as usize,
                    _ => index as usize,
                };

                self.colors[index]
            }
        };

        let background_color = match square.bg {
            AnsiColor::Spec(_rgb) => self.named_colors.foreground,
            AnsiColor::Named(NamedColor::Black) => self.named_colors.black,
            AnsiColor::Named(NamedColor::Background) => self.named_colors.background.0,
            AnsiColor::Named(NamedColor::Blue) => self.named_colors.blue,
            AnsiColor::Named(NamedColor::LightBlack) => self.named_colors.light_black,
            AnsiColor::Named(NamedColor::LightBlue) => self.named_colors.light_blue,
            AnsiColor::Named(NamedColor::LightCyan) => self.named_colors.light_cyan,
            AnsiColor::Named(NamedColor::LightForeground) => {
                self.named_colors.light_foreground
            }
            AnsiColor::Named(NamedColor::LightGreen) => self.named_colors.light_green,
            AnsiColor::Named(NamedColor::LightMagenta) => self.named_colors.light_magenta,
            AnsiColor::Named(NamedColor::LightRed) => self.named_colors.light_red,
            AnsiColor::Named(NamedColor::LightWhite) => self.named_colors.light_white,
            AnsiColor::Named(NamedColor::LightYellow) => self.named_colors.light_yellow,
            AnsiColor::Named(NamedColor::Cursor) => self.named_colors.cursor,
            AnsiColor::Named(NamedColor::Cyan) => self.named_colors.cyan,
            AnsiColor::Named(NamedColor::DimBlack) => self.named_colors.dim_black,
            AnsiColor::Named(NamedColor::DimBlue) => self.named_colors.dim_blue,
            AnsiColor::Named(NamedColor::DimCyan) => self.named_colors.dim_cyan,
            AnsiColor::Named(NamedColor::DimForeground) => {
                self.named_colors.dim_foreground
            }
            AnsiColor::Named(NamedColor::DimGreen) => self.named_colors.dim_green,
            AnsiColor::Named(NamedColor::DimMagenta) => self.named_colors.dim_magenta,
            AnsiColor::Named(NamedColor::DimRed) => self.named_colors.dim_red,
            AnsiColor::Named(NamedColor::DimWhite) => self.named_colors.dim_white,
            AnsiColor::Named(NamedColor::DimYellow) => self.named_colors.dim_yellow,
            AnsiColor::Named(NamedColor::Foreground) => self.named_colors.foreground,
            AnsiColor::Named(NamedColor::Green) => self.named_colors.green,
            AnsiColor::Named(NamedColor::Magenta) => self.named_colors.magenta,
            AnsiColor::Named(NamedColor::Red) => self.named_colors.red,
            AnsiColor::Named(NamedColor::White) => self.named_colors.white,
            AnsiColor::Named(NamedColor::Yellow) => self.named_colors.yellow,
            AnsiColor::Indexed(idx) => self.colors[idx as usize],
        };

        Sugar {
            content: square.c,
            foreground_color,
            background_color,
        }
    }

    #[inline]
    fn create_sugar_stack(&mut self, row: &Row<Square>, has_cursor: bool) -> SugarStack {
        let mut stack: Vec<Sugar> = vec![];
        let columns: usize = row.len();
        for column in 0..columns {
            let square = &row.inner[column];

            if has_cursor && column == self.cursor.position.0 {
                stack.push(Sugar {
                    content: self.cursor.content,
                    foreground_color: self.named_colors.cursor,
                    background_color: self.named_colors.cursor,
                });
            } else {
                stack.push(self.create_sugar_from_square(square));
            }

            // Render last column and break row
            if column == (columns - 1) {
                break;
            }
        }

        stack
    }

    #[inline]
    pub fn update(
        &mut self,
        rows: Vec<Row<Square>>,
        cursor: (pos::Column, pos::Line),
        sugarloaf: &mut Sugarloaf,
        style: sugarloaf::core::SugarloafStyle,
    ) {
        self.cursor.position = cursor;
        for (i, row) in rows.iter().enumerate() {
            let sugar_stack = self.create_sugar_stack(row, self.cursor.position.1 == i);
            sugarloaf.stack(sugar_stack, style);
        }
    }

    // pub fn draw_queued(
    //     &mut self,
    //     device: &wgpu::Device,
    //     staging_belt: &mut wgpu::util::StagingBelt,
    //     encoder: &mut wgpu::CommandEncoder,
    //     view: &wgpu::TextureView,
    //     size: (u32, u32),
    // ) {
    //     let _ =
    //         self.brush
    //             .draw_queued(device, staging_belt, encoder, view, (size.0, size.1));
    // }

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
