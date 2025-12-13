#[cfg(test)]
mod layout_tests;

use crate::context::Context;
use crate::mouse::Mouse;
use rio_backend::config::layout::Margin;
use rio_backend::crosswords::grid::Dimensions;
use rio_backend::event::EventListener;
use rio_backend::sugarloaf::{layout::TextDimensions, Object, Rect, RichText, Sugarloaf};
use rustc_hash::FxHashMap;

use taffy::{AvailableSpace, Display, style_helpers::length, Style, NodeId, TaffyTree, TaffyError, geometry};

const MIN_COLS: usize = 2;
const MIN_LINES: usize = 1;

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

    // Calculate lines - divide by scaled character height
    let char_height = dimensions.height * line_height;
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

/// Border configuration for split panels
#[derive(Debug, Clone, Copy)]
pub struct BorderConfig {
    pub width: f32,
    pub color: [f32; 4],        // RGBA for inactive panels
    pub active_color: [f32; 4], // RGBA for active panel
    pub radius: f32,            // Corner radius (0.0 = sharp)
}

impl Default for BorderConfig {
    fn default() -> Self {
        Self {
            width: 2.0,
            color: [0.8, 0.8, 0.8, 1.0],        // Light gray
            active_color: [0.3, 0.8, 0.9, 1.0], // Cyan-ish for active
            radius: 0.0,
        }
    }
}

pub struct ContextGrid<T: EventListener> {
    pub width: f32,
    pub height: f32,
    pub current: NodeId,
    pub scaled_margin: Margin,
    scale: f32,
    border_color: [f32; 4],
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
    /// Cached absolute layout: [x, y, width, height] in physical pixels
    layout_rect: [f32; 4],
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

