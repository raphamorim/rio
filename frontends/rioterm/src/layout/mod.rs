#[cfg(test)]
mod compute_tests;

use crate::context::Context;
use crate::mouse::Mouse;
use rio_backend::config::layout::Margin;
use rio_backend::crosswords::grid::Dimensions;
use rio_backend::event::EventListener;
use rio_backend::sugarloaf::{layout::TextDimensions, Rect, Sugarloaf};
use rustc_hash::FxHashMap;

use taffy::{
    geometry, style_helpers::length, AvailableSpace, Display, NodeId, Style, TaffyError,
    TaffyTree,
};

const MIN_COLS: usize = 2;
const MIN_LINES: usize = 1;

/// Direction of a draggable panel border
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BorderDirection {
    /// Border between left/right panels (drag horizontally)
    Vertical,
    /// Border between top/bottom panels (drag vertically)
    Horizontal,
}

/// Describes a draggable border between two panels
#[derive(Debug, Clone, Copy)]
pub struct PanelBorder {
    pub direction: BorderDirection,
    pub left_or_top: NodeId,
    pub right_or_bottom: NodeId,
}

/// What a removed panel held in sugarloaf, for the caller to release.
#[must_use = "release it with RemovedPanel::release"]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemovedPanel {
    pub rich_text_id: usize,
    pub route_id: usize,
}

impl RemovedPanel {
    /// Drop the image overlays and route images the panel owned;
    /// sugarloaf keeps no other per-panel state.
    pub fn release(self, sugarloaf: &mut Sugarloaf) {
        sugarloaf.clear_image_overlays_for(self.rich_text_id);
        sugarloaf.remove_route_images(self.route_id);
    }
}

/// Active resize drag state
#[derive(Debug, Clone, Copy)]
pub struct ResizeState {
    pub border: PanelBorder,
    /// Mouse position at drag start (physical pixels)
    pub start_pos: f32,
    /// Original sizes of the two panels at drag start
    pub original_sizes: (f32, f32),
}

fn compute(
    width: f32,
    height: f32,
    cell: rio_backend::sugarloaf::layout::CellMetrics,
    margin: Margin,
    scale: f32,
) -> (usize, usize) {
    // Ensure we have positive dimensions
    if width <= 0.0
        || height <= 0.0
        || scale <= 0.0
        || cell.cell_width == 0
        || cell.cell_height == 0
    {
        return (MIN_COLS, MIN_LINES);
    }

    // Calculate available space accounting for margins (scale margins to physical pixels)
    let available_width = width - (margin.left * scale) - (margin.right * scale);
    let available_height = height - (margin.top * scale) - (margin.bottom * scale);

    // Ensure we have positive available space
    if available_width <= 0.0 || available_height <= 0.0 {
        return (MIN_COLS, MIN_LINES);
    }

    // Cols/rows divide by the canonical integer cell stride
    // (`Metrics.cell_width / cell_height`). Same value the grid
    // shader uses for `cell_size` and the mouse-hit-test divides by;
    // single source of truth so the right/bottom edges of the grid
    // stay aligned with the painted cells.
    let cell_w = cell.cell_width as f32;
    let cell_h = cell.cell_height as f32;
    let visible_columns = std::cmp::max((available_width / cell_w) as usize, MIN_COLS);
    let visible_lines =
        std::cmp::max((available_height / cell_h).floor() as usize, MIN_LINES);

    (visible_columns, visible_lines)
}

/// Whether a flex direction lays children out left to right, the axis
/// a vertical divider sits across.
fn is_row_direction(direction: taffy::FlexDirection) -> bool {
    matches!(
        direction,
        taffy::FlexDirection::Row | taffy::FlexDirection::RowReverse
    )
}

/// `basis` moved by `delta` when it is a length; auto and percent
/// bases are left alone.
fn basis_plus(basis: taffy::Dimension, delta: f32) -> taffy::Dimension {
    match basis.into_option() {
        Some(value) => length(value + delta),
        None => basis,
    }
}

#[inline]
fn create_border(color: [f32; 4], position: [f32; 2], size: [f32; 2]) -> Rect {
    Rect::new(position[0], position[1], size[0], size[1], color)
}

/// Separator configuration for split panels
#[derive(Debug, Clone, Copy)]
pub struct BorderConfig {
    pub width: f32,
    pub color: [f32; 4],
}

impl Default for BorderConfig {
    fn default() -> Self {
        Self {
            width: 2.0,
            color: [0.8, 0.8, 0.8, 1.0],
        }
    }
}

pub struct ContextGrid<T: EventListener> {
    pub width: f32,
    pub height: f32,
    pub current: NodeId,
    pub scaled_margin: Margin,
    // custom_title has priority over the active panel's computed title.
    pub custom_title: Option<String>,
    // custom_color is the tab's background override (tab color picker).
    pub custom_color: Option<[f32; 4]>,
    // bell is set when a pane in this tab rang while the tab was in the
    // background; cleared once the tab is focused again.
    pub bell: bool,
    scale: f32,
    inner: FxHashMap<NodeId, ContextGridItem<T>>,
    pub root: Option<NodeId>,
    panel_config: rio_backend::config::layout::Panel,
    tree: TaffyTree<()>,
    root_node: NodeId,
    border_config: BorderConfig,
}

pub struct ContextGridItem<T: EventListener> {
    pub val: Context<T>,
    pub layout_rect: [f32; 4],
    /// The rect the panel was last laid out at, so a relayout only
    /// touches the panels it moved.
    applied_rect: [f32; 4],
    /// The grid last pushed into the terminal: columns, lines and cell
    /// size, the only things its reflow depends on.
    applied_grid: Option<GridKey>,
    /// A pixel-only winsize held back during a divider drag.
    winsize_pending: bool,
}

/// What a terminal's reflow depends on.
type GridKey = (usize, usize, u32, u32);

fn grid_key(dimension: &ContextDimension) -> GridKey {
    (
        dimension.columns(),
        dimension.screen_lines(),
        dimension.square_width().to_bits(),
        dimension.square_height().to_bits(),
    )
}

impl<T: rio_backend::event::EventListener> ContextGridItem<T> {
    pub fn new(context: Context<T>) -> Self {
        Self {
            val: context,
            layout_rect: [0.0; 4],
            applied_rect: [0.0; 4],
            applied_grid: None,
            winsize_pending: false,
        }
    }

    /// Push the panel's layout into its terminal and PTY.
    ///
    /// A panel whose rect did not move is left alone. One whose grid
    /// changed is reflowed and told its new winsize. One whose size
    /// changed only in pixels keeps its grid and gets the winsize now,
    /// or, `mid_drag`, once the pointer lets go: the pixel fields reach
    /// the child as a SIGWINCH, and a drag would otherwise raise one
    /// per pixel in both panels beside the divider.
    fn push_layout(&mut self, mid_drag: bool) {
        let moved = self.applied_rect != self.layout_rect;
        let resized = self.applied_rect[2..] != self.layout_rect[2..];
        self.applied_rect = self.layout_rect;
        let key = grid_key(&self.val.dimension);
        let reflowed = self.applied_grid != Some(key);
        if moved || reflowed {
            // The present gate reads `pending_update.is_dirty()` before
            // it looks at the terminal, so a panel that moved or was
            // reflowed in place (a font size change keeps the rect) has
            // to be marked or an idle terminal would not present.
            self.val.renderable_content.pending_update.set_dirty();
        }

        if reflowed {
            self.applied_grid = Some(key);
            let mut terminal = self.val.terminal.lock();
            terminal.end_sync_update();
            terminal.resize::<ContextDimension>(self.val.dimension);
            drop(terminal);
            self.send_winsize();
        } else if resized || self.winsize_pending {
            if mid_drag {
                self.winsize_pending = true;
            } else {
                self.send_winsize();
            }
        }
    }

