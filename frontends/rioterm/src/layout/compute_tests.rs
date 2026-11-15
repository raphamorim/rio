use super::*;

// This file tests compute function on different layouts.
// I've added some real scenarios so I can make sure it doesn't go off again.

/// Build a `CellMetrics` whose integer cell stride matches the given
/// `TextDimensions`. Used by tests that construct dimensions
/// directly without going through sugarloaf's font path.
fn cell_for(dims: TextDimensions) -> rio_backend::sugarloaf::layout::CellMetrics {
    rio_backend::sugarloaf::layout::CellMetrics {
        cell_width: dims.width.round().max(1.0) as u32,
        cell_height: dims.height.round().max(1.0) as u32,
        cell_baseline: 0,
        face_width: dims.width as f64,
        face_height: dims.height as f64,
        face_y: 0.0,
    }
}

/// note: Computes the renderer's actual per-line height in physical pixels.
///
/// The renderer gets metrics from Metrics::for_rich_text() which packs
/// cell_height as (ascent, descent, 0.0). cell_height is computed by
/// Metrics::calc at physical font_size scale, with ceil applied.
///
/// basically renderer line_height = ceil((ascent + descent + leading) * scale) * line_height_mod
fn renderer_line_height(
    ascent: f32,
    descent: f32,
    leading: f32,
    line_height_mod: f32,
    scale: f32,
) -> f32 {
    // Matches the Metrics::calc path: scale to physical, then ceil
    let cell_height = ((ascent + descent + leading) * scale).ceil();
    cell_height * line_height_mod
}

fn sugar_height(
    ascent: f32,
    descent: f32,
    leading: f32,
    line_height_mod: f32,
    scale: f32,
) -> f32 {
    ((ascent + descent + leading) * line_height_mod * scale).ceil()
}

/// Verifies that compute() row count fits when rendered.
#[allow(clippy::too_many_arguments)]
fn assert_rows_fit(
    panel_width: f32,
    panel_height: f32,
    sugar_width: f32,
    scale: f32,
    line_height_mod: f32,
    ascent: f32,
    descent: f32,
    leading: f32,
) {
    let sh = sugar_height(ascent, descent, leading, line_height_mod, scale);
    let dimensions = TextDimensions {
        width: sugar_width,
        height: sh,
        scale,
    };

    let (cols, rows) = compute(
        panel_width,
        panel_height,
        cell_for(dimensions),
        Margin::all(0.0),
        scale,
    );
    let _ = line_height_mod; // line_height already baked into `dimensions.height`.

    let actual_line_height =
        renderer_line_height(ascent, descent, leading, line_height_mod, scale);
    let rendered_height = rows as f32 * actual_line_height;

    assert!(
        rendered_height <= panel_height,
        "Rows overflow! {} rows * {:.2}px = {:.2}px rendered, but panel is only {:.2}px tall \
         (cols={}, sugar={:.2}x{:.2}, scale={:.1}, lh_mod={:.1})",
        rows,
        actual_line_height,
        rendered_height,
        panel_height,
        cols,
        sugar_width,
        sh,
        scale,
        line_height_mod,
    );
}

#[test]
fn test_user_case_1834x1436() {
    assert_rows_fit(1834.0, 1436.0, 16.41, 2.0, 1.0, 13.0, 3.5, 0.0);
}

#[test]
fn test_user_case_3766x1996() {
    assert_rows_fit(3766.0, 1996.0, 16.41, 2.0, 1.0, 13.0, 3.5, 0.0);
}

#[test]
fn test_user_case_5104x2736() {
    assert_rows_fit(5104.0, 2736.0, 16.41, 2.0, 1.0, 13.0, 3.5, 0.0);
}

#[test]
fn test_rows_fit_various_sizes() {
    for height in (500..=3000).step_by(50) {
        for width in [800.0, 1600.0, 2400.0, 3200.0] {
            assert_rows_fit(width, height as f32, 16.41, 2.0, 1.0, 13.0, 3.5, 0.0);
        }
    }
}

#[test]
fn test_rows_fit_with_nonzero_leading() {
    let test_cases: Vec<(f32, f32, f32)> = vec![
        (12.0, 3.0, 0.5),
        (12.0, 3.0, 1.0),
        (14.0, 4.0, 0.25),
        (10.0, 3.0, 2.0),
    ];

    for (ascent, descent, leading) in test_cases {
        for height in (500..=2000).step_by(100) {
            assert_rows_fit(
                1600.0,
                height as f32,
                16.0,
                2.0,
                1.0,
                ascent,
                descent,
                leading,
            );
        }
    }
}

#[test]
fn test_rows_fit_with_line_height_modifier() {
    for lh_mod in [1.1, 1.2, 1.5, 2.0] {
        for height in (500..=2000).step_by(100) {
            assert_rows_fit(1600.0, height as f32, 16.0, 2.0, lh_mod, 12.0, 3.0, 0.5);
        }
    }
}

#[test]
fn test_rows_fit_scale_1() {
    for height in (300..=1200).step_by(50) {
        assert_rows_fit(800.0, height as f32, 8.0, 1.0, 1.0, 13.0, 3.5, 0.0);
    }
}

#[test]
fn test_rows_fit_zero_leading() {
    for height in (500..=2000).step_by(100) {
        assert_rows_fit(1600.0, height as f32, 16.0, 2.0, 1.0, 12.77, 3.50, 0.0);
    }
}

#[test]
fn test_rows_fit_fractional_metrics() {
    // Fractional ascent+descent that would produce different results
    // with and without ceil
    assert_rows_fit(1600.0, 1000.0, 16.0, 2.0, 1.0, 12.3, 3.4, 0.1);
    assert_rows_fit(1600.0, 1000.0, 16.0, 2.0, 1.0, 11.9, 4.6, 0.3);
    assert_rows_fit(1600.0, 1000.0, 16.0, 1.5, 1.0, 12.0, 3.0, 0.5);
}

#[test]
fn test_compute_returns_min_for_zero_dimensions() {
    let dims = TextDimensions {
        width: 16.0,
        height: 32.0,
        scale: 2.0,
    };
    let (cols, rows) = compute(0.0, 0.0, cell_for(dims), Margin::all(0.0), 2.0);
    assert_eq!(cols, MIN_COLS);
    assert_eq!(rows, MIN_LINES);
}

#[test]
fn test_compute_returns_min_for_negative_dimensions() {
    let dims = TextDimensions {
        width: 16.0,
        height: 32.0,
        scale: 2.0,
    };
    let (cols, rows) = compute(-100.0, -100.0, cell_for(dims), Margin::all(0.0), 2.0);
    assert_eq!(cols, MIN_COLS);
    assert_eq!(rows, MIN_LINES);
}

#[test]
fn test_compute_returns_min_for_zero_scale() {
    let dims = TextDimensions {
        width: 16.0,
        height: 32.0,
        scale: 0.0,
    };
    let (cols, rows) = compute(1600.0, 900.0, cell_for(dims), Margin::all(0.0), 0.0);
    assert_eq!(cols, MIN_COLS);
    assert_eq!(rows, MIN_LINES);
}

#[test]
fn test_compute_basic_grid() {
    let dims = TextDimensions {
        width: 16.0,
        height: 33.0,
        scale: 2.0,
    };
    let (cols, rows) = compute(1600.0, 825.0, cell_for(dims), Margin::all(0.0), 2.0);
    assert_eq!(cols, 100);
    assert_eq!(rows, 25);
}

