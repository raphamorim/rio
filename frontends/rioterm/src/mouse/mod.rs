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
    pub on_border: bool,
    /// Raw (unclamped) cursor Y in physical pixels, for selection scroll.
    pub raw_y: f64,
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
            on_border: false,
            accumulated_scroll: AccumulatedScroll::default(),
            x: Default::default(),
            y: Default::default(),
            raw_y: 0.0,
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
    columns_rows: (usize, usize),
    margin_x_left: f32,
    margin_y_top: f32,
    cell_dimension: (f32, f32),
) -> Pos {
    // In case sugarloaf hasn't obtained the dimensions
    if cell_dimension.0 == 0.0 || cell_dimension.1 == 0.0 {
        return Pos::default();
    }

    let cell_width = cell_dimension.0;
    let cell_height = cell_dimension.1;
    // Margins are already pre-scaled (multiplied by scale_factor in
    // update_scaled_margin), so use them directly — do not scale again.
    let margin_x = margin_x_left as usize;
    let margin_y = margin_y_top as usize;

    let col = if (margin_x + cell_width as usize) > mouse.x {
        Column(0)
    } else {
        let col = ((mouse.x - margin_x) as f32 / cell_width) as usize;
        std::cmp::min(Column(col), Column(columns_rows.0 - 1))
    };

    let row = mouse.y.saturating_sub(margin_y) as f32 / cell_height;
    let calc_row = std::cmp::min(row as usize, columns_rows.1 - 1);
    let row = Line(calc_row as i32) - (display_offset);

    Pos::new(row, col)
}

/// Determine which side of a cell the mouse x-position falls on.
///
/// `margin_x` is already pre-scaled (physical pixels).
/// `cell_width` is the float cell width.
/// `grid_width` is the total width of the grid area (physical pixels).
///
/// Uses a 60% threshold (matching ghostty) rather than the 50% midpoint rio
/// inherited from alacritty. Clicks land on a cell until the cursor is past
/// 60% across it, and a drag must cross 60% of the next cell before it's
/// included — this reduces accidental half-cell snapping at the midpoint.
#[inline]
pub fn calculate_side_by_pos(
    x: usize,
    margin_x: f32,
    cell_width: f32,
    grid_width: f32,
) -> Side {
    let x_in_grid = (x as f32 - margin_x).max(0.0);
    let cell_x = x_in_grid % cell_width;
    let threshold = cell_width * 0.6;

    let additional_padding = (grid_width - margin_x) % cell_width;
    let end_of_grid = grid_width - margin_x - additional_padding;

    if cell_x >= threshold || x as f32 >= end_of_grid {
        Side::Right
    } else {
        Side::Left
    }
}

#[cfg(test)]
pub mod test {
    use super::*;

    /// Cell boundaries with width=9.4 and no margin: 0, 9.4, 18.8, 28.2, ...
    #[test]
    fn test_pos_calc_moving_mouse_x_with_scale_1() {
        let display_offset = 0;

        let columns = 10;
        let lines = 5;
        let margin_x_left = 0.0;
        let margin_y_top = 0.0;
        let cell_dimension_width = 9.4;
        let cell_dimension_height = 18.0;

        // x=8 → 8/9.4 = 0.85 → col 0
        let mouse = Mouse {
            x: 8,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(0)));

