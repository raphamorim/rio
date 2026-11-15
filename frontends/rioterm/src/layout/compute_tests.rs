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

/// Reproduces the bug: after resizing a panel to 80%/20% and then
/// resizing the window, the panel proportions should be preserved
/// but they are not because set_panel_size uses flex_shrink: 0.0.
#[test]
fn test_panel_resize_preserves_proportions_on_window_resize() {
    use taffy::{FlexDirection, TaffyTree};

    let mut tree: TaffyTree<()> = TaffyTree::new();

    let initial_width = 1000.0_f32;

    // Root container (simulates the grid root after margin subtraction)
    let root = tree
        .new_leaf(Style {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            size: geometry::Size {
                width: length(initial_width),
                height: length(800.0_f32),
            },
            ..Default::default()
        })
        .unwrap();

    // Two panels, initially equal (flex_grow: 1.0)
    let left = tree
        .new_leaf(Style {
            display: Display::Flex,
            flex_grow: 1.0,
            flex_shrink: 1.0,
            ..Default::default()
        })
        .unwrap();
    let right = tree
        .new_leaf(Style {
            display: Display::Flex,
            flex_grow: 1.0,
            flex_shrink: 1.0,
            ..Default::default()
        })
        .unwrap();

    tree.add_child(root, left).unwrap();
    tree.add_child(root, right).unwrap();

    // Compute initial layout — should be 500/500
    tree.compute_layout(
        root,
        geometry::Size {
            width: AvailableSpace::MaxContent,
            height: AvailableSpace::MaxContent,
        },
    )
    .unwrap();
    let left_w = tree.layout(left).unwrap().size.width;
    let right_w = tree.layout(right).unwrap().size.width;
    assert!(
        (left_w - 500.0).abs() < 1.0,
        "left should be ~500, got {left_w}"
    );
    assert!(
        (right_w - 500.0).abs() < 1.0,
        "right should be ~500, got {right_w}"
    );

    // Simulate move_divider: set left to 80%, right to 20%
    // Uses flex_grow proportional to the size so panels scale on resize
    let mut left_style = tree.style(left).unwrap().clone();
    left_style.flex_basis = length(0.0_f32);
    left_style.flex_grow = 800.0;
    left_style.flex_shrink = 1.0;
    tree.set_style(left, left_style).unwrap();

    let mut right_style = tree.style(right).unwrap().clone();
    right_style.flex_basis = length(0.0_f32);
    right_style.flex_grow = 200.0;
    right_style.flex_shrink = 1.0;
    tree.set_style(right, right_style).unwrap();

    // Verify 80/20 split
    tree.compute_layout(
        root,
        geometry::Size {
            width: AvailableSpace::MaxContent,
            height: AvailableSpace::MaxContent,
        },
    )
    .unwrap();
    let left_w = tree.layout(left).unwrap().size.width;
    let right_w = tree.layout(right).unwrap().size.width;
    assert!(
        (left_w - 800.0).abs() < 1.0,
        "left should be 800, got {left_w}"
    );
    assert!(
        (right_w - 200.0).abs() < 1.0,
        "right should be 200, got {right_w}"
    );

    // Now resize the window to 1200px (simulates try_update_size)
    let new_width = 1200.0;
    let mut root_style = tree.style(root).unwrap().clone();
    root_style.size.width = length(new_width);
    tree.set_style(root, root_style).unwrap();

    tree.compute_layout(
        root,
        geometry::Size {
            width: AvailableSpace::MaxContent,
            height: AvailableSpace::MaxContent,
        },
    )
    .unwrap();

    let left_w = tree.layout(left).unwrap().size.width;
    let right_w = tree.layout(right).unwrap().size.width;

    // The 80/20 proportion should be preserved: 960/240
    let expected_left = new_width * 0.8;
    let expected_right = new_width * 0.2;

    assert!(
        (left_w - expected_left).abs() < 1.0,
        "After resize, left should be ~{expected_left} (80%), got {left_w}"
    );
    assert!(
        (right_w - expected_right).abs() < 1.0,
        "After resize, right should be ~{expected_right} (20%), got {right_w}"
    );
}

