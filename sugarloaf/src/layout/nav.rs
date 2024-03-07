// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// Nav.rs was originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE

//! Support for navigating a render_data.

use super::render_data::{make_range, Line, RenderData};
use core::ops::Range;

/// Describes the text range for an erase operation.
#[derive(Clone, Debug)]
pub enum Erase {
    /// Specifies a range of text that should be erased.
    Full(Range<usize>),
    /// Specifies that the last character in a range should be erased.
    Last(Range<usize>),
}

/// Determines the item for selection extension.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ExtendTo {
    Point,
    Word,
    Line,
}

/// State of a selected range of text in a paragraph.
#[derive(Copy, Clone, Default, Debug)]
pub struct Selection {
    anchor: Node,
    focus: Node,
    move_state: Option<f32>,
}

impl Selection {
    /// Creates a new selection with the focus at the specified point.
    pub fn from_point(render_data: &RenderData, x: f32, y: f32) -> Self {
        let focus = Node::from_point(render_data, x, y);
        Self {
            anchor: focus,
            focus,
            move_state: None,
        }
    }

    /// Creates a new selection bounding the word at the specified point.
    pub fn word_from_point(render_data: &RenderData, x: f32, y: f32) -> Self {
        let target = Node::from_point_direct(render_data, x, y);
        let line_data = &render_data.line_data.lines[target.line as usize];
        let limit = line_data.clusters.1;
        let lower_limit = line_data.clusters.0;
        let mut logical_index = render_data.line_data.visual_to_logical(target.cluster);
        if logical_index as usize >= render_data.data.clusters.len() {
            logical_index -= 1;
        }
        let mut anchor_index = logical_index;
        for i in (lower_limit..=logical_index).rev() {
            anchor_index = i;
            let c = &render_data.data.clusters[i as usize];
            if c.info.is_boundary() {
                break;
            }
        }
        let mut focus_index = logical_index;
        for i in logical_index + 1..limit {
            let c = &render_data.data.clusters[i as usize];
            if c.info.is_boundary() {
                break;
            }
            focus_index = i;
        }
        let anchor_visual = render_data.line_data.logical_to_visual(anchor_index);
        let focus_visual = render_data.line_data.logical_to_visual(focus_index);
        let anchor_rtl = render_data.line_data.is_rtl(anchor_index);
        let focus_rtl = render_data.line_data.is_rtl(focus_index);
        Self {
            anchor: Node::from_visual_cluster(render_data, anchor_visual, anchor_rtl),
            focus: Node::from_visual_cluster(render_data, focus_visual, !focus_rtl),
            move_state: None,
        }
    }

    /// Creates a new selection bounding the line at the specified point.
    pub fn line_from_point(render_data: &RenderData, x: f32, y: f32) -> Self {
        let target = Node::from_point_direct(render_data, x, y);
        Self::from_focus(target)
            .home(render_data, false)
            .end(render_data, true)
    }

    /// Creates a new selection with a focus nearest to the character with the
    /// specified byte offset.
    pub fn from_offset(render_data: &RenderData, offset: usize) -> Self {
        for (i, cluster) in render_data.data.clusters.iter().enumerate() {
            if cluster.offset as usize >= offset {
                let prev =
                    i.saturating_sub(1).min(render_data.data.clusters.len() - 1) as u32;
                let after = offset != 0 && !render_data.line_data.is_rtl(prev);
                let visual = render_data.line_data.logical_to_visual(prev);
                return Self::from_focus(Node::from_visual_cluster(
                    render_data,
                    visual,
                    after,
                ));
            }
        }
        Self::from_focus(Node::from_visual_cluster(
            render_data,
            render_data.data.clusters.len().saturating_sub(1) as u32,
            false,
        ))
    }

    /// Returns true if the selection is collapsed.
    pub fn is_collapsed(&self) -> bool {
        (self.focus.cluster, self.focus.after) == (self.anchor.cluster, self.anchor.after)
    }

    /// Returns the visual geometry of the focus.
    pub fn cursor(&self, render_data: &RenderData) -> ([f32; 2], f32, bool) {
        let node =
            Node::from_visual_cluster(render_data, self.focus.cluster, self.focus.after);
        let line = Line::new(render_data, node.line as usize);
        ([node.edge, above(&line)], line.size(), node.rtl)
    }