#[test]
fn test_compute_floors_fractional_rows() {
    // 840px / 33px = 25.45 → floor → 25
    let dims = TextDimensions {
        width: 16.0,
        height: 33.0,
        scale: 1.0,
    };
    let (_, rows) = compute(1600.0, 840.0, cell_for(dims), Margin::all(0.0), 1.0);
    assert_eq!(rows, 25);
}

#[test]
fn test_compute_respects_margins() {
    let dims = TextDimensions {
        width: 16.0,
        height: 32.0,
        scale: 2.0,
    };
    let margin = Margin::new(0.0, 10.0, 0.0, 10.0);
    let (cols, _) = compute(1600.0, 800.0, cell_for(dims), margin, 2.0);
    // available = 1600 - 10*2 - 10*2 = 1560, cols = 1560/16 = 97
    assert_eq!(cols, 97);
}

#[test]
fn test_compute_margin_exceeds_size() {
    let dims = TextDimensions {
        width: 16.0,
        height: 32.0,
        scale: 2.0,
    };
    let margin = Margin::new(0.0, 0.0, 0.0, 1000.0);
    let (cols, rows) = compute(100.0, 800.0, cell_for(dims), margin, 2.0);
    assert_eq!(cols, MIN_COLS);
    assert_eq!(rows, MIN_LINES);
}

#[test]
fn test_context_dimension_update_scale_refreshes_scaled_font_size() {
    let dims = TextDimensions {
        width: 8.0,
        height: 16.0,
        scale: 1.0,
    };
    let cell = cell_for(dims);
    let mut context =
        ContextDimension::build(800.0, 600.0, dims, cell, 1.0, 14.0, Margin::all(10.0));

    context.update_scale(2.0);

    assert_eq!(context.dimension.scale, 2.0);
    assert_eq!(context.scaled_font_size, 28.0);
    // Width/height-derived layout is recomputed separately after the new
    // cell metrics are installed, so update_scale itself must not clobber
    // the current row/column counts.
    assert_eq!(context.columns, 97);
    assert_eq!(context.lines, 36);
}

#[test]
fn test_context_dimension_build() {
    let dims = TextDimensions {
        width: 16.0,
        height: 33.0,
        scale: 2.0,
    };
    let cd = ContextDimension::build(
        1650.0,
        825.0,
        dims,
        cell_for(dims),
        1.0,
        14.0,
        Margin::all(0.0),
    );
    assert_eq!(cd.columns, 103);
    assert_eq!(cd.lines, 25);
}

#[test]
fn test_context_dimension_update_width() {
    let dims = TextDimensions {
        width: 16.0,
        height: 33.0,
        scale: 2.0,
    };
    let mut cd = ContextDimension::build(
        1600.0,
        825.0,
        dims,
        cell_for(dims),
        1.0,
        14.0,
        Margin::all(0.0),
    );
    assert_eq!(cd.columns, 100);

    cd.update_width(800.0);
    assert_eq!(cd.columns, 50);
    assert_eq!(cd.lines, 25);
}

#[test]
fn test_context_dimension_update_height() {
    let dims = TextDimensions {
        width: 16.0,
        height: 33.0,
        scale: 2.0,
    };
    let mut cd = ContextDimension::build(
        1600.0,
        825.0,
        dims,
        cell_for(dims),
        1.0,
        14.0,
        Margin::all(0.0),
    );
    assert_eq!(cd.lines, 25);

    cd.update_height(660.0);
    assert_eq!(cd.lines, 20);
    assert_eq!(cd.columns, 100);
}

#[test]
fn test_context_dimension_update_dimensions() {
    let dims = TextDimensions {
        width: 16.0,
        height: 33.0,
        scale: 1.0,
    };
    let mut cd = ContextDimension::build(
        1600.0,
        825.0,
        dims,
        cell_for(dims),
        1.0,
        14.0,
        Margin::all(0.0),
    );
    assert_eq!(cd.lines, 25);

    let new_dims = TextDimensions {
        width: 16.0,
        height: 66.0,
        scale: 1.0,
    };
    cd.update_dimensions(new_dims, cell_for(new_dims));
    assert_eq!(cd.lines, 12); // 825/66 = 12.5 → 12
}

// Divider proportions through the grid itself: a moved divider keeps
// its ratio when the window grows or shrinks, and a split inside a
// resized pane keeps the parent's split. Both go through
// `set_panel_size` and `resize`, the code the app runs, rather than a
// hand-built taffy tree.
mod proportions {
    use super::*;
    use crate::context::{create_mock_context, ContextDimension};
    use rio_backend::config::layout::Panel;
    use rio_backend::event::{VoidListener, WindowId};

    type Grid = ContextGrid<VoidListener>;

    const WIDTH: f32 = 1200.0;
    const HEIGHT: f32 = 800.0;

    fn dimension(scale: f32) -> ContextDimension {
        let text = TextDimensions {
            scale,
            width: 18.,
            height: 9.,
        };
        ContextDimension::build(
            WIDTH,
            HEIGHT,
            text,
            cell_for(text),
            1.0,
            14.0,
            Margin::default(),
        )
    }

    fn grid(scale: f32) -> Grid {
        let ctx =
            create_mock_context(VoidListener {}, WindowId::from(0), 0, dimension(scale));
        let mut grid =
            ContextGrid::new(ctx, Margin::default(), [0.; 4], [0.; 4], Panel::default());
        grid.compute_layout().unwrap();
        grid
    }