/// Reproduces bug: two panels with 20/80 split, then splitting the 80%
/// panel horizontally should keep the 20/80 proportion in the parent.
#[test]
fn test_split_inside_resized_panel_preserves_proportions() {
    use taffy::{FlexDirection, TaffyTree};

    let mut tree: TaffyTree<()> = TaffyTree::new();

    // Root container
    let root = tree
        .new_leaf(Style {
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            size: geometry::Size {
                width: length(1000.0_f32),
                height: length(800.0_f32),
            },
            ..Default::default()
        })
        .unwrap();

    // Two panels
    let left = tree
        .new_leaf(Style {
            display: Display::Flex,
            flex_grow: 1.0,
            flex_shrink: 1.0,
            ..Default::default()
        })
        .unwrap();
    let right = tree
        .new_leaf(Style {
            display: Display::Flex,
            flex_grow: 1.0,
            flex_shrink: 1.0,
            ..Default::default()
        })
        .unwrap();

    tree.add_child(root, left).unwrap();
    tree.add_child(root, right).unwrap();

    // Resize: left=20%, right=80% (using flex_grow proportional)
    let mut left_style = tree.style(left).unwrap().clone();
    left_style.flex_basis = length(0.0_f32);
    left_style.flex_grow = 200.0;
    left_style.flex_shrink = 1.0;
    tree.set_style(left, left_style).unwrap();

    let mut right_style = tree.style(right).unwrap().clone();
    right_style.flex_basis = length(0.0_f32);
    right_style.flex_grow = 800.0;
    right_style.flex_shrink = 1.0;
    tree.set_style(right, right_style).unwrap();

    // Verify 20/80 split
    let available = geometry::Size {
        width: AvailableSpace::MaxContent,
        height: AvailableSpace::MaxContent,
    };
    tree.compute_layout(root, available).unwrap();
    let left_w = tree.layout(left).unwrap().size.width;
    let right_w = tree.layout(right).unwrap().size.width;
    assert!(
        (left_w - 200.0).abs() < 1.0,
        "left should be 200, got {left_w}"
    );
    assert!(
        (right_w - 800.0).abs() < 1.0,
        "right should be 800, got {right_w}"
    );

    // Now split the right panel horizontally (Column direction).
    // This simulates what split_panel does:
    // 1. Create container inheriting right's flex properties
    // 2. Reset right to flex_grow: 1.0
    // 3. Create new panel with flex_grow: 1.0
    // 4. Move right into container, add new panel

    let right_inherited = tree.style(right).unwrap().clone();
    let container = tree
        .new_leaf(Style {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            flex_basis: right_inherited.flex_basis,
            flex_grow: right_inherited.flex_grow,
            flex_shrink: right_inherited.flex_shrink,
            ..Default::default()
        })
        .unwrap();

    // Reset right panel to flexible inside container
    let mut reset_right = right_inherited;
    reset_right.flex_basis = taffy::Dimension::auto();
    reset_right.flex_grow = 1.0;
    reset_right.flex_shrink = 1.0;
    tree.set_style(right, reset_right).unwrap();

    let bottom = tree
        .new_leaf(Style {
            display: Display::Flex,
            flex_grow: 1.0,
            flex_shrink: 1.0,
            ..Default::default()
        })
        .unwrap();

    tree.remove_child(root, right).unwrap();
    tree.add_child(container, right).unwrap();
    tree.add_child(container, bottom).unwrap();
    tree.add_child(root, container).unwrap();

    tree.compute_layout(root, available).unwrap();

    // The container (replacing right) should still be ~800px wide (80%)
    let container_w = tree.layout(container).unwrap().size.width;
    assert!(
        (container_w - 800.0).abs() < 1.0,
        "Container should keep 80% (800px), got {container_w}"
    );

    // Left should still be ~200px (20%)
    let left_w = tree.layout(left).unwrap().size.width;
    assert!(
        (left_w - 200.0).abs() < 1.0,
        "Left should keep 20% (200px), got {left_w}"
    );

    // The two children inside the container should each be ~400px tall (50/50)
    let right_h = tree.layout(right).unwrap().size.height;
    let bottom_h = tree.layout(bottom).unwrap().size.height;
    assert!(
        (right_h - 400.0).abs() < 1.0,
        "Right (top half) should be ~400px tall, got {right_h}"
    );
    assert!(
        (bottom_h - 400.0).abs() < 1.0,
        "Bottom (bottom half) should be ~400px tall, got {bottom_h}"
    );
}

// Keyboard divider moves. They go through `resize_divider` and
// `compute_layout` directly, since `apply_taffy_layout` needs a GPU
// context only to push the new sizes to the terminals.
mod divider_keys {
    use super::*;
    use crate::context::{create_mock_context, ContextDimension};
    use rio_backend::config::layout::Panel;
    use rio_backend::event::{VoidListener, WindowId};

