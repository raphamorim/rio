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

use super::layout_data::*;
use super::line_breaker::BreakLines;
use super::Direction;
use crate::font::{
    Style, Weight, FONT_ID_BOLD, FONT_ID_BOLD_ITALIC, FONT_ID_ITALIC, FONT_ID_REGULAR,
};
use crate::layout::FragmentStyle;
use crate::sugarloaf::primitives::SugarCursor;
use core::iter::DoubleEndedIterator;
use core::ops::Range;
use swash::shape::{cluster::Glyph as ShapedGlyph, Shaper};
use swash::text::cluster::{Boundary, ClusterInfo};
use swash::{GlyphId, NormalizedCoord};

/// Collection of text, organized into lines, runs and clusters.
#[derive(Clone, Debug, Default)]
pub struct RenderData {
    pub data: LayoutData,
    last_line: u32,
    pub last_cached_run: RunCacheEntry,
    pub line_data: LineLayoutData,
}

impl RenderData {
    pub fn dump_clusters(&self) {
        for (i, cluster) in self.line_data.clusters.iter().enumerate() {
            println!("[{}] {} @ {}", i, cluster.0, cluster.1);
        }
    }
    /// Creates a new empty paragraph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears the current line state and returns a line breaker
    /// for the paragraph.
    #[inline]
    pub fn break_lines(&mut self) -> BreakLines {
        self.line_data.clear();
        BreakLines::new(&mut self.data, &mut self.line_data)
    }

    /// Returns an iterator over the lines in the paragraph.
    #[inline]
    pub fn lines(&self) -> Lines {
        Lines {
            layout: &self.data,
            line_layout: &self.line_data,
            iter: self.line_data.lines.iter(),
        }
    }

    /// Clears the paragraph.
    #[inline]
    pub fn clear(&mut self) {
        self.data.clear();
        self.line_data.clear();
    }
}

#[derive(Debug, Clone)]
pub struct CachedClusterData {
    pub info: ClusterInfo,
    pub flags: u8,
    /// Length of the cluster in the source text.
    pub len: u8,
    /// Offset of the cluster in the source text.
    pub offset: u32,
    /// Depending on `flags`, either an index into `glyphs` or an index
    /// into `detailed_clusters`
    pub glyphs: Vec<GlyphData>,
    pub details: Vec<DetailedClusterData>,
}

#[derive(Debug, Clone)]
pub struct CachedRunData {
    pub clusters: Vec<CachedClusterData>,
    pub coords: Vec<i16>,
    pub span: FragmentStyle,
    pub line: u32,
    pub font: usize,
    pub size: f32,
    pub level: u8,
    pub whitespace: bool,
    pub trailing_whitespace: bool,
    pub ascent: f32,
    pub descent: f32,
    pub leading: f32,
    pub strikeout_offset: f32,
    pub strikeout_size: f32,
    pub advance: f32,
}

#[derive(Clone, Default, Debug)]
pub struct RunCacheEntry {
    pub runs: Vec<CachedRunData>,
}