    fn split(grid: &mut Grid, right: bool) -> NodeId {
        let id = grid.inner.len();
        let ctx = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            id,
            dimension(grid.scale),
        );
        if right {
            grid.split_right(ctx);
        } else {
            grid.split_down(ctx);
        }
        grid.current
    }

    /// A pane's extent along the row including its margins: where the
    /// dividers sit, and what a window resize keeps in proportion.
    fn width(grid: &Grid, node: NodeId) -> f32 {
        let m = grid.panel_config.margin;
        grid.inner[&node].layout_rect[2] + (m.left + m.right) * grid.scale
    }

    fn height(grid: &Grid, node: NodeId) -> f32 {
        let m = grid.panel_config.margin;
        grid.inner[&node].layout_rect[3] + (m.top + m.bottom) * grid.scale
    }

    /// The pair keeps `ratio` (first over second) within the pixel
    /// taffy rounds each size to.
    #[track_caller]
    fn assert_split(first: f32, second: f32, ratio: f32) {
        let expected = (first + second) * ratio / (1.0 + ratio);
        assert!(
            (first - expected).abs() <= 1.5,
            "ratio {ratio:.3} became {:.3}: {first} of {}, expected {expected:.1}",
            first / second,
            first + second
        );
    }

    #[track_caller]
    fn assert_px(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1.0,
            "expected {expected}px, got {actual}px"
        );
    }

    // Taffy shares negative space by the content-box basis, so without
    // the shrink weighting a padded pane kept more than its share and
    // the ratio drifted by up to 6% at scale 2 when the window halved.
    #[test]
    fn a_moved_divider_keeps_its_ratio_through_window_resizes() {
        for scale in [1.0, 2.0] {
            let mut grid = grid(scale);
            let left = grid.current;
            let right = split(&mut grid, true);
            for _ in 0..4 {
                assert!(grid.move_divider_right(40.0));
            }
            let ratio = width(&grid, left) / width(&grid, right);
            assert!(ratio > 1.5, "the divider moved: {ratio}");

            for (w, h) in [
                (WIDTH + 600.0, HEIGHT),
                (WIDTH / 2.0, HEIGHT),
                (WIDTH / 3.0, HEIGHT * 2.0),
                (WIDTH, HEIGHT),
            ] {
                grid.resize(w, h);
                assert_split(width(&grid, left), width(&grid, right), ratio);
                let avail = w
                    - grid.scaled_margin.left
                    - grid.scaled_margin.right
                    - grid.panel_config.column_gap * grid.scale;
                assert_px(width(&grid, left) + width(&grid, right), avail);
            }
        }
    }

    #[test]
    fn a_moved_horizontal_divider_keeps_its_ratio_too() {
        for scale in [1.0, 2.0] {
            let mut grid = grid(scale);
            let top = grid.current;
            let bottom = split(&mut grid, false);
            for _ in 0..4 {
                assert!(grid.move_divider_down(40.0));
            }
            let ratio = height(&grid, top) / height(&grid, bottom);
            for h in [HEIGHT * 2.0, HEIGHT / 2.0, HEIGHT] {
                grid.resize(WIDTH, h);
                assert_split(height(&grid, top), height(&grid, bottom), ratio);
            }
        }
    }

    // Two panes at 20/80, then the 80% pane is split down: the parent
    // keeps 20/80 and both halves of the column take the 80.
    #[test]
    fn a_split_inside_a_resized_pane_keeps_the_parent_split() {
        for scale in [1.0, 2.0] {
            let mut grid = grid(scale);
            let left = grid.current;
            let right = split(&mut grid, true);
            while grid.move_divider_left(40.0) {
                if width(&grid, left) / (width(&grid, left) + width(&grid, right)) < 0.2 {
                    break;
                }
            }
            let (lw, rw) = (width(&grid, left), width(&grid, right));
            assert!(
                lw < rw / 3.0,
                "the left pane is the small one: {lw} vs {rw}"
            );

            grid.current = right;
            let bottom_right = split(&mut grid, false);
            assert_px(width(&grid, left), lw);
            assert_px(width(&grid, right), rw);
            assert_px(width(&grid, bottom_right), rw);
            assert_px(height(&grid, right), height(&grid, bottom_right));

            // And the parent split still scales with the window, both
            // ways: the container took the pane's basis, and its shrink
            // weight is its own, not the padded pane's.
            grid.resize(WIDTH * 2.0, HEIGHT);
            assert_split(width(&grid, left), width(&grid, right), lw / rw);
            grid.resize(WIDTH / 2.0, HEIGHT);
            assert_split(width(&grid, left), width(&grid, right), lw / rw);
        }
    }

    // Closing the bottom pane of a column hands the column's basis back
    // to the pane left in it. That pane shrinks by its own weight, so
    // the outer split keeps its ratio when the window shrinks.
    #[test]
    fn a_pane_promoted_by_a_close_shrinks_by_its_own_weight() {
        for scale in [1.0, 2.0] {
            let mut grid = grid(scale);
            let left = grid.current;
            let right = split(&mut grid, true);
            for _ in 0..4 {
                assert!(grid.move_divider_right(40.0));
            }
            let ratio = width(&grid, left) / width(&grid, right);

            grid.current = right;
            let bottom_right = split(&mut grid, false);
            grid.current = bottom_right;
            let removed = grid.remove_current().expect("a pane was removed");
            assert_eq!(removed.rich_text_id, 2);
            assert_eq!(grid.panel_count(), 2);
            assert_split(width(&grid, left), width(&grid, right), ratio);

            grid.resize(WIDTH / 2.0, HEIGHT);
            assert_split(width(&grid, left), width(&grid, right), ratio);
            grid.resize(WIDTH * 2.0, HEIGHT);
            assert_split(width(&grid, left), width(&grid, right), ratio);
        }
    }
}

// Keyboard divider moves. They go through `resize_divider` and
// `compute_layout` directly, since `apply_taffy_layout` needs a GPU
// context only to push the new sizes to the terminals. Everything runs
// at scale 1, the default-config case the linked issues report, and at
// scale 2, where margins and padding double and taffy rounds differently.
mod divider_keys {
    use super::*;
    use crate::context::{create_mock_context, ContextDimension};
    use rio_backend::config::layout::Panel;
    use rio_backend::event::{VoidListener, WindowId};

    const SCALES: [f32; 2] = [1.0, 2.0];
    const STEP: f32 = 40.0;

    type Grid = ContextGrid<VoidListener>;

    fn floor(grid: &Grid) -> f32 {
        50.0 * grid.scale
    }

    fn step(grid: &Grid) -> f32 {
        STEP * grid.scale
    }

    fn dimension(scale: f32) -> ContextDimension {
        let text = TextDimensions {
            scale,
            width: 18.,
            height: 9.,
        };
        ContextDimension::build(
            1200.0,
            800.0,
            text,
            cell_for(text),
            1.0,
            14.0,
            Margin::default(),
        )
    }

    fn grid(scale: f32) -> Grid {
        let ctx =
            create_mock_context(VoidListener {}, WindowId::from(0), 0, dimension(scale));
        let mut grid =
            ContextGrid::new(ctx, Margin::default(), [0.; 4], [0.; 4], Panel::default());
        grid.compute_layout().unwrap();
        grid
    }

