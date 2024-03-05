// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// line_breaker.rs was originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE

use super::layout_data::*;
use super::render_data::*;

/// Alignment of a paragraph.
#[derive(Copy, Default, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Alignment {
    #[default]
    Start,
    Middle,
    End,
}

/// Line breaking support for a paragraph.
pub struct BreakLines<'a> {
    layout: &'a mut LayoutData,
    lines: &'a mut LineLayoutData,
    state: BreakerState,
    prev_state: Option<BreakerState>,
}

impl<'a> BreakLines<'a> {
    pub(super) fn new(layout: &'a mut LayoutData, lines: &'a mut LineLayoutData) -> Self {
        Self {
            layout,
            lines,
            state: BreakerState::default(),
            prev_state: None,
        }
    }

    // pub fn from_data(layout: &'a mut LayoutData, lines: &'a mut LineLayoutData) -> Self {
    //     Self {
    //         layout,
    //         lines,
    //         state: BreakerState::default(),
    //         prev_state: None,
    //     }
    // }

    /// Computes the next line in the paragraph. Returns the advance and size
    /// (width and height for horizontal layouts) of the line.
    pub fn break_next(
        &mut self,
        max_advance: f32,
        alignment: Alignment,
    ) -> Option<(f32, f32)> {
        use swash::text::cluster::Boundary;
        self.prev_state = Some(self.state);
        let run_count = self.layout.runs.len();
        while self.state.i < run_count {
            let run = &self.layout.runs[self.state.i];
            let cluster_end = run.clusters.1 as usize;
            while self.state.j < cluster_end {
                let cluster =
                    Cluster::new(self.layout, self.layout.clusters[self.state.j]);
                let boundary = cluster.info().boundary();
                match boundary {
                    Boundary::Mandatory => {
                        if !self.state.line.skip_mandatory_break {
                            self.state.prev_boundary = None;
                            self.state.line.clusters.1 = self.state.j as u32;
                            self.state.line.runs.1 = self.state.i as u32 + 1;
                            self.state.line.skip_mandatory_break = true;
                            if commit_line(
                                self.layout,
                                self.lines,
                                &mut self.state.line,
                                Some(max_advance),
                                alignment,
                                true,
                            ) {
                                self.state.runs = self.lines.runs.len();
                                self.state.lines = self.lines.lines.len();
                                self.state.line.x = 0.;
                                let line = self.lines.lines.last().unwrap();
                                return Some((line.width, line.size()));
                            }
                        }
                    }
                    Boundary::Line => {
                        self.state.prev_boundary = Some(PrevBoundaryState {
                            i: self.state.i,
                            j: self.state.j,
                            state: self.state.line,
                        });
                    }
                    _ => {}
                }
                self.state.line.skip_mandatory_break = false;
                let advance = cluster.advance();
                let next_x = self.state.line.x + advance;
                if next_x > max_advance {
                    if cluster.info().whitespace().is_space_or_nbsp() {
                        // Hang overflowing whitespace
                        self.state.line.runs.1 = self.state.i as u32 + 1;
                        self.state.line.clusters.1 = self.state.j as u32 + 1;
                        self.state.line.x = next_x;
                        if commit_line(
                            self.layout,
                            self.lines,
                            &mut self.state.line,
                            Some(max_advance),
                            alignment,
                            false,
                        ) {
                            self.state.runs = self.lines.runs.len();
                            self.state.lines = self.lines.lines.len();
                            self.state.line.x = 0.;
                            let line = self.lines.lines.last().unwrap();
                            self.state.prev_boundary = None;
                            self.state.j += 1;
                            return Some((line.width, line.size()));
                        }
                    } else if let Some(prev) = self.state.prev_boundary.take() {
                        if prev.state.x == 0. {
                            // This will cycle if we try to rewrap. Accept the overflowing fragment.
                            self.state.line.runs.1 = self.state.i as u32 + 1;
                            self.state.line.clusters.1 = self.state.j as u32 + 1;
                            self.state.line.x = next_x;
                            self.state.j += 1;
                            if commit_line(
                                self.layout,
                                self.lines,
                                &mut self.state.line,
                                Some(max_advance),
                                alignment,
                                false,
                            ) {
                                self.state.runs = self.lines.runs.len();
                                self.state.lines = self.lines.lines.len();
                                self.state.line.x = 0.;
                                let line = self.lines.lines.last().unwrap();
                                self.state.prev_boundary = None;
                                self.state.j += 1;
                                return Some((line.width, line.size()));
                            }
                        } else {
                            self.state.line = prev.state;
                            if commit_line(
                                self.layout,
                                self.lines,
                                &mut self.state.line,
                                Some(max_advance),
                                alignment,
                                false,
                            ) {
                                self.state.runs = self.lines.runs.len();
                                self.state.lines = self.lines.lines.len();
                                self.state.line.x = 0.;
                                let line = self.lines.lines.last().unwrap();
                                self.state.i = prev.i;
                                self.state.j = prev.j;
                                return Some((line.width, line.size()));
                            }
                        }
                    } else {
                        if self.state.line.x == 0. {
                            // If we're at the start of the line, this particular
                            // cluster will never fit, so consume it and accept
                            // the overflow.
                            self.state.line.runs.1 = self.state.i as u32 + 1;
                            self.state.line.clusters.1 = self.state.j as u32 + 1;
                            self.state.line.x = next_x;
                            self.state.j += 1;
                        }
                        if commit_line(
                            self.layout,
                            self.lines,
                            &mut self.state.line,
                            Some(max_advance),
                            alignment,
                            false,
                        ) {
                            self.state.runs = self.lines.runs.len();
                            self.state.lines = self.lines.lines.len();
                            self.state.line.x = 0.;
                            let line = self.lines.lines.last().unwrap();
                            self.state.prev_boundary = None;
                            self.state.j += 1;
                            return Some((line.width, line.size()));
                        }
                    }
                } else {
                    // Commit the cluster to the line.
                    self.state.line.runs.1 = self.state.i as u32 + 1;
                    self.state.line.clusters.1 = self.state.j as u32 + 1;
                    self.state.line.x = next_x;
                    self.state.j += 1;
                }
            }
            self.state.i += 1;
        }
        if commit_line(
            self.layout,
            self.lines,
            &mut self.state.line,
            Some(max_advance),
            alignment,
            true,
        ) {
            self.state.runs = self.lines.runs.len();
            self.state.lines = self.lines.lines.len();
            self.state.line.x = 0.;
            let line = self.lines.lines.last().unwrap();
            return Some((line.width, line.size()));
        }
        None
    }

