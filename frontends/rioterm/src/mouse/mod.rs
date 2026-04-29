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
    /// Cursor X in physical pixels. `f64` so subpixel precision from
    /// the OS event survives all the way to the cell-grid divide.
    pub x: f64,
    /// Cursor Y in physical pixels.
    pub y: f64,
    pub on_border: bool,
    /// Raw (unclamped) cursor Y in physical pixels, for selection scroll.
    pub raw_y: f64,
    /// Last cell (line, column) the cursor was over. `None` until the
    /// first `CursorMoved` event arrives. Used by the input dispatcher
    /// to skip hint / OSC-8 / hyperlink work when the cursor moves
    /// within the same cell — replaces the old pixel-equality check
    /// that fired on every subpixel HiDPI jitter.
    pub last_cell: Option<Pos>,
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
            x: 0.0,
            y: 0.0,
            raw_y: 0.0,
            last_cell: None,
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

/// Map a physical-pixel cursor position to a terminal grid `Pos`.
///
/// Pixel coords stay `f64` until the final integer truncation, and
/// the divide uses the canonical `u32` `cell_width / cell_height`
/// (the same integers the GPU shader paints with — no drift between
/// painted cell stride and click→cell mapping).
///
/// `margin_x_left / margin_y_top` are already pre-scaled (physical
/// pixels), do not multiply by `scale_factor` here.
#[inline]
pub fn calculate_mouse_position(
    mouse: &Mouse,
    display_offset: usize,
    columns_rows: (usize, usize),
    margin_x_left: f32,
    margin_y_top: f32,
    cell: (u32, u32),
) -> Pos {
    let (cell_w, cell_h) = (cell.0 as f64, cell.1 as f64);
    if cell_w == 0.0 || cell_h == 0.0 {
        return Pos::default();
    }

    let margin_x = margin_x_left as f64;
    let margin_y = margin_y_top as f64;

    // f64 throughout. Negative-clamp via `.max(0.0)` so clicks in
    // the margin map to col/row 0 rather than wrapping or
    // overflowing on the cast.
    let x_in_grid = (mouse.x - margin_x).max(0.0);
    let y_in_grid = (mouse.y - margin_y).max(0.0);
    let col_idx = (x_in_grid / cell_w) as usize;
    let row_idx = (y_in_grid / cell_h) as usize;

    let col = std::cmp::min(Column(col_idx), Column(columns_rows.0 - 1));
    let row = std::cmp::min(row_idx, columns_rows.1 - 1);
    let row = Line(row as i32) - display_offset;

    Pos::new(row, col)
}

/// Determine which side of a cell the mouse x-position falls on.
///
/// `margin_x` is pre-scaled (physical pixels). `cell_width` is the
/// canonical `u32` cell width (same value the renderer paints with).
/// `grid_width` is the total width of the grid area in physical
/// pixels.
///
/// 60% threshold rather than the 50% midpoint inherited from
/// alacritty: clicks land on a cell until the cursor is past 60%
/// across it, and a drag must cross 60% of the next cell before
/// it's included — reduces accidental half-cell snapping at the
/// midpoint.
#[inline]
pub fn calculate_side_by_pos(
    x: f64,
    margin_x: f32,
    cell_width: u32,
    grid_width: f32,
) -> Side {
    let cell_w = cell_width as f64;
    let margin = margin_x as f64;
    let grid_w = grid_width as f64;

    let x_in_grid = (x - margin).max(0.0);
    let cell_x = x_in_grid % cell_w;
    let threshold = cell_w * 0.6;

    let usable = (grid_w - margin).max(0.0);
    let additional_padding = usable % cell_w;
    let end_of_grid = margin + usable - additional_padding;

    if cell_x >= threshold || x >= end_of_grid {
        Side::Right
    } else {
        Side::Left
    }
}

#[cfg(test)]
pub mod test {
    use super::*;

    fn mk_mouse(x: f64, y: f64) -> Mouse {
        Mouse {
            x,
            y,
            ..Default::default()
        }
    }

