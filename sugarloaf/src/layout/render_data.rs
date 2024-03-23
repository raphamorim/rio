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

#[derive(Clone, Default, Debug)]
pub struct RunCacheEntry {
    /// Normalized variation coordinates.
    pub coords: Vec<i16>,
    /// Simple glyphs.
    pub glyphs: Vec<ShapedGlyph>,
    /// Glyph runs.
    pub runs: Vec<RunData>,
    /// Last shaped span.
    pub clusters: Vec<ClusterData>,
    pub clusters_marks: Vec<u32>,
}

impl RenderData {
    pub(super) fn push_run_from_cached_line(&mut self, cached_entry: &RunCacheEntry) {
        let mut i = 0;
        let mut g = 0;

        // TO_DEBUG_CACHE:
        // let glyphs_start = self.data.glyphs.len() as u32;
        // let clusters_start = self.data.clusters.len() as u32;
        // println!("\nCACHE");
        // println!("current glyphs_start {}", glyphs_start);
        // println!("current clusters_start {}", clusters_start);
        // println!("first run expected glyphs_start {:?}", cached_entry.clusters[0].glyphs);
        // println!("first run expected cluster_start {:?}", cached_entry.runs[0].clusters.0);
        // println!("last run expected cluster_start {:?}", cached_entry.clusters[cached_entry.runs.len() - 1].glyphs);
        // println!("last run expected cluster_start {:?}", cached_entry.runs[cached_entry.runs.len() - 1].clusters.0);

        // println!("cache run length {:?}", cached_entry.runs.len());

        for run in &cached_entry.runs {
            let clusters_start = self.data.clusters.len() as u32;

            for _ in run.clusters.0..run.clusters.1 {
                let glyphs_start = self.data.glyphs.len() as u32;
                let mut cluster = cached_entry.clusters[i].to_owned();

                for _ in 0..cached_entry.clusters_marks[i] {
                    if g < cached_entry.glyphs.len() {
                        self.push_glyph(&cached_entry.glyphs[g]);
                        g += 1;
                    }
                }

                cluster.glyphs = glyphs_start;

                self.data.clusters.push(cluster);
                i += 1;
            }
            let clusters_end = self.data.clusters.len() as u32;

            let coords_start = self.data.coords.len() as u32;
            // for _ in run.coords.0..run.coords.1 {
            self.data.coords.extend_from_slice(&cached_entry.coords);
            // }
            let coords_end = self.data.coords.len() as u32;

            let mut run = run.to_owned();
            run.coords = (coords_start, coords_end);
            run.clusters = (clusters_start, clusters_end);

            self.data.runs.push(run);
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
            self.last_cached_run = RunCacheEntry::default();
        }

        let coords_start = self.data.coords.len() as u32;
        let coords = shaper.normalized_coords().to_owned();
        self.data.coords.extend_from_slice(&coords);
        self.last_cached_run.coords.extend_from_slice(&coords);

        let coords_end = self.data.coords.len() as u32;
        let mut clusters_start = self.data.clusters.len() as u32;
        let metrics = shaper.metrics();
        let mut advance = 0.;

        let mut last_span = self.data.last_span;
        let mut span_data = &styles[self.data.last_span];

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
                        color: span_data.color,
                        background_color: span_data.background_color,
                        size,
                        level,
                        whitespace: false,
                        trailing_whitespace: false,
                        clusters: (clusters_start, clusters_end),
                        ascent: metrics.ascent * span_data.line_spacing,
                        descent: metrics.descent * span_data.line_spacing,
                        leading: metrics.leading * span_data.line_spacing,
                        cursor: span_data.cursor,
                        underline: span_data.underline,
                        underline_color: span_data
                            .underline_color
                            .unwrap_or(span_data.color),
                        underline_offset: span_data
                            .underline_offset
                            .unwrap_or(metrics.underline_offset),
                        underline_size: span_data
                            .underline_size
                            .unwrap_or(metrics.stroke_size),
                        strikeout_offset: metrics.strikeout_offset,
                        strikeout_size: metrics.stroke_size,
                        advance,
                    };
                    self.data.runs.push(run_data);
                    self.last_cached_run.runs.push(run_data);
                    clusters_start = clusters_end;
                }
                last_span = span as usize;
            }
            let mut glyphs_start = self.data.glyphs.len() as u32;
            let mut cluster_advance = 0.;
            for glyph in c.glyphs {
                cluster_advance += glyph.advance;
                self.last_cached_run.glyphs.push(*glyph);
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
                self.last_cached_run.clusters.push(cluster);
                self.last_cached_run
                    .clusters_marks
                    .push(glyphs_end - glyphs_start);
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
                self.last_cached_run.clusters.push(cluster);
                self.last_cached_run
                    .clusters_marks
                    .push(glyphs_end - glyphs_start);
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
                    self.last_cached_run.clusters.push(cluster);
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
            span: *span_data,
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
            color: span_data.color,
            background_color: span_data.background_color,
            cursor: span_data.cursor,
            underline: span_data.underline,
            underline_color: span_data.underline_color.unwrap_or(span_data.color),
            underline_offset: span_data
                .underline_offset
                .unwrap_or(metrics.underline_offset),
            underline_size: span_data.underline_size.unwrap_or(metrics.stroke_size),
            strikeout_offset: metrics.strikeout_offset,
            strikeout_size: metrics.stroke_size,
            advance,
        };
        self.data.runs.push(run_data);
        self.last_cached_run.runs.push(run_data);
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
    pub fn span(&self) -> FragmentStyle {
        self.run.span
    }

    /// Returns the font for the run.
    pub fn font(&self) -> &usize {
        &self.run.font
    }

    /// Returns the font size for the run.
    pub fn font_size(&self) -> f32 {
        self.run.size
    }

    /// Returns the color for the run.
    pub fn color(&self) -> [f32; 4] {
        self.run.color
    }

    /// Returns the bidi level of the run.
    pub fn level(&self) -> u8 {
        self.run.level
    }

    /// Returns the cursor
    pub fn cursor(&self) -> SugarCursor {
        self.run.cursor
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
    pub fn advance(&self) -> f32 {
        self.run.advance
    }

    /// Returns true if the run has an background color
    pub fn background_color(&self) -> Option<[f32; 4]> {
        self.run.background_color
    }

    /// Returns true if the run has an underline decoration.
    pub fn underline(&self) -> bool {
        self.run.underline
    }

    /// Returns the underline offset for the run.
    pub fn underline_offset(&self) -> f32 {
        self.run.underline_offset
    }

    /// Returns the underline color for the run.
    pub fn underline_color(&self) -> [f32; 4] {
        self.run.underline_color
    }

    /// Returns the underline size for the run.
    pub fn underline_size(&self) -> f32 {
        self.run.underline_size
    }

    /// Returns an iterator over the clusters in logical order.
    pub fn clusters(&self) -> Clusters<'a> {
        Clusters {
            layout: self.layout,
            iter: self.layout.clusters[make_range(self.run.clusters)].iter(),
            rev: false,
        }
    }

    /// Returns an iterator over the clusters in visual order.
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
    pub fn info(&self) -> ClusterInfo {
        self.cluster.info
    }

    /// Returns true if the cluster is empty. This occurs when ignorable
    /// glyphs are removed by the shaper.
    pub fn is_empty(&self) -> bool {
        self.cluster.is_empty()
    }

    /// Returns true if the cluster is a ligature.
    pub fn is_ligature(&self) -> bool {
        self.cluster.is_ligature()
    }

    /// Returns true if the cluster is a continuation of a ligature.
    pub fn is_continuation(&self) -> bool {
        self.cluster.is_continuation()
    }

    /// Returns true if the cluster is the final continuation of a ligature.
    pub fn is_last_continuation(&self) -> bool {
        self.cluster.is_last_continuation()
    }

    /// Returns true if the following cluster is a mandatory line break.
    pub fn is_newline(&self) -> bool {
        self.cluster.is_newline()
    }

    /// Returns the byte offset of the cluster in the source text.
    pub fn offset(&self) -> usize {
        self.cluster.offset as usize
    }

    /// Returns the byte range of the cluster in the source text.
    pub fn range(&self) -> Range<usize> {
        let start = self.cluster.offset as usize;
        start..start + self.cluster.len as usize
    }

    /// Returns an iterator over the glyphs for the cluster.
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
    pub fn offset(&self) -> f32 {
        self.line.x
    }

    /// Returns the baseline offset.
    pub fn baseline(&self) -> f32 {
        self.line.baseline
    }

    /// Returns the ascent of the line.
    pub fn ascent(&self) -> f32 {
        self.line.ascent
    }

    /// Returns the descent of the line.
    pub fn descent(&self) -> f32 {
        self.line.descent
    }

    /// Returns the leading of the line.
    pub fn leading(&self) -> f32 {
        self.line.leading
    }

    /// Returns the total advance of the line.
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
    pub fn size(&self) -> f32 {
        self.line.ascent + self.line.descent + self.line.leading
    }

    /// Returns an iterator over the runs of the line.
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

pub fn make_range(r: (u32, u32)) -> Range<usize> {
    r.0 as usize..r.1 as usize
}