        // x=9 → 9/9.4 = 0.96 → col 0 (still within first cell)
        let mouse = Mouse {
            x: 9,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(0)));

        // x=10 → 10/9.4 = 1.06 → col 1
        let mouse = Mouse {
            x: 10,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(1)));

        // x=17 → 17/9.4 = 1.81 → col 1
        let mouse = Mouse {
            x: 17,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(1)));

        // x=19 → 19/9.4 = 2.02 → col 2
        let mouse = Mouse {
            x: 19,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(2)));
    }

    /// Same as scale_1 — scale_factor doesn't affect column math when margins
    /// are already pre-scaled (and here margin is 0).
    #[test]
    fn test_pos_calc_moving_mouse_x_with_scale_2() {
        let display_offset = 0;

        let columns = 10;
        let lines = 5;
        let margin_x_left = 0.0; // already scaled
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
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(0)));

        // x=9 → 9/9.4 = 0.96 → col 0
        let mouse = Mouse {
            x: 9,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(0)));

        // x=10 → 10/9.4 = 1.06 → col 1
        let mouse = Mouse {
            x: 10,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
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
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(2)));
    }

    /// With pre-scaled margin=10, cell boundaries at: 10, 19.4, 28.8, 38.2, ...
    #[test]
    fn test_pos_calc_moving_mouse_x_with_scale_1_with_margin_10() {
        let display_offset = 0;

        let columns = 10;
        let lines = 5;
        let margin_x_left = 10.0; // already scaled (10 * 1.0)
        let margin_y_top = 0.0;
        let cell_dimension_width = 9.4;
        let cell_dimension_height = 18.0;

        // x=8 → before margin → col 0
        let mouse = Mouse {
            x: 8,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(0)));

        // x=9 → before margin → col 0
        let mouse = Mouse {
            x: 9,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(0)));

        // x=18 → (18-10)/9.4 = 0.85 → col 0
        let mouse = Mouse {
            x: 18,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(0)));

        // x=20 → (20-10)/9.4 = 1.06 → col 1
        let mouse = Mouse {
            x: 20,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(1)));

        // x=28 → (28-10)/9.4 = 1.91 → col 1
        let mouse = Mouse {
            x: 28,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(1)));

        // x=29 → (29-10)/9.4 = 2.02 → col 2
        let mouse = Mouse {
            x: 29,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(2)));

        // x=38 → (38-10)/9.4 = 2.98 → col 2
        let mouse = Mouse {
            x: 38,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(2)));

        // x=39 → (39-10)/9.4 = 3.08 → col 3
        let mouse = Mouse {
            x: 39,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(3)));
    }

    /// Margin=20 is already pre-scaled (e.g. config 10.0 * scale 2.0).
    /// Cell boundaries at: 20, 29.4, 38.8, ...
    #[test]
    fn test_pos_calc_moving_mouse_x_with_scale_2_with_margin_10() {
        let display_offset = 0;

        let columns = 10;
        let lines = 5;
        let margin_x_left = 20.0; // already scaled (10 * 2.0)
        let margin_y_top = 0.0;
        let cell_dimension_width = 9.4;
        let cell_dimension_height = 18.0;

        // x=9 → before margin → col 0
        let mouse = Mouse {
            x: 9,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(0)));

        // x=28 → (28-20)/9.4 = 0.85 → col 0
        let mouse = Mouse {
            x: 28,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(0)));

        // x=30 → (30-20)/9.4 = 1.06 → col 1
        let mouse = Mouse {
            x: 30,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(1)));
    }

    /// Regression: margins passed to calculate_mouse_position are already
    /// pre-scaled (multiplied by scale_factor in update_scaled_margin), but the
    /// function multiplied them by scale_factor again. With a 2× display and
    /// margin_y_top=72 (already 36*2), the double-scaling produces 144 instead
    /// of 72, shifting the row calculation by ~2 rows.
    ///
    /// Uses exact values observed on a Retina display:
    /// cell 16.41×33, margin (4, 72) pre-scaled, scale 2.0, 96×27 grid.
    #[test]
    fn test_row_not_double_scaled() {
        let display_offset = 0;

        let columns = 96;
        let lines = 27;
        // These margins are ALREADY scaled (config margin * scale_factor).
        let margin_x_left = 4.0; // e.g. config 2.0 * scale 2.0
        let margin_y_top = 72.0; // e.g. config 36.0 * scale 2.0
        let cell_w = 16.41;
        let cell_h = 33.0;

        // Row 0 starts at y = margin_y_top = 72.
        // Row 6 spans y = [72 + 6*33, 72 + 7*33) = [270, 303).
        let mouse = Mouse {
            x: 100,
            y: 280,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_w, cell_h),
        );
        // (280 - 72) / 33 = 6.3 → row 6
        // Bug: (280 - 144) / 33 = 4.1 → row 4 (margin double-scaled)
        assert_eq!(pos.row, Line(6));

        // Row 22 spans y = [72 + 22*33, 72 + 23*33) = [798, 831).
        let mouse = Mouse {
            x: 100,
            y: 820,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_w, cell_h),
        );
        // (820 - 72) / 33 = 22.7 → row 22
        // Bug: (820 - 144) / 33 = 20.5 → row 20
        assert_eq!(pos.row, Line(22));
    }

    /// Same double-scaling issue on the X axis, but less visible with small
    /// margins. With margin_x_left=20 (pre-scaled) and scale=2.0 the error
    /// is 20 extra pixels — enough to shift a column.
    #[test]
    fn test_col_not_double_scaled() {
        let display_offset = 0;

        let columns = 96;
        let lines = 27;
        let margin_x_left = 20.0; // already scaled
        let margin_y_top = 72.0;
        let cell_w = 16.41;
        let cell_h = 33.0;

        // Col 3 starts at x = 20 + 3*16.41 = 69.23.
        let mouse = Mouse {
            x: 70,
            y: 200,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_w, cell_h),
        );
        // (70 - 20) / 16.41 = 3.05 → col 3
        // Bug: (70 - 40) / 16 = 1.87 → col 1 (margin double-scaled + int truncation)
        assert_eq!(pos.col, Column(3));
    }

    /// Cell width=12.6, margin=10 (pre-scaled). Boundaries: 10, 22.6, 35.2, 47.8, ...
    #[test]
    fn test_pos_calc_font_size_12_6_moving_mouse_x_with_scale_1_with_margin_10() {
        let display_offset = 0;

        let columns = 10;
        let lines = 5;
        let margin_x_left = 10.0;
        let margin_y_top = 0.0;
        let cell_dimension_width = 12.6;
        let cell_dimension_height = 18.0;

        // x=10 → (10-10)/12.6 = 0 → col 0
        let mouse = Mouse {
            x: 10,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(0)));

        // x=22 → (22-10)/12.6 = 0.95 → col 0
        let mouse = Mouse {
            x: 22,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(0)));

        // x=23 → (23-10)/12.6 = 1.03 → col 1
        let mouse = Mouse {
            x: 23,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(1)));

        // x=35 → (35-10)/12.6 = 1.98 → col 1
        let mouse = Mouse {
            x: 35,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(1)));

        // x=36 → (36-10)/12.6 = 2.06 → col 2
        let mouse = Mouse {
            x: 36,
            ..Default::default()
        };
        let pos = calculate_mouse_position(
            &mouse,
            display_offset,
            (columns, lines),
            margin_x_left,
            margin_y_top,
            (cell_dimension_width, cell_dimension_height),
        );
        assert_eq!(pos, Pos::new(Line(0), Column(2)));
    }

    /// With cell_width=16.41 the 60% threshold sits at 9.846, so Left
    /// spans [0, 9.846) and Right spans [9.846, 16.41). Using float math
    /// avoids truncation that would shift the boundary.
    #[test]
    fn test_side_by_pos_float_precision() {
        let cell_width = 16.41_f32;
        let margin_x = 8.0; // pre-scaled
        let grid_width = 8.0 + 96.0 * cell_width;

        // pixel 16 → cell_x = 8.0 < threshold 9.846 → Left
        assert_eq!(
            calculate_side_by_pos(16, margin_x, cell_width, grid_width),
            Side::Left,
        );

        // pixel 17 → cell_x = 9.0 < threshold 9.846 → still Left (was Right at 50%)
        assert_eq!(
            calculate_side_by_pos(17, margin_x, cell_width, grid_width),
            Side::Left,
        );

        // pixel 18 → cell_x = 10.0 >= threshold 9.846 → Right
        assert_eq!(
            calculate_side_by_pos(18, margin_x, cell_width, grid_width),
            Side::Right,
        );
    }

    /// Regression: integer truncation of cell_width (16.41 → 16) caused
    /// the modulo to drift at higher pixel positions, making the side
    /// detection wrong for cells far from the left edge.
    #[test]
    fn test_side_by_pos_no_drift_at_high_columns() {
        let cell_width = 16.41_f32;
        let margin_x = 8.0;
        let grid_width = 8.0 + 96.0 * cell_width;

        // Column 76 starts at margin + 76 * 16.41 = 8 + 1247.16 = 1255.16
        // Left side: pixel 1256 → cell_x = 1248 % 16.41 ≈ 0.66 → Left
        assert_eq!(
            calculate_side_by_pos(1256, margin_x, cell_width, grid_width),
            Side::Left,
        );

        // Pixel 1266 → cell_x = 1258 % 16.41 ≈ 10.48 >= 9.846 → Right
        assert_eq!(
            calculate_side_by_pos(1266, margin_x, cell_width, grid_width),
            Side::Right,
        );
    }

    /// Margin must not be double-scaled in side calculation.
    #[test]
    fn test_side_by_pos_prescaled_margin() {
        let cell_width = 16.0;
        let margin_x = 40.0; // already scaled (e.g. 20 * 2.0)
        let grid_width = 40.0 + 80.0 * 16.0;

        // Pixel 41: just past margin, left side of cell 0
        // cell_x = (41 - 40) % 16 = 1.0, threshold = 9.6 → Left
        assert_eq!(
            calculate_side_by_pos(41, margin_x, cell_width, grid_width),
            Side::Left,
        );

        // Pixel 50: past the 60% threshold of cell 0
        // cell_x = (50 - 40) % 16 = 10.0, threshold = 9.6 → Right
        assert_eq!(
            calculate_side_by_pos(50, margin_x, cell_width, grid_width),
            Side::Right,
        );

        // Pixel 49: cell_x = 9.0 < threshold 9.6 → Left (was Right at 50%)
        assert_eq!(
            calculate_side_by_pos(49, margin_x, cell_width, grid_width),
            Side::Left,
        );

        // Pixel 30: before margin → clamped to 0, Left
        assert_eq!(
            calculate_side_by_pos(30, margin_x, cell_width, grid_width),
            Side::Left,
        );
    }
}
