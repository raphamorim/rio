// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// Modules bidi, builder, builder_data, layout, layout_data, line_breaker
// nav and span_style were originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE

mod builder;
mod content;
mod layout_data;
mod render_data;

pub use content::{Content, ContentBuilder};
pub use render_data::RenderData;

/// Iterators over elements of a paragraph.
pub mod iter {
    pub use super::render_data::{Clusters, Glyphs, Lines, Runs};
}

pub use builder::{LayoutContext, ParagraphBuilder, FragmentStyle, FragmentStyleDecoration, UnderlineInfo, UnderlineShape,};
pub use render_data::{Cluster, Glyph, Line, Run};

/// Index of a span in sequential order of submission to a paragraph builder.
#[derive(Copy, Clone, PartialOrd, Ord, PartialEq, Eq, Hash, Default, Debug)]
pub struct SpanId(pub usize);

impl SpanId {
    /// Converts the span identifier to an index.
    pub fn to_usize(self) -> usize {
        self.0
    }
}

use crate::sugarloaf::primitives::SugarloafStyle;

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
pub struct SugarloafLayout {
    pub line_height: f32,
    pub width: f32,
    pub height: f32,
    pub font_size: f32,
    pub original_font_size: f32,
    pub columns: usize,
    pub lines: usize,
    pub margin: Delta<f32>,
    pub style: SugarloafStyle,
    pub dimensions: SugarDimensions,
}

impl Default for SugarloafLayout {
    fn default() -> Self {
        Self {
            line_height: 1.0,
            width: 0.0,
            height: 0.0,
            font_size: 0.0,
            original_font_size: 0.0,
            columns: MIN_COLS,
            lines: MIN_LINES,
            margin: Delta::<f32>::default(),
            style: SugarloafStyle::default(),
            dimensions: SugarDimensions::default(),
        }
    }
}

#[inline]
fn update_styles(layout: &mut SugarloafLayout) {
    let text_scale = layout.font_size * layout.dimensions.scale;
    let new_styles = SugarloafStyle {
        line_height: layout.line_height,
        screen_position: (
            layout.margin.x * layout.dimensions.scale,
            layout.margin.top_y * layout.dimensions.scale,
        ),
        text_scale,
    };
    layout.style = new_styles;
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
        line_height: f32,
    ) -> SugarloafLayout {
        let style = SugarloafStyle::default();

        // Line height can never be zero
        let line_height = if line_height == 0.0 { 1.0 } else { line_height };

        let mut layout = SugarloafLayout {
            width,
            height,
            columns: MIN_COLS,
            lines: MIN_LINES,
            original_font_size: font_size,
            font_size,
            dimensions: SugarDimensions {
                scale: scale_factor,
                ..SugarDimensions::default()
            },
            line_height,
            style,
            margin: Delta {
                x: padding.0,
                top_y: padding.1,
                bottom_y: padding.2,
            },
        };

        update_styles(&mut layout);
        layout
    }

    #[inline]
    pub fn rescale(&mut self, scale_factor: f32) -> &mut Self {
        self.dimensions.width *= scale_factor;
        self.dimensions.height *= scale_factor;
        self.dimensions.scale = scale_factor;
        self
    }

    #[inline]
    pub fn resize(&mut self, width: u32, height: u32) -> &mut Self {
        self.width = width as f32;
        self.height = height as f32;
        self
    }

    pub fn increase_font_size(&mut self) -> bool {
        if self.font_size < 40.0 {
            self.font_size += 1.0;
            return true;
        }
        false
    }

    pub fn decrease_font_size(&mut self) -> bool {
        if self.font_size > 6.0 {
            self.font_size -= 1.0;
            return true;
        }
        false
    }

    pub fn reset_font_size(&mut self) -> bool {
        if self.font_size != self.original_font_size {
            self.font_size = self.original_font_size;
            return true;
        }
        false
    }

    #[inline]
    pub fn update(&mut self) {
        update_styles(self);
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
    pub fn update_columns_per_font_width(&mut self) {
        // SugarStack is a primitive representation of columns data
        let current_stack_bound =
            (self.dimensions.width * self.dimensions.scale) * self.columns as f32;
        let expected_stack_bound = (self.width / self.dimensions.scale)
            - (self.dimensions.width * self.dimensions.scale);

        tracing::info!("expected columns {}", self.columns);
        if current_stack_bound < expected_stack_bound {
            let stack_difference = ((expected_stack_bound - current_stack_bound)
                / (self.dimensions.width * self.dimensions.scale))
                as usize;
            tracing::info!("recalculating columns due to font width, adding more {stack_difference:?} columns");
            self.columns += stack_difference;
        }

        if current_stack_bound > expected_stack_bound {
            let stack_difference = ((current_stack_bound - expected_stack_bound)
                / (self.dimensions.width * self.dimensions.scale))
                as usize;
            tracing::info!("recalculating columns due to font width, removing {stack_difference:?} columns");
            self.columns -= stack_difference;
        }
    }

    // This method will run over the new font and font_size
    #[inline]
    pub fn recalculate(
        &mut self,
        font_size: f32,
        line_height: f32,
        margin_x: f32,
        margin_y_top: f32,
        margin_y_bottom: f32,
    ) -> &mut Self {
        let mut should_apply_changes = false;
        if self.font_size != font_size {
            self.font_size = font_size;
            self.original_font_size = font_size;
            should_apply_changes = true;
        }

        if self.line_height != line_height {
            self.line_height = line_height;
            should_apply_changes = true;
        }

        if self.margin.x != margin_x {
            self.margin.x = margin_x;
            should_apply_changes = true;
        }

        if self.margin.bottom_y != margin_y_bottom {
            self.margin.bottom_y = margin_y_bottom;
            should_apply_changes = true;
        }

        if self.margin.top_y != margin_y_top {
            self.margin.top_y = margin_y_top;
            should_apply_changes = true;
        }

        if should_apply_changes {
            update_styles(self);
        }

        self
    }
}
