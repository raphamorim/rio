// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// Modules bidi, builder, builder_data, layout, layout_data, line_breaker
// nav and span_style were originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE

mod content;
mod glyph;
mod render_data;

pub use glyph::Glyph;
pub use render_data::RenderData;

pub use content::{
    BuilderLine, BuilderState, Content, FragmentStyle, FragmentStyleDecoration,
    UnderlineInfo, UnderlineShape,
};
pub use render_data::Run;

/// Index of a span in sequential order of submission to a paragraph content.
#[derive(Copy, Clone, PartialOrd, Ord, PartialEq, Eq, Hash, Default, Debug)]
pub struct SpanId(pub usize);

impl SpanId {
    /// Converts the span identifier to an index.
    pub fn to_usize(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Delta<T: Default> {
    pub x: T,
    pub top_y: T,
    pub bottom_y: T,
}

#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub struct SugarDimensions {
    pub width: f32,
    pub height: f32,
    pub scale: f32,
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub struct RichTextLayout {
    pub width: f32,
    pub height: f32,
    pub line_height: f32,
    pub font_size: f32,
    pub original_font_size: f32,
    pub columns: usize,
    pub lines: usize,
    pub dimensions: SugarDimensions,
    pub margin: Delta<f32>,
}

impl RichTextLayout {
    #[inline]
    pub fn update_columns_per_font_width(&mut self, layout: &SugarloafLayout) {
        // SugarStack is a primitive representation of columns data
        let current_stack_bound =
            (self.dimensions.width * self.dimensions.scale) * self.columns as f32;
        let expected_stack_bound = (layout.width / self.dimensions.scale)
            - (self.dimensions.width * self.dimensions.scale);

        tracing::info!("expected columns {}", self.columns);
        if current_stack_bound < expected_stack_bound {
            let stack_difference = ((expected_stack_bound - current_stack_bound)
                / (self.dimensions.width * self.dimensions.scale))
                as usize;
            tracing::info!("recalculating columns due to font width, adding more {stack_difference:?} columns");
            let _ = self.columns.wrapping_add(stack_difference);
        }

        if current_stack_bound > expected_stack_bound {
            let stack_difference = ((current_stack_bound - expected_stack_bound)
                / (self.dimensions.width * self.dimensions.scale))
                as usize;
            tracing::info!("recalculating columns due to font width, removing {stack_difference:?} columns");
            let _ = self.columns.wrapping_sub(stack_difference);
        }
    }

    #[inline]
    pub fn update(&mut self) {
        let (columns, lines) = compute(
            self.width,
            self.height,
            self.dimensions,
            self.line_height,
            self.margin,
        );
        self.columns = columns;
        self.lines = lines;
    }

    #[inline]
    pub fn rescale(&mut self, scale_factor: f32) -> &mut Self {
        self.dimensions.width *= scale_factor;
        self.dimensions.height *= scale_factor;
        self.dimensions.scale = scale_factor;
        self
    }
}

impl Default for RichTextLayout {
    fn default() -> Self {
        Self {
            width: 0.0,
            height: 0.0,
            line_height: 1.0,
            font_size: 0.0,
            original_font_size: 0.0,
            columns: MIN_COLS,
            lines: MIN_LINES,
            dimensions: SugarDimensions::default(),
            margin: Delta::<f32>::default(),
        }
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub struct SugarloafLayout {
    pub width: f32,
    pub height: f32,
    pub margin: Delta<f32>,
    pub scale_factor: f32,
    pub default_rich_text: RichTextLayout,
}

impl Default for SugarloafLayout {
    fn default() -> Self {
        Self {
            width: 0.0,
            height: 0.0,
            margin: Delta::<f32>::default(),
            scale_factor: 1.0,
            default_rich_text: RichTextLayout::default(),
        }
    }
}

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

impl SugarloafLayout {
    pub fn new(
        width: f32,
        height: f32,
        padding: (f32, f32, f32),
        scale_factor: f32,
        font_size: f32,
        _line_height: f32,
    ) -> SugarloafLayout {
        // Line height can never be zero
        // let line_height = if line_height == 0.0 { 1.0 } else { line_height };

        SugarloafLayout {
            width,
            height,
            // dimensions: SugarDimensions {
            //     scale: scale_factor,
            //     ..SugarDimensions::default()
            // },
            // line_height,
            scale_factor,
            margin: Delta {
                x: padding.0,
                top_y: padding.1,
                bottom_y: padding.2,
            },
            default_rich_text: RichTextLayout {
                font_size,
                original_font_size: font_size,
                columns: MIN_COLS,
                lines: MIN_LINES,
                dimensions: SugarDimensions {
                    scale: scale_factor,
                    ..SugarDimensions::default()
                },
                ..RichTextLayout::default()
            },
        }
    }

    #[inline]
    pub fn resize(&mut self, width: u32, height: u32) -> &mut Self {
        self.width = width as f32;
        self.height = height as f32;
        self
    }

    // This method will run over the new font and font_size
    #[inline]
    pub fn recalculate(
        &mut self,
        _font_size: f32,
        _line_height: f32,
        margin_x: f32,
        margin_y_top: f32,
        margin_y_bottom: f32,
    ) -> &mut Self {
        // if self.font_size != font_size {
        //     self.font_size = font_size;
        //     self.original_font_size = font_size;
        //     should_apply_changes = true;
        // }

        // if self.line_height != line_height {
        //     self.line_height = line_height;
        //     should_apply_changes = true;
        // }

        self.margin.x = margin_x;
        self.margin.bottom_y = margin_y_bottom;
        self.margin.top_y = margin_y_top;

        self
    }
}
