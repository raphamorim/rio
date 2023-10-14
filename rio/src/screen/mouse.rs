use crate::crosswords::pos::Column;
use crate::crosswords::pos::Line;
use crate::crosswords::pos::Side;
use crate::event::ClickState;
use crate::screen::Pos;
use std::time::Instant;
use sugarloaf::layout::SugarloafLayout;
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
    layout: &SugarloafLayout,
) -> Pos {
    let mouse_x_f32 = mouse.x as f32;

    let SugarloafLayout {
        scale_factor,
        columns,
        lines,
        margin,
        sugarwidth,
        sugarheight,
        ..
    } = layout.clone();

    let scaled_margin_x = margin.x * scale_factor;

    let col: Column = if scaled_margin_x >= mouse_x_f32 {
        Column(0)
    } else {
        let col = ((mouse_x_f32 - margin.x) / sugarwidth.floor()).floor() as usize;
        std::cmp::min(Column(col), Column(columns))
    };

    // TODO: Refactor row position
    let row = mouse
        .y
        .saturating_sub((margin.top_y * 2. * scale_factor) as usize)
        / sugarheight as usize;
    let calc_row = std::cmp::min(row, lines - 1);
    let row = Line(calc_row as i32) - (display_offset);

    Pos::new(row, col)
}

// TODO: Write more tests for calculate_mouse_position
#[cfg(test)]
pub mod test {
    use sugarloaf::layout::Delta;

    use super::*;

    #[test]
    fn test_position_calculation_by_moving_mouse_x() {
        let display_offset = 0;
        let scale_factor = 1.0;
        let columns = 80;
        let lines = 25;
        let margin_x_left = 0.0;
        let margin_y_top = 0.0;

        let layout = SugarloafLayout {
            scale_factor,
            columns,
            lines,
            margin: Delta {
                x: margin_x_left,
                top_y: margin_y_top,
                ..Default::default()
            },
            sugarwidth: 9.4,
            sugarheight: 18.0,
            ..Default::default()
        };

        let mouse = Mouse {
            x: 8,
            ..Default::default()
        };
        let pos = calculate_mouse_position(&mouse, display_offset, &layout);
        assert_eq!(pos, Pos::new(Line(0), Column(0)));

        let mouse = Mouse {
            x: 8,
            ..Default::default()
        };
        let pos = calculate_mouse_position(&mouse, display_offset, &layout);

        assert_eq!(pos, Pos::new(Line(0), Column(0)));

        let mouse = Mouse {
            x: 9,
            ..Default::default()
        };
        let pos = calculate_mouse_position(&mouse, display_offset, &layout);
        assert_eq!(pos, Pos::new(Line(0), Column(1)));

        let mouse = Mouse {
            x: 17,
            ..Default::default()
        };
        let pos = calculate_mouse_position(&mouse, display_offset, &layout);
        assert_eq!(pos, Pos::new(Line(0), Column(1)));

        let mouse = Mouse {
            x: 19,
            ..Default::default()
        };
        let pos = calculate_mouse_position(&mouse, display_offset, &layout);
        assert_eq!(pos, Pos::new(Line(0), Column(2)));
    }
}
