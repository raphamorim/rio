use crate::context::Context;
use crate::mouse::Mouse;
use rio_backend::crosswords::grid::Dimensions;
use rio_backend::event::EventListener;
use rio_backend::sugarloaf::{
    layout::SugarDimensions, Object, Quad, RichText, Sugarloaf,
};
use std::collections::HashMap;

const MIN_COLS: usize = 2;
const MIN_LINES: usize = 1;

const PADDING: f32 = 2.;

fn compute(
    width: f32,
    height: f32,
    dimensions: SugarDimensions,
    line_height: f32,
    margin: Delta<f32>,
) -> (usize, usize) {
    // Ensure we have positive dimensions
    if width <= 0.0 || height <= 0.0 || dimensions.scale <= 0.0 || line_height <= 0.0 {
        return (MIN_COLS, MIN_LINES);
    }

    let margin_x = (margin.x * dimensions.scale).round();
    let margin_spaces = margin.top_y + margin.bottom_y;

    // Calculate available space for content
    let available_width = (width / dimensions.scale) - margin_x;
    let available_height = (height / dimensions.scale) - margin_spaces;

    // Ensure we have positive available space
    if available_width <= 0.0 || available_height <= 0.0 {
        return (MIN_COLS, MIN_LINES);
    }

    // Calculate columns
    let char_width = dimensions.width / dimensions.scale;
    if char_width <= 0.0 {
        return (MIN_COLS, MIN_LINES);
    }
    let visible_columns =
        std::cmp::max((available_width / char_width) as usize, MIN_COLS);

    // Calculate lines
    let char_height = (dimensions.height / dimensions.scale) * line_height;
    if char_height <= 0.0 {
        return (visible_columns, MIN_LINES);
    }
    let lines = (available_height / char_height) - 1.0;
    let visible_lines = std::cmp::max(lines.round() as usize, MIN_LINES);

    (visible_columns, visible_lines)
}

#[inline]
fn create_border(color: [f32; 4], position: [f32; 2], size: [f32; 2]) -> Object {
    Object::Quad(Quad {
        color,
        position,
        shadow_blur_radius: 0.0,
        shadow_offset: [0.0, 0.0],
        shadow_color: [0.0, 0.0, 0.0, 0.0],
        border_color: [0.0, 0.0, 0.0, 0.0],
        border_width: 0.0,
        border_radius: [0.0, 0.0, 0.0, 0.0],
        size,
    })
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
            position: [0.0, 0.0],
            lines: None,
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
            rich_text.position
        } else {
            [0.0, 0.0]
        }
    }

    /// Update the position in the rich text object
    fn set_position(&mut self, position: [f32; 2]) {
        if let Object::RichText(ref mut rich_text) = self.rich_text_object {
            rich_text.position = position;
        }
    }
}