    const SCALE: f32 = 2.0;
    const MIN: f32 = 50.0 * SCALE;

    fn dimension() -> ContextDimension {
        let text = TextDimensions {
            scale: SCALE,
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

    fn grid(panel: Panel) -> ContextGrid<VoidListener> {
        let ctx = create_mock_context(VoidListener {}, WindowId::from(0), 0, dimension());
        let mut grid = ContextGrid::new(ctx, Margin::default(), [0.; 4], [0.; 4], panel);
        grid.compute_layout().unwrap();
        grid
    }

    /// Split the current panel the way `split_right`/`split_down` do,
    /// minus the sugarloaf push, and select the new panel.
    fn split(grid: &mut ContextGrid<VoidListener>, right: bool) -> NodeId {
        let node = if right {
            grid.try_split_right().unwrap()
        } else {
            grid.try_split_down().unwrap()
        };
        let id = grid.inner.len();
        let ctx =
            create_mock_context(VoidListener {}, WindowId::from(0), id, dimension());
        grid.inner.insert(node, ContextGridItem::new(ctx));
        grid.current = node;
        grid.compute_layout().unwrap();
        node
    }

    fn rect(grid: &ContextGrid<VoidListener>, node: NodeId) -> [f32; 4] {
        grid.inner[&node].layout_rect
    }

    fn width(grid: &ContextGrid<VoidListener>, node: NodeId) -> f32 {
        rect(grid, node)[2]
    }

    fn height(grid: &ContextGrid<VoidListener>, node: NodeId) -> f32 {
        rect(grid, node)[3]
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
    fn nudge(
        grid: &mut ContextGrid<VoidListener>,
        row: bool,
        toward_start: bool,
    ) -> bool {
        let moved = grid.resize_divider(row, toward_start, 40.0);
        grid.compute_layout().unwrap();
        moved
    }

    // #1956, #1907, #1550: with the default 2px panel margin the old
    // search measured the gap between panes, found it wider than the
    // tolerance, and every MoveDivider key did nothing.
    #[test]
    fn neighbour_is_found_with_the_default_panel_margin() {
        let mut grid = grid(Panel::default());
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

    #[test]
    fn a_single_pane_has_no_divider() {
        let mut grid = grid(Panel::default());
        assert!(!nudge(&mut grid, true, true));
        assert!(!nudge(&mut grid, false, false));
    }

    // The action names where the divider goes. Left shrinks the left
    // pane by the scaled amount whichever pane is current.
    #[test]
    fn left_moves_the_divider_left_from_either_side() {
        let mut grid = grid(Panel::default());
        let left = grid.current;
        let right = split(&mut grid, true);
        let (lw, rw) = (width(&grid, left), width(&grid, right));

        assert!(nudge(&mut grid, true, true));
        assert_px(width(&grid, left), lw - 40.0 * SCALE);
        assert_px(width(&grid, right), rw + 40.0 * SCALE);

        grid.current = left;
        assert!(nudge(&mut grid, true, true));
        assert_px(width(&grid, left), lw - 80.0 * SCALE);
        assert_px(width(&grid, right), rw + 80.0 * SCALE);

        assert!(nudge(&mut grid, true, false));
        assert_px(width(&grid, left), lw - 40.0 * SCALE);
        // Heights are untouched by a horizontal move.
        assert_px(height(&grid, left), height(&grid, right));
    }

    #[test]
    fn up_and_down_move_a_horizontal_divider() {
        let mut grid = grid(Panel::default());
        let top = grid.current;
        let bottom = split(&mut grid, false);
        let (th, bh) = (height(&grid, top), height(&grid, bottom));

        assert!(nudge(&mut grid, false, true));
        assert_px(height(&grid, top), th - 40.0 * SCALE);
        assert_px(height(&grid, bottom), bh + 40.0 * SCALE);

        assert!(nudge(&mut grid, false, false));
        assert!(nudge(&mut grid, false, false));
        assert_px(height(&grid, top), th + 40.0 * SCALE);
        assert_px(height(&grid, bottom), bh - 40.0 * SCALE);
        // A left/right key finds no divider in a single column.
        assert!(!nudge(&mut grid, true, true));
    }

    // A pane nested in a column moves the divider between its column
    // and the neighbour, so both panes of the column resize together.
    #[test]
    fn nested_pane_moves_the_enclosing_divider() {
        let mut grid = grid(Panel::default());
        let left = grid.current;
        let top_right = split(&mut grid, true);
        let bottom_right = split(&mut grid, false);
        let (lw, trw, brw) = (
            width(&grid, left),
            width(&grid, top_right),
            width(&grid, bottom_right),
        );
        assert_px(trw, brw);

        assert!(nudge(&mut grid, true, true));
        assert_px(width(&grid, left), lw - 40.0 * SCALE);
        assert_px(width(&grid, top_right), trw + 40.0 * SCALE);
        assert_px(width(&grid, bottom_right), brw + 40.0 * SCALE);

        // Up from the bottom-right pane moves the divider inside the column only.
        let (trh, brh) = (height(&grid, top_right), height(&grid, bottom_right));
        assert!(nudge(&mut grid, false, true));
        assert_px(height(&grid, top_right), trh - 40.0 * SCALE);
        assert_px(height(&grid, bottom_right), brh + 40.0 * SCALE);
        assert_px(width(&grid, left), lw - 40.0 * SCALE);
    }

    // The same floor the mouse drag keeps: a pane stops at 50 logical
    // pixels, the last step is partial, and the next one is refused.
    #[test]
    fn a_pane_never_shrinks_under_the_minimum() {
        let mut grid = grid(Panel::default());
        let left = grid.current;
        let right = split(&mut grid, true);
        let total = width(&grid, left) + width(&grid, right);

        let mut moves = 0;
        while nudge(&mut grid, true, true) {
            moves += 1;
            assert!(moves < 100, "the divider never stopped");
        }
        assert_px(width(&grid, left), MIN);
        assert_px(width(&grid, left) + width(&grid, right), total);
        assert!(!nudge(&mut grid, true, true));
        // The other way is still open.
        assert!(nudge(&mut grid, true, false));
        assert_px(width(&grid, left), MIN + 40.0 * SCALE);
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
        let mut grid = grid(Panel::default());
        let a = grid.current;
        let b = split(&mut grid, true);
        let d = split(&mut grid, true);
        let (aw, bw, dw) = (width(&grid, a), width(&grid, b), width(&grid, d));
        let group = |grid: &ContextGrid<VoidListener>| width(grid, b) + width(grid, d);
        let gw = group(&grid);

        grid.current = b;
        assert!(nudge(&mut grid, true, true));
        assert_px(width(&grid, a), aw - 40.0 * SCALE);
        assert_px(group(&grid), gw + 40.0 * SCALE);

        assert!(nudge(&mut grid, true, false));
        assert_px(width(&grid, a), aw - 40.0 * SCALE);
        assert_px(group(&grid), gw + 40.0 * SCALE);
        let (bw2, dw2) = (width(&grid, b), width(&grid, d));
        assert!(
            bw2 > bw && dw2 < dw,
            "right from the middle shrinks the right pane"
        );

        // A has nothing on its left: Left moves the divider on its right.
        grid.current = a;
        assert!(nudge(&mut grid, true, true));
        assert_px(width(&grid, a), aw - 80.0 * SCALE);
        assert_px(group(&grid), gw + 80.0 * SCALE);

        // D has nothing on its right: Right moves the divider on its left.
        grid.current = d;
        let (bw3, dw3) = (width(&grid, b), width(&grid, d));
        assert!(nudge(&mut grid, true, false));
        assert_px(width(&grid, b), bw3 + 40.0 * SCALE);
        assert_px(width(&grid, d), dw3 - 40.0 * SCALE);
        assert_px(width(&grid, a), aw - 80.0 * SCALE);
    }

    // The floor holds for the panes inside a container, not for the
    // container: pushing a column to the right stops when its panes
    // reach the minimum width.
    #[test]
    fn panes_inside_a_column_keep_the_minimum() {
        let mut grid = grid(Panel::default());
        let left = grid.current;
        let top_right = split(&mut grid, true);
        let bottom_right = split(&mut grid, false);

        grid.current = left;
        let mut moves = 0;
        while nudge(&mut grid, true, false) {
            moves += 1;
            assert!(moves < 100, "the divider never stopped");
        }
        assert!(moves > 0);
        assert_px(width(&grid, top_right), MIN);
        assert_px(width(&grid, bottom_right), MIN);
        assert!(!nudge(&mut grid, true, false));
        assert!(nudge(&mut grid, true, true));
        assert_px(width(&grid, top_right), MIN + 40.0 * SCALE);
    }
}
