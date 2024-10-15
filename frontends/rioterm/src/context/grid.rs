use crate::context::Context;
use rio_backend::crosswords::grid::Dimensions;
use rio_backend::event::EventListener;
use rio_backend::sugarloaf::{
    layout::SugarDimensions, Object, Rect, RichText, Sugarloaf,
};

const MIN_COLS: usize = 2;
const MIN_LINES: usize = 1;

const PADDING: f32 = 2.;

// $ tput columns
// $ tput lines
#[inline]
fn compute(
    width: f32,
    height: f32,
    dimensions: SugarDimensions,
    line_height: f32,
    margin: Delta<f32>,
) -> (usize, usize) {
    let margin_x = ((margin.x) * dimensions.scale).floor();
    let margin_spaces = margin.top_y + margin.bottom_y;

    let mut lines = (height / dimensions.scale) - margin_spaces;
    lines /= (dimensions.height / dimensions.scale) * line_height;
    let visible_lines = std::cmp::max(lines.floor() as usize, MIN_LINES);

    let mut visible_columns = (width / dimensions.scale) - margin_x;
    visible_columns /= dimensions.width / dimensions.scale;
    let visible_columns = std::cmp::max(visible_columns as usize, MIN_COLS);

    (visible_columns, visible_lines)
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
    pub fn current(&self) -> &Context<T> {
        &self.inner[self.current].val
    }

    #[inline]
    pub fn current_mut(&mut self) -> &mut Context<T> {
        &mut self.inner[self.current].val
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
                }));
            }
        } else {
            self.plot_objects(&mut objects, 0, self.margin);
        }
        objects
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
            }));

            if let Some(right_item) = item.right {
                let new_margin = Delta {
                    x: margin.x
                        + PADDING
                        + (item.val.dimension.width / item.val.dimension.dimension.scale),
                    top_y: margin.top_y,
                    bottom_y: margin.bottom_y,
                };

                objects.push(Object::Rect(Rect {
                    position: [new_margin.x - PADDING, new_margin.top_y],
                    color: self.border_color,
                    size: [
                        2. / item.val.dimension.dimension.scale,
                        item.val.dimension.height,
                    ],
                }));

                self.plot_objects(objects, right_item, new_margin);
            }

            if let Some(down_item) = item.down {
                let new_margin = Delta {
                    x: margin.x,
                    top_y: margin.top_y
                        + PADDING
                        + (item.val.dimension.height
                            / item.val.dimension.dimension.scale),
                    bottom_y: margin.bottom_y,
                };

                objects.push(Object::Rect(Rect {
                    position: [new_margin.x, new_margin.top_y - PADDING],
                    color: self.border_color,
                    size: [
                        item.val.dimension.width,
                        2. / item.val.dimension.dimension.scale,
                    ],
                }));

                self.plot_objects(objects, down_item, new_margin);
            }
        }
    }

    pub fn update_margin(&mut self, padding: (f32, f32, f32)) {
        self.margin = Delta {
            x: padding.0,
            top_y: padding.1,
            bottom_y: padding.2,
        };
    }

    pub fn rescale(&mut self, sugarloaf: &Sugarloaf) {
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

    pub fn remove_current(&mut self) {
        // TODO: Adjust width and height of pointing contexts
        // TODO: Adjust right and down of pointing contexts

        // let mut index = 0;
        // for context in &self.inner {
        //     if let Some(_right_val) = context.right {

        //     }

        //     if let Some(_down_val) = context.down {

        //     }

        //     index += 1;
        // }

        let old = self.current;
        self.select_prev_split();
        self.inner.remove(old);
    }

    pub fn split_right(&mut self, context: Context<T>) {
        let old_right = self.inner[self.current].right;

        let old_grid_item_width = self.inner[self.current].val.dimension.width;
        let new_grid_item_width = (old_grid_item_width / 2.0) - PADDING;
        self.inner[self.current]
            .val
            .dimension
            .update_width(new_grid_item_width - PADDING);

        let mut terminal = self.inner[self.current].val.terminal.lock();
        terminal.resize::<ContextDimension>(self.inner[self.current].val.dimension);
        drop(terminal);
        let winsize = crate::renderer::utils::terminal_dimensions(
            &self.inner[self.current].val.dimension,
        );
        let _ = self.inner[self.current].val.messenger.send_resize(winsize);

        let mut new_context = ContextGridItem::new(context);

        new_context
            .val
            .dimension
            .update_width(new_grid_item_width - PADDING);

        self.inner.push(new_context);
        let new_current = self.inner.len() - 1;

        let mut terminal = self.inner[new_current].val.terminal.lock();
        terminal.resize::<ContextDimension>(self.inner[new_current].val.dimension);
        drop(terminal);
        let winsize = crate::renderer::utils::terminal_dimensions(
            &self.inner[new_current].val.dimension,
        );
        let _ = self.inner[new_current].val.messenger.send_resize(winsize);

        self.inner[new_current].right = old_right;
        self.inner[self.current].right = Some(new_current);
        self.current = new_current;
    }

    pub fn split_down(&mut self, context: Context<T>) {
        let old_down = self.inner[self.current].down;

        let old_grid_item_height = self.inner[self.current].val.dimension.height;
        let new_grid_item_height = (old_grid_item_height / 2.0) - PADDING;
        self.inner[self.current]
            .val
            .dimension
            .update_height(new_grid_item_height - (PADDING * 2.0));

        let mut terminal = self.inner[self.current].val.terminal.lock();
        terminal.resize::<ContextDimension>(self.inner[self.current].val.dimension);
        drop(terminal);
        let winsize = crate::renderer::utils::terminal_dimensions(
            &self.inner[self.current].val.dimension,
        );
        let _ = self.inner[self.current].val.messenger.send_resize(winsize);

        let mut new_context = ContextGridItem::new(context);

        new_context
            .val
            .dimension
            .update_height(new_grid_item_height - (PADDING * 2.0));

        self.inner.push(new_context);
        let new_current = self.inner.len() - 1;

        let mut terminal = self.inner[new_current].val.terminal.lock();
        terminal.resize::<ContextDimension>(self.inner[new_current].val.dimension);
        drop(terminal);
        let winsize = crate::renderer::utils::terminal_dimensions(
            &self.inner[new_current].val.dimension,
        );
        let _ = self.inner[new_current].val.messenger.send_resize(winsize);

        self.inner[new_current].down = old_down;
        self.inner[self.current].down = Some(new_current);
        self.current = new_current;
    }
}

