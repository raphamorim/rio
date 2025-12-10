use crate::context::Context;
use crate::mouse::Mouse;
use rio_backend::crosswords::grid::Dimensions;
use rio_backend::event::EventListener;
use rio_backend::sugarloaf::{layout::TextDimensions, Object, Rect, RichText, Sugarloaf};
use std::collections::HashMap;

const MIN_COLS: usize = 2;
const MIN_LINES: usize = 1;

fn compute(
    width: f32,
    height: f32,
    dimensions: TextDimensions,
    line_height: f32,
    margin: Delta<f32>,
) -> (usize, usize) {
    // Ensure we have positive dimensions
    if width <= 0.0 || height <= 0.0 || dimensions.scale <= 0.0 || line_height <= 0.0 {
        return (MIN_COLS, MIN_LINES);
    }

    let margin_x = margin.x;
    let margin_spaces = margin.top_y + margin.bottom_y;

    // Calculate available space in physical pixels
    let available_width = width - margin_x;
    let available_height = height - margin_spaces;

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

    let lines = (available_height / char_height).floor() - 1.0;
    let visible_lines = std::cmp::max(lines as usize, MIN_LINES);

    (visible_columns, visible_lines)
}

#[inline]
fn create_border(color: [f32; 4], position: [f32; 2], size: [f32; 2]) -> Object {
    Object::Rect(Rect::new(position[0], position[1], size[0], size[1], color))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Delta<T: Default> {
    pub x: T,
    pub top_y: T,
    pub bottom_y: T,
}

pub struct ContextGrid<T: EventListener> {
    pub width: f32,
    pub height: f32,
    pub current: usize,
    pub margin: Delta<f32>,
    border_color: [f32; 4],
    padding_panel: f32,
    scaled_padding: f32,
    inner: HashMap<usize, ContextGridItem<T>>,
    pub root: Option<usize>,
}

pub struct ContextGridItem<T: EventListener> {
    pub val: Context<T>,
    right: Option<usize>,
    down: Option<usize>,
    parent: Option<usize>,
    rich_text_object: Object,
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
            right: None,
            down: None,
            parent: None,
            rich_text_object,
        }
    }
}

