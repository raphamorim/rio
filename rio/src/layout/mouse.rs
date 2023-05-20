use crate::crosswords::pos::{Column, Line, Pos, Side};
use crate::event::ClickState;
use std::cmp::min;
use winit::event::ElementState;
use winit::event::MouseButton;

use std::time::Instant;

const PADDING_X: f32 = 10.0;
const PADDING_Y: f32 = 50.0;

#[derive(Default, Debug)]
pub struct AccumulatedScroll {
    /// Scroll we should perform along `x` axis.
    pub x: f64,

    /// Scroll we should perform along `y` axis.
    pub y: f64,
}

#[derive(Debug)]
pub struct Mouse {
    pub multiplier: f64,
    pub left_button_state: ElementState,
    pub middle_button_state: ElementState,
    pub right_button_state: ElementState,
    pub last_click_timestamp: Instant,
    pub last_click_button: MouseButton,
    pub click_state: ClickState,
    pub accumulated_scroll: AccumulatedScroll,
    pub square_side: Side,
    pub lines_scrolled: f32,
    pub inside_text_area: bool,
    pub x: usize,
    pub y: usize,
}

impl Default for Mouse {
    fn default() -> Mouse {
        Mouse {
            multiplier: 3.0,
            last_click_timestamp: Instant::now(),
            last_click_button: MouseButton::Left,
            left_button_state: ElementState::Released,
            middle_button_state: ElementState::Released,
            right_button_state: ElementState::Released,
            click_state: ClickState::None,
            square_side: Side::Left,
            inside_text_area: Default::default(),
            lines_scrolled: Default::default(),
            accumulated_scroll: AccumulatedScroll::default(),
            x: Default::default(),
            y: Default::default(),
        }
    }
}

impl Mouse {
    fn viewport_to_point(&self, display_offset: usize, point: Pos<usize>) -> Pos {
        let row = Line(point.row as i32) - display_offset;
        Pos::new(row, point.col)
    }

    /// Convert mouse pixel coordinates to viewport point.
    ///
    /// If the coordinates are outside of the terminal grid, like positions inside the padding, the
    /// coordinates will be clamped to the closest grid coordinates.
    #[inline]
    pub fn position(&self, display_offset: usize) -> Pos {
        let text_scale = 16 * 2;
        let col = self.x.saturating_sub(PADDING_X as usize) / 16;
        // let col = min(Column(col), size.last_column());
        let col = min(Column(col), Column(80));

        // let line = self.y.saturating_sub(PADDING_Y as usize) / (16.0 as usize);
        let line = self.y.saturating_sub(PADDING_Y as usize) / (text_scale + 2);
        // let line = min(line, size.bottommost_line().0 as usize);
        let line = min(line, 25 as usize);

        // println!("{:?}{:?}{:?}", self.x, col, line);

        self.viewport_to_point(display_offset, Pos::new(line, col))
    }
}
