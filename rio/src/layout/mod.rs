use crate::crosswords::{MIN_COLUMNS, MIN_VISIBLE_ROWS};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Delta<T: Default> {
    pub x: T,
    pub y: T,
}

pub struct Layout {
    scale_factor: f32,
    width: u16,
    height: u16,
    padding: Delta<u8>,
}

impl Layout {
    pub fn new(width: u16, height: u16, scale_factor: f32) -> Layout {
        Layout {
            width,
            height,
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

    pub fn set_size(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
    }

    // $ tput columns
    // $ tput lines
    pub fn compute(&self, width: f32, height: f32) -> (usize, usize) {
        let (padding_x, padding_y) = self.padding();
        // let a_lines = (height - 2. * padding_y) / scale;
        let mut a_lines = (height - 2. * padding_y) / self.scale_factor;
        a_lines = a_lines / 22.;
        let a_screen_lines = std::cmp::max(a_lines as usize, MIN_VISIBLE_ROWS);

        let mut a_columns = (width - 2. * padding_x) / self.scale_factor;
        a_columns = a_columns / 10.;
        let a_columns = std::cmp::max(a_columns as usize, MIN_COLUMNS);

        println!("compute: {:?} {:?}", a_columns, a_screen_lines);

        (a_columns, a_screen_lines)
    }
}