    #[inline]
    pub fn position(&self) -> [f32; 2] {
        if let Object::RichText(ref rich_text) = self.rich_text_object {
            rich_text.render_data.position
        } else {
            [0.0, 0.0]
        }
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
        border_active_color: [f32; 4],
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

        let root_node = tree.new_leaf(root_style).expect("Failed to create root node");

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

        let panel_node = tree.new_leaf(panel_style).expect("Failed to create panel node");
        tree.add_child(root_node, panel_node).expect("Failed to add child");

        // Use NodeId as the key
        let mut inner = FxHashMap::default();
        inner.insert(panel_node, ContextGridItem::new(context));

        let border_config = BorderConfig {
            width: 1.0,
            color: border_color,
            active_color: border_active_color,
            radius: 0.0,
        };

        let mut grid = Self {
            inner,
            current: panel_node,
            scaled_margin,
            scale,
            width,
            height,
            border_color,
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
    pub fn get_by_route_id(&mut self, route_id: usize) -> Option<&mut ContextGridItem<T>> {
        self.inner.values_mut().find(|item| item.val.route_id == route_id)
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
        let available_height = height - self.scaled_margin.top - self.scaled_margin.bottom;

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
            if adj_x >= left && adj_x < left + width && adj_y >= top && adj_y < top + height {
                return Some(node_id);
            }
        }
        None
    }

    /// Get panel borders for rendering. Returns border rectangles in physical pixel coordinates.
    /// The caller is responsible for converting to logical coordinates and adding margin.
    /// Active panel uses `border_config.active_color`, inactive panels use `border_config.color`.
    pub fn get_panel_borders(&self) -> Vec<rio_backend::sugarloaf::Object> {
        use rio_backend::sugarloaf::{Object, Rect};

        if !self.should_draw_borders() {
            return vec![];
        }

        let mut borders = Vec::with_capacity(self.inner.len() * 4);
        let border_width = self.border_config.width;
        let inactive_color = self.border_config.color;
        let active_color = self.border_config.active_color;

        for (&context_id, item) in &self.inner {
            let [x, y, width, height] = item.layout_rect;
            let color = if context_id == self.current {
                active_color
            } else {
                inactive_color
            };

            // Top border
            borders.push(Object::Rect(Rect::new(x, y, width, border_width, color)));
            // Right border
            borders.push(Object::Rect(Rect::new(x + width - border_width, y, border_width, height, color)));
            // Bottom border
            borders.push(Object::Rect(Rect::new(x, y + height - border_width, width, border_width, color)));
            // Left border
            borders.push(Object::Rect(Rect::new(x, y, border_width, height, color)));
        }

        borders
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

    fn split_panel(&mut self, direction: taffy::FlexDirection) -> Result<NodeId, TaffyError> {
        // Current is already the NodeId
        let current_node = self.current;
        if !self.inner.contains_key(&current_node) {
            return Err(TaffyError::InvalidInputNode(self.root_node));
        }

        // Find the parent of the current node
        let parent_node = self.tree.parent(current_node).unwrap_or(self.root_node);

        // Create a container node with the split direction
        let scale = self.scale;
        let container_style = Style {
            display: Display::Flex,
            flex_direction: direction,
            flex_grow: 1.0,
            flex_shrink: 1.0,
            gap: geometry::Size {
                width: length(self.panel_config.column_gap * scale),
                height: length(self.panel_config.row_gap * scale),
            },
            ..Default::default()
        };
        let container_node = self.tree.new_leaf(container_style)?;

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
            self.tree.insert_child_at_index(parent_node, idx, container_node)?;
        } else {
            self.tree.add_child(parent_node, container_node)?;
        }

        Ok(new_node)
    }

    fn set_panel_size(&mut self, node: NodeId, width: Option<f32>, height: Option<f32>) -> Result<(), TaffyError> {
        let mut style = self.tree.style(node)?.clone();

        if let Some(w) = width {
            style.flex_basis = length(w);
            style.flex_grow = 0.0;
            style.flex_shrink = 0.0;
        } else if let Some(h) = height {
            style.flex_basis = length(h);
            style.flex_grow = 0.0;
            style.flex_shrink = 0.0;
        }

        self.tree.set_style(node, style)?;
        Ok(())
    }

    /// Reset all panels to flexible sizing so they expand to fill available space
    fn reset_panel_styles_to_flexible(&mut self) {
        let nodes: Vec<NodeId> = self.inner.keys().copied().collect();
        for node in nodes {
            if let Ok(mut style) = self.tree.style(node).cloned() {
                style.flex_basis = taffy::Dimension::auto();
                style.flex_grow = 1.0;
                style.flex_shrink = 1.0;
                let _ = self.tree.set_style(node, style);
            }
        }
    }

    fn find_horizontal_neighbors(&self, node_id: NodeId) -> Option<(NodeId, NodeId)> {
        if !self.inner.contains_key(&node_id) {
            return None;
        }
        let current_layout = self.tree.layout(node_id).ok()?;

        let gap = self.panel_config.column_gap * self.scale;

        // Find panel directly to the left (overlapping Y range, touching on X axis)
        for &other_id in self.inner.keys() {
            if other_id == node_id { continue; }

            let other_layout = self.tree.layout(other_id).ok()?;

            // Check if vertically overlapping (Y ranges overlap)
            let current_y_end = current_layout.location.y + current_layout.size.height;
            let other_y_end = other_layout.location.y + other_layout.size.height;
            let y_overlap = current_layout.location.y < other_y_end && other_layout.location.y < current_y_end;

            if y_overlap {
                // Check if other panel is directly to the left (touching with gap)
                let other_right = other_layout.location.x + other_layout.size.width;
                let distance = current_layout.location.x - other_right;

                if distance >= 0.0 && distance <= gap + 1.0 {
                    return Some((other_id, node_id));
                }
            }
        }

        // Try finding panel to the right
        let current_right = current_layout.location.x + current_layout.size.width;
        for &other_id in self.inner.keys() {
            if other_id == node_id { continue; }

            let other_layout = self.tree.layout(other_id).ok()?;

            // Check if vertically overlapping
            let current_y_end = current_layout.location.y + current_layout.size.height;
            let other_y_end = other_layout.location.y + other_layout.size.height;
            let y_overlap = current_layout.location.y < other_y_end && other_layout.location.y < current_y_end;

            if y_overlap {
                let distance = other_layout.location.x - current_right;

                if distance >= 0.0 && distance <= gap + 1.0 {
                    return Some((node_id, other_id));
                }
            }
        }

        None
    }

    fn find_vertical_neighbors(&self, node_id: NodeId) -> Option<(NodeId, NodeId)> {
        if !self.inner.contains_key(&node_id) {
            return None;
        }
        let current_layout = self.tree.layout(node_id).ok()?;

        let gap = self.panel_config.row_gap * self.scale;

        // Find panel directly above (overlapping X range, touching on Y axis)
        for &other_id in self.inner.keys() {
            if other_id == node_id { continue; }

            let other_layout = self.tree.layout(other_id).ok()?;

            // Check if horizontally overlapping (X ranges overlap)
            let current_x_end = current_layout.location.x + current_layout.size.width;
            let other_x_end = other_layout.location.x + other_layout.size.width;
            let x_overlap = current_layout.location.x < other_x_end && other_layout.location.x < current_x_end;

            if x_overlap {
                // Check if other panel is directly above (touching with gap)
                let other_bottom = other_layout.location.y + other_layout.size.height;
                let distance = current_layout.location.y - other_bottom;

                if distance >= 0.0 && distance <= gap + 1.0 {
                    return Some((other_id, node_id));
                }
            }
        }

        // Try finding panel below
        let current_bottom = current_layout.location.y + current_layout.size.height;
        for &other_id in self.inner.keys() {
            if other_id == node_id { continue; }

            let other_layout = self.tree.layout(other_id).ok()?;

            // Check if horizontally overlapping
            let current_x_end = current_layout.location.x + current_layout.size.width;
            let other_x_end = other_layout.location.x + other_layout.size.width;
            let x_overlap = current_layout.location.x < other_x_end && other_layout.location.x < current_x_end;

            if x_overlap {
                let distance = other_layout.location.y - current_bottom;

                if distance >= 0.0 && distance <= gap + 1.0 {
                    return Some((node_id, other_id));
                }
            }
        }

        None
    }

    fn apply_taffy_layout(&mut self, sugarloaf: &mut Sugarloaf) -> bool {
        if self.compute_layout().is_err() {
            return false;
        }

        let scale = sugarloaf.ctx.scale();

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

            let winsize = crate::renderer::utils::terminal_dimensions(&item.val.dimension);
            let _ = item.val.messenger.send_resize(winsize);

            // Update position via sugarloaf (handles scaling)
            sugarloaf.set_position(item.val.rich_text_id, x, y);
        }
        true
    }