impl<T: rio_backend::event::EventListener> ContextGridItem<T> {
    #[inline]
    #[allow(unused)]
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
        margin: Delta<f32>,
        border_color: [f32; 4],
        padding_panel: f32,
    ) -> Self {
        let width = context.dimension.width;
        let height = context.dimension.height;
        let scale = context.dimension.dimension.scale;
        let scaled_padding = padding_panel * scale;
        let mut inner = HashMap::new();
        let root_key = context.route_id;
        inner.insert(context.route_id, ContextGridItem::new(context));
        let mut grid = Self {
            inner,
            current: root_key,
            margin,
            width,
            height,
            border_color,
            padding_panel,
            scaled_padding,
            root: Some(root_key),
        };
        grid.calculate_positions_for_affected_nodes(&[root_key]);
        grid
    }

    #[inline]
    pub fn get_mut(&mut self, key: usize) -> Option<&mut ContextGridItem<T>> {
        self.inner.get_mut(&key)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[inline]
    #[allow(dead_code)]
    pub fn scale(&self) -> f32 {
        self.scaled_padding / self.padding_panel
    }

    #[inline]
    #[allow(dead_code)]
    pub fn scaled_padding(&self) -> f32 {
        self.scaled_padding
    }

    #[inline]
    pub fn contexts_mut(&mut self) -> &mut HashMap<usize, ContextGridItem<T>> {
        &mut self.inner
    }

    #[inline]
    #[allow(unused)]
    pub fn contexts(&mut self) -> &HashMap<usize, ContextGridItem<T>> {
        &self.inner
    }

    /// Get all keys in the order they appear in the grid (depth-first traversal)
    pub fn get_ordered_keys(&self) -> Vec<usize> {
        let mut keys = Vec::new();
        if let Some(root) = self.root {
            self.collect_keys_recursive(root, &mut keys);
        }
        keys
    }

    /// Get the index of the current key in the ordered list
    #[allow(dead_code)]
    pub fn current_index(&self) -> usize {
        let keys = self.get_ordered_keys();
        keys.iter().position(|&k| k == self.current).unwrap_or(0)
    }

    /// Get contexts in the order they appear in the grid
    #[allow(dead_code)]
    pub fn contexts_ordered(&self) -> Vec<&ContextGridItem<T>> {
        let keys = self.get_ordered_keys();
        keys.iter()
            .filter_map(|&key| self.inner.get(&key))
            .collect()
    }

    /// Get the index of a key in the ordered list
    #[allow(dead_code)]
    pub fn key_to_index(&self, key: usize) -> Option<usize> {
        let keys = self.get_ordered_keys();
        keys.iter().position(|&k| k == key)
    }

    fn collect_keys_recursive(&self, key: usize, keys: &mut Vec<usize>) {
        if let Some(item) = self.inner.get(&key) {
            keys.push(key);
            if let Some(right_key) = item.right {
                self.collect_keys_recursive(right_key, keys);
            }
            if let Some(down_key) = item.down {
                self.collect_keys_recursive(down_key, keys);
            }
        }
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
    #[allow(unused)]
    pub fn current_key(&self) -> usize {
        self.current
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
        } else {
            self.plot_objects(target);
        }
    }

    #[inline]
    pub fn objects(&self) -> Vec<Object> {
        let len = self.inner.len();
        if len == 0 {
            return vec![];
        }

        let mut objects = Vec::with_capacity(len);

        // In case there's only 1 context then ignore quad
        if len == 1 {
            if let Some(root) = self.root {
                if let Some(item) = self.inner.get(&root) {
                    objects.push(item.rich_text_object.clone());
                }
            }
        } else {
            self.plot_objects(&mut objects);
        }
        objects
    }

    pub fn current_context_with_computed_dimension(&self) -> (&Context<T>, Delta<f32>) {
        let len = self.inner.len();
        if len <= 1 {
            if let Some(item) = self.inner.get(&self.current) {
                return (&item.val, self.margin);
            } else if let Some(root) = self.root {
                if let Some(item) = self.inner.get(&root) {
                    return (&item.val, self.margin);
                }
            }
            panic!("Grid is in an invalid state - no contexts available");
        }

        if let Some(current_item) = self.inner.get(&self.current) {
            let objects = self.objects();
            let rich_text_id = current_item.val.rich_text_id;

            let mut margin = self.margin;
            for obj in objects {
                if let Object::RichText(rich_text_obj) = obj {
                    if rich_text_obj.id == rich_text_id {
                        margin.x =
                            rich_text_obj.render_data.position[0] + self.scaled_padding;
                        margin.top_y =
                            rich_text_obj.render_data.position[1] + self.scaled_padding;
                        break;
                    }
                }
            }

            (&current_item.val, margin)
        } else {
            tracing::error!("Current key {:?} not found in grid", self.current);
            if let Some(root) = self.root {
                if let Some(item) = self.inner.get(&root) {
                    return (&item.val, self.margin);
                }
            }
            panic!("Grid is in an invalid state - no contexts available");
        }
    }

    #[inline]
    pub fn select_current_based_on_mouse(&mut self, mouse: &Mouse) -> bool {
        let len = self.inner.len();
        if len <= 1 {
            return false;
        }

        let objects = self.objects();
        let mut select_new_current = None;
        for obj in objects {
            if let Object::RichText(rich_text_obj) = obj {
                if let Some(key) = self.find_by_rich_text_id(rich_text_obj.id) {
                    if let Some(item) = self.inner.get(&key) {
                        let scale_factor = self.scaled_padding / self.padding_panel;
                        let scaled_position_x = rich_text_obj.render_data.position[0] * scale_factor;
                        let scaled_position_y = rich_text_obj.render_data.position[1] * scale_factor;
                        if mouse.x >= scaled_position_x as usize
                            && mouse.y >= scaled_position_y as usize
                            && mouse.x
                                <= (scaled_position_x + item.val.dimension.width) as usize
                            && mouse.y
                                <= (scaled_position_y + item.val.dimension.height)
                                    as usize
                        {
                            select_new_current = Some(key);
                            break;
                        }
                    }
                }
            }
        }

        if let Some(new_current) = select_new_current {
            self.current = new_current;
            return true;
        }

        false
    }

    pub fn find_by_rich_text_id(&self, searched_rich_text_id: usize) -> Option<usize> {
        for (key, item) in &self.inner {
            if item.val.rich_text_id == searched_rich_text_id {
                return Some(*key);
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
                self.margin,
            )
        } else {
            tracing::error!("Current key {:?} not found in grid", self.current);
            ContextDimension::default()
        }
    }

    pub fn plot_objects(&self, objects: &mut Vec<Object>) {
        if self.inner.is_empty() {
            return;
        }
        if let Some(root) = self.root {
            self.plot_objects_recursive(objects, root);
        }
    }

    fn plot_objects_recursive(&self, objects: &mut Vec<Object>, key: usize) {
        if let Some(item) = self.inner.get(&key) {
            // Add pre-computed rich text object
            objects.push(item.rich_text_object.clone());

            let item_pos = item.position();

            // Always create horizontal border
            objects.push(create_border(
                self.border_color,
                [
                    item_pos[0],
                    item_pos[1]
                        + (item.val.dimension.height
                            / item.val.dimension.dimension.scale),
                ],
                [
                    item.val.dimension.width / item.val.dimension.dimension.scale,
                    1.,
                ],
            ));

            // Recurse down if child exists
            if let Some(down_key) = item.down {
                self.plot_objects_recursive(objects, down_key);
            }

            // Always create vertical border
            objects.push(create_border(
                self.border_color,
                [
                    item_pos[0]
                        + (item.val.dimension.width / item.val.dimension.dimension.scale),
                    item_pos[1],
                ],
                [
                    1.,
                    item.val.dimension.height / item.val.dimension.dimension.scale,
                ],
            ));

            // Recurse right if child exists
            if let Some(right_key) = item.right {
                self.plot_objects_recursive(objects, right_key);
            }
        }
    }

    pub fn update_margin(&mut self, padding: (f32, f32, f32)) {
        self.margin = Delta {
            x: padding.0,
            top_y: padding.1,
            bottom_y: padding.2,
        };
        for context in self.inner.values_mut() {
            context.val.dimension.update_margin(self.margin);
        }
    }

    pub fn update_line_height(&mut self, line_height: f32) {
        for context in self.inner.values_mut() {
            context.val.dimension.update_line_height(line_height);
        }
    }

    pub fn update_dimensions(&mut self, sugarloaf: &Sugarloaf) {
        for context in self.inner.values_mut() {
            if let Some(layout) = sugarloaf.get_text_layout(&context.val.rich_text_id) {
                context.val.dimension.update_dimensions(layout.dimensions);
            }
        }
        // Update scaled_padding from the first context (they should all have the same scale)
        if let Some(root) = self.root {
            if let Some(first_context) = self.inner.get(&root) {
                self.scaled_padding =
                    self.padding_panel * first_context.val.dimension.dimension.scale;
            }
        }
        self.calculate_positions();
    }

    pub fn resize(&mut self, new_width: f32, new_height: f32) {
        let width_difference = new_width - self.width;
        let height_difference = new_height - self.height;
        self.width = new_width;
        self.height = new_height;

        // Create a map to store resize deltas for each key
        let mut resize_deltas: std::collections::HashMap<usize, (f32, f32)> =
            std::collections::HashMap::new();

        if let Some(root) = self.root {
            self.resize_context_slotmap(
                &mut resize_deltas,
                root,
                width_difference,
                height_difference,
            );
        }

        // Apply the resize deltas
        for (key, (width_delta, height_delta)) in resize_deltas {
            if let Some(context) = self.inner.get_mut(&key) {
                let current_width = context.val.dimension.width;
                context
                    .val
                    .dimension
                    .update_width(current_width + width_delta);

                let current_height = context.val.dimension.height;
                context
                    .val
                    .dimension
                    .update_height(current_height + height_delta);

                let mut terminal = context.val.terminal.lock();
                terminal.resize::<ContextDimension>(context.val.dimension);
                drop(terminal);
                let winsize =
                    crate::renderer::utils::terminal_dimensions(&context.val.dimension);
                let _ = context.val.messenger.send_resize(winsize);
            }
        }

        // All nodes are affected by resize
        let all_keys: Vec<usize> = self.inner.keys().cloned().collect();
        self.calculate_positions_for_affected_nodes(&all_keys);
    }

    // Updated resize_context to work with slotmap
    fn resize_context_slotmap(
        &self,
        resize_deltas: &mut std::collections::HashMap<usize, (f32, f32)>,
        key: usize,
        available_width: f32,
        available_height: f32,
    ) -> (f32, f32) {
        if let Some(item) = self.inner.get(&key) {
            let mut current_available_width = available_width;
            let mut current_available_height = available_height;

            if let Some(right_key) = item.right {
                let (new_available_width, _) = self.resize_context_slotmap(
                    resize_deltas,
                    right_key,
                    available_width / 2.,
                    available_height,
                );
                current_available_width = new_available_width;
            }

            if let Some(down_key) = item.down {
                let (_, new_available_height) = self.resize_context_slotmap(
                    resize_deltas,
                    down_key,
                    available_width,
                    available_height / 2.,
                );
                current_available_height = new_available_height;
            }

            resize_deltas
                .insert(key, (current_available_width, current_available_height));
            return (current_available_width, current_available_height);
        }

        (available_width, available_height)
    }

    fn request_resize(&mut self, key: usize) {
        if let Some(item) = self.inner.get_mut(&key) {
            let mut terminal = item.val.terminal.lock();
            terminal.resize::<ContextDimension>(item.val.dimension);
            drop(terminal);
            let winsize =
                crate::renderer::utils::terminal_dimensions(&item.val.dimension);
            let _ = item.val.messenger.send_resize(winsize);
        }
    }

    /// Calculate and update positions for all grid items
    pub fn calculate_positions(&mut self) {
        if self.inner.is_empty() {
            return;
        }
        if let Some(root) = self.root {
            self.calculate_positions_recursive(root, self.margin);
        }
    }

    /// Calculate positions only for affected nodes and their children
    pub fn calculate_positions_for_affected_nodes(&mut self, affected_keys: &[usize]) {
        if self.inner.is_empty() {
            return;
        }

        // For each affected node, we need to recalculate its position and all its children
        for &key in affected_keys {
            if self.inner.contains_key(&key) {
                // Find the position this node should have based on its parent
                let margin = self.find_node_margin(key);
                self.calculate_positions_recursive(key, margin);
            }
        }
    }

    /// Find the margin/position a node should have based on its parent
    fn find_node_margin(&self, key: usize) -> Delta<f32> {
        // If it's the root node, use the grid margin
        if Some(key) == self.root {
            return self.margin;
        }

        // Get the current node to check its parent reference
        if let Some(node) = self.inner.get(&key) {
            if let Some(parent_key) = node.parent {
                if let Some(parent) = self.inner.get(&parent_key) {
                    // Determine if this node is a right or down child
                    if parent.right == Some(key) {
                        // This is a right child
                        let parent_pos = parent.position();
                        return Delta {
                            x: parent_pos[0]
                                + self.scaled_padding
                                + (parent.val.dimension.width
                                    / parent.val.dimension.dimension.scale),
                            top_y: parent_pos[1],
                            bottom_y: self.margin.bottom_y,
                        };
                    } else if parent.down == Some(key) {
                        // This is a down child
                        let parent_pos = parent.position();
                        return Delta {
                            x: parent_pos[0],
                            top_y: parent_pos[1]
                                + self.scaled_padding
                                + (parent.val.dimension.height
                                    / parent.val.dimension.dimension.scale),
                            bottom_y: self.margin.bottom_y,
                        };
                    }
                }
            }
        }

        // Fallback to grid margin if parent not found
        self.margin
    }

    /// Recursively calculate positions for grid items
    fn calculate_positions_recursive(&mut self, key: usize, margin: Delta<f32>) {
        if let Some(item) = self.inner.get_mut(&key) {
            // Set position for current item in the rich text object
            item.set_position([margin.x, margin.top_y]);

            // Calculate margin for down item
            let down_margin = Delta {
                x: margin.x,
                top_y: margin.top_y
                    + self.scaled_padding
                    + (item.val.dimension.height / item.val.dimension.dimension.scale),
                bottom_y: margin.bottom_y,
            };

            // Calculate margin for right item
            let right_margin = Delta {
                x: margin.x
                    + self.scaled_padding
                    + (item.val.dimension.width / item.val.dimension.dimension.scale),
                top_y: margin.top_y,
                bottom_y: margin.bottom_y,
            };

            // Store the down and right keys to avoid borrowing issues
            let down_key = item.down;
            let right_key = item.right;

            // Recursively calculate positions for child items
            if let Some(down_key) = down_key {
                self.calculate_positions_recursive(down_key, down_margin);
            }

            if let Some(right_key) = right_key {
                self.calculate_positions_recursive(right_key, right_margin);
            }
        }
    }

    pub fn remove_current(&mut self, sugarloaf: &mut Sugarloaf) {
        if self.inner.is_empty() {
            tracing::error!("Attempted to remove from empty grid");
            return;
        }

        if !self.inner.contains_key(&self.current) {
            tracing::error!("Current key {:?} not found in grid", self.current);
            if let Some(root) = self.root {
                self.current = root;
            }
            return;
        }

        // If there's only one context, we can't remove it
        if self.inner.len() == 1 {
            tracing::warn!("Cannot remove the last remaining context");
            return;
        }

        let to_be_removed = self.current;
        let (to_be_removed_width, to_be_removed_height) = {
            if let Some(item) = self.inner.get(&to_be_removed) {
                (
                    item.val.dimension.width + self.margin.x,
                    item.val.dimension.height,
                )
            } else {
                return;
            }
        };

        // Find parent context if it exists
        let mut parent_context = None;
        if Some(to_be_removed) != self.root {
            if let Some(node) = self.inner.get(&to_be_removed) {
                if let Some(parent_key) = node.parent {
                    if let Some(parent) = self.inner.get(&parent_key) {
                        // Determine if this is a right or down child
                        if parent.right == Some(to_be_removed) {
                            parent_context = Some((true, parent_key));
                        } else if parent.down == Some(to_be_removed) {
                            parent_context = Some((false, parent_key));
                        }
                    }
                }
            }
        }

        // Handle removal with parent context
        if let Some((is_right, parent_key)) = parent_context {
            self.handle_removal_with_parent(
                to_be_removed,
                parent_key,
                is_right,
                to_be_removed_width,
                to_be_removed_height,
                self.scaled_padding,
                sugarloaf,
            );
            self.calculate_positions_for_affected_nodes(&[parent_key]);
            return;
        }

        // Handle removal without parent (root context)
        self.handle_root_removal(
            to_be_removed,
            to_be_removed_height,
            self.scaled_padding,
            sugarloaf,
        );
        if let Some(root) = self.root {
            self.calculate_positions_for_affected_nodes(&[root]);
        }
    }

    fn handle_removal_with_parent(
        &mut self,
        to_be_removed: usize,
        parent_key: usize,
        is_right: bool,
        to_be_removed_width: f32,
        to_be_removed_height: f32,
        scaled_padding: f32,
        sugarloaf: &mut Sugarloaf,
    ) {
        if !self.inner.contains_key(&parent_key) {
            tracing::error!("Parent key {:?} not found in grid", parent_key);
            return;
        }

        let mut next_current = parent_key;

        if is_right {
            // Handle right child removal
            let current_down = self.inner.get(&to_be_removed).and_then(|item| item.down);
            if let Some(current_down) = current_down {
                if self.inner.contains_key(&current_down) {
                    if let Some(item) = self.inner.get_mut(&current_down) {
                        item.val
                            .dimension
                            .increase_height(to_be_removed_height + scaled_padding);
                    }

                    let to_be_remove_right =
                        self.inner.get(&to_be_removed).and_then(|item| item.right);
                    self.request_resize(current_down);
                    self.remove_key(to_be_removed, sugarloaf);

                    next_current = current_down;

                    // Handle right inheritance
                    if let Some(right_val) = to_be_remove_right {
                        self.inherit_right_children(
                            next_current,
                            right_val,
                            to_be_removed_height,
                            scaled_padding,
                        );
                    }

                    if let Some(parent) = self.inner.get_mut(&parent_key) {
                        parent.right = Some(next_current);
                    }
                    // Update parent reference for the new child
                    if let Some(new_child) = self.inner.get_mut(&next_current) {
                        new_child.parent = Some(parent_key);
                    }
                    self.current = next_current;
                    return;
                }
            }

            // No down children, expand parent
            let to_be_removed_right =
                self.inner.get(&to_be_removed).and_then(|item| item.right);
            if let Some(parent) = self.inner.get_mut(&parent_key) {
                let parent_width = parent.val.dimension.width;
                parent
                    .val
                    .dimension
                    .update_width(parent_width + to_be_removed_width + scaled_padding);
                parent.right = to_be_removed_right;
            }
            // Update parent reference for inherited right child
            if let Some(inherited_right) = to_be_removed_right {
                if let Some(inherited_child) = self.inner.get_mut(&inherited_right) {
                    inherited_child.parent = Some(parent_key);
                }
            }
            self.request_resize(parent_key);
        } else {
            // Handle down child removal
            let current_right =
                self.inner.get(&to_be_removed).and_then(|item| item.right);
            if let Some(current_right) = current_right {
                if self.inner.contains_key(&current_right) {
                    if let Some(item) = self.inner.get_mut(&current_right) {
                        item.val
                            .dimension
                            .increase_width(to_be_removed_width + scaled_padding);
                    }

                    self.request_resize(current_right);
                    next_current = current_right;

                    if let Some(parent) = self.inner.get_mut(&parent_key) {
                        parent.down = Some(next_current);
                    }
                    // Update parent reference for the new child
                    if let Some(new_child) = self.inner.get_mut(&next_current) {
                        new_child.parent = Some(parent_key);
                    }
                } else {
                    // Invalid right reference, just expand parent
                    let to_be_removed_down =
                        self.inner.get(&to_be_removed).and_then(|item| item.down);
                    if let Some(parent) = self.inner.get_mut(&parent_key) {
                        let parent_height = parent.val.dimension.height;
                        parent.val.dimension.update_height(
                            parent_height + to_be_removed_height + scaled_padding,
                        );
                        parent.down = to_be_removed_down;
                    }
                    // Update parent reference for inherited down child
                    if let Some(inherited_down) = to_be_removed_down {
                        if let Some(inherited_child) = self.inner.get_mut(&inherited_down)
                        {
                            inherited_child.parent = Some(parent_key);
                        }
                    }
                    self.request_resize(parent_key);
                }
            } else {
                // No right children, expand parent
                let to_be_removed_down =
                    self.inner.get(&to_be_removed).and_then(|item| item.down);
                if let Some(parent) = self.inner.get_mut(&parent_key) {
                    let parent_height = parent.val.dimension.height;
                    parent.val.dimension.update_height(
                        parent_height + to_be_removed_height + scaled_padding,
                    );
                    parent.down = to_be_removed_down;
                }
                // Update parent reference for inherited down child
                if let Some(inherited_down) = to_be_removed_down {
                    if let Some(inherited_child) = self.inner.get_mut(&inherited_down) {
                        inherited_child.parent = Some(parent_key);
                    }
                }
                self.request_resize(parent_key);
            }
        }

        self.remove_key(to_be_removed, sugarloaf);
        self.current = next_current;
    }

    fn handle_root_removal(
        &mut self,
        to_be_removed: usize,
        to_be_removed_height: f32,
        scaled_padding: f32,
        sugarloaf: &mut Sugarloaf,
    ) {
        // Priority: down items first, then right items
        let down_val = self.inner.get(&to_be_removed).and_then(|item| item.down);
        if let Some(down_val) = down_val {
            if self.inner.contains_key(&down_val) {
                if let Some(down_item) = self.inner.get_mut(&down_val) {
                    let down_height = down_item.val.dimension.height;
                    down_item.val.dimension.update_height(
                        down_height + to_be_removed_height + scaled_padding,
                    );
                }

                let to_be_removed_right_item =
                    self.inner.get(&to_be_removed).and_then(|item| item.right);

                // Get rich_text_id before removing
                let rich_text_id = if let Some(item) = self.inner.get(&to_be_removed) {
                    item.val.rich_text_id
                } else {
                    return;
                };

                // Move down item to root position by swapping the data
                if let (Some(_to_be_removed_item), Some(mut down_item)) = (
                    self.inner.remove(&to_be_removed),
                    self.inner.remove(&down_val),
                ) {
                    // Cleanup rich text from sugarloaf
                    sugarloaf.remove_content(rich_text_id);
                    // Clear parent reference since this becomes the new root
                    down_item.parent = None;

                    // Insert the down item as the new root
                    let new_root = down_item.val.route_id;
                    self.inner.insert(new_root, down_item);
                    self.root = Some(new_root);
                    self.current = new_root;

                    self.request_resize(new_root);

                    // Handle right inheritance
                    if let Some(right_val) = to_be_removed_right_item {
                        self.inherit_right_children(
                            new_root,
                            right_val,
                            to_be_removed_height,
                            scaled_padding,
                        );
                    }
                }
                return;
            }
        }

        let right_val = self.inner.get(&to_be_removed).and_then(|item| item.right);
        if let Some(right_val) = right_val {
            if self.inner.contains_key(&right_val) {
                let (right_width, to_be_removed_width) = {
                    let right_item = self.inner.get(&right_val).unwrap();
                    let to_be_removed_item = self.inner.get(&to_be_removed).unwrap();
                    (
                        right_item.val.dimension.width,
                        to_be_removed_item.val.dimension.width + self.margin.x,
                    )
                };

                if let Some(right_item) = self.inner.get_mut(&right_val) {
                    right_item
                        .val
                        .dimension
                        .update_width(right_width + to_be_removed_width + scaled_padding);
                }

                // Get rich_text_id before removing
                let rich_text_id = if let Some(item) = self.inner.get(&to_be_removed) {
                    item.val.rich_text_id
                } else {
                    return;
                };

                // Move right item to root position
                if let (Some(_to_be_removed_item), Some(right_item)) = (
                    self.inner.remove(&to_be_removed),
                    self.inner.remove(&right_val),
                ) {
                    // Cleanup rich text from sugarloaf
                    sugarloaf.remove_content(rich_text_id);

                    let new_root = right_item.val.route_id;
                    self.inner.insert(new_root, right_item);
                    self.root = Some(new_root);
                    self.current = new_root;

                    self.request_resize(new_root);
                }
                return;
            }
        }

        // Fallback: just remove the item
        // Get rich_text_id before removing
        if let Some(item) = self.inner.get(&to_be_removed) {
            let rich_text_id = item.val.rich_text_id;
            self.inner.remove(&to_be_removed);
            sugarloaf.remove_content(rich_text_id);
        }
        if let Some(first_key) = self.inner.keys().next() {
            self.current = *first_key;
            self.root = Some(*first_key);
        }
    }

    fn inherit_right_children(
        &mut self,
        base_key: usize,
        right_val: usize,
        height_increase: f32,
        scaled_padding: f32,
    ) {
        if !self.inner.contains_key(&base_key) || !self.inner.contains_key(&right_val) {
            return;
        }

        let mut last_right = None;
        let mut right_ptr = self.inner.get(&base_key).and_then(|item| item.right);

        // Find the last right item and resize all
        while let Some(right_key) = right_ptr {
            if !self.inner.contains_key(&right_key) {
                break;
            }

            last_right = Some(right_key);
            if let Some(item) = self.inner.get_mut(&right_key) {
                let last_right_height = item.val.dimension.height;
                item.val
                    .dimension
                    .update_height(last_right_height + height_increase + scaled_padding);
            }
            self.request_resize(right_key);
            right_ptr = self.inner.get(&right_key).and_then(|item| item.right);
        }

        // Attach the inherited right chain
        if let Some(last_right_val) = last_right {
            if let Some(item) = self.inner.get_mut(&last_right_val) {
                item.right = Some(right_val);
            }
        } else if let Some(item) = self.inner.get_mut(&base_key) {
            item.right = Some(right_val);
        }
    }

    fn remove_key(&mut self, key: usize, sugarloaf: &mut Sugarloaf) {
        if !self.inner.contains_key(&key) {
            tracing::error!("Attempted to remove key {:?} which doesn't exist", key);
            return;
        }

        // Get rich_text_id before removing
        let rich_text_id = if let Some(item) = self.inner.get(&key) {
            item.val.rich_text_id
        } else {
            return;
        };

        // Update all references to this key
        let keys_to_update: Vec<usize> = self.inner.keys().cloned().collect();
        for update_key in keys_to_update {
            if update_key == key {
                continue;
            }

            if let Some(context) = self.inner.get_mut(&update_key) {
                if let Some(right_val) = context.right {
                    if right_val == key {
                        // The referenced context is being removed
                        context.right = None;
                    }
                }

                if let Some(down_val) = context.down {
                    if down_val == key {
                        // The referenced context is being removed
                        context.down = None;
                    }
                }
            }
        }

        self.inner.remove(&key);

        // Cleanup rich text from sugarloaf
        sugarloaf.remove_content(rich_text_id);

        // Update root if necessary
        if Some(key) == self.root {
            self.root = self.inner.keys().next().copied();
        }

        // Ensure current key is still valid
        if self.current == key {
            if let Some(new_current) = self.root {
                self.current = new_current;
            } else if let Some(first_key) = self.inner.keys().next() {
                self.current = *first_key;
            }
        }
    }

    pub fn split_right(&mut self, context: Context<T>, sugarloaf: &mut Sugarloaf) {
        let current_item = if let Some(item) = self.inner.get(&self.current) {
            item
        } else {
            return;
        };

        let old_right = current_item.right;
        let old_grid_item_height = current_item.val.dimension.height;
        let old_grid_item_width = current_item.val.dimension.width - self.margin.x;
        let new_grid_item_width = old_grid_item_width / 2.0;
        let current_key = self.current;

        // Update current item width
        if let Some(current_item) = self.inner.get_mut(&self.current) {
            current_item
                .val
                .dimension
                .update_width(new_grid_item_width - self.scaled_padding);

            // The current dimension margin should reset
            // otherwise will add a space before the rect
            let mut new_margin = current_item.val.dimension.margin;
            new_margin.x = 0.0;
            current_item.val.dimension.update_margin(new_margin);
        }

        self.request_resize(self.current);

        let mut new_context = ContextGridItem::new(context);
        new_context.val.dimension.update_width(new_grid_item_width);
        new_context
            .val
            .dimension
            .update_height(old_grid_item_height);

        let new_key = new_context.val.route_id;
        self.inner.insert(new_key, new_context);

        // Update relationships
        if let Some(new_item) = self.inner.get_mut(&new_key) {
            new_item.right = old_right;
            new_item.parent = Some(self.current); // Set parent reference
        }
        if let Some(current_item) = self.inner.get_mut(&self.current) {
            current_item.right = Some(new_key);
        }

        // Update parent reference for old_right if it exists
        if let Some(old_right_key) = old_right {
            if let Some(old_right_item) = self.inner.get_mut(&old_right_key) {
                old_right_item.parent = Some(new_key);
            }
        }

        self.current = new_key;

        // In case the new context does not have right
        // it means it's the last one, for this case
        // whenever a margin exists then we need to add
        // half of margin to respect margin.x border on
        // the right side.
        if let Some(new_item) = self.inner.get_mut(&new_key) {
            if new_item.right.is_none() {
                let mut new_margin = new_item.val.dimension.margin;
                new_margin.x = self.margin.x / 2.0;
                new_item.val.dimension.update_margin(new_margin);
            }
        }

        self.request_resize(new_key);
        self.calculate_positions_for_affected_nodes(&[current_key, new_key]);

        // Update sugarloaf positions for affected contexts
        if let Some(current_item) = self.inner.get(&current_key) {
            let pos = current_item.position();
            sugarloaf.set_position(current_item.val.rich_text_id, pos[0], pos[1]);
            sugarloaf.set_visibility(current_item.val.rich_text_id, true);
        }
        if let Some(new_item) = self.inner.get(&new_key) {
            let pos = new_item.position();
            sugarloaf.set_position(new_item.val.rich_text_id, pos[0], pos[1]);
            sugarloaf.set_visibility(new_item.val.rich_text_id, true);
        }
    }

    pub fn split_down(&mut self, context: Context<T>, sugarloaf: &mut Sugarloaf) {
        let current_item = if let Some(item) = self.inner.get(&self.current) {
            item
        } else {
            return;
        };

        let old_down = current_item.down;
        let old_grid_item_height = current_item.val.dimension.height;
        let old_grid_item_width = current_item.val.dimension.width;
        let new_grid_item_height = old_grid_item_height / 2.0;
        let current_key = self.current;

        // Update current item
        if let Some(current_item) = self.inner.get_mut(&self.current) {
            current_item
                .val
                .dimension
                .update_height(new_grid_item_height - self.scaled_padding);

            // The current dimension margin should reset
            // otherwise will add a space before the rect
            let mut new_margin = current_item.val.dimension.margin;
            new_margin.bottom_y = 0.0;
            current_item.val.dimension.update_margin(new_margin);
        }

        self.request_resize(self.current);

        let mut new_context = ContextGridItem::new(context);
        new_context
            .val
            .dimension
            .update_height(new_grid_item_height);
        new_context.val.dimension.update_width(old_grid_item_width);

        let new_key = new_context.val.route_id;
        self.inner.insert(new_key, new_context);

        // Update relationships
        if let Some(new_item) = self.inner.get_mut(&new_key) {
            new_item.down = old_down;
            new_item.parent = Some(self.current); // Set parent reference
        }
        if let Some(current_item) = self.inner.get_mut(&self.current) {
            current_item.down = Some(new_key);
        }

        // Update parent reference for old_down if it exists
        if let Some(old_down_key) = old_down {
            if let Some(old_down_item) = self.inner.get_mut(&old_down_key) {
                old_down_item.parent = Some(new_key);
            }
        }

        self.current = new_key;

        // TODO: Needs to validate this
        // In case the new context does not have down
        // it means it's the last one, for this case
        // whenever a margin exists then we need to add
        // margin to respect margin.top_y and margin.bottom_y
        // borders on the bottom side.
        if let Some(new_item) = self.inner.get_mut(&new_key) {
            if new_item.down.is_none() {
                let mut new_margin = new_item.val.dimension.margin;
                new_margin.bottom_y = self.margin.bottom_y;
                new_item.val.dimension.update_margin(new_margin);
            }
        }

        self.request_resize(new_key);
        self.calculate_positions_for_affected_nodes(&[current_key, new_key]);

        // Update sugarloaf positions for affected contexts
        if let Some(current_item) = self.inner.get(&current_key) {
            let pos = current_item.position();
            sugarloaf.set_position(current_item.val.rich_text_id, pos[0], pos[1]);
            sugarloaf.set_visibility(current_item.val.rich_text_id, true);
        }
        if let Some(new_item) = self.inner.get(&new_key) {
            let pos = new_item.position();
            sugarloaf.set_position(new_item.val.rich_text_id, pos[0], pos[1]);
            sugarloaf.set_visibility(new_item.val.rich_text_id, true);
        }
    }

    /// Move divider up - decreases height of current split and increases height of split above
    pub fn move_divider_up(&mut self, amount: f32) -> bool {
        if self.inner.len() <= 1 {
            return false;
        }

        let current_key = self.current;
        if !self.inner.contains_key(&current_key) {
            tracing::error!("Current key {:?} not found in grid", current_key);
            return false;
        }

        // Strategy: Find any vertically adjacent split and adjust the divider between them
        // Case 1: Current split has a parent above (current is a down child)
        if let Some(current_item) = self.inner.get(&current_key) {
            if let Some(parent_key) = current_item.parent {
                if let Some(parent) = self.inner.get(&parent_key) {
                    if parent.down == Some(current_key) {
                        let (current_height, parent_height) = {
                            let current_item = self.inner.get(&current_key).unwrap();
                            let parent_item = self.inner.get(&parent_key).unwrap();
                            (
                                current_item.val.dimension.height,
                                parent_item.val.dimension.height,
                            )
                        };

                        let min_height = 50.0;
                        if current_height - amount < min_height
                            || parent_height + amount < min_height
                        {
                            return false;
                        }

                        // Shrink current, expand parent (above)
                        if let Some(current_item) = self.inner.get_mut(&current_key) {
                            current_item
                                .val
                                .dimension
                                .update_height(current_height - amount);
                        }
                        if let Some(parent_item) = self.inner.get_mut(&parent_key) {
                            parent_item
                                .val
                                .dimension
                                .update_height(parent_height + amount);
                        }

                        self.request_resize(current_key);
                        self.request_resize(parent_key);

                        // Update positions for affected nodes
                        self.calculate_positions_for_affected_nodes(&[
                            current_key,
                            parent_key,
                        ]);
                        return true;
                    }
                }
            }
        }

        // Case 2: Current split has a down child - move the divider between current and down child
        let down_child_key = self.inner.get(&current_key).and_then(|item| item.down);
        if let Some(down_child_key) = down_child_key {
            if self.inner.contains_key(&down_child_key) {
                let (current_height, down_height) = {
                    let current_item = self.inner.get(&current_key).unwrap();
                    let down_item = self.inner.get(&down_child_key).unwrap();
                    (
                        current_item.val.dimension.height,
                        down_item.val.dimension.height,
                    )
                };

                let min_height = 50.0;
                if current_height - amount < min_height
                    || down_height + amount < min_height
                {
                    return false;
                }

                // Shrink current, expand down child
                if let Some(current_item) = self.inner.get_mut(&current_key) {
                    current_item
                        .val
                        .dimension
                        .update_height(current_height - amount);
                }
                if let Some(down_item) = self.inner.get_mut(&down_child_key) {
                    down_item.val.dimension.update_height(down_height + amount);
                }

                self.request_resize(current_key);
                self.request_resize(down_child_key);

                // Update positions for affected nodes
                self.calculate_positions_for_affected_nodes(&[
                    current_key,
                    down_child_key,
                ]);
                return true;
            }
        }

        false
    }

    /// Move divider down - increases height of current split and decreases height of split above
    pub fn move_divider_down(&mut self, amount: f32) -> bool {
        if self.inner.len() <= 1 {
            return false;
        }

        let current_key = self.current;
        if !self.inner.contains_key(&current_key) {
            tracing::error!("Current key {:?} not found in grid", current_key);
            return false;
        }

        // Strategy: Find any vertically adjacent split and adjust the divider between them
        // Case 1: Current split has a parent above (current is a down child)
        if let Some(current_item) = self.inner.get(&current_key) {
            if let Some(parent_key) = current_item.parent {
                if let Some(parent) = self.inner.get(&parent_key) {
                    if parent.down == Some(current_key) {
                        let (current_height, parent_height) = {
                            let current_item = self.inner.get(&current_key).unwrap();
                            let parent_item = self.inner.get(&parent_key).unwrap();
                            (
                                current_item.val.dimension.height,
                                parent_item.val.dimension.height,
                            )
                        };

                        let min_height = 50.0;
                        if current_height + amount < min_height
                            || parent_height - amount < min_height
                        {
                            return false;
                        }

                        // Expand current, shrink parent (above) - divider moves down
                        if let Some(current_item) = self.inner.get_mut(&current_key) {
                            current_item
                                .val
                                .dimension
                                .update_height(current_height + amount);
                        }
                        if let Some(parent_item) = self.inner.get_mut(&parent_key) {
                            parent_item
                                .val
                                .dimension
                                .update_height(parent_height - amount);
                        }

                        self.request_resize(current_key);
                        self.request_resize(parent_key);

                        // Update positions for affected nodes
                        self.calculate_positions_for_affected_nodes(&[
                            current_key,
                            parent_key,
                        ]);
                        return true;
                    }
                }
            }
        }

        // Case 2: Current split has a down child - move the divider between current and down child
        let down_child_key = self.inner.get(&current_key).and_then(|item| item.down);
        if let Some(down_child_key) = down_child_key {
            if self.inner.contains_key(&down_child_key) {
                let (current_height, down_height) = {
                    let current_item = self.inner.get(&current_key).unwrap();
                    let down_item = self.inner.get(&down_child_key).unwrap();
                    (
                        current_item.val.dimension.height,
                        down_item.val.dimension.height,
                    )
                };

                let min_height = 50.0;
                if current_height + amount < min_height
                    || down_height - amount < min_height
                {
                    return false;
                }

                // Expand current, shrink down child - divider moves down
                if let Some(current_item) = self.inner.get_mut(&current_key) {
                    current_item
                        .val
                        .dimension
                        .update_height(current_height + amount);
                }
                if let Some(down_item) = self.inner.get_mut(&down_child_key) {
                    down_item.val.dimension.update_height(down_height - amount);
                }

                self.request_resize(current_key);
                self.request_resize(down_child_key);

                // Update positions for affected nodes
                self.calculate_positions_for_affected_nodes(&[
                    current_key,
                    down_child_key,
                ]);
                return true;
            }
        }

        false
    }

    /// Move divider left - shrinks current split and expands the split to the left
    pub fn move_divider_left(&mut self, amount: f32) -> bool {
        if self.inner.len() <= 1 {
            return false;
        }

        let current_key = self.current;
        if !self.inner.contains_key(&current_key) {
            tracing::error!("Current key {:?} not found in grid", current_key);
            return false;
        }

        // Find horizontally adjacent splits
        let mut left_split = None;
        let mut right_split = None;

        // Case 1: Current split is a right child - its parent is to the left
        if let Some(current_item) = self.inner.get(&current_key) {
            if let Some(parent_key) = current_item.parent {
                if let Some(parent) = self.inner.get(&parent_key) {
                    if parent.right == Some(current_key) {
                        left_split = Some(parent_key);
                        right_split = Some(current_key);
                    }
                }
            }
        }

        // Case 2: Current split has a right child - current is left, child is right
        if left_split.is_none() {
            let right_child_key =
                self.inner.get(&current_key).and_then(|item| item.right);
            if let Some(right_child_key) = right_child_key {
                if self.inner.contains_key(&right_child_key) {
                    left_split = Some(current_key);
                    right_split = Some(right_child_key);
                }
            }
        }

        // Case 3: Current split is a down child - check if its parent has horizontal relationships
        if left_split.is_none() {
            if let Some(current_item) = self.inner.get(&current_key) {
                if let Some(parent_key) = current_item.parent {
                    if let Some(parent) = self.inner.get(&parent_key) {
                        if parent.down == Some(current_key) {
                            // Current is a down child, check if parent has horizontal relationships
                            if let Some(grandparent_key) = parent.parent {
                                if let Some(grandparent) =
                                    self.inner.get(&grandparent_key)
                                {
                                    if grandparent.right == Some(parent_key) {
                                        // Parent is a right child, so grandparent is to the left
                                        left_split = Some(grandparent_key);
                                        right_split = Some(parent_key);
                                    }
                                }
                            }

                            // Also check if parent has a right child
                            if left_split.is_none() {
                                if let Some(parent_right) = parent.right {
                                    left_split = Some(parent_key);
                                    right_split = Some(parent_right);
                                }
                            }
                        }
                    }
                }
            }
        }

        if let (Some(left_key), Some(right_key)) = (left_split, right_split) {
            let (left_width, right_width) = {
                let left_item = self.inner.get(&left_key).unwrap();
                let right_item = self.inner.get(&right_key).unwrap();
                (
                    left_item.val.dimension.width,
                    right_item.val.dimension.width,
                )
            };

            let min_width = 100.0;
            if left_width - amount < min_width || right_width + amount < min_width {
                return false;
            }

            // Move divider left: shrink left split, expand right split
            if let Some(left_item) = self.inner.get_mut(&left_key) {
                left_item.val.dimension.update_width(left_width - amount);
            }
            if let Some(right_item) = self.inner.get_mut(&right_key) {
                right_item.val.dimension.update_width(right_width + amount);
            }

            // Update all children in the vertical stacks to match their parent's width
            self.update_children_width(left_key, left_width - amount);
            self.update_children_width(right_key, right_width + amount);

            self.request_resize(left_key);
            self.request_resize(right_key);

            // Collect all affected nodes (parents and their children)
            let mut affected_nodes = vec![left_key, right_key];
            self.collect_all_children(left_key, &mut affected_nodes);
            self.collect_all_children(right_key, &mut affected_nodes);

            // Update positions for affected nodes
            self.calculate_positions_for_affected_nodes(&affected_nodes);
            return true;
        }

        false
    }

    /// Move divider right - expands current split and shrinks the split to the right
    pub fn move_divider_right(&mut self, amount: f32) -> bool {
        if self.inner.len() <= 1 {
            return false;
        }

        let current_key = self.current;
        if !self.inner.contains_key(&current_key) {
            tracing::error!("Current key {:?} not found in grid", current_key);
            return false;
        }

        // Find horizontally adjacent splits
        let mut left_split = None;
        let mut right_split = None;

        // Case 1: Current split is a right child - its parent is to the left
        if let Some(current_item) = self.inner.get(&current_key) {
            if let Some(parent_key) = current_item.parent {
                if let Some(parent) = self.inner.get(&parent_key) {
                    if parent.right == Some(current_key) {
                        left_split = Some(parent_key);
                        right_split = Some(current_key);
                    }
                }
            }
        }

        // Case 2: Current split has a right child - current is left, child is right
        if left_split.is_none() {
            let right_child_key =
                self.inner.get(&current_key).and_then(|item| item.right);
            if let Some(right_child_key) = right_child_key {
                if self.inner.contains_key(&right_child_key) {
                    left_split = Some(current_key);
                    right_split = Some(right_child_key);
                }
            }
        }

        // Case 3: Current split is a down child - check if its parent has horizontal relationships
        if left_split.is_none() {
            if let Some(current_item) = self.inner.get(&current_key) {
                if let Some(parent_key) = current_item.parent {
                    if let Some(parent) = self.inner.get(&parent_key) {
                        if parent.down == Some(current_key) {
                            // Current is a down child, check if parent has horizontal relationships
                            if let Some(grandparent_key) = parent.parent {
                                if let Some(grandparent) =
                                    self.inner.get(&grandparent_key)
                                {
                                    if grandparent.right == Some(parent_key) {
                                        // Parent is a right child, so grandparent is to the left
                                        left_split = Some(grandparent_key);
                                        right_split = Some(parent_key);
                                    }
                                }
                            }

                            // Also check if parent has a right child
                            if left_split.is_none() {
                                if let Some(parent_right) = parent.right {
                                    left_split = Some(parent_key);
                                    right_split = Some(parent_right);
                                }
                            }
                        }
                    }
                }
            }
        }

        if let (Some(left_key), Some(right_key)) = (left_split, right_split) {
            let (left_width, right_width) = {
                let left_item = self.inner.get(&left_key).unwrap();
                let right_item = self.inner.get(&right_key).unwrap();
                (
                    left_item.val.dimension.width,
                    right_item.val.dimension.width,
                )
            };

            let min_width = 100.0;
            if left_width + amount < min_width || right_width - amount < min_width {
                return false;
            }

            // Move divider right: expand left split, shrink right split
            if let Some(left_item) = self.inner.get_mut(&left_key) {
                left_item.val.dimension.update_width(left_width + amount);
            }
            if let Some(right_item) = self.inner.get_mut(&right_key) {
                right_item.val.dimension.update_width(right_width - amount);
            }

            // Update all children in the vertical stacks to match their parent's width
            self.update_children_width(left_key, left_width + amount);
            self.update_children_width(right_key, right_width - amount);

            self.request_resize(left_key);
            self.request_resize(right_key);

            // Collect all affected nodes (parents and their children)
            let mut affected_nodes = vec![left_key, right_key];
            self.collect_all_children(left_key, &mut affected_nodes);
            self.collect_all_children(right_key, &mut affected_nodes);

            // Update positions for affected nodes
            self.calculate_positions_for_affected_nodes(&affected_nodes);
            return true;
        }

        false
    }

    /// Update the width of all children in a vertical stack to match the parent's width
    fn update_children_width(&mut self, parent_key: usize, new_width: f32) {
        // Find all down children and update their width
        if let Some(parent) = self.inner.get(&parent_key) {
            if let Some(down_key) = parent.down {
                self.update_children_width_recursive(down_key, new_width);
            }
        }
    }

    /// Recursively update width for all nodes in a vertical chain
    fn update_children_width_recursive(&mut self, key: usize, new_width: f32) {
        let down_key = if let Some(item) = self.inner.get_mut(&key) {
            item.val.dimension.update_width(new_width);
            item.down
        } else {
            return;
        };

        self.request_resize(key);

        // Continue down the chain
        if let Some(down_key) = down_key {
            self.update_children_width_recursive(down_key, new_width);
        }
    }

    /// Collect all children (down and right) of a given node
    fn collect_all_children(&self, parent_key: usize, affected_nodes: &mut Vec<usize>) {
        if let Some(parent) = self.inner.get(&parent_key) {
            if let Some(right_key) = parent.right {
                if !affected_nodes.contains(&right_key) {
                    affected_nodes.push(right_key);
                    self.collect_all_children(right_key, affected_nodes);
                }
            }
            if let Some(down_key) = parent.down {
                if !affected_nodes.contains(&down_key) {
                    affected_nodes.push(down_key);
                    self.collect_all_children(down_key, affected_nodes);
                }
            }
        }
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
    pub margin: Delta<f32>,
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
            margin: Delta::<f32>::default(),
        }
    }
}

impl ContextDimension {
    pub fn build(
        width: f32,
        height: f32,
        dimension: TextDimensions,
        line_height: f32,
        margin: Delta<f32>,
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
    pub fn update_margin(&mut self, margin: Delta<f32>) {
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
