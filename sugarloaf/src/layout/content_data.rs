// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::layout::content::BuilderState;
use smallvec::SmallVec;

/// Unified content data for all shape types
#[derive(Debug, Clone)]
pub enum ContentData {
    /// Rich text content with full text rendering capabilities
    Text(BuilderState),

    /// Simple rectangle
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [f32; 4],
        depth: f32,
    },

    /// Rectangle with rounded corners
    RoundedRect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [f32; 4],
        depth: f32,
        border_radius: f32,
    },

    /// Line segment
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        width: f32,
        color: [f32; 4],
        depth: f32,
    },

    /// Triangle (3 points)
    Triangle {
        points: [(f32, f32); 3],
        color: [f32; 4],
        depth: f32,
    },

    /// Polygon with arbitrary number of points (inline up to 8)
    Polygon {
        points: SmallVec<[(f32, f32); 8]>,
        color: [f32; 4],
        depth: f32,
    },

    /// Arc segment
    Arc {
        center_x: f32,
        center_y: f32,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
        stroke_width: f32,
        color: [f32; 4],
        depth: f32,
    },

    /// Image rectangle
    Image {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [f32; 4],
        coords: [f32; 4],
        has_alpha: bool,
        depth: f32,
        atlas_layer: i32,
    },
}

/// Render metadata for content
#[derive(Debug, Clone)]
pub struct ContentRenderData {
    /// Position where the content should be rendered [x, y]
    pub position: [f32; 2],
    /// Depth value for z-ordering
    pub depth: f32,
    /// Whether this content should be hidden during rendering
    pub hidden: bool,
    /// Whether this content needs to be repainted
    pub needs_repaint: bool,
    /// Whether this content should be removed
    pub should_remove: bool,
}

impl Default for ContentRenderData {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0],
            depth: 0.0,
            hidden: false,
            needs_repaint: true, // Should paint initially
            should_remove: false,
        }
    }
}

impl ContentRenderData {
    /// Update position and mark for repaint if changed
    pub fn set_position(&mut self, x: f32, y: f32) {
        if self.position[0] != x || self.position[1] != y {
            self.position = [x, y];
            self.needs_repaint = true;
        }
    }

    /// Set visibility and mark for repaint if changed
    pub fn set_hidden(&mut self, hidden: bool) {
        if self.hidden != hidden {
            self.hidden = hidden;
            self.needs_repaint = true;
        }
    }

    /// Mark for removal
    pub fn mark_for_removal(&mut self) {
        self.should_remove = true;
    }

    /// Clear repaint flag after rendering
    pub fn clear_repaint_flag(&mut self) {
        self.needs_repaint = false;
    }
}

/// Unified content state that wraps data and render metadata
#[derive(Debug, Clone)]
pub struct ContentState {
    pub data: ContentData,
    pub render_data: ContentRenderData,
}

impl ContentState {
    pub fn new(data: ContentData) -> Self {
        Self {
            data,
            render_data: ContentRenderData::default(),
        }
    }

    /// Check if this content is text
    pub fn is_text(&self) -> bool {
        matches!(self.data, ContentData::Text(_))
    }

    /// Get text data if this is a text content
    pub fn as_text(&self) -> Option<&BuilderState> {
        match &self.data {
            ContentData::Text(state) => Some(state),
            _ => None,
        }
    }

    /// Get mutable text data if this is a text content
    pub fn as_text_mut(&mut self) -> Option<&mut BuilderState> {
        match &mut self.data {
            ContentData::Text(state) => Some(state),
            _ => None,
        }
    }
}
