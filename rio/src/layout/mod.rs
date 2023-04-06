use crate::crosswords::grid::Dimensions;
use crate::crosswords::{MIN_COLUMNS, MIN_VISIBLE_ROWS};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Delta<T: Default> {
    pub x: T,
    pub y: T,
}

pub struct Layout {
    scale_factor: f32,
    width: f32,
    height: f32,
    pub columns: usize,
    pub rows: usize,
    padding: Delta<u8>,
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

impl Layout {
    pub fn new(width: f32, height: f32, scale_factor: f32) -> Layout {
        Layout {
            width,
            height,
            columns: 80,
            rows: 25,
            scale_factor,
            padding: Delta::<u8>::default(),
        }
    }

    #[inline]
    fn padding(&self) -> (f32, f32) {
        let padding_x = (f32::from(self.padding.x) * self.scale_factor).floor();
        let padding_y = (f32::from(self.padding.y) * self.scale_factor).floor();
        (padding_x, padding_y)
    }

    pub fn set_scale_factor(&mut self, scale_factor: f32) {
        self.scale_factor = scale_factor;
    }

    pub fn set_size(&mut self, width: f32, height: f32) {
        self.width = width;
        self.height = height;
    }

    // $ tput columns
    // $ tput lines

    pub fn compute(&mut self) -> (usize, usize) {
        let (padding_x, padding_y) = self.padding();
        // let a_lines = (height - 2. * padding_y) / scale;
        let mut a_lines = (self.height - 2. * padding_y) / self.scale_factor;
        a_lines = a_lines / 17.5;
        let a_screen_lines = std::cmp::max(a_lines as usize, MIN_VISIBLE_ROWS);

        let mut a_columns = (self.width - 2. * padding_x) / self.scale_factor;
        a_columns = a_columns / 8.;
        let a_columns = std::cmp::max(a_columns as usize, MIN_COLUMNS);

        println!("compute: {:?} {:?}", a_columns, a_screen_lines);

        self.columns = a_columns;
        self.rows = a_screen_lines;

        (a_columns, a_screen_lines)
    }
}