impl RenderData {
    pub(super) fn push_run_from_cached_line(
        &mut self,
        cached_entry: &RunCacheEntry,
        line: u32,
    ) {
        // Every time a line is cached we need to rebuild the indexes
        // so RunData, Clusters, DetailedClusterData and Glyphs need to be
        // pointed correctly across each other otherwise will lead to panic
        for cached_run in &cached_entry.runs {
            let coords_start = self.data.coords.len() as u32;
            self.data.coords.extend_from_slice(&cached_run.coords);
            let coords_end = self.data.coords.len() as u32;

            let clusters_start = self.data.clusters.len() as u32;
            for cached_cluster in &cached_run.clusters {
                let mut glyphs_start = self.data.glyphs.len() as u32;
                for glyph_data in &cached_cluster.glyphs {
                    self.data.glyphs.push(*glyph_data);
                }
                let glyphs_end = self.data.glyphs.len() as u32;

                let detailed_len = self.data.detailed_clusters.len() as u32;
                for detail in &cached_cluster.details {
                    self.data.detailed_clusters.push(DetailedClusterData {
                        glyphs: (glyphs_start, glyphs_end),
                        advance: detail.advance,
                    });
                }

                if !cached_cluster.details.is_empty() {
                    glyphs_start = detailed_len;
                }

                self.data.clusters.push(ClusterData {
                    info: cached_cluster.info,
                    flags: cached_cluster.flags,
                    len: cached_cluster.len,
                    offset: cached_cluster.offset,
                    glyphs: glyphs_start,
                });
            }
            let clusters_end = self.data.clusters.len() as u32;

            self.data.runs.push(RunData {
                coords: (coords_start, coords_end),
                clusters: (clusters_start, clusters_end),
                span: cached_run.span,
                line,
                font: cached_run.font,
                size: cached_run.size,
                level: cached_run.level,
                whitespace: false,
                trailing_whitespace: false,
                ascent: cached_run.ascent,
                descent: cached_run.descent,
                leading: cached_run.leading,
                strikeout_offset: cached_run.strikeout_offset,
                strikeout_size: cached_run.strikeout_size,
                advance: cached_run.advance,
            });
        }

        self.data.last_span = 0;
    }

