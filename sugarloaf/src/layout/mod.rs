// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// Modules bidi, builder, builder_data, layout, layout_data, line_breaker
// nav and span_style were originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE

pub mod content;
pub mod content_data;
mod glyph;
mod render_data;
pub mod rich_text_render_data;

pub use glyph::Glyph;
pub use render_data::RenderData;
pub use rich_text_render_data::RichTextRenderData;

pub use content::{
    BuilderLine, BuilderState, BuilderStateUpdate, Content, FragmentData, ShapingCache,
    SpanStyle, SpanStyleDecoration, UnderlineInfo, UnderlineShape,
};
pub use content_data::{ContentData, ContentRenderData, ContentState};
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

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TextDimensions {
    pub width: f32,
    pub height: f32,
    pub scale: f32,
}

impl Default for TextDimensions {
    fn default() -> Self {
        Self {
            width: 8.0,   // Reasonable character cell width fallback
            height: 16.0, // Reasonable character cell height fallback
            scale: 1.0,
        }
    }
}

/// Canonical cell metrics in physical pixels. Rounded `u32` cell
/// width / height / baseline are the single source of truth for the
/// GPU grid uniform, the col/row count math, and mouse hit testing.
/// The unrounded `f64` `face_width / face_height` are retained for
/// downstream subpixel math (image positioning, baseline-relative
/// offsets).
///
/// Important invariants:
/// - `cell_width = round(face_width)` (half-away-from-zero)
/// - `cell_height = round(face_height)` (half-away-from-zero) —
///   `face_height` already has the user's `line_height` multiplier
///   baked in; consumers MUST NOT re-apply it.
/// - `cell_baseline` is pixels from the **bottom** of the cell to
///   the text baseline.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct CellMetrics {
    pub cell_width: u32,
    pub cell_height: u32,
    pub cell_baseline: u32,
    pub face_width: f64,
    pub face_height: f64,
}

impl Default for CellMetrics {
    fn default() -> Self {
        Self {
            cell_width: 8,
            cell_height: 16,
            cell_baseline: 4,
            face_width: 8.0,
            face_height: 16.0,
        }
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub struct TextLayout {
    pub line_height: f32,
    pub font_size: f32,
    pub original_font_size: f32,
    pub dimensions: TextDimensions,
    /// Canonical cell metrics. Single source of truth for the
    /// integer cell stride and baseline. Consumers should prefer
    /// this over `dimensions` for any cell-coordinate math (renderer
    /// grid uniform, layout col/row count, mouse hit testing) —
    /// `dimensions` is kept for legacy callers that read the raw f32
    /// width/height.
    pub cell: CellMetrics,
}

impl TextLayout {
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
            dimensions: TextDimensions {
                scale: default_layout.scale_factor,
                ..TextDimensions::default()
            },
            cell: CellMetrics::default(),
        }
    }
}

impl Default for TextLayout {
    fn default() -> Self {
        Self {
            line_height: 1.0,
            font_size: 0.0,
            original_font_size: 0.0,
            dimensions: TextDimensions::default(),
            cell: CellMetrics::default(),
        }
    }
}

/// Configuration for creating rich text with custom properties
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct RichTextConfig {
    /// Initial position [x, y] where the rich text should be rendered
    pub position: Option<[f32; 2]>,
    /// Depth value for z-ordering (more negative = closer to camera/in front)
    pub depth: f32,
}

impl Default for RichTextConfig {
    fn default() -> Self {
        Self {
            position: None,
            depth: 0.0,
        }
    }
}

impl RichTextConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_position(mut self, x: f32, y: f32) -> Self {
        self.position = Some([x, y]);
        self
    }

    pub fn with_depth(mut self, depth: f32) -> Self {
        self.depth = depth;
        self
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
