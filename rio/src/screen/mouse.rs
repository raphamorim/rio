use crate::crosswords::pos::Column;
use crate::crosswords::pos::Line;
use crate::crosswords::pos::Side;
use crate::event::ClickState;
use crate::screen::Pos;
use std::time::Instant;
use winit::event::ElementState;
use winit::event::MouseButton;

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
    pub fn new(multiplier: f64) -> Self {
        Self {
            multiplier,
            ..Default::default()
        }
    }

    #[inline]
    pub fn set_multiplier(&mut self, multiplier: f64) {
        self.multiplier = multiplier;
    }
}

#[inline]
pub fn calculate_mouse_position(
    mouse: &Mouse,
    display_offset: usize,
    scale_factor: f32,
    config_columns_rows: (usize, usize),
    margin_x_left: f32,
    margin_y_top: f32,
    cell_dimension: (f32, f32),
) -> Pos {
    let mouse_x_f32 = mouse.x as f32;
    let scaled_margin_x = margin_x_left * scale_factor;
    // println!("mouse_x_f32 {:?}", mouse_x_f32);
    // println!("layout.margin.x {:?}", layout.margin.x);

    let col: Column = if scaled_margin_x >= mouse_x_f32 {
        Column(0)
    } else {
        let col =
            ((mouse_x_f32 - margin_x_left) / cell_dimension.0.floor()).floor() as usize;
        std::cmp::min(Column(col), Column(config_columns_rows.0))
    };

    // TODO: Refactor row position
    let row = mouse
        .y
        .saturating_sub((margin_y_top * 2. * scale_factor) as usize)
        / cell_dimension.1 as usize;
    let calc_row = std::cmp::min(row, config_columns_rows.1 - 1);
    let row = Line(calc_row as i32) - (display_offset);

    Pos::new(row, col)
}

// TODO: Write more tests for calculate_mouse_position
#[cfg(test)]
pub mod test {
    use super::*;

    #[test]
    fn test_position_calculation_by_moving_mouse_x() {
        let display_offset = 0;
        let scale_factor = 1.0;
        let columns = 80;
        let lines = 25;
        let margin_x_left = 0.0;
        let margin_y_top = 0.0;
        let cell_dimension_width = 9.4;
        let cell_dimension_height = 18.0;

        let mouse = Mouse {
            x: 8,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            scale_factor,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(0)));

        let mouse = Mouse {
            x: 8,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            scale_factor,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(0)));

        let mouse = Mouse {
            x: 9,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            scale_factor,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(1)));

        let mouse = Mouse {
            x: 17,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            scale_factor,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(1)));

        let mouse = Mouse {
            x: 19,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            scale_factor,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(2)));
    }
}