    pub(super) fn push_run(
        &mut self,
        styles: &[FragmentStyle],
        font: &usize,
        size: f32,
        level: u8,
        line: u32,
        shaper: Shaper<'_>,
    ) {
        // In case is a new line,
        // then needs to recompute the span index again
        if line != self.last_line {
            self.last_line = line;
            self.data.last_span = 0;
            self.last_cached_run.runs.clear();
        }

        let coords_start = self.data.coords.len() as u32;
        let coords = shaper.normalized_coords().to_owned();
        self.data.coords.extend_from_slice(&coords);

        let coords_end = self.data.coords.len() as u32;
        let mut clusters_start = self.data.clusters.len() as u32;
        let metrics = shaper.metrics();

        let mut advance = 0.;
        let mut last_span = self.data.last_span;
        let mut span_data = &styles[last_span];

        shaper.shape_with(|c| {
            if c.info.boundary() == Boundary::Mandatory {
                if let Some(c) = self.data.clusters.last_mut() {
                    c.flags |= CLUSTER_NEWLINE;
                }
            }
            let span = c.data;
            if span as usize != last_span {
                span_data = &styles[last_span];
                // Ensure that every run belongs to a single span.
                let clusters_end = self.data.clusters.len() as u32;
                if clusters_end != clusters_start {
                    let run_data = RunData {
                        span: styles[last_span],
                        line,
                        font: *font,
                        coords: (coords_start, coords_end),
                        size,
                        level,
                        whitespace: false,
                        trailing_whitespace: false,
                        clusters: (clusters_start, clusters_end),
                        ascent: metrics.ascent * span_data.line_spacing,
                        descent: metrics.descent * span_data.line_spacing,
                        leading: metrics.leading * span_data.line_spacing,
                        strikeout_offset: metrics.strikeout_offset,
                        strikeout_size: metrics.stroke_size,
                        advance,
                    };
                    self.data.runs.push(run_data);
                    let mut owned_clusters = Vec::with_capacity(
                        (clusters_end - clusters_start).try_into().unwrap(),
                    );
                    for current_cluster in &self.data.clusters
                        [clusters_start as usize..clusters_end as usize]
                    {
                        let mut detailed_clusters = Vec::with_capacity(2);
                        let glyphs_data = if current_cluster.is_detailed() {
                            let detail = &self.data.detailed_clusters
                                [current_cluster.glyphs as usize];
                            detailed_clusters.push(*detail);
                            &self.data.glyphs
                                [detail.glyphs.0 as usize..detail.glyphs.1 as usize]
                        } else if current_cluster.is_empty() {
                            &[]
                        } else {
                            &self.data.glyphs[current_cluster.glyphs as usize
                                ..current_cluster.glyphs as usize + 1]
                        };
                        owned_clusters.push(CachedClusterData {
                            info: current_cluster.info,
                            flags: current_cluster.flags,
                            len: current_cluster.len,
                            offset: current_cluster.offset,
                            glyphs: glyphs_data.to_vec(),
                            details: detailed_clusters,
                        });
                    }
                    self.last_cached_run.runs.push(CachedRunData {
                        span: styles[last_span],
                        line,
                        font: *font,
                        coords: coords.to_owned(),
                        size,
                        level,
                        whitespace: false,
                        trailing_whitespace: false,
                        clusters: owned_clusters,
                        ascent: metrics.ascent * span_data.line_spacing,
                        descent: metrics.descent * span_data.line_spacing,
                        leading: metrics.leading * span_data.line_spacing,
                        strikeout_offset: metrics.strikeout_offset,
                        strikeout_size: metrics.stroke_size,
                        advance,
                    });
                    clusters_start = clusters_end;
                }
                last_span = span as usize;
            }
            let mut glyphs_start = self.data.glyphs.len() as u32;
            let mut cluster_advance = 0.;
            for glyph in c.glyphs {
                cluster_advance += glyph.advance;
                self.push_glyph(glyph);
            }
            advance += cluster_advance;
            let mut component_advance = cluster_advance;
            let is_ligature = c.components.len() > 1;
            let (len, base_flags) = if is_ligature {
                let x = &c.components[0];
                component_advance /= c.components.len() as f32;
                ((x.end - x.start) as u8, CLUSTER_LIGATURE)
            } else {
                ((c.source.end - c.source.start) as u8, 0)
            };
            let glyphs_end = self.data.glyphs.len() as u32;
            if glyphs_end - glyphs_start > 1 || is_ligature {
                let detail_index = self.data.detailed_clusters.len() as u32;
                self.data.detailed_clusters.push(DetailedClusterData {
                    glyphs: (glyphs_start, glyphs_end),
                    advance: component_advance,
                });
                let cluster = ClusterData {
                    info: c.info,
                    flags: base_flags | CLUSTER_DETAILED,
                    len,
                    offset: c.source.start,
                    glyphs: detail_index,
                };
                self.data.clusters.push(cluster);
            } else {
                let flags = if glyphs_start == glyphs_end {
                    glyphs_start = c.data;
                    CLUSTER_EMPTY
                } else {
                    base_flags
                };
                let cluster = ClusterData {
                    info: c.info,
                    flags,
                    len,
                    offset: c.source.start,
                    glyphs: glyphs_start,
                };
                self.data.clusters.push(cluster);
            }
            if base_flags != 0 {
                // Emit continuations
                for component in &c.components[1..] {
                    let cluster = ClusterData {
                        info: Default::default(),
                        flags: CLUSTER_CONTINUATION | CLUSTER_EMPTY,
                        len: (component.end - component.start) as u8,
                        offset: component.start,
                        glyphs: component_advance.to_bits(),
                    };
                    self.data.clusters.push(cluster);
                }

                if let Some(c) = self.data.clusters.last_mut() {
                    c.flags |= CLUSTER_LAST_CONTINUATION
                }
            }
        });
        let clusters_end = self.data.clusters.len() as u32;
        if clusters_end == clusters_start {
            return;
        }
        self.data.last_span = last_span;
        let run_data = RunData {
            span: styles[last_span],
            line,
            font: *font,
            coords: (coords_start, coords_end),
            size,
            level,
            whitespace: false,
            trailing_whitespace: false,
            clusters: (clusters_start, clusters_end),
            ascent: metrics.ascent * span_data.line_spacing,
            descent: metrics.descent * span_data.line_spacing,
            leading: metrics.leading * span_data.line_spacing,
            strikeout_offset: metrics.strikeout_offset,
            strikeout_size: metrics.stroke_size,
            advance,
        };
        self.data.runs.push(run_data);
        let mut owned_clusters =
            Vec::with_capacity((clusters_end - clusters_start).try_into().unwrap());
        for current_cluster in
            &self.data.clusters[clusters_start as usize..clusters_end as usize]
        {
            let mut detailed_clusters = Vec::with_capacity(2);
            let glyphs_data = if current_cluster.is_detailed() {
                let detail =
                    &self.data.detailed_clusters[current_cluster.glyphs as usize];
                detailed_clusters.push(*detail);
                &self.data.glyphs[detail.glyphs.0 as usize..detail.glyphs.1 as usize]
            } else if current_cluster.is_empty() {
                &[]
            } else {
                &self.data.glyphs
                    [current_cluster.glyphs as usize..current_cluster.glyphs as usize + 1]
            };
            owned_clusters.push(CachedClusterData {
                info: current_cluster.info,
                flags: current_cluster.flags,
                len: current_cluster.len,
                offset: current_cluster.offset,
                glyphs: glyphs_data.to_vec(),
                details: detailed_clusters,
            });
        }
        self.last_cached_run.runs.push(CachedRunData {
            span: styles[last_span],
            line,
            font: *font,
            coords: coords.to_owned(),
            size,
            level,
            whitespace: false,
            trailing_whitespace: false,
            clusters: owned_clusters,
            ascent: metrics.ascent * span_data.line_spacing,
            descent: metrics.descent * span_data.line_spacing,
            leading: metrics.leading * span_data.line_spacing,
            strikeout_offset: metrics.strikeout_offset,
            strikeout_size: metrics.stroke_size,
            advance,
        });
    }

