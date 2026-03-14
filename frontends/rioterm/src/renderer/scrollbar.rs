// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use rio_backend::sugarloaf::Sugarloaf;
use std::time::Instant;

// Layout
const SCROLLBAR_WIDTH: f32 = 6.0;
const SCROLLBAR_MARGIN: f32 = 2.0;
const SCROLLBAR_MIN_THUMB_HEIGHT: f32 = 20.0;
// Wider hit area for easier grabbing
const SCROLLBAR_HIT_WIDTH: f32 = 14.0;

// Timing
const FADE_OUT_DELAY_MS: u128 = 2000;
const FADE_OUT_DURATION_MS: u128 = 300;

// Colors
const SCROLLBAR_COLOR: [f32; 4] = [0.6, 0.6, 0.6, 0.5];
const SCROLLBAR_DRAG_COLOR: [f32; 4] = [0.7, 0.7, 0.7, 0.7];

// Depth / order (render on top of content but below overlays)
const DEPTH: f32 = -0.05;
const ORDER: u8 = 5;

/// Computed geometry of a scrollbar track and thumb in logical pixels.
pub struct ThumbGeometry {
    bar_x: f32,
    bar_y: f32,
    track_height: f32,
    thumb_y: f32,
    thumb_height: f32,
}

/// State for an active scrollbar drag operation.
#[derive(Clone, Copy)]
pub struct ScrollbarDragState {
    /// The rich_text_id of the panel being dragged
    pub rich_text_id: usize,
    /// Y offset within the thumb where the drag started (logical pixels)
    grab_offset_y: f32,
    /// Cached track geometry
    bar_y: f32,
    track_height: f32,
    thumb_height: f32,
    /// The history_size at drag start
    history_size: usize,
}

/// Cached scroll state for a panel, updated each frame.
#[derive(Clone, Copy)]
pub struct PanelScrollState {
    pub rich_text_id: usize,
    pub panel_rect: [f32; 4],
    pub display_offset: usize,
    pub history_size: usize,
    pub screen_lines: usize,
}

pub struct Scrollbar {
    enabled: bool,
    /// Timestamp of last scroll activity per panel (keyed by rich_text_id)
    last_scroll_times: Vec<(usize, Instant)>,
    /// Active drag state
    pub drag_state: Option<ScrollbarDragState>,
    /// Cached per-panel scroll state, updated each render frame
    panel_states: Vec<PanelScrollState>,
}

impl Scrollbar {
    pub fn new(enabled: bool) -> Self {
        Scrollbar {
            enabled,
            last_scroll_times: Vec::new(),
            drag_state: None,
            panel_states: Vec::new(),
        }
    }

    /// Clear panel states before collecting new ones for this frame.
    pub fn clear_panel_states(&mut self) {
        self.panel_states.clear();
    }

    /// Add a panel's scroll state for this frame.
    pub fn push_panel_state(&mut self, state: PanelScrollState) {
        self.panel_states.push(state);
    }

    /// Get the cached panel states for rendering.
    pub fn panel_states(&self) -> &[PanelScrollState] {
        &self.panel_states
    }

