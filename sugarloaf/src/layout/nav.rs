//! Support for navigating a layout.

use super::layout::{make_range, Line};
use super::Paragraph;
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
    pub fn from_point(layout: &Paragraph, x: f32, y: f32) -> Self {
        let focus = Node::from_point(layout, x, y);
        Self {
            anchor: focus,
            focus,
            move_state: None,
        }
    }

    /// Creates a new selection bounding the word at the specified point.
    pub fn word_from_point(layout: &Paragraph, x: f32, y: f32) -> Self {
        let target = Node::from_point_direct(layout, x, y);
        let line_data = &layout.line_data.lines[target.line as usize];
        let limit = line_data.clusters.1;
        let lower_limit = line_data.clusters.0;
        let mut logical_index = layout.line_data.visual_to_logical(target.cluster);
        if logical_index as usize >= layout.data.clusters.len() {
            logical_index -= 1;
        }
        let mut anchor_index = logical_index;
        for i in (lower_limit..=logical_index).rev() {
            anchor_index = i;
            let c = &layout.data.clusters[i as usize];
            if c.info.is_boundary() {
                break;
            }
        }
        let mut focus_index = logical_index;
        for i in logical_index + 1..limit {
            let c = &layout.data.clusters[i as usize];
            if c.info.is_boundary() {
                break;
            }
            focus_index = i;
        }
        let anchor_visual = layout.line_data.logical_to_visual(anchor_index);
        let focus_visual = layout.line_data.logical_to_visual(focus_index);
        let anchor_rtl = layout.line_data.is_rtl(anchor_index);
        let focus_rtl = layout.line_data.is_rtl(focus_index);
        Self {
            anchor: Node::from_visual_cluster(layout, anchor_visual, anchor_rtl),
            focus: Node::from_visual_cluster(layout, focus_visual, !focus_rtl),
            move_state: None,
        }
    }

    /// Creates a new selection bounding the line at the specified point.
    pub fn line_from_point(layout: &Paragraph, x: f32, y: f32) -> Self {
        let target = Node::from_point_direct(layout, x, y);
        Self::from_focus(target)
            .home(layout, false)
            .end(layout, true)
    }

    /// Creates a new selection with a focus nearest to the character with the
    /// specified byte offset.
    pub fn from_offset(layout: &Paragraph, offset: usize) -> Self {
        for (i, cluster) in layout.data.clusters.iter().enumerate() {
            if cluster.offset as usize >= offset {
                let prev = i.saturating_sub(1).min(layout.data.clusters.len() - 1) as u32;
                let after = offset != 0 && !layout.line_data.is_rtl(prev);
                let visual = layout.line_data.logical_to_visual(prev);
                return Self::from_focus(Node::from_visual_cluster(
                    layout, visual, after,
                ));
            }
        }
        Self::from_focus(Node::from_visual_cluster(
            layout,
            layout.data.clusters.len().saturating_sub(1) as u32,
            false,
        ))
    }

    /// Returns true if the selection is collapsed.
    pub fn is_collapsed(&self) -> bool {
        (self.focus.cluster, self.focus.after) == (self.anchor.cluster, self.anchor.after)
    }

    /// Returns the visual geometry of the focus.
    pub fn cursor(&self, layout: &Paragraph) -> ([f32; 2], f32, bool) {
        let node =
            Node::from_visual_cluster(layout, self.focus.cluster, self.focus.after);
        let line = Line::new(layout, node.line as usize);
        ([node.edge, above(&line)], line.size(), node.rtl)
    }

    /// Returns the current source offset for the focus of the selection. This
    /// is where text should be inserted.
    pub fn offset(&self, layout: &Paragraph) -> usize {
        self.focus.text_offset(layout)
    }

    /// Returns the current source offset for the anchor of the selection.
    pub fn anchor_offset(&self, layout: &Paragraph) -> usize {
        self.anchor.text_offset(layout)
    }

    /// Returns the source range for the currently selected text. Note that
    /// the ordering of this range is dependent on the direction in which
    /// the text was selected. Use [`normalized_range`](Selection::normalized_range)
    /// for a standard ordering.
    pub fn range(&self, layout: &Paragraph) -> Range<usize> {
        let start = self.anchor.text_offset(layout);
        let end = self.focus.text_offset(layout);
        start..end
    }

    /// Returns the source range for the currently selected text. This
    /// function ensures that `start <= end`.
    pub fn normalized_range(&self, layout: &Paragraph) -> Range<usize> {
        let mut start = self.focus.text_offset(layout);
        let mut end = self.anchor.text_offset(layout);
        if start > end {
            core::mem::swap(&mut start, &mut end);
        }
        start..end
    }

    /// Returns the range of text that should be erased based on the
    /// current selection. This operation implements the action of the
    /// `delete` key.
    pub fn erase(&self, layout: &Paragraph) -> Option<Erase> {
        if !self.is_collapsed() {
            return Some(Erase::Full(self.normalized_range(layout)));
        }
        let cluster = layout
            .data
            .clusters
            .get(self.focus.logical_index(layout) as usize)?;
        let start = cluster.offset as usize;
        let end = start + cluster.len as usize;
        Some(Erase::Full(start..end))
    }

    /// Returns the range of text that should be erased based on the current
    /// selection. This operation implements the action of the `backspace`
    /// key.
    pub fn erase_previous(&self, layout: &Paragraph) -> Option<Erase> {
        if !self.is_collapsed() {
            return Some(Erase::Full(self.normalized_range(layout)));
        }
        let logical_index = self.focus.logical_index(layout) as usize;
        if logical_index == 0 {
            return None;
        }
        let prev_logical = logical_index - 1;
        let cluster = layout.data.clusters.get(prev_logical)?;
        let start = cluster.offset as usize;
        let end = start + cluster.len as usize;
        let emoji = layout.data.clusters.get(prev_logical)?.info.is_emoji();
        Some(if emoji {
            Erase::Full(start..end)
        } else {
            Erase::Last(start..end)
        })
    }

    /// Returns a new selection, extending self to the specified point.
    pub fn extend_to(&self, layout: &Paragraph, x: f32, y: f32, to: ExtendTo) -> Self {
        match to {
            ExtendTo::Point => self.extend(layout, Node::from_point(layout, x, y)),
            ExtendTo::Word => {
                self.extend_word(layout, Self::word_from_point(layout, x, y))
            }
            ExtendTo::Line => {
                self.extend_full(layout, Self::line_from_point(layout, x, y))
            }
        }
    }

    /// Returns a new, optionally extended, selection with the focus at
    /// the next visual character.
    pub fn next(&self, layout: &Paragraph, extend: bool) -> Self {
        if !extend && !self.is_collapsed() {
            return self.collapse(layout, false);
        }
        let mut index = self.focus.cluster;
        let mut eol = false;
        if let Some(eol_state) = self.focus.eol_state(layout) {
            index += eol_state;
            eol = true;
        } else if self.focus.after {
            index += 1;
        }
        let focus = Node::from_visual_cluster(layout, index, !eol || extend);
        if extend {
            self.extend(layout, focus)
        } else {
            Self::from_focus(focus)
        }
    }

    /// Returns a new, optionally extended, selection with the focus at
    /// the previous visual character.
    pub fn previous(&self, layout: &Paragraph, extend: bool) -> Self {
        if !extend && !self.is_collapsed() {
            return self.collapse(layout, true);
        }
        let mut index = self.focus.cluster;
        if !self.focus.after || self.focus.nl {
            index = index.saturating_sub(1);
        }
        let focus = Node::from_visual_cluster(layout, index, false);
        if extend {
            self.extend(layout, focus)
        } else {
            Self::from_focus(focus)
        }
    }

    /// Returns a new, optionally extended, selection with the focus at
    /// the beginning of the current line.
    pub fn home(&self, layout: &Paragraph, extend: bool) -> Self {
        let baseline = self.cursor(layout).0[1];
        if extend {
            self.extend_to(layout, 0., baseline + 0.001, ExtendTo::Point)
        } else {
            Self::from_point(layout, 0., baseline + 0.001)
        }
    }

    /// Returns a new, optionally extended, selection with the focus at
    /// the end of the current line.    
    pub fn end(&self, layout: &Paragraph, extend: bool) -> Self {
        let baseline = self.cursor(layout).0[1];
        if extend {
            self.extend_to(layout, f32::MAX, baseline + 0.001, ExtendTo::Point)
        } else {
            Self::from_point(layout, f32::MAX, baseline + 0.001)
        }
    }

    /// Returns a new, optionally extended, selection with the focus at
    /// a position on the next line that matches the state of the current
    /// selection.   
    pub fn next_line(&self, layout: &Paragraph, extend: bool) -> Self {
        let mut move_state = self.move_state;
        if let Some(focus) = self.focus.adjacent_line(layout, false, &mut move_state) {
            let mut res = if extend {
                self.extend(layout, focus)
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
    pub fn previous_line(&self, layout: &Paragraph, extend: bool) -> Self {
        let mut move_state = self.move_state;
        if let Some(focus) = self.focus.adjacent_line(layout, true, &mut move_state) {
            let mut res = if extend {
                self.extend(layout, focus)
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
        layout: &Paragraph,
        mut f: impl FnMut([f32; 4]),
    ) -> Option<()> {
        if self.is_collapsed() {
            return Some(());
        }
        let mut start = self.focus.logical_index(layout);
        let mut end = self.anchor.logical_index(layout);
        if start > end {
            core::mem::swap(&mut start, &mut end);
        }
        let mut in_region = false;
        let start_line = layout.line_data.line_index_for_cluster(start);
        let end_line = layout.line_data.line_index_for_cluster(end);
        for line_index in start_line..=end_line {
            let line = Line::new(layout, line_index);
            let line_data = line.data();
            let line_end = line.offset() + line.advance();
            let mut rect = [line.offset(), above(&line), 0., line.size()];
            let clusters = &layout.line_data.clusters[make_range(line_data.clusters)];
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
    pub fn dump(&self, layout: &Paragraph) {
        println!("anchor: {:?}", self.anchor);
        println!(" -- logical: {}", self.anchor.logical_index(layout));
        println!("focus: {:?}", self.focus);
        println!(" -- logical: {}", self.focus.logical_index(layout));
    }

    fn from_focus(focus: Node) -> Self {
        Self {
            anchor: focus,
            focus,
            move_state: None,
        }
    }

    fn extend(&self, layout: &Paragraph, focus: Node) -> Self {
        let mut anchor = self.anchor;
        if anchor.line < focus.line
            || (anchor.line == focus.line && anchor.edge < focus.edge)
        {
            // If the anchor precedes focus in visual order, ensure that it is in the
            // 'before' state.
            if anchor.after {
                let index = anchor.cluster + 1;
                if index as usize <= layout.line_data.clusters.len() {
                    anchor = Node::from_visual_cluster(layout, index, false);
                }
            }
        } else if anchor.line > focus.line || anchor.edge > focus.edge {
            // Otherwise, set it to 'after' state.
            if !anchor.after {
                if anchor.cluster > 0 {
                    anchor = Node::from_visual_cluster(layout, anchor.cluster - 1, true);
                }
            }
        }
        Self {
            anchor,
            focus,
            move_state: None,
        }
    }

    fn extend_word(&self, layout: &Paragraph, other: Selection) -> Self {
        let fudge = if self.anchor.after { -0.01 } else { 0.01 };
        let initial_word = Self::word_from_point(
            layout,
            self.anchor.edge + fudge,
            layout.line_data.lines[self.anchor.line as usize].baseline - 0.001,
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
                if index as usize <= layout.line_data.clusters.len() {
                    anchor = Node::from_visual_cluster(layout, index, false);
                }
            }
        } else if anchor.line > focus.line || anchor.edge > focus.edge {
            // Otherwise, set it to 'after' state.
            if !anchor.after {
                if anchor.cluster > 0 {
                    anchor = Node::from_visual_cluster(layout, anchor.cluster - 1, true);
                }
            }
        }
        Self {
            anchor,
            focus,
            move_state: None,
        }
    }

    fn extend_full(&self, layout: &Paragraph, other: Selection) -> Self {
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
                if index as usize <= layout.line_data.clusters.len() {
                    anchor = Node::from_visual_cluster(layout, index, false);
                }
            }
        } else if anchor.line > focus.line || anchor.edge > focus.edge {
            // Otherwise, set it to 'after' state.
            if !anchor.after {
                if anchor.cluster > 0 {
                    anchor = Node::from_visual_cluster(layout, anchor.cluster - 1, true);
                }
            }
        }
        Self {
            anchor,
            focus,
            move_state: None,
        }
    }

    fn collapse(&self, _layout: &Paragraph, prev: bool) -> Self {
        let node = if prev {
            if self.focus < self.anchor {
                &self.focus
            } else {
                &self.anchor
            }
        } else {
            if self.focus > self.anchor {
                &self.focus
            } else {
                &self.anchor
            }
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
    fn from_point(layout: &Paragraph, mut x: f32, y: f32) -> Self {
        let mut this = Self::default();
        let line_count = layout.line_data.lines.len();
        if line_count == 0 {
            return this;
        }
        let last_line_index = line_count - 1;
        for (i, line) in layout.lines().enumerate() {
            if y <= (line.baseline() + line.descent()) || i == last_line_index {
                if y > line.baseline() + line.descent() {
                    x = f32::MAX;
                }
                let line_end = line.offset() + line.advance();
                let line_data = line.data();
                this.line = i as u32;
                let clusters = &layout.line_data.clusters[make_range(line_data.clusters)];
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
                                layout,
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
                this.setup_from_visual(layout, line_data.clusters.1.saturating_sub(1));
                return this;
            }
        }
        this
    }

    fn from_point_direct(layout: &Paragraph, mut x: f32, y: f32) -> Self {
        let mut this = Self::default();
        let line_count = layout.line_data.lines.len();
        if line_count == 0 {
            return this;
        }
        let last_line_index = line_count - 1;
        for (i, line) in layout.lines().enumerate() {
            if y <= (line.baseline() + line.descent()) || i == last_line_index {
                let line_start = line.offset();
                let line_end = line_start + line.advance();
                let line_data = line.data();
                if y > line.baseline() + line.descent() {
                    x = f32::MAX;
                }
                this.line = i as u32;
                let clusters = &layout.line_data.clusters[make_range(line_data.clusters)];
                for (i, &(_, edge)) in clusters.iter().enumerate() {
                    if x >= edge || x < line_start {
                        let far_edge =
                            clusters.get(i + 1).map(|x| x.1).unwrap_or(line_end);
                        if x < far_edge {
                            this.edge = edge;
                            this.setup_from_visual(
                                layout,
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
                this.setup_from_visual(layout, line_data.clusters.1.saturating_sub(1));
                return this;
            }
        }
        this
    }

    fn from_visual_cluster(layout: &Paragraph, mut index: u32, mut after: bool) -> Self {
        let limit = layout.line_data.clusters.len() as u32;
        let mut this = Self::default();
        if limit == 0 {
            return this;
        }
        if index >= limit {
            after = false;
            index = limit - 1;
        }
        let line_index = layout.line_data.line_index_for_cluster(index);
        let line = Line::new(layout, line_index);
        this.line = line_index as u32;
        this.cluster = index;
        let logical_index = layout.line_data.visual_to_logical(index);
        this.nl = layout.data.clusters[logical_index as usize].is_newline();
        this.rtl = layout.line_data.is_rtl(logical_index);
        if after {
            index += 1;
        }
        let mut last_cluster = line.data().clusters.1;
        if this.nl && after {
            this.line += 1;
            last_cluster = (last_cluster + 1).min(limit);
        }
        let line_clusters = &layout.line_data.clusters[0..last_cluster as usize];
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
        layout: &Paragraph,
        prev: bool,
        move_state: &mut Option<f32>,
    ) -> Option<Self> {
        let x = move_state.unwrap_or(self.edge);
        let mut line_index = layout.line_data.line_index_for_cluster(self.cluster);
        if prev {
            line_index = line_index.checked_sub(1)?;
        } else {
            line_index = line_index.checked_add(1)?;
        }
        let line = layout.line_data.lines.get(line_index)?;
        let y = line.baseline - 0.001;
        *move_state = Some(x);
        Some(Self::from_point(layout, x, y))
    }

    fn setup_from_visual(&mut self, layout: &Paragraph, mut index: u32) {
        let limit = layout.data.clusters.len() as u32;
        index = index.min(limit.saturating_sub(1));
        self.cluster = index;
        let logical_index = layout.line_data.visual_to_logical(index);
        self.rtl = layout.line_data.is_rtl(logical_index);
        self.nl = layout.data.clusters[logical_index as usize].is_newline();
        if index == limit {
            self.after = false;
        }
    }

    fn eol_state(&self, layout: &Paragraph) -> Option<u32> {
        if let Some(line) = layout.line_data.lines.get(self.line as usize) {
            let tw = line.trailing_whitespace;
            if self.cluster + 1 == line.clusters.1 && tw {
                return Some(1);
            } else if self.after && self.cluster + 2 == line.clusters.1 && tw {
                return Some(2);
            }
        }
        return None;
    }

    // fn previous_text_location(&self, layout: &Paragraph) -> (FragmentId, usize) {
    //     let data = &layout.data;
    //     let limit = data.clusters.len() as u32;
    //     if limit == 0 {
    //         return (FragmentId(0), 0);
    //     }
    //     if self.after {
    //         if self.rtl {
    //             let logical_index = layout
    //                 .line_data
    //                 .visual_to_logical(self.cluster)
    //                 .min(limit - 1)
    //                 .saturating_sub(1);
    //             return Self::from_visual_cluster(
    //                 layout,
    //                 layout.line_data.logical_to_visual(logical_index),
    //                 true,
    //             )
    //             .text_location(layout);
    //         } else {
    //             return Self::from_visual_cluster(layout, self.cluster, false)
    //                 .text_location(layout);
    //         }
    //     }
    //     let logical_index = layout
    //         .line_data
    //         .visual_to_logical(self.cluster)
    //         .min(limit - 1)
    //         .saturating_sub(1);
    //     Self::from_visual_cluster(
    //         layout,
    //         layout.line_data.logical_to_visual(logical_index),
    //         false,
    //     )
    //     .text_location(layout)
    // }

    fn text_offset(&self, layout: &Paragraph) -> usize {
        let data = &layout.data;
        let limit = data.clusters.len() as u32;
        if limit == 0 {
            return 0;
        }
        let index = layout
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

    fn logical_index(&self, layout: &Paragraph) -> u32 {
        let mut index = layout.line_data.visual_to_logical(self.cluster);
        if self.rtl {
            if !self.after {
                index += 1;
            }
        } else {
            if self.after {
                index += 1;
            }
        }
        index
    }
}

impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        use core::cmp::Ordering::*;
        if self.line < other.line {
            Some(Less)
        } else if self.line > other.line {
            Some(Greater)
        } else {
            self.edge.partial_cmp(&other.edge)
        }
    }
}

fn above(line: &Line) -> f32 {
    line.baseline() - line.ascent() - line.leading() * 0.5
}
