#[cfg(test)]
mod compute_tests;

use crate::context::Context;
use crate::mouse::Mouse;
use rio_backend::config::layout::Margin;
use rio_backend::crosswords::grid::Dimensions;
use rio_backend::event::EventListener;
use rio_backend::sugarloaf::{layout::TextDimensions, Object, Rect, RichText, Sugarloaf};
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

/// Compass direction for spatial split navigation. Used by
/// [`ContextGrid::select_split_direction`] to pick the neighbour
/// pane lying in the requested direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Left,
    Right,
    Up,
    Down,
}

/// Pure scoring core of [`ContextGrid::select_split_direction`].
///
/// Given the current pane's `[x, y, w, h]` rect and an iterator of other
/// candidate `(id, rect)` pairs, returns the id that best matches the
/// requested direction, or `None` if nothing lies on that side. Generic
/// over the id type so it can be unit-tested without standing up a full
/// `ContextGrid` (which needs a Sugarloaf, Taffy tree, EventListener…).
///
/// Algorithm: keep panes that lie on the requested side of the current
/// pane (a half-pixel epsilon tolerates float jitter at the shared
/// edge) AND that overlap us on the perpendicular axis, then pick the
/// one with the most perpendicular overlap, tiebroken by smallest gap.
/// "On the side" is a half-plane test, not strict adjacency, so panes
/// separated by an inter-pane border (or even further away) still
/// qualify; the closest one wins via the distance tiebreaker.
pub fn pick_split_in_direction<Id: Copy>(
    current: [f32; 4],
    candidates: impl IntoIterator<Item = (Id, [f32; 4])>,
    direction: SplitDirection,
) -> Option<Id> {
    // Half-pixel tolerance for the half-plane test, so that two panes
    // sharing an edge (cx1 == x0 in exact math) still qualify when float
    // rounding nudges the values apart by a fraction of a pixel.
    const EPS: f32 = 0.5;

    let (cx0, cy0, cx1, cy1) = (
        current[0],
        current[1],
        current[0] + current[2],
        current[1] + current[3],
    );

    let mut best: Option<(Id, f32, f32)> = None;
    for (id, r) in candidates {
        let (x0, y0, x1, y1) = (r[0], r[1], r[0] + r[2], r[1] + r[3]);

        let (on_side, perp_overlap, distance) = match direction {
            SplitDirection::Left => {
                let overlap = cy1.min(y1) - cy0.max(y0);
                (x1 <= cx0 + EPS, overlap, cx0 - x1)
            }
            SplitDirection::Right => {
                let overlap = cy1.min(y1) - cy0.max(y0);
                (x0 + EPS >= cx1, overlap, x0 - cx1)
            }
            SplitDirection::Up => {
                let overlap = cx1.min(x1) - cx0.max(x0);
                (y1 <= cy0 + EPS, overlap, cy0 - y1)
            }
            SplitDirection::Down => {
                let overlap = cx1.min(x1) - cx0.max(x0);
                (y0 + EPS >= cy1, overlap, y0 - cy1)
            }
        };

        if !on_side || perp_overlap <= 0.0 {
            continue;
        }

        // Smaller gap is better, so we negate it for the max-tuple
        // comparison: higher `(overlap, -distance)` wins.
        let neg_distance = -distance;
        let better = match best {
            None => true,
            Some((_, best_overlap, best_neg_distance)) => {
                (perp_overlap, neg_distance) > (best_overlap, best_neg_distance)
            }
        };
        if better {
            best = Some((id, perp_overlap, neg_distance));
        }
    }

    best.map(|(id, _, _)| id)
}

/// Describes a draggable border between two panels
#[derive(Debug, Clone, Copy)]
pub struct PanelBorder {
    pub direction: BorderDirection,
    pub left_or_top: NodeId,
    pub right_or_bottom: NodeId,
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
    dimensions: TextDimensions,
    line_height: f32,
    margin: Margin,
) -> (usize, usize) {
    // Ensure we have positive dimensions
    if width <= 0.0 || height <= 0.0 || dimensions.scale <= 0.0 || line_height <= 0.0 {
        return (MIN_COLS, MIN_LINES);
    }

    // Calculate available space accounting for margins (scale margins to physical pixels)
    let scale = dimensions.scale;
    let available_width = width - (margin.left * scale) - (margin.right * scale);
    let available_height = height - (margin.top * scale) - (margin.bottom * scale);

    // Ensure we have positive available space
    if available_width <= 0.0 || available_height <= 0.0 {
        return (MIN_COLS, MIN_LINES);
    }

    // Calculate columns - divide by scaled character width
    let visible_columns =
        std::cmp::max((available_width / dimensions.width) as usize, MIN_COLS);

    // note: TextDimensions.height already includes the line_height modifier
    let char_height = dimensions.height;
    if char_height <= 0.0 {
        return (visible_columns, MIN_LINES);
    }

    let lines = (available_height / char_height).floor();
    let visible_lines = std::cmp::max(lines as usize, MIN_LINES);

    (visible_columns, visible_lines)
}