    /// Reverts the last computed line, returning to the previous state.
    pub fn revert(&mut self) -> bool {
        if let Some(state) = self.prev_state.take() {
            self.state = state;
            self.lines.lines.truncate(self.state.lines);
            self.lines.runs.truncate(self.state.runs);
            true
        } else {
            false
        }
    }

    /// Breaks all remaining lines with the specified maximum advance. This
    /// consumes the line breaker.
    pub fn break_remaining(mut self, max_advance: f32, alignment: Alignment) {
        while self.break_next(max_advance, alignment).is_some() {}
        self.finish();
    }

    pub fn break_based_on_span(mut self) {
        let mut last_line = 0;
        // let mut y = 0;

        for i in 0..self.layout.runs.len() {
            let run = &self.layout.runs[i];

            if last_line < run.line {
                // println!("entrou no {}", last_line);

                if commit_line(
                    self.layout,
                    self.lines,
                    &mut self.state.line,
                    None,
                    Alignment::Start,
                    false,
                ) {
                    // println!("commitou no 0 {}", last_line);
                    self.state.runs = self.lines.runs.len();
                    self.state.lines = self.lines.lines.len();
                    self.state.line.x = 0.;
                    // self.state.line.clusters.1 = 0;
                }
                last_line = run.line;
                // y = 0;
            }

            // let cluster_end = run.clusters.1 as usize;
            // while self.state.j < cluster_end {
            // let cluster =
            //     Cluster::new(self.layout, self.layout.clusters[self.state.j]);
            // let boundary = cluster.info().boundary();
            // println!("{:?}", boundary);
            // }
            self.state.prev_boundary = None;
            // self.state.line.clusters.1 = self.state.j as u32;
            self.state.line.clusters.1 = run.clusters.1;
            self.state.line.runs.1 = i as u32 + 1;
            self.state.j += 1;
            // y += 1;
        }

        self.finish();
    }

