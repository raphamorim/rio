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

/// Where a press went, and with it the drag and the release of its
/// button, whatever the modifiers and the mode say by then.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PressTarget {
    /// No press is held, or the chrome consumed it: a tab, a button of
    /// the search overlay, the scrollbar, a divider. Nothing follows
    /// such a press onto the grid: it extends no older selection on
    /// its way out, and its release copies nothing.
    #[default]
    Nowhere,
    /// The grid, with the left button: the press selects while it is
    /// held and moves, and its release copies the selection it leaves.
    Grid,
    /// The application, which was told about the press and is told
    /// about its drag and its release too, so it never sees one
    /// without the other. Such a press never selects.
    Application,
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
    /// The hint under an in-flight left press that bypassed mouse
    /// reporting. The release consults this latch instead of
    /// re-checking the highlight (the modifier can change mid-click,
    /// and re-evaluating would let an application see a press without
    /// its release) and follows the link only when the release lands
    /// on the same hint.
    pub hint_click_latched: Option<crate::hints::HintMatch>,
    /// Where the press of each button went. The drag and the release
    /// of a button follow its press.
    left_press: PressTarget,
    middle_press: PressTarget,
    right_press: PressTarget,
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
            hint_click_latched: None,
            left_press: PressTarget::Nowhere,
            middle_press: PressTarget::Nowhere,
            right_press: PressTarget::Nowhere,
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

    /// Whether `button` is held; the buttons that are not tracked read
    /// released.
    pub fn button_state(&self, button: MouseButton) -> ElementState {
        match button {
            MouseButton::Left => self.left_button_state,
            MouseButton::Middle => self.middle_button_state,
            MouseButton::Right => self.right_button_state,
            _ => ElementState::Released,
        }
    }

    /// `button` is pressed or released.
    pub fn set_button_state(&mut self, button: MouseButton, state: ElementState) {
        match button {
            MouseButton::Left => self.left_button_state = state,
            MouseButton::Middle => self.middle_button_state = state,
            MouseButton::Right => self.right_button_state = state,
            _ => {}
        }
    }

    /// Where the press of `button` went; the buttons that are not
    /// tracked went nowhere.
    pub fn press_target(&self, button: MouseButton) -> PressTarget {
        match button {
            MouseButton::Left => self.left_press,
            MouseButton::Middle => self.middle_press,
            MouseButton::Right => self.right_press,
            _ => PressTarget::Nowhere,
        }
    }

    fn set_press_target(&mut self, button: MouseButton, target: PressTarget) {
        match button {
            MouseButton::Left => self.left_press = target,
            MouseButton::Middle => self.middle_press = target,
            MouseButton::Right => self.right_press = target,
            _ => {}
        }
    }

    /// A press begins. Nothing an earlier press of the button left
    /// behind carries into it, not even one whose release was lost.
    pub fn begin_press(&mut self, button: MouseButton) {
        self.set_press_target(button, PressTarget::Nowhere);
    }

    /// The press of `button` landed on the grid or with the
    /// application; the chrome's presses land nowhere.
    pub fn press_lands(&mut self, button: MouseButton, target: PressTarget) {
        self.set_press_target(button, target);
    }

    /// Whether the left button is held by a press on the grid: its
    /// motion extends the selection.
    pub fn is_selecting(&self) -> bool {
        self.left_button_state == ElementState::Pressed
            && self.left_press == PressTarget::Grid
    }

    /// Whether `button` is held by a press the application was told
    /// about: its motion is reported as a drag.
    pub fn is_dragging_for_application(&self, button: MouseButton) -> bool {
        self.button_state(button) == ElementState::Pressed
            && self.press_target(button) == PressTarget::Application
    }

    /// Where the press of `button` went; asked once, by its release,
    /// which follows it there.
    pub fn take_press(&mut self, button: MouseButton) -> PressTarget {
        let target = self.press_target(button);
        self.set_press_target(button, PressTarget::Nowhere);
        target
    }

    /// The release of `button` arrived where the terminal could not
    /// act on it: a dialog or another route took the pointer. The
    /// button reads released and its press is over, or the next
    /// release would follow a press that is not its own.
    pub fn release_lost(&mut self, button: MouseButton) {
        self.set_button_state(button, ElementState::Released);
        self.set_press_target(button, PressTarget::Nowhere);
    }

    /// The window lost focus: every button reads released, or the
    /// next motion after refocus would read as a held button. A
    /// release that still arrives follows its press as usual, so the
    /// presses stay where they went; one that never arrives is
    /// forgotten by the next press of its button.
    pub fn focus_lost(&mut self) {
        for button in [MouseButton::Left, MouseButton::Middle, MouseButton::Right] {
            self.set_button_state(button, ElementState::Released);
        }
    }
}