    /// Returns the current source offset for the focus of the selection. This
    /// is where text should be inserted.
    pub fn offset(&self, render_data: &RenderData) -> usize {
        self.focus.text_offset(render_data)
    }

    /// Returns the current source offset for the anchor of the selection.
    pub fn anchor_offset(&self, render_data: &RenderData) -> usize {
        self.anchor.text_offset(render_data)
    }

    /// Returns the source range for the currently selected text. Note that
    /// the ordering of this range is dependent on the direction in which
    /// the text was selected. Use [`normalized_range`](Selection::normalized_range)
    /// for a standard ordering.
    pub fn range(&self, render_data: &RenderData) -> Range<usize> {
        let start = self.anchor.text_offset(render_data);
        let end = self.focus.text_offset(render_data);
        start..end
    }

    /// Returns the source range for the currently selected text. This
    /// function ensures that `start <= end`.
    pub fn normalized_range(&self, render_data: &RenderData) -> Range<usize> {
        let mut start = self.focus.text_offset(render_data);
        let mut end = self.anchor.text_offset(render_data);
        if start > end {
            core::mem::swap(&mut start, &mut end);
        }
        start..end
    }

    /// Returns the range of text that should be erased based on the
    /// current selection. This operation implements the action of the
    /// `delete` key.
    pub fn erase(&self, render_data: &RenderData) -> Option<Erase> {
        if !self.is_collapsed() {
            return Some(Erase::Full(self.normalized_range(render_data)));
        }
        let cluster = render_data
            .data
            .clusters
            .get(self.focus.logical_index(render_data) as usize)?;
        let start = cluster.offset as usize;
        let end = start + cluster.len as usize;
        Some(Erase::Full(start..end))
    }

    /// Returns the range of text that should be erased based on the current
    /// selection. This operation implements the action of the `backspace`
    /// key.
    pub fn erase_previous(&self, render_data: &RenderData) -> Option<Erase> {
        if !self.is_collapsed() {
            return Some(Erase::Full(self.normalized_range(render_data)));
        }
        let logical_index = self.focus.logical_index(render_data) as usize;
        if logical_index == 0 {
            return None;
        }
        let prev_logical = logical_index - 1;
        let cluster = render_data.data.clusters.get(prev_logical)?;
        let start = cluster.offset as usize;
        let end = start + cluster.len as usize;
        let emoji = render_data.data.clusters.get(prev_logical)?.info.is_emoji();
        Some(if emoji {
            Erase::Full(start..end)
        } else {
            Erase::Last(start..end)
        })
    }

    /// Returns a new selection, extending self to the specified point.
    pub fn extend_to(
        &self,
        render_data: &RenderData,
        x: f32,
        y: f32,
        to: ExtendTo,
    ) -> Self {
        match to {
            ExtendTo::Point => {
                self.extend(render_data, Node::from_point(render_data, x, y))
            }
            ExtendTo::Word => {
                self.extend_word(render_data, Self::word_from_point(render_data, x, y))
            }
            ExtendTo::Line => {
                self.extend_full(render_data, Self::line_from_point(render_data, x, y))
            }
        }
    }

    /// Returns a new, optionally extended, selection with the focus at
    /// the next visual character.
    pub fn next(&self, render_data: &RenderData, extend: bool) -> Self {
        if !extend && !self.is_collapsed() {
            return self.collapse(render_data, false);
        }
        let mut index = self.focus.cluster;
        let mut eol = false;
        if let Some(eol_state) = self.focus.eol_state(render_data) {
            index += eol_state;
            eol = true;
        } else if self.focus.after {
            index += 1;
        }
        let focus = Node::from_visual_cluster(render_data, index, !eol || extend);
        if extend {
            self.extend(render_data, focus)
        } else {
            Self::from_focus(focus)
        }
    }