    #[inline]
    fn push_glyph(&mut self, glyph: &ShapedGlyph) {
        const MAX_SIMPLE_ADVANCE: u32 = 0x7FFF;
        if glyph.x == 0. && glyph.y == 0. {
            let packed_advance = (glyph.advance * 64.) as u32;
            if packed_advance <= MAX_SIMPLE_ADVANCE {
                // Simple glyph
                self.data.glyphs.push(GlyphData {
                    data: glyph.id as u32 | (packed_advance << 16),
                    size: glyph.data as usize,
                });
                return;
            }
        }
        // Complex glyph
        let detail_index = self.data.detailed_glyphs.len() as u32;
        self.data.detailed_glyphs.push(Glyph::new(glyph));
        self.data.glyphs.push(GlyphData {
            data: GLYPH_DETAILED | detail_index,
            size: glyph.data as usize,
        });
    }

    pub(super) fn apply_spacing(&mut self) {
        for run in &mut self.data.runs {
            let word = run.span.word_spacing;
            let letter = run.span.letter_spacing;
            if word == 0. && letter == 0. {
                continue;
            }
            let clusters =
                &mut self.data.clusters[run.clusters.0 as usize..run.clusters.1 as usize];
            for cluster in clusters {
                let mut spacing = letter;
                if word != 0. && cluster.info.whitespace().is_space_or_nbsp() {
                    spacing += word;
                }
                if spacing != 0. {
                    let detailed_glyphs = &mut self.data.detailed_glyphs[..];
                    if cluster.is_detailed() && !cluster.is_ligature() {
                        self.data.detailed_clusters[cluster.glyphs as usize].advance +=
                            spacing;
                    } else if cluster.is_last_continuation() {
                        cluster.glyphs =
                            (f32::from_bits(cluster.glyphs) + spacing).to_bits();
                    }
                    if let Some(g) = cluster
                        .glyphs_mut(&self.data.detailed_clusters, &mut self.data.glyphs)
                        .last_mut()
                    {
                        if g.is_simple() {
                            g.add_spacing(spacing);
                        } else {
                            detailed_glyphs[g.detail_index()].advance += spacing;
                        }
                        run.advance += spacing;
                    }
                }
            }
        }
    }