    /// Canonical stride: cell width = 9 (u32). Boundaries at 0, 9, 18, 27.
    #[test]
    fn pos_calc_basic_no_margin() {
        let cols = 10;
        let lines = 5;
        let cell = (9u32, 18u32);

        // x=8 → 8/9 = 0 → col 0
        assert_eq!(
            calculate_mouse_position(
                &mk_mouse(8.0, 0.0),
                0,
                (cols, lines),
                0.0,
                0.0,
                cell
            )
            .col,
            Column(0),
        );
        // x=8.99 → still col 0 (subpixel precision preserved)
        assert_eq!(
            calculate_mouse_position(
                &mk_mouse(8.99, 0.0),
                0,
                (cols, lines),
                0.0,
                0.0,
                cell
            )
            .col,
            Column(0),
        );
        // x=9 → boundary → col 1
        assert_eq!(
            calculate_mouse_position(
                &mk_mouse(9.0, 0.0),
                0,
                (cols, lines),
                0.0,
                0.0,
                cell
            )
            .col,
            Column(1),
        );
        // x=17.5 → 17.5/9 = 1.94 → col 1
        assert_eq!(
            calculate_mouse_position(
                &mk_mouse(17.5, 0.0),
                0,
                (cols, lines),
                0.0,
                0.0,
                cell
            )
            .col,
            Column(1),
        );
        // x=18 → col 2
        assert_eq!(
            calculate_mouse_position(
                &mk_mouse(18.0, 0.0),
                0,
                (cols, lines),
                0.0,
                0.0,
                cell
            )
            .col,
            Column(2),
        );
    }

    /// Pre-scaled margin: clicks before the margin clamp to col 0.
    /// Boundaries at 10, 19, 28, 37, ... (cell width 9 with margin 10).
    #[test]
    fn pos_calc_with_prescaled_margin() {
        let cols = 10;
        let lines = 5;
        let cell = (9u32, 18u32);
        let margin_x = 10.0_f32;

        // Click before margin → col 0.
        assert_eq!(
            calculate_mouse_position(
                &mk_mouse(5.0, 0.0),
                0,
                (cols, lines),
                margin_x,
                0.0,
                cell
            )
            .col,
            Column(0),
        );
        // x=10 → col 0 (start of cell 0).
        assert_eq!(
            calculate_mouse_position(
                &mk_mouse(10.0, 0.0),
                0,
                (cols, lines),
                margin_x,
                0.0,
                cell
            )
            .col,
            Column(0),
        );
        // x=18.99 → still col 0.
        assert_eq!(
            calculate_mouse_position(
                &mk_mouse(18.99, 0.0),
                0,
                (cols, lines),
                margin_x,
                0.0,
                cell
            )
            .col,
            Column(0),
        );
        // x=19 → col 1.
        assert_eq!(
            calculate_mouse_position(
                &mk_mouse(19.0, 0.0),
                0,
                (cols, lines),
                margin_x,
                0.0,
                cell
            )
            .col,
            Column(1),
        );
    }

    /// Regression for the actual mouse-positioning bug: with the
    /// renderer painting at canonical `u32` stride `cell.cell_width`,
    /// a click on the visual middle of a high-index column must map
    /// to that same column index. The old code passed unrounded
    /// `f32` cell_width to the divide, causing accumulating drift
    /// (≈0.41px per col → wrong by 3 columns at col 100 with width
    /// 16.41).
    #[test]
    fn pos_calc_no_drift_at_high_column_index() {
        let cols = 200;
        let lines = 50;
        // u32 stride — the same value the GPU shader paints with.
        let cell = (16u32, 33u32);

        // Painted col 100 occupies pixels [1600, 1616). Click in the middle.
        let pos = calculate_mouse_position(
            &mk_mouse(1608.0, 100.0),
            0,
            (cols, lines),
            0.0,
            0.0,
            cell,
        );
        assert_eq!(pos.col, Column(100));

        // Painted col 150 at pixels [2400, 2416). Click left edge.
        let pos = calculate_mouse_position(
            &mk_mouse(2400.0, 100.0),
            0,
            (cols, lines),
            0.0,
            0.0,
            cell,
        );
        assert_eq!(pos.col, Column(150));
    }

    /// Subpixel mouse precision survives the divide. With cell=16
    /// and HiDPI delivering `1608.5` from the OS, the f64 path gives
    /// col 100; if we'd cast `mouse.x` to `usize` early (the old
    /// behavior) we'd see `1608 / 16 = 100` too, but at the boundary
    /// (e.g. `1599.9 → col 99` not `col 100`) precision matters.
    #[test]
    fn pos_calc_subpixel_precision_preserved() {
        let cell = (16u32, 33u32);
        let cols = 200;
        let lines = 50;

        // 1599.9 should map to col 99 (cell 99 at [1584,1600)).
        let pos = calculate_mouse_position(
            &mk_mouse(1599.9, 100.0),
            0,
            (cols, lines),
            0.0,
            0.0,
            cell,
        );
        assert_eq!(pos.col, Column(99));

        // 1600.1 should map to col 100.
        let pos = calculate_mouse_position(
            &mk_mouse(1600.1, 100.0),
            0,
            (cols, lines),
            0.0,
            0.0,
            cell,
        );
        assert_eq!(pos.col, Column(100));
    }