    /// Consumes the line breaker and finalizes all line computations.
    pub fn finish(self) {
        for run in &mut self.lines.runs {
            run.whitespace = true;
            if run.level & 1 != 0 {
                // RTL runs check for "trailing" whitespace at the front.
                for cluster in self.layout.clusters[make_range(run.clusters)].iter() {
                    if cluster.info.is_whitespace() {
                        run.trailing_whitespace = true;
                    } else {
                        run.whitespace = false;
                        break;
                    }
                }
            } else {
                for cluster in self.layout.clusters[make_range(run.clusters)].iter().rev()
                {
                    if cluster.info.is_whitespace() {
                        run.trailing_whitespace = true;
                    } else {
                        run.whitespace = false;
                        break;
                    }
                }
            }
        }
        let mut y = 0.;
        for line in &mut self.lines.lines {
            let run_base = line.runs.0 as usize;
            let run_count = line.runs.1 as usize - run_base;
            line.x = 0.;
            line.ascent = 0.;
            line.descent = 0.;
            line.leading = 0.;
            let mut have_metrics = false;
            let mut needs_reorder = false;
            // Compute metrics for the line, but ignore trailing whitespace.
            for run in self.lines.runs[make_range(line.runs)].iter().rev() {
                if run.level != 0 {
                    needs_reorder = true;
                }
                if !have_metrics && run.whitespace {
                    continue;
                }
                line.ascent = line.ascent.max(run.ascent);
                line.descent = line.descent.max(run.descent);
                line.leading = line.leading.max(run.leading);
                have_metrics = true;
            }
            if needs_reorder && run_count > 1 {
                reorder_runs(&mut self.lines.runs[make_range(line.runs)]);
            }
            let mut total_advance = 0.;
            // for run in self.lines.runs[make_range(line.runs)].iter() {
            //     let r = Run::new(self.layout, &run);
            //     let rtl = run.level & 1 != 0;
            //     let mut clusters = r.visual_clusters();
            //     let mut pos = 0;
            //     let mut ligature_step = 0.;
            //     let mut ligature_count = 0;
            //     while let Some(cluster) = clusters.next() {
            //         let index = if rtl {
            //             run.clusters.1.wrapping_sub(pos).wrapping_sub(1)
            //         } else {
            //             run.clusters.0 + pos
            //         };
            //         pos += 1;
            //         if ligature_count > 0 {
            //             ligature_count -= 1;
            //             self.lines.clusters.push((index, total_advance));
            //             total_advance += ligature_step;
            //         } else {
            //             let mut advance = cluster.advance();
            //             if cluster.is_ligature() {
            //                 let count = clusters
            //                     .clone()
            //                     .take_while(|c| c.is_continuation())
            //                     .count();
            //                 ligature_step = advance / (count + 1) as f32;
            //                 ligature_count = count;
            //                 advance = ligature_step;
            //             }
            //             self.lines.clusters.push((index, total_advance));
            //             total_advance += advance;
            //         }
            //     }
            // }
            for run in self.lines.runs[make_range(line.runs)].iter() {
                let r = Run::new(self.layout, run);
                let rtl = run.level & 1 != 0;
                let clusters = r.visual_clusters();
                let mut pos = 0;
                #[allow(clippy::explicit_counter_loop)]
                for cluster in clusters {
                    let index = if rtl {
                        run.clusters.1.wrapping_sub(pos).wrapping_sub(1)
                    } else {
                        run.clusters.0 + pos
                    };
                    pos += 1;
                    self.lines.clusters.push((index, total_advance));
                    total_advance += cluster.advance();
                }
            }
            if line.alignment != Alignment::Start {
                let trailing_space_advance =
                    if line.clusters.0 != line.clusters.1 && line.clusters.1 > 0 {
                        let (cluster_index, cluster_offset) =
                            self.lines.clusters[line.clusters.1 as usize - 1];
                        let cluster_data = self.layout.clusters[cluster_index as usize];
                        if cluster_data.info.whitespace().is_space_or_nbsp() {
                            total_advance - cluster_offset
                        } else {
                            0.
                        }
                    } else {
                        0.
                    };

                if let Some(max_advance) = line.max_advance {
                    let extra = max_advance - total_advance + trailing_space_advance;
                    if extra > 0. {
                        let offset = if line.alignment == Alignment::Middle {
                            extra * 0.5
                        } else {
                            extra
                        };
                        for cluster in &mut self.lines.clusters[make_range(line.clusters)]
                        {
                            cluster.1 += offset;
                        }
                        line.x = offset;
                    }
                }
            }
            if line.explicit_break {
                // self.lines.clusters.get_mut(line.clusters.1.saturating_sub(1) as usize).map(|c| c.flags |= CLUSTER_NEWLINE);
            }
            line.width = total_advance;
            line.trailing_whitespace =
                self.lines.runs[line.runs.1 as usize - 1].trailing_whitespace;
            if !have_metrics {
                // Line consisting entirely of whitespace?
                if line.runs.0 != line.runs.1 {
                    let run = &self.lines.runs[line.runs.0 as usize];
                    line.ascent = run.ascent;
                    line.descent = run.descent;
                    line.leading = run.leading;
                }
            }
            line.ascent = line.ascent.round();
            line.descent = line.descent.round();
            line.leading = (line.leading * 0.5).round() * 2.;
            let above = (line.ascent + line.leading * 0.5).round();
            let below = (line.descent + line.leading * 0.5).round();
            line.baseline = y + above;
            y = line.baseline + below;
        }
    }
}

