use crate::context::Context;
use rio_backend::crosswords::grid::Dimensions;
use rio_backend::event::EventListener;
use rio_backend::sugarloaf::{Object, RichText, layout::SugarDimensions};

const MIN_COLS: usize = 2;
const MIN_LINES: usize = 1;

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
    pub current: usize,
    pub margin: Delta<f32>,
    inner: Vec<Context<T>>,
}

impl<T: rio_backend::event::EventListener> ContextGrid<T> {
    pub fn new(context: Context<T>, margin: Delta<f32>,) -> Self {
        Self {
            inner: vec![context],
            current: 0,
            margin,
        }
    }

    #[inline]
    pub fn current(&self) -> &Context<T> {
        &self.inner[self.current]
    }

    #[inline]
    pub fn current_mut(&mut self) -> &mut Context<T> {
        &mut self.inner[self.current]
    }

    #[inline]
    pub fn objects(&self) -> Vec<Object> {
        vec![Object::RichText(RichText {
            id: 0,
            position: [0., 0.],
        })]
    }

    pub fn update_margin(&mut self, padding: (f32, f32, f32)) {
        self.margin = Delta {
            x: padding.0,
            top_y: padding.1,
            bottom_y: padding.2,
        };
    }

    pub fn split_right(&mut self) {}

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
    ) -> Self {
        let margin = Delta {
            x: 0.,
            top_y: 0.,
            bottom_y: 0.,
        };
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