    /// Returns a new, optionally extended, selection with the focus at
    /// the previous visual character.
    pub fn previous(&self, render_data: &RenderData, extend: bool) -> Self {
        if !extend && !self.is_collapsed() {
            return self.collapse(render_data, true);
        }
        let mut index = self.focus.cluster;
        if !self.focus.after || self.focus.nl {
            index = index.saturating_sub(1);
        }
        let focus = Node::from_visual_cluster(render_data, index, false);
        if extend {
            self.extend(render_data, focus)
        } else {
            Self::from_focus(focus)
        }
    }

    /// Returns a new, optionally extended, selection with the focus at
    /// the beginning of the current line.
    pub fn home(&self, render_data: &RenderData, extend: bool) -> Self {
        let baseline = self.cursor(render_data).0[1];
        if extend {
            self.extend_to(render_data, 0., baseline + 0.001, ExtendTo::Point)
        } else {
            Self::from_point(render_data, 0., baseline + 0.001)
        }
    }

    /// Returns a new, optionally extended, selection with the focus at
    /// the end of the current line.    
    pub fn end(&self, render_data: &RenderData, extend: bool) -> Self {
        let baseline = self.cursor(render_data).0[1];
        if extend {
            self.extend_to(render_data, f32::MAX, baseline + 0.001, ExtendTo::Point)
        } else {
            Self::from_point(render_data, f32::MAX, baseline + 0.001)
        }
    }

    /// Returns a new, optionally extended, selection with the focus at
    /// a position on the next line that matches the state of the current
    /// selection.   
    pub fn next_line(&self, render_data: &RenderData, extend: bool) -> Self {
        let mut move_state = self.move_state;
        if let Some(focus) = self
            .focus
            .adjacent_line(render_data, false, &mut move_state)
        {
            let mut res = if extend {
                self.extend(render_data, focus)
            } else {
                Self::from_focus(focus)
            };
            res.move_state = move_state;
            res
        } else {
            *self
        }
    }

    /// Returns a new, optionally extended, selection with the focus at
    /// a position on the previous line that matches the state of the current
    /// selection.       
    pub fn previous_line(&self, render_data: &RenderData, extend: bool) -> Self {
        let mut move_state = self.move_state;
        if let Some(focus) = self.focus.adjacent_line(render_data, true, &mut move_state)
        {
            let mut res = if extend {
                self.extend(render_data, focus)
            } else {
                Self::from_focus(focus)
            };
            res.move_state = move_state;
            res
        } else {
            *self
        }
    }

    /// Invokes the specified function with a series of rectangles that define
    /// the visual state of the selection.
    pub fn regions_with(
        &self,
        render_data: &RenderData,
        mut f: impl FnMut([f32; 4]),
    ) -> Option<()> {
        if self.is_collapsed() {
            return Some(());
        }
        let mut start = self.focus.logical_index(render_data);
        let mut end = self.anchor.logical_index(render_data);
        if start > end {
            core::mem::swap(&mut start, &mut end);
        }
        let mut in_region = false;
        let start_line = render_data.line_data.line_index_for_cluster(start);
        let end_line = render_data.line_data.line_index_for_cluster(end);
        for line_index in start_line..=end_line {
            let line = Line::new(render_data, line_index);
            let line_data = line.data();
            let line_end = line.offset() + line.advance();
            let mut rect = [line.offset(), above(&line), 0., line.size()];
            let clusters =
                &render_data.line_data.clusters[make_range(line_data.clusters)];
            for (i, &(logical_index, edge)) in clusters.iter().enumerate() {
                if logical_index >= start && logical_index < end {
                    let far_edge = clusters.get(i + 1).map(|x| x.1).unwrap_or(line_end);
                    if !in_region {
                        rect[0] = edge;
                    }
                    rect[2] = far_edge - rect[0];
                    in_region = true;
                } else if in_region {
                    f(rect);
                    in_region = false;
                }
            }
            if in_region {
                if line_index != end_line {
                    rect[2] = line_end - rect[0];
                }
                if rect[2] == 0. {
                    rect[2] = 8.;
                }
                f(rect);
            }
        }
        Some(())
    }
}