    pub(super) fn finish(&mut self) {
        // Zero out the advance for the extra trailing space.
        self.data.glyphs.last_mut().unwrap().clear_advance();
    }
}

/// Sequence of clusters sharing the same font, size and span.
#[derive(Copy, Clone)]
pub struct Run<'a> {
    layout: &'a LayoutData,
    pub(super) run: &'a RunData,
}

impl<'a> Run<'a> {
    pub(super) fn new(layout: &'a LayoutData, run: &'a RunData) -> Self {
        Self { layout, run }
    }
    /// Returns the span that contains the run.
    #[inline]
    pub fn span(&self) -> FragmentStyle {
        self.run.span
    }

    #[inline]
    pub fn font_id_based_on_attr(&self) -> usize {
        let is_italic = self.run.span.font_attrs.2 == Style::Italic;
        let is_bold = self.run.span.font_attrs.1 == Weight::BOLD;

        if is_bold && is_italic {
            return FONT_ID_BOLD_ITALIC;
        } else if is_bold {
            return FONT_ID_BOLD;
        } else if is_italic {
            return FONT_ID_ITALIC;
        }

        FONT_ID_REGULAR
    }

    /// Returns the font for the run.
    #[inline]
    pub fn font(&self) -> &usize {
        &self.run.font
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

    /// Returns the bidi level of the run.
    #[inline]
    pub fn level(&self) -> u8 {
        self.run.level
    }

    /// Returns the cursor
    #[inline]
    pub fn cursor(&self) -> SugarCursor {
        self.run.span.cursor
    }

    /// Returns the direction of the run.
    pub fn direction(&self) -> Direction {
        if self.run.level & 1 != 0 {
            Direction::RightToLeft
        } else {
            Direction::LeftToRight
        }
    }

    /// Returns the normalized variation coordinates for the run.
    pub fn normalized_coords(&self) -> &'a [NormalizedCoord] {
        self.layout
            .coords
            .get(make_range(self.run.coords))
            .unwrap_or(&[])
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
    pub fn underline(&self) -> bool {
        self.run.span.underline
    }

    /// Returns the underline offset for the run.
    #[inline]
    pub fn underline_offset(&self) -> f32 {
        // span_data.underline_offset.unwrap_or(metrics.underline_offset),
        self.run.span.underline_offset.unwrap_or(0.0)
    }

    /// Returns the underline color for the run.
    #[inline]
    pub fn underline_color(&self) -> [f32; 4] {
        self.run.span.underline_color.unwrap_or(self.run.span.color)
    }

    /// Returns the underline size for the run.
    #[inline]
    pub fn underline_size(&self) -> f32 {
        self.run
            .span
            .underline_size
            .unwrap_or(self.run.strikeout_size)
    }

    /// Returns an iterator over the clusters in logical order.
    #[inline]
    pub fn clusters(&self) -> Clusters<'a> {
        Clusters {
            layout: self.layout,
            iter: self.layout.clusters[make_range(self.run.clusters)].iter(),
            rev: false,
        }
    }

    /// Returns an iterator over the clusters in visual order.
    #[inline]
    pub fn visual_clusters(&self) -> Clusters<'a> {
        let rev = self.run.level & 1 != 0;
        Clusters {
            layout: self.layout,
            iter: self.layout.clusters[make_range(self.run.clusters)].iter(),
            rev,
        }
    }
}

/// Iterator over the runs in a paragraph.
#[derive(Clone)]
pub struct Runs<'a> {
    layout: &'a LayoutData,
    iter: core::slice::Iter<'a, RunData>,
}

impl<'a> Iterator for Runs<'a> {
    type Item = Run<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let run = self.iter.next()?;
        Some(Run {
            layout: self.layout,
            run,
        })
    }
}