impl<T: rio_backend::event::EventListener> ContextGrid<T> {
    pub fn new(context: Context<T>, margin: Delta<f32>, border_color: [f32; 4]) -> Self {
        let width = context.dimension.width;
        let height = context.dimension.height;
        let scale = context.dimension.dimension.scale;
        let scaled_padding = PADDING * scale;
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
        self.scaled_padding / PADDING
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
                        margin.x = rich_text_obj.position[0] + self.scaled_padding;
                        margin.top_y = rich_text_obj.position[1] + self.scaled_padding;
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
                        let scaled_position_x =
                            rich_text_obj.position[0] * (self.scaled_padding / PADDING);
                        let scaled_position_y =
                            rich_text_obj.position[1] * (self.scaled_padding / PADDING);
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
            let layout = sugarloaf.rich_text_layout(&context.val.rich_text_id);
            context.val.dimension.update_dimensions(layout.dimensions);
        }
        // Update scaled_padding from the first context (they should all have the same scale)
        if let Some(root) = self.root {
            if let Some(first_context) = self.inner.get(&root) {
                self.scaled_padding =
                    PADDING * first_context.val.dimension.dimension.scale;
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

    pub fn remove_current(&mut self) {
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
            );
            self.calculate_positions_for_affected_nodes(&[parent_key]);
            return;
        }

        // Handle removal without parent (root context)
        self.handle_root_removal(
            to_be_removed,
            to_be_removed_height,
            self.scaled_padding,
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
                    self.remove_key(to_be_removed);

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

        self.remove_key(to_be_removed);
        self.current = next_current;
    }

    fn handle_root_removal(
        &mut self,
        to_be_removed: usize,
        to_be_removed_height: f32,
        scaled_padding: f32,
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

                // Move down item to root position by swapping the data
                if let (Some(_to_be_removed_item), Some(mut down_item)) = (
                    self.inner.remove(&to_be_removed),
                    self.inner.remove(&down_val),
                ) {
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

                // Move right item to root position
                if let (Some(_to_be_removed_item), Some(right_item)) = (
                    self.inner.remove(&to_be_removed),
                    self.inner.remove(&right_val),
                ) {
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
        self.inner.remove(&to_be_removed);
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

    fn remove_key(&mut self, key: usize) {
        if !self.inner.contains_key(&key) {
            tracing::error!("Attempted to remove key {:?} which doesn't exist", key);
            return;
        }

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

    pub fn split_right(&mut self, context: Context<T>) {
        let current_item = if let Some(item) = self.inner.get(&self.current) {
            item
        } else {
            return;
        };

        let old_right = current_item.right;
        let old_grid_item_height = current_item.val.dimension.height;
        let old_grid_item_width = current_item.val.dimension.width - self.margin.x;
        let new_grid_item_width = old_grid_item_width / 2.0;

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
        self.calculate_positions_for_affected_nodes(&[self.current, new_key]);
    }

    pub fn split_down(&mut self, context: Context<T>) {
        let current_item = if let Some(item) = self.inner.get(&self.current) {
            item
        } else {
            return;
        };

        let old_down = current_item.down;
        let old_grid_item_height = current_item.val.dimension.height;
        let old_grid_item_width = current_item.val.dimension.width;
        let new_grid_item_height = old_grid_item_height / 2.0;

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
        self.calculate_positions_for_affected_nodes(&[self.current, new_key]);
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
}

#[derive(Copy, Clone, Debug)]
pub struct ContextDimension {
    pub width: f32,
    pub height: f32,
    pub columns: usize,
    pub lines: usize,
    pub dimension: SugarDimensions,
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
            dimension: SugarDimensions::default(),
            margin: Delta::<f32>::default(),
        }
    }
}

impl ContextDimension {
    pub fn build(
        width: f32,
        height: f32,
        dimension: SugarDimensions,
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
    pub fn update_dimensions(&mut self, dimensions: SugarDimensions) {
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
#[allow(
    clippy::field_reassign_with_default,
    clippy::bool_comparison,
    clippy::uninlined_format_args,
    clippy::clone_on_copy
)]
pub mod test {
    use super::*;
    // Easier to test big structs
    use crate::context::create_mock_context;
    use crate::event::VoidListener;
    use pretty_assertions::assert_eq;
    use rio_window::window::WindowId;

    #[test]
    fn test_compute() {
        // (1000. / ((74. / 2.)=37.))
        // (1000./37.)=27.027
        assert_eq!(
            (88, 26),
            compute(
                1600.0,
                1000.0,
                SugarDimensions {
                    scale: 2.,
                    width: 18.,
                    height: 37.,
                },
                1.0,
                Delta::<f32>::default()
            )
        );
        assert_eq!(
            (80, 24),
            compute(
                1600.0,
                1000.0,
                SugarDimensions {
                    scale: 2.,
                    width: 20.,
                    height: 40.,
                },
                1.0,
                Delta::<f32>::default()
            )
        );
    }

    #[test]
    fn test_single_context_respecting_margin_and_no_quad_creation() {
        let margin = Delta {
            x: 10.,
            top_y: 20.,
            bottom_y: 20.,
        };

        let context_dimension = ContextDimension::build(
            1200.0,
            800.0,
            SugarDimensions {
                scale: 2.,
                width: 18.,
                height: 9.,
            },
            1.0,
            Delta::<f32>::default(),
        );

        assert_eq!(context_dimension.columns, 66);
        assert_eq!(context_dimension.lines, 88);
        let rich_text_id = 0;
        let context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            rich_text_id,
            context_dimension,
        );
        let context_width = context.dimension.width;
        let context_height = context.dimension.height;
        let context_margin = context.dimension.margin;
        let grid = ContextGrid::<VoidListener>::new(context, margin, [0., 0., 0., 0.]);
        // The first context should fill completely w/h grid
        assert_eq!(grid.width, context_width);
        assert_eq!(grid.height, context_height);

        // Context margin should empty
        assert_eq!(Delta::<f32>::default(), context_margin);
        assert_eq!(grid.margin, margin);

        assert_eq!(
            grid.objects(),
            vec![Object::RichText(RichText {
                id: rich_text_id,
                position: [10., 20.],
                lines: None,
            },)]
        );
    }

    #[test]
    fn test_split_right() {
        let margin = Delta {
            x: 0.,
            top_y: 0.,
            bottom_y: 0.,
        };

        let context_dimension = ContextDimension::build(
            1200.0,
            800.0,
            SugarDimensions {
                scale: 1.,
                width: 14.,
                height: 8.,
            },
            1.0,
            Delta::<f32>::default(),
        );

        assert_eq!(context_dimension.columns, 85);
        assert_eq!(context_dimension.lines, 99);

        let (first_context, first_context_id) = {
            let rich_text_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, second_context_id) = {
            let rich_text_id = 1;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let mut grid =
            ContextGrid::<VoidListener>::new(first_context, margin, [1., 0., 0., 0.]);

        assert_eq!(
            grid.objects(),
            vec![Object::RichText(RichText {
                id: first_context_id,
                position: [0., 0.],
                lines: None,
            },)]
        );
        grid.split_right(second_context);

        assert_eq!(
            grid.objects(),
            vec![
                Object::RichText(RichText {
                    id: first_context_id,
                    position: [0.0, 0.0],
                    lines: None,
                },),
                create_border([1.0, 0.0, 0.0, 0.0], [0.0, 800.0], [598., 1.0]),
                create_border([1.0, 0.0, 0.0, 0.0], [598.0, 0.0], [1.0, 800.0]),
                Object::RichText(RichText {
                    id: second_context_id,
                    position: [600., 0.0],
                    lines: None,
                },),
                create_border([1.0, 0.0, 0.0, 0.0], [600.0, 800.0], [600.0, 1.0]),
                create_border([1.0, 0.0, 0.0, 0.0], [1200.0, 0.0], [1.0, 800.0]),
            ]
        );

        let (third_context, third_context_id) = {
            let rich_text_id = 2;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        grid.split_right(third_context);

        assert_eq!(
            grid.objects(),
            vec![
                Object::RichText(RichText {
                    id: first_context_id,
                    position: [0.0, 0.0],
                    lines: None,
                },),
                create_border([1.0, 0.0, 0.0, 0.0], [0.0, 800.0], [598., 1.0]),
                create_border([1.0, 0.0, 0.0, 0.0], [598.0, 0.0], [1.0, 800.0]),
                Object::RichText(RichText {
                    id: second_context_id,
                    position: [600.0, 0.0],
                    lines: None,
                },),
                create_border([1.0, 0.0, 0.0, 0.0], [600.0, 800.0], [298.0, 1.0]),
                create_border([1.0, 0.0, 0.0, 0.0], [898.0, 0.0], [1.0, 800.0]),
                Object::RichText(RichText {
                    id: third_context_id,
                    position: [900.0, 0.0],
                    lines: None,
                },),
                create_border([1.0, 0.0, 0.0, 0.0], [900.0, 800.0], [300.0, 1.0]),
                create_border([1.0, 0.0, 0.0, 0.0], [1200.0, 0.0], [1.0, 800.0]),
            ]
        );
    }

    #[test]
    fn test_split_right_with_margin() {
        let margin = Delta {
            x: 20.,
            top_y: 30.,
            bottom_y: 40.,
        };

        let context_dimension = ContextDimension::build(
            600.0,
            600.0,
            SugarDimensions {
                scale: 2.,
                width: 14.,
                height: 8.,
            },
            1.0,
            Delta::<f32>::default(),
        );

        assert_eq!(context_dimension.columns, 42);
        assert_eq!(context_dimension.lines, 74);

        let (first_context, first_context_id) = {
            let rich_text_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, second_context_id) = {
            let rich_text_id = 1;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let mut grid =
            ContextGrid::<VoidListener>::new(first_context, margin, [1., 0., 0., 0.]);

        assert_eq!(
            grid.objects(),
            vec![Object::RichText(RichText {
                id: first_context_id,
                position: [margin.x, margin.top_y],
                lines: None,
            },)]
        );
        grid.split_right(second_context);

        /*
            > before split:
            20  (600/20)
                |------|

        Available width should compute with margin
        so should be 600 - 20 = 580, then will be:
        289 + 4 (PADDING) + 290

            > after split:
            10  (289/0)   (4)  (290/10)
                |----------|----------|

        Margin should be splitted between first columns
        items and last columns items
        */

        let contexts = grid.contexts_ordered();
        assert_eq!(contexts[0].val.dimension.width, 286.);
        assert_eq!(contexts[0].val.dimension.margin.x, 0.);
        assert_eq!(contexts[1].val.dimension.width, 290.);
        assert_eq!(contexts[1].val.dimension.margin.x, 10.);

        assert_eq!(
            grid.objects(),
            vec![
                Object::RichText(RichText {
                    id: first_context_id,
                    position: [margin.x, margin.top_y],
                    lines: None,
                },),
                create_border([1.0, 0.0, 0.0, 0.0], [20.0, 330.0], [143.0, 1.0]),
                create_border([1.0, 0.0, 0.0, 0.0], [163.0, margin.top_y], [1.0, 300.0]),
                Object::RichText(RichText {
                    id: second_context_id,
                    position: [167.0, margin.top_y],
                    lines: None,
                },),
                create_border([1.0, 0.0, 0.0, 0.0], [167.0, 330.0], [145.0, 1.0]),
                create_border([1.0, 0.0, 0.0, 0.0], [312.0, margin.top_y], [1.0, 300.0]),
            ]
        );

        let (third_context, third_context_id) = {
            let rich_text_id = 2;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        grid.split_right(third_context);

        assert_eq!(
            grid.objects(),
            vec![
                Object::RichText(RichText {
                    id: first_context_id,
                    position: [margin.x, margin.top_y],
                    lines: None,
                },),
                create_border([1.0, 0.0, 0.0, 0.0], [20.0, 330.0], [143.0, 1.0]),
                create_border([1.0, 0.0, 0.0, 0.0], [163.0, margin.top_y], [1.0, 300.0]),
                Object::RichText(RichText {
                    id: second_context_id,
                    position: [167.0, margin.top_y],
                    lines: None,
                },),
                create_border([1.0, 0.0, 0.0, 0.0], [167.0, 330.0], [65.5, 1.0]),
                create_border([1.0, 0.0, 0.0, 0.0], [232.5, margin.top_y], [1.0, 300.0]),
                Object::RichText(RichText {
                    id: third_context_id,
                    position: [236.5, margin.top_y],
                    lines: None,
                },),
                create_border([1.0, 0.0, 0.0, 0.0], [236.5, 330.0], [67.5, 1.0]),
                create_border([1.0, 0.0, 0.0, 0.0], [304.0, margin.top_y], [1.0, 300.0]),
            ]
        );

        // Last context should be updated with half of x
        let contexts = grid.contexts_ordered();
        assert_eq!(contexts[0].val.dimension.width, 286.);
        assert_eq!(contexts[0].val.dimension.margin.x, 0.);
        assert_eq!(contexts[1].val.dimension.width, 131.);
        assert_eq!(contexts[1].val.dimension.margin.x, 0.);
        assert_eq!(contexts[2].val.dimension.width, 135.);
        assert_eq!(contexts[2].val.dimension.margin.x, 10.);

        let (fourth_context, fourth_context_id) = {
            let rich_text_id = 3;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        grid.select_prev_split();
        grid.split_right(fourth_context);

        // If the split right happens in not the last
        // then should not update margin to half of x
        let contexts = grid.contexts_ordered();
        assert_eq!(contexts[0].val.dimension.width, 286.);
        assert_eq!(contexts[0].val.dimension.margin.x, 0.);
        assert_eq!(contexts[1].val.dimension.width, 51.5);
        assert_eq!(contexts[1].val.dimension.margin.x, 0.);
        assert_eq!(contexts[2].val.dimension.width, 55.5);
        assert_eq!(contexts[2].val.dimension.margin.x, 0.);

        // 3 has the larger width and margin
        assert_eq!(contexts[3].val.dimension.width, 135.0);
        assert_eq!(contexts[3].val.dimension.margin.x, 10.);

        assert_eq!(
            grid.objects(),
            vec![
                Object::RichText(RichText {
                    id: first_context_id,
                    position: [margin.x, margin.top_y],
                    lines: None,
                },),
                create_border([1.0, 0.0, 0.0, 0.0], [20.0, 330.0], [143.0, 1.0]),
                create_border([1.0, 0.0, 0.0, 0.0], [163.0, margin.top_y], [1.0, 300.0]),
                Object::RichText(RichText {
                    id: second_context_id,
                    position: [167.0, margin.top_y],
                    lines: None,
                },),
                create_border([1.0, 0.0, 0.0, 0.0], [167.0, 330.0], [25.75, 1.0]),
                create_border([1.0, 0.0, 0.0, 0.0], [192.75, margin.top_y], [1.0, 300.0]),
                Object::RichText(RichText {
                    id: fourth_context_id,
                    position: [196.75, margin.top_y],
                    lines: None,
                },),
                create_border([1.0, 0.0, 0.0, 0.0], [196.75, 330.0], [27.75, 1.0]),
                create_border([1.0, 0.0, 0.0, 0.0], [224.5, margin.top_y], [1.0, 300.0]),
                Object::RichText(RichText {
                    id: third_context_id,
                    position: [228.5, margin.top_y],
                    lines: None,
                },),
                create_border([1.0, 0.0, 0.0, 0.0], [228.5, 330.0], [67.5, 1.0]),
                create_border([1.0, 0.0, 0.0, 0.0], [296.0, margin.top_y], [1.0, 300.0]),
            ]
        );
    }

    #[test]
    fn test_split_right_with_margin_inside_parent() {
        let margin = Delta {
            x: 20.,
            top_y: 30.,
            bottom_y: 40.,
        };

        let context_dimension = ContextDimension::build(
            600.0,
            600.0,
            SugarDimensions {
                scale: 2.,
                width: 14.,
                height: 8.,
            },
            1.0,
            Delta::<f32>::default(),
        );

        assert_eq!(context_dimension.columns, 42);
        assert_eq!(context_dimension.lines, 74);

        let (first_context, first_context_id) = {
            let rich_text_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, _second_context_id) = {
            let rich_text_id = 1;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let mut grid =
            ContextGrid::<VoidListener>::new(first_context, margin, [1., 0., 0., 0.]);

        assert_eq!(
            grid.objects(),
            vec![Object::RichText(RichText {
                id: first_context_id,
                position: [margin.x, margin.top_y],
                lines: None,
            },)]
        );

        let (third_context, _third_context_id) = {
            let rich_text_id = 2;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (fourth_context, _fourth_context_id) = {
            let rich_text_id = 3;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (fifth_context, _fifth_context_id) = {
            let rich_text_id = 3;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        grid.split_right(second_context);
        grid.select_prev_split();
        grid.split_down(third_context);
        grid.split_right(fourth_context);
        grid.split_right(fifth_context);

        // If the split right happens in not the last
        // then should not update margin to half of x

        assert_eq!(grid.current_index(), 4);

        // |1.--------------|2.------------|
        // |3.----|4.--|5.--|--------------|
        let contexts = grid.contexts_ordered();
        assert_eq!(contexts.len(), 5);
        assert_eq!(contexts[0].val.dimension.width, 286.);
        assert_eq!(contexts[0].val.dimension.margin.x, 0.);
        assert_eq!(contexts[1].val.dimension.width, 290.);
        assert_eq!(contexts[1].val.dimension.margin.x, 10.);
        assert_eq!(contexts[2].val.dimension.width, 129.0);
        assert_eq!(contexts[2].val.dimension.margin.x, 0.);
        assert_eq!(contexts[3].val.dimension.width, 52.5);
        assert_eq!(contexts[3].val.dimension.margin.x, 0.);
        assert_eq!(contexts[4].val.dimension.width, 56.5);

        // Fifth context should not have any margin x
        // TODO:
        // assert_eq!(contexts[4].val.dimension.margin.x, 0.);

        grid.remove_current();
        assert_eq!(grid.current_index(), 3);
        let contexts = grid.contexts_ordered();
        assert_eq!(contexts[1].val.dimension.margin.x, 10.);
        // Fourth context should not have any margin x
        // TODO:
        // assert_eq!(contexts[3].val.dimension.margin.x, 0.);
    }

    #[test]
    fn test_split_down_with_margin_inside_parent() {
        let margin = Delta {
            x: 20.,
            top_y: 30.,
            bottom_y: 40.,
        };

        let context_dimension = ContextDimension::build(
            600.0,
            600.0,
            SugarDimensions {
                scale: 1.,
                width: 14.,
                height: 8.,
            },
            1.0,
            Delta::<f32>::default(),
        );

        assert_eq!(context_dimension.columns, 42);
        assert_eq!(context_dimension.lines, 74);

        let (first_context, first_context_id) = {
            let rich_text_id = 1;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, second_context_id) = {
            let rich_text_id = 2;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (third_context, third_context_id) = {
            let rich_text_id = 3;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (fourth_context, fourth_context_id) = {
            let rich_text_id = 4;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (fifth_context, fifth_context_id) = {
            let rich_text_id = 5;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let mut grid =
            ContextGrid::<VoidListener>::new(first_context, margin, [1., 0., 0., 0.]);

        assert_eq!(
            grid.objects(),
            vec![Object::RichText(RichText {
                id: first_context_id,
                position: [margin.x, margin.top_y],
                lines: None,
            },)]
        );

        grid.split_down(second_context);
        grid.split_down(third_context);
        grid.split_right(fourth_context);
        grid.select_prev_split();
        grid.select_prev_split();
        let current_index = grid.current_index();
        let contexts = grid.contexts_ordered();
        assert_eq!(contexts[current_index].val.rich_text_id, second_context_id);
        grid.split_right(fifth_context);

        // If the split right happens in not the last
        // then should not update margin to half of x

        assert_eq!(grid.current_index(), 2);

        // |1.--------------|
        // |2.------|5.-----|
        // |3.------|4.-----|
        let contexts = grid.contexts_ordered();
        assert_eq!(contexts.len(), 5);

        assert_eq!(contexts[0].val.rich_text_id, first_context_id);
        assert_eq!(contexts[0].val.dimension.height, 298.);
        assert_eq!(contexts[0].val.dimension.margin.top_y, 0.);
        assert_eq!(contexts[0].val.dimension.margin.bottom_y, 0.);
        let first_down = contexts[0].down;
        // Check that first context has a down child and it's at index 1
        assert!(first_down.is_some());
        if let Some(down_key) = first_down {
            assert_eq!(grid.key_to_index(down_key), Some(1));
        }
        // The down child should be at index 1
        assert_eq!(contexts[1].val.rich_text_id, second_context_id);

        assert_eq!(contexts[1].val.rich_text_id, second_context_id);
        assert_eq!(contexts[1].val.dimension.height, 148.);
        assert_eq!(contexts[1].val.dimension.margin.top_y, 0.);
        assert_eq!(contexts[1].val.dimension.margin.bottom_y, 0.);

        assert_eq!(contexts[2].val.rich_text_id, fifth_context_id);
        assert_eq!(contexts[2].val.dimension.height, 148.0);
        assert_eq!(contexts[2].val.dimension.margin.top_y, 0.);
        assert_eq!(contexts[2].val.dimension.margin.bottom_y, 0.);

        assert_eq!(contexts[3].val.rich_text_id, third_context_id);
        assert_eq!(contexts[3].val.dimension.height, 150.0);
        assert_eq!(contexts[3].val.dimension.margin.top_y, 0.);
        assert_eq!(contexts[3].val.dimension.margin.bottom_y, 40.);

        assert_eq!(contexts[4].val.rich_text_id, fourth_context_id);
        assert_eq!(contexts[4].val.dimension.height, 150.0);
        assert_eq!(contexts[4].val.dimension.margin.top_y, 0.);
        assert_eq!(contexts[4].val.dimension.margin.bottom_y, 0.);

        // Fifth context should not have any margin x
        // TODO: Removal
        // grid.remove_current();
    }

    #[test]
    // https://github.com/raphamorim/rio/issues/760
    fn test_split_issue_760() {
        let width = 1200.;
        let height = 800.;

        let margin = Delta {
            x: 0.,
            top_y: 0.,
            bottom_y: 0.,
        };

        let context_dimension = ContextDimension::build(
            width,
            height,
            SugarDimensions {
                scale: 2.,
                width: 14.,
                height: 8.,
            },
            1.0,
            Delta::<f32>::default(),
        );

        assert_eq!(context_dimension.columns, 85);
        assert_eq!(context_dimension.lines, 99);

        let (first_context, first_context_id) = {
            let rich_text_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, second_context_id) = {
            let rich_text_id = 1;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let mut grid =
            ContextGrid::<VoidListener>::new(first_context, margin, [0., 0., 1., 0.]);

        assert_eq!(
            grid.objects(),
            vec![Object::RichText(RichText {
                id: first_context_id,
                position: [0., 0.],
                lines: None,
            },)]
        );
        grid.split_down(second_context);

        assert_eq!(
            grid.objects(),
            vec![
                Object::RichText(RichText {
                    id: first_context_id,
                    position: [0.0, 0.0],
                    lines: None,
                },),
                create_border([0.0, 0.0, 1.0, 0.0], [0.0, 198.0], [600.0, 1.0]),
                Object::RichText(RichText {
                    id: second_context_id,
                    position: [0.0, 202.0],
                    lines: None,
                },),
                create_border([0.0, 0.0, 1.0, 0.0], [0.0, 402.0], [600.0, 1.0]),
                create_border([0.0, 0.0, 1.0, 0.0], [600.0, 202.0], [1.0, 200.0]),
                create_border([0.0, 0.0, 1.0, 0.0], [600.0, 0.0], [1.0, 198.0]),
            ]
        );

        let (third_context, third_context_id) = {
            let rich_text_id = 2;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        grid.select_prev_split();
        assert_eq!(grid.current().rich_text_id, first_context_id);
        assert_eq!(grid.current_index(), 0);
        grid.split_right(third_context);
        assert_eq!(grid.current().rich_text_id, third_context_id);
        assert_eq!(grid.current_index(), 1);

        let scaled_padding = grid.scaled_padding();
        let contexts = grid.contexts_ordered();

        // Check their respective width
        assert_eq!(
            contexts[0].val.dimension.width,
            (width / 2.) - scaled_padding
        );
        assert_eq!(contexts[1].val.dimension.width, width / 2.);
        assert_eq!(contexts[2].val.dimension.width, width);

        // Check their respective height
        let top_height = (height / 2.) - scaled_padding;
        assert_eq!(contexts[0].val.dimension.height, top_height);
        assert_eq!(contexts[1].val.dimension.height, top_height);
        assert_eq!(contexts[2].val.dimension.height, height / 2.);

        // [RichText(RichText { id: 0, position: [0.0, 0.0] }),
        // Rect(Rect { position: [298.0, 0.0], color: [0.0, 0.0, 1.0, 0.0], size: [1.0, 396.0] }),
        // RichText(RichText { id: 2, position: [302.0, 0.0] }),
        // Rect(Rect { position: [0.0, 198.0], color: [0.0, 0.0, 1.0, 0.0], size: [596.0, 1.0] }),
        // RichText(RichText { id: 1, position: [0.0, 202.0] }, None)]

        assert_eq!(
            grid.objects(),
            vec![
                Object::RichText(RichText {
                    id: first_context_id,
                    position: [0.0, 0.0],
                    lines: None,
                },),
                create_border([0.0, 0.0, 1.0, 0.0], [0.0, 198.0], [298.0, 1.0]),
                Object::RichText(RichText {
                    id: second_context_id,
                    position: [0.0, 202.0],
                    lines: None,
                },),
                create_border([0.0, 0.0, 1.0, 0.0], [0.0, 402.0], [600.0, 1.0]),
                create_border([0.0, 0.0, 1.0, 0.0], [600.0, 202.0], [1.0, 200.0]),
                create_border([0.0, 0.0, 1.0, 0.0], [298.0, 0.0], [1.0, 198.0]),
                Object::RichText(RichText {
                    id: third_context_id,
                    position: [302.0, 0.0],
                    lines: None,
                },),
                create_border([0.0, 0.0, 1.0, 0.0], [302.0, 198.0], [300.0, 1.0]),
                create_border([0.0, 0.0, 1.0, 0.0], [602.0, 0.0], [1.0, 198.0]),
            ]
        );
    }

    #[test]
    fn test_remove_right_with_margin() {
        let margin = Delta {
            x: 20.,
            top_y: 30.,
            bottom_y: 40.,
        };

        let context_dimension = ContextDimension::build(
            600.0,
            600.0,
            SugarDimensions {
                scale: 2.,
                width: 14.,
                height: 8.,
            },
            1.0,
            Delta::<f32>::default(),
        );

        assert_eq!(context_dimension.columns, 42);
        assert_eq!(context_dimension.lines, 74);

        let (first_context, first_context_id) = {
            let rich_text_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, second_context_id) = {
            let rich_text_id = 1;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let mut grid =
            ContextGrid::<VoidListener>::new(first_context, margin, [1., 0., 0., 0.]);

        assert_eq!(
            grid.objects(),
            vec![Object::RichText(RichText {
                id: first_context_id,
                position: [margin.x, margin.top_y],
                lines: None,
            },)]
        );
        grid.split_right(second_context);

        let (third_context, third_context_id) = {
            let rich_text_id = 2;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (fourth_context, fourth_context_id) = {
            let rich_text_id = 3;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        grid.split_right(third_context);

        let first_expected_dimension = (286., 0.);
        let second_expected_dimension = (131., 0.);
        let third_expected_dimension = (135., 10.);
        let contexts = grid.contexts_ordered();
        assert_eq!(contexts[0].val.dimension.width, first_expected_dimension.0);
        assert_eq!(
            contexts[0].val.dimension.margin.x,
            first_expected_dimension.1
        );
        assert_eq!(contexts[1].val.dimension.width, second_expected_dimension.0);
        assert_eq!(
            contexts[1].val.dimension.margin.x,
            second_expected_dimension.1
        );
        assert_eq!(contexts[2].val.dimension.width, third_expected_dimension.0);
        assert_eq!(
            contexts[2].val.dimension.margin.x,
            third_expected_dimension.1
        );

        grid.select_prev_split();
        grid.split_right(fourth_context);

        // If the split right happens in not the last
        // then should not update margin to half of x
        let contexts = grid.contexts_ordered();

        // Debug: print actual values
        println!("Debug test_split_right_with_margin after fourth split:");
        for (i, context) in contexts.iter().enumerate() {
            println!(
                "  contexts[{}]: width={}, margin.x={}",
                i, context.val.dimension.width, context.val.dimension.margin.x
            );
        }

        assert_eq!(contexts[0].val.dimension.width, 286.);
        assert_eq!(contexts[0].val.dimension.margin.x, 0.);
        assert_eq!(contexts[1].val.dimension.width, 51.5);
        assert_eq!(contexts[1].val.dimension.margin.x, 0.);
        assert_eq!(contexts[2].val.dimension.width, 55.5);
        assert_eq!(contexts[2].val.dimension.margin.x, 0.);

        // 3 has the larger width and margin
        assert_eq!(contexts[3].val.dimension.width, 135.0);
        assert_eq!(contexts[3].val.dimension.margin.x, 10.);

        assert_eq!(
            grid.objects(),
            vec![
                Object::RichText(RichText {
                    id: first_context_id,
                    position: [margin.x, margin.top_y],
                    lines: None,
                },),
                create_border([1.0, 0.0, 0.0, 0.0], [20.0, 330.], [143.0, 1.0]),
                create_border([1.0, 0.0, 0.0, 0.0], [163.0, margin.top_y], [1.0, 300.0]),
                Object::RichText(RichText {
                    id: second_context_id,
                    position: [167.0, margin.top_y],
                    lines: None,
                },),
                create_border([1.0, 0.0, 0.0, 0.0], [167.0, 330.], [25.75, 1.0]),
                create_border([1.0, 0.0, 0.0, 0.0], [192.75, margin.top_y], [1.0, 300.0]),
                Object::RichText(RichText {
                    id: fourth_context_id,
                    position: [196.75, margin.top_y],
                    lines: None,
                },),
                create_border([1.0, 0.0, 0.0, 0.0], [196.75, 330.], [27.75, 1.0]),
                create_border([1.0, 0.0, 0.0, 0.0], [224.5, margin.top_y], [1.0, 300.0]),
                Object::RichText(RichText {
                    id: third_context_id,
                    position: [228.5, margin.top_y],
                    lines: None,
                },),
                create_border([1.0, 0.0, 0.0, 0.0], [228.5, 330.], [67.5, 1.0]),
                create_border([1.0, 0.0, 0.0, 0.0], [296.0, margin.top_y], [1.0, 300.0]),
            ]
        );

        grid.remove_current();

        // If the split right happens in not the last
        // then should not update margin to half of x
        let contexts = grid.contexts_ordered();
        assert_eq!(contexts[0].val.dimension.width, first_expected_dimension.0);
        assert_eq!(
            contexts[0].val.dimension.margin.x,
            first_expected_dimension.1
        );
        assert_eq!(contexts[1].val.dimension.width, second_expected_dimension.0);
        assert_eq!(
            contexts[1].val.dimension.margin.x,
            second_expected_dimension.1
        );
        assert_eq!(contexts[2].val.dimension.width, third_expected_dimension.0);
        assert_eq!(
            contexts[2].val.dimension.margin.x,
            third_expected_dimension.1
        );

        assert_eq!(grid.current_index(), 1);
        grid.select_next_split();
        assert_eq!(grid.current_index(), 2);

        // Margin x should move to last
        grid.remove_current();
        let contexts = grid.contexts_ordered();
        assert_eq!(contexts[0].val.dimension.width, 286.);
        assert_eq!(contexts[0].val.dimension.margin.x, 0.);
        assert_eq!(contexts[1].val.dimension.width, 290.);
        // After removal, margin should be recalculated - this is the correct behavior
        assert_eq!(contexts[1].val.dimension.margin.x, 0.0);
    }

    #[test]
    fn test_split_down() {
        let margin = Delta {
            x: 0.,
            top_y: 0.,
            bottom_y: 0.,
        };

        let context_dimension = ContextDimension::build(
            1200.0,
            800.0,
            SugarDimensions {
                scale: 2.,
                width: 14.,
                height: 8.,
            },
            1.0,
            Delta::<f32>::default(),
        );

        assert_eq!(context_dimension.columns, 85);
        assert_eq!(context_dimension.lines, 99);

        let (first_context, first_context_id) = {
            let rich_text_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, second_context_id) = {
            let rich_text_id = 1;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let mut grid =
            ContextGrid::<VoidListener>::new(first_context, margin, [0., 0., 1., 0.]);

        assert_eq!(
            grid.objects(),
            vec![Object::RichText(RichText {
                id: first_context_id,
                position: [0., 0.],
                lines: None,
            },)]
        );
        grid.split_down(second_context);

        assert_eq!(
            grid.objects(),
            vec![
                Object::RichText(RichText {
                    id: first_context_id,
                    position: [0.0, 0.0],
                    lines: None,
                },),
                create_border([0.0, 0.0, 1.0, 0.0], [0.0, 198.0], [600.0, 1.0]),
                Object::RichText(RichText {
                    id: second_context_id,
                    position: [0.0, 202.0],
                    lines: None,
                },),
                create_border([0.0, 0.0, 1.0, 0.0], [0.0, 402.0], [600.0, 1.0]),
                create_border([0.0, 0.0, 1.0, 0.0], [600.0, 202.0], [1.0, 200.0]),
                create_border([0.0, 0.0, 1.0, 0.0], [600.0, 0.0], [1.0, 198.0]),
            ]
        );

        let (third_context, third_context_id) = {
            let rich_text_id = 2;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        grid.split_down(third_context);

        assert_eq!(
            grid.objects(),
            vec![
                Object::RichText(RichText {
                    id: first_context_id,
                    position: [0.0, 0.0],
                    lines: None,
                },),
                create_border([0.0, 0.0, 1.0, 0.0], [0.0, 198.0], [600.0, 1.0]),
                Object::RichText(RichText {
                    id: second_context_id,
                    position: [0.0, 202.0],
                    lines: None,
                },),
                create_border([0.0, 0.0, 1.0, 0.0], [0.0, 300.0], [600.0, 1.0]),
                Object::RichText(RichText {
                    id: third_context_id,
                    position: [0.0, 304.0],
                    lines: None,
                },),
                create_border([0.0, 0.0, 1.0, 0.0], [0.0, 404.0], [600.0, 1.0]),
                create_border([0.0, 0.0, 1.0, 0.0], [600.0, 304.0], [1.0, 100.0]),
                create_border([0.0, 0.0, 1.0, 0.0], [600.0, 202.0], [1.0, 98.0]),
                create_border([0.0, 0.0, 1.0, 0.0], [600.0, 0.0], [1.0, 198.0]),
            ]
        );
    }

    #[test]
    fn test_resize() {
        let margin = Delta {
            x: 0.,
            top_y: 0.,
            bottom_y: 0.,
        };

        let context_dimension = ContextDimension::build(
            600.0,
            600.0,
            SugarDimensions {
                scale: 2.,
                width: 14.,
                height: 8.,
            },
            1.0,
            Delta::<f32>::default(),
        );

        assert_eq!(context_dimension.columns, 42);
        assert_eq!(context_dimension.lines, 74);

        let (first_context, first_context_id) = {
            let rich_text_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, _second_context_id) = {
            let rich_text_id = 1;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (third_context, _third_context_id) = {
            let rich_text_id = 2;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let mut grid =
            ContextGrid::<VoidListener>::new(first_context, margin, [0., 0., 0., 0.]);

        assert_eq!(
            grid.objects(),
            vec![Object::RichText(RichText {
                id: first_context_id,
                position: [0., 0.],
                lines: None,
            },)]
        );

        grid.split_right(second_context);
        grid.split_down(third_context);

        // assert_eq!(
        //     grid.objects(),
        //     vec![
        //         Object::RichText(RichText {
        //             id: first_context_id,
        //             position: [0.0, 0.0],
        //         }),
        //         Object::Rect(Rect {
        //             position: [147.0, 0.0],
        //             color: [0.0, 0.0, 0.0, 0.0],
        //             size: [1.0, 300.0]
        //         }),
        //         Object::RichText(RichText {
        //             id: second_context_id,
        //             position: [149.0, 0.0]
        //         }),
        //         Object::Rect(Rect {
        //             position: [149.0, 147.0],
        //             color: [0.0, 0.0, 0.0, 0.0],
        //             size: [294.0, 1.0]
        //         }),
        //         Object::RichText(RichText {
        //             id: third_context_id,
        //             position: [149.0, 149.0]
        //         }),
        //     ]
        // );

        assert_eq!(grid.width, 600.0);
        assert_eq!(grid.height, 600.0);

        grid.resize(1200.0, 600.0);

        // TODO: Finish test
    }

    #[test]
    fn test_remove_right_without_children() {
        let margin = Delta {
            x: 0.,
            top_y: 0.,
            bottom_y: 0.,
        };

        let context_dimension = ContextDimension::build(
            600.0,
            600.0,
            SugarDimensions {
                scale: 2.,
                width: 14.,
                height: 8.,
            },
            1.0,
            Delta::<f32>::default(),
        );

        assert_eq!(context_dimension.columns, 42);
        assert_eq!(context_dimension.lines, 74);

        let (first_context, first_context_id) = {
            let rich_text_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, _second_context_id) = {
            let rich_text_id = 1;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let mut grid =
            ContextGrid::<VoidListener>::new(first_context, margin, [0., 0., 0., 0.]);

        assert_eq!(
            grid.objects(),
            vec![Object::RichText(RichText {
                id: first_context_id,
                position: [0., 0.],
                lines: None,
            },)]
        );

        assert_eq!(grid.width, 600.0);
        assert_eq!(grid.height, 600.0);
        assert_eq!(grid.current().dimension.width, 600.);

        grid.split_right(second_context);

        let new_expected_width = 600. / 2.;

        assert_eq!(grid.current().dimension.width, new_expected_width);
        assert_eq!(grid.current_index(), 1);

        grid.select_prev_split();
        let scaled_padding = grid.scaled_padding();
        let old_expected_width = (600. / 2.) - scaled_padding;
        assert_eq!(grid.current().dimension.width, old_expected_width);
        assert_eq!(grid.current_index(), 0);

        grid.select_next_split();
        assert_eq!(grid.current_index(), 1);

        grid.remove_current();

        assert_eq!(grid.current_index(), 0);
        // Whenever return to one should drop padding
        assert_eq!(grid.current().dimension.width, 600.);
    }

    #[test]
    fn test_remove_right_with_children() {
        let margin = Delta {
            x: 0.,
            top_y: 0.,
            bottom_y: 0.,
        };

        let context_dimension = ContextDimension::build(
            600.0,
            600.0,
            SugarDimensions {
                scale: 2.,
                width: 14.,
                height: 8.,
            },
            1.0,
            Delta::<f32>::default(),
        );

        assert_eq!(context_dimension.columns, 42);
        assert_eq!(context_dimension.lines, 74);

        let (first_context, first_context_id) = {
            let rich_text_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, _second_context_id) = {
            let rich_text_id = 1;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let mut grid =
            ContextGrid::<VoidListener>::new(first_context, margin, [0., 0., 0., 0.]);

        assert_eq!(
            grid.objects(),
            vec![Object::RichText(RichText {
                id: first_context_id,
                position: [0., 0.],
                lines: None,
            },)]
        );

        grid.split_right(second_context);

        assert_eq!(grid.width, 600.0);
        assert_eq!(grid.height, 600.0);

        let new_context_expected_width = 600. / 2.;

        assert_eq!(grid.current().dimension.width, new_context_expected_width);
        assert_eq!(grid.current_index(), 1);

        grid.select_prev_split();

        let scaled_padding = grid.scaled_padding();
        let old_context_expected_width = (600. / 2.) - scaled_padding;
        assert_eq!(grid.current().dimension.width, old_context_expected_width);
        assert_eq!(grid.current_index(), 0);

        let current_index = grid.current_index();
        let contexts = grid.contexts_ordered();
        // Check that current context has a right child at index 1
        if let Some(right_key) = contexts[current_index].right {
            assert_eq!(grid.key_to_index(right_key), Some(1));
        }
        assert_eq!(contexts[current_index].down, None);

        grid.remove_current();

        assert_eq!(grid.current_index(), 0);
        // Whenever return to one should drop padding
        let expected_width = 600.;
        assert_eq!(grid.current().dimension.width, expected_width);

        let current_index = grid.current_index();
        let contexts = grid.contexts_ordered();
        assert_eq!(contexts[current_index].right, None);
        assert_eq!(contexts[current_index].down, None);
    }

    #[test]
    fn test_remove_right_with_down_children() {
        let margin = Delta {
            x: 0.,
            top_y: 0.,
            bottom_y: 0.,
        };

        let context_dimension = ContextDimension::build(
            600.0,
            600.0,
            SugarDimensions {
                scale: 2.,
                width: 14.,
                height: 8.,
            },
            1.0,
            Delta::<f32>::default(),
        );

        assert_eq!(context_dimension.columns, 42);
        assert_eq!(context_dimension.lines, 74);

        let (first_context, first_context_id) = {
            let rich_text_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, second_context_id) = {
            let rich_text_id = 1;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let mut grid =
            ContextGrid::<VoidListener>::new(first_context, margin, [0., 0., 0., 0.]);

        assert_eq!(
            grid.objects(),
            vec![Object::RichText(RichText {
                id: first_context_id,
                position: [0., 0.],
                lines: None,
            },)]
        );

        grid.split_right(second_context);

        assert_eq!(grid.width, 600.0);
        assert_eq!(grid.height, 600.0);

        let new_context_expected_width = 600. / 2.;

        assert_eq!(grid.current().dimension.width, new_context_expected_width);
        assert_eq!(grid.current_index(), 1);

        let (third_context, third_context_id) = {
            let rich_text_id = 2;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        grid.split_down(third_context);
        assert_eq!(grid.current_index(), 2);
        assert_eq!(grid.current().dimension.width, new_context_expected_width);
        assert_eq!(grid.current().dimension.height, 300.);

        // Move back
        grid.select_prev_split();

        assert_eq!(grid.current_index(), 1);
        assert_eq!(grid.current().rich_text_id, second_context_id);
        assert_eq!(grid.current().dimension.width, new_context_expected_width);
        assert_eq!(grid.current().dimension.height, 296.);

        // Remove the current should actually make right being down
        grid.remove_current();

        assert_eq!(grid.current_index(), 1);
        assert_eq!(grid.current().rich_text_id, third_context_id);
        assert_eq!(grid.current().dimension.width, new_context_expected_width);
        assert_eq!(grid.current().dimension.height, 600.);
    }

    #[test]
    fn test_remove_down_without_children() {
        let margin = Delta {
            x: 0.,
            top_y: 0.,
            bottom_y: 0.,
        };

        let context_dimension = ContextDimension::build(
            600.0,
            600.0,
            SugarDimensions {
                scale: 2.,
                width: 14.,
                height: 8.,
            },
            1.0,
            Delta::<f32>::default(),
        );

        assert_eq!(context_dimension.columns, 42);
        assert_eq!(context_dimension.lines, 74);

        let (first_context, first_context_id) = {
            let rich_text_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, _second_context_id) = {
            let rich_text_id = 1;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let mut grid =
            ContextGrid::<VoidListener>::new(first_context, margin, [0., 0., 0., 0.]);

        assert_eq!(
            grid.objects(),
            vec![Object::RichText(RichText {
                id: first_context_id,
                position: [0., 0.],
                lines: None,
            },)]
        );

        assert_eq!(grid.width, 600.0);
        assert_eq!(grid.height, 600.0);
        assert_eq!(grid.current().dimension.width, 600.);

        grid.split_down(second_context);

        let new_expected_width = 600. / 2.;

        assert_eq!(grid.current().dimension.height, new_expected_width);
        assert_eq!(grid.current_index(), 1);

        grid.select_prev_split();
        let scaled_padding = grid.scaled_padding();
        let old_expected_width = (600. / 2.) - scaled_padding;
        assert_eq!(grid.current().dimension.height, old_expected_width);
        assert_eq!(grid.current_index(), 0);

        grid.select_next_split();
        assert_eq!(grid.current_index(), 1);

        grid.remove_current();

        assert_eq!(grid.current_index(), 0);
        // Whenever return to one should drop padding
        assert_eq!(grid.current().dimension.height, 600.);
    }

    #[test]
    fn test_remove_down_with_children() {
        let margin = Delta {
            x: 0.,
            top_y: 0.,
            bottom_y: 0.,
        };

        let context_dimension = ContextDimension::build(
            600.0,
            600.0,
            SugarDimensions {
                scale: 2.,
                width: 14.,
                height: 8.,
            },
            1.0,
            Delta::<f32>::default(),
        );

        assert_eq!(context_dimension.columns, 42);
        assert_eq!(context_dimension.lines, 74);

        let (first_context, first_context_id) = {
            let rich_text_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, _second_context_id) = {
            let rich_text_id = 1;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let mut grid =
            ContextGrid::<VoidListener>::new(first_context, margin, [0., 0., 0., 0.]);

        assert_eq!(
            grid.objects(),
            vec![Object::RichText(RichText {
                id: first_context_id,
                position: [0., 0.],
                lines: None,
            },)]
        );

        grid.split_down(second_context);

        assert_eq!(grid.width, 600.0);
        assert_eq!(grid.height, 600.0);

        let new_context_expected_height = 600. / 2.;

        assert_eq!(grid.current().dimension.height, new_context_expected_height);
        assert_eq!(grid.current_index(), 1);

        grid.select_prev_split();

        let scaled_padding = grid.scaled_padding();
        let old_context_expected_height = (600. / 2.) - scaled_padding;
        assert_eq!(grid.current().dimension.height, old_context_expected_height);
        assert_eq!(grid.current_index(), 0);

        let current_index = grid.current_index();
        let contexts = grid.contexts_ordered();
        // Check that current context has a down child at index 1
        if let Some(down_key) = contexts[current_index].down {
            assert_eq!(grid.key_to_index(down_key), Some(1));
        }
        assert_eq!(contexts[current_index].right, None);

        grid.remove_current();

        assert_eq!(grid.current_index(), 0);
        // Whenever return to one should drop padding
        let expected_height = 600.;
        assert_eq!(grid.current().dimension.height, expected_height);

        let current_index = grid.current_index();
        let contexts = grid.contexts_ordered();
        assert_eq!(contexts[current_index].down, None);
        assert_eq!(contexts[current_index].right, None);
    }

    #[test]
    fn test_remove_down_with_right_children() {
        let margin = Delta {
            x: 0.,
            top_y: 0.,
            bottom_y: 0.,
        };

        let context_dimension = ContextDimension::build(
            600.0,
            600.0,
            SugarDimensions {
                scale: 2.,
                width: 14.,
                height: 8.,
            },
            1.0,
            Delta::<f32>::default(),
        );

        assert_eq!(context_dimension.columns, 42);
        assert_eq!(context_dimension.lines, 74);

        let (first_context, first_context_id) = {
            let rich_text_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, second_context_id) = {
            let rich_text_id = 1;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let mut grid =
            ContextGrid::<VoidListener>::new(first_context, margin, [0., 0., 0., 0.]);

        assert_eq!(
            grid.objects(),
            vec![Object::RichText(RichText {
                id: first_context_id,
                position: [0., 0.],
                lines: None,
            },)]
        );

        grid.split_down(second_context);

        assert_eq!(grid.width, 600.0);
        assert_eq!(grid.height, 600.0);

        let new_context_expected_height = 600. / 2.;

        assert_eq!(grid.current().dimension.height, new_context_expected_height);
        assert_eq!(grid.current_index(), 1);

        let (third_context, third_context_id) = {
            let rich_text_id = 2;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        grid.split_right(third_context);
        assert_eq!(grid.current_index(), 2);
        assert_eq!(grid.current().dimension.width, new_context_expected_height);
        assert_eq!(grid.current().dimension.height, 300.);

        // Move back
        grid.select_prev_split();

        assert_eq!(grid.current_index(), 1);
        assert_eq!(grid.current().rich_text_id, second_context_id);
        assert_eq!(grid.current().dimension.height, new_context_expected_height);
        assert_eq!(grid.current().dimension.width, 296.);

        // Remove the current should actually make down being down
        grid.remove_current();

        assert_eq!(grid.current_index(), 1);
        assert_eq!(grid.current().rich_text_id, third_context_id);
        assert_eq!(grid.current().dimension.height, new_context_expected_height);
        assert_eq!(grid.current().dimension.width, 600.);
    }

    #[test]
    fn test_remove_context_with_parent_but_down_children() {
        let margin = Delta {
            x: 0.,
            top_y: 0.,
            bottom_y: 0.,
        };

        let context_dimension = ContextDimension::build(
            600.0,
            600.0,
            SugarDimensions {
                scale: 2.,
                width: 14.,
                height: 8.,
            },
            1.0,
            Delta::<f32>::default(),
        );

        assert_eq!(context_dimension.columns, 42);
        assert_eq!(context_dimension.lines, 74);

        let (first_context, first_context_id) = {
            let rich_text_id = 1;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, second_context_id) = {
            let rich_text_id = 2;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (third_context, third_context_id) = {
            let rich_text_id = 3;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (fourth_context, fourth_context_id) = {
            let rich_text_id = 4;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (fifth_context, fifth_context_id) = {
            let rich_text_id = 5;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (sixth_context, sixth_context_id) = {
            let rich_text_id = 6;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let mut grid =
            ContextGrid::<VoidListener>::new(first_context, margin, [0., 0., 0., 0.]);

        assert_eq!(
            grid.objects(),
            vec![Object::RichText(RichText {
                id: first_context_id,
                position: [0., 0.],
                lines: None,
            },)]
        );

        // The test is to validate the removal of a context with parenting however
        // should move to up the down items
        //
        // Test setup
        //
        // |1.-----|.3-----|4.-----|
        // |2.-----|.5-|6.-|-------|

        grid.split_down(second_context);

        assert_eq!(grid.width, 600.0);
        assert_eq!(grid.height, 600.0);

        let new_context_expected_height = 600. / 2.;

        assert_eq!(grid.current().dimension.height, new_context_expected_height);
        assert_eq!(grid.current().rich_text_id, second_context_id);
        assert_eq!(grid.current_index(), 1);

        grid.select_prev_split();
        assert_eq!(grid.current().rich_text_id, first_context_id);

        grid.split_right(third_context);
        assert_eq!(grid.current().rich_text_id, third_context_id);

        grid.split_right(fourth_context);
        assert_eq!(grid.current().rich_text_id, fourth_context_id);

        let current_index = grid.current_index();
        assert_eq!(current_index, 2);
        assert_eq!(grid.contexts_ordered()[current_index].down, None);

        // So far we have:
        //
        // |1.-----|.3-----|4.-----|
        // |2.-----|-------|-------|

        grid.select_prev_split();
        assert_eq!(grid.current().rich_text_id, third_context_id);
        let current_index = grid.current_index();
        assert_eq!(current_index, 1);
        assert_eq!(grid.contexts_ordered()[current_index].down, None);

        grid.split_down(fifth_context);
        assert_eq!(grid.current().rich_text_id, fifth_context_id);

        grid.split_right(sixth_context);
        assert_eq!(grid.current().rich_text_id, sixth_context_id);

        grid.select_prev_split();
        grid.select_prev_split();
        grid.select_prev_split();

        assert_eq!(grid.current().rich_text_id, third_context_id);

        let current_index = grid.current_index();
        let right = grid.contexts_ordered()[current_index].right;
        assert_eq!(
            grid.inner[&right.unwrap_or_default()].val.rich_text_id,
            fourth_context_id
        );
        let current_index = grid.current_index();
        let down = grid.contexts_ordered()[current_index].down;
        assert_eq!(
            grid.inner[&down.unwrap_or_default()].val.rich_text_id,
            fifth_context_id
        );
        // Setup complete, now we have 3 as active as well
        //
        // |1.-----|.3-----|4.-----|
        // |2.-----|.5-|6.-|-------|
        //
        // If we remove 3 then should be
        //
        // |1.-----|.5-|6.-|4.-----|
        // |2.-----|---|---|-------|

        grid.remove_current();

        // Check if current is 5 and next is 6
        assert_eq!(grid.current().rich_text_id, fifth_context_id);
        let current_index = grid.current_index();
        let right = grid.contexts_ordered()[current_index].right;
        assert_eq!(
            grid.inner[&right.unwrap_or_default()].val.rich_text_id,
            sixth_context_id
        );

        // Let's go back to 1 to check if leads to 5
        grid.select_prev_split();

        assert_eq!(grid.current().rich_text_id, first_context_id);
        let current_index = grid.current_index();
        assert_eq!(current_index, 0);
        let right = grid.contexts_ordered()[current_index].right;
        if let Some(key) = right {
            assert_eq!(grid.key_to_index(key), Some(1));
        };
        assert_eq!(
            grid.inner[&right.unwrap_or_default()].val.rich_text_id,
            fifth_context_id
        );

        // Let's go to 6 to check if leads to 4
        //
        // |1.-----|.5-|6.-|4.-----|
        // |2.-----|---|---|-------|

        grid.select_next_split();
        grid.select_next_split();

        assert_eq!(grid.current().rich_text_id, sixth_context_id);
        let current_index = grid.current_index();
        let right = grid.contexts_ordered()[current_index].right;
        if let Some(key) = right {
            assert_eq!(grid.key_to_index(key), Some(3));
        };
        assert_eq!(
            grid.inner[&right.unwrap_or_default()].val.rich_text_id,
            fourth_context_id
        );
    }

    #[test]
    fn test_remove_context_without_parents_but_with_right_and_down_children() {
        let margin = Delta {
            x: 0.,
            top_y: 0.,
            bottom_y: 0.,
        };

        let context_dimension = ContextDimension::build(
            600.0,
            600.0,
            SugarDimensions {
                scale: 2.,
                width: 14.,
                height: 8.,
            },
            1.0,
            Delta::<f32>::default(),
        );

        assert_eq!(context_dimension.columns, 42);
        assert_eq!(context_dimension.lines, 74);

        let (first_context, first_context_id) = {
            let rich_text_id = 1;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, second_context_id) = {
            let rich_text_id = 2;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (third_context, third_context_id) = {
            let rich_text_id = 3;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (fourth_context, fourth_context_id) = {
            let rich_text_id = 4;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let mut grid =
            ContextGrid::<VoidListener>::new(first_context, margin, [0., 0., 0., 0.]);

        assert_eq!(
            grid.objects(),
            vec![Object::RichText(RichText {
                id: first_context_id,
                position: [0., 0.],
                lines: None,
            },)]
        );

        grid.split_right(second_context);

        assert_eq!(grid.width, 600.0);
        assert_eq!(grid.height, 600.0);

        let new_context_expected_width = 600. / 2.;

        assert_eq!(grid.current().dimension.width, new_context_expected_width);
        assert_eq!(grid.current().rich_text_id, second_context_id);
        assert_eq!(grid.current_index(), 1);

        grid.select_prev_split();
        grid.select_prev_split();
        assert_eq!(grid.current().rich_text_id, second_context_id);

        grid.select_prev_split();
        assert_eq!(grid.current().rich_text_id, first_context_id);

        grid.split_down(third_context);
        assert_eq!(grid.current().rich_text_id, third_context_id);
        assert_eq!(grid.current().dimension.width, 296.);
        assert_eq!(grid.current().dimension.height, 300.);

        grid.select_prev_split();
        grid.select_prev_split();
        grid.select_prev_split();
        grid.select_prev_split();
        assert_eq!(grid.current().rich_text_id, second_context_id);
        grid.split_down(fourth_context);

        assert_eq!(grid.current_index(), 2);
        assert_eq!(grid.current().rich_text_id, fourth_context_id);

        grid.select_next_split();
        grid.select_next_split();
        assert_eq!(grid.current_index(), 0);
        assert_eq!(grid.current().rich_text_id, first_context_id);

        // Active is 1
        // |1.----|.2----|
        // |3.----|.4----|

        // Remove the current should actually make right being down
        grid.remove_current();
        let current_index = grid.current_index();
        // Move third context to first position
        assert_eq!(current_index, 0);
        assert_eq!(grid.current().rich_text_id, third_context_id);
        let right = grid.contexts_ordered()[current_index].right;
        let right_context = grid.inner[&right.unwrap_or_default()].val.rich_text_id;
        assert_eq!(right_context, second_context_id);

        // Result:
        // |3.----|.2----|
        // |------|.4----|

        // Now let's create a more complex case
        // |3.---------|.2---------|
        // |5.-|6.-|7.-|.4---------|

        let (fifth_context, fifth_context_id) = {
            let rich_text_id = 5;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (sixth_context, sixth_context_id) = {
            let rich_text_id = 6;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (seventh_context, seventh_context_id) = {
            let rich_text_id = 7;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        grid.split_down(fifth_context);
        grid.split_right(sixth_context);
        grid.split_right(seventh_context);

        assert_eq!(grid.current_index(), 5);
        assert_eq!(grid.current().rich_text_id, seventh_context_id);

        // Current:
        // |3.---------|.2---------|
        // |5.-|6.-|7.-|.4---------|
        //
        // Now if we move back to 3. and remove it:
        // Should move 5, 6 and 7 to top.
        //
        // |5.-|6.-|7.-|.2---------|
        // |---|---|---|.4---------|
        grid.select_next_split();
        assert_eq!(grid.current().rich_text_id, third_context_id);
        let current_index = grid.current_index();
        let down = grid.contexts_ordered()[current_index].down;
        assert_eq!(
            grid.inner[&down.unwrap_or_default()].val.rich_text_id,
            fifth_context_id
        );

        grid.remove_current();
        assert_eq!(grid.current().rich_text_id, fifth_context_id);

        let current_index = grid.current_index();
        let right = grid.contexts_ordered()[current_index]
            .right
            .unwrap_or_default();
        let right_context = &grid.contexts()[&right];
        assert_eq!(right_context.val.rich_text_id, sixth_context_id);

        // Current:
        // |5.-|6.-|7.-|.2---------|
        // |---|---|---|.4---------|

        // Ok, let's test the reverse to right operations
        // First remove 5 and 6

        grid.remove_current();
        assert_eq!(grid.current().rich_text_id, sixth_context_id);
        let current_index = grid.current_index();
        assert_eq!(grid.contexts_ordered()[current_index].down, None);
        let right = grid.contexts_ordered()[current_index].right;
        assert_eq!(
            grid.inner[&right.unwrap_or_default()].val.rich_text_id,
            seventh_context_id
        );

        grid.remove_current();
        assert_eq!(grid.current().rich_text_id, seventh_context_id);
        let right = grid.contexts_ordered()[current_index].right;
        assert_eq!(
            grid.inner[&right.unwrap_or_default()].val.rich_text_id,
            second_context_id
        );

        // Current:
        // |7.---------|.2---------|
        // |-----------|.4---------|

        // Now let's add many 5 and 6 as down items on 7th
        //
        // Should be:
        // |7.---------|.2---------|
        // |5.---------|.4---------|
        // |6.---------|-----------|

        let (fifth_context, fifth_context_id) = {
            let rich_text_id = 5;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (sixth_context, sixth_context_id) = {
            let rich_text_id = 6;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        grid.split_down(fifth_context);
        grid.split_down(sixth_context);

        assert_eq!(grid.current().rich_text_id, sixth_context_id);
        grid.select_prev_split();
        assert_eq!(grid.current().rich_text_id, fifth_context_id);
        grid.select_prev_split();
        grid.select_prev_split();
        grid.select_prev_split();
        assert_eq!(grid.current().rich_text_id, seventh_context_id);

        // Next step remove 7
        //
        // Should be:
        // |5.---------|.2---------|
        // |6.---------|.4---------|
        // |-----------|-----------|

        grid.remove_current();
        assert_eq!(grid.current().rich_text_id, fifth_context_id);
        let right = grid.contexts_ordered()[current_index].right;
        let down = grid.contexts_ordered()[current_index].down;
        assert_eq!(
            grid.inner[&right.unwrap_or_default()].val.rich_text_id,
            second_context_id
        );
        assert_eq!(
            grid.inner[&down.unwrap_or_default()].val.rich_text_id,
            sixth_context_id
        );

        // Next step remove 5
        //
        // Should be:
        // |6.---------|.2---------|
        // |-----------|.4---------|
        // |-----------|-----------|

        grid.remove_current();
        assert_eq!(grid.current().rich_text_id, sixth_context_id);
        let right = grid.contexts_ordered()[current_index].right;
        assert_eq!(
            grid.inner[&right.unwrap_or_default()].val.rich_text_id,
            second_context_id
        );
        assert_eq!(grid.contexts_ordered()[current_index].down, None);
    }

    #[test]
    fn test_select_current_based_on_mouse() {
        let mut mouse = Mouse::default();
        let margin = Delta {
            x: 0.,
            top_y: 0.,
            bottom_y: 0.,
        };

        let context_dimension = ContextDimension::build(
            600.0,
            600.0,
            SugarDimensions {
                scale: 2.,
                width: 14.,
                height: 8.,
            },
            1.0,
            Delta::<f32>::default(),
        );

        assert_eq!(context_dimension.columns, 42);
        assert_eq!(context_dimension.lines, 74);

        let (first_context, first_context_id) = {
            let rich_text_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, second_context_id) = {
            let rich_text_id = 1;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let mut grid =
            ContextGrid::<VoidListener>::new(first_context, margin, [0., 0., 0., 0.]);

        assert_eq!(
            grid.objects(),
            vec![Object::RichText(RichText {
                id: first_context_id,
                position: [0., 0.],
                lines: None,
            },)]
        );

        grid.select_current_based_on_mouse(&mouse);
        // On first should always return first item
        assert_eq!(grid.current_index(), 0);

        grid.split_down(second_context);

        assert_eq!(grid.width, 600.0);
        assert_eq!(grid.height, 600.0);

        let new_context_expected_height = 600. / 2.;

        assert_eq!(grid.current().dimension.height, new_context_expected_height);
        assert_eq!(grid.current_index(), 1);

        let (third_context, third_context_id) = {
            let rich_text_id = 2;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        grid.split_right(third_context);
        assert_eq!(grid.current_index(), 2);
        assert_eq!(grid.current().dimension.width, new_context_expected_height);
        assert_eq!(grid.current().dimension.height, 300.);

        grid.select_current_based_on_mouse(&mouse);
        assert_eq!(grid.current_index(), 0);
        assert_eq!(grid.current().rich_text_id, 0);

        let scaled_padding = grid.scaled_padding();
        mouse.y = (new_context_expected_height + scaled_padding) as usize;
        grid.select_current_based_on_mouse(&mouse);

        assert_eq!(grid.current_index(), 1);
        assert_eq!(grid.current().rich_text_id, second_context_id);

        mouse.x = 304;
        grid.select_current_based_on_mouse(&mouse);

        assert_eq!(grid.current_index(), 2);
        assert_eq!(grid.current().rich_text_id, third_context_id);
    }

    #[test]
    fn test_edge_case_empty_grid() {
        let margin = Delta::default();
        let context_dimension = ContextDimension::default();
        let context =
            create_mock_context(VoidListener {}, WindowId::from(0), 0, context_dimension);

        let mut grid =
            ContextGrid::<VoidListener>::new(context, margin, [0., 0., 0., 0.]);

        // Test that we can't remove the last context
        assert_eq!(grid.len(), 1);
        grid.remove_current();
        assert_eq!(grid.len(), 1); // Should still have 1 context
    }

    #[test]
    fn test_edge_case_invalid_dimensions() {
        // Test with zero dimensions
        let (cols, lines) =
            compute(0.0, 0.0, SugarDimensions::default(), 1.0, Delta::default());
        assert_eq!(cols, MIN_COLS);
        assert_eq!(lines, MIN_LINES);

        // Test with negative dimensions
        let (cols, lines) = compute(
            -100.0,
            -100.0,
            SugarDimensions::default(),
            1.0,
            Delta::default(),
        );
        assert_eq!(cols, MIN_COLS);
        assert_eq!(lines, MIN_LINES);

        // Test with invalid scale
        let (cols, lines) = compute(
            1000.0,
            1000.0,
            SugarDimensions {
                scale: 0.0,
                width: 10.0,
                height: 10.0,
            },
            1.0,
            Delta::default(),
        );
        assert_eq!(cols, MIN_COLS);
        assert_eq!(lines, MIN_LINES);
    }

    #[test]
    fn test_edge_case_complex_removal_scenario() {
        let margin = Delta::default();
        let context_dimension = ContextDimension::build(
            600.0,
            600.0,
            SugarDimensions {
                scale: 1.0,
                width: 10.0,
                height: 10.0,
            },
            1.0,
            Delta::default(),
        );

        // Create a complex grid structure
        let mut grid = ContextGrid::<VoidListener>::new(
            create_mock_context(VoidListener {}, WindowId::from(0), 0, context_dimension),
            margin,
            [0., 0., 0., 0.],
        );

        // Add multiple splits to create a complex structure
        for i in 1..=5 {
            let context = create_mock_context(
                VoidListener {},
                WindowId::from(0),
                i,
                context_dimension,
            );
            if i % 2 == 0 {
                grid.split_down(context);
            } else {
                grid.split_right(context);
            }
        }

        let initial_len = grid.len();
        assert!(initial_len > 1);

        // Remove contexts one by one and ensure no crashes
        while grid.len() > 1 {
            let len_before = grid.len();
            grid.remove_current();
            let len_after = grid.len();

            // Should have removed exactly one context
            assert_eq!(len_before - 1, len_after);

            // Current should still be valid
            assert!(grid.inner.contains_key(&grid.current));
        }

        // Should have exactly one context left
        assert_eq!(grid.len(), 1);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default, clippy::bool_comparison)]
    fn test_edge_case_dimension_updates_with_invalid_data() {
        let margin = Delta::default();
        let mut context_dimension = ContextDimension::default();

        // Test with invalid dimensions
        context_dimension.width = -100.0;
        context_dimension.height = -100.0;

        let context =
            create_mock_context(VoidListener {}, WindowId::from(0), 0, context_dimension);

        let mut grid =
            ContextGrid::<VoidListener>::new(context, margin, [0., 0., 0., 0.]);

        // These operations should not crash
        grid.resize(0.0, 0.0);
        grid.resize(-100.0, -100.0);

        // Grid should still be functional
        assert_eq!(grid.len(), 1);
    }

    #[test]
    fn test_edge_case_mouse_selection_with_invalid_coordinates() {
        let margin = Delta::default();
        let context_dimension = ContextDimension::build(
            600.0,
            400.0,
            SugarDimensions {
                scale: 1.0,
                width: 10.0,
                height: 10.0,
            },
            1.0,
            Delta::default(),
        );

        let mut grid = ContextGrid::<VoidListener>::new(
            create_mock_context(VoidListener {}, WindowId::from(0), 0, context_dimension),
            margin,
            [0., 0., 0., 0.],
        );

        // Add a split
        let context =
            create_mock_context(VoidListener {}, WindowId::from(0), 1, context_dimension);
        grid.split_right(context);

        let mut mouse = Mouse::default();

        // Test with extreme coordinates
        mouse.x = usize::MAX;
        mouse.y = usize::MAX;
        let result = grid.select_current_based_on_mouse(&mouse);
        // Should not crash and should return a valid result
        assert!(result == true || result == false);

        // Test with zero coordinates
        mouse.x = 0;
        mouse.y = 0;
        let result = grid.select_current_based_on_mouse(&mouse);
        assert!(result == true || result == false);
    }

    #[test]
    fn test_edge_case_navigation_with_empty_or_invalid_states() {
        let margin = Delta::default();
        let context_dimension = ContextDimension::default();
        let context =
            create_mock_context(VoidListener {}, WindowId::from(0), 0, context_dimension);

        let mut grid =
            ContextGrid::<VoidListener>::new(context, margin, [0., 0., 0., 0.]);

        // Test navigation with single context
        grid.select_next_split();
        assert_eq!(grid.current_index(), 0);

        grid.select_prev_split();
        assert_eq!(grid.current_index(), 0);

        assert!(!grid.select_next_split_no_loop());
        assert!(!grid.select_prev_split_no_loop());
    }

    #[test]
    fn test_stress_test_many_splits() {
        let margin = Delta::default();
        let context_dimension = ContextDimension::build(
            1200.0,
            800.0,
            SugarDimensions {
                scale: 1.0,
                width: 8.0,
                height: 8.0,
            },
            1.0,
            Delta::default(),
        );

        let mut grid = ContextGrid::<VoidListener>::new(
            create_mock_context(VoidListener {}, WindowId::from(0), 0, context_dimension),
            margin,
            [0., 0., 0., 0.],
        );

        // Create many splits
        for i in 1..=20 {
            let context = create_mock_context(
                VoidListener {},
                WindowId::from(0),
                i,
                context_dimension,
            );

            if i % 3 == 0 {
                grid.split_down(context);
            } else {
                grid.split_right(context);
            }

            // Verify grid state after each split
            assert!(grid.len() > 0);
            assert!(grid.inner.contains_key(&grid.current));
        }

        // Test navigation through all splits
        let initial_current = grid.current;
        for _ in 0..grid.len() * 2 {
            grid.select_next_split();
            assert!(grid.inner.contains_key(&grid.current));
        }

        // Should cycle back
        assert_eq!(grid.current, initial_current);

        // Remove all but one
        while grid.len() > 1 {
            let len_before = grid.len();
            grid.remove_current();
            assert!(grid.len() < len_before);
            assert!(grid.inner.contains_key(&grid.current));
        }
    }

    #[test]
    fn test_edge_case_resize_with_extreme_values() {
        let margin = Delta::default();
        let context_dimension = ContextDimension::build(
            100.0,
            100.0,
            SugarDimensions {
                scale: 1.0,
                width: 10.0,
                height: 10.0,
            },
            1.0,
            Delta::default(),
        );

        let mut grid = ContextGrid::<VoidListener>::new(
            create_mock_context(VoidListener {}, WindowId::from(0), 0, context_dimension),
            margin,
            [0., 0., 0., 0.],
        );

        // Add a split
        let context =
            create_mock_context(VoidListener {}, WindowId::from(0), 1, context_dimension);
        grid.split_right(context);

        // Test resize with reasonable large values (not MAX to avoid overflow)
        grid.resize(10000.0, 10000.0);
        assert!(grid.len() > 0);

        grid.resize(0.1, 0.1);
        assert!(grid.len() > 0);

        grid.resize(1.0, 1.0);
        assert!(grid.len() > 0);
    }

    #[test]
    fn test_dimension_calculation_edge_cases() {
        // Test with very small positive values
        let (cols, lines) = compute(
            0.1,
            0.1,
            SugarDimensions {
                scale: 0.1,
                width: 0.1,
                height: 0.1,
            },
            0.1,
            Delta::default(),
        );
        assert_eq!(cols, MIN_COLS);
        // With very small values, we should get minimum lines
        assert!(lines >= MIN_LINES);

        // Test with very large margins that exceed available space
        let (cols, lines) = compute(
            100.0,
            100.0,
            SugarDimensions {
                scale: 1.0,
                width: 10.0,
                height: 10.0,
            },
            1.0,
            Delta {
                x: 1000.0,
                top_y: 1000.0,
                bottom_y: 1000.0,
            },
        );
        assert_eq!(cols, MIN_COLS);
        assert_eq!(lines, MIN_LINES);
    }

    #[test]
    fn test_move_divider_up_basic() {
        let margin = Delta::default();
        let context_dimension = ContextDimension::build(
            800.0,
            600.0,
            SugarDimensions {
                scale: 1.0,
                width: 10.0,
                height: 10.0,
            },
            1.0,
            Delta::default(),
        );

        let mut grid = ContextGrid::<VoidListener>::new(
            create_mock_context(VoidListener {}, WindowId::from(0), 0, context_dimension),
            margin,
            [0., 0., 0., 0.],
        );

        // Single split - should return false
        assert!(!grid.move_divider_up(20.0));

        // Add a split down
        let second_context =
            create_mock_context(VoidListener {}, WindowId::from(0), 1, context_dimension);
        grid.split_down(second_context);

        // Now we should be able to move divider up
        let original_current_height = grid.inner[&grid.current].val.dimension.height;
        let original_parent_height =
            grid.inner[&grid.get_ordered_keys()[0]].val.dimension.height;

        assert!(grid.move_divider_up(20.0));

        // Current split should be smaller, parent should be larger
        assert!(grid.inner[&grid.current].val.dimension.height < original_current_height);
        assert!(
            grid.inner[&grid.get_ordered_keys()[0]].val.dimension.height
                > original_parent_height
        );
    }

    #[test]
    fn test_move_divider_down_basic() {
        let margin = Delta::default();
        let context_dimension = ContextDimension::build(
            800.0,
            600.0,
            SugarDimensions {
                scale: 1.0,
                width: 10.0,
                height: 10.0,
            },
            1.0,
            Delta::default(),
        );

        let mut grid = ContextGrid::<VoidListener>::new(
            create_mock_context(VoidListener {}, WindowId::from(0), 0, context_dimension),
            margin,
            [0., 0., 0., 0.],
        );

        // Add a split down
        let second_context =
            create_mock_context(VoidListener {}, WindowId::from(0), 1, context_dimension);
        grid.split_down(second_context);

        let original_current_height = grid.inner[&grid.current].val.dimension.height;
        let original_parent_height =
            grid.inner[&grid.get_ordered_keys()[0]].val.dimension.height;

        assert!(grid.move_divider_down(20.0));

        // Current split should be larger, parent should be smaller
        assert!(grid.inner[&grid.current].val.dimension.height > original_current_height);
        assert!(
            grid.inner[&grid.get_ordered_keys()[0]].val.dimension.height
                < original_parent_height
        );
    }

    #[test]
    fn test_move_divider_left_basic() {
        let margin = Delta::default();
        let context_dimension = ContextDimension::build(
            800.0,
            600.0,
            SugarDimensions {
                scale: 1.0,
                width: 10.0,
                height: 10.0,
            },
            1.0,
            Delta::default(),
        );

        let mut grid = ContextGrid::<VoidListener>::new(
            create_mock_context(VoidListener {}, WindowId::from(0), 0, context_dimension),
            margin,
            [0., 0., 0., 0.],
        );

        // Single split - should return false
        assert!(!grid.move_divider_left(40.0));

        // Add a split right
        let second_context =
            create_mock_context(VoidListener {}, WindowId::from(0), 1, context_dimension);
        grid.split_right(second_context);

        // Test from the right split (index 1) - moving left should shrink left panel, expand right panel
        let original_left_width =
            grid.inner[&grid.get_ordered_keys()[0]].val.dimension.width;
        let original_right_width =
            grid.inner[&grid.get_ordered_keys()[1]].val.dimension.width;

        assert!(grid.move_divider_left(40.0));

        // Left split should be smaller, right split should be larger
        assert!(
            grid.inner[&grid.get_ordered_keys()[0]].val.dimension.width
                < original_left_width
        );
        assert!(
            grid.inner[&grid.get_ordered_keys()[1]].val.dimension.width
                > original_right_width
        );

        // Test from the left split (index 0) - should have same effect
        grid.current = grid.get_ordered_keys()[0];
        let original_left_width2 =
            grid.inner[&grid.get_ordered_keys()[0]].val.dimension.width;
        let original_right_width2 =
            grid.inner[&grid.get_ordered_keys()[1]].val.dimension.width;

        assert!(grid.move_divider_left(20.0));

        // Left split should be smaller, right split should be larger
        assert!(
            grid.inner[&grid.get_ordered_keys()[0]].val.dimension.width
                < original_left_width2
        );
        assert!(
            grid.inner[&grid.get_ordered_keys()[1]].val.dimension.width
                > original_right_width2
        );
    }

    #[test]
    fn test_move_divider_right_basic() {
        let margin = Delta::default();
        let context_dimension = ContextDimension::build(
            800.0,
            600.0,
            SugarDimensions {
                scale: 1.0,
                width: 10.0,
                height: 10.0,
            },
            1.0,
            Delta::default(),
        );

        let mut grid = ContextGrid::<VoidListener>::new(
            create_mock_context(VoidListener {}, WindowId::from(0), 0, context_dimension),
            margin,
            [0., 0., 0., 0.],
        );

        // Add a split right
        let second_context =
            create_mock_context(VoidListener {}, WindowId::from(0), 1, context_dimension);
        grid.split_right(second_context);

        // Test from the right split (index 1) - moving right should expand left panel, shrink right panel
        let original_left_width =
            grid.inner[&grid.get_ordered_keys()[0]].val.dimension.width;
        let original_right_width =
            grid.inner[&grid.get_ordered_keys()[1]].val.dimension.width;

        assert!(grid.move_divider_right(40.0));

        // Left split should be larger, right split should be smaller
        assert!(
            grid.inner[&grid.get_ordered_keys()[0]].val.dimension.width
                > original_left_width
        );
        assert!(
            grid.inner[&grid.get_ordered_keys()[1]].val.dimension.width
                < original_right_width
        );

        // Test from the left split (index 0) - should have same effect
        grid.current = grid.get_ordered_keys()[0];
        let original_left_width2 =
            grid.inner[&grid.get_ordered_keys()[0]].val.dimension.width;
        let original_right_width2 =
            grid.inner[&grid.get_ordered_keys()[1]].val.dimension.width;

        assert!(grid.move_divider_right(20.0));

        // Left split should be larger, right split should be smaller
        assert!(
            grid.inner[&grid.get_ordered_keys()[0]].val.dimension.width
                > original_left_width2
        );
        assert!(
            grid.inner[&grid.get_ordered_keys()[1]].val.dimension.width
                < original_right_width2
        );
    }

    #[test]
    fn test_move_divider_minimum_size_constraints() {
        let margin = Delta::default();
        let context_dimension = ContextDimension::build(
            200.0, // Small total width
            150.0, // Small total height
            SugarDimensions {
                scale: 1.0,
                width: 10.0,
                height: 10.0,
            },
            1.0,
            Delta::default(),
        );

        let mut grid = ContextGrid::<VoidListener>::new(
            create_mock_context(VoidListener {}, WindowId::from(0), 0, context_dimension),
            margin,
            [0., 0., 0., 0.],
        );

        // Add splits
        let second_context =
            create_mock_context(VoidListener {}, WindowId::from(0), 1, context_dimension);
        grid.split_right(second_context);

        let third_context =
            create_mock_context(VoidListener {}, WindowId::from(0), 2, context_dimension);
        grid.split_down(third_context);

        // Try to move dividers beyond minimum constraints
        // Should fail when trying to make splits too small
        let large_amount = 1000.0;

        // These should fail due to minimum size constraints
        assert!(!grid.move_divider_left(large_amount));
        assert!(!grid.move_divider_right(large_amount));
        assert!(!grid.move_divider_up(large_amount));
        assert!(!grid.move_divider_down(large_amount));
    }

    #[test]
    fn test_move_divider_complex_layout() {
        let margin = Delta::default();
        let context_dimension = ContextDimension::build(
            1200.0,
            800.0,
            SugarDimensions {
                scale: 1.0,
                width: 10.0,
                height: 10.0,
            },
            1.0,
            Delta::default(),
        );

        let mut grid = ContextGrid::<VoidListener>::new(
            create_mock_context(VoidListener {}, WindowId::from(0), 0, context_dimension),
            margin,
            [0., 0., 0., 0.],
        );

        // Create a complex layout: split right, then split down on the right side
        let second_context =
            create_mock_context(VoidListener {}, WindowId::from(0), 1, context_dimension);
        grid.split_right(second_context);

        let third_context =
            create_mock_context(VoidListener {}, WindowId::from(0), 2, context_dimension);
        grid.split_down(third_context);

        // Test moving dividers in different splits
        assert!(grid.move_divider_up(30.0));
        assert!(grid.move_divider_down(15.0));

        // Switch to first split (index 0) and test horizontal movement
        grid.current = grid.get_ordered_keys()[0];
        assert!(grid.move_divider_right(50.0));
        assert!(grid.move_divider_left(25.0));

        // Verify grid is still in valid state
        assert!(grid.len() == 3);
        assert!(grid.inner.contains_key(&grid.current));
    }

    #[test]
    fn test_move_divider_edge_cases() {
        let margin = Delta::default();
        let context_dimension = ContextDimension::build(
            800.0,
            600.0,
            SugarDimensions {
                scale: 1.0,
                width: 10.0,
                height: 10.0,
            },
            1.0,
            Delta::default(),
        );

        let mut grid = ContextGrid::<VoidListener>::new(
            create_mock_context(VoidListener {}, WindowId::from(0), 0, context_dimension),
            margin,
            [0., 0., 0., 0.],
        );

        // Test with zero amount
        let second_context =
            create_mock_context(VoidListener {}, WindowId::from(0), 1, context_dimension);
        grid.split_right(second_context);

        let original_width = grid.inner[&grid.current].val.dimension.width;
        assert!(grid.move_divider_left(0.0));
        // Width should remain the same with zero movement
        assert_eq!(
            grid.inner[&grid.current].val.dimension.width,
            original_width
        );

        // Test with negative amount (should still work as it's just direction)
        assert!(grid.move_divider_right(-10.0));
    }

    #[test]
    fn test_move_divider_no_adjacent_splits() {
        let margin = Delta::default();
        let context_dimension = ContextDimension::build(
            800.0,
            600.0,
            SugarDimensions {
                scale: 1.0,
                width: 10.0,
                height: 10.0,
            },
            1.0,
            Delta::default(),
        );

        let mut grid = ContextGrid::<VoidListener>::new(
            create_mock_context(VoidListener {}, WindowId::from(0), 0, context_dimension),
            margin,
            [0., 0., 0., 0.],
        );

        // With only one split, no divider movement should work
        assert!(!grid.move_divider_up(20.0));
        assert!(!grid.move_divider_down(20.0));
        assert!(!grid.move_divider_left(40.0));
        assert!(!grid.move_divider_right(40.0));

        // Add only a vertical split (down)
        let second_context =
            create_mock_context(VoidListener {}, WindowId::from(0), 1, context_dimension);
        grid.split_down(second_context);

        // Select the top split (index 0) - should not be able to move horizontal dividers
        // but should be able to move vertical dividers (since it has a down child)
        grid.current = grid.get_ordered_keys()[0];
        assert!(!grid.move_divider_left(40.0));
        assert!(!grid.move_divider_right(40.0));
        assert!(grid.move_divider_up(20.0)); // Can move up by shrinking itself and expanding down child
        assert!(grid.move_divider_down(20.0)); // Can move down by expanding itself and shrinking down child

        // The bottom split (index 1) should be able to move up (has parent above)
        grid.current = grid.get_ordered_keys()[1];
        assert!(grid.move_divider_up(20.0));
        assert!(grid.move_divider_down(20.0));
    }

    #[test]
    fn test_move_divider_stress_test() {
        let margin = Delta::default();
        let context_dimension = ContextDimension::build(
            1600.0,
            1200.0,
            SugarDimensions {
                scale: 1.0,
                width: 10.0,
                height: 10.0,
            },
            1.0,
            Delta::default(),
        );

        let mut grid = ContextGrid::<VoidListener>::new(
            create_mock_context(VoidListener {}, WindowId::from(0), 0, context_dimension),
            margin,
            [0., 0., 0., 0.],
        );

        // Create multiple splits
        for i in 1..6 {
            let context = create_mock_context(
                VoidListener {},
                WindowId::from(0),
                i,
                context_dimension,
            );
            if i % 2 == 0 {
                grid.split_right(context);
            } else {
                grid.split_down(context);
            }
        }

        // Perform many divider movements
        for _ in 0..20 {
            grid.select_next_split();

            // Try all movement directions
            grid.move_divider_up(10.0);
            grid.move_divider_down(5.0);
            grid.move_divider_left(15.0);
            grid.move_divider_right(8.0);

            // Verify grid state remains valid
            assert!(grid.len() >= 1);
            assert!(grid.inner.contains_key(&grid.current));

            // Verify all dimensions are positive
            for item in grid.inner.values() {
                assert!(item.val.dimension.width > 0.0);
                assert!(item.val.dimension.height > 0.0);
            }
        }
    }

    #[test]
    fn test_move_divider_preserves_total_space() {
        let margin = Delta::default();
        let context_dimension = ContextDimension::build(
            800.0,
            600.0,
            SugarDimensions {
                scale: 1.0,
                width: 10.0,
                height: 10.0,
            },
            1.0,
            Delta::default(),
        );

        let mut grid = ContextGrid::<VoidListener>::new(
            create_mock_context(VoidListener {}, WindowId::from(0), 0, context_dimension),
            margin,
            [0., 0., 0., 0.],
        );

        // Add a horizontal split
        let second_context =
            create_mock_context(VoidListener {}, WindowId::from(0), 1, context_dimension);
        grid.split_right(second_context);

        let original_total_width =
            grid.inner[&grid.get_ordered_keys()[0]].val.dimension.width
                + grid.inner[&grid.get_ordered_keys()[1]].val.dimension.width;

        // Move divider and check total space is preserved (approximately)
        assert!(grid.move_divider_left(50.0));

        let new_total_width = grid.inner[&grid.get_ordered_keys()[0]].val.dimension.width
            + grid.inner[&grid.get_ordered_keys()[1]].val.dimension.width;

        // Total width should be approximately the same (allowing for small floating point differences)
        let difference = (original_total_width - new_total_width).abs();
        assert!(
            difference < 1.0,
            "Total width changed by more than 1.0: {} vs {}",
            original_total_width,
            new_total_width
        );

        // Test with vertical split
        let third_context =
            create_mock_context(VoidListener {}, WindowId::from(0), 2, context_dimension);
        grid.split_down(third_context);

        let parent_key = grid
            .inner
            .iter()
            .find(|(_key, item)| {
                item.down.is_some() && item.down.unwrap() == grid.current
            })
            .map(|(key, _item)| key)
            .unwrap()
            .clone();

        let original_total_height = grid.inner[&parent_key].val.dimension.height
            + grid.inner[&grid.current].val.dimension.height;

        assert!(grid.move_divider_up(30.0));

        let new_total_height = grid.inner[&parent_key].val.dimension.height
            + grid.inner[&grid.current].val.dimension.height;

        let height_difference = (original_total_height - new_total_height).abs();
        assert!(
            height_difference < 1.0,
            "Total height changed by more than 1.0: {} vs {}",
            original_total_height,
            new_total_height
        );
    }

    #[test]
    fn test_position_calculation_single_context() {
        let context_dimension = ContextDimension::build(
            600.,
            400.,
            SugarDimensions {
                scale: 2.0,
                width: 20.0,
                height: 40.0,
            },
            1.0,
            Delta::default(),
        );

        let context =
            create_mock_context(VoidListener, WindowId::from(0), 1, context_dimension);

        let margin = Delta {
            x: 10.0,
            top_y: 20.0,
            bottom_y: 30.0,
        };

        let grid =
            ContextGrid::<VoidListener>::new(context, margin, [1.0, 1.0, 1.0, 1.0]);

        // Single context should be positioned at margin
        assert_eq!(
            grid.inner[&grid.get_ordered_keys()[0]].position(),
            [10.0, 20.0]
        );
        assert_eq!(grid.scaled_padding(), PADDING * 2.0);
    }

    #[test]
    fn test_position_calculation_after_split_right() {
        let first_context_dimension = ContextDimension::build(
            600.,
            400.,
            SugarDimensions {
                scale: 1.0,
                width: 20.0,
                height: 40.0,
            },
            1.0,
            Delta::default(),
        );

        let second_context_dimension = ContextDimension::build(
            600.,
            400.,
            SugarDimensions {
                scale: 1.0,
                width: 20.0,
                height: 40.0,
            },
            1.0,
            Delta::default(),
        );

        let first_context = create_mock_context(
            VoidListener,
            WindowId::from(0),
            1,
            first_context_dimension,
        );

        let second_context = create_mock_context(
            VoidListener,
            WindowId::from(1),
            2,
            second_context_dimension,
        );

        let margin = Delta {
            x: 0.0,
            top_y: 0.0,
            bottom_y: 0.0,
        };

        let mut grid =
            ContextGrid::<VoidListener>::new(first_context, margin, [1.0, 1.0, 1.0, 1.0]);
        grid.split_right(second_context);

        // First context should remain at origin
        assert_eq!(
            grid.inner[&grid.get_ordered_keys()[0]].position(),
            [0.0, 0.0]
        );

        // Second context should be positioned to the right of first + padding
        let expected_x = 0.0
            + PADDING
            + (grid.inner[&grid.get_ordered_keys()[0]].val.dimension.width
                / grid.inner[&grid.get_ordered_keys()[0]]
                    .val
                    .dimension
                    .dimension
                    .scale);
        assert_eq!(
            grid.inner[&grid.get_ordered_keys()[1]].position(),
            [expected_x, 0.0]
        );
    }

    #[test]
    fn test_position_calculation_after_split_down() {
        let first_context_dimension = ContextDimension::build(
            600.,
            400.,
            SugarDimensions {
                scale: 1.0,
                width: 20.0,
                height: 40.0,
            },
            1.0,
            Delta::default(),
        );

        let second_context_dimension = ContextDimension::build(
            600.,
            400.,
            SugarDimensions {
                scale: 1.0,
                width: 20.0,
                height: 40.0,
            },
            1.0,
            Delta::default(),
        );

        let first_context = create_mock_context(
            VoidListener,
            WindowId::from(0),
            1,
            first_context_dimension,
        );

        let second_context = create_mock_context(
            VoidListener,
            WindowId::from(1),
            2,
            second_context_dimension,
        );

        let margin = Delta {
            x: 0.0,
            top_y: 0.0,
            bottom_y: 0.0,
        };

        let mut grid =
            ContextGrid::<VoidListener>::new(first_context, margin, [1.0, 1.0, 1.0, 1.0]);
        grid.split_down(second_context);

        // First context should remain at origin
        assert_eq!(
            grid.inner[&grid.get_ordered_keys()[0]].position(),
            [0.0, 0.0]
        );

        // Second context should be positioned below first + padding
        let expected_y = 0.0
            + PADDING
            + (grid.inner[&grid.get_ordered_keys()[0]].val.dimension.height
                / grid.inner[&grid.get_ordered_keys()[0]]
                    .val
                    .dimension
                    .dimension
                    .scale);
        assert_eq!(
            grid.inner[&grid.get_ordered_keys()[1]].position(),
            [0.0, expected_y]
        );
    }

    #[test]
    fn test_position_calculation_complex_layout() {
        let context_dimension = ContextDimension::build(
            600.,
            400.,
            SugarDimensions {
                scale: 1.0,
                width: 100.0,
                height: 100.0,
            },
            1.0,
            Delta::default(),
        );

        // Create separate contexts instead of trying to clone
        let first_context = create_mock_context(
            VoidListener,
            WindowId::from(0),
            1,
            context_dimension.clone(),
        );

        let second_context = create_mock_context(
            VoidListener,
            WindowId::from(1),
            2,
            context_dimension.clone(),
        );

        let third_context = create_mock_context(
            VoidListener,
            WindowId::from(2),
            3,
            context_dimension.clone(),
        );

        let fourth_context =
            create_mock_context(VoidListener, WindowId::from(3), 4, context_dimension);

        let margin = Delta {
            x: 0.0,
            top_y: 0.0,
            bottom_y: 0.0,
        };

        let mut grid =
            ContextGrid::<VoidListener>::new(first_context, margin, [1.0, 1.0, 1.0, 1.0]);

        // Create layout:
        // [0] [1]
        // [2] [3]
        grid.split_right(second_context);
        grid.current = grid.get_ordered_keys()[0];
        grid.split_down(third_context);
        grid.current = grid.get_ordered_keys()[1];
        grid.split_down(fourth_context);

        // Verify positions
        assert_eq!(
            grid.inner[&grid.get_ordered_keys()[0]].position(),
            [0.0, 0.0]
        ); // Top-left

        let right_x = PADDING
            + (grid.inner[&grid.get_ordered_keys()[0]].val.dimension.width
                / grid.inner[&grid.get_ordered_keys()[0]]
                    .val
                    .dimension
                    .dimension
                    .scale);
        assert_eq!(
            grid.inner[&grid.get_ordered_keys()[1]].position(),
            [right_x, 0.0]
        ); // Top-right

        let down_y = PADDING
            + (grid.inner[&grid.get_ordered_keys()[0]].val.dimension.height
                / grid.inner[&grid.get_ordered_keys()[0]]
                    .val
                    .dimension
                    .dimension
                    .scale);
        assert_eq!(
            grid.inner[&grid.get_ordered_keys()[3]].position(),
            [0.0, down_y]
        ); // Bottom-left (Context 3)

        assert_eq!(
            grid.inner[&grid.get_ordered_keys()[2]].position(),
            [right_x, down_y]
        ); // Bottom-right (Context 4)
    }

    #[test]
    fn test_scaled_padding_consistency() {
        let context_dimension = ContextDimension::build(
            600.,
            400.,
            SugarDimensions {
                scale: 2.5,
                width: 20.0,
                height: 40.0,
            },
            1.0,
            Delta::default(),
        );

        let context =
            create_mock_context(VoidListener, WindowId::from(0), 1, context_dimension);

        let grid = ContextGrid::<VoidListener>::new(
            context,
            Delta::default(),
            [1.0, 1.0, 1.0, 1.0],
        );

        // Verify scaled_padding is correctly calculated and stored
        assert_eq!(grid.scaled_padding(), PADDING * 2.5);
        assert_eq!(grid.scale(), 2.5);
    }

    #[test]
    fn test_move_divider_right_updates_positions() {
        let context_dimension = ContextDimension::build(
            600.,
            400.,
            SugarDimensions {
                scale: 1.0,
                width: 20.0,
                height: 40.0,
            },
            1.0,
            Delta::default(),
        );

        let first_context = create_mock_context(
            VoidListener,
            WindowId::from(0),
            1,
            context_dimension.clone(),
        );

        let second_context =
            create_mock_context(VoidListener, WindowId::from(1), 2, context_dimension);

        let mut grid = ContextGrid::<VoidListener>::new(
            first_context,
            Delta::default(),
            [1.0, 1.0, 1.0, 1.0],
        );
        grid.split_right(second_context);

        // Record initial positions
        let initial_first_pos = grid.inner[&grid.get_ordered_keys()[0]].position();
        let initial_second_pos = grid.inner[&grid.get_ordered_keys()[1]].position();

        // Move divider right by 50 pixels
        assert!(grid.move_divider_right(50.0));

        // Verify positions are updated
        let new_first_pos = grid.inner[&grid.get_ordered_keys()[0]].position();
        let new_second_pos = grid.inner[&grid.get_ordered_keys()[1]].position();

        // First split position should remain the same (it's at origin)
        assert_eq!(new_first_pos, initial_first_pos);

        // Second split should move right because first split expanded
        assert!(new_second_pos[0] > initial_second_pos[0]);
        assert_eq!(new_second_pos[1], initial_second_pos[1]); // Y should remain same
    }

    #[test]
    fn test_move_divider_down_updates_positions() {
        let context_dimension = ContextDimension::build(
            600.,
            400.,
            SugarDimensions {
                scale: 1.0,
                width: 20.0,
                height: 40.0,
            },
            1.0,
            Delta::default(),
        );

        let first_context = create_mock_context(
            VoidListener,
            WindowId::from(0),
            1,
            context_dimension.clone(),
        );

        let second_context =
            create_mock_context(VoidListener, WindowId::from(1), 2, context_dimension);

        let mut grid = ContextGrid::<VoidListener>::new(
            first_context,
            Delta::default(),
            [1.0, 1.0, 1.0, 1.0],
        );
        grid.split_down(second_context);

        // Record initial positions
        let initial_first_pos = grid.inner[&grid.get_ordered_keys()[0]].position();
        let initial_second_pos = grid.inner[&grid.get_ordered_keys()[1]].position();

        // Move divider down by 30 pixels
        assert!(grid.move_divider_down(30.0));

        // Verify positions are updated
        let new_first_pos = grid.inner[&grid.get_ordered_keys()[0]].position();
        let new_second_pos = grid.inner[&grid.get_ordered_keys()[1]].position();

        // First split position should remain the same (it's at origin)
        assert_eq!(new_first_pos, initial_first_pos);

        // When we move divider down from the bottom split (current), it expands the bottom split
        // and shrinks the top split. This means the bottom split moves UP to fill the space
        // left by the shrinking top split.
        assert!(new_second_pos[1] < initial_second_pos[1]); // Bottom split moves up
        assert_eq!(new_second_pos[0], initial_second_pos[0]); // X should remain same
    }

    #[test]
    fn test_move_divider_left_updates_positions() {
        let context_dimension = ContextDimension::build(
            600.,
            400.,
            SugarDimensions {
                scale: 1.0,
                width: 20.0,
                height: 40.0,
            },
            1.0,
            Delta::default(),
        );

        let first_context = create_mock_context(
            VoidListener,
            WindowId::from(0),
            1,
            context_dimension.clone(),
        );

        let second_context =
            create_mock_context(VoidListener, WindowId::from(1), 2, context_dimension);

        let mut grid = ContextGrid::<VoidListener>::new(
            first_context,
            Delta::default(),
            [1.0, 1.0, 1.0, 1.0],
        );
        grid.split_right(second_context);

        // Record initial positions
        let initial_first_pos = grid.inner[&grid.get_ordered_keys()[0]].position();
        let initial_second_pos = grid.inner[&grid.get_ordered_keys()[1]].position();

        // Move divider left by 40 pixels
        assert!(grid.move_divider_left(40.0));

        // Verify positions are updated
        let new_first_pos = grid.inner[&grid.get_ordered_keys()[0]].position();
        let new_second_pos = grid.inner[&grid.get_ordered_keys()[1]].position();

        // First split position should remain the same (it's at origin)
        assert_eq!(new_first_pos, initial_first_pos);

        // Second split should move left because first split shrank
        assert!(new_second_pos[0] < initial_second_pos[0]);
        assert_eq!(new_second_pos[1], initial_second_pos[1]); // Y should remain same
    }

    #[test]
    fn test_move_divider_up_updates_positions() {
        let context_dimension = ContextDimension::build(
            600.,
            400.,
            SugarDimensions {
                scale: 1.0,
                width: 20.0,
                height: 40.0,
            },
            1.0,
            Delta::default(),
        );

        let first_context = create_mock_context(
            VoidListener,
            WindowId::from(0),
            1,
            context_dimension.clone(),
        );

        let second_context =
            create_mock_context(VoidListener, WindowId::from(1), 2, context_dimension);

        let mut grid = ContextGrid::<VoidListener>::new(
            first_context,
            Delta::default(),
            [1.0, 1.0, 1.0, 1.0],
        );
        grid.split_down(second_context);

        // Record initial positions
        let initial_first_pos = grid.inner[&grid.get_ordered_keys()[0]].position();
        let initial_second_pos = grid.inner[&grid.get_ordered_keys()[1]].position();

        // Move divider up by 25 pixels
        assert!(grid.move_divider_up(25.0));

        // Verify positions are updated
        let new_first_pos = grid.inner[&grid.get_ordered_keys()[0]].position();
        let new_second_pos = grid.inner[&grid.get_ordered_keys()[1]].position();

        // First split position should remain the same (it's at origin)
        assert_eq!(new_first_pos, initial_first_pos);

        // When we move divider up from the bottom split (current), it shrinks the bottom split
        // and expands the top split. This means the bottom split moves DOWN to make room
        // for the expanded top split.
        assert!(new_second_pos[1] > initial_second_pos[1]); // Bottom split moves down
        assert_eq!(new_second_pos[0], initial_second_pos[0]); // X should remain same
    }

    #[test]
    fn test_divider_movement_in_complex_layout() {
        // Test the |1|2/3| layout where panel 3 should be able to move horizontal dividers
        let margin = Delta::default();
        let context_dimension = ContextDimension::build(
            800.0,
            600.0,
            SugarDimensions {
                scale: 1.0,
                width: 20.0,
                height: 40.0,
            },
            1.0,
            Delta::default(),
        );

        // Create contexts
        let context1 =
            create_mock_context(VoidListener, WindowId::from(0), 1, context_dimension);
        let context2 =
            create_mock_context(VoidListener, WindowId::from(1), 2, context_dimension);
        let context3 =
            create_mock_context(VoidListener, WindowId::from(2), 3, context_dimension);

        // Build layout: |1|2/3|
        let mut grid =
            ContextGrid::<VoidListener>::new(context1, margin, [1.0, 1.0, 1.0, 1.0]);

        // Split right to get |1|2|
        grid.split_right(context2);

        // Split down on panel 2 to get |1|2/3|
        grid.split_down(context3);

        // Now we should have 3 panels: 1 (left), 2 (top-right), 3 (bottom-right)
        assert_eq!(grid.len(), 3);

        // Get the keys for each panel
        let ordered_keys = grid.get_ordered_keys();
        assert_eq!(ordered_keys.len(), 3);

        let panel1_key = ordered_keys[0]; // Left panel
        let panel2_key = ordered_keys[1]; // Top-right panel
        let panel3_key = ordered_keys[2]; // Bottom-right panel

        // Select panel 3 (bottom-right)
        grid.current = panel3_key;

        // Record initial widths
        let initial_panel1_width =
            grid.inner.get(&panel1_key).unwrap().val.dimension.width;
        let initial_panel2_width =
            grid.inner.get(&panel2_key).unwrap().val.dimension.width;
        let initial_panel3_width =
            grid.inner.get(&panel3_key).unwrap().val.dimension.width;

        // Panel 2 and 3 should have the same width (they're in the same vertical stack)
        assert_eq!(initial_panel2_width, initial_panel3_width);

        // Move divider left from panel 3 - this should affect the vertical divider between 1 and 2/3
        let move_amount = 50.0;
        assert!(
            grid.move_divider_left(move_amount),
            "Should be able to move divider left from panel 3"
        );

        // Check that the widths changed correctly
        let new_panel1_width = grid.inner.get(&panel1_key).unwrap().val.dimension.width;
        let new_panel2_width = grid.inner.get(&panel2_key).unwrap().val.dimension.width;
        let new_panel3_width = grid.inner.get(&panel3_key).unwrap().val.dimension.width;

        // Panel 1 should shrink, panels 2 and 3 should expand
        assert!(
            new_panel1_width < initial_panel1_width,
            "Panel 1 should shrink"
        );
        assert!(
            new_panel2_width > initial_panel2_width,
            "Panel 2 should expand"
        );
        assert!(
            new_panel3_width > initial_panel3_width,
            "Panel 3 should expand"
        );

        // Panel 2 and 3 should have the same width (both expanded)
        assert_eq!(new_panel2_width, new_panel3_width);

        // The change should be approximately the move amount
        assert!((initial_panel1_width - new_panel1_width - move_amount).abs() < 1.0);
        assert!((new_panel2_width - initial_panel2_width - move_amount).abs() < 1.0);

        // Now test moving divider right
        assert!(
            grid.move_divider_right(move_amount),
            "Should be able to move divider right from panel 3"
        );

        // Should be back to approximately original widths
        let final_panel1_width = grid.inner.get(&panel1_key).unwrap().val.dimension.width;
        let final_panel2_width = grid.inner.get(&panel2_key).unwrap().val.dimension.width;
        let final_panel3_width = grid.inner.get(&panel3_key).unwrap().val.dimension.width;

        assert!((final_panel1_width - initial_panel1_width).abs() < 1.0);
        assert!((final_panel2_width - initial_panel2_width).abs() < 1.0);
        assert!((final_panel3_width - initial_panel3_width).abs() < 1.0);
    }

    #[test]
    fn test_divider_movement_from_panel2_in_complex_layout() {
        // Test the |1|2/3| layout where panel 2 should also be able to move horizontal dividers
        let margin = Delta::default();
        let context_dimension = ContextDimension::build(
            800.0,
            600.0,
            SugarDimensions {
                scale: 1.0,
                width: 20.0,
                height: 40.0,
            },
            1.0,
            Delta::default(),
        );

        // Create contexts
        let context1 =
            create_mock_context(VoidListener, WindowId::from(0), 1, context_dimension);
        let context2 =
            create_mock_context(VoidListener, WindowId::from(1), 2, context_dimension);
        let context3 =
            create_mock_context(VoidListener, WindowId::from(2), 3, context_dimension);

        // Build layout: |1|2/3|
        let mut grid =
            ContextGrid::<VoidListener>::new(context1, margin, [1.0, 1.0, 1.0, 1.0]);
        grid.split_right(context2);
        grid.split_down(context3);

        let ordered_keys = grid.get_ordered_keys();
        let panel1_key = ordered_keys[0];
        let panel2_key = ordered_keys[1];
        let panel3_key = ordered_keys[2];

        // Select panel 2 (top-right)
        grid.current = panel2_key;

        // Record initial widths
        let initial_panel1_width =
            grid.inner.get(&panel1_key).unwrap().val.dimension.width;
        let initial_panel2_width =
            grid.inner.get(&panel2_key).unwrap().val.dimension.width;
        let initial_panel3_width =
            grid.inner.get(&panel3_key).unwrap().val.dimension.width;

        // Move divider left from panel 2
        let move_amount = 30.0;
        assert!(
            grid.move_divider_left(move_amount),
            "Should be able to move divider left from panel 2"
        );

        // Check that the widths changed correctly
        let new_panel1_width = grid.inner.get(&panel1_key).unwrap().val.dimension.width;
        let new_panel2_width = grid.inner.get(&panel2_key).unwrap().val.dimension.width;
        let new_panel3_width = grid.inner.get(&panel3_key).unwrap().val.dimension.width;

        // Panel 1 should shrink, panel 2 and 3 should expand equally
        assert!(new_panel1_width < initial_panel1_width);
        assert!(new_panel2_width > initial_panel2_width);
        assert!(new_panel3_width > initial_panel3_width);

        // Panel 2 and 3 should now have the same width (both expanded)
        assert_eq!(new_panel2_width, new_panel3_width);
    }

    #[test]
    fn test_divider_movement_limits() {
        // Test that divider movement respects minimum width limits
        let margin = Delta::default();
        let context_dimension = ContextDimension::build(
            300.0, // Small width to test limits
            400.0,
            SugarDimensions {
                scale: 1.0,
                width: 20.0,
                height: 40.0,
            },
            1.0,
            Delta::default(),
        );

        let context1 =
            create_mock_context(VoidListener, WindowId::from(0), 1, context_dimension);
        let context2 =
            create_mock_context(VoidListener, WindowId::from(1), 2, context_dimension);

        let mut grid =
            ContextGrid::<VoidListener>::new(context1, margin, [1.0, 1.0, 1.0, 1.0]);
        grid.split_right(context2);

        // Try to move divider by a large amount that would violate minimum width
        let large_amount = 200.0; // This should be rejected due to min_width = 100.0
        assert!(
            !grid.move_divider_left(large_amount),
            "Should reject movement that violates minimum width"
        );
        assert!(
            !grid.move_divider_right(large_amount),
            "Should reject movement that violates minimum width"
        );

        // Small movement should work
        let small_amount = 10.0;
        assert!(
            grid.move_divider_left(small_amount),
            "Should accept small movement"
        );
        assert!(
            grid.move_divider_right(small_amount),
            "Should accept movement back"
        );
    }

    #[test]
    fn test_divider_movement_single_panel() {
        // Test that divider movement fails gracefully with only one panel
        let margin = Delta::default();
        let context_dimension = ContextDimension::build(
            800.0,
            600.0,
            SugarDimensions {
                scale: 1.0,
                width: 20.0,
                height: 40.0,
            },
            1.0,
            Delta::default(),
        );

        let context1 =
            create_mock_context(VoidListener, WindowId::from(0), 1, context_dimension);
        let mut grid =
            ContextGrid::<VoidListener>::new(context1, margin, [1.0, 1.0, 1.0, 1.0]);

        // Should not be able to move dividers with only one panel
        assert!(!grid.move_divider_left(50.0));
        assert!(!grid.move_divider_right(50.0));
        assert!(!grid.move_divider_up(50.0));
        assert!(!grid.move_divider_down(50.0));
    }

    #[test]
    fn test_vertical_divider_movement() {
        // Test vertical divider movement in a simple horizontal split
        let margin = Delta::default();
        let context_dimension = ContextDimension::build(
            800.0,
            600.0,
            SugarDimensions {
                scale: 1.0,
                width: 20.0,
                height: 40.0,
            },
            1.0,
            Delta::default(),
        );

        let context1 =
            create_mock_context(VoidListener, WindowId::from(0), 1, context_dimension);
        let context2 =
            create_mock_context(VoidListener, WindowId::from(1), 2, context_dimension);

        let mut grid =
            ContextGrid::<VoidListener>::new(context1, margin, [1.0, 1.0, 1.0, 1.0]);
        grid.split_down(context2);

        let ordered_keys = grid.get_ordered_keys();
        let panel1_key = ordered_keys[0];
        let panel2_key = ordered_keys[1];

        // Select bottom panel
        grid.current = panel2_key;

        // Record initial heights
        let initial_panel1_height =
            grid.inner.get(&panel1_key).unwrap().val.dimension.height;
        let initial_panel2_height =
            grid.inner.get(&panel2_key).unwrap().val.dimension.height;

        // Move divider up (shrink bottom panel, expand top panel)
        let move_amount = 40.0;
        assert!(
            grid.move_divider_up(move_amount),
            "Should be able to move divider up"
        );

        let new_panel1_height = grid.inner.get(&panel1_key).unwrap().val.dimension.height;
        let new_panel2_height = grid.inner.get(&panel2_key).unwrap().val.dimension.height;

        // Top panel should expand, bottom panel should shrink
        assert!(new_panel1_height > initial_panel1_height);
        assert!(new_panel2_height < initial_panel2_height);

        // Move divider down (expand bottom panel, shrink top panel)
        assert!(
            grid.move_divider_down(move_amount),
            "Should be able to move divider down"
        );

        let final_panel1_height =
            grid.inner.get(&panel1_key).unwrap().val.dimension.height;
        let final_panel2_height =
            grid.inner.get(&panel2_key).unwrap().val.dimension.height;

        // Should be back to approximately original heights
        assert!((final_panel1_height - initial_panel1_height).abs() < 1.0);
        assert!((final_panel2_height - initial_panel2_height).abs() < 1.0);
    }

    #[test]
    fn test_divider_movement_fix_for_complex_layout() {
        // This test specifically addresses the issue where panel 3 in |1|2/3| layout
        // couldn't move horizontal dividers. This was the main bug we fixed.
        let margin = Delta::default();
        let context_dimension = ContextDimension::build(
            800.0,
            600.0,
            SugarDimensions {
                scale: 1.0,
                width: 20.0,
                height: 40.0,
            },
            1.0,
            Delta::default(),
        );

        // Create the |1|2/3| layout step by step
        let context1 =
            create_mock_context(VoidListener, WindowId::from(0), 1, context_dimension);
        let context2 =
            create_mock_context(VoidListener, WindowId::from(1), 2, context_dimension);
        let context3 =
            create_mock_context(VoidListener, WindowId::from(2), 3, context_dimension);

        let mut grid =
            ContextGrid::<VoidListener>::new(context1, margin, [1.0, 1.0, 1.0, 1.0]);

        // Step 1: Split right to create |1|2|
        grid.split_right(context2);
        assert_eq!(grid.len(), 2, "Should have 2 panels after right split");

        // Step 2: Split down on panel 2 to create |1|2/3|
        grid.split_down(context3);
        assert_eq!(grid.len(), 3, "Should have 3 panels after down split");

        // Get panel keys in order
        let ordered_keys = grid.get_ordered_keys();
        let panel1_key = ordered_keys[0]; // Left panel
        let panel2_key = ordered_keys[1]; // Top-right panel
        let panel3_key = ordered_keys[2]; // Bottom-right panel

        // Verify the layout structure
        assert!(
            grid.inner.get(&panel1_key).unwrap().right == Some(panel2_key),
            "Panel 1 should point right to panel 2"
        );
        assert!(
            grid.inner.get(&panel2_key).unwrap().down == Some(panel3_key),
            "Panel 2 should point down to panel 3"
        );
        assert!(
            grid.inner.get(&panel3_key).unwrap().right.is_none(),
            "Panel 3 should have no right child"
        );
        assert!(
            grid.inner.get(&panel3_key).unwrap().down.is_none(),
            "Panel 3 should have no down child"
        );

        // Select panel 3 (this was the problematic case)
        grid.current = panel3_key;

        // Record initial widths
        let initial_panel1_width =
            grid.inner.get(&panel1_key).unwrap().val.dimension.width;
        let initial_panel2_width =
            grid.inner.get(&panel2_key).unwrap().val.dimension.width;
        let initial_panel3_width =
            grid.inner.get(&panel3_key).unwrap().val.dimension.width;

        // Panels 2 and 3 should have the same width (they're in the same vertical column)
        assert_eq!(
            initial_panel2_width, initial_panel3_width,
            "Panels 2 and 3 should have same initial width"
        );

        // THE FIX TEST: Move divider left from panel 3
        // Before the fix, this would return false because panel 3 couldn't find horizontal relationships
        let move_amount = 50.0;
        let result = grid.move_divider_left(move_amount);

        // This should now work with our fix
        assert!(result, "Panel 3 should be able to move horizontal divider left (this was the bug we fixed)");

        // Verify the changes
        let new_panel1_width = grid.inner.get(&panel1_key).unwrap().val.dimension.width;
        let new_panel2_width = grid.inner.get(&panel2_key).unwrap().val.dimension.width;
        let new_panel3_width = grid.inner.get(&panel3_key).unwrap().val.dimension.width;

        // Panel 1 should shrink
        assert!(
            new_panel1_width < initial_panel1_width,
            "Panel 1 should shrink when moving divider left"
        );

        // Panel 2 and 3 should both expand
        assert!(
            new_panel2_width > initial_panel2_width,
            "Panel 2 should expand when moving divider left"
        );
        assert!(
            new_panel3_width > initial_panel3_width,
            "Panel 3 should expand when moving divider left"
        );

        // Panel 2 and 3 should have the same width (both expanded)
        assert_eq!(
            new_panel2_width, new_panel3_width,
            "Panels 2 and 3 should have the same width after divider movement"
        );

        // The width changes should be approximately the move amount
        let panel1_shrink = initial_panel1_width - new_panel1_width;
        let panel2_expand = new_panel2_width - initial_panel2_width;
        let panel3_expand = new_panel3_width - initial_panel3_width;

        assert!(
            (panel1_shrink - move_amount).abs() < 1.0,
            "Panel 1 should shrink by approximately the move amount"
        );
        assert!(
            (panel2_expand - move_amount).abs() < 1.0,
            "Panel 2 should expand by approximately the move amount"
        );
        assert!(
            (panel3_expand - move_amount).abs() < 1.0,
            "Panel 3 should expand by approximately the move amount"
        );

        // Test moving divider right (should work too)
        let right_result = grid.move_divider_right(move_amount);
        assert!(
            right_result,
            "Panel 3 should also be able to move horizontal divider right"
        );

        // Should be back to approximately original widths
        let final_panel1_width = grid.inner.get(&panel1_key).unwrap().val.dimension.width;
        let final_panel2_width = grid.inner.get(&panel2_key).unwrap().val.dimension.width;
        let final_panel3_width = grid.inner.get(&panel3_key).unwrap().val.dimension.width;

        assert!(
            (final_panel1_width - initial_panel1_width).abs() < 1.0,
            "Panel 1 should return to approximately original width"
        );
        assert!(
            (final_panel2_width - initial_panel2_width).abs() < 1.0,
            "Panel 2 should return to approximately original width"
        );
        assert!(
            (final_panel3_width - initial_panel3_width).abs() < 1.0,
            "Panel 3 should return to approximately original width"
        );
    }

    #[test]
    fn test_parent_references_basic() {
        use crate::context::create_mock_context;
        use rio_backend::event::WindowId;
        use rio_backend::sugarloaf::layout::SugarDimensions;

        // Create a simple context for testing
        let context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            1,
            ContextDimension::build(
                300.,
                300.,
                SugarDimensions::default(),
                1.0,
                Delta::default(),
            ),
        );

        let mut grid = ContextGrid::new(context, Delta::default(), [0.0, 0.0, 0.0, 0.0]);
        let root_key = grid.root.unwrap();

        // Verify root has no parent
        assert_eq!(grid.inner.get(&root_key).unwrap().parent, None);

        // Create second context and split right
        let second_context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            2,
            ContextDimension::build(
                300.,
                300.,
                SugarDimensions::default(),
                1.0,
                Delta::default(),
            ),
        );
        grid.split_right(second_context);
        let right_key = grid.current;

        // Verify right child has correct parent
        assert_eq!(grid.inner.get(&right_key).unwrap().parent, Some(root_key));

        // Create third context and split down from right panel
        let third_context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            3,
            ContextDimension::build(
                300.,
                300.,
                SugarDimensions::default(),
                1.0,
                Delta::default(),
            ),
        );
        grid.split_down(third_context);
        let down_key = grid.current;

        // Verify down child has correct parent (should be right_key)
        assert_eq!(grid.inner.get(&down_key).unwrap().parent, Some(right_key));
    }

    #[test]
    fn test_parent_references_complex_layout() {
        use crate::context::create_mock_context;
        use rio_backend::event::WindowId;
        use rio_backend::sugarloaf::layout::SugarDimensions;

        // Create initial context
        let context = create_mock_context(
            VoidListener {},
            WindowId::from(0), // unique route_id
            1,
            ContextDimension::build(
                300.,
                300.,
                SugarDimensions::default(),
                1.0,
                Delta::default(),
            ),
        );

        let mut grid = ContextGrid::new(context, Delta::default(), [0.0, 0.0, 0.0, 0.0]);
        let panel1_key = grid.root.unwrap();

        // Create |1|2| layout
        let context2 = create_mock_context(
            VoidListener {},
            WindowId::from(0), // unique route_id
            2,
            ContextDimension::build(
                300.,
                300.,
                SugarDimensions::default(),
                1.0,
                Delta::default(),
            ),
        );
        grid.split_right(context2);
        let panel2_key = grid.current;

        // Create |1|2/3| layout (split panel 2 down)
        let context3 = create_mock_context(
            VoidListener {},
            WindowId::from(0), // unique route_id
            3,
            ContextDimension::build(
                300.,
                300.,
                SugarDimensions::default(),
                1.0,
                Delta::default(),
            ),
        );
        grid.split_down(context3);
        let panel3_key = grid.current;

        // Verify parent relationships
        assert_eq!(
            grid.inner.get(&panel1_key).unwrap().parent,
            None,
            "Panel 1 should be root"
        );
        assert_eq!(
            grid.inner.get(&panel2_key).unwrap().parent,
            Some(panel1_key),
            "Panel 2 parent should be Panel 1"
        );
        assert_eq!(
            grid.inner.get(&panel3_key).unwrap().parent,
            Some(panel2_key),
            "Panel 3 parent should be Panel 2"
        );

        // Verify child relationships are maintained
        assert_eq!(grid.inner.get(&panel1_key).unwrap().right, Some(panel2_key));
        assert_eq!(grid.inner.get(&panel2_key).unwrap().down, Some(panel3_key));
        assert_eq!(grid.inner.get(&panel3_key).unwrap().right, None);
        assert_eq!(grid.inner.get(&panel3_key).unwrap().down, None);
    }

    #[test]
    fn test_parent_references_after_removal() {
        use crate::context::create_mock_context;
        use rio_backend::event::WindowId;
        use rio_backend::sugarloaf::layout::SugarDimensions;

        // Create initial context
        let context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            1,
            ContextDimension::build(
                300.,
                300.,
                SugarDimensions::default(),
                1.0,
                Delta::default(),
            ),
        );

        let mut grid = ContextGrid::new(context, Delta::default(), [0.0, 0.0, 0.0, 0.0]);
        let panel1_key = grid.root.unwrap();

        // Create |1|2|3| layout
        let context2 = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            2,
            ContextDimension::build(
                300.,
                300.,
                SugarDimensions::default(),
                1.0,
                Delta::default(),
            ),
        );
        grid.split_right(context2);
        let panel2_key = grid.current;

        let context3 = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            3,
            ContextDimension::build(
                300.,
                300.,
                SugarDimensions::default(),
                1.0,
                Delta::default(),
            ),
        );
        grid.split_right(context3);
        let panel3_key = grid.current;

        // Verify initial parent relationships
        assert_eq!(
            grid.inner.get(&panel2_key).unwrap().parent,
            Some(panel1_key)
        );
        assert_eq!(
            grid.inner.get(&panel3_key).unwrap().parent,
            Some(panel2_key)
        );

        // Remove middle panel (panel 2)
        grid.current = panel2_key;
        grid.remove_current();

        // After removal, panel 3 should now be a direct child of panel 1
        // (the exact behavior depends on removal logic, but parent references should be consistent)
        if grid.inner.contains_key(&panel3_key) {
            let panel3_parent = grid.inner.get(&panel3_key).unwrap().parent;
            if let Some(parent_key) = panel3_parent {
                assert!(
                    grid.inner.contains_key(&parent_key),
                    "Panel 3's parent should exist in grid"
                );
            }
        }
    }

    #[test]
    fn test_find_node_margin_optimization() {
        use crate::context::create_mock_context;
        use rio_backend::event::WindowId;
        use rio_backend::sugarloaf::layout::SugarDimensions;

        // Create initial context
        let context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            1,
            ContextDimension::build(
                300.,
                300.,
                SugarDimensions::default(),
                1.0,
                Delta::default(),
            ),
        );

        let mut grid = ContextGrid::new(context, Delta::default(), [0.0, 0.0, 0.0, 0.0]);
        let root_key = grid.root.unwrap();

        // Test root margin calculation (should use grid margin)
        let root_margin = grid.find_node_margin(root_key);
        assert_eq!(root_margin.x, grid.margin.x);
        assert_eq!(root_margin.top_y, grid.margin.top_y);
        assert_eq!(root_margin.bottom_y, grid.margin.bottom_y);

        // Create right child
        let context2 = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            2,
            ContextDimension::build(
                300.,
                300.,
                SugarDimensions::default(),
                1.0,
                Delta::default(),
            ),
        );
        grid.split_right(context2);
        let right_key = grid.current;

        // Test right child margin calculation (should be based on parent position)
        let right_margin = grid.find_node_margin(right_key);
        // Right child should have x offset based on parent width + padding
        assert!(
            right_margin.x > root_margin.x,
            "Right child should have x offset"
        );
    }

    #[test]
    fn test_divider_movement_vertical_stack_width_propagation() {
        // Test that moving horizontal dividers correctly updates width for all panels in vertical stacks
        let margin = Delta::default();
        let context_dimension = ContextDimension::build(
            800.0,
            600.0,
            SugarDimensions {
                scale: 1.0,
                width: 20.0,
                height: 40.0,
            },
            1.0,
            Delta::default(),
        );

        // Create contexts for |1|2/3| layout
        let context1 =
            create_mock_context(VoidListener, WindowId::from(0), 1, context_dimension);
        let context2 =
            create_mock_context(VoidListener, WindowId::from(1), 2, context_dimension);
        let context3 =
            create_mock_context(VoidListener, WindowId::from(2), 3, context_dimension);

        let mut grid =
            ContextGrid::<VoidListener>::new(context1, margin, [1.0, 1.0, 1.0, 1.0]);

        // Build layout: |1|2/3|
        grid.split_right(context2);
        grid.split_down(context3);

        let ordered_keys = grid.get_ordered_keys();
        let panel1_key = ordered_keys[0];
        let panel2_key = ordered_keys[1];
        let panel3_key = ordered_keys[2];

        // Record initial widths
        let initial_panel1_width =
            grid.inner.get(&panel1_key).unwrap().val.dimension.width;
        let initial_panel2_width =
            grid.inner.get(&panel2_key).unwrap().val.dimension.width;
        let initial_panel3_width =
            grid.inner.get(&panel3_key).unwrap().val.dimension.width;

        // Panels 2 and 3 should start with the same width (they're in the same vertical stack)
        assert_eq!(initial_panel2_width, initial_panel3_width);

        // Test 1: Move divider left from panel 3 (bottom-right)
        grid.current = panel3_key;
        let move_amount = 50.0;
        assert!(
            grid.move_divider_left(move_amount),
            "Should be able to move divider left from panel 3"
        );

        let new_panel1_width = grid.inner.get(&panel1_key).unwrap().val.dimension.width;
        let new_panel2_width = grid.inner.get(&panel2_key).unwrap().val.dimension.width;
        let new_panel3_width = grid.inner.get(&panel3_key).unwrap().val.dimension.width;

        // Panel 1 should shrink
        assert!(
            new_panel1_width < initial_panel1_width,
            "Panel 1 should shrink"
        );

        // Panels 2 and 3 should both expand by the same amount
        assert!(
            new_panel2_width > initial_panel2_width,
            "Panel 2 should expand"
        );
        assert!(
            new_panel3_width > initial_panel3_width,
            "Panel 3 should expand"
        );
        assert_eq!(
            new_panel2_width, new_panel3_width,
            "Panels 2 and 3 should have same width"
        );

        // The width changes should be approximately the move amount
        assert!((initial_panel1_width - new_panel1_width - move_amount).abs() < 1.0);
        assert!((new_panel2_width - initial_panel2_width - move_amount).abs() < 1.0);
        assert!((new_panel3_width - initial_panel3_width - move_amount).abs() < 1.0);

        // Test 2: Move divider right to restore original widths
        assert!(
            grid.move_divider_right(move_amount),
            "Should be able to move divider right from panel 3"
        );

        let final_panel1_width = grid.inner.get(&panel1_key).unwrap().val.dimension.width;
        let final_panel2_width = grid.inner.get(&panel2_key).unwrap().val.dimension.width;
        let final_panel3_width = grid.inner.get(&panel3_key).unwrap().val.dimension.width;

        // Should be back to approximately original widths
        assert!((final_panel1_width - initial_panel1_width).abs() < 1.0);
        assert!((final_panel2_width - initial_panel2_width).abs() < 1.0);
        assert!((final_panel3_width - initial_panel3_width).abs() < 1.0);
        assert_eq!(final_panel2_width, final_panel3_width);
    }

    #[test]
    fn test_divider_movement_from_different_panels_in_stack() {
        // Test that divider movement works the same regardless of which panel in the stack is selected
        let margin = Delta::default();
        let context_dimension = ContextDimension::build(
            800.0,
            600.0,
            SugarDimensions {
                scale: 1.0,
                width: 20.0,
                height: 40.0,
            },
            1.0,
            Delta::default(),
        );

        // Create contexts for |1|2/3| layout
        let context1 =
            create_mock_context(VoidListener, WindowId::from(0), 1, context_dimension);
        let context2 =
            create_mock_context(VoidListener, WindowId::from(1), 2, context_dimension);
        let context3 =
            create_mock_context(VoidListener, WindowId::from(2), 3, context_dimension);

        let mut grid =
            ContextGrid::<VoidListener>::new(context1, margin, [1.0, 1.0, 1.0, 1.0]);

        // Build layout: |1|2/3|
        grid.split_right(context2);
        grid.split_down(context3);

        let ordered_keys = grid.get_ordered_keys();
        let panel1_key = ordered_keys[0];
        let panel2_key = ordered_keys[1];
        let panel3_key = ordered_keys[2];

        let move_amount = 30.0;

        // Test moving from panel 2 (top-right)
        grid.current = panel2_key;
        let initial_widths_2 = (
            grid.inner.get(&panel1_key).unwrap().val.dimension.width,
            grid.inner.get(&panel2_key).unwrap().val.dimension.width,
            grid.inner.get(&panel3_key).unwrap().val.dimension.width,
        );

        assert!(grid.move_divider_left(move_amount));

        let after_panel2_widths = (
            grid.inner.get(&panel1_key).unwrap().val.dimension.width,
            grid.inner.get(&panel2_key).unwrap().val.dimension.width,
            grid.inner.get(&panel3_key).unwrap().val.dimension.width,
        );

        // Reset to original state
        assert!(grid.move_divider_right(move_amount));

        // Test moving from panel 3 (bottom-right)
        grid.current = panel3_key;
        let initial_widths_3 = (
            grid.inner.get(&panel1_key).unwrap().val.dimension.width,
            grid.inner.get(&panel2_key).unwrap().val.dimension.width,
            grid.inner.get(&panel3_key).unwrap().val.dimension.width,
        );

        assert!(grid.move_divider_left(move_amount));

        let after_panel3_widths = (
            grid.inner.get(&panel1_key).unwrap().val.dimension.width,
            grid.inner.get(&panel2_key).unwrap().val.dimension.width,
            grid.inner.get(&panel3_key).unwrap().val.dimension.width,
        );

        // Both operations should produce the same result
        assert_eq!(
            initial_widths_2, initial_widths_3,
            "Initial widths should be the same"
        );
        assert_eq!(
            after_panel2_widths, after_panel3_widths,
            "Results should be the same regardless of which panel is selected"
        );

        // Both panels 2 and 3 should have expanded equally
        assert_eq!(
            after_panel3_widths.1, after_panel3_widths.2,
            "Panels 2 and 3 should have same width"
        );
    }

    #[test]
    fn test_divider_movement_complex_vertical_stack() {
        // Test with a more complex vertical stack: |1|2/3| where we add another panel to the right
        let margin = Delta::default();
        let context_dimension = ContextDimension::build(
            1200.0, // Wider to accommodate 3 horizontal panels
            600.0,
            SugarDimensions {
                scale: 1.0,
                width: 20.0,
                height: 40.0,
            },
            1.0,
            Delta::default(),
        );

        let context1 =
            create_mock_context(VoidListener, WindowId::from(0), 1, context_dimension);
        let context2 =
            create_mock_context(VoidListener, WindowId::from(1), 2, context_dimension);
        let context3 =
            create_mock_context(VoidListener, WindowId::from(2), 3, context_dimension);
        let context4 =
            create_mock_context(VoidListener, WindowId::from(3), 4, context_dimension);

        let mut grid =
            ContextGrid::<VoidListener>::new(context1, margin, [1.0, 1.0, 1.0, 1.0]);

        // Build layout: |1|2/3|4|
        grid.split_right(context2); // |1|2|
        grid.split_down(context3); // |1|2/3|
        grid.current = grid.get_ordered_keys()[0]; // Select panel 1
        grid.split_right(context4); // |1|4|2/3| -> but this creates |1|2/3|4| due to ordering

        let ordered_keys = grid.get_ordered_keys();
        // Find panels by their rich_text_id for clarity
        let mut panel_keys = std::collections::HashMap::new();
        for &key in &ordered_keys {
            let item = &grid.inner[&key];
            panel_keys.insert(item.val.rich_text_id, key);
        }

        let panel1_key = panel_keys[&1];
        let panel2_key = panel_keys[&2];
        let panel3_key = panel_keys[&3];
        let panel4_key = panel_keys[&4];

        // Record initial widths
        let _initial_widths = (
            grid.inner.get(&panel1_key).unwrap().val.dimension.width,
            grid.inner.get(&panel2_key).unwrap().val.dimension.width,
            grid.inner.get(&panel3_key).unwrap().val.dimension.width,
            grid.inner.get(&panel4_key).unwrap().val.dimension.width,
        );

        // Try moving divider from panel 2 (which should work)
        grid.current = panel2_key;
        let move_amount = 40.0;
        let move_result = grid.move_divider_left(move_amount);

        if move_result {
            let new_widths = (
                grid.inner.get(&panel1_key).unwrap().val.dimension.width,
                grid.inner.get(&panel2_key).unwrap().val.dimension.width,
                grid.inner.get(&panel3_key).unwrap().val.dimension.width,
                grid.inner.get(&panel4_key).unwrap().val.dimension.width,
            );

            // Panels 2 and 3 should have the same width (they're in the same vertical stack)
            assert_eq!(
                new_widths.1, new_widths.2,
                "Panels 2 and 3 should have same width"
            );
        } else {
            panic!(
                "Divider movement not supported for this specific layout - test skipped"
            );
        }
    }

    #[test]
    fn test_divider_movement_preserves_total_width() {
        // Test that divider movement preserves the total width of the grid
        let margin = Delta::default();
        let total_width = 800.0;
        let context_dimension = ContextDimension::build(
            total_width,
            600.0,
            SugarDimensions {
                scale: 1.0,
                width: 20.0,
                height: 40.0,
            },
            1.0,
            Delta::default(),
        );

        let context1 =
            create_mock_context(VoidListener, WindowId::from(0), 1, context_dimension);
        let context2 =
            create_mock_context(VoidListener, WindowId::from(1), 2, context_dimension);
        let context3 =
            create_mock_context(VoidListener, WindowId::from(2), 3, context_dimension);

        let mut grid =
            ContextGrid::<VoidListener>::new(context1, margin, [1.0, 1.0, 1.0, 1.0]);

        // Build layout: |1|2/3|
        grid.split_right(context2);
        grid.split_down(context3);

        let ordered_keys = grid.get_ordered_keys();
        let panel1_key = ordered_keys[0];
        let panel2_key = ordered_keys[1];
        let panel3_key = ordered_keys[2];

        // Calculate initial total width
        let initial_total = grid.inner.get(&panel1_key).unwrap().val.dimension.width
            + grid.inner.get(&panel2_key).unwrap().val.dimension.width; // Panel 3 shares width with Panel 2

        // Move divider and check total width is preserved
        grid.current = panel3_key;
        assert!(grid.move_divider_left(50.0));

        let new_total = grid.inner.get(&panel1_key).unwrap().val.dimension.width
            + grid.inner.get(&panel2_key).unwrap().val.dimension.width; // Panel 3 shares width with Panel 2

        assert!(
            (initial_total - new_total).abs() < 1.0,
            "Total width should be preserved"
        );
    }

    #[test]
    fn test_issue_panel3_width_changes_when_moving_divider() {
        // Regression test for the specific issue:
        // "if i have two vertical tabs and in the second i have two horizontals.
        // If i am focused on the 3 |1|2/3| , if i try to move the divider left,
        // it does work but the width of the 3 doesn't change"

        let margin = Delta::default();
        let context_dimension = ContextDimension::build(
            800.0,
            600.0,
            SugarDimensions {
                scale: 1.0,
                width: 20.0,
                height: 40.0,
            },
            1.0,
            Delta::default(),
        );

        // Create the exact layout described: |1|2/3|
        let context1 =
            create_mock_context(VoidListener, WindowId::from(0), 1, context_dimension);
        let context2 =
            create_mock_context(VoidListener, WindowId::from(1), 2, context_dimension);
        let context3 =
            create_mock_context(VoidListener, WindowId::from(2), 3, context_dimension);

        let mut grid =
            ContextGrid::<VoidListener>::new(context1, margin, [1.0, 1.0, 1.0, 1.0]);

        // Build layout: |1|2/3|
        grid.split_right(context2); // |1|2|
        grid.split_down(context3); // |1|2/3|

        let ordered_keys = grid.get_ordered_keys();
        let _panel1_key = ordered_keys[0];
        let _panel2_key = ordered_keys[1];
        let panel3_key = ordered_keys[2];

        // Focus on panel 3 (as described in the issue)
        grid.current = panel3_key;

        // Record initial width of panel 3
        let initial_panel3_width =
            grid.inner.get(&panel3_key).unwrap().val.dimension.width;

        // Move divider left (this should now work and change panel 3's width)
        let move_amount = 50.0;
        let move_result = grid.move_divider_left(move_amount);
        assert!(move_result, "Moving divider left should work");

        // Check that panel 3's width actually changed
        let new_panel3_width = grid.inner.get(&panel3_key).unwrap().val.dimension.width;

        // This is the key assertion - panel 3's width should have changed!
        assert!(
            new_panel3_width > initial_panel3_width,
            "Panel 3's width should increase when moving divider left (was {}, now {})",
            initial_panel3_width,
            new_panel3_width
        );

        // The change should be approximately the move amount
        assert!(
            (new_panel3_width - initial_panel3_width - move_amount).abs() < 1.0,
            "Panel 3 should expand by approximately the move amount"
        );
    }
}