    /// Y axis: pre-scaled margin must not be re-scaled. Row stride
    /// is the canonical `cell.1` (already includes line_height).
    #[test]
    fn pos_calc_row_with_prescaled_margin() {
        let cols = 96;
        let lines = 27;
        let margin_y = 72.0_f32;
        let cell = (16u32, 33u32);

        // Row 0 begins at y = 72. Row 6 spans [72+198, 72+231) = [270, 303).
        let pos = calculate_mouse_position(
            &mk_mouse(100.0, 280.0),
            0,
            (cols, lines),
            0.0,
            margin_y,
            cell,
        );
        assert_eq!(pos.row, Line(6));

        // Row 22 spans [72 + 22*33, 72 + 23*33) = [798, 831).
        let pos = calculate_mouse_position(
            &mk_mouse(100.0, 820.0),
            0,
            (cols, lines),
            0.0,
            margin_y,
            cell,
        );
        assert_eq!(pos.row, Line(22));
    }

    /// Display offset shifts the reported row by the scrollback
    /// position so callers get a viewport-relative `Line` index.
    #[test]
    fn pos_calc_display_offset_shifts_row() {
        let cell = (16u32, 33u32);
        let cols = 96;
        let lines = 27;

        // Row index 5 from top, display_offset 10 → Line(5) - 10 = Line(-5).
        let pos = calculate_mouse_position(
            &mk_mouse(100.0, 5.0 * 33.0 + 1.0),
            10,
            (cols, lines),
            0.0,
            0.0,
            cell,
        );
        assert_eq!(pos.row, Line(-5));
    }

    /// 60% threshold: cell_x < 0.6*cell_w → Left, otherwise Right.
    /// Boundary lands on subpixel position.
    #[test]
    fn side_by_pos_60_percent_threshold() {
        let cell = 16u32;
        let margin_x = 0.0_f32;
        let grid_w = 16.0 * 200.0;

        // cell_x = 9.59 < 9.6 → Left.
        assert_eq!(
            calculate_side_by_pos(9.59, margin_x, cell, grid_w),
            Side::Left,
        );
        // cell_x ≥ 9.6 → Right. Use 9.61 to dodge f64 rep of 9.6.
        assert_eq!(
            calculate_side_by_pos(9.61, margin_x, cell, grid_w),
            Side::Right,
        );
    }

    /// Regression: side detection had drift at high columns when the
    /// f32 divide was off by half a pixel per cell. With u32 stride
    /// the modulo stays exact at any column.
    #[test]
    fn side_by_pos_no_drift_at_high_columns() {
        let cell = 16u32;
        let margin_x = 0.0_f32;
        let grid_w = 16.0 * 200.0;

        // Col 100 = pixel [1600, 1616). Left side: cell_x = 0..9.6.
        assert_eq!(
            calculate_side_by_pos(1600.0, margin_x, cell, grid_w),
            Side::Left,
        );
        assert_eq!(
            calculate_side_by_pos(1609.5, margin_x, cell, grid_w),
            Side::Left,
        );
        // ≥ 9.6 → Right. Use 9.61 to dodge f64 representation of 9.6.
        assert_eq!(
            calculate_side_by_pos(1609.61, margin_x, cell, grid_w),
            Side::Right,
        );
    }

    /// Margin pre-scaled, must not be re-scaled in the side
    /// calculation. Pixel before margin clamps to Left.
    #[test]
    fn side_by_pos_prescaled_margin() {
        let cell = 16u32;
        let margin_x = 40.0_f32;
        let grid_w = 40.0 + 80.0 * 16.0;

        // Before margin → Left.
        assert_eq!(
            calculate_side_by_pos(30.0, margin_x, cell, grid_w),
            Side::Left,
        );
        // x=49: cell_x = 9 < 9.6 → Left.
        assert_eq!(
            calculate_side_by_pos(49.0, margin_x, cell, grid_w),
            Side::Left,
        );
        // x=49.61 → cell_x ≥ 9.6 → Right (49.6 hits f64 rep edge).
        assert_eq!(
            calculate_side_by_pos(49.61, margin_x, cell, grid_w),
            Side::Right,
        );
    }
}