    /// Split the current panel the way `split_right`/`split_down` do,
    /// minus the sugarloaf push, and select the new panel.
    fn split(grid: &mut Grid, right: bool) -> NodeId {
        let node = if right {
            grid.try_split_right().unwrap()
        } else {
            grid.try_split_down().unwrap()
        };
        let id = grid.inner.len();
        let ctx = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            id,
            dimension(grid.scale),
        );
        grid.inner.insert(node, ContextGridItem::new(ctx));
        grid.current = node;
        grid.compute_layout().unwrap();
        node
    }

    fn width(grid: &Grid, node: NodeId) -> f32 {
        grid.inner[&node].layout_rect[2]
    }

    fn height(grid: &Grid, node: NodeId) -> f32 {
        grid.inner[&node].layout_rect[3]
    }

    /// Taffy rounds the final layout to whole pixels.
    #[track_caller]
    fn assert_px(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1.0,
            "expected {expected}px, got {actual}px"
        );
    }

    /// `resize_divider` followed by the layout pass the real path runs.
    fn nudge(grid: &mut Grid, row: bool, toward_start: bool) -> bool {
        let moved = grid.resize_divider(row, toward_start, STEP);
        grid.compute_layout().unwrap();
        moved
    }

    /// Press until the move is refused, guarding against a divider
    /// that never stops.
    fn push_to_the_floor(grid: &mut Grid, row: bool, toward_start: bool) {
        let mut moves = 0;
        while nudge(grid, row, toward_start) {
            moves += 1;
            assert!(moves < 100, "the divider never stopped");
        }
        assert!(moves > 0, "the divider did not move at all");
    }

    // #1956, #1907, #1550: with the default 2px panel margin the old
    // search measured the gap between panes, found it wider than the
    // tolerance, and every MoveDivider key did nothing.
    #[test]
    fn neighbour_is_found_with_the_default_panel_margin() {
        for scale in SCALES {
            let mut grid = grid(scale);
            let left = grid.current;
            let right = split(&mut grid, true);
            for toward_start in [true, false] {
                assert_eq!(
                    grid.divider_siblings(right, true, toward_start),
                    Some((left, right))
                );
                assert_eq!(
                    grid.divider_siblings(left, true, toward_start),
                    Some((left, right))
                );
                // No divider runs the other way in a single row.
                assert_eq!(grid.divider_siblings(right, false, toward_start), None);
            }
        }
    }

    #[test]
    fn a_single_pane_has_no_divider() {
        for scale in SCALES {
            let mut grid = grid(scale);
            assert!(!nudge(&mut grid, true, true));
            assert!(!nudge(&mut grid, false, false));
        }
    }

    // The action names where the divider goes. Left shrinks the left
    // pane by the scaled step whichever pane is current.
    #[test]
    fn left_moves_the_divider_left_from_either_side() {
        for scale in SCALES {
            let mut grid = grid(scale);
            let left = grid.current;
            let right = split(&mut grid, true);
            let (lw, rw, s) = (width(&grid, left), width(&grid, right), step(&grid));

            assert!(nudge(&mut grid, true, true));
            assert_px(width(&grid, left), lw - s);
            assert_px(width(&grid, right), rw + s);

            grid.current = left;
            assert!(nudge(&mut grid, true, true));
            assert_px(width(&grid, left), lw - 2.0 * s);
            assert_px(width(&grid, right), rw + 2.0 * s);

            assert!(nudge(&mut grid, true, false));
            assert_px(width(&grid, left), lw - s);
            // Heights are untouched by a horizontal move.
            assert_eq!(height(&grid, left), height(&grid, right));
        }
    }

    #[test]
    fn up_and_down_move_a_horizontal_divider() {
        for scale in SCALES {
            let mut grid = grid(scale);
            let top = grid.current;
            let bottom = split(&mut grid, false);
            let (th, bh, s) = (height(&grid, top), height(&grid, bottom), step(&grid));

            assert!(nudge(&mut grid, false, true));
            assert_px(height(&grid, top), th - s);
            assert_px(height(&grid, bottom), bh + s);

            assert!(nudge(&mut grid, false, false));
            assert!(nudge(&mut grid, false, false));
            assert_px(height(&grid, top), th + s);
            assert_px(height(&grid, bottom), bh - s);
            // A left/right key finds no divider in a single column.
            assert!(!nudge(&mut grid, true, true));
        }
    }

    // A pane nested in a column moves the divider between its column
    // and the neighbour, so both panes of the column resize together.
    #[test]
    fn nested_pane_moves_the_enclosing_divider() {
        for scale in SCALES {
            let mut grid = grid(scale);
            let left = grid.current;
            let top_right = split(&mut grid, true);
            let bottom_right = split(&mut grid, false);
            let s = step(&grid);
            let (lw, trw, brw) = (
                width(&grid, left),
                width(&grid, top_right),
                width(&grid, bottom_right),
            );
            assert_px(trw, brw);

            assert!(nudge(&mut grid, true, true));
            assert_px(width(&grid, left), lw - s);
            assert_px(width(&grid, top_right), trw + s);
            assert_px(width(&grid, bottom_right), brw + s);

            // Up from the bottom-right pane moves the divider inside the column only.
            let (trh, brh) = (height(&grid, top_right), height(&grid, bottom_right));
            assert!(nudge(&mut grid, false, true));
            assert_px(height(&grid, top_right), trh - s);
            assert_px(height(&grid, bottom_right), brh + s);
            assert_px(width(&grid, left), lw - s);
        }
    }

    // A | B | D, with B and D in a nested row. The key names the side:
    // from the middle pane, Left moves the divider on its left even
    // though that divider belongs to the enclosing container, and Right
    // moves the one on its right. At the window edge the only divider
    // the pane has is the one on its other side. Moving the outer
    // divider resizes the container, whose panes share the change in
    // proportion, as they do when the mouse drags that same border.
    #[test]
    fn the_key_names_the_side_in_a_row_of_three() {
        for scale in SCALES {
            let mut grid = grid(scale);
            let a = grid.current;
            let b = split(&mut grid, true);
            let d = split(&mut grid, true);
            let s = step(&grid);
            let (aw, bw, dw) = (width(&grid, a), width(&grid, b), width(&grid, d));
            let group = |grid: &Grid| width(grid, b) + width(grid, d);
            let gw = group(&grid);

            grid.current = b;
            assert!(nudge(&mut grid, true, true));
            assert_px(width(&grid, a), aw - s);
            assert_px(group(&grid), gw + s);

            assert!(nudge(&mut grid, true, false));
            assert_px(width(&grid, a), aw - s);
            assert_px(group(&grid), gw + s);
            assert!(
                width(&grid, b) > bw && width(&grid, d) < dw,
                "right from the middle shrinks the right pane"
            );

            // A has nothing on its left: Left moves the divider on its right.
            grid.current = a;
            assert!(nudge(&mut grid, true, true));
            assert_px(width(&grid, a), aw - 2.0 * s);
            assert_px(group(&grid), gw + 2.0 * s);

            // D has nothing on its right: Right moves the divider on its left.
            grid.current = d;
            let (bw3, dw3) = (width(&grid, b), width(&grid, d));
            assert!(nudge(&mut grid, true, false));
            assert_px(width(&grid, b), bw3 + s);
            assert_px(width(&grid, d), dw3 - s);
            assert_px(width(&grid, a), aw - 2.0 * s);
        }
    }

    // The same floor the mouse drag keeps: a pane stops at 50 logical
    // pixels, the last step is partial, and the next one is refused.
    #[test]
    fn a_pane_never_shrinks_under_the_minimum() {
        for scale in SCALES {
            let mut grid = grid(scale);
            let left = grid.current;
            let right = split(&mut grid, true);
            let total = width(&grid, left) + width(&grid, right);

            push_to_the_floor(&mut grid, true, true);
            assert_px(width(&grid, left), floor(&grid));
            assert_px(width(&grid, left) + width(&grid, right), total);
            assert!(!nudge(&mut grid, true, true));
            // The other way is still open.
            assert!(nudge(&mut grid, true, false));
            assert_px(width(&grid, left), floor(&grid) + step(&grid));
        }
    }

    // The floor holds for the panes inside a container, not for the
    // container: pushing a column to the right stops when its panes
    // reach the minimum width.
    #[test]
    fn panes_inside_a_column_keep_the_minimum() {
        for scale in SCALES {
            let mut grid = grid(scale);
            let left = grid.current;
            let top_right = split(&mut grid, true);
            let bottom_right = split(&mut grid, false);

            grid.current = left;
            push_to_the_floor(&mut grid, true, false);
            assert_px(width(&grid, top_right), floor(&grid));
            assert_px(width(&grid, bottom_right), floor(&grid));
            assert!(!nudge(&mut grid, true, false));
            assert!(nudge(&mut grid, true, true));
            assert_px(width(&grid, top_right), floor(&grid) + step(&grid));
        }
    }

    // A nested row shares the loss between its panes, so the push goes
    // on until the smaller of them reaches the floor, not earlier.
    #[test]
    fn panes_inside_a_row_keep_the_minimum() {
        for scale in SCALES {
            let mut grid = grid(scale);
            let a = grid.current;
            let b = split(&mut grid, true);
            let d = split(&mut grid, true);
            // Make D the smaller one first, so the floor is reached by D.
            grid.current = d;
            assert!(nudge(&mut grid, true, false));
            assert!(width(&grid, d) < width(&grid, b));

            grid.current = a;
            push_to_the_floor(&mut grid, true, false);
            assert_px(width(&grid, d), floor(&grid));
            assert!(width(&grid, b) > floor(&grid));
            assert!(!nudge(&mut grid, true, false));
        }
    }

    // Sizes live in the basis, the unit a container is laid out in too,
    // so splitting a pane that was resized does not move the divider.
    #[test]
    fn splitting_a_resized_pane_keeps_the_divider() {
        for scale in SCALES {
            let mut grid = grid(scale);
            let left = grid.current;
            let right = split(&mut grid, true);
            for _ in 0..3 {
                assert!(nudge(&mut grid, true, false));
            }
            let (lw, rw) = (width(&grid, left), width(&grid, right));

            grid.current = right;
            let bottom_right = split(&mut grid, false);
            assert_px(width(&grid, left), lw);
            assert_px(width(&grid, right), rw);
            assert_px(width(&grid, bottom_right), rw);

            grid.current = left;
            let bottom_left = split(&mut grid, false);
            assert_px(width(&grid, left), lw);
            assert_px(width(&grid, bottom_left), lw);
            assert_px(width(&grid, right), rw);
        }
    }
}

