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
    BuilderLine, BuilderState, BuilderStateUpdate, Content, FragmentStyle,
    FragmentStyleDecoration, UnderlineInfo, UnderlineShape, WordCache,
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
    pub line_height: f32,
    pub font_size: f32,
    pub original_font_size: f32,
    pub dimensions: SugarDimensions,
}

impl RichTextLayout {
    #[inline]
    pub fn rescale(&mut self, scale_factor: f32) -> &mut Self {
        self.dimensions.width *= scale_factor;
        self.dimensions.height *= scale_factor;
        self.dimensions.scale = scale_factor;
        self
    }

    pub fn from_default_layout(default_layout: &RootStyle) -> Self {
        Self {
            line_height: default_layout.line_height,
            font_size: default_layout.font_size,
            original_font_size: default_layout.font_size,
            dimensions: SugarDimensions {
                scale: default_layout.scale_factor,
                ..SugarDimensions::default()
            },
        }
    }
}

impl Default for RichTextLayout {
    fn default() -> Self {
        Self {
            line_height: 1.0,
            font_size: 0.0,
            original_font_size: 0.0,
            dimensions: SugarDimensions::default(),
        }
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub struct RootStyle {
    pub scale_factor: f32,
    pub font_size: f32,
    pub line_height: f32,
}

impl Default for RootStyle {
    fn default() -> Self {
        Self {
            line_height: 1.0,
            scale_factor: 1.0,
            font_size: 14.,
        }
    }
}

impl RootStyle {
    pub fn new(scale_factor: f32, font_size: f32, line_height: f32) -> RootStyle {
        // Line height cannot be under 1
        let line_height = if line_height <= 1.0 { 1.0 } else { line_height };

        RootStyle {
            scale_factor,
            font_size,
            line_height,
        }
    }
}