#[inline]
fn create_border(color: [f32; 4], position: [f32; 2], size: [f32; 2]) -> Object {
    Object::Rect(Rect::new(position[0], position[1], size[0], size[1], color))
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
    rich_text_object: Object,
    pub layout_rect: [f32; 4],
}

impl<T: rio_backend::event::EventListener> ContextGridItem<T> {
    pub fn new(context: Context<T>) -> Self {
        let rich_text_object = Object::RichText(RichText {
            id: context.rich_text_id,
            lines: None,
            render_data: rio_backend::sugarloaf::RichTextRenderData {
                position: [0.0, 0.0],
                should_repaint: false,
                should_remove: false,
                hidden: false,
            },
        });

        Self {
            val: context,
            rich_text_object,
            layout_rect: [0.0; 4],
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

    /// Update the position in the rich text object
    fn set_position(&mut self, position: [f32; 2]) {
        if let Object::RichText(ref mut rich_text) = self.rich_text_object {
            rich_text.render_data.position = position;
        }
    }
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
        sugarloaf: &mut Sugarloaf,
    ) {
        let min_size = 50.0 * self.scale;

        let new_a = (original_sizes.0 + delta).max(min_size);
        let new_b = (original_sizes.1 - delta).max(min_size);

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

        self.apply_taffy_layout(sugarloaf);
    }

    /// Get separator lines between adjacent panels for rendering.
    pub fn get_panel_borders(&self) -> Vec<rio_backend::sugarloaf::Object> {
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
                Ok(s) => matches!(
                    s.flex_direction,
                    taffy::FlexDirection::Row | taffy::FlexDirection::RowReverse
                ),
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
        let current_style = self.tree.style(current_node)?.clone();
        let scale = self.scale;
        let container_style = Style {
            display: Display::Flex,
            flex_direction: direction,
            flex_basis: current_style.flex_basis,
            flex_grow: current_style.flex_grow,
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

        // Use flex_grow proportional to the desired size so panels
        // scale correctly when the window is resized.
        if let Some(w) = width {
            style.flex_basis = length(0.0);
            style.flex_grow = w;
            style.flex_shrink = 1.0;
        } else if let Some(h) = height {
            style.flex_basis = length(0.0);
            style.flex_grow = h;
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
                            // Copy container's flex properties to the promoted child
                            if let Ok(container_style) = self.tree.style(child).cloned() {
                                if let Ok(mut gc_style) =
                                    self.tree.style(grandchild).cloned()
                                {
                                    gc_style.flex_basis = container_style.flex_basis;
                                    gc_style.flex_grow = container_style.flex_grow;
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

    /// Walk up the taffy tree from `node_id` to find the nearest ancestor flex
    /// container whose direction matches `axis_is_row` AND where the branch
    /// containing `node_id` has a sibling on the `pick_next` side (next
    /// sibling if true, previous if false). Returns `(branch, neighbor)`.
    /// Returns `None` if no such divider exists.
    fn find_divider_siblings(
        &self,
        node_id: NodeId,
        axis_is_row: bool,
        pick_next: bool,
    ) -> Option<(NodeId, NodeId)> {
        let mut node = node_id;
        loop {
            let parent = self.tree.parent(node)?;
            let parent_dir = self.tree.style(parent).ok()?.flex_direction;
            let parent_is_row = matches!(
                parent_dir,
                taffy::FlexDirection::Row | taffy::FlexDirection::RowReverse
            );

            if parent_is_row == axis_is_row {
                let children = self.tree.children(parent).ok()?;
                let idx = children.iter().position(|&n| n == node)?;
                let neighbor = if pick_next {
                    (idx + 1 < children.len()).then(|| children[idx + 1])
                } else {
                    (idx > 0).then(|| children[idx - 1])
                };
                if let Some(neighbor) = neighbor {
                    return Some((node, neighbor));
                }
            }

            node = parent;
            if node == self.root_node {
                return None;
            }
        }
    }

    fn apply_taffy_layout(&mut self, sugarloaf: &mut Sugarloaf) -> bool {
        if self.compute_layout().is_err() {
            return false;
        }

        let scale = sugarloaf.ctx.scale();
        let is_multi_panel = self.inner.len() > 1;

        for item in self.inner.values_mut() {
            let [abs_x, abs_y, width, height] = item.layout_rect;

            let x = (abs_x + self.scaled_margin.left) / scale;
            let y = (abs_y + self.scaled_margin.top) / scale;

            // Clear margin since Taffy layout already accounts for spacing
            item.val.dimension.margin = Margin::all(0.0);
            item.val.dimension.update_width(width);
            item.val.dimension.update_height(height);

            // Update terminal size
            let mut terminal = item.val.terminal.lock();
            terminal.resize::<ContextDimension>(item.val.dimension);
            drop(terminal);

            let winsize =
                crate::renderer::utils::terminal_dimensions(&item.val.dimension);
            let _ = item.val.messenger.send_resize(winsize);

            // Update position via sugarloaf (handles scaling)
            sugarloaf.set_position(item.val.rich_text_id, x, y);

            // Set clipping bounds for multi-panel text overflow prevention
            if is_multi_panel {
                let bounds_x = abs_x + self.scaled_margin.left;
                let bounds_y = abs_y + self.scaled_margin.top;
                sugarloaf.set_bounds(
                    item.val.rich_text_id,
                    Some([bounds_x, bounds_y, width, height]),
                );
            } else {
                sugarloaf.set_bounds(item.val.rich_text_id, None);
            }
        }
        true
    }

    #[inline]
    pub fn contexts_mut(&mut self) -> &mut FxHashMap<NodeId, ContextGridItem<T>> {
        &mut self.inner
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

    /// Move focus to the neighbouring split lying in `direction` from the
    /// currently focused pane, returning true if focus moved.
    ///
    /// Algorithm (loosely modelled on tmux's `window_pane_find_*`, but
    /// relaxed to handle non-grid-aligned splits):
    ///
    /// 1. Filter to panes lying entirely on the requested side of the
    ///    current pane (a small epsilon tolerates float rounding).
    /// 2. Among those, keep only ones that overlap the current pane on
    ///    the perpendicular axis — anything that doesn't is "diagonal"
    ///    from the user's POV and should not steal focus.
    /// 3. Score by `(perpendicular_overlap, -gap_distance)` and pick the
    ///    max. Bigger overlap wins; ties go to the closest pane. This
    ///    deterministically picks the pane the user "sees" in that
    ///    direction without needing an LRU side table like tmux uses.
    pub fn select_split_direction(&mut self, direction: SplitDirection) -> bool {
        if self.inner.len() <= 1 {
            return false;
        }

        let cur = match self.inner.get(&self.current) {
            Some(item) => item.layout_rect,
            None => return false,
        };

        let candidates = self.inner.iter().filter_map(|(&id, item)| {
            if id == self.current {
                None
            } else {
                Some((id, item.layout_rect))
            }
        });

        if let Some(id) = pick_split_in_direction(cur, candidates, direction) {
            self.current = id;
            true
        } else {
            false
        }
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
                current_context_dimension.line_height,
                unscaled_margin,
            )
        } else {
            tracing::error!("Current key {:?} not found in grid", self.current);
            ContextDimension::default()
        }
    }

    pub fn update_scaled_margin(&mut self, scaled_margin: Margin) {
        self.scaled_margin = scaled_margin;
    }

    pub fn update_line_height(&mut self, line_height: f32) {
        for context in self.inner.values_mut() {
            context.val.dimension.update_line_height(line_height);
        }
    }

    pub fn update_dimensions(&mut self, sugarloaf: &mut Sugarloaf) {
        for context in self.inner.values_mut() {
            if let Some(layout) = sugarloaf.get_text_layout(&context.val.rich_text_id) {
                context.val.dimension.update_dimensions(layout.dimensions);
            }
        }

        // Always apply Taffy layout for consistent positioning
        self.apply_taffy_layout(sugarloaf);
    }

    /// Resize grid - always uses Taffy for consistent layout
    pub fn resize(&mut self, new_width: f32, new_height: f32, sugarloaf: &mut Sugarloaf) {
        self.width = new_width;
        self.height = new_height;

        // Update Taffy size and recompute layout
        let _ = self.try_update_size(new_width, new_height);

        // Apply layout - works for both single and multi-panel
        self.apply_taffy_layout(sugarloaf);
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

    pub fn remove_current(&mut self, sugarloaf: &mut Sugarloaf) {
        if self.inner.is_empty() {
            tracing::error!("Attempted to remove from empty grid");
            return;
        }

        // Can't remove the last panel
        if self.inner.len() == 1 {
            tracing::warn!("Cannot remove the last remaining context");
            return;
        }

        let to_remove = self.current;

        if !self.inner.contains_key(&to_remove) {
            tracing::error!("Current key {:?} not found in grid", to_remove);
            return;
        }

        // Get rich text ID before removing
        let rich_text_id = self.inner.get(&to_remove).map(|item| item.val.rich_text_id);

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

        // Cleanup rich text from sugarloaf
        if let Some(id) = rich_text_id {
            sugarloaf.remove_content(id);
        }

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
            self.apply_taffy_layout(sugarloaf);
        }
    }

    pub fn split_right(&mut self, context: Context<T>, sugarloaf: &mut Sugarloaf) {
        if !self.inner.contains_key(&self.current) {
            return;
        }

        // Create taffy node first, then item
        if let Ok(new_node) = self.try_split_right() {
            let new_context = ContextGridItem::new(context);
            self.inner.insert(new_node, new_context);
            self.apply_taffy_layout(sugarloaf);
            self.current = new_node;
        }
    }

    /// Split down - create new panel below using Taffy
    pub fn split_down(&mut self, context: Context<T>, sugarloaf: &mut Sugarloaf) {
        if !self.inner.contains_key(&self.current) {
            return;
        }

        // Create taffy node first, then item
        if let Ok(new_node) = self.try_split_down() {
            let new_context = ContextGridItem::new(context);
            self.inner.insert(new_node, new_context);
            self.apply_taffy_layout(sugarloaf);
            self.current = new_node;
        }
    }

    /// Shared divider-move implementation. `direction` selects both the axis
    /// (horizontal for Left/Right, vertical for Up/Down) and which side of
    /// the divider should grow (Right/Down: upper/left side grows;
    /// Left/Up: lower/right side grows).
    fn move_divider(
        &mut self,
        direction: SplitDirection,
        amount: f32,
        sugarloaf: &mut Sugarloaf,
    ) -> bool {
        if self.panel_count() <= 1 {
            return false;
        }

        let (axis_is_row, grow_first) = match direction {
            // Move divider right: upper/left sibling grows.
            SplitDirection::Right => (true, true),
            // Move divider left: upper/left sibling shrinks.
            SplitDirection::Left => (true, false),
            // Move divider down: upper sibling grows.
            SplitDirection::Down => (false, true),
            // Move divider up: upper sibling shrinks.
            SplitDirection::Up => (false, false),
        };

        // Find an adjacent sibling pair on this axis. Try next sibling first,
        // then fall back to the previous one. The pair is normalised so that
        // `first` is the upper/left and `second` is the lower/right.
        let (first, second) = if let Some((branch, next)) =
            self.find_divider_siblings(self.current, axis_is_row, true)
        {
            (branch, next)
        } else if let Some((branch, prev)) =
            self.find_divider_siblings(self.current, axis_is_row, false)
        {
            (prev, branch)
        } else {
            return false;
        };

        let size_of = |node| {
            self.tree.layout(node).ok().map(|l| {
                if axis_is_row {
                    l.size.width
                } else {
                    l.size.height
                }
            })
        };
        let (Some(first_size), Some(second_size)) = (size_of(first), size_of(second))
        else {
            return false;
        };
        let (new_first, new_second) = if grow_first {
            (first_size + amount, second_size - amount)
        } else {
            (first_size - amount, second_size + amount)
        };

        let min = if axis_is_row { 100.0 } else { 50.0 };
        if new_first < min || new_second < min {
            return false;
        }

        let to_dims = |size| {
            if axis_is_row {
                (Some(size), None)
            } else {
                (None, Some(size))
            }
        };
        let (w, h) = to_dims(new_first);
        let _ = self.set_panel_size(first, w, h);
        let (w, h) = to_dims(new_second);
        let _ = self.set_panel_size(second, w, h);

        self.apply_taffy_layout(sugarloaf)
    }

    pub fn move_divider_up(&mut self, amount: f32, sugarloaf: &mut Sugarloaf) -> bool {
        self.move_divider(SplitDirection::Up, amount, sugarloaf)
    }

    pub fn move_divider_down(&mut self, amount: f32, sugarloaf: &mut Sugarloaf) -> bool {
        self.move_divider(SplitDirection::Down, amount, sugarloaf)
    }

    pub fn move_divider_left(&mut self, amount: f32, sugarloaf: &mut Sugarloaf) -> bool {
        self.move_divider(SplitDirection::Left, amount, sugarloaf)
    }

    pub fn move_divider_right(&mut self, amount: f32, sugarloaf: &mut Sugarloaf) -> bool {
        self.move_divider(SplitDirection::Right, amount, sugarloaf)
    }

    #[inline]
    pub fn set_all_rich_text_visibility(&self, sugarloaf: &mut Sugarloaf, hidden: bool) {
        for item in self.inner.values() {
            sugarloaf.set_visibility(item.val.rich_text_id, hidden);
        }
    }

    #[inline]
    pub fn remove_all_rich_text(&self, sugarloaf: &mut Sugarloaf) {
        for item in self.inner.values() {
            sugarloaf.remove_content(item.val.rich_text_id);
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
}

impl Default for ContextDimension {
    fn default() -> ContextDimension {
        ContextDimension {
            width: 0.,
            height: 0.,
            columns: MIN_COLS,
            lines: MIN_LINES,
            line_height: 1.,
            dimension: TextDimensions::default(),
            margin: Margin::default(),
        }
    }
}

impl ContextDimension {
    pub fn build(
        width: f32,
        height: f32,
        dimension: TextDimensions,
        line_height: f32,
        margin: Margin,
    ) -> Self {
        let (columns, lines) = compute(width, height, dimension, line_height, margin);
        Self {
            width,
            height,
            columns,
            lines,
            dimension,
            margin,
            line_height,
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
    pub fn update_dimensions(&mut self, dimensions: TextDimensions) {
        self.dimension = dimensions;
        self.update();
    }

    #[inline]
    fn update(&mut self) {
        let (columns, lines) = compute(
            self.width,
            self.height,
            self.dimension,
            self.line_height,
            self.margin,
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

#[cfg(test)]
mod split_direction_tests {
    use super::{pick_split_in_direction, SplitDirection};

    // Two side-by-side panes (no border).
    //   +---+---+
    //   | L | R |
    //   +---+---+
    #[test]
    fn picks_right_neighbour() {
        let cur = [0.0, 0.0, 100.0, 200.0];
        let candidates = vec![("R", [100.0, 0.0, 100.0, 200.0])];
        assert_eq!(
            pick_split_in_direction(cur, candidates.clone(), SplitDirection::Right),
            Some("R")
        );
        // ...and nothing on the left.
        assert_eq!(
            pick_split_in_direction(cur, candidates, SplitDirection::Left),
            None
        );
    }

    // Tolerates panes separated by a border / arbitrary gap.
    #[test]
    fn tolerates_border_gap() {
        let cur = [0.0, 0.0, 100.0, 200.0];
        // Right pane sits 2px to the right (typical inter-pane border).
        // The "on side" test is a half-plane check, not strict adjacency,
        // so any pane to the right of cx1 qualifies.
        let candidates = vec![("R", [102.0, 0.0, 100.0, 200.0])];
        assert_eq!(
            pick_split_in_direction(cur, candidates, SplitDirection::Right),
            Some("R")
        );
    }

    // Three-pane layout: tall pane on the left, two stacked panes on
    // the right. From the bottom-right pane, "Left" must pick the tall
    // left pane (the only one whose Y range fully overlaps), not the
    // top-right pane (which is above us, not to our left).
    //   +---+---+
    //   |   | T |
    //   | L +---+
    //   |   | B |   <- current
    //   +---+---+
    #[test]
    fn prefers_perpendicular_overlap_over_diagonal() {
        let cur = [100.0, 100.0, 100.0, 100.0]; // bottom-right
        let candidates = vec![
            ("L", [0.0, 0.0, 100.0, 200.0]),   // tall left
            ("T", [100.0, 0.0, 100.0, 100.0]), // top-right (no overlap with us on Y)
        ];
        assert_eq!(
            pick_split_in_direction(cur, candidates, SplitDirection::Left),
            Some("L")
        );
    }

    // When two candidates both lie to the right and overlap on Y, pick
    // the one with the bigger perpendicular overlap.
    //   +-----+-----+
    //   |     |  A  |     <- A is small, only top half overlaps
    //   |  C  +-----+
    //   |     |  B  |     <- B fully overlaps with current bottom half
    //   +-----+-----+
    #[test]
    fn breaks_ties_by_overlap_then_distance() {
        let cur = [0.0, 0.0, 100.0, 200.0];
        let candidates = vec![
            ("A", [100.0, 0.0, 100.0, 50.0]),    // 50px overlap
            ("B", [100.0, 50.0, 100.0, 150.0]),  // 150px overlap (wins)
        ];
        assert_eq!(
            pick_split_in_direction(cur, candidates, SplitDirection::Right),
            Some("B")
        );
    }

    #[test]
    fn returns_none_when_alone() {
        let cur = [0.0, 0.0, 100.0, 100.0];
        let candidates: Vec<(&str, [f32; 4])> = vec![];
        assert_eq!(
            pick_split_in_direction(cur, candidates, SplitDirection::Up),
            None
        );
    }
}