// What a relayout pushes to the panels. `apply_taffy_layout` runs for
// real here: each panel's messenger is swapped for a channel so the
// winsizes it sends can be counted.
mod relayout {
    use super::*;
    use crate::context::{create_mock_context, ContextDimension};
    use crate::event::Msg;
    use crate::messenger::Messenger;
    use corcovado::channel::Receiver;
    use rio_backend::config::layout::Panel;
    use rio_backend::event::{VoidListener, WindowId, WindowSize};

    type Grid = ContextGrid<VoidListener>;

    // 18px cells: a 4 logical px move keeps the columns, a 40px move
    // changes them.
    const CELL: f32 = 18.0;
    const SMALL: f32 = 4.0;
    const LARGE: f32 = 40.0;

    fn dimension() -> ContextDimension {
        let text = TextDimensions {
            scale: 1.0,
            width: CELL,
            height: 9.,
        };
        ContextDimension::build(
            1200.0,
            800.0,
            text,
            cell_for(text),
            1.0,
            14.0,
            Margin::default(),
        )
    }

    fn tapped_context(
        id: usize,
    ) -> (crate::context::Context<VoidListener>, Receiver<Msg>) {
        let mut ctx =
            create_mock_context(VoidListener {}, WindowId::from(0), id, dimension());
        let (tx, rx) = corcovado::channel::channel();
        ctx.messenger = Messenger::new(tx);
        (ctx, rx)
    }

    fn winsizes(rx: &Receiver<Msg>) -> Vec<WindowSize> {
        let mut out = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            if let Msg::Resize(size) = msg {
                out.push(size);
            }
        }
        out
    }

    /// A | [B | D]: three panes in a row, the right two nested. The
    /// receivers come back in that order, drained after the splits.
    fn row_of_three() -> (Grid, [NodeId; 3], [Receiver<Msg>; 3]) {
        let (ctx, rx_a) = tapped_context(0);
        let mut grid =
            ContextGrid::new(ctx, Margin::default(), [0.; 4], [0.; 4], Panel::default());
        let a = grid.current;
        let split = |grid: &mut Grid, id| {
            let node = grid.try_split_right().unwrap();
            let (ctx, rx) = tapped_context(id);
            grid.inner.insert(node, ContextGridItem::new(ctx));
            grid.current = node;
            assert!(grid.apply_taffy_layout(false));
            (node, rx)
        };
        let (b, rx_b) = split(&mut grid, 1);
        let (d, rx_d) = split(&mut grid, 2);
        for rx in [&rx_a, &rx_b, &rx_d] {
            winsizes(rx);
        }
        (grid, [a, b, d], [rx_a, rx_b, rx_d])
    }

    fn columns(grid: &Grid, node: NodeId) -> usize {
        grid.inner[&node].val.dimension.columns
    }

    fn pixel_width(grid: &Grid, node: NodeId) -> u16 {
        crate::renderer::utils::terminal_dimensions(&grid.inner[&node].val.dimension)
            .width
    }

    // Splitting sent each pane its first winsize; a relayout that moves
    // nothing sends nothing and keeps every terminal where it was.
    #[test]
    fn a_relayout_that_moves_nothing_sends_nothing() {
        let (mut grid, nodes, rxs) = row_of_three();
        let before: Vec<usize> = nodes.iter().map(|&n| columns(&grid, n)).collect();
        assert!(grid.apply_taffy_layout(false));
        assert!(grid.apply_taffy_layout(true));
        for rx in &rxs {
            assert!(winsizes(rx).is_empty());
        }
        let after: Vec<usize> = nodes.iter().map(|&n| columns(&grid, n)).collect();
        assert_eq!(before, after);
    }

    // Mid drag, a step that keeps the columns sends no winsize, so the
    // programs beside the divider get no SIGWINCH per pixel. Letting go
    // sends the final pixel size once to the two panes that moved.
    #[test]
    fn a_drag_step_that_keeps_the_columns_sends_no_winsize() {
        let (mut grid, [_a, b, d], [rx_a, rx_b, rx_d]) = row_of_three();
        let (bw, dw) = (pixel_width(&grid, b), pixel_width(&grid, d));
        let (bc, dc) = (columns(&grid, b), columns(&grid, d));

        grid.current = d;
        assert!(grid.resize_divider(true, true, SMALL));
        assert!(grid.apply_taffy_layout(true));
        assert_eq!((columns(&grid, b), columns(&grid, d)), (bc, dc));
        for rx in [&rx_a, &rx_b, &rx_d] {
            assert!(
                winsizes(rx).is_empty(),
                "a pixel-only drag step sent a winsize"
            );
        }
        assert_ne!(pixel_width(&grid, b), bw);

        grid.end_border_resize();
        assert!(
            winsizes(&rx_a).is_empty(),
            "the pane that did not move got a winsize"
        );
        let sent_b = winsizes(&rx_b);
        let sent_d = winsizes(&rx_d);
        assert_eq!(sent_b.len(), 1);
        assert_eq!(sent_d.len(), 1);
        assert_eq!(sent_b[0].width, pixel_width(&grid, b));
        assert_eq!(sent_d[0].width, pixel_width(&grid, d));
        assert_ne!(sent_d[0].width, dw);
        assert_eq!(sent_b[0].cols as usize, columns(&grid, b));

        // Nothing is pending once the final size is out.
        grid.end_border_resize();
        for rx in [&rx_a, &rx_b, &rx_d] {
            assert!(winsizes(rx).is_empty());
        }
    }

    // A drag step that changes the columns sends one winsize to each
    // pane beside the divider, with the pixel size of that moment, and
    // nothing to the pane that did not move. Letting go then has
    // nothing left to send.
    #[test]
    fn a_drag_step_that_changes_the_columns_sends_one_winsize_per_pane() {
        let (mut grid, [_a, b, d], [rx_a, rx_b, rx_d]) = row_of_three();
        let (bc, dc) = (columns(&grid, b), columns(&grid, d));

        grid.current = d;
        assert!(grid.resize_divider(true, true, LARGE));
        assert!(grid.apply_taffy_layout(true));
        assert!(columns(&grid, b) < bc && columns(&grid, d) > dc);
        assert!(winsizes(&rx_a).is_empty());
        let sent_b = winsizes(&rx_b);
        let sent_d = winsizes(&rx_d);
        assert_eq!(sent_b.len(), 1);
        assert_eq!(sent_d.len(), 1);
        assert_eq!(sent_b[0].cols as usize, columns(&grid, b));
        assert_eq!(sent_b[0].width, pixel_width(&grid, b));
        assert_eq!(sent_d[0].cols as usize, columns(&grid, d));

        grid.end_border_resize();
        for rx in [&rx_a, &rx_b, &rx_d] {
            assert!(winsizes(rx).is_empty());
        }
    }

    // A key move is a single step, so the pixel size goes out at once,
    // whether or not the columns changed.
    #[test]
    fn a_key_move_sends_the_pixel_size_at_once() {
        let (mut grid, [_a, _b, d], [rx_a, rx_b, rx_d]) = row_of_three();
        grid.current = d;
        assert!(grid.move_divider_left(SMALL));
        assert!(winsizes(&rx_a).is_empty());
        assert_eq!(winsizes(&rx_b).len(), 1);
        assert_eq!(winsizes(&rx_d).len(), 1);
    }

    // The terminal is only reflowed when its dimension changed: the
    // pane left of a nested divider keeps its grid through a drag.
    #[test]
    fn an_untouched_pane_keeps_its_grid_through_a_drag() {
        let (mut grid, [a, _b, d], _rxs) = row_of_three();
        let cols = grid.inner[&a].val.terminal.lock().grid.columns();
        grid.current = d;
        for _ in 0..3 {
            assert!(grid.resize_divider(true, true, LARGE));
            assert!(grid.apply_taffy_layout(true));
        }
        grid.end_border_resize();
        assert_eq!(grid.inner[&a].val.terminal.lock().grid.columns(), cols);
        assert_eq!(columns(&grid, a), cols);
    }

    // A pixel-only drag step must not touch the terminal: a program
    // inside a synchronized update (mode 2026) would otherwise have its
    // frame cut short on every pixel. A step that changes the columns
    // reflows, and ends the update as any resize does.
    #[test]
    fn a_pixel_only_drag_step_leaves_the_terminal_alone() {
        use rio_backend::performer::handler::Processor;
        let (mut grid, [_a, b, d], _rxs) = row_of_three();
        let mut parser = Processor::default();
        parser.advance(&mut *grid.inner[&b].val.terminal.lock(), b"\x1b[?2026h");
        assert!(grid.inner[&b].val.terminal.lock().synchronized_output());

        grid.current = d;
        assert!(grid.resize_divider(true, true, SMALL));
        assert!(grid.apply_taffy_layout(true));
        assert!(
            grid.inner[&b].val.terminal.lock().synchronized_output(),
            "a pixel-only step ended the synchronized update"
        );

        assert!(grid.resize_divider(true, true, LARGE));
        assert!(grid.apply_taffy_layout(true));
        assert!(!grid.inner[&b].val.terminal.lock().synchronized_output());
    }

    // A winsize held back by a drag is not lost if the drag never gets
    // its release: the next relayout of any kind sends it.
    #[test]
    fn a_held_back_winsize_goes_out_with_the_next_relayout() {
        let (mut grid, [_a, _b, d], [rx_a, rx_b, rx_d]) = row_of_three();
        grid.current = d;
        assert!(grid.resize_divider(true, true, SMALL));
        assert!(grid.apply_taffy_layout(true));
        for rx in [&rx_a, &rx_b, &rx_d] {
            assert!(winsizes(rx).is_empty());
        }
        assert!(grid.apply_taffy_layout(false));
        assert!(winsizes(&rx_a).is_empty());
        assert_eq!(winsizes(&rx_b).len(), 1);
        assert_eq!(winsizes(&rx_d).len(), 1);
    }
}