/// Shaped glyph in a paragraph.
#[derive(Copy, Debug, Clone)]
pub struct Glyph {
    /// Glyph identifier.
    pub id: GlyphId,
    /// Horizontal offset.
    pub x: f32,
    /// Vertical offset.
    pub y: f32,
    /// Advance width or height.
    pub advance: f32,
    /// Span that generated the glyph.
    pub span: usize,
}

impl Glyph {
    fn new(g: &ShapedGlyph) -> Self {
        Self {
            id: g.id,
            x: g.x,
            y: g.y,
            advance: g.advance,
            span: g.data as usize,
        }
    }
}

/// Iterator over a sequence of glyphs in a cluster.
#[derive(Clone)]
pub struct Glyphs<'a> {
    layout: &'a LayoutData,
    iter: core::slice::Iter<'a, GlyphData>,
}

impl<'a> Iterator for Glyphs<'a> {
    type Item = Glyph;

    fn next(&mut self) -> Option<Self::Item> {
        let data = self.iter.next()?;
        if data.is_simple() {
            let (id, advance) = data.simple_data();
            Some(Glyph {
                id,
                x: 0.,
                y: 0.,
                advance,
                span: id as usize,
            })
        } else {
            self.layout
                .detailed_glyphs
                .get(data.detail_index())
                .copied()
        }
    }
}

impl<'a> DoubleEndedIterator for Glyphs<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let data = self.iter.next_back()?;
        if data.is_simple() {
            let (id, advance) = data.simple_data();
            Some(Glyph {
                id,
                x: 0.,
                y: 0.,
                advance,
                span: id as usize,
            })
        } else {
            self.layout
                .detailed_glyphs
                .get(data.detail_index())
                .copied()
        }
    }
}

/// Collection of glyphs representing an atomic textual unit.
#[derive(Copy, Clone)]
pub struct Cluster<'a> {
    layout: &'a LayoutData,
    cluster: ClusterData,
}

impl<'a> Cluster<'a> {
    pub(super) fn new(layout: &'a LayoutData, cluster: ClusterData) -> Self {
        Self { layout, cluster }
    }

    /// Returns the cluster information.
    #[inline]
    pub fn info(&self) -> ClusterInfo {
        self.cluster.info
    }

    /// Returns true if the cluster is empty. This occurs when ignorable
    /// glyphs are removed by the shaper.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.cluster.is_empty()
    }

    /// Returns true if the cluster is a ligature.
    #[inline]
    pub fn is_ligature(&self) -> bool {
        self.cluster.is_ligature()
    }

    /// Returns true if the cluster is a continuation of a ligature.
    #[inline]
    pub fn is_continuation(&self) -> bool {
        self.cluster.is_continuation()
    }

    /// Returns true if the cluster is the final continuation of a ligature.
    #[inline]
    pub fn is_last_continuation(&self) -> bool {
        self.cluster.is_last_continuation()
    }

    /// Returns true if the following cluster is a mandatory line break.
    #[inline]
    pub fn is_newline(&self) -> bool {
        self.cluster.is_newline()
    }

    /// Returns the byte offset of the cluster in the source text.
    #[inline]
    pub fn offset(&self) -> usize {
        self.cluster.offset as usize
    }

    /// Returns the byte range of the cluster in the source text.
    #[inline]
    pub fn range(&self) -> Range<usize> {
        let start = self.cluster.offset as usize;
        start..start + self.cluster.len as usize
    }

    /// Returns an iterator over the glyphs for the cluster.
    #[inline]
    pub fn glyphs(&self) -> Glyphs<'a> {
        let glyphs = self
            .cluster
            .glyphs(&self.layout.detailed_clusters, &self.layout.glyphs);
        Glyphs {
            layout: self.layout,
            iter: glyphs.iter(),
        }
    }

    /// Returns the advance of the cluster.
    #[inline]
    pub fn advance(&self) -> f32 {
        self.cluster.advance(
            &self.layout.detailed_clusters,
            &self.layout.glyphs,
            &self.layout.detailed_glyphs,
        )
    }
}

