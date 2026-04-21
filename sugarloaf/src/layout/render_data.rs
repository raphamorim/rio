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
#[cfg(test)]
use crate::font_introspector::shape::cluster::OwnedGlyphCluster;
#[cfg(not(target_os = "macos"))]
use crate::font_introspector::shape::Shaper;
use crate::font_introspector::Metrics;
use crate::layout::content::{CachedRun, ShapingCache, SpanStyleDecoration};
use crate::layout::SpanStyle;
use crate::sugarloaf::primitives::SugarCursor;
use crate::{Graphic, GraphicId};
use std::hash::Hasher;
use wyhash::WyHash;

/// Compute a cache key from glyph IDs, font_id and size.
/// Position-independent: same glyphs at different screen positions produce the same key.
#[inline]
fn compute_cache_key(glyphs: &[GlyphData], font_id: usize, size: f32) -> u64 {
    let mut hasher = WyHash::with_seed(0);
    for (i, g) in glyphs.iter().enumerate() {
        hasher.write_u16(g.simple_data().0);
        hasher.write_usize(i);
    }
    hasher.write_usize(glyphs.len());
    hasher.write_usize(font_id);
    hasher.write_u32((size * 100.0) as u32);
    hasher.finish()
}

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
    #[cfg(not(target_os = "macos"))]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn push_run(
        &mut self,
        style: SpanStyle,
        size: f32,
        line: u32,
        shaper: Shaper<'_>,
        shaping_cache: &mut ShapingCache,
    ) {
        let metrics = shaper.metrics();

        let mut glyphs = vec![];
        let mut detailed_glyphs = vec![];
        let mut advance = 0.;

        shaper.shape_with(|c| {
            let mut cluster_advance = 0.;
            for glyph in c.glyphs {
                cluster_advance += glyph.advance;
                const MAX_SIMPLE_ADVANCE: u32 = 0x7FFF;
                if glyph.x == 0. && glyph.y == 0. {
                    let packed_advance = (glyph.advance * 64.) as u32;
                    if packed_advance <= MAX_SIMPLE_ADVANCE {
                        glyphs.push(GlyphData {
                            data: glyph.id as u32 | (packed_advance << 16),
                            size: glyph.data,
                        });
                        continue;
                    }
                }
                let detail_index = detailed_glyphs.len() as u32;
                detailed_glyphs.push(Glyph::new(glyph));
                glyphs.push(GlyphData {
                    data: GLYPH_DETAILED | detail_index,
                    size: glyph.data,
                });
            }
            advance += cluster_advance;
        });

        if let Some(graphic) = style.media {
            self.graphics.insert(graphic.id);
        }

        let cache_key = compute_cache_key(&glyphs, style.font_id, size);

        // Store pre-packed run in shaping cache
        shaping_cache.finish_with_run(CachedRun {
            glyphs: glyphs.clone(),
            detailed_glyphs: detailed_glyphs.clone(),
            advance,
            cache_key,
        });

        let run_data = RunData {
            span: style,
            line,
            size,
            detailed_glyphs,
            glyphs,
            ascent: metrics.ascent,
            descent: metrics.descent,
            leading: metrics.leading,
            underline_offset: metrics.underline_offset,
            strikeout_offset: metrics.strikeout_offset,
            strikeout_size: metrics.stroke_size,
            x_height: metrics.x_height,
            advance,
            cache_key,
        };
        self.runs.push(run_data);
    }

    /// macOS equivalent of `push_run`: consumes a pre-shaped slice from
    /// CoreText instead of running the swash `Shaper` callback.
    ///
    /// Packing / cache-fill / `RunData` layout are byte-identical to the
    /// swash path, so the cache-hit path (`push_cached_run`) and downstream
    /// composition don't care which shaper produced the glyphs.
    ///
    /// `metrics` comes from [`crate::font::macos::font_metrics`] — CoreText
    /// native ascent/descent/leading/underline, plus strikeout derived from
    /// x-height (CT has no strikeout API).
    #[cfg(target_os = "macos")]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn push_run_macos(
        &mut self,
        style: SpanStyle,
        size: f32,
        line: u32,
        shaped: &[crate::font::macos::ShapedGlyph],
        metrics: &crate::font::macos::FontMetrics,
        shaping_cache: &mut ShapingCache,
    ) {
        let mut glyphs = Vec::with_capacity(shaped.len());
        let mut detailed_glyphs = Vec::new();
        let mut advance = 0.0f32;

        for g in shaped {
            advance += g.advance;
            const MAX_SIMPLE_ADVANCE: u32 = 0x7FFF;
            if g.x == 0.0 && g.y == 0.0 {
                let packed_advance = (g.advance * 64.0) as u32;
                if packed_advance <= MAX_SIMPLE_ADVANCE {
                    glyphs.push(GlyphData {
                        data: g.id as u32 | (packed_advance << 16),
                        size: g.cluster,
                    });
                    continue;
                }
            }
            let detail_index = detailed_glyphs.len() as u32;
            detailed_glyphs.push(Glyph {
                id: g.id,
                x: g.x,
                y: g.y,
                advance: g.advance,
                span: g.cluster as usize,
            });
            glyphs.push(GlyphData {
                data: GLYPH_DETAILED | detail_index,
                size: g.cluster,
            });
        }

        if let Some(graphic) = style.media {
            self.graphics.insert(graphic.id);
        }

        let cache_key = compute_cache_key(&glyphs, style.font_id, size);

        shaping_cache.finish_with_run(CachedRun {
            glyphs: glyphs.clone(),
            detailed_glyphs: detailed_glyphs.clone(),
            advance,
            cache_key,
        });

        let run_data = RunData {
            span: style,
            line,
            size,
            detailed_glyphs,
            glyphs,
            ascent: metrics.ascent,
            descent: metrics.descent,
            leading: metrics.leading,
            underline_offset: metrics.underline_offset,
            strikeout_offset: metrics.strikeout_offset,
            strikeout_size: metrics.strikeout_thickness,
            x_height: metrics.x_height,
            advance,
            cache_key,
        };
        self.runs.push(run_data);
    }

    /// Push a pre-packed cached run — no repacking, no hashing.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn push_cached_run(
        &mut self,
        style: SpanStyle,
        size: f32,
        line: u32,
        cached: &CachedRun,
        ascent: f32,
        descent: f32,
        leading: f32,
    ) {
        if let Some(graphic) = style.media {
            self.graphics.insert(graphic.id);
        }
        let run_data = RunData {
            span: style,
            line,
            size,
            glyphs: cached.glyphs.clone(),
            detailed_glyphs: cached.detailed_glyphs.clone(),
            ascent,
            descent,
            leading,
            underline_offset: 0.,
            strikeout_offset: 0.,
            strikeout_size: 0.,
            x_height: 0.,
            advance: cached.advance,
            cache_key: cached.cache_key,
        };
        self.runs.push(run_data);
    }

    #[cfg(test)]
    pub(super) fn push_run_without_shaper(
        &mut self,
        style: SpanStyle,
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
                            size: glyph.data,
                        });
                        continue;
                    }
                }
                // Complex glyph
                let detail_index = detailed_glyphs.len() as u32;
                detailed_glyphs.push(Glyph::new(glyph));
                glyphs.push(GlyphData {
                    data: GLYPH_DETAILED | detail_index,
                    size: glyph.data,
                });
            }
            advance += cluster_advance;
        }
        if let Some(graphic) = style.media {
            self.graphics.insert(graphic.id);
        }
        let cache_key = compute_cache_key(&glyphs, style.font_id, size);
        let run_data = RunData {
            span: style,
            line,
            size,
            detailed_glyphs,
            glyphs,
            ascent: metrics.ascent,
            descent: metrics.descent,
            leading: metrics.leading,
            underline_offset: metrics.underline_offset,
            strikeout_offset: metrics.strikeout_offset,
            strikeout_size: metrics.stroke_size,
            x_height: metrics.x_height,
            advance,
            cache_key,
        };
        self.runs.push(run_data);
        true
    }

    /// Push an empty run that advances position without any glyphs.
    /// Used for unwritten cells ('\0') that need to occupy space.
    pub(super) fn push_empty_run(
        &mut self,
        style: SpanStyle,
        size: f32,
        line: u32,
        metrics: &Metrics,
    ) {
        let run_data = RunData {
            span: style,
            line,
            size,
            detailed_glyphs: vec![],
            glyphs: vec![],
            ascent: metrics.ascent,
            descent: metrics.descent,
            leading: metrics.leading,
            underline_offset: metrics.underline_offset,
            strikeout_offset: metrics.strikeout_offset,
            strikeout_size: metrics.stroke_size,
            x_height: metrics.x_height,
            advance: 0.,
            cache_key: 0,
        };
        self.runs.push(run_data);
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
    pub fn span(&self) -> SpanStyle {
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
    pub fn decoration(&self) -> Option<SpanStyleDecoration> {
        self.run.span.decoration
    }

    #[inline]
    pub fn decoration_color(&self) -> Option<[f32; 4]> {
        self.run.span.decoration_color
    }
}
