// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

/// Rendering metadata for rich text elements
#[derive(Debug, Clone)]
pub struct RichTextRenderData {
    /// Position where the rich text should be rendered [x, y]
    pub position: [f32; 2],
    /// Depth value for z-ordering (more negative = closer to camera/in front)
    pub depth: f32,
    /// Whether this rich text should be hidden during rendering
    pub hidden: bool,
    /// Whether this rich text needs to be repainted
    pub needs_repaint: bool,
    /// Whether this rich text should be removed
    pub should_remove: bool,
}

impl Default for RichTextRenderData {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0],
            depth: 0.0,          // Default depth
            hidden: false,       // Visible by default
            needs_repaint: true, // Should paint initially
            should_remove: false,
        }
    }
}

impl RichTextRenderData {
    /// Create new render data with position
    pub fn new(x: f32, y: f32) -> Self {
        Self {
            position: [x, y],
            depth: 0.0,
            hidden: false,
            needs_repaint: true,
            should_remove: false,
        }
    }

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