    fn send_winsize(&mut self) {
        let winsize = crate::renderer::utils::terminal_dimensions(&self.val.dimension);
        let _ = self.val.messenger.send_resize(winsize);
        self.winsize_pending = false;
    }

    /// Send a winsize held back during a drag.
    fn flush_winsize(&mut self) {
        if self.winsize_pending {
            self.send_winsize();
        }
    }

    #[inline]
    pub fn context(&self) -> &Context<T> {
        &self.val
    }

    #[inline]
    pub fn context_mut(&mut self) -> &mut Context<T> {
        &mut self.val
    }

    /// Previously stashed panel position into the rich-text object's
    /// render_data; that object tree is gone with the Content drop.
    /// The grid renderer reads panel positions directly from
    /// `layout_rect`.
    fn set_position(&mut self, _position: [f32; 2]) {}
}

impl<T: rio_backend::event::EventListener> ContextGrid<T> {
    pub fn new(
        context: Context<T>,
        scaled_margin: Margin,
        border_color: [f32; 4],
        _border_active_color: [f32; 4],
        panel_config: rio_backend::config::layout::Panel,
    ) -> Self {
        let width = context.dimension.width;
        let height = context.dimension.height;
        let scale = context.dimension.dimension.scale;

        let mut tree: TaffyTree<()> = TaffyTree::new();

        // Calculate available size after window margin (already scaled)
        let available_width = width - scaled_margin.left - scaled_margin.right;
        let available_height = height - scaled_margin.top - scaled_margin.bottom;

        // Create root container (window margin handled separately via position offset)
        let root_style = Style {
            display: Display::Flex,
            gap: geometry::Size {
                width: length(panel_config.column_gap * scale),
                height: length(panel_config.row_gap * scale),
            },
            size: geometry::Size {
                width: length(available_width),
                height: length(available_height),
            },
            ..Default::default()
        };

        let root_node = tree
            .new_leaf(root_style)
            .expect("Failed to create root node");

        let panel_style = Style {
            display: Display::Flex,
            flex_grow: 1.0,
            flex_shrink: 1.0,
            padding: geometry::Rect {
                left: length(panel_config.padding.left * scale),
                right: length(panel_config.padding.right * scale),
                top: length(panel_config.padding.top * scale),
                bottom: length(panel_config.padding.bottom * scale),
            },
            margin: geometry::Rect {
                left: length(panel_config.margin.left * scale),
                right: length(panel_config.margin.right * scale),
                top: length(panel_config.margin.top * scale),
                bottom: length(panel_config.margin.bottom * scale),
            },
            ..Default::default()
        };

        let panel_node = tree
            .new_leaf(panel_style)
            .expect("Failed to create panel node");
        tree.add_child(root_node, panel_node)
            .expect("Failed to add child");

        // Use NodeId as the key
        let mut inner = FxHashMap::default();
        inner.insert(panel_node, ContextGridItem::new(context));

        let border_config = BorderConfig {
            width: panel_config.border_width,
            color: border_color,
        };

        let mut grid = Self {
            inner,
            current: panel_node,
            scaled_margin,
            custom_title: None,
            custom_color: None,
            bell: false,
            scale,
            width,
            height,
            root: Some(panel_node),
            panel_config,
            tree,
            root_node,
            border_config,
        };
        grid.calculate_positions();
        grid
    }

    #[inline]
    #[allow(dead_code)]
    pub fn get_mut(&mut self, key: NodeId) -> Option<&mut ContextGridItem<T>> {
        self.inner.get_mut(&key)
    }