#[derive(Default, Copy, Clone)]
pub struct ContextDimension {
    pub width: f32,
    pub height: f32,
    pub columns: usize,
    pub lines: usize,
    pub dimension: SugarDimensions,
    pub margin: Delta<f32>,
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
        }
    }

    pub fn update_width(&mut self, width: f32) {
        self.width = width;
        self.update();
    }

    pub fn update_height(&mut self, height: f32) {
        self.height = height;
        self.update();
    }

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
            // self.line_height,
            1.0,
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
    use crate::context::create_mock_context;
    use crate::event::VoidListener;
    use rio_window::window::WindowId;

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
        let rich_text_id = 1;
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
            })]
        );
    }

    #[test]
    fn test_split_right() {
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
                width: 14.,
                height: 8.,
            },
            1.0,
            Delta::<f32>::default(),
        );

        assert_eq!(context_dimension.columns, 85);
        assert_eq!(context_dimension.lines, 100);

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
                position: [10., 20.],
            })]
        );
        grid.split_right(second_context);

        assert_eq!(
            grid.objects(),
            vec![
                Object::RichText(RichText {
                    id: first_context_id,
                    position: [10.0, 20.0],
                }),
                Object::Rect(Rect {
                    position: [307.0, 20.0],
                    color: [1.0, 0.0, 0.0, 0.0],
                    size: [1.0, 380.0]
                }),
                Object::RichText(RichText {
                    id: second_context_id,
                    position: [309.0, 20.0]
                }),
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
                    position: [10.0, 20.0],
                }),
                Object::Rect(Rect {
                    position: [307.0, 20.0],
                    color: [1.0, 0.0, 0.0, 0.0],
                    size: [1.0, 380.0]
                }),
                Object::RichText(RichText {
                    id: second_context_id,
                    position: [309.0, 20.0]
                }),
                Object::Rect(Rect {
                    position: [10.0, 20.0],
                    color: [1.0, 0.0, 0.0, 0.0],
                    size: [1.0, 380.0]
                }),
                Object::RichText(RichText {
                    id: third_context_id,
                    position: [456.5, 20.0]
                }),
            ]
        );
    }

    #[test]
    fn test_split_down() {
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
                width: 14.,
                height: 8.,
            },
            1.0,
            Delta::<f32>::default(),
        );

        assert_eq!(context_dimension.columns, 85);
        assert_eq!(context_dimension.lines, 100);

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
                position: [10., 20.],
            })]
        );
        grid.split_down(second_context);

        assert_eq!(
            grid.objects(),
            vec![
                Object::RichText(RichText {
                    id: first_context_id,
                    position: [10.0, 20.0],
                }),
                Object::Rect(Rect {
                    position: [10.0, 217.0],
                    color: [0.0, 0.0, 1.0, 0.0],
                    size: [1200.0, 1.0]
                }),
                Object::RichText(RichText {
                    id: second_context_id,
                    position: [10.0, 219.0]
                }),
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
                    position: [10.0, 20.0],
                }),
                Object::Rect(Rect {
                    position: [10.0, 217.0],
                    color: [0.0, 0.0, 1.0, 0.0],
                    size: [1200.0, 1.0]
                }),
                Object::RichText(RichText {
                    id: second_context_id,
                    position: [10.0, 219.0]
                }),
                Object::Rect(Rect {
                    position: [10.0, 314.5],
                    color: [0.0, 0.0, 1.0, 0.0],
                    size: [1200.0, 1.0]
                }),
                Object::RichText(RichText {
                    id: third_context_id,
                    position: [10.0, 316.5]
                }),
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
        assert_eq!(context_dimension.lines, 75);

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

        let mut grid =
            ContextGrid::<VoidListener>::new(first_context, margin, [0., 0., 0., 0.]);

        assert_eq!(
            grid.objects(),
            vec![Object::RichText(RichText {
                id: first_context_id,
                position: [0., 0.],
            })]
        );

        grid.split_right(second_context);
        grid.split_down(third_context);

        assert_eq!(
            grid.objects(),
            vec![
                Object::RichText(RichText {
                    id: first_context_id,
                    position: [0.0, 0.0],
                }),
                Object::Rect(Rect {
                    position: [147.0, 0.0],
                    color: [0.0, 0.0, 0.0, 0.0],
                    size: [1.0, 300.0]
                }),
                Object::RichText(RichText {
                    id: second_context_id,
                    position: [149.0, 0.0]
                }),
                Object::Rect(Rect {
                    position: [149.0, 147.0],
                    color: [0.0, 0.0, 0.0, 0.0],
                    size: [294.0, 1.0]
                }),
                Object::RichText(RichText {
                    id: third_context_id,
                    position: [149.0, 149.0]
                }),
            ]
        );

        assert_eq!(grid.width, 600.0);
        assert_eq!(grid.height, 600.0);

        grid.resize(1200.0, 600.0);
    }
}
