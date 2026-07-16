use librio_vt::{AnsiColor, NamedColor, RenderState, StyleFlags};
use sugarloaf::font::FontLibrary;
use sugarloaf::layout::RootStyle;
use sugarloaf::text::DrawOpts;
use sugarloaf::{Sugarloaf, SugarloafRenderer, SugarloafWindow};

pub use sugarloaf::{SugarloafWindow as Window, SugarloafWindowSize as WindowSize};

#[derive(Debug, Clone)]
pub struct Theme {
    pub foreground: [f32; 4],
    pub background: [f32; 4],
    pub cursor: [f32; 4],
    pub ansi: [[f32; 4]; 16],
}

impl Default for Theme {
    fn default() -> Self {
        fn c(hex: u32) -> [f32; 4] {
            [
                ((hex >> 16) & 0xff) as f32 / 255.0,
                ((hex >> 8) & 0xff) as f32 / 255.0,
                (hex & 0xff) as f32 / 255.0,
                1.0,
            ]
        }
        Self {
            foreground: c(0xf8f8f2),
            background: c(0x0f0f10),
            cursor: [1.0, 1.0, 1.0, 0.45],
            ansi: [
                c(0x21222c),
                c(0xff5555),
                c(0x50fa7b),
                c(0xf1fa8c),
                c(0xbd93f9),
                c(0xff79c6),
                c(0x8be9fd),
                c(0xf8f8f2),
                c(0x6272a4),
                c(0xff6e6e),
                c(0x69ff94),
                c(0xffffa5),
                c(0xd6acff),
                c(0xff92df),
                c(0xa4ffff),
                c(0xffffff),
            ],
        }
    }
}

impl Theme {
    fn resolve(&self, color: AnsiColor) -> [f32; 4] {
        match color {
            AnsiColor::Spec(rgb) => [
                rgb.r as f32 / 255.0,
                rgb.g as f32 / 255.0,
                rgb.b as f32 / 255.0,
                1.0,
            ],
            AnsiColor::Indexed(index) => self.indexed(index),
            AnsiColor::Named(named) => match named {
                NamedColor::Background => self.background,
                NamedColor::Foreground => self.foreground,
                NamedColor::Cursor => self.cursor,
                other => {
                    let index = other as usize;
                    if index < 16 {
                        self.ansi[index]
                    } else {
                        self.foreground
                    }
                }
            },
        }
    }

    fn indexed(&self, index: u8) -> [f32; 4] {
        match index {
            0..=15 => self.ansi[index as usize],
            16..=231 => {
                let value = index as usize - 16;
                let steps = [0.0, 95.0, 135.0, 175.0, 215.0, 255.0];
                let r = steps[value / 36];
                let g = steps[(value % 36) / 6];
                let b = steps[value % 6];
                [r / 255.0, g / 255.0, b / 255.0, 1.0]
            }
            _ => {
                let level = (8 + (index as usize - 232) * 10) as f32 / 255.0;
                [level, level, level, 1.0]
            }
        }
    }
}

fn to_u8(color: [f32; 4]) -> [u8; 4] {
    [
        (color[0] * 255.0) as u8,
        (color[1] * 255.0) as u8,
        (color[2] * 255.0) as u8,
        (color[3] * 255.0) as u8,
    ]
}

pub struct Renderer {
    sugarloaf: Sugarloaf<'static>,
    theme: Theme,
    font_size: f32,
    line_height: f32,
    cell_width: f32,
    cell_height: f32,
    padding: f32,
}

impl Renderer {
    pub fn new(
        window: SugarloafWindow,
        font_size: f32,
        theme: Theme,
    ) -> Result<Self, String> {
        let scale = window.scale;
        let font_library = FontLibrary::default();
        let line_height = 1.0;
        let sugarloaf = match Sugarloaf::new(
            window,
            SugarloafRenderer::default(),
            &font_library,
            RootStyle::new(scale, font_size, line_height),
        ) {
            Ok(instance) => instance,
            Err(with_errors) => with_errors.instance,
        };

        let mut renderer = Self {
            sugarloaf,
            theme,
            font_size,
            line_height,
            cell_width: 0.0,
            cell_height: 0.0,
            padding: 4.0,
        };
        renderer.refresh_cell_metrics();
        Ok(renderer)
    }

    fn refresh_cell_metrics(&mut self) {
        let scale = self.sugarloaf.get_scale();
        let (dimensions, _metrics) =
            self.sugarloaf
                .compute_cell_metrics(self.font_size, self.line_height, scale);
        self.cell_width = dimensions.width / scale;
        self.cell_height = dimensions.height / scale;
    }

    pub fn cell_size(&self) -> (f32, f32) {
        (self.cell_width, self.cell_height)
    }

    pub fn padding(&self) -> f32 {
        self.padding
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.sugarloaf.resize(width, height);
    }

    pub fn rescale(&mut self, scale: f32) {
        self.sugarloaf.rescale(scale);
        self.refresh_cell_metrics();
    }

    pub fn draw(&mut self, state: &RenderState) {
        let columns = state.columns();
        let lines = state.lines();
        let (cell_w, cell_h) = (self.cell_width, self.cell_height);
        let pad = self.padding;

        let size = self.sugarloaf.window_size();
        let scale = self.sugarloaf.get_scale();
        self.sugarloaf.rect(
            None,
            0.0,
            0.0,
            size.width / scale,
            size.height / scale,
            self.theme.background,
            0.0,
            0,
        );

        let mut run = String::new();
        for line in 0..lines {
            let y = pad + line as f32 * cell_h;
            let mut column = 0;
            while column < columns {
                let Some(square) = state.square(line, column) else {
                    break;
                };
                let style = state.style_of(square);
                let run_start = column;
                run.clear();
                while let Some(square) = state.square(line, column) {
                    if state.style_of(square) != style {
                        break;
                    }
                    let ch = square.c();
                    run.push(if ch == '\0' { ' ' } else { ch });
                    column += 1;
                }

                let inverse = style.flags.contains(StyleFlags::INVERSE);
                let mut fg = self.theme.resolve(style.fg);
                let mut bg = self.theme.resolve(style.bg);
                if inverse {
                    std::mem::swap(&mut fg, &mut bg);
                }

                let x = pad + run_start as f32 * cell_w;
                if bg != self.theme.background {
                    self.sugarloaf.rect(
                        None,
                        x,
                        y,
                        run.chars().count() as f32 * cell_w,
                        cell_h,
                        bg,
                        0.0,
                        0,
                    );
                }
                if !run.trim().is_empty() {
                    self.sugarloaf.text_mut().draw(
                        x,
                        y,
                        &run,
                        &DrawOpts {
                            font_size: self.font_size,
                            color: to_u8(fg),
                            bold: style.flags.contains(StyleFlags::BOLD),
                            italic: style.flags.contains(StyleFlags::ITALIC),
                            font_id: None,
                        },
                    );
                }
            }
        }

        if state.display_offset() == 0 {
            let (cursor_line, cursor_column) = state.cursor();
            self.sugarloaf.rect(
                None,
                pad + cursor_column as f32 * cell_w,
                pad + cursor_line as f32 * cell_h,
                cell_w,
                cell_h,
                self.theme.cursor,
                0.0,
                0,
            );
        }

        self.sugarloaf.render();
    }
}