/// The code a press or release of `button` is reported with; none
/// for the buttons the protocols cannot carry.
pub fn report_code(button: MouseButton) -> Option<u8> {
    match button {
        MouseButton::Left => Some(0),
        MouseButton::Middle => Some(1),
        MouseButton::Right => Some(2),
        _ => None,
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

    // A release that arrived where the terminal could not act on it
    // (a dialog took the pointer) ends that button's press entirely:
    // the next release of the button follows no press.
    #[test]
    fn a_lost_release_ends_the_press() {
        let mut mouse = Mouse::default();
        mouse.set_button_state(MouseButton::Left, ElementState::Pressed);
        mouse.set_button_state(MouseButton::Right, ElementState::Pressed);
        mouse.press_lands(MouseButton::Left, PressTarget::Grid);
        mouse.press_lands(MouseButton::Right, PressTarget::Application);

        mouse.release_lost(MouseButton::Right);
        assert_eq!(mouse.right_button_state, ElementState::Released);
        assert_eq!(mouse.left_button_state, ElementState::Pressed);
        assert_eq!(mouse.take_press(MouseButton::Right), PressTarget::Nowhere);
        assert!(mouse.is_selecting(), "the left press is untouched");

        mouse.release_lost(MouseButton::Left);
        assert!(!mouse.is_selecting());
        assert_eq!(mouse.take_press(MouseButton::Left), PressTarget::Nowhere);
    }

    // Focus loss releases every button, but the release may still
    // arrive and follows its press as usual. A press of the button
    // forgets it instead.
    #[test]
    fn focus_loss_releases_the_buttons_and_keeps_the_press_for_its_release() {
        let mut mouse = Mouse::default();
        for button in [MouseButton::Left, MouseButton::Middle, MouseButton::Right] {
            mouse.set_button_state(button, ElementState::Pressed);
        }
        mouse.press_lands(MouseButton::Left, PressTarget::Grid);
        mouse.press_lands(MouseButton::Middle, PressTarget::Application);

        mouse.focus_lost();
        for button in [MouseButton::Left, MouseButton::Middle, MouseButton::Right] {
            assert_eq!(mouse.button_state(button), ElementState::Released);
        }
        assert_eq!(
            mouse.button_state(MouseButton::Back),
            ElementState::Released
        );
        assert!(!mouse.is_selecting(), "released, so no longer selecting");
        assert!(!mouse.is_dragging_for_application(MouseButton::Middle));
        assert_eq!(mouse.take_press(MouseButton::Left), PressTarget::Grid);
        assert_eq!(
            mouse.take_press(MouseButton::Middle),
            PressTarget::Application
        );
    }

    // A new press of a button starts from nowhere, and leaves the
    // other buttons' presses where they went, since those are still
    // held.
    #[test]
    fn a_press_leaves_nothing_of_the_last_one_behind() {
        let mut mouse = Mouse::default();
        mouse.set_button_state(MouseButton::Left, ElementState::Pressed);
        mouse.press_lands(MouseButton::Left, PressTarget::Grid);
        mouse.press_lands(MouseButton::Middle, PressTarget::Application);

        mouse.begin_press(MouseButton::Right);
        assert!(mouse.is_selecting(), "the left press is still held");
        assert_eq!(
            mouse.press_target(MouseButton::Middle),
            PressTarget::Application
        );

        mouse.begin_press(MouseButton::Left);
        assert!(!mouse.is_selecting());
        assert_eq!(mouse.take_press(MouseButton::Left), PressTarget::Nowhere);

        mouse.begin_press(MouseButton::Middle);
        assert_eq!(
            mouse.press_target(MouseButton::Middle),
            PressTarget::Nowhere
        );
    }

    // Only a held left press on the grid selects, and only a held
    // press the application was told about drags for it; a press
    // that went nowhere, or a released one, does neither.
    #[test]
    fn a_held_press_acts_where_it_landed() {
        let mut mouse = Mouse::default();
        mouse.set_button_state(MouseButton::Left, ElementState::Pressed);
        assert!(!mouse.is_selecting(), "consumed by chrome");
        assert!(!mouse.is_dragging_for_application(MouseButton::Left));

        mouse.press_lands(MouseButton::Left, PressTarget::Grid);
        assert!(mouse.is_selecting());
        assert!(!mouse.is_dragging_for_application(MouseButton::Left));

        mouse.set_button_state(MouseButton::Right, ElementState::Pressed);
        mouse.press_lands(MouseButton::Right, PressTarget::Application);
        assert!(mouse.is_dragging_for_application(MouseButton::Right));

        mouse.set_button_state(MouseButton::Left, ElementState::Released);
        assert!(!mouse.is_selecting(), "released");
        assert_eq!(mouse.take_press(MouseButton::Left), PressTarget::Grid);
        assert_eq!(mouse.take_press(MouseButton::Left), PressTarget::Nowhere);
    }

    // The buttons the protocols can carry have a code; the others
    // have none and are tracked nowhere.
    #[test]
    fn only_reportable_buttons_have_a_code() {
        assert_eq!(report_code(MouseButton::Left), Some(0));
        assert_eq!(report_code(MouseButton::Middle), Some(1));
        assert_eq!(report_code(MouseButton::Right), Some(2));
        assert_eq!(report_code(MouseButton::Back), None);
        let mut mouse = Mouse::default();
        mouse.press_lands(MouseButton::Forward, PressTarget::Application);
        assert_eq!(
            mouse.press_target(MouseButton::Forward),
            PressTarget::Nowhere
        );
    }

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