impl Selection {
    pub fn dump(&self, render_data: &RenderData) {
        println!("anchor: {:?}", self.anchor);
        println!(" -- logical: {}", self.anchor.logical_index(render_data));
        println!("focus: {:?}", self.focus);
        println!(" -- logical: {}", self.focus.logical_index(render_data));
    }

    fn from_focus(focus: Node) -> Self {
        Self {
            anchor: focus,
            focus,
            move_state: None,
        }
    }

    fn extend(&self, render_data: &RenderData, focus: Node) -> Self {
        let mut anchor = self.anchor;
        if anchor.line < focus.line
            || (anchor.line == focus.line && anchor.edge < focus.edge)
        {
            // If the anchor precedes focus in visual order, ensure that it is in the
            // 'before' state.
            if anchor.after {
                let index = anchor.cluster + 1;
                if index as usize <= render_data.line_data.clusters.len() {
                    anchor = Node::from_visual_cluster(render_data, index, false);
                }
            }
        } else if anchor.line > focus.line || anchor.edge > focus.edge {
            // Otherwise, set it to 'after' state.
            if !anchor.after && anchor.cluster > 0 {
                anchor = Node::from_visual_cluster(render_data, anchor.cluster - 1, true);
            }
        }
        Self {
            anchor,
            focus,
            move_state: None,
        }
    }

    fn extend_word(&self, render_data: &RenderData, other: Selection) -> Self {
        let fudge = if self.anchor.after { -0.01 } else { 0.01 };
        let initial_word = Self::word_from_point(
            render_data,
            self.anchor.edge + fudge,
            render_data.line_data.lines[self.anchor.line as usize].baseline - 0.001,
        );
        let mut anchor = initial_word.anchor;
        let mut focus = other.focus;
        if anchor > focus {
            if initial_word.focus > initial_word.anchor {
                anchor = initial_word.focus;
            }
            if other.anchor < other.focus {
                focus = other.anchor;
            }
        }
        if anchor.line < focus.line
            || (anchor.line == focus.line && anchor.edge < focus.edge)
        {
            // If the anchor precedes focus in visual order, ensure that it is in the
            // 'before' state.
            if anchor.after {
                let index = anchor.cluster + 1;
                if index as usize <= render_data.line_data.clusters.len() {
                    anchor = Node::from_visual_cluster(render_data, index, false);
                }
            }
        } else if anchor.line > focus.line || anchor.edge > focus.edge {
            // Otherwise, set it to 'after' state.
            if !anchor.after && anchor.cluster > 0 {
                anchor = Node::from_visual_cluster(render_data, anchor.cluster - 1, true);
            }
        }
        Self {
            anchor,
            focus,
            move_state: None,
        }
    }

    fn extend_full(&self, render_data: &RenderData, other: Selection) -> Self {
        let mut anchor = self.anchor;
        let mut focus = other.focus;
        if anchor > focus {
            if self.focus > self.anchor {
                anchor = self.focus;
            }
            if other.anchor < other.focus {
                focus = other.anchor;
            }
        }
        if anchor.line < focus.line
            || (anchor.line == focus.line && anchor.edge < focus.edge)
        {
            // If the anchor precedes focus in visual order, ensure that it is in the
            // 'before' state.
            if anchor.after {
                let index = anchor.cluster + 1;
                if index as usize <= render_data.line_data.clusters.len() {
                    anchor = Node::from_visual_cluster(render_data, index, false);
                }
            }
        } else if anchor.line > focus.line || anchor.edge > focus.edge {
            // Otherwise, set it to 'after' state.
            if !anchor.after && anchor.cluster > 0 {
                anchor = Node::from_visual_cluster(render_data, anchor.cluster - 1, true);
            }
        }
        Self {
            anchor,
            focus,
            move_state: None,
        }
    }

    fn collapse(&self, _layout: &RenderData, prev: bool) -> Self {
        let node = if prev {
            if self.focus < self.anchor {
                &self.focus
            } else {
                &self.anchor
            }
        } else if self.focus > self.anchor {
            &self.focus
        } else {
            &self.anchor
        };
        Self::from_focus(*node)
    }
}

#[derive(Copy, Clone, PartialEq, Default, Debug)]
struct Node {
    line: u32,
    cluster: u32,
    edge: f32,
    rtl: bool,
    after: bool,
    nl: bool,
}

