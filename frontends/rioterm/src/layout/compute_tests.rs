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

mod tmux {
    use super::super::*;
    use crate::context::create_mock_context;
    use crate::event::VoidListener;
    use rio_backend::event::WindowId;

    const CELL: u32 = 10;

    fn dimension(columns: u32, lines: u32) -> ContextDimension {
        let text = TextDimensions {
            width: CELL as f32,
            height: CELL as f32,
            scale: 1.0,
        };
        ContextDimension::build(
            (columns * CELL) as f32,
            (lines * CELL) as f32,
            text,
            super::cell_for(text),
            1.0,
            14.0,
            Margin::all(0.0),
        )
    }

    fn pane(id: u32, columns: usize, lines: usize) -> Context<VoidListener> {
        let mut context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            id as usize,
            dimension(columns as u32, lines as u32),
        );
        context.messenger = crate::messenger::Messenger::for_tmux_pane(
            context.messenger.channel.clone(),
            id,
        );
        context
    }

    fn grid(columns: u32, lines: u32, panel: Panel) -> ContextGrid<VoidListener> {
        let mut context = pane(0, columns as usize, lines as usize);
        context.dimension = dimension(columns, lines);
        ContextGrid::new(context, Margin::all(0.0), [0.0; 4], [0.0; 4], panel)
    }

    use rio_backend::config::layout::Panel;

    fn rects(grid: &mut ContextGrid<VoidListener>) -> Vec<(u32, [f32; 4])> {
        grid.calculate_positions();
        let mut rects: Vec<_> = grid
            .contexts()
            .values()
            .map(|item| (item.val.messenger.tmux_pane().unwrap(), item.layout_rect))
            .collect();
        rects.sort_by_key(|(pane, _)| *pane);
        rects
    }

    #[test]
    fn a_status_line_takes_a_row_from_tmux() {
        let mut grid = grid(100, 30, Panel::default());
        let cell = grid.current().dimension.cell.cell_height as f32;
        let margin = grid.base_margin();

        assert!(grid.set_status_room(0.0, cell));
        assert!(!grid.set_status_room(0.0, cell));
        assert_eq!(grid.tmux_client_size(), (99, 28));
        assert_eq!(grid.base_margin().bottom, margin.bottom);

        // The shared margin can change underneath it.
        let mut changed = margin;
        changed.bottom += 1.0;
        grid.update_scaled_margin(changed);
        assert_eq!(grid.scaled_margin.bottom, changed.bottom + cell);

        grid.update_scaled_margin(margin);
        assert!(grid.set_status_room(0.0, 0.0));
        assert_eq!(grid.scaled_margin.bottom, margin.bottom);
        assert_eq!(grid.tmux_client_size(), (99, 29));
    }

    #[test]
    fn panels_are_large_enough_for_their_tmux_size() {
        let mut grid = grid(100, 30, Panel::default());
        // The default panel margin costs a cell of each axis.
        assert_eq!(grid.tmux_client_size(), (99, 29));

        let layout = LayoutNode::parse(
            "0000,99x29,0,0[99x15,0,0,0,99x13,0,16{94x13,0,16,1,4x13,95,16,2}]",
        )
        .unwrap();
        let removed = grid.set_tmux_layout(&layout, Some(1), &mut pane);
        assert!(removed.is_empty());

        let rects = rects(&mut grid);
        assert_eq!(rects.len(), 3);
        for ((_, rect), (_, columns, lines)) in rects.iter().zip(layout.panes()) {
            assert!(rect[2] >= (columns as u32 * CELL) as f32, "{rect:?}");
            assert!(rect[3] >= (lines as u32 * CELL) as f32, "{rect:?}");
        }
        // Side by side below the full-width pane, in tmux's proportions.
        assert!(rects[1].1[1] > rects[0].1[1]);
        assert_eq!(rects[1].1[1], rects[2].1[1]);
        assert!(rects[2].1[0] > rects[1].1[0]);
        assert!(rects[1].1[2] > rects[2].1[2] * 2.0);
        // Nothing runs off the tab.
        assert!(rects[2].1[0] + rects[2].1[2] <= 1000.0);
    }

    #[test]
    fn active_pane_becomes_the_current_panel() {
        let layout =
            LayoutNode::parse("0000,100x30,0,0{50x30,0,0,0,49x30,51,0,1}").unwrap();
        let mut grid = grid(100, 30, Panel::default());
        grid.set_tmux_layout(&layout, Some(1), &mut pane);
        assert_eq!(grid.current().messenger.tmux_pane(), Some(1));
    }

    #[test]
    fn surviving_panes_keep_their_context() {
        let split =
            LayoutNode::parse("0000,100x30,0,0{50x30,0,0,0,49x30,51,0,1}").unwrap();
        let single = LayoutNode::parse("0000,100x30,0,0,1").unwrap();
        let mut grid = grid(100, 30, Panel::default());
        grid.set_tmux_layout(&split, Some(0), &mut pane);
        let route = grid
            .contexts()
            .values()
            .find(|item| item.val.messenger.tmux_pane() == Some(1))
            .unwrap()
            .val
            .route_id;

        let removed = grid.set_tmux_layout(&single, Some(1), &mut |_, _, _| {
            panic!("pane 1 exists already")
        });

        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].val.messenger.tmux_pane(), Some(0));
        assert_eq!(grid.len(), 1);
        assert_eq!(grid.current().route_id, route);
        assert_eq!(grid.current().tmux_size, Some((100, 30)));
    }

    #[test]
    fn dividers_are_dragged_through_the_pane_before_them() {
        let grid = tmux_grid("0000,100x30,0,0{50x30,0,0,0,49x30,51,0,1}");
        let drag = grid.tmux_divider_drag(&top_divider(&grid), 0.0).unwrap();
        assert_eq!(drag.pane, 0);
        assert_eq!(drag.direction, BorderDirection::Vertical);
    }

    #[test]
    fn a_column_before_a_divider_is_sized_through_one_of_its_panes() {
        let grid = tmux_grid(
            "0000,100x30,0,0{50x30,0,0[50x15,0,0,3,50x14,0,16,4],49x30,51,0,1}",
        );
        let drag = grid.tmux_divider_drag(&top_divider(&grid), 0.0).unwrap();
        assert_eq!(drag.pane, 3);
    }

    #[test]
    fn dividers_no_pane_can_move_are_not_draggable() {
        // Every pane left of the divider sits in a nested row, which is what
        // `resize-pane` would resize instead.
        let grid = tmux_grid(
            "0000,100x30,0,0{50x30,0,0[50x15,0,0{25x15,0,0,3,24x15,26,0,4},\
             50x14,0,16{25x14,0,16,5,24x14,26,16,6}],49x30,51,0,1}",
        );
        assert!(grid.tmux_divider_drag(&top_divider(&grid), 0.0).is_none());
    }

    #[test]
    fn dragging_asks_for_whole_cells_once_each() {
        let grid = tmux_grid("0000,100x30,0,0{50x30,0,0,0,49x30,51,0,1}");
        let mut drag = grid.tmux_divider_drag(&top_divider(&grid), 500.0).unwrap();
        let cell = grid.current().dimension.cell.cell_width as f32;

        assert_eq!(drag.drag_to(500.0), None);
        assert_eq!(drag.drag_to(500.0 + cell), Some(51));
        assert_eq!(drag.drag_to(500.0 + cell * 1.2), None);
        assert_eq!(drag.drag_to(500.0 - cell * 2.0), Some(48));
        assert_eq!(drag.drag_to(-100_000.0), Some(1));
        assert_eq!(drag.drag_to(-200_000.0), None);
    }

    #[test]
    fn hovering_finds_a_tmux_divider() {
        let grid = tmux_grid("0000,100x30,0,0{50x30,0,0,0,49x30,51,0,1}");
        let right = grid
            .contexts()
            .values()
            .find(|item| item.val.messenger.tmux_pane() == Some(1))
            .unwrap()
            .layout_rect;
        let x = right[0] - grid.panel_config.column_gap * grid.scale / 2.0;
        let border = grid.find_border_at_position(x, 10.0).unwrap();
        assert_eq!(border.direction, BorderDirection::Vertical);
    }

    fn tmux_grid(layout: &str) -> ContextGrid<VoidListener> {
        let layout = LayoutNode::parse(layout).unwrap();
        let mut grid = grid(100, 30, Panel::default());
        grid.set_tmux_layout(&layout, Some(0), &mut pane);
        grid.tmux = Some(TmuxGridState {
            host_route: 0,
            window: 0,
            layout,
            active_pane: Some(0),
        });
        grid.calculate_positions();
        grid
    }

    /// The divider between the first two children of the layout's top node.
    fn top_divider(grid: &ContextGrid<VoidListener>) -> PanelBorder {
        let top = grid.tree.children(grid.root_node).unwrap()[0];
        let children = grid.tree.children(top).unwrap();
        PanelBorder {
            direction: BorderDirection::Vertical,
            left_or_top: children[0],
            right_or_bottom: children[1],
        }
    }
}
