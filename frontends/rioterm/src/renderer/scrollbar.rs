// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use rio_backend::sugarloaf::Sugarloaf;
use std::time::Instant;

// Layout. Kept `pub` so other UI elements (command palette, future
// overlays) can render a scrollbar that matches the terminal's exactly
// without duplicating the numbers.
pub const SCROLLBAR_WIDTH: f32 = 6.0;
pub const SCROLLBAR_MARGIN: f32 = 2.0;
pub const SCROLLBAR_MIN_THUMB_HEIGHT: f32 = 20.0;
// Wider hit area for easier grabbing (terminal-only, no palette drag)
const SCROLLBAR_HIT_WIDTH: f32 = 14.0;

// Timing
pub const FADE_OUT_DELAY_MS: u128 = 2000;
pub const FADE_OUT_DURATION_MS: u128 = 300;

// Colors
pub const SCROLLBAR_COLOR: [f32; 4] = [0.6, 0.6, 0.6, 0.5];
pub const SCROLLBAR_DRAG_COLOR: [f32; 4] = [0.7, 0.7, 0.7, 0.7];

// Depth / order for the terminal-surface scrollbar (render on top of
// content but below overlays). Palette / other UIs pick their own
// values via `draw_thumb`'s parameters.
pub const TERMINAL_DEPTH: f32 = 0.0;
pub const TERMINAL_ORDER: u8 = 5;