impl Node {
    fn from_point(render_data: &RenderData, mut x: f32, y: f32) -> Self {
        let mut this = Self::default();
        let line_count = render_data.line_data.lines.len();
        if line_count == 0 {
            return this;
        }
        let last_line_index = line_count - 1;
        for (i, line) in render_data.lines().enumerate() {
            if y <= (line.baseline() + line.descent()) || i == last_line_index {
                if y > line.baseline() + line.descent() {
                    x = f32::MAX;
                }
                let line_end = line.offset() + line.advance();
                let line_data = line.data();
                this.line = i as u32;
                let clusters =
                    &render_data.line_data.clusters[make_range(line_data.clusters)];
                let mut last_edge = f32::MIN;
                for (i, &(_, edge)) in clusters.iter().enumerate() {
                    if x >= last_edge {
                        let far_edge =
                            clusters.get(i + 1).map(|x| x.1).unwrap_or(line_end);
                        if x < far_edge {
                            this.after = true;
                            let middle = (edge + far_edge) * 0.5;
                            if x <= middle {
                                this.after = false;
                                this.edge = edge;
                            } else {
                                this.after = true;
                                this.edge = far_edge;
                            }
                            this.setup_from_visual(
                                render_data,
                                line_data.clusters.0 + i as u32,
                            );
                            return this;
                        }
                        last_edge = edge;
                    }
                }
                this.edge = line_end;
                if !line_data.explicit_break {
                    this.after = true;
                }
                this.setup_from_visual(
                    render_data,
                    line_data.clusters.1.saturating_sub(1),
                );
                return this;
            }
        }
        this
    }

    fn from_point_direct(render_data: &RenderData, mut x: f32, y: f32) -> Self {
        let mut this = Self::default();
        let line_count = render_data.line_data.lines.len();
        if line_count == 0 {
            return this;
        }
        let last_line_index = line_count - 1;
        for (i, line) in render_data.lines().enumerate() {
            if y <= (line.baseline() + line.descent()) || i == last_line_index {
                let line_start = line.offset();
                let line_end = line_start + line.advance();
                let line_data = line.data();
                if y > line.baseline() + line.descent() {
                    x = f32::MAX;
                }
                this.line = i as u32;
                let clusters =
                    &render_data.line_data.clusters[make_range(line_data.clusters)];
                for (i, &(_, edge)) in clusters.iter().enumerate() {
                    if x >= edge || x < line_start {
                        let far_edge =
                            clusters.get(i + 1).map(|x| x.1).unwrap_or(line_end);
                        if x < far_edge {
                            this.edge = edge;
                            this.setup_from_visual(
                                render_data,
                                line_data.clusters.0 + i as u32,
                            );
                            return this;
                        }
                    }
                }
                this.edge = line_end;
                if !line_data.explicit_break {
                    this.after = true;
                }
                this.setup_from_visual(
                    render_data,
                    line_data.clusters.1.saturating_sub(1),
                );
                return this;
            }
        }
        this
    }

    fn from_visual_cluster(
        render_data: &RenderData,
        mut index: u32,
        mut after: bool,
    ) -> Self {
        let limit = render_data.line_data.clusters.len() as u32;
        let mut this = Self::default();
        if limit == 0 {
            return this;
        }
        if index >= limit {
            after = false;
            index = limit - 1;
        }
        let line_index = render_data.line_data.line_index_for_cluster(index);
        let line = Line::new(render_data, line_index);
        this.line = line_index as u32;
        this.cluster = index;
        let logical_index = render_data.line_data.visual_to_logical(index);
        this.nl = render_data.data.clusters[logical_index as usize].is_newline();
        this.rtl = render_data.line_data.is_rtl(logical_index);
        if after {
            index += 1;
        }
        let mut last_cluster = line.data().clusters.1;
        if this.nl && after {
            this.line += 1;
            last_cluster = (last_cluster + 1).min(limit);
        }
        let line_clusters = &render_data.line_data.clusters[0..last_cluster as usize];
        if let Some(x) = line_clusters.get(index as usize) {
            this.edge = x.1;
        } else {
            this.edge = line.offset() + line.advance();
        }
        this.after = after; // && !this.nl;
        this
    }