// Grid behaviour reachable now that the layout pass needs no GPU
// context: mouse-border hit tests and drags, separator geometry,
// visual ordering, window resizes, scale changes and pane removal.
//
// Layout rects and separators are relative to the window margin;
// pointer positions are window coordinates. `win` converts.
mod grid {
    use super::*;
    use crate::context::{create_mock_context, ContextDimension};
    use rio_backend::config::layout::Panel;
    use rio_backend::event::{VoidListener, WindowId};

    type Grid = ContextGrid<VoidListener>;

    const WIDTH: f32 = 1200.0;
    const HEIGHT: f32 = 800.0;
    /// Default panel margin, per side, in logical pixels.
    const MARGIN: f32 = 2.0;

    fn dimension(scale: f32) -> ContextDimension {
        let text = TextDimensions {
            scale,
            width: 18.,
            height: 9.,
        };
        ContextDimension::build(
            WIDTH,
            HEIGHT,
            text,
            cell_for(text),
            1.0,
            14.0,
            Margin::default(),
        )
    }

    fn context(scale: f32, id: usize) -> crate::context::Context<VoidListener> {
        create_mock_context(VoidListener {}, WindowId::from(0), id, dimension(scale))
    }

    fn grid(scale: f32) -> Grid {
        let mut grid = ContextGrid::new(
            context(scale, 0),
            Margin::default(),
            [0.; 4],
            [0.; 4],
            Panel::default(),
        );
        grid.compute_layout().unwrap();
        grid
    }

    fn split(grid: &mut Grid, right: bool) -> NodeId {
        let id = grid.inner.len();
        let ctx = context(grid.scale, id);
        if right {
            grid.split_right(ctx);
        } else {
            grid.split_down(ctx);
        }
        grid.current
    }

    /// The space the panes share, inside the window margin.
    fn available(grid: &Grid) -> (f32, f32) {
        let m = grid.scaled_margin;
        (
            grid.width - m.left - m.right,
            grid.height - m.top - m.bottom,
        )
    }

    /// A layout-relative point as a window position.
    fn win(grid: &Grid, x: f32, y: f32) -> (f32, f32) {
        (x + grid.scaled_margin.left, y + grid.scaled_margin.top)
    }

    fn rect(grid: &Grid, node: NodeId) -> [f32; 4] {
        grid.inner[&node].layout_rect
    }

    fn width(grid: &Grid, node: NodeId) -> f32 {
        rect(grid, node)[2]
    }

