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
pub struct SugarloafLayout {
    pub scale_factor: f32,
    pub default_rich_text: RichTextLayout,
}

impl Default for SugarloafLayout {
    fn default() -> Self {
        Self {
            scale_factor: 1.0,
            default_rich_text: RichTextLayout::default(),
        }
    }
}

impl SugarloafLayout {
    pub fn new(
        scale_factor: f32,
        font_size: f32,
        _line_height: f32,
    ) -> SugarloafLayout {
        // Line height can never be zero
        // let line_height = if line_height == 0.0 { 1.0 } else { line_height };

        SugarloafLayout {
            // dimensions: SugarDimensions {
            //     scale: scale_factor,
            //     ..SugarDimensions::default()
            // },
            // line_height,
            scale_factor,
            default_rich_text: RichTextLayout {
                font_size,
                original_font_size: font_size,
                dimensions: SugarDimensions {
                    scale: scale_factor,
                    ..SugarDimensions::default()
                },
                ..RichTextLayout::default()
            },
        }
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

        // self.margin.x = margin_x;
        // self.margin.bottom_y = margin_y_bottom;
        // self.margin.top_y = margin_y_top;

        self
    }
}