#[derive(Copy, Clone, Default)]
struct LineState {
    x: f32,
    runs: (u32, u32),
    clusters: (u32, u32),
    skip_mandatory_break: bool,
}

#[derive(Copy, Clone, Default)]
struct PrevBoundaryState {
    i: usize,
    j: usize,
    state: LineState,
}

#[derive(Copy, Clone, Default)]
struct BreakerState {
    runs: usize,
    lines: usize,
    i: usize,
    j: usize,
    line: LineState,
    prev_boundary: Option<PrevBoundaryState>,
}

fn commit_line(
    layout: &LayoutData,
    lines: &mut LineLayoutData,
    state: &mut LineState,
    max_advance: Option<f32>,
    alignment: Alignment,
    explicit: bool,
) -> bool {
    state.clusters.1 = state.clusters.1.min(layout.clusters.len() as u32);
    if state.runs.0 == state.runs.1 || state.clusters.0 == state.clusters.1 {
        return false;
    }
    let line_index = lines.lines.len() as u32;
    let last_run = (state.runs.1 - state.runs.0) as usize - 1;
    let runs_start = lines.runs.len() as u32;
    for (i, run) in layout.runs[make_range(state.runs)].iter().enumerate() {
        let mut cluster_range = run.clusters;
        if i == 0 {
            cluster_range.0 = state.clusters.0;
        }
        if i == last_run {
            cluster_range.1 = state.clusters.1;
        }
        if cluster_range.0 >= cluster_range.1 {
            continue;
        }
        let mut copy = run.to_owned();
        copy.clusters = cluster_range;
        copy.line = line_index;
        lines.runs.push(copy);
    }
    let runs_end = lines.runs.len() as u32;
    if runs_start == runs_end {
        return false;
    }
    let line = LineData {
        runs: (runs_start, runs_end),
        clusters: state.clusters,
        width: state.x,
        max_advance,
        alignment,
        explicit_break: explicit,
        ..Default::default()
    };
    lines.lines.push(line);
    state.clusters.0 = state.clusters.1;
    state.clusters.1 += 1;
    state.runs.0 = state.runs.1 - 1;
    true
}

fn reorder_runs(runs: &mut [RunData]) {
    let mut max_level = 0;
    let mut lowest_odd_level = 255;
    let len = runs.len();
    for element in runs.iter() {
        let level = element.level;
        if level > max_level {
            max_level = level;
        }
        if level & 1 != 0 && level < lowest_odd_level {
            lowest_odd_level = level;
        }
    }
    for level in (lowest_odd_level..=max_level).rev() {
        let mut i = 0;
        while i < len {
            if runs[i].level >= level {
                let mut end = i + 1;
                while end < len && runs[end].level >= level {
                    end += 1;
                }
                let mut j = i;
                let mut k = end - 1;
                while j < k {
                    runs.swap(j, k);
                    j += 1;
                    k -= 1;
                }
                i = end;
            }
            i += 1;
        }
    }
}
