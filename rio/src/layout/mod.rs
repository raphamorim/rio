mod mouse;

use crate::crosswords::grid::Dimensions;
use crate::crosswords::{MIN_COLUMNS, MIN_VISIBLE_ROWS};
use mouse::{AccumulatedScroll, Mouse};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Delta<T: Default> {
    pub x: T,
    pub y: T,
}

const PADDING_X: f32 = 10.0;
const PADDING_Y: f32 = 30.0;

pub struct Layout {
    scale_factor: f32,
    pub width: f32,
    pub height: f32,
    pub width_u32: u32,
    pub height_u32: u32,
    pub font_size: f32,
    pub mouse: Mouse,
    pub columns: usize,
    pub rows: usize,
    padding: Delta<f32>,
    pub styles: LayoutStyles,
}

#[derive(Default, Copy, Clone)]
pub struct Style {
    pub screen_position: (f32, f32),
    pub bounds: (f32, f32),
    pub text_scale: f32,
}

#[derive(Default)]
pub struct LayoutStyles {
    pub term: Style,
    pub tabs_active: Style,
}

impl Dimensions for Layout {
    #[inline]
    fn columns(&self) -> usize {
        self.columns
    }

    #[inline]
    fn screen_lines(&self) -> usize {
        self.rows
    }

    #[inline]
    fn total_lines(&self) -> usize {
        self.screen_lines()
    }
}

fn update_styles(layout: &mut Layout) {
    let new_styles = LayoutStyles {
        term: Style {
            screen_position: (
                layout.padding.x * layout.scale_factor,
                (layout.padding.y * layout.scale_factor),
            ),
            bounds: (
                layout.width * layout.scale_factor,
                layout.height * layout.scale_factor,
            ),
            text_scale: layout.font_size * layout.scale_factor,
        },
        // TODO: Fix tabs style
        tabs_active: Style {
            screen_position: (80.0 * layout.scale_factor, (8.0 * layout.scale_factor)),
            bounds: (
                layout.width - (40.0 * layout.scale_factor),
                layout.height * layout.scale_factor,
            ),
            text_scale: 15.0 * layout.scale_factor,
        },
    };
    layout.styles = new_styles;
}

impl Layout {
    pub fn new(width: f32, height: f32, scale_factor: f32, font_size: f32) -> Layout {
        let styles = LayoutStyles::default();

        let mut layout = Layout {
            width,
            width_u32: width as u32,
            height,
            height_u32: height as u32,
            columns: 80,
            rows: 25,
            scale_factor,
            font_size,
            mouse: Mouse {
                multiplier: 3.0,
                ..Mouse::default()
            },
            styles,
            padding: Delta {
                x: PADDING_X,
                y: PADDING_Y,
            },
        };

        update_styles(&mut layout);
        layout
    }

    #[inline]
    fn padding(&self) -> (f32, f32) {
        let padding_x = ((self.padding.x) * self.scale_factor).floor();
        let padding_y = ((self.padding.y) * self.scale_factor).floor();
        (padding_x, padding_y)
    }

    pub fn set_scale(&mut self, scale_factor: f32) -> &mut Self {
        self.scale_factor = scale_factor;
        self
    }

    pub fn set_size(&mut self, width: u32, height: u32) -> &mut Self {
        self.width_u32 = width;
        self.height_u32 = height;
        self.width = width as f32;
        self.height = height as f32;
        self
    }

    pub fn update(&mut self) -> &mut Self {
        update_styles(self);
        self
    }

    pub fn reset_mouse(&mut self) {
        self.mouse.accumulated_scroll = AccumulatedScroll::default();
    }

    pub fn mouse_mut(&mut self) -> &mut Mouse {
        &mut self.mouse
    }

    // $ tput columns
    // $ tput lines
    pub fn compute(&mut self) -> (usize, usize) {
        let (padding_x, padding_y) = self.padding();
        let mut rows = (self.height - padding_y) / self.scale_factor;
        rows /= self.font_size;
        let visible_rows = std::cmp::max(rows as usize, MIN_VISIBLE_ROWS);

        let mut visible_columns = (self.width - 2. * padding_x) / self.scale_factor;
        visible_columns /= self.font_size / 2.;
        let visible_columns = std::cmp::max(visible_columns as usize, MIN_COLUMNS);

        self.columns = visible_columns;
        self.rows = visible_rows;

        (visible_columns, visible_rows)
    }
}
