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
    lines_uses_same_height: bool,
}

impl<'a> BreakLines<'a> {
    pub(super) fn new(layout: &'a mut LayoutData, lines: &'a mut LineLayoutData) -> Self {
        Self {
            layout,
            lines,
            state: BreakerState::default(),
            prev_state: None,
            // This should be configurable but since sugarloaf is used
            // mainly in Rio terminal should be ok leave this way for now
            lines_uses_same_height: true,
        }
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

    #[inline]
    pub fn break_without_advance_or_alignment(&'a mut self) {
        let run_len = self.layout.runs.len();

        for i in 0..self.layout.runs.len() {
            let run = &self.layout.runs[i];
            let mut should_commit_line = false;
            // self.state.prev_boundary = None;

            if i == run_len - 1 {
                should_commit_line = true;
            } else {
                // If next run has a different line number then
                // try to commit line
                let next_run = &self.layout.runs[i + 1];
                if next_run.line != run.line {
                    should_commit_line = true;
                }
            }

            self.state.line.runs.1 = i as u32 + 1;
            // self.state.line.clusters.1 = self.state.j as u32;
            self.state.line.clusters.1 = run.clusters.1;

            if should_commit_line
                && commit_line(
                    self.layout,
                    self.lines,
                    &mut self.state.line,
                    None,
                    Alignment::Start,
                    true,
                    run.hash,
                )
            {
                self.state.runs = self.lines.runs.len();
                self.state.lines = self.lines.lines.len();
                self.state.line.x = 0.;
                // self.state.j += 1;
                self.state.line.clusters.1 = run.clusters.1 + 1;
            }
        }

        self.finish();
    }

    /// Consumes the line breaker and finalizes all line computations.
    pub fn finish(&'a mut self) {
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
            line.x = 0.;
            line.ascent = 0.;
            line.descent = 0.;
            line.leading = 0.;
            let mut total_advance = 0.;
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
            line.width = total_advance;
            line.trailing_whitespace =
                self.lines.runs[line.runs.1 as usize - 1].trailing_whitespace;

            if self.lines_uses_same_height {
                let run = &self.lines.runs[line.runs.0 as usize];
                line.ascent = run.ascent;
                line.descent = run.descent;
                line.leading = run.leading;
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
}

#[derive(Copy, Clone, Default)]
struct BreakerState {
    runs: usize,
    lines: usize,
    line: LineState,
}

#[inline]
fn commit_line(
    layout: &LayoutData,
    lines: &mut LineLayoutData,
    state: &mut LineState,
    max_advance: Option<f32>,
    alignment: Alignment,
    explicit: bool,
    line_hash: Option<u64>,
) -> bool {
    state.clusters.1 = state.clusters.1.min(layout.clusters.len() as u32);
    if state.runs.0 == state.runs.1 || state.clusters.0 == state.clusters.1 {
        return false;
    }
    // let line_index = lines.lines.len() as u64;
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
        // copy.line = line_index;
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
        hash: line_hash,
        ..Default::default()
    };
    lines.lines.push(line);
    state.clusters.0 = state.clusters.1;
    state.clusters.1 += 1;
    state.runs.0 = state.runs.1 - 1;
    true
}