    /// Get item by route_id (used for event routing)
    #[inline]
    pub fn get_by_route_id(
        &mut self,
        route_id: usize,
    ) -> Option<&mut ContextGridItem<T>> {
        self.inner
            .values_mut()
            .find(|item| item.val.route_id == route_id)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn panel_count(&self) -> usize {
        self.inner.len()
    }

    pub fn should_draw_borders(&self) -> bool {
        self.panel_count() > 1
    }

    fn try_update_size(&mut self, width: f32, height: f32) -> Result<(), TaffyError> {
        // Subtract window margin from available size
        let available_width = width - self.scaled_margin.left - self.scaled_margin.right;
        let available_height =
            height - self.scaled_margin.top - self.scaled_margin.bottom;

        let mut style = self.tree.style(self.root_node)?.clone();
        style.size = geometry::Size {
            width: length(available_width),
            height: length(available_height),
        };
        self.tree.set_style(self.root_node, style)?;
        Ok(())
    }

    fn compute_layout(&mut self) -> Result<(), TaffyError> {
        let available = geometry::Size {
            width: AvailableSpace::MaxContent,
            height: AvailableSpace::MaxContent,
        };
        self.tree.compute_layout(self.root_node, available)?;
        self.update_layout_rects();
        Ok(())
    }

    /// Update layout_rect for all panel items in a single top-down traversal.
    /// O(n) where n is total nodes in tree.
    fn update_layout_rects(&mut self) {
        let mut stack: Vec<(NodeId, f32, f32)> = vec![(self.root_node, 0.0, 0.0)];

        while let Some((node, parent_x, parent_y)) = stack.pop() {
            let layout = match self.tree.layout(node) {
                Ok(l) => l,
                Err(_) => continue,
            };

            let abs_x = parent_x + layout.location.x;
            let abs_y = parent_y + layout.location.y;

            // Update layout_rect if this node is a panel (exists in inner)
            if let Some(item) = self.inner.get_mut(&node) {
                item.layout_rect = [abs_x, abs_y, layout.size.width, layout.size.height];
            }

            // Add children to stack
            if let Ok(children) = self.tree.children(node) {
                for child in children {
                    stack.push((child, abs_x, abs_y));
                }
            }
        }
    }

    pub fn find_context_at_position(&self, x: f32, y: f32) -> Option<NodeId> {
        // Adjust for window margin - layout_rect is relative to root container
        let adj_x = x - self.scaled_margin.left;
        let adj_y = y - self.scaled_margin.top;

        for (&node_id, item) in &self.inner {
            let [left, top, width, height] = item.layout_rect;
            if adj_x >= left
                && adj_x < left + width
                && adj_y >= top
                && adj_y < top + height
            {
                return Some(node_id);
            }
        }
        None
    }

    /// Find a draggable border near the given mouse position (physical pixels).
    /// Returns None if no border is within the hit threshold.
    pub fn find_border_at_position(&self, x: f32, y: f32) -> Option<PanelBorder> {
        if self.inner.len() <= 1 {
            return None;
        }

        let adj_x = x - self.scaled_margin.left;
        let adj_y = y - self.scaled_margin.top;
        let hit_half = (self.border_config.width / 2.0 + 3.0) * self.scale;

        self.walk_separators(|dir, center, span, child_a, child_b| {
            let hit = match dir {
                BorderDirection::Vertical => {
                    (adj_x - center).abs() < hit_half
                        && adj_y >= span[0]
                        && adj_y <= span[1]
                }
                BorderDirection::Horizontal => {
                    (adj_y - center).abs() < hit_half
                        && adj_x >= span[0]
                        && adj_x <= span[1]
                }
            };
            if hit {
                Some(PanelBorder {
                    direction: dir,
                    left_or_top: child_a,
                    right_or_bottom: child_b,
                })
            } else {
                None
            }
        })
    }

    /// Get the current size of a node along the relevant axis for a border direction.
    /// Works for both panel leaves and container nodes.
    pub fn get_panel_size(&self, node: NodeId, direction: BorderDirection) -> f32 {
        if let Ok(layout) = self.tree.layout(node) {
            match direction {
                BorderDirection::Vertical => layout.size.width,
                BorderDirection::Horizontal => layout.size.height,
            }
        } else {
            0.0
        }
    }

    /// Resize two adjacent panels by moving their shared border.
    /// `delta` is in physical pixels (positive = right/down).
    pub fn resize_border(
        &mut self,
        border: &PanelBorder,
        original_sizes: (f32, f32),
        delta: f32,
    ) {
        // The border moves until a panel on either side, nested or not,
        // reaches the floor. The floor is a size, so it holds whether
        // the sizes come from the drag start or from the current layout.
        let (a, b) = (border.left_or_top, border.right_or_bottom);
        let row = border.direction == BorderDirection::Vertical;
        let floor = |grid: &Self, node| {
            grid.get_panel_size(node, border.direction) - grid.shrink_room(node, row)
        };
        let lowest = floor(self, a) - original_sizes.0;
        let highest = original_sizes.1 - floor(self, b);
        let delta = if lowest <= highest {
            delta.clamp(lowest, highest)
        } else {
            0.0
        };

        let new_a = original_sizes.0 + delta;
        let new_b = original_sizes.1 - delta;

        match border.direction {
            BorderDirection::Vertical => {
                let _ = self.set_panel_size(border.left_or_top, Some(new_a), None);
                let _ = self.set_panel_size(border.right_or_bottom, Some(new_b), None);
            }
            BorderDirection::Horizontal => {
                let _ = self.set_panel_size(border.left_or_top, None, Some(new_a));
                let _ = self.set_panel_size(border.right_or_bottom, None, Some(new_b));
            }
        }

        self.apply_taffy_layout(true);
    }

    /// Get separator lines between adjacent panels for rendering.
    pub fn get_panel_borders(&self) -> Vec<Rect> {
        if !self.should_draw_borders() {
            return vec![];
        }

        let mut separators = Vec::new();
        let border_width = self.border_config.width;
        let color = self.border_config.color;

        self.walk_separators(|dir, center, span, _child_a, _child_b| -> Option<()> {
            match dir {
                BorderDirection::Vertical => {
                    separators.push(create_border(
                        color,
                        [center - border_width / 2.0, span[0]],
                        [border_width, span[1] - span[0]],
                    ));
                }
                BorderDirection::Horizontal => {
                    separators.push(create_border(
                        color,
                        [span[0], center - border_width / 2.0],
                        [span[1] - span[0], border_width],
                    ));
                }
            }
            None // continue walking
        });

        separators
    }

    /// Walk the taffy tree visiting every separator between sibling nodes.
    ///
    /// For each separator, calls `visitor(direction, center, [span_min, span_max], child_a, child_b)`.
    /// - `center`: the main-axis midpoint of the gap (x for vertical, y for horizontal)
    /// - `span`: the cross-axis extent [min, max]
    /// - `child_a`/`child_b`: the two sibling NodeIds (left/top, right/bottom)
    ///
    /// If the visitor returns `Some(R)`, the walk stops and returns that value.
    fn walk_separators<R>(
        &self,
        mut visitor: impl FnMut(BorderDirection, f32, [f32; 2], NodeId, NodeId) -> Option<R>,
    ) -> Option<R> {
        let mut stack: Vec<(NodeId, f32, f32)> = vec![(self.root_node, 0.0, 0.0)];

        while let Some((node, parent_x, parent_y)) = stack.pop() {
            let children = match self.tree.children(node) {
                Ok(c) => c,
                _ => continue,
            };

            let node_layout = match self.tree.layout(node) {
                Ok(l) => l,
                Err(_) => continue,
            };
            let abs_x = parent_x + node_layout.location.x;
            let abs_y = parent_y + node_layout.location.y;

            for &child in &children {
                stack.push((child, abs_x, abs_y));
            }

            if children.len() < 2 {
                continue;
            }

            let is_row = match self.tree.style(node) {
                Ok(s) => is_row_direction(s.flex_direction),
                Err(_) => continue,
            };

            for i in 0..children.len() - 1 {
                let la = match self.tree.layout(children[i]) {
                    Ok(l) => l,
                    Err(_) => continue,
                };
                let lb = match self.tree.layout(children[i + 1]) {
                    Ok(l) => l,
                    Err(_) => continue,
                };

                if is_row {
                    let (left, right, left_id, right_id) =
                        if la.location.x < lb.location.x {
                            (la, lb, children[i], children[i + 1])
                        } else {
                            (lb, la, children[i + 1], children[i])
                        };
                    let left_edge = abs_x + left.location.x + left.size.width;
                    let right_start = abs_x + right.location.x;
                    let center = (left_edge + right_start) / 2.0;
                    let min_y = abs_y + left.location.y.min(right.location.y);
                    let max_y = abs_y
                        + (left.location.y + left.size.height)
                            .max(right.location.y + right.size.height);

                    if let Some(r) = visitor(
                        BorderDirection::Vertical,
                        center,
                        [min_y, max_y],
                        left_id,
                        right_id,
                    ) {
                        return Some(r);
                    }
                } else {
                    let (top, bottom, top_id, bottom_id) =
                        if la.location.y < lb.location.y {
                            (la, lb, children[i], children[i + 1])
                        } else {
                            (lb, la, children[i + 1], children[i])
                        };
                    let top_edge = abs_y + top.location.y + top.size.height;
                    let bottom_start = abs_y + bottom.location.y;
                    let center = (top_edge + bottom_start) / 2.0;
                    let min_x = abs_x + top.location.x.min(bottom.location.x);
                    let max_x = abs_x
                        + (top.location.x + top.size.width)
                            .max(bottom.location.x + bottom.size.width);

                    if let Some(r) = visitor(
                        BorderDirection::Horizontal,
                        center,
                        [min_x, max_x],
                        top_id,
                        bottom_id,
                    ) {
                        return Some(r);
                    }
                }
            }
        }

        None
    }

    #[inline]
    pub fn get_scaled_margin(&self) -> Margin {
        self.scaled_margin
    }

    fn create_panel_style(&self) -> Style {
        let scale = self.scale;
        Style {
            display: Display::Flex,
            flex_grow: 1.0,
            flex_shrink: 1.0,
            padding: geometry::Rect {
                left: length(self.panel_config.padding.left * scale),
                right: length(self.panel_config.padding.right * scale),
                top: length(self.panel_config.padding.top * scale),
                bottom: length(self.panel_config.padding.bottom * scale),
            },
            margin: geometry::Rect {
                left: length(self.panel_config.margin.left * scale),
                right: length(self.panel_config.margin.right * scale),
                top: length(self.panel_config.margin.top * scale),
                bottom: length(self.panel_config.margin.bottom * scale),
            },
            ..Default::default()
        }
    }

    fn try_split_right(&mut self) -> Result<NodeId, TaffyError> {
        self.split_panel(taffy::FlexDirection::Row)
    }

    fn try_split_down(&mut self) -> Result<NodeId, TaffyError> {
        self.split_panel(taffy::FlexDirection::Column)
    }

    fn split_panel(
        &mut self,
        direction: taffy::FlexDirection,
    ) -> Result<NodeId, TaffyError> {
        // Current is already the NodeId
        let current_node = self.current;
        if !self.inner.contains_key(&current_node) {
            return Err(TaffyError::InvalidInputNode(self.root_node));
        }

        // Find the parent of the current node
        let parent_node = self.tree.parent(current_node).unwrap_or(self.root_node);

        // Inherit the current panel's flex properties so the container
        // keeps the same proportion in its parent (e.g. 80/20 split).
        // The container has no margin, so it takes the panel's margin
        // box or the margins would turn into free space that shifts
        // every divider in the parent.
        let current_style = self.tree.style(current_node)?.clone();
        let margin = self.margin_along(current_node, parent_node);
        let sized = current_style.flex_basis.into_option().is_some();
        let scale = self.scale;
        let container_style = Style {
            display: Display::Flex,
            flex_direction: direction,
            flex_basis: basis_plus(current_style.flex_basis, margin),
            flex_grow: if sized {
                current_style.flex_grow + margin
            } else {
                current_style.flex_grow
            },
            flex_shrink: current_style.flex_shrink,
            gap: geometry::Size {
                width: length(self.panel_config.column_gap * scale),
                height: length(self.panel_config.row_gap * scale),
            },
            ..Default::default()
        };
        let container_node = self.tree.new_leaf(container_style)?;

        // Reset the current panel to flexible sizing inside the new container
        let mut reset_style = current_style;
        reset_style.flex_basis = taffy::Dimension::auto();
        reset_style.flex_grow = 1.0;
        reset_style.flex_shrink = 1.0;
        self.tree.set_style(current_node, reset_style)?;

        // Create the new panel node
        let new_node = self.tree.new_leaf(self.create_panel_style())?;

        // Get the index of current_node in its parent
        let children = self.tree.children(parent_node)?;
        let current_index = children.iter().position(|&n| n == current_node);

        // Remove current_node from parent
        self.tree.remove_child(parent_node, current_node)?;

        // Add current_node and new_node as children of container
        self.tree.add_child(container_node, current_node)?;
        self.tree.add_child(container_node, new_node)?;

        // Insert container at the same position in parent
        if let Some(idx) = current_index {
            self.tree
                .insert_child_at_index(parent_node, idx, container_node)?;
        } else {
            self.tree.add_child(parent_node, container_node)?;
        }

        Ok(new_node)
    }

    fn set_panel_size(
        &mut self,
        node: NodeId,
        width: Option<f32>,
        height: Option<f32>,
    ) -> Result<(), TaffyError> {
        let mut style = self.tree.style(node)?.clone();

        // The basis is the border-box size, the unit taffy lays out in
        // for a panel and a container alike, so a split or a collapse
        // can hand one node's sizing to the other kind unchanged and the
        // pair lands on the sizes asked for. Grow and shrink weighted by
        // the same size keep the proportion when the window resizes.
        if let Some(size) = width.or(height) {
            style.flex_basis = length(size);
            style.flex_grow = size;
            style.flex_shrink = 1.0;
        }

        self.tree.set_style(node, style)?;
        Ok(())
    }

    /// Reset all panels to flexible sizing so they expand to fill available space
    /// Reset all nodes (panels and containers) to flexible sizing.
    fn reset_panel_styles_to_flexible(&mut self) {
        let mut stack = vec![self.root_node];
        while let Some(node) = stack.pop() {
            if let Ok(mut style) = self.tree.style(node).cloned() {
                style.flex_basis = taffy::Dimension::auto();
                style.flex_grow = 1.0;
                style.flex_shrink = 1.0;
                let _ = self.tree.set_style(node, style);
            }
            if let Ok(children) = self.tree.children(node) {
                for child in children {
                    stack.push(child);
                }
            }
        }
    }

    /// Remove containers that have only one child by promoting the child
    /// to the container's parent. Repeats until no single-child containers remain.
    fn collapse_single_child_containers(&mut self) {
        loop {
            let mut collapsed = false;
            let mut stack = vec![self.root_node];

            while let Some(node) = stack.pop() {
                let children = match self.tree.children(node) {
                    Ok(c) => c,
                    _ => continue,
                };

                for &child in &children {
                    // Only consider non-panel nodes (containers)
                    if self.inner.contains_key(&child) {
                        continue;
                    }

                    let grandchildren = match self.tree.children(child) {
                        Ok(gc) => gc,
                        _ => continue,
                    };

                    if grandchildren.len() == 1 {
                        // Promote the single grandchild to replace this container,
                        // inheriting the container's flex sizing so siblings keep
                        // their proportions.
                        let grandchild = grandchildren[0];
                        let child_idx = children.iter().position(|&c| c == child);

                        if let Some(idx) = child_idx {
                            // Copy container's flex properties to the promoted child.
                            // A promoted panel brings its margins along, so a
                            // sized basis gives them back (see split_panel).
                            if let Ok(container_style) = self.tree.style(child).cloned() {
                                if let Ok(mut gc_style) =
                                    self.tree.style(grandchild).cloned()
                                {
                                    let margin = if self.inner.contains_key(&grandchild) {
                                        self.margin_along(grandchild, node)
                                    } else {
                                        0.0
                                    };
                                    let sized = container_style
                                        .flex_basis
                                        .into_option()
                                        .is_some();
                                    gc_style.flex_basis =
                                        basis_plus(container_style.flex_basis, -margin);
                                    gc_style.flex_grow = if sized {
                                        (container_style.flex_grow - margin).max(0.0)
                                    } else {
                                        container_style.flex_grow
                                    };
                                    gc_style.flex_shrink = container_style.flex_shrink;
                                    let _ = self.tree.set_style(grandchild, gc_style);
                                }
                            }

                            let _ = self.tree.remove_child(child, grandchild);
                            let _ = self.tree.remove_child(node, child);
                            let _ =
                                self.tree.insert_child_at_index(node, idx, grandchild);
                            collapsed = true;
                            break; // Tree changed, restart
                        }
                    } else if grandchildren.is_empty() {
                        // Empty container — remove it
                        let _ = self.tree.remove_child(node, child);
                        collapsed = true;
                        break;
                    } else {
                        stack.push(child);
                    }
                }

                if collapsed {
                    break;
                }
            }

            if !collapsed {
                break;
            }
        }
    }

    /// What `node`'s margins add along `parent`'s main axis. A panel
    /// occupies its border box plus these; a container has none, so a
    /// size handed from one kind to the other moves by this much.
    fn margin_along(&self, node: NodeId, parent: NodeId) -> f32 {
        let Ok(layout) = self.tree.layout(node) else {
            return 0.0;
        };
        let row = self
            .tree
            .style(parent)
            .map(|style| is_row_direction(style.flex_direction))
            .unwrap_or(true);
        if row {
            layout.margin.left + layout.margin.right
        } else {
            layout.margin.top + layout.margin.bottom
        }
    }

    /// The children of `node` in visual order along its main axis, when
    /// that axis is the one a divider move travels: `row` for left and
    /// right, column otherwise. `None` for a node laid out the other way.
    fn children_along(&self, node: NodeId, row: bool) -> Option<Vec<NodeId>> {
        let direction = self.tree.style(node).ok()?.flex_direction;
        if is_row_direction(direction) != row {
            return None;
        }
        let mut children = self.tree.children(node).ok()?;
        if matches!(
            direction,
            taffy::FlexDirection::RowReverse | taffy::FlexDirection::ColumnReverse
        ) {
            children.reverse();
        }
        Some(children)
    }

    /// The two siblings whose shared divider a key moves for `node`, in
    /// visual order: the divider on the side the key names (before the
    /// panel for left and up, after it for right and down), or, when the
    /// panel sits at the window edge on that side, the only divider it
    /// has, the one on its other side.
    ///
    /// Walks up from the panel through every container laid out along
    /// the axis, so a pane whose own container has no sibling on that
    /// side still reaches the divider an enclosing container holds. The
    /// pair can be containers: a panel nested in a column moves the
    /// divider between its column and the neighbour column. Geometry is
    /// not consulted, so panel margins and gaps cannot hide a neighbour.
    fn divider_siblings(
        &self,
        node: NodeId,
        row: bool,
        toward_start: bool,
    ) -> Option<(NodeId, NodeId)> {
        if !self.inner.contains_key(&node) {
            return None;
        }
        let mut other_side = None;
        let mut child = node;
        while let Some(parent) = self.tree.parent(child) {
            if let Some(children) = self.children_along(parent, row) {
                if children.len() >= 2 {
                    let index = children.iter().position(|&c| c == child)?;
                    let before = (index > 0).then(|| (children[index - 1], child));
                    let after = children.get(index + 1).map(|&next| (child, next));
                    let (named, other) = if toward_start {
                        (before, after)
                    } else {
                        (after, before)
                    };
                    if named.is_some() {
                        return named;
                    }
                    if other_side.is_none() {
                        other_side = other;
                    }
                }
            }
            child = parent;
        }
        other_side
    }

    /// How far `node` can lose along an axis before a panel in it
    /// reaches the floor the mouse drag keeps: a panel's size less the
    /// floor. A container across the axis passes the whole loss to each
    /// child; one along the axis shares it in proportion to the
    /// children's sizes, so a child's room counts for the container
    /// scaled by the children's total over that child's size.
    fn shrink_room(&self, node: NodeId, row: bool) -> f32 {
        let extent = |layout: &taffy::Layout| {
            if row {
                layout.size.width
            } else {
                layout.size.height
            }
        };
        let Ok(layout) = self.tree.layout(node) else {
            return 0.0;
        };
        if self.inner.contains_key(&node) {
            return extent(layout) - 50.0 * self.scale;
        }
        let Ok(children) = self.tree.children(node) else {
            return 0.0;
        };
        let along = self.children_along(node, row).is_some();
        let sizes: Vec<f32> = children
            .iter()
            .map(|&child| self.tree.layout(child).map(extent).unwrap_or(0.0))
            .collect();
        let total: f32 = sizes.iter().sum();
        children
            .iter()
            .zip(sizes)
            .map(|(&child, size)| {
                let room = self.shrink_room(child, row);
                if along && size > 0.0 {
                    room * total / size
                } else {
                    room
                }
            })
            .fold(f32::INFINITY, f32::min)
    }

    /// Move the divider a key names for the current panel by `amount`
    /// logical pixels along an axis, toward the window's start (left or
    /// up) or end. The side the divider moves toward shrinks and the
    /// other grows, whichever of the two is current; a container on
    /// either side spreads the change over its panels in proportion,
    /// as the mouse drag of that same border does. No panel on the
    /// shrinking side drops under the floor the mouse drag keeps, and
    /// nothing changes when the divider is already there. Styles only:
    /// the caller recomputes the layout.
    fn resize_divider(&mut self, row: bool, toward_start: bool, amount: f32) -> bool {
        let Some((first, second)) =
            self.divider_siblings(self.current, row, toward_start)
        else {
            return false;
        };
        let size = |layout: &taffy::Layout| {
            if row {
                layout.size.width
            } else {
                layout.size.height
            }
        };
        let (Ok(first_layout), Ok(second_layout)) =
            (self.tree.layout(first), self.tree.layout(second))
        else {
            return false;
        };
        let (first_size, second_size) = (size(first_layout), size(second_layout));

        let shrinking = if toward_start { first } else { second };
        let room = self.shrink_room(shrinking, row);
        // Under half a pixel the rounded layout would not change.
        let amount = (amount * self.scale).min(room);
        if amount < 0.5 {
            return false;
        }
        let (new_first, new_second) = if toward_start {
            (first_size - amount, second_size + amount)
        } else {
            (first_size + amount, second_size - amount)
        };

        let set = |grid: &mut Self, node, value| {
            if row {
                grid.set_panel_size(node, Some(value), None)
            } else {
                grid.set_panel_size(node, None, Some(value))
            }
        };
        // A pair set by halves would surface on the next unrelated
        // layout pass, so the first is put back if the second fails.
        let Ok(first_style) = self.tree.style(first).cloned() else {
            return false;
        };
        if set(self, first, new_first).is_err() {
            return false;
        }
        if set(self, second, new_second).is_err() {
            let _ = self.tree.set_style(first, first_style);
            return false;
        }
        true
    }

    /// Lay the tree out and push the result to the panels it moved;
    /// see [`ContextGridItem::push_layout`] for what each one gets.
    fn apply_taffy_layout(&mut self, mid_drag: bool) -> bool {
        if self.compute_layout().is_err() {
            return false;
        }
        for item in self.inner.values_mut() {
            let [_, _, width, height] = item.layout_rect;
            // Clear margin since Taffy layout already accounts for spacing
            item.val.dimension.margin = Margin::all(0.0);
            item.val.dimension.update_width(width);
            item.val.dimension.update_height(height);
            item.push_layout(mid_drag);
        }
        true
    }

    /// The pointer let go of a divider: panels whose pixel size moved
    /// during the drag get their final winsize now.
    pub fn end_border_resize(&mut self) {
        for item in self.inner.values_mut() {
            item.flush_winsize();
        }
    }

    #[inline]
    pub fn contexts_mut(&mut self) -> &mut FxHashMap<NodeId, ContextGridItem<T>> {
        &mut self.inner
    }

    /// Immutable view into the panel map. Kept even though the
    /// emission loop uses `contexts_mut` (it needs `&mut
    /// renderable_content` to take damage) — this one is handy for
    /// read-only cross-panel queries like the damage audit.
    #[allow(dead_code)]
    #[inline]
    pub fn contexts(&self) -> &FxHashMap<NodeId, ContextGridItem<T>> {
        &self.inner
    }

    /// Get contexts ordered by visual position (top-to-bottom, left-to-right)
    pub fn get_ordered_keys(&self) -> Vec<NodeId> {
        let mut panels: Vec<(NodeId, f32, f32)> = self
            .inner
            .iter()
            .map(|(&id, item)| (id, item.layout_rect[1], item.layout_rect[0])) // (id, y, x)
            .collect();

        // Sort by Y first (top to bottom), then X (left to right)
        panels.sort_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
        });

