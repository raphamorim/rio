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

#[inline]
pub fn calculate_mouse_position(
    mouse: &Mouse,
    display_offset: usize,
    scale_factor: f32,
    config_columns_rows: (usize, usize),
    margin_x: f32,
    margin_y: (f32, f32),
    cell_dimension: (f32, f32),
) -> Pos {
    let mouse_x_f32 = mouse.x as f32;
    let scaled_margin_x = margin_x * scale_factor;
    // println!("mouse_x_f32 {:?}", mouse_x_f32);
    // println!("layout.margin.x {:?}", layout.margin.x);

    let col: Column = if scaled_margin_x >= mouse_x_f32 {
        Column(0)
    } else {
        let col = ((mouse_x_f32 - margin_x) / cell_dimension.0.floor()).floor() as usize;
        std::cmp::min(Column(col), Column(config_columns_rows.0))
    };

    // println!("{:?}", col);

    let row = mouse
        .y
        .saturating_sub((margin_y.0 * 2. * scale_factor) as usize)
        / cell_dimension.1 as usize;
    let calc_row = std::cmp::min(row, config_columns_rows.1 - 1);
    let row = Line(calc_row as i32) - (display_offset);

    Pos::new(row, col)
}

// TODO: Write down calculate_mouse_position tests