    fn adjacent_line(
        &self,
        render_data: &RenderData,
        prev: bool,
        move_state: &mut Option<f32>,
    ) -> Option<Self> {
        let x = move_state.unwrap_or(self.edge);
        let mut line_index = render_data.line_data.line_index_for_cluster(self.cluster);
        if prev {
            line_index = line_index.checked_sub(1)?;
        } else {
            line_index = line_index.checked_add(1)?;
        }
        let line = render_data.line_data.lines.get(line_index)?;
        let y = line.baseline - 0.001;
        *move_state = Some(x);
        Some(Self::from_point(render_data, x, y))
    }

    fn setup_from_visual(&mut self, render_data: &RenderData, mut index: u32) {
        let limit = render_data.data.clusters.len() as u32;
        index = index.min(limit.saturating_sub(1));
        self.cluster = index;
        let logical_index = render_data.line_data.visual_to_logical(index);
        self.rtl = render_data.line_data.is_rtl(logical_index);
        self.nl = render_data.data.clusters[logical_index as usize].is_newline();
        if index == limit {
            self.after = false;
        }
    }

    fn eol_state(&self, render_data: &RenderData) -> Option<u32> {
        if let Some(line) = render_data.line_data.lines.get(self.line as usize) {
            let tw = line.trailing_whitespace;
            if self.cluster + 1 == line.clusters.1 && tw {
                return Some(1);
            } else if self.after && self.cluster + 2 == line.clusters.1 && tw {
                return Some(2);
            }
        }
        None
    }

    // fn previous_text_location(&self, render_data: &RenderData) -> (FragmentId, usize) {
    //     let data = &render_data.data;
    //     let limit = data.clusters.len() as u32;
    //     if limit == 0 {
    //         return (FragmentId(0), 0);
    //     }
    //     if self.after {
    //         if self.rtl {
    //             let logical_index = render_data
    //                 .line_data
    //                 .visual_to_logical(self.cluster)
    //                 .min(limit - 1)
    //                 .saturating_sub(1);
    //             return Self::from_visual_cluster(
    //                 render_data,
    //                 render_data.line_data.logical_to_visual(logical_index),
    //                 true,
    //             )
    //             .text_location(render_data);
    //         } else {
    //             return Self::from_visual_cluster(render_data, self.cluster, false)
    //                 .text_location(render_data);
    //         }
    //     }
    //     let logical_index = render_data
    //         .line_data
    //         .visual_to_logical(self.cluster)
    //         .min(limit - 1)
    //         .saturating_sub(1);
    //     Self::from_visual_cluster(
    //         render_data,
    //         render_data.line_data.logical_to_visual(logical_index),
    //         false,
    //     )
    //     .text_location(render_data)
    // }

    fn text_offset(&self, render_data: &RenderData) -> usize {
        let data = &render_data.data;
        let limit = data.clusters.len() as u32;
        if limit == 0 {
            return 0;
        }
        let index = render_data
            .line_data
            .visual_to_logical(self.cluster)
            .min(limit - 1);
        if let Some(cluster_data) = data.clusters.get(index as usize) {
            let mut offset = cluster_data.offset;
            if self.rtl {
                if !self.after {
                    offset += cluster_data.len as u32;
                }
            } else if self.after {
                offset += cluster_data.len as u32;
            }
            return offset as usize;
        }
        0
    }

    fn logical_index(&self, render_data: &RenderData) -> u32 {
        let mut index = render_data.line_data.visual_to_logical(self.cluster);
        if self.rtl {
            if !self.after {
                index += 1;
            }
        } else if self.after {
            index += 1;
        }
        index
    }
}

impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        use core::cmp::Ordering::*;
        match self.line.cmp(&other.line) {
            Greater => Some(Greater),
            Less => Some(Less),
            Equal => self.edge.partial_cmp(&other.edge),
        }
    }
}

fn above(line: &Line) -> f32 {
    line.baseline() - line.ascent() - line.leading() * 0.5
}