        panels.into_iter().map(|(id, _, _)| id).collect()
    }

    #[inline]
    pub fn select_next_split(&mut self) {
        if self.inner.len() == 1 {
            return;
        }

        let keys = self.get_ordered_keys();
        if let Some(current_pos) = keys.iter().position(|&k| k == self.current) {
            if current_pos >= keys.len() - 1 {
                self.current = keys[0];
            } else {
                self.current = keys[current_pos + 1];
            }
        }
    }

    #[inline]
    pub fn select_next_split_no_loop(&mut self) -> bool {
        if self.inner.len() == 1 {
            return false;
        }

        let keys = self.get_ordered_keys();
        if let Some(current_pos) = keys.iter().position(|&k| k == self.current) {
            if current_pos >= keys.len() - 1 {
                return false;
            } else {
                self.current = keys[current_pos + 1];
                return true;
            }
        }
        false
    }

    #[inline]
    pub fn select_prev_split(&mut self) {
        if self.inner.len() == 1 {
            return;
        }

        let keys = self.get_ordered_keys();
        if let Some(current_pos) = keys.iter().position(|&k| k == self.current) {
            if current_pos == 0 {
                self.current = keys[keys.len() - 1];
            } else {
                self.current = keys[current_pos - 1];
            }
        }
    }

    #[inline]
    pub fn select_prev_split_no_loop(&mut self) -> bool {
        if self.inner.len() == 1 {
            return false;
        }

        let keys = self.get_ordered_keys();
        if let Some(current_pos) = keys.iter().position(|&k| k == self.current) {
            if current_pos == 0 {
                return false;
            } else {
                self.current = keys[current_pos - 1];
                return true;
            }
        }
        false
    }

    #[inline]
    pub fn current_item(&self) -> Option<&ContextGridItem<T>> {
        self.inner.get(&self.current)
    }

    pub fn current(&self) -> &Context<T> {
        if let Some(item) = self.inner.get(&self.current) {
            &item.val
        } else {
            // This should never happen, but if it does, return the first context
            tracing::error!("Current key {:?} not found in grid", self.current);
            if let Some(root) = self.root {
                if let Some(item) = self.inner.get(&root) {
                    return &item.val;
                }
            }
            // If even root is not found, panic as this indicates a serious bug
            panic!("Grid is in an invalid state - no contexts available");
        }
    }

    #[inline]
    pub fn current_mut(&mut self) -> &mut Context<T> {
        let current_key = self.current;

        // Check if current key exists, if not try to fix it
        if !self.inner.contains_key(&current_key) {
            tracing::error!("Current key {:?} not found in grid", current_key);
            if let Some(root) = self.root {
                self.current = root;
            } else if let Some(first_key) = self.inner.keys().next() {
                self.current = *first_key;
                self.root = Some(*first_key);
            } else {
                panic!("Grid is in an invalid state - no contexts available");
            }
        }

        // Now get the mutable reference
        let current_key = self.current;
        if let Some(item) = self.inner.get_mut(&current_key) {
            &mut item.val
        } else {
            panic!(
                "Grid is in an invalid state - current key not found after fix attempt"
            );
        }
    }

    pub fn current_context_with_computed_dimension(&self) -> (&Context<T>, Margin) {
        let len = self.inner.len();
        if len <= 1 {
            if let Some(item) = self.inner.get(&self.current) {
                return (&item.val, self.scaled_margin);
            } else if let Some(root) = self.root {
                if let Some(item) = self.inner.get(&root) {
                    return (&item.val, self.scaled_margin);
                }
            }
            panic!("Grid is in an invalid state - no contexts available");
        }

        if let Some(current_item) = self.inner.get(&self.current) {
            // For multi-panel layouts, the margin must include the panel's
            // absolute offset so that mouse coordinates (which are relative
            // to the window) are correctly translated to panel-local grid
            // positions.
            let [abs_x, abs_y, _, _] = current_item.layout_rect;
            let margin = Margin {
                left: self.scaled_margin.left + abs_x,
                top: self.scaled_margin.top + abs_y,
                right: self.scaled_margin.right,
                bottom: self.scaled_margin.bottom,
            };
            (&current_item.val, margin)
        } else {
            tracing::error!("Current key {:?} not found in grid", self.current);
            if let Some(root) = self.root {
                if let Some(item) = self.inner.get(&root) {
                    return (&item.val, self.scaled_margin);
                }
            }
            panic!("Grid is in an invalid state - no contexts available");
        }
    }

    #[inline]
    /// Select panel based on mouse position using Taffy layout.
    /// Returns true only when focus actually changed to a different panel.
    pub fn select_current_based_on_mouse(&mut self, mouse: &Mouse) -> bool {
        if self.inner.len() <= 1 {
            return false;
        }

        let x = mouse.x as f32;
        let y = mouse.y as f32;

        // Use Taffy's find_context_at_position to find the panel
        if let Some(context_id) = self.find_context_at_position(x, y) {
            if context_id != self.current {
                self.current = context_id;
                return true;
            }
        }

        false
    }

    #[inline]
    pub fn grid_dimension(&self) -> ContextDimension {
        if let Some(current_item) = self.inner.get(&self.current) {
            let current_context_dimension = current_item.val.dimension;
            let scale = current_context_dimension.dimension.scale;
            // scaled_margin is already in physical pixels, but
            // ContextDimension::build scales the margin again via compute(),
            // so unscale it here to avoid double-scaling.
            let unscaled_margin = if scale > 0.0 {
                Margin::new(
                    self.scaled_margin.top / scale,
                    self.scaled_margin.right / scale,
                    self.scaled_margin.bottom / scale,
                    self.scaled_margin.left / scale,
                )
            } else {
                self.scaled_margin
            };
            ContextDimension::build(
                self.width,
                self.height,
                current_context_dimension.dimension,
                current_context_dimension.cell,
                current_context_dimension.line_height,
                current_context_dimension.font_size,
                unscaled_margin,
            )
        } else {
            tracing::error!("Current key {:?} not found in grid", self.current);
            ContextDimension::default()
        }
    }

    pub fn update_scaled_margin(&mut self, scaled_margin: Margin) {
        self.scaled_margin = scaled_margin;
        // Keep the taffy root size in sync with the new margins,
        // otherwise panels keep stale sizes until the next window
        // resize recomputes the available space.
        let _ = self.try_update_size(self.width, self.height);
    }

    /// Refresh the grid's DPI scale and every scale-derived value baked
    /// into the taffy tree at creation time: container gaps, panel
    /// padding/margins, and the live reads (border hit-boxes, divider
    /// math) that go through `self.scale`. Without this a grid created on
    /// one display keeps its creation-time DPI for paddings and gaps
    /// forever, even though the cell metrics update.
    pub fn update_scale(&mut self, new_scale: f32) {
        if (self.scale - new_scale).abs() < f32::EPSILON {
            return;
        }
        self.scale = new_scale;

        let gap = geometry::Size {
            width: length(self.panel_config.column_gap * new_scale),
            height: length(self.panel_config.row_gap * new_scale),
        };
        let padding = geometry::Rect {
            left: length(self.panel_config.padding.left * new_scale),
            right: length(self.panel_config.padding.right * new_scale),
            top: length(self.panel_config.padding.top * new_scale),
            bottom: length(self.panel_config.padding.bottom * new_scale),
        };
        let margin = geometry::Rect {
            left: length(self.panel_config.margin.left * new_scale),
            right: length(self.panel_config.margin.right * new_scale),
            top: length(self.panel_config.margin.top * new_scale),
            bottom: length(self.panel_config.margin.bottom * new_scale),
        };

        let mut stack = vec![self.root_node];
        while let Some(node) = stack.pop() {
            if let Ok(mut style) = self.tree.style(node).cloned() {
                if self.inner.contains_key(&node) {
                    style.padding = padding;
                    style.margin = margin;
                } else {
                    style.gap = gap;
                }
                let _ = self.tree.set_style(node, style);
            }
            if let Ok(children) = self.tree.children(node) {
                stack.extend(children);
            }
        }
    }

    pub fn update_line_height(&mut self, line_height: f32) {
        for context in self.inner.values_mut() {
            context.val.dimension.update_line_height(line_height);
        }
    }

    pub fn update_dimensions(&mut self, sugarloaf: &mut Sugarloaf) {
        // Per-panel cell metrics are recomputed locally now — sugarloaf
        // is consulted only for the font library it owns. Each panel's
        // `dimension.font_size` / `line_height` / `dimension.scale`
        // drive the result, so panels with per-panel zoom keep
        // independent cell strides.
        for context in self.inner.values_mut() {
            let dim = &mut context.val.dimension;
            if dim.font_size <= 0.0 {
                continue;
            }
            let (text_dims, cell) = sugarloaf.compute_cell_metrics(
                dim.font_size,
                dim.line_height,
                dim.dimension.scale,
            );
            dim.update_dimensions(text_dims, cell);
        }

        // Always apply Taffy layout for consistent positioning
        self.apply_taffy_layout(false);
    }

    /// Resize grid - always uses Taffy for consistent layout
    pub fn resize(&mut self, new_width: f32, new_height: f32) {
        self.width = new_width;
        self.height = new_height;

        // Update Taffy size and recompute layout
        let _ = self.try_update_size(new_width, new_height);

        // Apply layout - works for both single and multi-panel
        self.apply_taffy_layout(false);
    }

    #[inline]
    pub fn calculate_positions(&mut self) {
        if self.inner.is_empty() {
            return;
        }

        // Compute Taffy layout (also updates layout_rect via update_layout_rects)
        if self.compute_layout().is_err() {
            return;
        }

        // Update positions from layout_rect for all panels
        for item in self.inner.values_mut() {
            let x = item.layout_rect[0] + self.scaled_margin.left;
            let y = item.layout_rect[1] + self.scaled_margin.top;
            item.set_position([x, y]);
        }
    }

    /// Remove the panel owning `route_id`, wherever it sits in the
    /// grid. The grid's focused panel is preserved unless it is the
    /// one being removed, in which case `remove_current`'s sibling
    /// selection applies. Returns what the panel held in sugarloaf,
    /// or `None` when no panel was removed.
    pub fn remove_by_route(&mut self, route_id: usize) -> Option<RemovedPanel> {
        let key = self
            .inner
            .iter()
            .find(|(_, item)| item.val.route_id == route_id)
            .map(|(key, _)| *key)?;

        let previous = self.current;
        self.current = key;
        let removed = self.remove_current();
        if previous != key && self.inner.contains_key(&previous) {
            self.current = previous;
        }
        removed
    }

    /// Remove the current panel and select its neighbour in visual
    /// order. Returns what the panel held in sugarloaf, for the caller
    /// to release; `None` when nothing was removed, as for the last
    /// panel of a grid.
    pub fn remove_current(&mut self) -> Option<RemovedPanel> {
        if self.inner.is_empty() {
            tracing::error!("Attempted to remove from empty grid");
            return None;
        }

        // Can't remove the last panel
        if self.inner.len() == 1 {
            tracing::warn!("Cannot remove the last remaining context");
            return None;
        }

        let to_remove = self.current;

        let Some(item) = self.inner.get(&to_remove) else {
            tracing::error!("Current key {:?} not found in grid", to_remove);
            return None;
        };
        let removed = RemovedPanel {
            rich_text_id: item.val.rich_text_id,
            route_id: item.val.route_id,
        };

        // Select next panel before removing (use visual ordering)
        let ordered_keys = self.get_ordered_keys();
        let current_pos = ordered_keys.iter().position(|&k| k == to_remove);
        let next_current = if let Some(pos) = current_pos {
            // Try next panel, or previous if we're at the end
            if pos + 1 < ordered_keys.len() {
                ordered_keys[pos + 1]
            } else if pos > 0 {
                ordered_keys[pos - 1]
            } else {
                // Fallback to any other panel
                *ordered_keys
                    .iter()
                    .find(|&&k| k != to_remove)
                    .unwrap_or(&to_remove)
            }
        } else {
            // Fallback to first panel
            *self
                .inner
                .keys()
                .find(|&&k| k != to_remove)
                .unwrap_or(&to_remove)
        };

        // Remove from Taffy - to_remove IS the NodeId
        let _ = self.tree.remove(to_remove);

        // Remove from inner map
        self.inner.remove(&to_remove);

        // Update root if necessary
        if Some(to_remove) == self.root {
            self.root = self.inner.keys().next().copied();
        }

        // Set new current
        self.current = next_current;

        // Collapse single-child containers left behind by removal
        self.collapse_single_child_containers();

        // Recompute layout
        if self.panel_count() > 0 {
            // When back to a single panel, reset to flexible so it fills the window
            if self.panel_count() == 1 {
                self.reset_panel_styles_to_flexible();
            }
            self.apply_taffy_layout(false);
        }
        Some(removed)
    }

    pub fn split_right(&mut self, context: Context<T>) {
        if !self.inner.contains_key(&self.current) {
            return;
        }

        // Create taffy node first, then item
        if let Ok(new_node) = self.try_split_right() {
            let new_context = ContextGridItem::new(context);
            self.inner.insert(new_node, new_context);
            self.apply_taffy_layout(false);
            self.current = new_node;
        }
    }

    /// Split down - create new panel below using Taffy
    pub fn split_down(&mut self, context: Context<T>) {
        if !self.inner.contains_key(&self.current) {
            return;
        }

        // Create taffy node first, then item
        if let Ok(new_node) = self.try_split_down() {
            let new_context = ContextGridItem::new(context);
            self.inner.insert(new_node, new_context);
            self.apply_taffy_layout(false);
            self.current = new_node;
        }
    }

    pub fn move_divider_up(&mut self, amount: f32) -> bool {
        self.resize_divider(false, true, amount) && self.apply_taffy_layout(false)
    }

    pub fn move_divider_down(&mut self, amount: f32) -> bool {
        self.resize_divider(false, false, amount) && self.apply_taffy_layout(false)
    }

    pub fn move_divider_left(&mut self, amount: f32) -> bool {
        self.resize_divider(true, true, amount) && self.apply_taffy_layout(false)
    }

    pub fn move_divider_right(&mut self, amount: f32) -> bool {
        self.resize_divider(true, false, amount) && self.apply_taffy_layout(false)
    }

    /// Hide the panel set by clearing per-panel image overlays. The
    /// `visible=true` case is a no-op — the next `Renderer::run` will
    /// repopulate overlays naturally for whichever tab/group is
    /// active. (Naming preserved for callers; the function used to
    /// drive sugarloaf's content visibility flag, which is gone.)
    #[inline]
    pub fn set_all_rich_text_visibility(&self, sugarloaf: &mut Sugarloaf, visible: bool) {
        if visible {
            return;
        }
        for item in self.inner.values() {
            sugarloaf.clear_image_overlays_for(item.val.rich_text_id);
        }
    }

    /// Drop image overlays for every panel in the grid. Used on tab
    /// teardown — the panels themselves go away with the
    /// `ContextManager`; only the kitty graphics state needs an
    /// explicit cleanup signal.
    #[inline]
    /// Release everything this grid's panels hold in sugarloaf: image
    /// overlays and the images they drew.
    pub fn remove_from_sugarloaf(&self, sugarloaf: &mut Sugarloaf) {
        for item in self.inner.values() {
            sugarloaf.clear_image_overlays_for(item.val.rich_text_id);
            sugarloaf.remove_route_images(item.val.route_id);
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct ContextDimension {
    pub width: f32,
    pub height: f32,
    pub columns: usize,
    pub lines: usize,
    pub dimension: TextDimensions,
    pub margin: Margin,
    pub line_height: f32,
    /// Logical-point font size for this panel. Per-panel zoom updates
    /// here; the global `RootStyle.font_size` is the default that new
    /// panels inherit at create time.
    pub font_size: f32,
    /// Font size at panel creation (or last `update_config`). Drives
    /// the "reset font size" action so zoom in/out always returns to
    /// the user's configured size.
    pub original_font_size: f32,
    /// `font_size * scale_factor` — physical-pixel size used by the
    /// grid emit path to drive glyph rasterization.
    pub scaled_font_size: f32,
    /// Canonical cell metrics — single source of truth shared by the
    /// GPU grid uniform, col/row count math, and mouse hit testing.
    /// Rounded `u32` cell width / height / baseline plus unrounded
    /// `f64` face dimensions for downstream subpixel math. Avoids
    /// drift between painted cell stride and click→cell mapping.
    pub cell: rio_backend::sugarloaf::layout::CellMetrics,
}

impl Default for ContextDimension {
    fn default() -> ContextDimension {
        ContextDimension {
            width: 0.,
            height: 0.,
            columns: MIN_COLS,
            lines: MIN_LINES,
            line_height: 1.,
            font_size: 0.,
            original_font_size: 0.,
            scaled_font_size: 0.,
            dimension: TextDimensions::default(),
            margin: Margin::default(),
            cell: rio_backend::sugarloaf::layout::CellMetrics::default(),
        }
    }
}

impl ContextDimension {
    pub fn build(
        width: f32,
        height: f32,
        dimension: TextDimensions,
        cell: rio_backend::sugarloaf::layout::CellMetrics,
        line_height: f32,
        font_size: f32,
        margin: Margin,
    ) -> Self {
        let (columns, lines) = compute(width, height, cell, margin, dimension.scale);
        Self {
            width,
            height,
            columns,
            lines,
            dimension,
            margin,
            line_height,
            font_size,
            original_font_size: font_size,
            scaled_font_size: font_size * dimension.scale,
            cell,
        }
    }

    #[inline]
    pub fn update_width(&mut self, width: f32) {
        self.width = width;
        self.update();
    }

    #[inline]
    pub fn update_height(&mut self, height: f32) {
        self.height = height;
        self.update();
    }

    #[inline]
    pub fn update_line_height(&mut self, line_height: f32) {
        self.line_height = line_height;
        self.update();
    }

    #[inline]
    pub fn update_font_size(&mut self, font_size: f32) {
        self.font_size = font_size;
        self.scaled_font_size = font_size * self.dimension.scale;
        // Caller is responsible for re-running `compute_cell_metrics`
        // and feeding the result back via `update_dimensions` —
        // `font_size` alone doesn't change the cell stride, the
        // recomputed metrics do.
    }

    /// Update only the stored scale factor. Caller must follow with
    /// `compute_cell_metrics` + `update_dimensions` so width/height
    /// and canonical cell stride are recomputed for the new DPI.
    #[inline]
    pub fn update_scale(&mut self, scale: f32) {
        self.dimension.scale = scale;
        self.scaled_font_size = self.font_size * scale;
    }

    /// Re-baseline the font size — both current and "original".
    /// Called from `update_config` so a config edit becomes the new
    /// reset target. Per-panel zoom (`change_font_size`) uses
    /// `update_font_size` instead so the original stays put.
    #[inline]
    pub fn rebaseline_font_size(&mut self, font_size: f32) {
        self.font_size = font_size;
        self.original_font_size = font_size;
        self.scaled_font_size = font_size * self.dimension.scale;
    }

    /// Increment the panel's font size by 1 logical point. Returns
    /// `true` if the size changed (i.e. wasn't already at the upper
    /// clamp). Caller must follow with `compute_cell_metrics` +
    /// `update_dimensions` to refresh the cell stride.
    #[inline]
    pub fn increase_font_size(&mut self) -> bool {
        if self.font_size < 100.0 {
            self.update_font_size(self.font_size + 1.0);
            true
        } else {
            false
        }
    }

    #[inline]
    pub fn decrease_font_size(&mut self) -> bool {
        if self.font_size > 6.0 {
            self.update_font_size(self.font_size - 1.0);
            true
        } else {
            false
        }
    }

    #[inline]
    pub fn reset_font_size(&mut self) -> bool {
        if (self.font_size - self.original_font_size).abs() > f32::EPSILON {
            self.update_font_size(self.original_font_size);
            true
        } else {
            false
        }
    }

    #[inline]
    pub fn update_dimensions(
        &mut self,
        dimensions: TextDimensions,
        cell: rio_backend::sugarloaf::layout::CellMetrics,
    ) {
        self.dimension = dimensions;
        self.cell = cell;
        self.scaled_font_size = self.font_size * dimensions.scale;
        self.update();
    }

    #[inline]
    fn update(&mut self) {
        let (columns, lines) = compute(
            self.width,
            self.height,
            self.cell,
            self.margin,
            self.dimension.scale,
        );

        self.columns = columns;
        self.lines = lines;
    }
}

impl Dimensions for ContextDimension {
    #[inline]
    fn columns(&self) -> usize {
        self.columns
    }

    #[inline]
    fn screen_lines(&self) -> usize {
        self.lines
    }

    #[inline]
    fn total_lines(&self) -> usize {
        self.screen_lines()
    }

    fn square_width(&self) -> f32 {
        self.dimension.width
    }

    fn square_height(&self) -> f32 {
        self.dimension.height
    }
}
