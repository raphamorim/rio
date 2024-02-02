// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// Modules bidi, builder, builder_data, layout, layout_data, line_breaker
// nav and span_style were originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE

//! Experimental paragraph layout engine.

pub mod font;

mod bidi;
mod builder;
mod builder_data;
mod layout;
mod layout_data;
mod line_breaker;
mod nav;
mod span_style;

pub use swash;

pub use font::prelude::*;

#[doc(inline)]
pub use swash::text::Language;

/// Iterators over elements of a paragraph.
pub mod iter {
    pub use super::layout::{Clusters, Glyphs, Lines, Runs};
}

pub use builder::{LayoutContext, ParagraphBuilder};
#[doc(inline)]
pub use font::{Font, FontLibrary, FontLibraryBuilder};
pub use layout::{Cluster, Glyph, Line, Run};
pub use line_breaker::{Alignment, BreakLines};
pub use nav::{Erase, ExtendTo, Selection};
pub use span_style::*;

use layout_data::{LayoutData, LineLayoutData};

/// Collection of text, organized into lines, runs and clusters.
#[derive(Clone, Default)]
pub struct Paragraph {
    data: LayoutData,
    line_data: LineLayoutData,
}

/// Largest allowable span or fragment identifier.
const MAX_ID: u32 = i32::MAX as u32;

/// Index of a span in sequential order of submission to a paragraph builder.
#[derive(Copy, Clone, PartialOrd, Ord, PartialEq, Eq, Hash, Default, Debug)]
pub struct SpanId(pub u32);

impl SpanId {
    /// Converts the span identifier to an index.
    pub fn to_usize(self) -> usize {
        self.0 as usize
    }
}

use crate::components::layer::types;
use crate::core::SugarloafStyle;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Delta<T: Default> {
    pub x: T,
    pub top_y: T,
    pub bottom_y: T,
}

#[derive(Default, Clone)]
pub struct SugarloafLayout {
    pub scale_factor: f32,
    pub line_height: f32,
    pub width: f32,
    pub height: f32,
    pub width_u32: u32,
    pub height_u32: u32,
    pub font_size: f32,
    pub original_font_size: f32,
    pub columns: usize,
    pub lines: usize,
    pub margin: Delta<f32>,
    pub style: SugarloafStyle,
    pub background_color: wgpu::Color,
    pub background_image: Option<types::Image>,
    pub min_cols_lines: (usize, usize),
    pub sugarwidth: f32,
    pub sugarheight: f32,
    pub scaled_sugarwidth: f32,
    pub scaled_sugarheight: f32,
}

#[inline]
fn update_styles(layout: &mut SugarloafLayout) {
    let text_scale = layout.font_size * layout.scale_factor;
    let new_styles = SugarloafStyle {
        line_height: layout.line_height,
        screen_position: (
            layout.margin.x * layout.scale_factor,
            layout.margin.top_y * layout.scale_factor,
        ),
        text_scale,
    };
    layout.style = new_styles;
}

// $ tput columns
// $ tput lines
#[inline]
fn compute(
    dimensions: (f32, f32),
    scale_factor: f32,
    line_height: f32,
    sugarwidth: f32,
    sugarheight: f32,
    margin: Delta<f32>,
    min_cols_lines: (usize, usize),
) -> (usize, usize) {
    let margin_x = ((margin.x) * scale_factor).floor();
    let margin_spaces = (margin.top_y * 2.) + margin.bottom_y;

    let mut lines = (dimensions.1 / scale_factor) - margin_spaces;
    lines /= sugarheight * line_height;
    let visible_lines = std::cmp::max(lines.floor() as usize, min_cols_lines.1);

    let mut visible_columns = (dimensions.0 / scale_factor) - margin_x;
    visible_columns /= sugarwidth;
    let visible_columns = std::cmp::max(visible_columns as usize, min_cols_lines.0);

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
        min_cols_lines: (usize, usize),
    ) -> SugarloafLayout {
        let style = SugarloafStyle::default();

        let mut layout = SugarloafLayout {
            width,
            width_u32: width as u32,
            height,
            height_u32: height as u32,
            columns: 80,
            lines: 25,
            scale_factor,
            original_font_size: font_size,
            font_size,
            sugarwidth: font_size,
            sugarheight: font_size,
            scaled_sugarwidth: font_size * scale_factor,
            scaled_sugarheight: font_size * scale_factor,
            background_image: None,
            line_height,
            style,
            margin: Delta {
                x: padding.0,
                top_y: padding.1,
                bottom_y: padding.2,
            },
            background_color: wgpu::Color::BLACK,
            min_cols_lines,
        };

        update_styles(&mut layout);
        layout
    }

    #[inline]
    pub fn rescale(&mut self, scale_factor: f32) -> &mut Self {
        self.scale_factor = scale_factor;
        self
    }

    #[inline]
    pub fn resize(&mut self, width: u32, height: u32) -> &mut Self {
        self.width_u32 = width;
        self.height_u32 = height;
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
    pub fn update(&mut self) -> &mut Self {
        update_styles(self);
        let (columns, lines) = compute(
            (self.width, self.height),
            self.scale_factor,
            self.line_height,
            self.sugarwidth,
            self.sugarheight,
            self.margin,
            self.min_cols_lines,
        );
        self.columns = columns;
        self.lines = lines;
        self
    }

    #[inline]
    pub fn update_columns_per_font_width(&mut self) {
        // SugarStack is a primitive representation of columns data
        let current_stack_bound = self.sugarwidth * self.columns as f32;
        let expected_stack_bound = (self.width / self.scale_factor) - self.sugarwidth;

        log::info!("expected columns {}", self.columns);
        if current_stack_bound < expected_stack_bound {
            let stack_difference =
                ((expected_stack_bound - current_stack_bound) / self.sugarwidth) as usize;
            log::info!("recalculating columns due to font width, adding more {stack_difference:?} columns");
            self.columns += stack_difference;
        }

        if current_stack_bound > expected_stack_bound {
            let stack_difference =
                ((current_stack_bound - expected_stack_bound) / self.sugarwidth) as usize;
            log::info!("recalculating columns due to font width, removing {stack_difference:?} columns");
            self.columns -= stack_difference;
        }
    }

    #[inline]
    pub fn set_margin_top_y(&mut self, top_y: f32) {
        self.margin.top_y = top_y;
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