    /// Notify the scrollbar that a scroll happened in the given panel.
    #[inline]
    pub fn notify_scroll(&mut self, rich_text_id: usize) {
        if !self.enabled {
            return;
        }
        let now = Instant::now();
        if let Some(entry) = self
            .last_scroll_times
            .iter_mut()
            .find(|(id, _)| *id == rich_text_id)
        {
            entry.1 = now;
        } else {
            self.last_scroll_times.push((rich_text_id, now));
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn is_dragging(&self) -> bool {
        self.drag_state.is_some()
    }

    /// Compute the current opacity for a panel's scrollbar.
    fn opacity_for(&self, rich_text_id: usize) -> f32 {
        if self
            .drag_state
            .map_or(false, |d| d.rich_text_id == rich_text_id)
        {
            return 1.0;
        }
        let entry = self
            .last_scroll_times
            .iter()
            .find(|(id, _)| *id == rich_text_id);

        let last_scroll = match entry {
            Some((_, t)) => t,
            None => return 0.0,
        };

        let elapsed = last_scroll.elapsed().as_millis();
        if elapsed < FADE_OUT_DELAY_MS {
            1.0
        } else {
            let fade_elapsed = elapsed - FADE_OUT_DELAY_MS;
            if fade_elapsed >= FADE_OUT_DURATION_MS {
                0.0
            } else {
                1.0 - (fade_elapsed as f32 / FADE_OUT_DURATION_MS as f32)
            }
        }
    }

    /// Compute thumb geometry in logical pixels.
    fn compute_thumb(
        panel_rect: [f32; 4],
        scale_factor: f32,
        display_offset: usize,
        history_size: usize,
        screen_lines: usize,
        grid_margin: (f32, f32),
    ) -> ThumbGeometry {
        let total_lines = history_size + screen_lines;

        let panel_x = (panel_rect[0] + grid_margin.0) / scale_factor;
        let panel_y = (panel_rect[1] + grid_margin.1) / scale_factor;
        let panel_width = panel_rect[2] / scale_factor;
        let panel_height = panel_rect[3] / scale_factor;

        let bar_x = panel_x + panel_width - SCROLLBAR_WIDTH - SCROLLBAR_MARGIN;
        let bar_y = panel_y + SCROLLBAR_MARGIN;
        let track_height = panel_height - SCROLLBAR_MARGIN * 2.0;

        let thumb_ratio = screen_lines as f32 / total_lines as f32;
        let thumb_height = (track_height * thumb_ratio).max(SCROLLBAR_MIN_THUMB_HEIGHT);

        let scroll_ratio = if history_size > 0 {
            display_offset as f32 / history_size as f32
        } else {
            0.0
        };
        let thumb_y = bar_y + (1.0 - scroll_ratio) * (track_height - thumb_height);

        ThumbGeometry {
            bar_x,
            bar_y,
            track_height,
            thumb_y,
            thumb_height,
        }
    }

    /// Test if a click at (mouse_x, mouse_y) in logical pixels hits the scrollbar
    /// track area for a given panel. If it hits the thumb, returns the grab offset.
    /// If it hits the track (but not thumb), returns None for offset (jump-scroll).
    ///
    /// Returns `Some((grab_offset_y, geometry))` if hit, `None` if miss.
    pub fn hit_test(
        &self,
        mouse_x: f32,
        mouse_y: f32,
        panel_rect: [f32; 4],
        scale_factor: f32,
        display_offset: usize,
        history_size: usize,
        screen_lines: usize,
        grid_margin: (f32, f32),
    ) -> Option<(Option<f32>, ThumbGeometry)> {
        if !self.enabled || history_size == 0 {
            return None;
        }

        let geom = Self::compute_thumb(
            panel_rect,
            scale_factor,
            display_offset,
            history_size,
            screen_lines,
            grid_margin,
        );

        // Use wider hit area for easier grabbing
        let hit_x = geom.bar_x - (SCROLLBAR_HIT_WIDTH - SCROLLBAR_WIDTH) / 2.0;
        let hit_width = SCROLLBAR_HIT_WIDTH;

        if mouse_x < hit_x || mouse_x > hit_x + hit_width {
            return None;
        }
        if mouse_y < geom.bar_y || mouse_y > geom.bar_y + geom.track_height {
            return None;
        }

        // Check if clicking on the thumb itself
        if mouse_y >= geom.thumb_y && mouse_y <= geom.thumb_y + geom.thumb_height {
            let grab_offset = mouse_y - geom.thumb_y;
            Some((Some(grab_offset), geom))
        } else {
            // Clicked on track but not thumb - jump scroll
            Some((None, geom))
        }
    }

    /// Start a drag operation. `grab_offset_y` is the offset within the thumb,
    /// or None to center the thumb on the click position.
    pub fn start_drag(
        &mut self,
        rich_text_id: usize,
        grab_offset_y: Option<f32>,
        geom: &ThumbGeometry,
        history_size: usize,
    ) {
        let grab_offset = grab_offset_y.unwrap_or(geom.thumb_height / 2.0);
        self.drag_state = Some(ScrollbarDragState {
            rich_text_id,
            grab_offset_y: grab_offset,
            bar_y: geom.bar_y,
            track_height: geom.track_height,
            thumb_height: geom.thumb_height,
            history_size,
        });
        self.notify_scroll(rich_text_id);
    }

    /// Update scroll position during drag. Returns the new display_offset.
    pub fn drag_update(&mut self, mouse_y: f32) -> Option<usize> {
        let state = self.drag_state?;
        let thumb_top = mouse_y - state.grab_offset_y;
        let available = state.track_height - state.thumb_height;
        if available <= 0.0 {
            return Some(0);
        }
        // Clamp thumb position
        let clamped = (thumb_top - state.bar_y).clamp(0.0, available);
        // Convert position to scroll ratio (top=0 → scroll_ratio=1, bottom=available → scroll_ratio=0)
        let scroll_ratio = 1.0 - (clamped / available);
        let display_offset = (scroll_ratio * state.history_size as f32).round() as usize;
        let display_offset = display_offset.min(state.history_size);

        self.notify_scroll(state.rich_text_id);
        Some(display_offset)
    }

    /// End the drag operation.
    pub fn end_drag(&mut self) {
        if let Some(state) = self.drag_state.take() {
            self.notify_scroll(state.rich_text_id);
        }
    }

    /// Render a scrollbar for a given panel.
    pub fn render(
        &self,
        sugarloaf: &mut Sugarloaf,
        panel_rect: [f32; 4],
        scale_factor: f32,
        display_offset: usize,
        history_size: usize,
        screen_lines: usize,
        rich_text_id: usize,
        grid_margin: (f32, f32),
    ) {
        if !self.enabled || history_size == 0 {
            return;
        }

        let opacity = self.opacity_for(rich_text_id);
        if opacity <= 0.0 {
            return;
        }

        let geom = Self::compute_thumb(
            panel_rect,
            scale_factor,
            display_offset,
            history_size,
            screen_lines,
            grid_margin,
        );

        let is_dragging = self
            .drag_state
            .map_or(false, |d| d.rich_text_id == rich_text_id);
        let base_color = if is_dragging {
            SCROLLBAR_DRAG_COLOR
        } else {
            SCROLLBAR_COLOR
        };

        let color = [
            base_color[0],
            base_color[1],
            base_color[2],
            base_color[3] * opacity,
        ];

        sugarloaf.rect(
            None,
            geom.bar_x,
            geom.thumb_y,
            SCROLLBAR_WIDTH,
            geom.thumb_height,
            color,
            DEPTH,
            ORDER,
        );
    }

    /// Returns true if any scrollbar is still animating (visible or fading).
    pub fn needs_redraw(&mut self) -> bool {
        if !self.enabled {
            return false;
        }
        if self.drag_state.is_some() {
            return true;
        }
        // Prune fully faded entries and check if any are still active
        let deadline = FADE_OUT_DELAY_MS + FADE_OUT_DURATION_MS;
        self.last_scroll_times
            .retain(|(_, t)| t.elapsed().as_millis() < deadline);
        !self.last_scroll_times.is_empty()
    }
}
