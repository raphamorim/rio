use crate::context::Context;
use crate::mouse::Mouse;
use rio_backend::crosswords::grid::Dimensions;
use rio_backend::event::EventListener;
use rio_backend::sugarloaf::{
    layout::SugarDimensions, Object, Quad, RichText, Sugarloaf,
};

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
    inner: Vec<ContextGridItem<T>>,
}

pub struct ContextGridItem<T: EventListener> {
    val: Context<T>,
    right: Option<usize>,
    down: Option<usize>,
}

impl<T: rio_backend::event::EventListener> ContextGridItem<T> {
    pub fn new(context: Context<T>) -> Self {
        Self {
            val: context,
            right: None,
            down: None,
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
}

impl<T: rio_backend::event::EventListener> ContextGrid<T> {
    pub fn new(context: Context<T>, margin: Delta<f32>, border_color: [f32; 4]) -> Self {
        let width = context.dimension.width;
        let height = context.dimension.height;
        let inner = vec![ContextGridItem::new(context)];
        Self {
            inner,
            current: 0,
            margin,
            width,
            height,
            border_color,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[inline]
    pub fn contexts_mut(&mut self) -> &mut Vec<ContextGridItem<T>> {
        &mut self.inner
    }

    #[inline]
    #[allow(unused)]
    pub fn contexts(&mut self) -> &Vec<ContextGridItem<T>> {
        &self.inner
    }

    #[inline]
    pub fn select_next_split(&mut self) {
        if self.inner.len() == 1 {
            return;
        }

        if self.current >= self.inner.len() - 1 {
            self.current = 0;
        } else {
            self.current += 1;
        }
    }

    #[inline]
    pub fn select_next_split_no_loop(&mut self) -> bool {
        if self.inner.len() == 1 {
            return false;
        }

        if self.current >= self.inner.len() - 1 {
            return false;
        } else {
            self.current += 1;
        }

        true
    }

    #[inline]
    pub fn select_prev_split(&mut self) {
        if self.inner.len() == 1 {
            return;
        }

        if self.current == 0 {
            self.current = self.inner.len() - 1;
        } else {
            self.current -= 1;
        }
    }

    #[inline]
    pub fn select_prev_split_no_loop(&mut self) -> bool {
        if self.inner.len() == 1 {
            return false;
        }

        if self.current == 0 {
            return false;
        } else {
            self.current -= 1;
        }
        true
    }

    #[inline]
    #[allow(unused)]
    pub fn current_index(&self) -> usize {
        self.current
    }

    #[inline]
    pub fn current(&self) -> &Context<T> {
        if self.current >= self.inner.len() {
            // This should never happen, but if it does, return the first context
            tracing::error!(
                "Current index {} is out of bounds (len: {})",
                self.current,
                self.inner.len()
            );
            return &self.inner[0].val;
        }
        &self.inner[self.current].val
    }

    #[inline]
    pub fn current_mut(&mut self) -> &mut Context<T> {
        if self.current >= self.inner.len() {
            // This should never happen, but if it does, return the first context
            tracing::error!(
                "Current index {} is out of bounds (len: {})",
                self.current,
                self.inner.len()
            );
            self.current = 0;
        }
        &mut self.inner[self.current].val
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
            if let Some(item) = self.inner.first() {
                target.push(Object::RichText(RichText {
                    id: item.val.rich_text_id,
                    position: [self.margin.x, self.margin.top_y],
                    lines: None,
                }));
            }
        } else {
            self.plot_objects(target, 0, self.margin);
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
            if let Some(item) = self.inner.first() {
                objects.push(Object::RichText(RichText {
                    id: item.val.rich_text_id,
                    position: [self.margin.x, self.margin.top_y],
                    lines: None,
                }));
            }
        } else {
            self.plot_objects(&mut objects, 0, self.margin);
        }
        objects
    }

    pub fn current_context_with_computed_dimension(&self) -> (&Context<T>, Delta<f32>) {
        let len = self.inner.len();
        if len <= 1 {
            if self.current >= len {
                tracing::error!(
                    "Current index {} is out of bounds (len: {})",
                    self.current,
                    len
                );
                return (&self.inner[0].val, self.margin);
            }
            return (&self.inner[self.current].val, self.margin);
        }

        if self.current >= len {
            tracing::error!(
                "Current index {} is out of bounds (len: {})",
                self.current,
                len
            );
            return (&self.inner[0].val, self.margin);
        }

        let objects = self.objects();
        let rich_text_id = self.inner[self.current].val.rich_text_id;
        let scale = self.inner[self.current].val.dimension.dimension.scale;
        let scaled_padding = PADDING * scale;

        let mut margin = self.margin;
        for obj in objects {
            if let Object::RichText(rich_text_obj) = obj {
                if rich_text_obj.id == rich_text_id {
                    margin.x = rich_text_obj.position[0] + scaled_padding;
                    margin.top_y = rich_text_obj.position[1] + scaled_padding;
                    break;
                }
            }
        }

        (&self.inner[self.current].val, margin)
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
                if let Some(position) = self.find_by_rich_text_id(rich_text_obj.id) {
                    let scaled_position_x = rich_text_obj.position[0]
                        * self.inner[position].val.dimension.dimension.scale;
                    let scaled_position_y = rich_text_obj.position[1]
                        * self.inner[position].val.dimension.dimension.scale;
                    if mouse.x >= scaled_position_x as usize
                        && mouse.y >= scaled_position_y as usize
                    {
                        // println!("{:?} {:?} {:?}", mouse.x <= (scaled_position_x + self.inner[position].val.dimension.width) as usize, mouse.x, scaled_position_x + self.inner[position].val.dimension.width);
                        // println!("{:?} {:?} {:?}", mouse.y <= (scaled_position_y + self.inner[position].val.dimension.height) as usize, mouse.y, scaled_position_y + self.inner[position].val.dimension.height);
                        if mouse.x
                            <= (scaled_position_x
                                + self.inner[position].val.dimension.width)
                                as usize
                            && mouse.y
                                <= (scaled_position_y
                                    + self.inner[position].val.dimension.height)
                                    as usize
                        {
                            select_new_current = Some(position);
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
        self.inner
            .iter()
            .position(|context| context.val.rich_text_id == searched_rich_text_id)
    }

    #[inline]
    pub fn grid_dimension(&self) -> ContextDimension {
        if self.current >= self.inner.len() {
            tracing::error!(
                "Current index {} is out of bounds (len: {})",
                self.current,
                self.inner.len()
            );
            return ContextDimension::default();
        }

        let current_context_dimension = self.inner[self.current].val.dimension;
        ContextDimension::build(
            self.width,
            self.height,
            current_context_dimension.dimension,
            current_context_dimension.line_height,
            self.margin,
        )
    }

    pub fn plot_objects(
        &self,
        objects: &mut Vec<Object>,
        index: usize,
        margin: Delta<f32>,
    ) {
        if let Some(item) = self.inner.get(index) {
            objects.push(Object::RichText(RichText {
                id: item.val.rich_text_id,
                position: [margin.x, margin.top_y],
                lines: None,
            }));

            let scale = self.inner[self.current].val.dimension.dimension.scale;
            let scaled_padding = PADDING * scale;

            let new_margin = Delta {
                x: margin.x,
                top_y: margin.top_y
                    + scaled_padding
                    + (item.val.dimension.height / item.val.dimension.dimension.scale),
                bottom_y: margin.bottom_y,
            };

            objects.push(create_border(
                self.border_color,
                [new_margin.x, new_margin.top_y - scaled_padding],
                [
                    item.val.dimension.width / item.val.dimension.dimension.scale,
                    1.,
                ],
            ));

            if let Some(down_item) = item.down {
                self.plot_objects(objects, down_item, new_margin);
            }

            let new_margin = Delta {
                x: margin.x
                    + scaled_padding
                    + (item.val.dimension.width / item.val.dimension.dimension.scale),
                top_y: margin.top_y,
                bottom_y: margin.bottom_y,
            };

            objects.push(create_border(
                self.border_color,
                [new_margin.x - scaled_padding, new_margin.top_y],
                [
                    1.,
                    item.val.dimension.height / item.val.dimension.dimension.scale,
                ],
            ));

            if let Some(right_item) = item.right {
                self.plot_objects(objects, right_item, new_margin);
            }
        }
    }

    pub fn update_margin(&mut self, padding: (f32, f32, f32)) {
        self.margin = Delta {
            x: padding.0,
            top_y: padding.1,
            bottom_y: padding.2,
        };
        for context in &mut self.inner {
            context.val.dimension.update_margin(self.margin);
        }
    }

    pub fn update_line_height(&mut self, line_height: f32) {
        for context in &mut self.inner {
            context.val.dimension.update_line_height(line_height);
        }
    }

    pub fn update_dimensions(&mut self, sugarloaf: &Sugarloaf) {
        for context in &mut self.inner {
            let layout = sugarloaf.rich_text_layout(&context.val.rich_text_id);
            context.val.dimension.update_dimensions(layout.dimensions);
        }
    }

    pub fn resize(&mut self, new_width: f32, new_height: f32) {
        let width_difference = new_width - self.width;
        let height_difference = new_height - self.height;
        self.width = new_width;
        self.height = new_height;

        let mut vector = vec![(0., 0.); self.inner.len()];
        self.resize_context(&mut vector, 0, width_difference, height_difference);

        for (index, val) in vector.into_iter().enumerate() {
            let context = &mut self.inner[index];
            let current_width = context.val.dimension.width;
            context.val.dimension.update_width(current_width + val.0);

            let current_height = context.val.dimension.height;
            context.val.dimension.update_height(current_height + val.1);

            let mut terminal = context.val.terminal.lock();
            terminal.resize::<ContextDimension>(context.val.dimension);
            drop(terminal);
            let winsize =
                crate::renderer::utils::terminal_dimensions(&context.val.dimension);
            let _ = context.val.messenger.send_resize(winsize);
        }
    }

    // TODO: It works partially, if the panels have different dimensions it gets a bit funky
    fn resize_context(
        &self,
        vector: &mut Vec<(f32, f32)>,
        index: usize,
        available_width: f32,
        available_height: f32,
    ) -> (f32, f32) {
        if let Some(item) = self.inner.get(index) {
            let mut current_available_width = available_width;
            let mut current_available_heigth = available_height;
            if let Some(right_item) = item.right {
                let (new_available_width, _) = self.resize_context(
                    vector,
                    right_item,
                    available_width / 2.,
                    available_height,
                );
                current_available_width = new_available_width;
            }

            if let Some(down_item) = item.down {
                let (_, new_available_heigth) = self.resize_context(
                    vector,
                    down_item,
                    available_width,
                    available_height / 2.,
                );
                current_available_heigth = new_available_heigth;
            }

            vector[index] = (current_available_width, current_available_heigth);

            return (current_available_width, current_available_heigth);
        }

        (available_width, available_height)
    }

    fn request_resize(&mut self, index: usize) {
        let mut terminal = self.inner[index].val.terminal.lock();
        terminal.resize::<ContextDimension>(self.inner[index].val.dimension);
        drop(terminal);
        let winsize =
            crate::renderer::utils::terminal_dimensions(&self.inner[index].val.dimension);
        let _ = self.inner[index].val.messenger.send_resize(winsize);
    }

    pub fn remove_current(&mut self) {
        if self.inner.is_empty() {
            tracing::error!("Attempted to remove from empty grid");
            return;
        }

        if self.current >= self.inner.len() {
            tracing::error!(
                "Current index {} is out of bounds (len: {})",
                self.current,
                self.inner.len()
            );
            self.current = if self.inner.is_empty() {
                0
            } else {
                self.inner.len() - 1
            };
            return;
        }

        // If there's only one context, we can't remove it
        if self.inner.len() == 1 {
            tracing::warn!("Cannot remove the last remaining context");
            return;
        }

        let to_be_removed = self.current;
        let to_be_removed_width =
            self.inner[to_be_removed].val.dimension.width + self.margin.x;
        let to_be_removed_height = self.inner[to_be_removed].val.dimension.height;
        let scaled_padding =
            PADDING * self.inner[to_be_removed].val.dimension.dimension.scale;

        // Find parent context if it exists
        let mut parent_context = None;
        if to_be_removed > 0 {
            for (index, context) in self.inner.iter().enumerate() {
                if let Some(right_val) = context.right {
                    if right_val == self.current {
                        parent_context = Some((true, index));
                        break;
                    }
                }

                if let Some(down_val) = context.down {
                    if down_val == self.current {
                        parent_context = Some((false, index));
                        break;
                    }
                }
            }
        }

        // Handle removal with parent context
        if let Some((is_right, parent_index)) = parent_context {
            self.handle_removal_with_parent(
                to_be_removed,
                parent_index,
                is_right,
                to_be_removed_width,
                to_be_removed_height,
                scaled_padding,
            );
            return;
        }

        // Handle removal without parent (root context)
        self.handle_root_removal(to_be_removed, to_be_removed_height, scaled_padding);
    }

    fn handle_removal_with_parent(
        &mut self,
        to_be_removed: usize,
        parent_index: usize,
        is_right: bool,
        to_be_removed_width: f32,
        to_be_removed_height: f32,
        scaled_padding: f32,
    ) {
        if parent_index >= self.inner.len() {
            tracing::error!("Parent index {} is out of bounds", parent_index);
            return;
        }

        let mut next_current = parent_index;

        if is_right {
            // Handle right child removal
            if let Some(current_down) = self.inner[to_be_removed].down {
                if current_down < self.inner.len() {
                    self.inner[current_down]
                        .val
                        .dimension
                        .increase_height(to_be_removed_height + scaled_padding);

                    let to_be_remove_right = self.inner[to_be_removed].right;
                    self.request_resize(current_down);
                    self.remove_index(to_be_removed);

                    if current_down > 0 {
                        next_current = current_down - 1;
                    } else {
                        next_current = 0;
                    }

                    // Handle right inheritance
                    if let Some(right_val) = to_be_remove_right {
                        if right_val > 0 {
                            self.inherit_right_children(
                                next_current,
                                right_val - 1,
                                to_be_removed_height,
                                scaled_padding,
                            );
                        }
                    }

                    if parent_index < self.inner.len() {
                        self.inner[parent_index].right = Some(next_current);
                    }
                    self.current = next_current;
                    return;
                }
            }

            // No down children, expand parent
            if parent_index < self.inner.len() {
                let parent_width = self.inner[parent_index].val.dimension.width;
                self.inner[parent_index]
                    .val
                    .dimension
                    .update_width(parent_width + to_be_removed_width + scaled_padding);
                self.inner[parent_index].right = self.inner[to_be_removed].right;
                self.request_resize(parent_index);
            }
        } else {
            // Handle down child removal
            if let Some(current_right) = self.inner[to_be_removed].right {
                if current_right < self.inner.len() {
                    self.inner[current_right]
                        .val
                        .dimension
                        .increase_width(to_be_removed_width + scaled_padding);

                    self.request_resize(current_right);

                    if current_right > 0 {
                        next_current = current_right - 1;
                    } else {
                        next_current = 0;
                    }

                    if parent_index < self.inner.len() {
                        self.inner[parent_index].down = Some(next_current);
                    }
                } else {
                    // Invalid right reference, just expand parent
                    if parent_index < self.inner.len() {
                        let parent_height = self.inner[parent_index].val.dimension.height;
                        self.inner[parent_index].val.dimension.update_height(
                            parent_height + to_be_removed_height + scaled_padding,
                        );
                        self.inner[parent_index].down = self.inner[to_be_removed].down;
                        self.request_resize(parent_index);
                    }
                }
            } else {
                // No right children, expand parent
                if parent_index < self.inner.len() {
                    let parent_height = self.inner[parent_index].val.dimension.height;
                    self.inner[parent_index].val.dimension.update_height(
                        parent_height + to_be_removed_height + scaled_padding,
                    );
                    self.inner[parent_index].down = self.inner[to_be_removed].down;
                    self.request_resize(parent_index);
                }
            }
        }

        self.remove_index(to_be_removed);
        self.current = next_current.min(self.inner.len().saturating_sub(1));
    }

    fn handle_root_removal(
        &mut self,
        to_be_removed: usize,
        to_be_removed_height: f32,
        scaled_padding: f32,
    ) {
        // Priority: down items first, then right items
        if let Some(down_val) = self.inner[to_be_removed].down {
            if down_val < self.inner.len() {
                let down_height = self.inner[down_val].val.dimension.height;
                self.inner[down_val]
                    .val
                    .dimension
                    .update_height(down_height + to_be_removed_height + scaled_padding);

                let to_be_removed_right_item = self.inner[to_be_removed].right;

                // Move down item to root position
                self.inner.swap(to_be_removed, down_val);
                self.request_resize(to_be_removed);
                self.remove_index(down_val);

                // Handle right inheritance
                if let Some(right_val) = to_be_removed_right_item {
                    self.inherit_right_children(
                        to_be_removed,
                        right_val,
                        to_be_removed_height,
                        scaled_padding,
                    );
                }

                self.current = to_be_removed;
                return;
            }
        }

        if let Some(right_val) = self.inner[to_be_removed].right {
            if right_val < self.inner.len() {
                let right_width = self.inner[right_val].val.dimension.width;
                let to_be_removed_width =
                    self.inner[to_be_removed].val.dimension.width + self.margin.x;

                self.inner[right_val]
                    .val
                    .dimension
                    .update_width(right_width + to_be_removed_width + scaled_padding);

                // Move right item to root position
                self.inner.swap(to_be_removed, right_val);
                self.request_resize(to_be_removed);
                self.remove_index(right_val);

                self.current = to_be_removed;
                return;
            }
        }

        // Fallback: just remove the item
        self.remove_index(to_be_removed);
        if self.current >= self.inner.len() && !self.inner.is_empty() {
            self.current = self.inner.len() - 1;
        }
    }

    fn inherit_right_children(
        &mut self,
        base_index: usize,
        right_val: usize,
        height_increase: f32,
        scaled_padding: f32,
    ) {
        if base_index >= self.inner.len() || right_val >= self.inner.len() {
            return;
        }

        let mut last_right = None;
        let mut right_ptr = self.inner[base_index].right;

        // Find the last right item and resize all
        while let Some(right_index) = right_ptr {
            if right_index >= self.inner.len() {
                break;
            }

            last_right = Some(right_index);
            let last_right_height = self.inner[right_index].val.dimension.height;
            self.inner[right_index]
                .val
                .dimension
                .update_height(last_right_height + height_increase + scaled_padding);
            self.request_resize(right_index);
            right_ptr = self.inner[right_index].right;
        }

        // Attach the inherited right chain
        if let Some(last_right_val) = last_right {
            if last_right_val < self.inner.len() {
                self.inner[last_right_val].right = Some(right_val);
            }
        } else if base_index < self.inner.len() {
            self.inner[base_index].right = Some(right_val);
        }
    }

    fn remove_index(&mut self, index: usize) {
        if index >= self.inner.len() {
            tracing::error!(
                "Attempted to remove index {} which is out of bounds (len: {})",
                index,
                self.inner.len()
            );
            return;
        }

        // Update all references to indices that will be shifted
        for context in &mut self.inner {
            if let Some(right_val) = context.right {
                if right_val > index {
                    context.right = Some(right_val.saturating_sub(1));
                } else if right_val == index {
                    // The referenced context is being removed
                    context.right = None;
                }
            }

            if let Some(down_val) = context.down {
                if down_val > index {
                    context.down = Some(down_val.saturating_sub(1));
                } else if down_val == index {
                    // The referenced context is being removed
                    context.down = None;
                }
            }
        }

        self.inner.remove(index);

        // Ensure current index is still valid
        if self.current >= self.inner.len() && !self.inner.is_empty() {
            self.current = self.inner.len() - 1;
        }
    }

    pub fn split_right(&mut self, context: Context<T>) {
        let old_right = self.inner[self.current].right;
        // let margin_x = self.margin.x;

        let old_grid_item_height = self.inner[self.current].val.dimension.height;
        let old_grid_item_width =
            self.inner[self.current].val.dimension.width - self.margin.x;
        let new_grid_item_width = old_grid_item_width / 2.0;
        let scale = self.inner[self.current].val.dimension.dimension.scale;
        let scaled_padding = PADDING * scale;

        self.inner[self.current]
            .val
            .dimension
            .update_width(new_grid_item_width - scaled_padding);

        // The current dimension margin should reset
        // otherwise will add a space before the rect
        let mut new_margin = self.inner[self.current].val.dimension.margin;
        new_margin.x = 0.0;
        self.inner[self.current]
            .val
            .dimension
            .update_margin(new_margin);

        self.request_resize(self.current);

        let mut new_context = ContextGridItem::new(context);

        new_context.val.dimension.update_width(new_grid_item_width);
        new_context
            .val
            .dimension
            .update_height(old_grid_item_height);

        self.inner.push(new_context);
        let new_current = self.inner.len() - 1;

        self.inner[new_current].right = old_right;
        self.inner[self.current].right = Some(new_current);
        self.current = new_current;

        // In case the new context does not have right
        // it means it's the last one, for this case
        // whenever a margin exists then we need to add
        // half of margin to respect margin.x border on
        // the right side.
        if self.inner[self.current].right.is_none() {
            new_margin.x = self.margin.x / 2.0;
            self.inner[self.current]
                .val
                .dimension
                .update_margin(new_margin);
        }

        self.request_resize(new_current);
    }

    pub fn split_down(&mut self, context: Context<T>) {
        let old_down = self.inner[self.current].down;

        let old_grid_item_height = self.inner[self.current].val.dimension.height;
        let old_grid_item_width = self.inner[self.current].val.dimension.width;
        let new_grid_item_height = old_grid_item_height / 2.0;
        let scale = self.inner[self.current].val.dimension.dimension.scale;
        let scaled_padding = PADDING * scale;
        self.inner[self.current]
            .val
            .dimension
            .update_height(new_grid_item_height - scaled_padding);

        // The current dimension margin should reset
        // otherwise will add a space before the rect
        let mut new_margin = self.inner[self.current].val.dimension.margin;
        new_margin.bottom_y = 0.0;
        self.inner[self.current]
            .val
            .dimension
            .update_margin(new_margin);

        self.request_resize(self.current);

        let mut new_context = ContextGridItem::new(context);
        new_context
            .val
            .dimension
            .update_height(new_grid_item_height);
        new_context.val.dimension.update_width(old_grid_item_width);

        self.inner.push(new_context);
        let new_current = self.inner.len() - 1;

        self.inner[new_current].down = old_down;
        self.inner[self.current].down = Some(new_current);
        self.current = new_current;

        // TODO: Needs to validate this
        // In case the new context does not have right
        // it means it's the last one, for this case
        // whenever a margin exists then we need to add
        // half of margin to respect margin.top_y and margin.bottom_y
        // borders on the bottom side.
        if self.inner[self.current].down.is_none() {
            new_margin.bottom_y = self.margin.bottom_y;
            self.inner[self.current]
                .val
                .dimension
                .update_margin(new_margin);
        }

        self.request_resize(new_current);
    }

    /// Move divider up - decreases height of current split and increases height of split above
    pub fn move_divider_up(&mut self, amount: f32) -> bool {
        if self.inner.len() <= 1 {
            return false;
        }

        let current_index = self.current;
        if current_index >= self.inner.len() {
            tracing::error!("Current index {} is out of bounds", current_index);
            return false;
        }

        // Strategy: Find any vertically adjacent split and adjust the divider between them
        // Case 1: Current split has a parent above (current is a down child)
        for (index, context) in self.inner.iter().enumerate() {
            if let Some(down_val) = context.down {
                if down_val == current_index {
                    let current_height = self.inner[current_index].val.dimension.height;
                    let parent_height = self.inner[index].val.dimension.height;
                    
                    let min_height = 50.0;
                    if current_height - amount < min_height || parent_height + amount < min_height {
                        return false;
                    }

                    // Shrink current, expand parent (above)
                    self.inner[current_index].val.dimension.update_height(current_height - amount);
                    self.inner[index].val.dimension.update_height(parent_height + amount);

                    self.request_resize(current_index);
                    self.request_resize(index);
                    return true;
                }
            }
        }

        // Case 2: Current split has a down child - move the divider between current and down child
        if let Some(down_child_index) = self.inner[current_index].down {
            if down_child_index < self.inner.len() {
                let current_height = self.inner[current_index].val.dimension.height;
                let down_height = self.inner[down_child_index].val.dimension.height;
                
                let min_height = 50.0;
                if current_height - amount < min_height || down_height + amount < min_height {
                    return false;
                }

                // Shrink current, expand down child
                self.inner[current_index].val.dimension.update_height(current_height - amount);
                self.inner[down_child_index].val.dimension.update_height(down_height + amount);

                self.request_resize(current_index);
                self.request_resize(down_child_index);
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

        let current_index = self.current;
        if current_index >= self.inner.len() {
            tracing::error!("Current index {} is out of bounds", current_index);
            return false;
        }

        // Strategy: Find any vertically adjacent split and adjust the divider between them
        // Case 1: Current split has a parent above (current is a down child)
        for (index, context) in self.inner.iter().enumerate() {
            if let Some(down_val) = context.down {
                if down_val == current_index {
                    let current_height = self.inner[current_index].val.dimension.height;
                    let parent_height = self.inner[index].val.dimension.height;
                    
                    let min_height = 50.0;
                    if current_height + amount < min_height || parent_height - amount < min_height {
                        return false;
                    }

                    // Expand current, shrink parent (above)
                    self.inner[current_index].val.dimension.update_height(current_height + amount);
                    self.inner[index].val.dimension.update_height(parent_height - amount);

                    self.request_resize(current_index);
                    self.request_resize(index);
                    return true;
                }
            }
        }

        // Case 2: Current split has a down child - move the divider between current and down child
        if let Some(down_child_index) = self.inner[current_index].down {
            if down_child_index < self.inner.len() {
                let current_height = self.inner[current_index].val.dimension.height;
                let down_height = self.inner[down_child_index].val.dimension.height;
                
                let min_height = 50.0;
                if current_height + amount < min_height || down_height - amount < min_height {
                    return false;
                }

                // Expand current, shrink down child
                self.inner[current_index].val.dimension.update_height(current_height + amount);
                self.inner[down_child_index].val.dimension.update_height(down_height - amount);

                self.request_resize(current_index);
                self.request_resize(down_child_index);
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

        let current_index = self.current;
        if current_index >= self.inner.len() {
            tracing::error!("Current index {} is out of bounds", current_index);
            return false;
        }

        // Find horizontally adjacent splits
        let mut left_split = None;
        let mut right_split = None;

        // Case 1: Current split is a right child - its parent is to the left
        for (index, context) in self.inner.iter().enumerate() {
            if let Some(right_val) = context.right {
                if right_val == current_index {
                    left_split = Some(index);
                    right_split = Some(current_index);
                    break;
                }
            }
        }

        // Case 2: Current split has a right child - current is left, child is right
        if left_split.is_none() {
            if let Some(right_child_index) = self.inner[current_index].right {
                if right_child_index < self.inner.len() {
                    left_split = Some(current_index);
                    right_split = Some(right_child_index);
                }
            }
        }

        if let (Some(left_idx), Some(right_idx)) = (left_split, right_split) {
            let left_width = self.inner[left_idx].val.dimension.width;
            let right_width = self.inner[right_idx].val.dimension.width;
            
            let min_width = 100.0;
            if left_width - amount < min_width || right_width + amount < min_width {
                return false;
            }

            // Move divider left: shrink left split, expand right split
            self.inner[left_idx].val.dimension.update_width(left_width - amount);
            self.inner[right_idx].val.dimension.update_width(right_width + amount);

            self.request_resize(left_idx);
            self.request_resize(right_idx);
            return true;
        }

        false
    }

    /// Move divider right - expands current split and shrinks the split to the right
    pub fn move_divider_right(&mut self, amount: f32) -> bool {
        if self.inner.len() <= 1 {
            return false;
        }

        let current_index = self.current;
        if current_index >= self.inner.len() {
            tracing::error!("Current index {} is out of bounds", current_index);
            return false;
        }

        // Find horizontally adjacent splits
        let mut left_split = None;
        let mut right_split = None;

        // Case 1: Current split is a right child - its parent is to the left
        for (index, context) in self.inner.iter().enumerate() {
            if let Some(right_val) = context.right {
                if right_val == current_index {
                    left_split = Some(index);
                    right_split = Some(current_index);
                    break;
                }
            }
        }

        // Case 2: Current split has a right child - current is left, child is right
        if left_split.is_none() {
            if let Some(right_child_index) = self.inner[current_index].right {
                if right_child_index < self.inner.len() {
                    left_split = Some(current_index);
                    right_split = Some(right_child_index);
                }
            }
        }

        if let (Some(left_idx), Some(right_idx)) = (left_split, right_split) {
            let left_width = self.inner[left_idx].val.dimension.width;
            let right_width = self.inner[right_idx].val.dimension.width;
            
            let min_width = 100.0;
            if left_width + amount < min_width || right_width - amount < min_width {
                return false;
            }

            // Move divider right: expand left split, shrink right split
            self.inner[left_idx].val.dimension.update_width(left_width + amount);
            self.inner[right_idx].val.dimension.update_width(right_width - amount);

            self.request_resize(left_idx);
            self.request_resize(right_idx);
            return true;
        }

        false
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
        let route_id = 0;
        let context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            route_id,
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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, second_context_id) = {
            let rich_text_id = 1;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, second_context_id) = {
            let rich_text_id = 1;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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

        let contexts = grid.contexts();
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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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
        let contexts = grid.contexts();
        assert_eq!(contexts[0].val.dimension.width, 286.);
        assert_eq!(contexts[0].val.dimension.margin.x, 0.);
        assert_eq!(contexts[1].val.dimension.width, 131.);
        assert_eq!(contexts[1].val.dimension.margin.x, 0.);
        assert_eq!(contexts[2].val.dimension.width, 135.);
        assert_eq!(contexts[2].val.dimension.margin.x, 10.);

        let (fourth_context, fourth_context_id) = {
            let rich_text_id = 3;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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
        let contexts = grid.contexts();
        assert_eq!(contexts[0].val.dimension.width, 286.);
        assert_eq!(contexts[0].val.dimension.margin.x, 0.);
        assert_eq!(contexts[1].val.dimension.width, 51.5);
        assert_eq!(contexts[1].val.dimension.margin.x, 0.);
        assert_eq!(contexts[3].val.dimension.width, 55.5);
        assert_eq!(contexts[3].val.dimension.margin.x, 0.);

        // 2 is the last one
        assert_eq!(contexts[2].val.dimension.width, 135.0);
        assert_eq!(contexts[2].val.dimension.margin.x, 10.);

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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, _second_context_id) = {
            let rich_text_id = 1;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (fourth_context, _fourth_context_id) = {
            let rich_text_id = 3;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (fifth_context, _fifth_context_id) = {
            let rich_text_id = 3;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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
        let contexts = grid.contexts();
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
        let contexts = grid.contexts();
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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, second_context_id) = {
            let rich_text_id = 2;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (third_context, third_context_id) = {
            let rich_text_id = 3;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (fourth_context, fourth_context_id) = {
            let rich_text_id = 4;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (fifth_context, fifth_context_id) = {
            let rich_text_id = 5;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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
        let contexts = grid.contexts();
        assert_eq!(contexts[current_index].val.rich_text_id, second_context_id);
        grid.split_right(fifth_context);

        // If the split right happens in not the last
        // then should not update margin to half of x

        assert_eq!(grid.current_index(), 4);

        // |1.--------------|
        // |2.------|5.-----|
        // |3.------|4.-----|
        let contexts = grid.contexts();
        assert_eq!(contexts.len(), 5);

        assert_eq!(contexts[0].val.rich_text_id, first_context_id);
        assert_eq!(contexts[0].val.dimension.height, 298.);
        assert_eq!(contexts[0].val.dimension.margin.top_y, 0.);
        assert_eq!(contexts[0].val.dimension.margin.bottom_y, 0.);
        let first_down = contexts[0].down;
        assert_eq!(first_down, Some(1));
        assert_eq!(
            contexts[first_down.unwrap_or_default()].val.rich_text_id,
            second_context_id
        );

        assert_eq!(contexts[1].val.rich_text_id, second_context_id);
        assert_eq!(contexts[1].val.dimension.height, 148.);
        assert_eq!(contexts[1].val.dimension.margin.top_y, 0.);
        assert_eq!(contexts[1].val.dimension.margin.bottom_y, 0.);

        assert_eq!(contexts[4].val.rich_text_id, fifth_context_id);
        assert_eq!(contexts[4].val.dimension.height, 148.0);
        assert_eq!(contexts[4].val.dimension.margin.top_y, 0.);
        assert_eq!(contexts[4].val.dimension.margin.bottom_y, 0.);

        assert_eq!(contexts[2].val.rich_text_id, third_context_id);
        assert_eq!(contexts[2].val.dimension.height, 150.0);
        assert_eq!(contexts[2].val.dimension.margin.top_y, 0.);
        assert_eq!(contexts[2].val.dimension.margin.bottom_y, 40.);

        assert_eq!(contexts[3].val.rich_text_id, fourth_context_id);
        assert_eq!(contexts[3].val.dimension.height, 150.0);
        assert_eq!(contexts[3].val.dimension.margin.top_y, 0.);
        assert_eq!(contexts[3].val.dimension.margin.bottom_y, 40.);

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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, second_context_id) = {
            let rich_text_id = 1;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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
        assert_eq!(grid.current_index(), 2);

        let contexts = grid.contexts();

        let scaled_padding = PADDING * contexts[0].val.dimension.dimension.scale;

        // Check their respective width
        assert_eq!(
            contexts[0].val.dimension.width,
            (width / 2.) - scaled_padding
        );
        assert_eq!(contexts[1].val.dimension.width, width);
        assert_eq!(contexts[2].val.dimension.width, width / 2.);

        // Check their respective height
        let top_height = (height / 2.) - scaled_padding;
        assert_eq!(contexts[0].val.dimension.height, top_height);
        assert_eq!(contexts[1].val.dimension.height, height / 2.);
        assert_eq!(contexts[2].val.dimension.height, top_height);

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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, second_context_id) = {
            let rich_text_id = 1;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (fourth_context, fourth_context_id) = {
            let rich_text_id = 3;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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
        let contexts = grid.contexts();
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
        let contexts = grid.contexts();
        assert_eq!(contexts[0].val.dimension.width, 286.);
        assert_eq!(contexts[0].val.dimension.margin.x, 0.);
        assert_eq!(contexts[1].val.dimension.width, 51.5);
        assert_eq!(contexts[1].val.dimension.margin.x, 0.);
        assert_eq!(contexts[3].val.dimension.width, 55.5);
        assert_eq!(contexts[3].val.dimension.margin.x, 0.);

        // 2 is the last one
        assert_eq!(contexts[2].val.dimension.width, 135.0);
        assert_eq!(contexts[2].val.dimension.margin.x, 10.);

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
        let contexts = grid.contexts();
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
        let contexts = grid.contexts();
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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, second_context_id) = {
            let rich_text_id = 1;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, _second_context_id) = {
            let rich_text_id = 1;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (third_context, _third_context_id) = {
            let rich_text_id = 2;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, _second_context_id) = {
            let rich_text_id = 1;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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
        let scaled_padding = PADDING * grid.current().dimension.dimension.scale;
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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, _second_context_id) = {
            let rich_text_id = 1;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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

        let scaled_padding = PADDING * grid.current().dimension.dimension.scale;
        let old_context_expected_width = (600. / 2.) - scaled_padding;
        assert_eq!(grid.current().dimension.width, old_context_expected_width);
        assert_eq!(grid.current_index(), 0);

        let current_index = grid.current_index();
        assert_eq!(grid.contexts()[current_index].right, Some(1));
        assert_eq!(grid.contexts()[current_index].down, None);

        grid.remove_current();

        assert_eq!(grid.current_index(), 0);
        // Whenever return to one should drop padding
        let expected_width = 600.;
        assert_eq!(grid.current().dimension.width, expected_width);

        let current_index = grid.current_index();
        assert_eq!(grid.contexts()[current_index].right, None);
        assert_eq!(grid.contexts()[current_index].down, None);
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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, second_context_id) = {
            let rich_text_id = 1;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, _second_context_id) = {
            let rich_text_id = 1;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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
        let scaled_padding = PADDING * grid.current().dimension.dimension.scale;
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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, _second_context_id) = {
            let rich_text_id = 1;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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

        let scaled_padding = PADDING * grid.current().dimension.dimension.scale;
        let old_context_expected_height = (600. / 2.) - scaled_padding;
        assert_eq!(grid.current().dimension.height, old_context_expected_height);
        assert_eq!(grid.current_index(), 0);

        let current_index = grid.current_index();
        assert_eq!(grid.contexts()[current_index].down, Some(1));
        assert_eq!(grid.contexts()[current_index].right, None);

        grid.remove_current();

        assert_eq!(grid.current_index(), 0);
        // Whenever return to one should drop padding
        let expected_height = 600.;
        assert_eq!(grid.current().dimension.height, expected_height);

        let current_index = grid.current_index();
        assert_eq!(grid.contexts()[current_index].down, None);
        assert_eq!(grid.contexts()[current_index].right, None);
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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, second_context_id) = {
            let rich_text_id = 1;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, second_context_id) = {
            let rich_text_id = 2;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (third_context, third_context_id) = {
            let rich_text_id = 3;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (fourth_context, fourth_context_id) = {
            let rich_text_id = 4;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (fifth_context, fifth_context_id) = {
            let rich_text_id = 5;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (sixth_context, sixth_context_id) = {
            let rich_text_id = 6;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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
        assert_eq!(current_index, 3);
        assert_eq!(grid.contexts()[current_index].down, None);

        // So far we have:
        //
        // |1.-----|.3-----|4.-----|
        // |2.-----|-------|-------|

        grid.select_prev_split();
        assert_eq!(grid.current().rich_text_id, third_context_id);
        let current_index = grid.current_index();
        assert_eq!(current_index, 2);
        assert_eq!(grid.contexts()[current_index].down, None);

        grid.split_down(fifth_context);
        assert_eq!(grid.current().rich_text_id, fifth_context_id);

        grid.split_right(sixth_context);
        assert_eq!(grid.current().rich_text_id, sixth_context_id);

        grid.select_prev_split();
        grid.select_prev_split();
        grid.select_prev_split();

        assert_eq!(grid.current().rich_text_id, third_context_id);

        let current_index = grid.current_index();
        let right = grid.contexts()[current_index].right;
        assert_eq!(
            grid.contexts()[right.unwrap_or_default()].val.rich_text_id,
            fourth_context_id
        );
        let current_index = grid.current_index();
        let down = grid.contexts()[current_index].down;
        assert_eq!(
            grid.contexts()[down.unwrap_or_default()].val.rich_text_id,
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
        let right = grid.contexts()[current_index].right;
        assert_eq!(
            grid.contexts()[right.unwrap_or_default()].val.rich_text_id,
            sixth_context_id
        );

        // Let's go back to 1 to check if leads to 5
        grid.select_prev_split();
        grid.select_prev_split();
        grid.select_prev_split();

        assert_eq!(grid.current().rich_text_id, first_context_id);
        let current_index = grid.current_index();
        assert_eq!(current_index, 0);
        let right = grid.contexts()[current_index].right;
        assert_eq!(right, Some(3));
        assert_eq!(
            grid.contexts()[right.unwrap_or_default()].val.rich_text_id,
            fifth_context_id
        );

        // Let's go to 6 to check if leads to 4
        //
        // |1.-----|.5-|6.-|4.-----|
        // |2.-----|---|---|-------|

        grid.select_next_split();
        grid.select_next_split();
        grid.select_next_split();
        grid.select_next_split();

        assert_eq!(grid.current().rich_text_id, sixth_context_id);
        let current_index = grid.current_index();
        let right = grid.contexts()[current_index].right;
        assert_eq!(right, Some(2));
        assert_eq!(
            grid.contexts()[right.unwrap_or_default()].val.rich_text_id,
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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, second_context_id) = {
            let rich_text_id = 2;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (third_context, third_context_id) = {
            let rich_text_id = 3;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (fourth_context, fourth_context_id) = {
            let rich_text_id = 4;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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

        assert_eq!(grid.current_index(), 3);
        assert_eq!(grid.current().rich_text_id, fourth_context_id);

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
        let right = grid.contexts()[current_index].right;
        let right_context = grid.contexts()[right.unwrap_or_default()].val.rich_text_id;
        assert_eq!(right_context, second_context_id);

        // Result:
        // |3.----|.2----|
        // |------|.4----|

        // Now let's create a more complex case
        // |3.---------|.2---------|
        // |5.-|6.-|7.-|.4---------|

        let (fifth_context, fifth_context_id) = {
            let rich_text_id = 5;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (sixth_context, sixth_context_id) = {
            let rich_text_id = 6;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (seventh_context, seventh_context_id) = {
            let rich_text_id = 7;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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
        let down = grid.contexts()[current_index].down;
        assert_eq!(
            grid.contexts()[down.unwrap_or_default()].val.rich_text_id,
            fifth_context_id
        );

        grid.remove_current();
        assert_eq!(grid.current().rich_text_id, fifth_context_id);

        let current_index = grid.current_index();
        let right = grid.contexts()[current_index].right.unwrap_or_default();
        let right_context = &grid.contexts()[right];
        assert_eq!(right_context.val.rich_text_id, sixth_context_id);

        // Current:
        // |5.-|6.-|7.-|.2---------|
        // |---|---|---|.4---------|

        // Ok, let's test the reverse to right operations
        // First remove 5 and 6

        grid.remove_current();
        assert_eq!(grid.current().rich_text_id, sixth_context_id);
        let current_index = grid.current_index();
        assert_eq!(grid.contexts()[current_index].down, None);
        let right = grid.contexts()[current_index].right;
        assert_eq!(
            grid.contexts()[right.unwrap_or_default()].val.rich_text_id,
            seventh_context_id
        );

        grid.remove_current();
        assert_eq!(grid.current().rich_text_id, seventh_context_id);
        let right = grid.contexts()[current_index].right;
        assert_eq!(
            grid.contexts()[right.unwrap_or_default()].val.rich_text_id,
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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (sixth_context, sixth_context_id) = {
            let rich_text_id = 6;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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
        let right = grid.contexts()[current_index].right;
        let down = grid.contexts()[current_index].down;
        assert_eq!(
            grid.contexts()[right.unwrap_or_default()].val.rich_text_id,
            second_context_id
        );
        assert_eq!(
            grid.contexts()[down.unwrap_or_default()].val.rich_text_id,
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
        let right = grid.contexts()[current_index].right;
        assert_eq!(
            grid.contexts()[right.unwrap_or_default()].val.rich_text_id,
            second_context_id
        );
        assert_eq!(grid.contexts()[current_index].down, None);
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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
                    rich_text_id,
                    context_dimension,
                ),
                rich_text_id,
            )
        };

        let (second_context, second_context_id) = {
            let rich_text_id = 1;
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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
            let route_id = 0;
            (
                create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    route_id,
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

        let scaled_padding = PADDING * grid.current().dimension.dimension.scale;
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
        let context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            0,
            0,
            context_dimension,
        );

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
    fn test_edge_case_out_of_bounds_current() {
        let margin = Delta::default();
        let context_dimension = ContextDimension::default();
        let context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            0,
            0,
            context_dimension,
        );

        let mut grid =
            ContextGrid::<VoidListener>::new(context, margin, [0., 0., 0., 0.]);

        // Manually set current to out of bounds
        grid.current = 999;

        // These should not panic and should handle the error gracefully
        let _ = grid.current();
        let _ = grid.current_mut();
        let _ = grid.grid_dimension();
        let _ = grid.current_context_with_computed_dimension();
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
            create_mock_context(
                VoidListener {},
                WindowId::from(0),
                0,
                0,
                context_dimension,
            ),
            margin,
            [0., 0., 0., 0.],
        );

        // Add multiple splits to create a complex structure
        for i in 1..=5 {
            let context = create_mock_context(
                VoidListener {},
                WindowId::from(0),
                i,
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
            assert!(grid.current < grid.len());
        }

        // Should have exactly one context left
        assert_eq!(grid.len(), 1);
    }

    #[test]
    fn test_edge_case_rapid_split_and_remove() {
        let margin = Delta::default();
        let context_dimension = ContextDimension::build(
            800.0,
            600.0,
            SugarDimensions {
                scale: 1.0,
                width: 12.0,
                height: 12.0,
            },
            1.0,
            Delta::default(),
        );

        let mut grid = ContextGrid::<VoidListener>::new(
            create_mock_context(
                VoidListener {},
                WindowId::from(0),
                0,
                0,
                context_dimension,
            ),
            margin,
            [0., 0., 0., 0.],
        );

        // Rapidly add and remove contexts
        for iteration in 0..10 {
            // Add some contexts
            for i in 0..3 {
                let context = create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    iteration * 10 + i,
                    iteration * 10 + i,
                    context_dimension,
                );
                if i % 2 == 0 {
                    grid.split_right(context);
                } else {
                    grid.split_down(context);
                }
            }

            // Remove some contexts
            while grid.len() > 2 {
                grid.remove_current();
            }

            // Verify grid is still in a valid state
            assert!(grid.len() >= 1);
            assert!(grid.current < grid.len());
        }
    }

    #[test]
    fn test_edge_case_dimension_updates_with_invalid_data() {
        let margin = Delta::default();
        let mut context_dimension = ContextDimension::default();

        // Test with invalid dimensions
        context_dimension.width = -100.0;
        context_dimension.height = -100.0;

        let context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            0,
            0,
            context_dimension,
        );

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
            create_mock_context(
                VoidListener {},
                WindowId::from(0),
                0,
                0,
                context_dimension,
            ),
            margin,
            [0., 0., 0., 0.],
        );

        // Add a split
        let context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            1,
            1,
            context_dimension,
        );
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
        let context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            0,
            0,
            context_dimension,
        );

        let mut grid =
            ContextGrid::<VoidListener>::new(context, margin, [0., 0., 0., 0.]);

        // Test navigation with single context
        grid.select_next_split();
        assert_eq!(grid.current, 0);

        grid.select_prev_split();
        assert_eq!(grid.current, 0);

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
            create_mock_context(
                VoidListener {},
                WindowId::from(0),
                0,
                0,
                context_dimension,
            ),
            margin,
            [0., 0., 0., 0.],
        );

        // Create many splits
        for i in 1..=20 {
            let context = create_mock_context(
                VoidListener {},
                WindowId::from(0),
                i,
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
            assert!(grid.current < grid.len());
        }

        // Test navigation through all splits
        let initial_current = grid.current;
        for _ in 0..grid.len() * 2 {
            grid.select_next_split();
            assert!(grid.current < grid.len());
        }

        // Should cycle back
        assert_eq!(grid.current, initial_current);

        // Remove all but one
        while grid.len() > 1 {
            let len_before = grid.len();
            grid.remove_current();
            assert!(grid.len() < len_before);
            assert!(grid.current < grid.len());
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
            create_mock_context(
                VoidListener {},
                WindowId::from(0),
                0,
                0,
                context_dimension,
            ),
            margin,
            [0., 0., 0., 0.],
        );

        // Add a split
        let context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            1,
            1,
            context_dimension,
        );
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
    fn test_edge_case_corrupted_internal_state() {
        let margin = Delta::default();
        let context_dimension = ContextDimension::default();
        let context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            0,
            0,
            context_dimension,
        );

        let mut grid =
            ContextGrid::<VoidListener>::new(context, margin, [0., 0., 0., 0.]);

        // Add some contexts
        for i in 1..=3 {
            let context = create_mock_context(
                VoidListener {},
                WindowId::from(0),
                i,
                i,
                context_dimension,
            );
            grid.split_right(context);
        }

        // Manually corrupt the internal state to test robustness
        if grid.inner.len() > 1 {
            // Set invalid right/down references
            grid.inner[0].right = Some(999);
            grid.inner[1].down = Some(888);
        }

        // Operations should handle corrupted state gracefully
        grid.remove_current();
        assert!(grid.len() > 0);
        assert!(grid.current < grid.len());
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
    fn test_concurrent_operations_simulation() {
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
            create_mock_context(
                VoidListener {},
                WindowId::from(0),
                0,
                0,
                context_dimension,
            ),
            margin,
            [0., 0., 0., 0.],
        );

        // Simulate concurrent operations that might happen in real usage
        for round in 0..5 {
            // Add contexts
            for i in 0..3 {
                let context = create_mock_context(
                    VoidListener {},
                    WindowId::from(0),
                    round * 10 + i,
                    round * 10 + i,
                    context_dimension,
                );
                if i % 2 == 0 {
                    grid.split_right(context);
                } else {
                    grid.split_down(context);
                }
            }

            // Navigate
            for _ in 0..grid.len() {
                grid.select_next_split();
            }

            // Resize
            grid.resize(800.0 + round as f32 * 100.0, 600.0 + round as f32 * 50.0);

            // Mouse selection
            let mut mouse = Mouse::default();
            mouse.x = (round * 100) % 400;
            mouse.y = (round * 80) % 300;
            grid.select_current_based_on_mouse(&mouse);

            // Remove some contexts
            while grid.len() > 2 {
                grid.remove_current();
            }

            // Verify state consistency
            assert!(grid.len() >= 1);
            assert!(grid.current < grid.len());
        }
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
            create_mock_context(
                VoidListener {},
                WindowId::from(0),
                0,
                0,
                context_dimension,
            ),
            margin,
            [0., 0., 0., 0.],
        );

        // Single split - should return false
        assert!(!grid.move_divider_up(20.0));

        // Add a split down
        let second_context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            1,
            1,
            context_dimension,
        );
        grid.split_down(second_context);

        // Now we should be able to move divider up
        let original_current_height = grid.inner[grid.current].val.dimension.height;
        let original_parent_height = grid.inner[0].val.dimension.height;

        assert!(grid.move_divider_up(20.0));

        // Current split should be smaller, parent should be larger
        assert!(grid.inner[grid.current].val.dimension.height < original_current_height);
        assert!(grid.inner[0].val.dimension.height > original_parent_height);
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
            create_mock_context(
                VoidListener {},
                WindowId::from(0),
                0,
                0,
                context_dimension,
            ),
            margin,
            [0., 0., 0., 0.],
        );

        // Add a split down
        let second_context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            1,
            1,
            context_dimension,
        );
        grid.split_down(second_context);

        let original_current_height = grid.inner[grid.current].val.dimension.height;
        let original_parent_height = grid.inner[0].val.dimension.height;

        assert!(grid.move_divider_down(20.0));

        // Current split should be larger, parent should be smaller
        assert!(grid.inner[grid.current].val.dimension.height > original_current_height);
        assert!(grid.inner[0].val.dimension.height < original_parent_height);
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
            create_mock_context(
                VoidListener {},
                WindowId::from(0),
                0,
                0,
                context_dimension,
            ),
            margin,
            [0., 0., 0., 0.],
        );

        // Single split - should return false
        assert!(!grid.move_divider_left(40.0));

        // Add a split right
        let second_context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            1,
            1,
            context_dimension,
        );
        grid.split_right(second_context);

        // Test from the right split (index 1) - moving left should shrink left panel, expand right panel
        let original_left_width = grid.inner[0].val.dimension.width;
        let original_right_width = grid.inner[1].val.dimension.width;

        assert!(grid.move_divider_left(40.0));

        // Left split should be smaller, right split should be larger
        assert!(grid.inner[0].val.dimension.width < original_left_width);
        assert!(grid.inner[1].val.dimension.width > original_right_width);

        // Test from the left split (index 0) - should have same effect
        grid.current = 0;
        let original_left_width2 = grid.inner[0].val.dimension.width;
        let original_right_width2 = grid.inner[1].val.dimension.width;

        assert!(grid.move_divider_left(20.0));

        // Left split should be smaller, right split should be larger
        assert!(grid.inner[0].val.dimension.width < original_left_width2);
        assert!(grid.inner[1].val.dimension.width > original_right_width2);
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
            create_mock_context(
                VoidListener {},
                WindowId::from(0),
                0,
                0,
                context_dimension,
            ),
            margin,
            [0., 0., 0., 0.],
        );

        // Add a split right
        let second_context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            1,
            1,
            context_dimension,
        );
        grid.split_right(second_context);

        // Test from the right split (index 1) - moving right should expand left panel, shrink right panel
        let original_left_width = grid.inner[0].val.dimension.width;
        let original_right_width = grid.inner[1].val.dimension.width;

        assert!(grid.move_divider_right(40.0));

        // Left split should be larger, right split should be smaller
        assert!(grid.inner[0].val.dimension.width > original_left_width);
        assert!(grid.inner[1].val.dimension.width < original_right_width);

        // Test from the left split (index 0) - should have same effect
        grid.current = 0;
        let original_left_width2 = grid.inner[0].val.dimension.width;
        let original_right_width2 = grid.inner[1].val.dimension.width;

        assert!(grid.move_divider_right(20.0));

        // Left split should be larger, right split should be smaller
        assert!(grid.inner[0].val.dimension.width > original_left_width2);
        assert!(grid.inner[1].val.dimension.width < original_right_width2);
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
            create_mock_context(
                VoidListener {},
                WindowId::from(0),
                0,
                0,
                context_dimension,
            ),
            margin,
            [0., 0., 0., 0.],
        );

        // Add splits
        let second_context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            1,
            1,
            context_dimension,
        );
        grid.split_right(second_context);

        let third_context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            2,
            2,
            context_dimension,
        );
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
            create_mock_context(
                VoidListener {},
                WindowId::from(0),
                0,
                0,
                context_dimension,
            ),
            margin,
            [0., 0., 0., 0.],
        );

        // Create a complex layout: split right, then split down on the right side
        let second_context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            1,
            1,
            context_dimension,
        );
        grid.split_right(second_context);

        let third_context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            2,
            2,
            context_dimension,
        );
        grid.split_down(third_context);

        // Test moving dividers in different splits
        assert!(grid.move_divider_up(30.0));
        assert!(grid.move_divider_down(15.0));

        // Switch to first split (index 0) and test horizontal movement
        grid.current = 0;
        assert!(grid.move_divider_right(50.0));
        assert!(grid.move_divider_left(25.0));

        // Verify grid is still in valid state
        assert!(grid.len() == 3);
        assert!(grid.current < grid.len());
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
            create_mock_context(
                VoidListener {},
                WindowId::from(0),
                0,
                0,
                context_dimension,
            ),
            margin,
            [0., 0., 0., 0.],
        );

        // Test with zero amount
        let second_context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            1,
            1,
            context_dimension,
        );
        grid.split_right(second_context);

        let original_width = grid.inner[grid.current].val.dimension.width;
        assert!(grid.move_divider_left(0.0));
        // Width should remain the same with zero movement
        assert_eq!(grid.inner[grid.current].val.dimension.width, original_width);

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
            create_mock_context(
                VoidListener {},
                WindowId::from(0),
                0,
                0,
                context_dimension,
            ),
            margin,
            [0., 0., 0., 0.],
        );

        // With only one split, no divider movement should work
        assert!(!grid.move_divider_up(20.0));
        assert!(!grid.move_divider_down(20.0));
        assert!(!grid.move_divider_left(40.0));
        assert!(!grid.move_divider_right(40.0));

        // Add only a vertical split (down)
        let second_context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            1,
            1,
            context_dimension,
        );
        grid.split_down(second_context);

        // Select the top split (index 0) - should not be able to move horizontal dividers
        // but should be able to move vertical dividers (since it has a down child)
        grid.current = 0;
        assert!(!grid.move_divider_left(40.0));
        assert!(!grid.move_divider_right(40.0));
        assert!(grid.move_divider_up(20.0)); // Can move up by shrinking itself and expanding down child
        assert!(grid.move_divider_down(20.0)); // Can move down by expanding itself and shrinking down child
        
        // The bottom split (index 1) should be able to move up (has parent above)
        grid.current = 1;
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
            create_mock_context(
                VoidListener {},
                WindowId::from(0),
                0,
                0,
                context_dimension,
            ),
            margin,
            [0., 0., 0., 0.],
        );

        // Create multiple splits
        for i in 1..6 {
            let context = create_mock_context(
                VoidListener {},
                WindowId::from(0),
                i,
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
            assert!(grid.current < grid.len());
            
            // Verify all dimensions are positive
            for item in &grid.inner {
                assert!(item.val.dimension.width > 0.0);
                assert!(item.val.dimension.height > 0.0);
            }
        }
    }

    #[test]
    fn test_move_divider_with_invalid_current_index() {
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
            create_mock_context(
                VoidListener {},
                WindowId::from(0),
                0,
                0,
                context_dimension,
            ),
            margin,
            [0., 0., 0., 0.],
        );

        // Manually set invalid current index
        grid.current = 999;

        // All divider movements should fail gracefully
        assert!(!grid.move_divider_up(20.0));
        assert!(!grid.move_divider_down(20.0));
        assert!(!grid.move_divider_left(40.0));
        assert!(!grid.move_divider_right(40.0));
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
            create_mock_context(
                VoidListener {},
                WindowId::from(0),
                0,
                0,
                context_dimension,
            ),
            margin,
            [0., 0., 0., 0.],
        );

        // Add a horizontal split
        let second_context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            1,
            1,
            context_dimension,
        );
        grid.split_right(second_context);

        let original_total_width = grid.inner[0].val.dimension.width + grid.inner[1].val.dimension.width;

        // Move divider and check total space is preserved (approximately)
        assert!(grid.move_divider_left(50.0));
        
        let new_total_width = grid.inner[0].val.dimension.width + grid.inner[1].val.dimension.width;
        
        // Total width should be approximately the same (allowing for small floating point differences)
        let difference = (original_total_width - new_total_width).abs();
        assert!(difference < 1.0, "Total width changed by more than 1.0: {} vs {}", original_total_width, new_total_width);

        // Test with vertical split
        let third_context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            2,
            2,
            context_dimension,
        );
        grid.split_down(third_context);

        let parent_index = grid.inner.iter().position(|item| {
            item.down.is_some() && item.down.unwrap() == grid.current
        }).unwrap();

        let original_total_height = grid.inner[parent_index].val.dimension.height + grid.inner[grid.current].val.dimension.height;

        assert!(grid.move_divider_up(30.0));
        
        let new_total_height = grid.inner[parent_index].val.dimension.height + grid.inner[grid.current].val.dimension.height;
        
        let height_difference = (original_total_height - new_total_height).abs();
        assert!(height_difference < 1.0, "Total height changed by more than 1.0: {} vs {}", original_total_height, new_total_height);
    }
}
