use crate::crosswords::pos::Column;
use crate::crosswords::pos::Line;
use crate::crosswords::pos::Side;
use crate::event::ClickState;
use rio_backend::crosswords::pos::Pos;
use rio_window::event::ElementState;
use rio_window::event::MouseButton;
use std::time::Instant;

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
    pub divider: f64,
    pub left_button_state: ElementState,
    pub middle_button_state: ElementState,
    pub right_button_state: ElementState,
    pub last_click_timestamp: Instant,
    pub last_click_button: MouseButton,
    pub click_state: ClickState,
    pub accumulated_scroll: AccumulatedScroll,
    pub square_side: Side,
    pub inside_text_area: bool,
    pub x: usize,
    pub y: usize,
}

impl Default for Mouse {
    fn default() -> Mouse {
        Mouse {
            multiplier: 3.0,
            divider: 1.0,
            last_click_timestamp: Instant::now(),
            last_click_button: MouseButton::Left,
            left_button_state: ElementState::Released,
            middle_button_state: ElementState::Released,
            right_button_state: ElementState::Released,
            click_state: ClickState::None,
            square_side: Side::Left,
            inside_text_area: Default::default(),
            accumulated_scroll: AccumulatedScroll::default(),
            x: Default::default(),
            y: Default::default(),
        }
    }
}

impl Mouse {
    pub fn new(multiplier: f64, divider: f64) -> Self {
        Self {
            multiplier,
            divider,
            ..Default::default()
        }
    }

    #[inline]
    pub fn set_multiplier_and_divider(&mut self, multiplier: f64, divider: f64) {
        self.multiplier = multiplier;
        self.divider = divider;
    }
}

#[inline]
pub fn calculate_mouse_position(
    mouse: &Mouse,
    display_offset: usize,
    scale_factor: f32,
    columns_rows: (usize, usize),
    margin_x_left: f32,
    margin_y_top: f32,
    cell_dimension: (f32, f32),
) -> Pos {
    // In case sugarloaf hasn't obtained the dimensions
    if cell_dimension.0 == 0.0 || cell_dimension.1 == 0.0 {
        return Pos::default();
    }

    let cell_width = cell_dimension.0 as usize;
    let cell_height = cell_dimension.1 as usize;
    let scaled_margin_x = (margin_x_left * scale_factor) as usize;

    let col: Column = if (scaled_margin_x + cell_width) > mouse.x {
        Column(0)
    } else {
        let col = (mouse.x - scaled_margin_x) / cell_width;
        std::cmp::min(Column(col), Column(columns_rows.0 - 1))
    };

    // TODO: Refactor row position
    let row = mouse
        .y
        .saturating_sub((margin_y_top * scale_factor) as usize)
        / cell_height;
    let calc_row = std::cmp::min(row, columns_rows.1 - 1);
    let row = Line(calc_row as i32) - (display_offset);

    Pos::new(row, col)
}

#[cfg(test)]
pub mod test {
    use super::*;

    #[test]
    fn test_pos_calc_moving_mouse_x_with_scale_1() {
        let display_offset = 0;
        let scale_factor = 1.0;
        let columns = 10;
        let lines = 5;
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

    #[test]
    fn test_pos_calc_moving_mouse_x_with_scale_2() {
        let display_offset = 0;
        let scale_factor = 2.0;
        let columns = 10;
        let lines = 5;
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

    #[test]
    fn test_pos_calc_moving_mouse_x_with_scale_1_with_margin_10() {
        let display_offset = 0;
        let scale_factor = 1.0;
        let columns = 10;
        let lines = 5;
        let margin_x_left = 10.0;
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
        assert_eq!(pos, Pos::new(Line(0), Column(0)));

        let mouse = Mouse {
            x: 18,
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
        assert_eq!(pos, Pos::new(Line(0), Column(1)));

        let mouse = Mouse {
            x: 27,
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
            x: 28,
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

        let mouse = Mouse {
            x: 36,
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

        let mouse = Mouse {
            x: 37,
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
        assert_eq!(pos, Pos::new(Line(0), Column(3)));
    }

    #[test]
    fn test_pos_calc_moving_mouse_x_with_scale_2_with_margin_10() {
        let display_offset = 0;
        let scale_factor = 2.0;
        let columns = 10;
        let lines = 5;
        let margin_x_left = 10.0;
        let margin_y_top = 0.0;
        let cell_dimension_width = 9.4;
        let cell_dimension_height = 18.0;

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
        assert_eq!(pos, Pos::new(Line(0), Column(0)));

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
        assert_eq!(pos, Pos::new(Line(0), Column(0)));

        let mouse = Mouse {
            x: 28,
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
            x: 29,
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
    }

    #[test]
    fn test_pos_calc_font_size_12_6_moving_mouse_x_with_scale_1_with_margin_10() {
        let display_offset = 0;
        let scale_factor = 1.0;
        let columns = 10;
        let lines = 5;
        let margin_x_left = 10.0;
        let margin_y_top = 0.0;
        let cell_dimension_width = 12.6;
        let cell_dimension_height = 18.0;

        let mouse = Mouse {
            x: 10,
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
            x: 22,
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
            x: 23,
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
            x: 35,
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

        let mouse = Mouse {
            x: 36,
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