    #[inline]
    pub fn contexts_mut(&mut self) -> &mut FxHashMap<NodeId, ContextGridItem<T>> {
        &mut self.inner
    }

    #[inline]
    pub fn contexts(&mut self) -> &FxHashMap<NodeId, ContextGridItem<T>> {
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

    /// Get the index of the current key in the ordered list
    pub fn current_index(&self) -> usize {
        let keys = self.get_ordered_keys();
        keys.iter().position(|&k| k == self.current).unwrap_or(0)
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

    #[inline]
    pub fn extend_with_objects(&self, target: &mut Vec<Object>) {
        let len = self.inner.len();
        if len == 0 {
            return;
        }

        // Reserve space for more objects
        target.reserve(len);

        // In case there's only 1 context then ignore quad
        if len == 1 {
            if let Some(root) = self.root {
                if let Some(item) = self.inner.get(&root) {
                    target.push(item.rich_text_object.clone());
                }
            }
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
            (&current_item.val, self.scaled_margin)
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
    /// Select panel based on mouse position using Taffy layout
    pub fn select_current_based_on_mouse(&mut self, mouse: &Mouse) -> bool {
        if self.inner.len() <= 1 {
            return false;
        }

        let x = mouse.x as f32;
        let y = mouse.y as f32;

        // Use Taffy's find_context_at_position to find the panel
        if let Some(context_id) = self.find_context_at_position(x, y) {
            self.current = context_id;
            return true;
        }

        false
    }

    pub fn find_by_rich_text_id(&self, searched_rich_text_id: usize) -> Option<NodeId> {
        for (&key, item) in &self.inner {
            if item.val.rich_text_id == searched_rich_text_id {
                return Some(key);
            }
        }
        None
    }

    #[inline]
    pub fn grid_dimension(&self) -> ContextDimension {
        if let Some(current_item) = self.inner.get(&self.current) {
            let current_context_dimension = current_item.val.dimension;
            ContextDimension::build(
                self.width,
                self.height,
                current_context_dimension.dimension,
                current_context_dimension.line_height,
                self.scaled_margin,
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

    fn request_resize(&mut self, key: NodeId) {
        if let Some(item) = self.inner.get_mut(&key) {
            let mut terminal = item.val.terminal.lock();
            terminal.resize::<ContextDimension>(item.val.dimension);
            drop(terminal);
            let winsize =
                crate::renderer::utils::terminal_dimensions(&item.val.dimension);
            let _ = item.val.messenger.send_resize(winsize);
        }
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
        for (&node_id, item) in &mut self.inner {
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
                *ordered_keys.iter().find(|&&k| k != to_remove).unwrap_or(&to_remove)
            }
        } else {
            // Fallback to first panel
            *self.inner.keys().find(|&&k| k != to_remove).unwrap_or(&to_remove)
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

        // Reset remaining panels to flexible sizing and recompute layout
        if self.panel_count() > 0 {
            self.reset_panel_styles_to_flexible();
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

    pub fn move_divider_up(&mut self, amount: f32, sugarloaf: &mut Sugarloaf) -> bool {
        if self.panel_count() <= 1 {
            return false;
        }

        let current_node = self.current;

        // Find vertically adjacent panels - returns (top_node, bottom_node)
        if let Some((top_node, bottom_node)) = self.find_vertical_neighbors(current_node) {
            // Get current sizes
            let top_layout = match self.tree.layout(top_node).ok() {
                Some(layout) => layout,
                None => return false,
            };
            let bottom_layout = match self.tree.layout(bottom_node).ok() {
                Some(layout) => layout,
                None => return false,
            };

            let min_height = 50.0;

            // Determine which panel to shrink based on which one is current
            let new_top_height;
            let new_bottom_height;

            if current_node == bottom_node {
                // Current is bottom: shrink bottom, expand top (divider moves up)
                new_bottom_height = bottom_layout.size.height - amount;
                new_top_height = top_layout.size.height + amount;
            } else {
                // Current is top: shrink top, expand bottom (divider moves up)
                new_top_height = top_layout.size.height - amount;
                new_bottom_height = bottom_layout.size.height + amount;
            }

            if new_top_height < min_height || new_bottom_height < min_height {
                return false;
            }

            // Update panel sizes using flex_basis
            let _ = self.set_panel_size(top_node, None, Some(new_top_height));
            let _ = self.set_panel_size(bottom_node, None, Some(new_bottom_height));

            // Apply layout and update all contexts
            return self.apply_taffy_layout(sugarloaf);
        }

        false
    }

    pub fn move_divider_down(&mut self, amount: f32, sugarloaf: &mut Sugarloaf) -> bool {
        if self.panel_count() <= 1 {
            return false;
        }

        let current_node = self.current;

        // Find vertically adjacent panels - returns (top_node, bottom_node)
        if let Some((top_node, bottom_node)) = self.find_vertical_neighbors(current_node) {
            // Get current sizes
            let top_layout = match self.tree.layout(top_node).ok() {
                Some(layout) => layout,
                None => return false,
            };
            let bottom_layout = match self.tree.layout(bottom_node).ok() {
                Some(layout) => layout,
                None => return false,
            };

            let min_height = 50.0;

            // Determine which panel to expand based on which one is current
            let new_top_height;
            let new_bottom_height;

            if current_node == bottom_node {
                // Current is bottom: expand bottom, shrink top (divider moves down)
                new_bottom_height = bottom_layout.size.height + amount;
                new_top_height = top_layout.size.height - amount;
            } else {
                // Current is top: expand top, shrink bottom (divider moves down)
                new_top_height = top_layout.size.height + amount;
                new_bottom_height = bottom_layout.size.height - amount;
            }

            if new_top_height < min_height || new_bottom_height < min_height {
                return false;
            }

            // Update panel sizes using flex_basis
            let _ = self.set_panel_size(top_node, None, Some(new_top_height));
            let _ = self.set_panel_size(bottom_node, None, Some(new_bottom_height));

            // Apply layout and update all contexts
            return self.apply_taffy_layout(sugarloaf);
        }

        false
    }

    pub fn move_divider_left(&mut self, amount: f32, sugarloaf: &mut Sugarloaf) -> bool {
        if self.panel_count() <= 1 {
            return false;
        }

        let current_node = self.current;

        // Find horizontally adjacent panels - returns (left_node, right_node)
        if let Some((left_node, right_node)) = self.find_horizontal_neighbors(current_node) {
            // Get current sizes
            let left_layout = match self.tree.layout(left_node).ok() {
                Some(layout) => layout,
                None => return false,
            };
            let right_layout = match self.tree.layout(right_node).ok() {
                Some(layout) => layout,
                None => return false,
            };

            let min_width = 100.0;

            // Determine which panel to shrink based on which one is current
            let new_left_width;
            let new_right_width;

            if current_node == right_node {
                // Current is right: shrink right, expand left (divider moves left)
                new_right_width = right_layout.size.width - amount;
                new_left_width = left_layout.size.width + amount;
            } else {
                // Current is left: shrink left, expand right (divider moves left)
                new_left_width = left_layout.size.width - amount;
                new_right_width = right_layout.size.width + amount;
            }

            if new_left_width < min_width || new_right_width < min_width {
                return false;
            }

            // Update panel sizes using flex_basis
            let _ = self.set_panel_size(left_node, Some(new_left_width), None);
            let _ = self.set_panel_size(right_node, Some(new_right_width), None);

            // Apply layout and update all contexts
            return self.apply_taffy_layout(sugarloaf);
        }

        false
    }

    pub fn move_divider_right(&mut self, amount: f32, sugarloaf: &mut Sugarloaf) -> bool {
        if self.panel_count() <= 1 {
            return false;
        }

        let current_node = self.current;

        // Find horizontally adjacent panels - returns (left_node, right_node)
        if let Some((left_node, right_node)) = self.find_horizontal_neighbors(current_node) {
            // Get current sizes
            let left_layout = match self.tree.layout(left_node).ok() {
                Some(layout) => layout,
                None => return false,
            };
            let right_layout = match self.tree.layout(right_node).ok() {
                Some(layout) => layout,
                None => return false,
            };

            let min_width = 100.0;

            // Determine which panel to expand based on which one is current
            let new_left_width;
            let new_right_width;

            if current_node == right_node {
                // Current is right: expand right, shrink left (divider moves right)
                new_right_width = right_layout.size.width + amount;
                new_left_width = left_layout.size.width - amount;
            } else {
                // Current is left: expand left, shrink right (divider moves right)
                new_left_width = left_layout.size.width + amount;
                new_right_width = right_layout.size.width - amount;
            }

            if new_left_width < min_width || new_right_width < min_width {
                return false;
            }

            // Update panel sizes using flex_basis
            let _ = self.set_panel_size(left_node, Some(new_left_width), None);
            let _ = self.set_panel_size(right_node, Some(new_right_width), None);

            // Apply layout and update all contexts
            return self.apply_taffy_layout(sugarloaf);
        }

        false
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
    pub fn increase_width(&mut self, acc_width: f32) {
        self.width += acc_width;
        self.update();
    }

    #[inline]
    pub fn update_height(&mut self, height: f32) {
        self.height = height;
        self.update();
    }

    #[inline]
    pub fn increase_height(&mut self, acc_height: f32) {
        self.height += acc_height;
        self.update();
    }

    #[inline]
    pub fn update_margin(&mut self, margin: Margin) {
        self.margin = margin;
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