    #[track_caller]
    fn assert_px(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1.0,
            "expected {expected}px, got {actual}px"
        );
    }

    /// The layout x of the separator between two side-by-side panes.
    fn separator_x(grid: &Grid, left: NodeId, right: NodeId) -> f32 {
        let l = rect(grid, left);
        let r = rect(grid, right);
        ((l[0] + l[2]) + r[0]) / 2.0
    }

    fn border_between(grid: &Grid, left: NodeId, right: NodeId) -> PanelBorder {
        let (x, y) = win(
            grid,
            separator_x(grid, left, right),
            rect(grid, left)[1] + 1.0,
        );
        grid.find_border_at_position(x, y)
            .expect("the separator is under the pointer")
    }

    #[test]
    fn a_split_selects_the_new_pane_and_sizes_both() {
        for scale in [1.0, 2.0] {
            let mut grid = grid(scale);
            let left = grid.current;
            let right = split(&mut grid, true);
            assert_ne!(left, right);
            assert_eq!(grid.current, right);
            assert_eq!(grid.panel_count(), 2);
            let margins = 4.0 * MARGIN * scale;
            assert_px(width(&grid, left), (available(&grid).0 - margins) / 2.0);
            assert_px(width(&grid, right), width(&grid, left));
            // The new terminal was told its grid.
            let cols = grid.inner[&right].val.terminal.lock().grid.columns();
            assert_eq!(cols, grid.inner[&right].val.dimension.columns);
            assert_eq!(cols, grid.inner[&left].val.dimension.columns);
        }
    }

    #[test]
    fn separators_sit_between_the_panes() {
        let mut grid = grid(1.0);
        let left = grid.current;
        let right = split(&mut grid, true);
        let (_, avail_h) = available(&grid);
        let borders = grid.get_panel_borders();
        assert_eq!(borders.len(), 1);
        let b = &borders[0];
        assert_px(b.x + b.width / 2.0, separator_x(&grid, left, right));
        assert_px(b.y, MARGIN);
        assert_px(b.height, avail_h - 2.0 * MARGIN);

        // A column inside the right pane adds one horizontal separator
        // spanning only that pane.
        let bottom_right = split(&mut grid, false);
        let borders = grid.get_panel_borders();
        assert_eq!(borders.len(), 2);
        let horizontal = borders
            .iter()
            .find(|b| b.width > b.height)
            .expect("a horizontal separator");
        let r = rect(&grid, right);
        assert_px(horizontal.x, r[0]);
        assert_px(horizontal.width, r[2]);
        let top_end = r[1] + r[3];
        let bottom_start = rect(&grid, bottom_right)[1];
        assert_px(
            horizontal.y + horizontal.height / 2.0,
            (top_end + bottom_start) / 2.0,
        );
    }

    #[test]
    fn a_single_pane_draws_no_border() {
        let grid = grid(1.0);
        assert!(!grid.should_draw_borders());
        assert!(grid.get_panel_borders().is_empty());
        assert!(grid
            .find_border_at_position(WIDTH / 2.0, HEIGHT / 2.0)
            .is_none());
    }

    // The hit box is half the border width plus 3px each side, in
    // physical pixels, so it doubles at scale 2.
    #[test]
    fn the_border_hit_box_scales_with_the_display() {
        for scale in [1.0, 2.0] {
            let mut grid = grid(scale);
            let left = grid.current;
            let right = split(&mut grid, true);
            let (x, y) = win(
                &grid,
                separator_x(&grid, left, right),
                available(&grid).1 / 2.0,
            );
            let hit = grid
                .find_border_at_position(x, y)
                .expect("the separator is under the pointer");
            assert_eq!(hit.direction, BorderDirection::Vertical);
            assert_eq!((hit.left_or_top, hit.right_or_bottom), (left, right));

            let half = (1.0 + 3.0) * scale;
            assert!(grid.find_border_at_position(x + half - 0.5, y).is_some());
            assert!(grid.find_border_at_position(x + half + 0.5, y).is_none());
            assert!(grid.find_border_at_position(x - half + 0.5, y).is_some());
            assert!(grid.find_border_at_position(x - half - 0.5, y).is_none());
            // Above the top of the separator.
            assert!(grid.find_border_at_position(x, 0.0).is_none());
        }
    }

    #[test]
    fn a_mouse_drag_moves_the_border_by_the_pointer_delta() {
        for scale in [1.0, 2.0] {
            let mut grid = grid(scale);
            let left = grid.current;
            let right = split(&mut grid, true);
            let border = border_between(&grid, left, right);
            let sizes = (
                grid.get_panel_size(left, border.direction),
                grid.get_panel_size(right, border.direction),
            );
            let (lw, rw) = (width(&grid, left), width(&grid, right));
            assert_eq!(sizes, (lw, rw));

            grid.resize_border(&border, sizes, 100.0);
            assert_px(width(&grid, left), lw + 100.0);
            assert_px(width(&grid, right), rw - 100.0);

            // The delta is from the drag start, not from the last event.
            grid.resize_border(&border, sizes, 30.0);
            assert_px(width(&grid, left), lw + 30.0);
            assert_px(width(&grid, right), rw - 30.0);

            grid.resize_border(&border, sizes, -30.0);
            assert_px(width(&grid, left), lw - 30.0);
        }
    }

    // The drag stops where the smallest pane on the shrinking side
    // reaches 50 logical pixels, nested panes included, and the sum of
    // the pair is kept.
    #[test]
    fn a_mouse_drag_stops_at_the_floor_of_the_nested_panes() {
        for scale in [1.0, 2.0] {
            let mut grid = grid(scale);
            let left = grid.current;
            let right = split(&mut grid, true);
            let bottom_right = split(&mut grid, false);
            let border = border_between(&grid, left, right);
            // The pair is the left pane and the column container.
            assert_eq!(border.left_or_top, left);
            assert_ne!(border.right_or_bottom, right);
            let sizes = (
                grid.get_panel_size(border.left_or_top, border.direction),
                grid.get_panel_size(border.right_or_bottom, border.direction),
            );
            let total = width(&grid, left) + width(&grid, right);

            grid.resize_border(&border, sizes, 5000.0);
            assert_px(width(&grid, right), 50.0 * scale);
            assert_px(width(&grid, bottom_right), 50.0 * scale);
            assert_px(width(&grid, left) + width(&grid, right), total);

            grid.resize_border(&border, sizes, -5000.0);
            assert_px(width(&grid, left), 50.0 * scale);
        }
    }

    #[test]
    fn a_point_inside_a_pane_finds_that_pane() {
        let mut grid = grid(1.0);
        let left = grid.current;
        let right = split(&mut grid, true);
        let bottom_right = split(&mut grid, false);
        for node in [left, right, bottom_right] {
            let [x, y, w, h] = rect(&grid, node);
            for (px, py) in [
                (x + w / 2.0, y + h / 2.0),
                (x, y),
                (x + w - 0.5, y + h - 0.5),
            ] {
                let (wx, wy) = win(&grid, px, py);
                assert_eq!(grid.find_context_at_position(wx, wy), Some(node));
            }
        }
        assert_eq!(grid.find_context_at_position(-1.0, 10.0), None);
        assert_eq!(grid.find_context_at_position(WIDTH + 1.0, 10.0), None);
        // In the gap between two panes.
        let (gx, gy) = win(
            &grid,
            separator_x(&grid, left, right),
            available(&grid).1 / 2.0,
        );
        assert_eq!(grid.find_context_at_position(gx, gy), None);
    }

    // Visual order is top to bottom, then left to right, with wrap.
    #[test]
    fn next_and_previous_split_follow_the_visual_order() {
        let mut grid = grid(1.0);
        let a = grid.current;
        let b = split(&mut grid, true);
        let d = split(&mut grid, false); // under b
        grid.current = a;
        let c = split(&mut grid, false); // under a
                                         // Rows: a | b  then  c | d. Top row first.
        let expected = [a, b, c, d];
        grid.current = a;
        for &want in &expected[1..] {
            grid.select_next_split();
            assert_eq!(grid.current, want);
        }
        grid.select_next_split();
        assert_eq!(grid.current, a, "next wraps to the first pane");
        grid.select_prev_split();
        assert_eq!(grid.current, d, "previous wraps to the last pane");

        grid.current = d;
        assert!(!grid.select_next_split_no_loop());
        assert_eq!(grid.current, d);
        grid.current = a;
        assert!(!grid.select_prev_split_no_loop());
        assert!(grid.select_next_split_no_loop());
        assert_eq!(grid.current, b);
    }

    // Growing the window keeps the proportion of a moved divider, and
    // the panes always fill the available width. (Shrinking drifts by
    // the padding share; see the follow-up on shrink weighting.)
    #[test]
    fn a_window_resize_keeps_the_proportion_when_growing_and_fills_the_width() {
        for scale in [1.0, 2.0] {
            let mut grid = grid(scale);
            let left = grid.current;
            let right = split(&mut grid, true);
            for _ in 0..4 {
                assert!(grid.move_divider_right(40.0));
            }
            let ratio = width(&grid, left) / width(&grid, right);
            let margins = 4.0 * MARGIN * scale;

            grid.resize(WIDTH + 600.0, HEIGHT);
            let grown = width(&grid, left) / width(&grid, right);
            assert!(
                (grown - ratio).abs() / ratio < 0.01,
                "{ratio} became {grown}"
            );
            assert_px(
                width(&grid, left) + width(&grid, right),
                available(&grid).0 - margins,
            );

            grid.resize(WIDTH / 2.0, HEIGHT);
            assert_px(
                width(&grid, left) + width(&grid, right),
                available(&grid).0 - margins,
            );
            assert!(width(&grid, left) > width(&grid, right));

            grid.resize(WIDTH, HEIGHT);
            let back = width(&grid, left) / width(&grid, right);
            assert!((back - ratio).abs() / ratio < 0.01, "{ratio} became {back}");
        }
    }

    #[test]
    fn a_scale_change_rescales_margins_and_padding() {
        let mut grid = grid(1.0);
        let left = grid.current;
        let right = split(&mut grid, true);
        let avail = available(&grid).0;
        assert_px(
            width(&grid, left) + width(&grid, right),
            avail - 4.0 * MARGIN,
        );

        grid.update_scale(2.0);
        assert!(grid.apply_taffy_layout(false));
        assert_px(
            width(&grid, left) + width(&grid, right),
            avail - 4.0 * MARGIN * 2.0,
        );
    }

    #[test]
    fn only_moved_panes_are_marked_dirty() {
        let mut grid = grid(1.0);
        let a = grid.current;
        let b = split(&mut grid, true);
        let d = split(&mut grid, true);
        for node in [a, b, d] {
            grid.inner
                .get_mut(&node)
                .unwrap()
                .val
                .renderable_content
                .pending_update
                .reset();
        }
        grid.current = d;
        assert!(grid.move_divider_left(40.0));
        let dirty = |grid: &Grid, node| {
            grid.inner[&node]
                .val
                .renderable_content
                .pending_update
                .is_dirty()
        };
        assert!(
            !dirty(&grid, a),
            "the pane that did not move was marked dirty"
        );
        assert!(dirty(&grid, b));
        assert!(dirty(&grid, d));
    }

    // A reflow that keeps the rect, as a line height or font size
    // change does, still has to open the present gate on every pane,
    // or an idle terminal would keep showing the old cell metrics.
    #[test]
    fn a_reflow_in_place_marks_every_pane_dirty() {
        let mut grid = grid(1.0);
        let a = grid.current;
        let b = split(&mut grid, true);
        for node in [a, b] {
            grid.inner
                .get_mut(&node)
                .unwrap()
                .val
                .renderable_content
                .pending_update
                .reset();
        }
        let rects = [rect(&grid, a), rect(&grid, b)];
        let lines = grid.inner[&a].val.dimension.lines;

        // What ContextGrid::update_dimensions does after a font size or
        // line height change: new cell metrics on every pane.
        let taller = TextDimensions {
            scale: 1.0,
            width: 18.,
            height: 18.,
        };
        for node in [a, b] {
            grid.inner
                .get_mut(&node)
                .unwrap()
                .val
                .dimension
                .update_dimensions(taller, cell_for(taller));
        }
        assert!(grid.apply_taffy_layout(false));
        assert_eq!(
            [rect(&grid, a), rect(&grid, b)],
            rects,
            "the rects did not move"
        );
        assert!(
            grid.inner[&a].val.dimension.lines < lines,
            "the grid changed"
        );
        for node in [a, b] {
            assert!(
                grid.inner[&node]
                    .val
                    .renderable_content
                    .pending_update
                    .is_dirty(),
                "a reflowed pane was not marked dirty"
            );
            let terminal = grid.inner[&node].val.terminal.lock();
            assert_eq!(
                terminal.grid.screen_lines(),
                grid.inner[&node].val.dimension.lines
            );
        }
    }

    #[test]
    fn removing_a_pane_selects_the_next_in_visual_order_and_fills_the_space() {
        let mut grid = grid(1.0);
        let a = grid.current;
        let b = split(&mut grid, true);
        let d = split(&mut grid, true);
        let route_b = grid.inner[&b].val.route_id;
        let avail = available(&grid).0;

        grid.current = b;
        let removed = grid.remove_current().expect("a pane was removed");
        assert_eq!(removed.route_id, route_b);
        assert_eq!(
            grid.current, d,
            "the pane after the removed one is selected"
        );
        assert_eq!(grid.panel_count(), 2);
        assert!(!grid.inner.contains_key(&b));
        assert_px(width(&grid, a) + width(&grid, d), avail - 4.0 * MARGIN);
        assert_eq!(grid.get_panel_borders().len(), 1);

        // Removing the last pane in visual order selects the previous.
        grid.current = d;
        assert!(grid.remove_current().is_some());
        assert_eq!(grid.current, a);
        assert_eq!(grid.panel_count(), 1);
        assert_px(width(&grid, a), avail - 2.0 * MARGIN);
        assert!(grid.get_panel_borders().is_empty());
    }

    #[test]
    fn the_last_pane_cannot_be_removed() {
        let mut grid = grid(1.0);
        let a = grid.current;
        assert!(grid.remove_current().is_none());
        assert_eq!(grid.panel_count(), 1);
        assert_eq!(grid.current, a);
    }

    #[test]
    fn removing_by_route_keeps_the_focus_where_it_was() {
        let mut grid = grid(1.0);
        let a = grid.current;
        let b = split(&mut grid, true);
        let d = split(&mut grid, false); // under b
        let route_d = grid.inner[&d].val.route_id;
        let (_, avail_h) = available(&grid);

        grid.current = a;
        let removed = grid.remove_by_route(route_d).expect("a pane was removed");
        assert_eq!(removed.route_id, route_d);
        assert_eq!(
            grid.current, a,
            "focus stays on the pane that was not removed"
        );
        assert_eq!(grid.panel_count(), 2);
        // b takes the whole right column again.
        let [_, by, _, bh] = rect(&grid, b);
        assert_px(by, MARGIN);
        assert_px(bh, avail_h - 2.0 * MARGIN);

        assert!(grid.remove_by_route(usize::MAX).is_none());
        assert_eq!(grid.panel_count(), 2);
    }

    // Closing a nested pane collapses its container, and the divider
    // between the remaining panes still works.
    #[test]
    fn a_collapsed_container_leaves_a_working_divider() {
        let mut grid = grid(1.0);
        let a = grid.current;
        let b = split(&mut grid, true);
        let d = split(&mut grid, true);
        grid.current = d;
        assert!(grid.remove_current().is_some());
        assert_eq!(grid.current, b);
        let (aw, bw) = (width(&grid, a), width(&grid, b));
        assert!(grid.move_divider_left(40.0));
        assert_px(width(&grid, a), aw - 40.0);
        assert_px(width(&grid, b), bw + 40.0);
    }
}
