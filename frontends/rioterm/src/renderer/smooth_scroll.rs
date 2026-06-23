// Sub-pixel scroll offset for smooth scrolling.
//
// Manages a fractional vertical offset that shifts the terminal grid
// visually between integer scroll positions. The integer `display_offset`
// is updated immediately; this module handles only the fractional
// remainder.
//
// Sign convention:
//   - Positive delta_y = user scrolls to see older content (scroll up)
//   - Positive offset_y = content should slide DOWN to reveal older rows
//   - In the shader, offset_y is ADDED to grid_padding.x so the grid
//     origin moves down, causing content to slide down.

/// Threshold below which the offset is snapped to zero (physical pixels).
const SETTLE_THRESHOLD: f32 = 0.5;

pub struct SmoothScroll {
    /// Current sub-pixel offset in physical pixels.
    /// Positive = content shifted downward (revealing older content at top).
    offset_y: f32,
    /// Whether we're in trackpad direct-drive mode (true between
    /// TouchPhase::Started and TouchPhase::Ended).
    trackpad_active: bool,
    /// Accumulated pixel delta for the current event batch.
    accumulated: f64,
}

impl SmoothScroll {
    pub fn new() -> Self {
        Self {
            offset_y: 0.0,
            trackpad_active: false,
            accumulated: 0.0,
        }
    }

    /// Feed a sub-pixel scroll delta (from trackpad or accumulated mouse
    /// wheel pixels). Returns the integer line count that should be applied
    /// to `display_offset`, and stores the fractional remainder internally.
    ///
    /// `delta_y` is in physical pixels after multiplier/divider.
    /// `cell_height` is one row height in physical pixels.
    pub fn feed_pixel_delta(&mut self, delta_y: f64, cell_height: f32) -> i32 {
        self.accumulated += delta_y;

        let total_offset = self.offset_y + self.accumulated as f32;
        self.accumulated = 0.0;

        // Truncate toward zero — matches the original `as i32` cast used
        // by the old scroll code and handles negative deltas correctly.
        let lines = (total_offset / cell_height) as i32;

        // Keep the fractional remainder.
        self.offset_y = total_offset - lines as f32 * cell_height;

        lines
    }

    /// Called when trackpad TouchPhase::Started.
    pub fn trackpad_started(&mut self) {
        self.trackpad_active = true;
    }

    /// Called when trackpad TouchPhase::Ended or Cancelled.
    /// macOS continues sending momentum events via Moved, so we just
    /// leave the offset as-is. No spring animation needed.
    pub fn trackpad_ended(&mut self) {
        self.trackpad_active = false;
        // Snap tiny residuals to zero to avoid a persistent sub-pixel
        // offset when the user is done scrolling.
        if self.offset_y.abs() < SETTLE_THRESHOLD {
            self.offset_y = 0.0;
        }
    }

    /// No-op — the offset is driven purely by scroll events, not by
    /// spring animation. Kept as a no-op so the call site doesn't need
    /// to change.
    pub fn update(&mut self) -> bool {
        false
    }

    /// Current sub-pixel offset for the shader uniform (physical pixels).
    #[inline]
    pub fn offset_y(&self) -> f32 {
        self.offset_y
    }

    /// Whether an extra row should be fetched for partial reveal.
    /// Only true for positive offsets (scrolling up / revealing history).
    #[inline]
    pub fn needs_extra_row(&self) -> bool {
        self.offset_y > 0.01
    }

    /// Whether the animation loop should keep requesting redraws.
    /// Always false — redraws are triggered by set_dirty() in scroll().
    #[inline]
    pub fn is_animating(&self) -> bool {
        false
    }

    /// Reset all state. Called on PageUp/PageDown, resize, terminal output
    /// while scrolled up, etc.
    pub fn reset(&mut self) {
        self.offset_y = 0.0;
        self.trackpad_active = false;
        self.accumulated = 0.0;
    }

    /// Clamp the offset so it doesn't try to reveal content past the
    /// scrollback boundaries.
    ///
    /// - `at_bottom`: display_offset == 0 → can't scroll further down,
    ///   so negative offset (revealing content below) is clamped to 0.
    /// - `at_top`: display_offset == history_size → can't scroll further
    ///   up, so positive offset (revealing content above) is clamped to 0.
    pub fn clamp_at_boundary(&mut self, at_bottom: bool, at_top: bool) {
        if at_bottom && self.offset_y < 0.0 {
            self.offset_y = 0.0;
        }
        if at_top && self.offset_y > 0.0 {
            self.offset_y = 0.0;
        }
    }
}
