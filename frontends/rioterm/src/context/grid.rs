use crate::context::Context;
use rio_backend::crosswords::grid::Dimensions;
use rio_backend::event::EventListener;
use rio_backend::sugarloaf::{
    layout::SugarDimensions, ComposedQuad, Object, Quad, RichText,
};

const MIN_COLS: usize = 2;
const MIN_LINES: usize = 1;

const PADDING: f32 = 4.;

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
    active_border_color: [f32; 4],
    inner: Vec<ContextGridItem<T>>,
}

pub struct ContextGridItem<T: EventListener> {
    val: Context<T>,
    pub width: f32,
    pub height: f32,
    right: Option<usize>,
    down: Option<usize>,
}

impl<T: rio_backend::event::EventListener> ContextGridItem<T> {
    pub fn new(context: Context<T>) -> Self {
        Self {
            width: context.dimension.width,
            height: context.dimension.height,
            val: context,
            right: None,
            down: None,
        }
    }
}

impl<T: rio_backend::event::EventListener> ContextGridItem<T> {
    #[inline]
    pub fn context(&self) -> &Context<T> {
        &self.val
    }

    #[inline]
    pub fn context_mut(&mut self) -> &mut Context<T> {
        &mut self.val
    }
}

impl<T: rio_backend::event::EventListener> ContextGrid<T> {
    pub fn new(
        context: Context<T>,
        margin: Delta<f32>,
        split_colors: ([f32; 4], [f32; 4]),
    ) -> Self {
        let border_color = split_colors.0;
        let active_border_color = split_colors.1;
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
            active_border_color,
        }
    }

    #[inline]
    pub fn get_grid_item(&self, index: usize) -> &ContextGridItem<T> {
        &self.inner[index]
    }

    #[inline]
    pub fn contexts(&self) -> &Vec<ContextGridItem<T>> {
        &self.inner
    }

    #[inline]
    pub fn contexts_mut(&mut self) -> &mut Vec<ContextGridItem<T>> {
        &mut self.inner
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
            if let Some(item) = self.inner.get(0) {
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
            let border_color = if index == self.current {
                self.active_border_color
            } else {
                self.border_color
            };

            let border_width = 1.0;

            objects.push(Object::Quad(ComposedQuad {
                color: [0.0, 0.0, 0.0, 0.0],
                quad: Quad {
                    position: [margin.x, margin.top_y],
                    shadow_blur_radius: 0.0,
                    shadow_offset: [0.0, 0.0],
                    shadow_color: [0.0, 0.0, 0.0, 0.5],
                    border_color,
                    border_width: 1.0,
                    border_radius: [0.0, 0.0, 0.0, 0.0],
                    size: [
                        item.width / item.val.dimension.dimension.scale,
                        item.height / item.val.dimension.dimension.scale,
                    ],
                },
            }));

            objects.push(Object::RichText(RichText {
                id: item.val.rich_text_id,
                position: [margin.x + border_width, margin.top_y - border_width],
            }));

            if let Some(right_item) = item.right {
                let new_margin = Delta {
                    x: margin.x
                        + PADDING
                        + (item.width / item.val.dimension.dimension.scale),
                    top_y: margin.top_y,
                    bottom_y: margin.bottom_y,
                };
                self.plot_objects(objects, right_item, new_margin);
            }

            // if let Some(right_item) = item.right {
            //     self.plot_objects(objects, right_item);
            // }
        }
    }

    pub fn update_margin(&mut self, padding: (f32, f32, f32)) {
        self.margin = Delta {
            x: padding.0,
            top_y: padding.1,
            bottom_y: padding.2,
        };
    }

    pub fn remove_current_grid(&mut self) {}

    pub fn split_right(&mut self, context: Context<T>) {
        // If we are moving from first to second context, needs to change height
        let should_change_height = self.inner.len() == 1;

        if should_change_height {
            self.inner[self.current].height -= self.margin.top_y
                * self.inner[self.current].val.dimension.dimension.scale;
            // self.inner[self.current].val.dimension.height -= PADDING * 2.0;
        }

        let old_grid_item_width = self.inner[self.current].width;
        let new_grid_item_width = (old_grid_item_width / 2.0) - PADDING;
        // Change grid item by half
        self.inner[self.current].width = new_grid_item_width;
        // Move content to middle
        self.inner[self.current]
            .val
            .dimension
            .update_width(new_grid_item_width - (PADDING * 2.0));

        let mut terminal = self.inner[self.current].val.terminal.lock();
        terminal.resize::<ContextDimension>(self.inner[self.current].val.dimension);
        drop(terminal);
        let winsize = crate::renderer::utils::terminal_dimensions(
            &self.inner[self.current].val.dimension,
        );
        let _ = self.inner[self.current].val.messenger.send_resize(winsize);

        let mut new_context = ContextGridItem::new(context);
        new_context.width = new_grid_item_width;
        new_context.height = self.inner[self.current].height;

        new_context
            .val
            .dimension
            .update_width(new_grid_item_width - (PADDING * 2.0));

        self.inner.push(new_context);
        let new_current = self.inner.len() - 1;

        let mut terminal = self.inner[new_current].val.terminal.lock();
        terminal.resize::<ContextDimension>(self.inner[new_current].val.dimension);
        drop(terminal);
        let winsize = crate::renderer::utils::terminal_dimensions(
            &self.inner[new_current].val.dimension,
        );
        let _ = self.inner[new_current].val.messenger.send_resize(winsize);

        self.inner[self.current].right = Some(new_current);
        self.current = new_current;
    }

    pub fn split_down(&mut self) {}
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
        let (columns, lines) =
            compute(self.width, self.height, self.dimension, 1.0, self.margin);
        self.columns = columns;
        self.lines = lines;
    }

    #[inline]
    pub fn update(&mut self) {
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
        let grid = ContextGrid::<VoidListener>::new(
            context,
            margin,
            ([0., 0., 0., 0.], [0., 0., 0., 0.]),
        );
        // The first context should fill completely w/h grid
        assert_eq!(grid.width, context_width);
        assert_eq!(grid.height, context_height);
        // The first context should fill completely w/g grid item
        let grid_item = grid.get_grid_item(0);
        assert_eq!(grid_item.width, context_width);
        assert_eq!(grid_item.height, context_height);

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
    fn test_single_split_right() {
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

        let mut grid = ContextGrid::<VoidListener>::new(
            first_context,
            margin,
            ([0., 0., 0., 0.], [0., 0., 0., 0.]),
        );

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
                Object::Quad(ComposedQuad {
                    color: [1.0, 0.5, 0.5, 0.5],
                    quad: Quad {
                        position: [440., 5.],
                        shadow_blur_radius: 0.0,
                        shadow_offset: [0.0, 0.0],
                        shadow_color: [1.0, 1.0, 0.0, 1.0],
                        border_color: [1.0, 0.0, 1.0, 1.0],
                        border_width: 2.0,
                        border_radius: [0.0, 0.0, 0.0, 0.0],
                        size: [320.0, 150.0],
                    },
                }),
                Object::RichText(RichText {
                    id: first_context_id,
                    position: [10., 20.],
                }),
                Object::Quad(ComposedQuad {
                    color: [1.0, 0.5, 0.5, 0.5],
                    quad: Quad {
                        position: [440., 5.],
                        shadow_blur_radius: 0.0,
                        shadow_offset: [0.0, 0.0],
                        shadow_color: [1.0, 1.0, 0.0, 1.0],
                        border_color: [1.0, 0.0, 1.0, 1.0],
                        border_width: 2.0,
                        border_radius: [0.0, 0.0, 0.0, 0.0],
                        size: [320.0, 150.0],
                    },
                }),
                Object::RichText(RichText {
                    id: second_context_id,
                    position: [10., 20.],
                }),
            ]
        );
    }

    #[test]
    fn test_resize() {}
}
