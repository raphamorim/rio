use crate::crosswords::pos::{Line, Side, Column, Pos};
use crate::event::ClickState;
use std::cmp::min;
use winit::event::ElementState;
use winit::event::MouseButton;

use std::time::Instant;

#[derive(Debug)]
pub struct Mouse {
    pub left_button_state: ElementState,
    pub middle_button_state: ElementState,
    pub right_button_state: ElementState,
    pub last_click_timestamp: Instant,
    pub last_click_button: MouseButton,
    pub click_state: ClickState,
    // pub accumulated_scroll: AccumulatedScroll,
    pub square_side: Side,
    pub lines_scrolled: f32,
    pub block_hint_launcher: bool,
    pub hint_highlight_dirty: bool,
    pub inside_text_area: bool,
    pub x: usize,
    pub y: usize,
}

impl Default for Mouse {
    fn default() -> Mouse {
        Mouse {
            last_click_timestamp: Instant::now(),
            last_click_button: MouseButton::Left,
            left_button_state: ElementState::Released,
            middle_button_state: ElementState::Released,
            right_button_state: ElementState::Released,
            click_state: ClickState::None,
            square_side: Side::Left,
            hint_highlight_dirty: Default::default(),
            block_hint_launcher: Default::default(),
            inside_text_area: Default::default(),
            lines_scrolled: Default::default(),
            // accumulated_scroll: Default::default(),
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
        // let col = self.x.saturating_sub(size.padding_x() as usize) / (size.cell_width() as usize);
        // let col = min(Column(col), size.last_column());

        // let line = self.y.saturating_sub(size.padding_y() as usize) / (size.cell_height() as usize);
        // let line = min(line, size.bottommost_line().0 as usize);

        self.viewport_to_point(display_offset, Pos::new(1, Column(10)))
    }
}