/// Fade-in/out opacity for a scrollbar given the timestamp of the most
/// recent scroll event (`None` = never scrolled). `dragging` pins it to
/// fully opaque so a slow drag doesn't fade out under the user's cursor.
///
/// Matches the terminal scrollbar's envelope:
/// - 0.0 before any scroll ever happened
/// - 1.0 for the first `FADE_OUT_DELAY_MS` after a scroll
/// - linear fade over `FADE_OUT_DURATION_MS` back to 0.0
pub fn opacity_from_last_scroll(
    last_scroll: Option<Instant>,
    dragging: bool,
) -> f32 {
    if dragging {
        return 1.0;
    }
    let last_scroll = match last_scroll {
        Some(t) => t,
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

/// Thumb geometry (y offset, height) inside a vertical track of
/// `track_height` anchored at `track_top`. Returns `None` when the
/// list fits entirely (`visible >= total`) — caller skips drawing.
///
/// `normalized_offset` is the scroll position in `[0.0, 1.0]` where
/// 0.0 = top (unscrolled) and 1.0 = maximum scroll. Callers that
/// think in "scroll from the top" (command palette) and callers that
/// think in "scroll back from live edge" (terminal history) both
/// plug into the same geometry by normalizing on their side.
///
/// Thumb height is clamped at `SCROLLBAR_MIN_THUMB_HEIGHT` so very
/// long lists don't shrink the thumb to a sub-pixel sliver.
pub fn compute_thumb(
    visible: usize,
    total: usize,
    track_top: f32,
    track_height: f32,
    normalized_offset: f32,
) -> Option<(f32, f32)> {
    if total <= visible || track_height <= 0.0 {
        return None;
    }
    let ratio = visible as f32 / total as f32;
    let thumb_height =
        (track_height * ratio).clamp(SCROLLBAR_MIN_THUMB_HEIGHT.min(track_height), track_height);
    let scrollable = (track_height - thumb_height).max(0.0);
    let progress = normalized_offset.clamp(0.0, 1.0);
    Some((track_top + scrollable * progress, thumb_height))
}

/// Paint a single scrollbar thumb — the one and only way rio renders a
/// scrollbar. Uses `SCROLLBAR_COLOR` (or `SCROLLBAR_DRAG_COLOR` if
/// `dragging`) modulated by `opacity`. `opacity <= 0.0` is a no-op so
/// callers can pipe the fade helper straight in.
///
/// `depth` + `order` let callers place the thumb above their own
/// background layers: the terminal uses `TERMINAL_DEPTH` /
/// `TERMINAL_ORDER` so the bar lives on top of the cell content, the
/// command palette uses a higher order so the bar isn't swallowed by
/// the palette's backdrop/bg rects.
#[allow(clippy::too_many_arguments)]
pub fn draw_thumb(
    sugarloaf: &mut Sugarloaf,
    x: f32,
    y: f32,
    height: f32,
    opacity: f32,
    dragging: bool,
    depth: f32,
    order: u8,
) {
    if opacity <= 0.0 {
        return;
    }
    let base = if dragging {
        SCROLLBAR_DRAG_COLOR
    } else {
        SCROLLBAR_COLOR
    };
    let color = [base[0], base[1], base[2], base[3] * opacity];
    sugarloaf.rect(None, x, y, SCROLLBAR_WIDTH, height, color, depth, order);
}

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

    /// Compute the current opacity for a panel's scrollbar. Thin
    /// wrapper around the module-level `opacity_from_last_scroll`
    /// helper so the terminal and command palette share the same
    /// fade envelope.
    fn opacity_for(&self, rich_text_id: usize) -> f32 {
        let dragging = self
            .drag_state
            .is_some_and(|d| d.rich_text_id == rich_text_id);
        let last_scroll = self
            .last_scroll_times
            .iter()
            .find(|(id, _)| *id == rich_text_id)
            .map(|(_, t)| *t);
        opacity_from_last_scroll(last_scroll, dragging)
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
    #[allow(clippy::too_many_arguments)]
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
    #[allow(clippy::too_many_arguments)]
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
            .is_some_and(|d| d.rich_text_id == rich_text_id);

        draw_thumb(
            sugarloaf,
            geom.bar_x,
            geom.thumb_y,
            geom.thumb_height,
            opacity,
            is_dragging,
            TERMINAL_DEPTH,
            TERMINAL_ORDER,
        );
    }

    /// Test hook: direct access to the per-panel last-scroll timestamp
    /// so tests can seed / read it without reaching through
    /// `notify_scroll` + clock manipulation.
    #[cfg(test)]
    fn last_scroll_for(&self, rich_text_id: usize) -> Option<Instant> {
        self.last_scroll_times
            .iter()
            .find(|(id, _)| *id == rich_text_id)
            .map(|(_, t)| *t)
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

#[cfg(test)]
mod tests {
    use super::*;

    // --- opacity_from_last_scroll ----------------------------------------

    #[test]
    fn opacity_zero_when_never_scrolled() {
        assert_eq!(opacity_from_last_scroll(None, false), 0.0);
    }

    #[test]
    fn opacity_one_while_dragging_regardless_of_last_scroll() {
        // Dragging pins the thumb at full alpha so it doesn't fade
        // out from under the cursor during a slow drag.
        assert_eq!(opacity_from_last_scroll(None, true), 1.0);
        let old = Instant::now() - std::time::Duration::from_secs(10);
        assert_eq!(opacity_from_last_scroll(Some(old), true), 1.0);
    }

    #[test]
    fn opacity_one_inside_visibility_window() {
        // A scroll that just happened is fully visible.
        let now = Instant::now();
        assert_eq!(opacity_from_last_scroll(Some(now), false), 1.0);
    }

    #[test]
    fn opacity_zero_after_full_fade() {
        // Past FADE_OUT_DELAY + FADE_OUT_DURATION, the thumb is gone.
        let deep_past = Instant::now()
            - std::time::Duration::from_millis(
                (FADE_OUT_DELAY_MS + FADE_OUT_DURATION_MS + 50) as u64,
            );
        assert_eq!(opacity_from_last_scroll(Some(deep_past), false), 0.0);
    }

    // --- compute_thumb ---------------------------------------------------

    #[test]
    fn compute_thumb_hidden_when_list_fits() {
        assert!(compute_thumb(8, 8, 0.0, 256.0, 0.0).is_none());
        assert!(compute_thumb(8, 7, 0.0, 256.0, 0.0).is_none());
    }

    #[test]
    fn compute_thumb_hidden_on_zero_track() {
        assert!(compute_thumb(8, 100, 0.0, 0.0, 0.0).is_none());
    }

    #[test]
    fn compute_thumb_top_at_zero_offset() {
        let track_top = 42.0;
        let (thumb_y, thumb_h) = compute_thumb(8, 100, track_top, 200.0, 0.0).unwrap();
        assert_eq!(thumb_y, track_top);
        assert!(thumb_h >= SCROLLBAR_MIN_THUMB_HEIGHT);
        assert!(thumb_h <= 200.0);
    }

    #[test]
    fn compute_thumb_bottom_at_full_offset() {
        let (thumb_y, thumb_h) = compute_thumb(8, 100, 0.0, 200.0, 1.0).unwrap();
        assert!((thumb_y + thumb_h - 200.0).abs() < 0.001);
    }

    #[test]
    fn compute_thumb_clamps_excess_offset() {
        // Normalized offsets outside [0, 1] are clamped (defensive
        // against future resize / filter-shrink races).
        let (thumb_y, thumb_h) = compute_thumb(8, 20, 0.0, 200.0, 3.5).unwrap();
        assert!((thumb_y + thumb_h - 200.0).abs() < 0.001);
        let (thumb_y, _) = compute_thumb(8, 20, 0.0, 200.0, -0.7).unwrap();
        assert_eq!(thumb_y, 0.0);
    }

    #[test]
    fn compute_thumb_respects_minimum_height() {
        // Huge lists would give a sub-pixel thumb by proportion alone;
        // the min-height clamp keeps it visible.
        let (_, thumb_h) = compute_thumb(8, 10_000, 0.0, 200.0, 0.0).unwrap();
        assert!(thumb_h >= SCROLLBAR_MIN_THUMB_HEIGHT);
    }

    // --- Scrollbar::notify_scroll + opacity_for --------------------------

    #[test]
    fn notify_scroll_stores_timestamp_per_panel() {
        let mut bar = Scrollbar::new(true);
        assert!(bar.last_scroll_for(7).is_none());
        bar.notify_scroll(7);
        assert!(bar.last_scroll_for(7).is_some());
        assert!(bar.last_scroll_for(99).is_none());
    }

    #[test]
    fn notify_scroll_noop_when_disabled() {
        let mut bar = Scrollbar::new(false);
        bar.notify_scroll(1);
        assert!(bar.last_scroll_for(1).is_none());
    }
}