/// Iterator over the clusters in a run.
#[derive(Clone)]
pub struct Clusters<'a> {
    layout: &'a LayoutData,
    iter: core::slice::Iter<'a, ClusterData>,
    rev: bool,
}

impl<'a> Iterator for Clusters<'a> {
    type Item = Cluster<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let data = if self.rev {
            self.iter.next_back()?
        } else {
            self.iter.next()?
        };
        Some(Cluster {
            layout: self.layout,
            cluster: *data,
        })
    }
}

impl<'a> DoubleEndedIterator for Clusters<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let data = self.iter.next_back()?;
        Some(Cluster {
            layout: self.layout,
            cluster: *data,
        })
    }
}

/// Collection of runs occupying a single line in a paragraph.
#[derive(Copy, Clone)]
pub struct Line<'a> {
    layout: &'a LayoutData,
    line_layout: &'a LineLayoutData,
    line: &'a LineData,
}

impl<'a> Line<'a> {
    // pub(super) fn new(layout: &'a RenderData, line_index: usize) -> Self {
    //     Self {
    //         layout: &layout.data,
    //         line_layout: &layout.line_data,
    //         line: &layout.line_data.lines[line_index],
    //     }
    // }

    /// Returns the offset in line direction.
    #[inline]
    pub fn offset(&self) -> f32 {
        self.line.x
    }

    /// Returns the baseline offset.
    #[inline]
    pub fn baseline(&self) -> f32 {
        self.line.baseline
    }

    /// Returns the ascent of the line.
    #[inline]
    pub fn ascent(&self) -> f32 {
        self.line.ascent
    }

    /// Returns the descent of the line.
    #[inline]
    pub fn descent(&self) -> f32 {
        self.line.descent
    }

    /// Returns the leading of the line.
    #[inline]
    pub fn leading(&self) -> f32 {
        self.line.leading
    }

    /// Returns the total advance of the line.
    #[inline]
    pub fn advance(&self) -> f32 {
        self.line.width
    }

    /// Returns the total advance of the line excluding trailing whitespace.
    pub fn advance_without_trailing_whitespace(&self) -> f32 {
        let mut advance = self.line.width;
        for run in self.line_layout.runs[make_range(self.line.runs)]
            .iter()
            .rev()
        {
            if !run.trailing_whitespace {
                break;
            }
            for cluster in self.layout.clusters[make_range(run.clusters)].iter().rev() {
                if !cluster.info.is_whitespace() {
                    break;
                }
                advance -= Cluster {
                    layout: self.layout,
                    cluster: *cluster,
                }
                .advance();
            }
        }
        advance
    }

    /// Returns the size of the line (height for horizontal and width
    /// for vertical layouts).
    #[inline]
    pub fn size(&self) -> f32 {
        self.line.ascent + self.line.descent + self.line.leading
    }

    /// Returns an iterator over the runs of the line.
    #[inline]
    pub fn runs(&self) -> Runs<'a> {
        let range = self.line.runs.0 as usize..self.line.runs.1 as usize;
        Runs {
            layout: self.layout,
            iter: self.line_layout.runs[range].iter(),
        }
    }

    // pub(super) fn data(&self) -> &'a LineData {
    //     self.line
    // }
}

/// Iterator over the lines of a paragraph.
#[derive(Clone)]
pub struct Lines<'a> {
    layout: &'a LayoutData,
    line_layout: &'a LineLayoutData,
    iter: core::slice::Iter<'a, LineData>,
}

impl<'a> Iterator for Lines<'a> {
    type Item = Line<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let line = self.iter.next()?;
        Some(Line {
            layout: self.layout,
            line_layout: self.line_layout,
            line,
        })
    }
}

#[inline]
pub fn make_range(r: (u32, u32)) -> Range<usize> {
    r.0 as usize..r.1 as usize
}
