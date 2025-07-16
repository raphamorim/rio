// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// layout.rs was originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE
//
// This file however suffered updates made by Raphael Amorim to support
// underline_color, background_color, text color and other functionalities

//! RenderData.
use super::glyph::*;
use crate::font_introspector::shape::cluster::OwnedGlyphCluster;
use crate::font_introspector::shape::Shaper;
use crate::font_introspector::Metrics;
use crate::layout::content::{FragmentStyleDecoration, WordCache};
use crate::layout::FragmentStyle;
use crate::sugarloaf::primitives::SugarCursor;
use crate::{Graphic, GraphicId};

/// Collection of text, organized into lines, runs and clusters.
#[derive(Clone, Debug, Default)]
pub struct RenderData {
    pub runs: Vec<RunData>,
    pub glyphs: Vec<GlyphData>,
    pub detailed_glyphs: Vec<Glyph>,
    pub graphics: std::collections::HashSet<GraphicId>,
}

impl RenderData {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// Creates a new empty paragraph.
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn reserve(&mut self, capacity: usize) {
        self.runs.reserve(capacity);
        self.glyphs.reserve(capacity);
        self.detailed_glyphs.reserve(capacity);
        self.graphics.reserve(capacity);
    }

    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            runs: Vec::with_capacity(capacity),
            glyphs: Vec::with_capacity(capacity),
            detailed_glyphs: Vec::with_capacity(capacity),
            graphics: std::collections::HashSet::with_capacity(capacity),
        }
    }

    /// Clears the paragraph.
    #[inline]
    pub fn clear(&mut self) {
        self.runs.clear();
        self.glyphs.clear();
        self.detailed_glyphs.clear();
        self.graphics.clear();
    }
}

impl RenderData {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn push_run(
        &mut self,
        style: FragmentStyle,
        size: f32,
        line: u32,
        shaper: Shaper<'_>,
        shaper_cache: &mut WordCache,
    ) {
        // let clusters_start = self.data.clusters.len() as u32;
        let metrics = shaper.metrics();

        let mut glyphs = vec![];
        let mut detailed_glyphs = vec![];
        let mut advance = 0.;

        shaper.shape_with(|c| {
            shaper_cache.add_glyph_cluster(c);

            let mut cluster_advance = 0.;
            for glyph in c.glyphs {
                cluster_advance += glyph.advance;
                const MAX_SIMPLE_ADVANCE: u32 = 0x7FFF;
                if glyph.x == 0. && glyph.y == 0. {
                    let packed_advance = (glyph.advance * 64.) as u32;
                    if packed_advance <= MAX_SIMPLE_ADVANCE {
                        // Simple glyph
                        glyphs.push(GlyphData {
                            data: glyph.id as u32 | (packed_advance << 16),
                            size: glyph.data as usize,
                        });
                        continue;
                    }
                }
                // Complex glyph
                let detail_index = detailed_glyphs.len() as u32;
                detailed_glyphs.push(Glyph::new(glyph));
                glyphs.push(GlyphData {
                    data: GLYPH_DETAILED | detail_index,
                    size: glyph.data as usize,
                });
            }
            advance += cluster_advance;
        });
        shaper_cache.finish();
        if let Some(graphic) = style.media {
            self.graphics.insert(graphic.id);
        }

        let run_data = RunData {
            span: style,
            line,
            size,
            detailed_glyphs,
            glyphs,
            // ascent: metrics.ascent * span_data.line_spacing,
            ascent: metrics.ascent,
            // descent: metrics.descent * span_data.line_spacing,
            descent: metrics.descent,
            // leading: metrics.leading * span_data.line_spacing,
            leading: metrics.leading,
            underline_offset: metrics.underline_offset,
            strikeout_offset: metrics.strikeout_offset,
            strikeout_size: metrics.stroke_size,
            x_height: metrics.x_height,
            advance,
        };
        self.runs.push(run_data);
    }

    pub(super) fn push_run_without_shaper(
        &mut self,
        style: FragmentStyle,
        size: f32,
        line: u32,
        glyph_clusters: &Vec<OwnedGlyphCluster>,
        metrics: &Metrics,
    ) -> bool {
        // In case is a new line,
        // then needs to recompute the span index again
        let mut advance = 0.;
        let mut glyphs = vec![];
        let mut detailed_glyphs = vec![];

        for c in glyph_clusters {
            let mut cluster_advance = 0.;
            for glyph in &c.glyphs {
                cluster_advance += glyph.advance;
                const MAX_SIMPLE_ADVANCE: u32 = 0x7FFF;
                if glyph.x == 0. && glyph.y == 0. {
                    let packed_advance = (glyph.advance * 64.) as u32;
                    if packed_advance <= MAX_SIMPLE_ADVANCE {
                        // Simple glyph
                        glyphs.push(GlyphData {
                            data: glyph.id as u32 | (packed_advance << 16),
                            size: glyph.data as usize,
                        });
                        continue;
                    }
                }
                // Complex glyph
                let detail_index = detailed_glyphs.len() as u32;
                detailed_glyphs.push(Glyph::new(glyph));
                glyphs.push(GlyphData {
                    data: GLYPH_DETAILED | detail_index,
                    size: glyph.data as usize,
                });
            }
            advance += cluster_advance;
        }
        if let Some(graphic) = style.media {
            self.graphics.insert(graphic.id);
        }
        let run_data = RunData {
            span: style,
            line,
            size,
            detailed_glyphs,
            glyphs,
            // ascent: metrics.ascent * span_data.line_spacing,
            ascent: metrics.ascent,
            // descent: metrics.descent * span_data.line_spacing,
            descent: metrics.descent,
            // leading: metrics.leading * span_data.line_spacing,
            leading: metrics.leading,
            underline_offset: metrics.underline_offset,
            strikeout_offset: metrics.strikeout_offset,
            strikeout_size: metrics.stroke_size,
            x_height: metrics.x_height,
            advance,
        };
        self.runs.push(run_data);
        true
    }
}

/// Sequence of clusters sharing the same font, size and span.
#[derive(Copy, Clone)]
pub struct Run<'a> {
    pub(super) run: &'a RunData,
}

impl Run<'_> {
    /// Returns the span that contains the run.
    #[inline]
    pub fn span(&self) -> FragmentStyle {
        self.run.span
    }

    #[inline]
    pub fn media(&self) -> Option<Graphic> {
        self.run.span.media
    }

    /// Returns the font for the run.
    #[inline]
    pub fn font(&self) -> &usize {
        &self.run.span.font_id
    }

    /// Returns the font size for the run.
    #[inline]
    pub fn font_size(&self) -> f32 {
        self.run.size
    }

    /// Returns the color for the run.
    #[inline]
    pub fn color(&self) -> [f32; 4] {
        self.run.span.color
    }

    #[inline]
    pub fn char_width(&self) -> f32 {
        self.run.span.width
    }

    /// Returns the cursor
    #[inline]
    pub fn cursor(&self) -> Option<SugarCursor> {
        self.run.span.cursor
    }

    /// Returns the advance of the run.
    #[inline]
    pub fn advance(&self) -> f32 {
        self.run.advance
    }

    /// Returns true if the run has an background color
    #[inline]
    pub fn background_color(&self) -> Option<[f32; 4]> {
        self.run.span.background_color
    }

    /// Returns true if the run has an underline decoration.
    #[inline]
    pub fn decoration(&self) -> Option<FragmentStyleDecoration> {
        self.run.span.decoration
    }

    #[inline]
    pub fn decoration_color(&self) -> Option<[f32; 4]> {
        self.run.span.decoration_color
    }
}
