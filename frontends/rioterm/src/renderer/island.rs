// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// island.rs was originally retired from boo editor
// which is licensed under MIT license.

use crate::context::ContextManager;
use crate::renderer::spring::Spring;
use rio_backend::event::{EventProxy, ProgressReport, ProgressState};
use rio_backend::sugarloaf::text::DrawOpts;
use rio_backend::sugarloaf::{Attributes, Sugarloaf};
use rustc_hash::FxHashMap;
use std::borrow::Cow;
use std::time::Instant;

pub const ISLAND_HEIGHT: f32 = 34.0;
const PROGRESS_BAR_HEIGHT: f32 = 3.0;

const PROGRESS_BAR_TIMEOUT_SECS: u64 = 15;
const TITLE_FONT_SIZE: f32 = 12.0;

const TAB_PADDING_X: f32 = 24.0;
const TITLE_ELLIPSIS: char = '…';
const DRAG_THRESHOLD: f32 = 4.0;
const DRAG_ANIMATION_LENGTH: f32 = 0.15;
const DRAG_MAX_DT: f32 = 0.05;
pub const ISLAND_MARGIN_RIGHT: f32 = 8.0;

/// Color picker constants
const PICKER_SWATCH_SIZE: f32 = 18.0;
const PICKER_SWATCH_GAP: f32 = 4.0;
const PICKER_PADDING: f32 = 6.0;
const PICKER_INPUT_HEIGHT: f32 = 26.0;
const PICKER_INPUT_FONT_SIZE: f32 = 12.0;
const PICKER_INPUT_MARGIN_TOP: f32 = 8.0;
const PICKER_TOP_PADDING: f32 = 4.0;
const PICKER_HEIGHT: f32 = PICKER_TOP_PADDING
    + PICKER_SWATCH_SIZE
    + PICKER_PADDING * 2.0
    + PICKER_INPUT_MARGIN_TOP
    + PICKER_INPUT_HEIGHT
    + PICKER_PADDING;
const PICKER_COLORS: [[f32; 4]; 6] = [
    // red
    [0.86, 0.26, 0.27, 1.0],
    // orange
    [0.90, 0.57, 0.22, 1.0],
    // yellow
    [0.85, 0.78, 0.25, 1.0],
    // green
    [0.34, 0.70, 0.38, 1.0],
    // blue
    [0.30, 0.55, 0.85, 1.0],
    // purple
    [0.68, 0.40, 0.80, 1.0],
];

/// Left margin on macOS to account for traffic light buttons
#[cfg(target_os = "macos")]
pub const ISLAND_MARGIN_LEFT_MACOS: f32 = 76.0;

struct TabDrag {
    // Index of the dragged tab, follows the tab as it reorders.
    tab_index: usize,
    // Mouse x at press (unscaled), for the drag threshold.
    press_x: f32,
    // press_x − tab_left_x, keeps the grab point under the cursor.
    grab_offset: f32,
    // Latest unscaled mouse x.
    current_x: f32,
    // True once movement exceeded `DRAG_THRESHOLD`.
    started: bool,
}

fn fit_title_to_width<'a>(
    sugarloaf: &mut Sugarloaf,
    title: &'a str,
    max_width: f32,
) -> Cow<'a, str> {
    let attrs = Attributes::default();
    fit_title_with_widths(title, max_width, |c| {
        sugarloaf.char_advance(c, attrs, TITLE_FONT_SIZE)
    })
}

fn fit_title_with_widths<'a>(
    title: &'a str,
    max_width: f32,
    mut char_width: impl FnMut(char) -> f32,
) -> Cow<'a, str> {
    let suffix_width = char_width(TITLE_ELLIPSIS);

    // `truncate_ix` tracks the last byte offset at which the prefix so
    // far still has room for the suffix. Updated before adding the next
    // char's width so the moment we detect overflow we already know
    // where to cut.
    let mut accumulated: f32 = 0.0;
    let mut truncate_ix: usize = 0;
    for (ix, c) in title.char_indices() {
        if accumulated + suffix_width <= max_width {
            truncate_ix = ix;
        }
        accumulated += char_width(c);
        if accumulated > max_width {
            let mut out = String::with_capacity(truncate_ix + TITLE_ELLIPSIS.len_utf8());
            out.push_str(&title[..truncate_ix]);
            out.push(TITLE_ELLIPSIS);
            return Cow::Owned(out);
        }
    }
    Cow::Borrowed(title)
}

/// Equal-width tab strip geometry, in logical px. Single source of
/// truth shared by rendering, hit-testing, the color picker, and the
/// drag-reorder math.
#[derive(Clone, Copy, PartialEq)]
pub struct TabStripLayout {
    pub left_margin: f32,
    pub available_width: f32,
    pub tab_width: f32,
}

/// Compute the tab strip layout from the physical window width.
pub fn tab_strip_layout(
    window_width: f32,
    scale_factor: f32,
    num_tabs: usize,
) -> TabStripLayout {
    #[cfg(target_os = "macos")]
    let left_margin = ISLAND_MARGIN_LEFT_MACOS;
    #[cfg(not(target_os = "macos"))]
    let left_margin = 0.0;

    let available_width =
        (window_width / scale_factor) - ISLAND_MARGIN_RIGHT - left_margin;
    let tab_width = available_width / num_tabs.max(1) as f32;
    TabStripLayout {
        left_margin,
        available_width,
        tab_width,
    }
}

pub struct Island {
    pub hide_if_single: bool,
    pub inactive_text_color: [f32; 4],
    pub active_text_color: [f32; 4],
    pub border_color: [f32; 4],
    /// Current progress bar state
    progress_state: Option<ProgressState>,
    /// Current progress value (0-100)
    progress_value: Option<u8>,
    /// When the *current* state began. Reset only when transitioning into a
    /// new state, so the indeterminate animation phase is not yanked back to
    /// zero by repeated identical OSC 9;4 reports (issue #1509).
    progress_started_at: Option<Instant>,
    /// Last time we saw an OSC 9;4 report — bumped on every report, used by
    /// the stale-bar dismissal timer. Decoupled from `progress_started_at`
    /// for the same reason.
    progress_last_seen: Option<Instant>,
    /// Progress bar color
    pub progress_bar_color: [f32; 4],
    /// Progress bar error color
    pub progress_bar_error_color: [f32; 4],
    /// Which tab has the color picker open (None = closed)
    color_picker_tab: Option<usize>,
    /// Current rename input text while picker is open
    rename_input: String,
    /// Caret blink timer
    rename_caret_time: Instant,
    /// In-progress tab drag (reorder by dragging)
    drag: Option<TabDrag>,
    /// Per-tab x-offset springs: displaced tabs sliding into their slot
    /// and the released tab settling after a drag. Keyed by tab index.
    slide_springs: FxHashMap<usize, Spring>,
    /// Timestamp of the last spring advance, for per-frame dt.
    last_anim_frame: Instant,
}

impl Island {
    pub fn new(
        inactive_text_color: [f32; 4],
        active_text_color: [f32; 4],
        border_color: [f32; 4],
        hide_if_single: bool,
    ) -> Self {
        Self {
            hide_if_single,
            inactive_text_color,
            active_text_color,
            border_color,
            progress_state: None,
            progress_value: None,
            progress_started_at: None,
            progress_last_seen: None,
            // Default progress bar color (blue-ish)
            progress_bar_color: [0.3, 0.6, 1.0, 1.0],
            // Default error color (red-ish)
            progress_bar_error_color: [1.0, 0.3, 0.3, 1.0],
            color_picker_tab: None,
            rename_input: String::new(),
            rename_caret_time: Instant::now(),
            drag: None,
            slide_springs: FxHashMap::default(),
            last_anim_frame: Instant::now(),
        }
    }

    pub fn update_colors(
        &mut self,
        inactive_text_color: [f32; 4],
        active_text_color: [f32; 4],
        border_color: [f32; 4],
    ) {
        self.inactive_text_color = inactive_text_color;
        self.active_text_color = active_text_color;
        self.border_color = border_color;
    }

    /// Update the progress bar state from an OSC 9;4 report.
    ///
    /// `progress_last_seen` is bumped on every (non-Remove) report so the
    /// stale-bar dismissal timer keeps the bar alive while the TUI is
    /// actively reporting. `progress_started_at` is reset only when the
    /// state actually transitions, so a TUI sending the same `OSC 9;4;3`
    /// every 100 ms (issue #1509) doesn't yank the indeterminate animation
    /// phase back to zero on every report. Mirrors ghostty's split between
    /// `glib.timeoutAdd` (heartbeat) and `GtkProgressBar`'s internal pulse
    /// state (animation).
    pub fn set_progress_report(&mut self, report: ProgressReport) {
        match report.state {
            ProgressState::Remove => {
                self.progress_state = None;
                self.progress_value = None;
                self.progress_started_at = None;
                self.progress_last_seen = None;
            }
            new_state => {
                let now = Instant::now();
                self.progress_last_seen = Some(now);

                let transitioning = self.progress_state != Some(new_state);
                self.progress_state = Some(new_state);
                self.progress_value = report.progress;
                if transitioning {
                    self.progress_started_at = Some(now);
                }
            }
        }
    }

    /// Check if the island needs continuous rendering (for animations)
    pub fn needs_redraw(&self) -> bool {
        // A held drag doesn't need continuous frames: the floating tab
        // only moves on CursorMoved (which requests its own redraws);
        // only the slide springs animate between input events.
        matches!(self.progress_state, Some(ProgressState::Indeterminate))
            || !self.slide_springs.is_empty()
    }

    /// Arm a tab drag at mouse press. The drag only `started`s once the
    /// pointer moves past `DRAG_THRESHOLD`.
    pub fn start_drag(&mut self, tab_index: usize, grab_offset: f32, x: f32) {
        self.drag = Some(TabDrag {
            tab_index,
            press_x: x,
            grab_offset,
            current_x: x,
            started: false,
        });
    }

    /// Feed a mouse move into the armed drag. Returns `true` once the
    /// drag is active (threshold exceeded).
    pub fn update_drag(&mut self, x: f32) -> bool {
        match self.drag.as_mut() {
            Some(drag) => {
                drag.current_x = x;
                if !drag.started && (x - drag.press_x).abs() > DRAG_THRESHOLD {
                    drag.started = true;
                }
                drag.started
            }
            None => false,
        }
    }

    /// Whether a drag is armed or active.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Index of the dragged tab, if a drag is active.
    pub fn drag_index(&self) -> Option<usize> {
        self.drag
            .as_ref()
            .filter(|d| d.started)
            .map(|d| d.tab_index)
    }

    /// Left edge of the floating (dragged) tab, clamped to the tab strip.
    fn drag_floating_left(
        &self,
        left_margin: f32,
        available_width: f32,
        tab_width: f32,
    ) -> Option<f32> {
        let drag = self.drag.as_ref().filter(|d| d.started)?;
        let left = drag.current_x - drag.grab_offset;
        // `.max(0.0)` keeps the clamp range valid (min ≤ max) even if a
        // pathologically narrow window makes available_width < tab_width.
        let max_left = left_margin + (available_width - tab_width).max(0.0);
        Some(left.clamp(left_margin, max_left))
    }

    /// Center x of the floating tab — the reference point that decides
    /// which slot the drag targets.
    pub fn drag_center(
        &self,
        left_margin: f32,
        available_width: f32,
        tab_width: f32,
    ) -> Option<f32> {
        self.drag_floating_left(left_margin, available_width, tab_width)
            .map(|left| left + tab_width / 2.0)
    }

    /// Finish a drag: seed a settle spring from the floating position
    /// into the slot so the tab slides into place.
    pub fn end_drag(&mut self, left_margin: f32, available_width: f32, tab_width: f32) {
        if let (Some(floating_left), Some(drag)) = (
            self.drag_floating_left(left_margin, available_width, tab_width),
            self.drag.as_ref().filter(|d| d.started),
        ) {
            let slot_x = left_margin + drag.tab_index as f32 * tab_width;
            let offset = floating_left - slot_x;
            if offset.abs() > 0.01 {
                let spring = self
                    .slide_springs
                    .entry(drag.tab_index)
                    .or_insert_with(Spring::new);
                spring.position = offset;
            }
        }
        self.drag = None;
    }

    /// Drop an armed/active drag without any settle animation.
    pub fn cancel_drag(&mut self) {
        self.drag = None;
    }

    /// New index of tab `i` after the tab at `from` rotated to `to`.
    fn remap_index(i: usize, from: usize, to: usize) -> usize {
        if i == from {
            to
        } else if from < to && i > from && i <= to {
            i - 1
        } else if to < from && i >= to && i < from {
            i + 1
        } else {
            i
        }
    }

    /// Re-key all per-tab-index state after the tab at `from` moved to
    /// `to` (rotate semantics, matching
    /// `ContextManager::move_current_tab_to`), then seed slide springs
    /// on the displaced tabs so they animate into their new slot.
    pub fn remap_tab_move(&mut self, from: usize, to: usize, tab_width: f32) {
        if from == to {
            return;
        }

        self.slide_springs = self
            .slide_springs
            .drain()
            .map(|(i, v)| (Self::remap_index(i, from, to), v))
            .collect();
        if let Some(picker) = self.color_picker_tab {
            self.color_picker_tab = Some(Self::remap_index(picker, from, to));
        }
        if let Some(ref mut drag) = self.drag {
            drag.tab_index = Self::remap_index(drag.tab_index, from, to);
        }

        // Displaced tabs shifted one slot away from `from` toward `to`'s
        // side; seed (or accumulate into) a spring so each one starts at
        // its old x and slides to the new slot. The moved tab itself ends
        // at `to`, which both ranges exclude — while dragging it floats,
        // and on a keyboard move it jumps (no old position to animate
        // from that wouldn't fight the selection change).
        let (range, delta) = if from < to {
            // Tabs at from+1..=to moved left by one: now at from..to.
            (from..to, tab_width)
        } else {
            // Tabs at to..from moved right by one: now at to+1..=from.
            (to + 1..from + 1, -tab_width)
        };
        for i in range {
            let spring = self.slide_springs.entry(i).or_insert_with(Spring::new);
            spring.position += delta;
        }
    }

    /// Re-key per-tab state after tabs `a` and `b` swapped places —
    /// `ContextManager::move_current_to_prev/next` semantics, which swap
    /// (including the wrap-around end-to-end case) instead of rotating.
    /// Adjacent swaps get slide springs; wrap-around jumps don't (a
    /// full-bar slide reads as glitch, not motion).
    pub fn remap_tab_swap(&mut self, a: usize, b: usize, tab_width: f32) {
        if a == b {
            return;
        }

        let swap_key = |i: usize| {
            if i == a {
                b
            } else if i == b {
                a
            } else {
                i
            }
        };
        self.slide_springs = self
            .slide_springs
            .drain()
            .map(|(i, v)| (swap_key(i), v))
            .collect();
        if let Some(picker) = self.color_picker_tab {
            self.color_picker_tab = Some(swap_key(picker));
        }

        if a.abs_diff(b) == 1 {
            let delta = (b as f32 - a as f32) * tab_width;
            let spring = self.slide_springs.entry(a).or_insert_with(Spring::new);
            spring.position += delta;
            let spring = self.slide_springs.entry(b).or_insert_with(Spring::new);
            spring.position -= delta;
        }
    }

    /// Check if the progress bar should be auto-dismissed due to timeout.
    /// Uses `progress_last_seen` (heartbeat), not `progress_started_at`, so
    /// a long-running TUI that keeps reporting stays visible.
    fn check_progress_timeout(&mut self) {
        if let Some(last_seen) = self.progress_last_seen {
            if last_seen.elapsed().as_secs() >= PROGRESS_BAR_TIMEOUT_SECS {
                self.progress_state = None;
                self.progress_value = None;
                self.progress_started_at = None;
                self.progress_last_seen = None;
            }
        }
    }

    /// Render the progress bar below the island
    fn render_progress_bar(
        &mut self,
        sugarloaf: &mut Sugarloaf,
        window_width: f32,
        scale_factor: f32,
    ) {
        // Check for timeout first
        self.check_progress_timeout();

        let state = match self.progress_state {
            Some(s) => s,
            None => return, // No progress bar to render
        };

        let width = window_width / scale_factor;
        let y_position = ISLAND_HEIGHT;

        // Determine color based on state
        let color = match state {
            ProgressState::Error => self.progress_bar_error_color,
            _ => self.progress_bar_color,
        };

        match state {
            ProgressState::Remove => {
                // Should not reach here, but just in case
            }
            ProgressState::Set | ProgressState::Error | ProgressState::Pause => {
                // Render progress bar with specific percentage
                let progress = self.progress_value.unwrap_or(0) as f32 / 100.0;
                let bar_width = width * progress;

                if bar_width > 0.0 {
                    sugarloaf.rect(
                        None,
                        0.0,
                        y_position,
                        bar_width,
                        PROGRESS_BAR_HEIGHT,
                        color,
                        0.0, // Same depth as other rects
                        0,
                    );
                }
            }
            ProgressState::Indeterminate => {
                // For indeterminate, show a pulsing/moving indicator.
                // Phase is anchored to `progress_started_at` (set only on
                // state transition) — using `progress_last_seen` here would
                // freeze the bar at position 0 for any TUI that heartbeats
                // its OSC 9;4;3 faster than `cycle_ms`. (Issue #1509.)
                let elapsed = self
                    .progress_started_at
                    .map(|t| t.elapsed().as_millis() as f32)
                    .unwrap_or(0.0);

                // Move the bar from left to right over 2 seconds, then repeat
                let cycle_ms = 2000.0;
                let position = (elapsed % cycle_ms) / cycle_ms;
                let bar_fraction = 0.2; // 20% of width
                let bar_width = width * bar_fraction;
                let x_pos = position * (width - bar_width);

                sugarloaf.rect(
                    None,
                    x_pos,
                    y_position,
                    bar_width,
                    PROGRESS_BAR_HEIGHT,
                    color,
                    0.0,
                    0,
                );
            }
        }
    }

    /// Get the height of the island
    #[inline]
    pub fn height(&self) -> f32 {
        ISLAND_HEIGHT
    }

    /// Render tabs using equal-width layout
    #[inline]
    pub fn render(
        &mut self,
        sugarloaf: &mut Sugarloaf,
        dimensions: (f32, f32, f32),
        context_manager: &ContextManager<EventProxy>,
        bg_color: [f32; 4],
    ) {
        let (window_width, _window_height, scale_factor) = dimensions;
        let num_tabs = context_manager.len();
        let current_tab_index = context_manager.current_index();

        // Immediate-mode: no cached ids to hide. If we early-return
        // without drawing, the tabs just don't appear this frame.
        if self.hide_if_single && num_tabs == 1 {
            // No tab strip — drop any leftover drag/slide state so
            // `needs_redraw` doesn't keep frames alive for invisible
            // tabs.
            self.drag = None;
            self.slide_springs.clear();
            self.render_progress_bar(sugarloaf, window_width, scale_factor);
            return;
        }

        // A reorder that didn't come from this drag (tab closed via
        // shell exit, keyboard move) breaks the drag.tab_index ==
        // current_index invariant — drop the drag instead of floating
        // a phantom tab over the wrong slot.
        if self
            .drag
            .as_ref()
            .is_some_and(|d| d.tab_index != current_tab_index)
        {
            self.drag = None;
        }

        // Advance the slide springs (drag-reorder animation) by this
        // frame's dt; settled springs drop out of the map.
        let now = Instant::now();
        let dt = now
            .duration_since(self.last_anim_frame)
            .as_secs_f32()
            .min(DRAG_MAX_DT);
        self.last_anim_frame = now;
        self.slide_springs
            .retain(|_, s| s.update(dt, DRAG_ANIMATION_LENGTH));

        let TabStripLayout {
            left_margin,
            available_width,
            tab_width,
        } = tab_strip_layout(window_width, scale_factor, num_tabs);

        // Starting from left edge (with margin on macOS for traffic lights)
        let mut x_position = left_margin;

        // Active drag: the dragged tab is skipped in the slot loop and
        // drawn floating (after the loop, on a higher layer) instead.
        let drag_index = self.drag_index();
        let floating_left =
            self.drag_floating_left(left_margin, available_width, tab_width);

        // Draw bottom border for the left margin area (traffic light space on macOS)
        if left_margin > 0.0 {
            sugarloaf.rect(
                None,
                0.0,
                ISLAND_HEIGHT - 1.0,
                left_margin,
                0.5,
                self.border_color,
                0.1,
                0,
            );
        }

        // Render each tab
        for tab_index in 0..num_tabs {
            // The dragged tab floats — drawn after the loop instead.
            if Some(tab_index) == drag_index {
                x_position += tab_width;
                continue;
            }

            let is_active = tab_index == current_tab_index;

            // Slot position plus any slide-spring offset (tab still
            // animating into its slot after a reorder).
            let tab_x = x_position
                + self
                    .slide_springs
                    .get(&tab_index)
                    .map_or(0.0, |s| s.position);

            // Get title for this tab, then truncate with a trailing
            // ellipsis so overflowing titles can't bleed into the next
            // tab or past the left edge (issue #1508).
            let raw_title = self.get_title_for_tab(context_manager, tab_index);
            if raw_title.is_empty() {
                x_position += tab_width;
                continue;
            }
            let max_text_width = (tab_width - TAB_PADDING_X * 2.0).max(0.0);
            let title = fit_title_to_width(sugarloaf, &raw_title, max_text_width);

            let text_color = if is_active {
                self.active_text_color
            } else {
                self.inactive_text_color
            };

            let title_opts = DrawOpts {
                font_size: TITLE_FONT_SIZE,
                color: color_u8(text_color),
                ..DrawOpts::default()
            };

            // UI text always paints in a final pass above every rect,
            // so the floating tab's opaque background can't occlude
            // titles passing underneath it — skip a title once the
            // floating tab intrudes past the slot's text padding (the
            // widest a centered title can reach).
            let hidden_by_drag = floating_left.is_some_and(|fl| {
                let overlap =
                    (tab_x + tab_width).min(fl + tab_width) - tab_x.max(fl);
                overlap > TAB_PADDING_X
            });

            if !hidden_by_drag {
                // Measure → centre → draw. Immediate mode, no cached
                // text_id bookkeeping.
                let ui = sugarloaf.text_mut();
                let text_width = ui.measure(&title, &title_opts);
                let text_x = tab_x + (tab_width - text_width) / 2.0;
                let text_y = (ISLAND_HEIGHT / 2.0) - (TITLE_FONT_SIZE / 2.);
                ui.draw(text_x, text_y, &title, &title_opts);
            }

            // Draw tab background color if set
            if let Some(bg_color) = context_manager.custom_color(tab_index) {
                sugarloaf.rect(
                    None,
                    tab_x,
                    0.0,
                    tab_width,
                    ISLAND_HEIGHT,
                    bg_color,
                    0.05,
                    0,
                );
            }

            // Draw vertical left border (separator between tabs)
            // Skip for first tab UNLESS it's active (then draw to separate from traffic lights)
            if tab_index > 0 || (tab_index == 0 && is_active && left_margin > 0.0) {
                sugarloaf.rect(
                    None,
                    tab_x,
                    0.0, // Start from top
                    0.5, // 1px width
                    ISLAND_HEIGHT,
                    self.border_color,
                    0.1, // Same depth as other island elements
                    0,
                );
            }

            // Draw bottom border for inactive tabs (active tabs have no border)
            if !is_active {
                sugarloaf.rect(
                    None,
                    tab_x,
                    ISLAND_HEIGHT - 1.0,
                    tab_width,
                    0.5, // 1px height
                    self.border_color,
                    0.1, // Same depth as other island elements
                    0,
                );
            }

            // Move to next tab position
            x_position += tab_width;
        }

        // Draw the floating (dragged) tab above the slot tabs.
        if let (Some(drag_idx), Some(floating_x)) = (drag_index, floating_left) {
            // Soft elevation: faint dark bands just outside the lifted
            // tab so it reads as floating over the strip.
            const SHADOW_WIDTH: f32 = 3.0;
            for shadow_x in [floating_x - SHADOW_WIDTH, floating_x + tab_width] {
                sugarloaf.rect(
                    None,
                    shadow_x,
                    0.0,
                    SHADOW_WIDTH,
                    ISLAND_HEIGHT,
                    [0.0, 0.0, 0.0, 0.18],
                    0.05,
                    11,
                );
            }

            // Opaque elevated background so the floating tab reads as
            // lifted out of the strip while passing over other slots.
            let mut fill = context_manager.custom_color(drag_idx).unwrap_or(bg_color);
            fill[3] = 1.0;
            sugarloaf.rect(
                None,
                floating_x,
                0.0,
                tab_width,
                ISLAND_HEIGHT,
                fill,
                0.05,
                11,
            );

            // Left/right edges so the tab keeps its outline mid-flight.
            for edge_x in [floating_x, floating_x + tab_width - 0.5] {
                sugarloaf.rect(
                    None,
                    edge_x,
                    0.0,
                    0.5,
                    ISLAND_HEIGHT,
                    self.border_color,
                    0.1,
                    11,
                );
            }

            let raw_title = self.get_title_for_tab(context_manager, drag_idx);
            if !raw_title.is_empty() {
                let max_text_width = (tab_width - TAB_PADDING_X * 2.0).max(0.0);
                let title = fit_title_to_width(sugarloaf, &raw_title, max_text_width);
                let title_opts = DrawOpts {
                    font_size: TITLE_FONT_SIZE,
                    color: color_u8(self.active_text_color),
                    ..DrawOpts::default()
                };
                let ui = sugarloaf.text_mut();
                let text_width = ui.measure(&title, &title_opts);
                let text_x = floating_x + (tab_width - text_width) / 2.0;
                let text_y = (ISLAND_HEIGHT / 2.0) - (TITLE_FONT_SIZE / 2.);
                ui.draw(text_x, text_y, &title, &title_opts);
            }
        }

        // Render color picker if open
        if let Some(picker_tab) = self.color_picker_tab {
            if picker_tab < num_tabs {
                let picker_tab_x = left_margin + picker_tab as f32 * tab_width;
                let selected = context_manager.custom_color(picker_tab);
                self.render_color_picker(sugarloaf, picker_tab_x, tab_width, selected);
            }
        }

        // Render the progress bar below the island
        self.render_progress_bar(sugarloaf, window_width, scale_factor);
    }

    /// Toggle the color picker for a given tab index
    pub fn toggle_color_picker(
        &mut self,
        tab_index: usize,
        current_title: &str,
        context_manager: &mut ContextManager<EventProxy>,
    ) {
        if self.color_picker_tab == Some(tab_index) {
            self.apply_rename(context_manager);
            self.color_picker_tab = None;
        } else {
            self.color_picker_tab = Some(tab_index);
            // Initialize rename input with custom title or current displayed title
            self.rename_input = context_manager
                .custom_title(tab_index)
                .map(str::to_string)
                .unwrap_or_else(|| current_title.to_string());
            self.rename_caret_time = Instant::now();
        }
    }

    /// Close the color picker, applying any pending rename
    pub fn close_color_picker(
        &mut self,
        context_manager: &mut ContextManager<EventProxy>,
    ) {
        if self.color_picker_tab.is_some() {
            self.apply_rename(context_manager);
        }
        self.color_picker_tab = None;
    }

    /// Dismiss the picker WITHOUT committing a pending rename. Used when the
    /// tab set changes underneath it (e.g. a tab close), where the anchored
    /// index may no longer point at the same tab.
    pub fn dismiss_color_picker(&mut self) {
        self.color_picker_tab = None;
    }

    /// Apply the rename input as a custom title for the current picker tab
    fn apply_rename(&mut self, context_manager: &mut ContextManager<EventProxy>) {
        if let Some(tab) = self.color_picker_tab {
            let trimmed = self.rename_input.trim().to_string();
            let title = (!trimmed.is_empty()).then_some(trimmed);
            context_manager.set_custom_title(tab, title);
        }
    }

    /// Handle keyboard input while the color picker (with rename field) is open.
    /// Returns true if input was consumed.
    pub fn handle_rename_input(
        &mut self,
        key_event: &rio_window::event::KeyEvent,
        context_manager: &mut ContextManager<EventProxy>,
    ) -> bool {
        use rio_window::event::ElementState;
        use rio_window::keyboard::{Key, NamedKey};

        if self.color_picker_tab.is_none() {
            return false;
        }

        if key_event.state != ElementState::Pressed {
            return true; // consume release events too
        }

        match &key_event.logical_key {
            Key::Named(NamedKey::Escape) => {
                // Cancel — discard input, close picker
                self.color_picker_tab = None;
            }
            Key::Named(NamedKey::Enter) => {
                // Confirm — apply rename and close
                self.apply_rename(context_manager);
                self.color_picker_tab = None;
            }
            Key::Named(NamedKey::Backspace) => {
                self.rename_input.pop();
                self.rename_caret_time = Instant::now();
            }
            _ => {
                if let Some(text) = key_event.text.as_ref() {
                    let s = text.as_str();
                    if !s.is_empty() && s.chars().all(|c| !c.is_control()) {
                        self.rename_input.push_str(s);
                        self.rename_caret_time = Instant::now();
                    }
                }
            }
        }
        true
    }

    /// Check if a click hits a color swatch in the picker.
    /// Returns true if the click was consumed.
    pub fn handle_color_picker_click(
        &mut self,
        mouse_x: f32,
        mouse_y: f32,
        scale_factor: f32,
        window_width: f32,
        num_tabs: usize,
        context_manager: &mut ContextManager<EventProxy>,
    ) -> bool {
        let picker_tab = match self.color_picker_tab {
            Some(t) => t,
            None => return false,
        };

        let mouse_x_unscaled = mouse_x / scale_factor;
        let mouse_y_unscaled = mouse_y / scale_factor;

        // Compute the same tab layout as render()
        let TabStripLayout {
            left_margin,
            tab_width,
            ..
        } = tab_strip_layout(window_width, scale_factor, num_tabs);
        let tab_x = left_margin + picker_tab as f32 * tab_width;

        // Picker is rendered just below the island
        let picker_y = ISLAND_HEIGHT;

        // Check if click is within picker vertical range
        if mouse_y_unscaled < picker_y || mouse_y_unscaled > picker_y + PICKER_HEIGHT {
            // Click outside picker — apply rename and close
            self.apply_rename(context_manager);
            self.color_picker_tab = None;
            return false;
        }

        // Total picker width — N color swatches + 1 reset swatch
        let slot_count = PICKER_COLORS.len() + 1;
        let total_swatches_width = slot_count as f32 * PICKER_SWATCH_SIZE
            + (slot_count - 1) as f32 * PICKER_SWATCH_GAP;
        let picker_start_x = tab_x + (tab_width - total_swatches_width) / 2.0;

        // Check each swatch
        let swatch_y = picker_y + PICKER_PADDING + PICKER_TOP_PADDING;
        let swatch_y_end = swatch_y + PICKER_SWATCH_SIZE;
        for (i, color) in PICKER_COLORS.iter().enumerate() {
            let swatch_x =
                picker_start_x + i as f32 * (PICKER_SWATCH_SIZE + PICKER_SWATCH_GAP);
            if mouse_x_unscaled >= swatch_x
                && mouse_x_unscaled <= swatch_x + PICKER_SWATCH_SIZE
                && mouse_y_unscaled >= swatch_y
                && mouse_y_unscaled <= swatch_y_end
            {
                context_manager.set_custom_color(picker_tab, Some(*color));
                self.apply_rename(context_manager);
                self.color_picker_tab = None;
                return true;
            }
        }

        // Reset swatch — clears any custom color for this tab
        let reset_x = picker_start_x
            + PICKER_COLORS.len() as f32 * (PICKER_SWATCH_SIZE + PICKER_SWATCH_GAP);
        if mouse_x_unscaled >= reset_x
            && mouse_x_unscaled <= reset_x + PICKER_SWATCH_SIZE
            && mouse_y_unscaled >= swatch_y
            && mouse_y_unscaled <= swatch_y_end
        {
            context_manager.set_custom_color(picker_tab, None);
            self.apply_rename(context_manager);
            self.color_picker_tab = None;
            return true;
        }

        // Clicked in picker area but not on a swatch
        true
    }

    /// Render the color picker dropdown below a tab
    fn render_color_picker(
        &mut self,
        sugarloaf: &mut Sugarloaf,
        tab_x: f32,
        tab_width: f32,
        selected_color: Option<[f32; 4]>,
    ) {
        let padding = PICKER_PADDING;
        let bg_y = ISLAND_HEIGHT;

        // Compute total swatches width to derive the consistent inner content width
        // N color swatches + 1 reset swatch
        let slot_count = PICKER_COLORS.len() + 1;
        let total_swatches_width = slot_count as f32 * PICKER_SWATCH_SIZE
            + (slot_count - 1) as f32 * PICKER_SWATCH_GAP;
        let inner_width = total_swatches_width;
        let bg_width = inner_width + padding * 2.0;
        let bg_x = tab_x + (tab_width - bg_width) / 2.0;
        let content_x = bg_x + padding;

        // Background
        sugarloaf.rounded_rect(
            None,
            bg_x,
            bg_y,
            bg_width,
            PICKER_HEIGHT,
            [0.15, 0.15, 0.15, 1.0],
            0.0,
            4.0,
            10,
        );

        // Swatches — aligned to content_x
        let swatch_y = bg_y + padding + PICKER_TOP_PADDING;
        for (i, color) in PICKER_COLORS.iter().enumerate() {
            let sx = content_x + i as f32 * (PICKER_SWATCH_SIZE + PICKER_SWATCH_GAP);
            let is_selected = selected_color == Some(*color);

            // Draw white border behind selected swatch
            if is_selected {
                let border = 2.0;
                sugarloaf.rounded_rect(
                    None,
                    sx - border,
                    swatch_y - border,
                    PICKER_SWATCH_SIZE + border * 2.0,
                    PICKER_SWATCH_SIZE + border * 2.0,
                    [1.0, 1.0, 1.0, 1.0],
                    0.0,
                    4.0,
                    10,
                );
            }

            sugarloaf.rounded_rect(
                None,
                sx,
                swatch_y,
                PICKER_SWATCH_SIZE,
                PICKER_SWATCH_SIZE,
                *color,
                0.0,
                3.0,
                10,
            );
        }

        // Reset swatch — neutral box with a diagonal slash, selected when no color is set
        let reset_x = content_x
            + PICKER_COLORS.len() as f32 * (PICKER_SWATCH_SIZE + PICKER_SWATCH_GAP);
        let reset_selected = selected_color.is_none();
        if reset_selected {
            let border = 2.0;
            sugarloaf.rounded_rect(
                None,
                reset_x - border,
                swatch_y - border,
                PICKER_SWATCH_SIZE + border * 2.0,
                PICKER_SWATCH_SIZE + border * 2.0,
                [1.0, 1.0, 1.0, 1.0],
                0.0,
                4.0,
                10,
            );
        }
        sugarloaf.rounded_rect(
            None,
            reset_x,
            swatch_y,
            PICKER_SWATCH_SIZE,
            PICKER_SWATCH_SIZE,
            [0.22, 0.22, 0.22, 1.0],
            0.0,
            3.0,
            10,
        );
        let slash_inset = 3.0;
        sugarloaf.line(
            reset_x + slash_inset,
            swatch_y + PICKER_SWATCH_SIZE - slash_inset,
            reset_x + PICKER_SWATCH_SIZE - slash_inset,
            swatch_y + slash_inset,
            1.5,
            0.0,
            [0.86, 0.26, 0.27, 1.0],
            10,
        );

        // Rename text input — same left/right edge as swatches
        let input_y = swatch_y + PICKER_SWATCH_SIZE + PICKER_INPUT_MARGIN_TOP;
        let input_x = content_x;
        let input_width = inner_width;

        // Input background
        sugarloaf.rounded_rect(
            None,
            input_x,
            input_y,
            input_width,
            PICKER_INPUT_HEIGHT,
            [0.10, 0.10, 0.10, 1.0],
            0.0,
            3.0,
            10,
        );

        let text_inset = 6.0;
        let text_x = input_x + text_inset;
        let max_text_width = input_width - text_inset * 2.0;
        let text_y = input_y + (PICKER_INPUT_HEIGHT - PICKER_INPUT_FONT_SIZE) / 2.0;

        let text_color = if self.rename_input.is_empty() {
            [0.45, 0.45, 0.45, 1.0]
        } else {
            [0.93, 0.93, 0.93, 1.0]
        };
        let rename_opts = DrawOpts {
            font_size: PICKER_INPUT_FONT_SIZE,
            color: color_u8(text_color),
            ..DrawOpts::default()
        };

        // Determine visible text: trim from the front if it overflows.
        let display_text: String = if self.rename_input.is_empty() {
            "Tab title...".to_string()
        } else {
            let input = self.rename_input.as_str();
            let chars: Vec<char> = input.chars().collect();
            let ui = sugarloaf.text_mut();
            let mut start = 0;
            let full_width = ui.measure(input, &rename_opts);
            if full_width > max_text_width {
                let mut lo = 0;
                let mut hi = chars.len();
                while lo < hi {
                    let mid = (lo + hi) / 2;
                    let substr: String = chars[mid..].iter().collect();
                    let w = ui.measure(&substr, &rename_opts);
                    if w > max_text_width {
                        lo = mid + 1;
                    } else {
                        hi = mid;
                    }
                }
                start = lo;
            }
            chars[start..].iter().collect()
        };

        let rendered_width =
            sugarloaf
                .text_mut()
                .draw(text_x, text_y, &display_text, &rename_opts);
        let rendered_width = if self.rename_input.is_empty() {
            0.0
        } else {
            rendered_width
        };

        // Blinking caret
        let elapsed = self.rename_caret_time.elapsed().as_millis();
        let show_caret = (elapsed / 500).is_multiple_of(2);
        if show_caret {
            let caret_x = text_x + rendered_width;
            if caret_x <= input_x + input_width {
                sugarloaf.rect(
                    None,
                    caret_x,
                    input_y + 4.0,
                    1.5,
                    PICKER_INPUT_HEIGHT - 8.0,
                    [0.93, 0.93, 0.93, 1.0],
                    0.0,
                    10,
                );
            }
        }
    }

    /// Whether the color picker is currently open
    pub fn is_color_picker_open(&self) -> bool {
        self.color_picker_tab.is_some()
    }

    /// Get the title text for a specific tab index
    fn get_title_for_tab(
        &self,
        context_manager: &ContextManager<EventProxy>,
        tab_index: usize,
    ) -> String {
        // Custom user-set title takes priority
        if let Some(custom) = context_manager.custom_title(tab_index) {
            return custom.to_string();
        }

        if let Some(context_title) = context_manager.title(tab_index) {
            if !context_title.content.is_empty() {
                return context_title.content.clone();
            }

            // Fallback to program name if title is empty
            if let Some(ref extra) = context_title.extra {
                if !extra.program.is_empty() {
                    return extra.program.clone();
                }
            }
        }

        // Default fallback - show tab number
        String::from("~")
    }
}

#[inline]
fn color_u8(c: [f32; 4]) -> [u8; 4] {
    [
        (c[0].clamp(0.0, 1.0) * 255.0) as u8,
        (c[1].clamp(0.0, 1.0) * 255.0) as u8,
        (c[2].clamp(0.0, 1.0) * 255.0) as u8,
        (c[3].clamp(0.0, 1.0) * 255.0) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_island_constants() {
        // Verify all constants are set correctly
        assert_eq!(ISLAND_HEIGHT, 34.0);
        assert_eq!(TITLE_FONT_SIZE, 12.0);
        assert_eq!(TAB_PADDING_X, 24.0);
        assert_eq!(ISLAND_MARGIN_RIGHT, 8.0);
        #[cfg(target_os = "macos")]
        assert_eq!(ISLAND_MARGIN_LEFT_MACOS, 76.0);
    }

    #[test]
    fn test_island_initialization() {
        let inactive_color = [0.5, 0.5, 0.5, 1.0];
        let active_color = [0.9, 0.9, 0.9, 1.0];
        let border_color = [0.7, 0.7, 0.7, 1.0];

        let island = Island::new(inactive_color, active_color, border_color, true);

        assert_eq!(island.inactive_text_color, inactive_color);
        assert_eq!(island.active_text_color, active_color);
        assert_eq!(island.border_color, border_color);
        assert!(island.hide_if_single);
    }

    #[test]
    fn test_island_height() {
        let island = Island::new(
            [0.8, 0.8, 0.8, 1.0],
            [1.0, 1.0, 1.0, 1.0],
            [0.8, 0.8, 0.8, 1.0],
            false,
        );
        assert_eq!(island.height(), ISLAND_HEIGHT);
    }

    fn test_island() -> Island {
        Island::new(
            [0.5, 0.5, 0.5, 1.0],
            [0.9, 0.9, 0.9, 1.0],
            [0.7, 0.7, 0.7, 1.0],
            false,
        )
    }

    #[test]
    fn progress_first_report_seeds_started_and_seen() {
        let mut island = test_island();
        island.set_progress_report(ProgressReport {
            state: ProgressState::Indeterminate,
            progress: None,
        });
        assert!(island.progress_started_at.is_some());
        assert!(island.progress_last_seen.is_some());
        assert_eq!(island.progress_state, Some(ProgressState::Indeterminate));
    }

    #[test]
    fn progress_repeated_same_state_keeps_started_at_stable() {
        // Issue #1509: a TUI that heartbeats `OSC 9;4;3` (or any same-state
        // report) must NOT restart the indeterminate animation phase, or the
        // pulsing block snaps back to the left edge on every report.
        let mut island = test_island();
        island.set_progress_report(ProgressReport {
            state: ProgressState::Indeterminate,
            progress: None,
        });
        let first_started = island.progress_started_at.unwrap();
        let first_seen = island.progress_last_seen.unwrap();

        // Sleep so a subsequent Instant::now() is observably later — the
        // started_at field must stay equal while last_seen advances.
        std::thread::sleep(std::time::Duration::from_millis(15));
        island.set_progress_report(ProgressReport {
            state: ProgressState::Indeterminate,
            progress: None,
        });

        assert_eq!(
            island.progress_started_at,
            Some(first_started),
            "started_at must not move on a same-state heartbeat"
        );
        assert!(
            island.progress_last_seen.unwrap() > first_seen,
            "last_seen must advance on every report"
        );
    }

    #[test]
    fn progress_state_transition_resets_started_at() {
        // Set → Indeterminate is a real state change, so the animation
        // anchor should be reseated. (Set has no animation, but the
        // started_at field still becomes meaningful as soon as we hit
        // Indeterminate.)
        let mut island = test_island();
        island.set_progress_report(ProgressReport {
            state: ProgressState::Set,
            progress: Some(50),
        });
        let first = island.progress_started_at.unwrap();

        std::thread::sleep(std::time::Duration::from_millis(15));
        island.set_progress_report(ProgressReport {
            state: ProgressState::Indeterminate,
            progress: None,
        });

        assert!(
            island.progress_started_at.unwrap() > first,
            "transitioning into a new state must move started_at forward"
        );
        assert_eq!(island.progress_state, Some(ProgressState::Indeterminate));
    }

    #[test]
    fn progress_set_value_change_does_not_reseat_started_at() {
        // Same `Set` state with a different percentage is still the same
        // state — only the value updates. started_at stays put; the bar
        // just redraws at the new fraction.
        let mut island = test_island();
        island.set_progress_report(ProgressReport {
            state: ProgressState::Set,
            progress: Some(20),
        });
        let first = island.progress_started_at.unwrap();

        std::thread::sleep(std::time::Duration::from_millis(15));
        island.set_progress_report(ProgressReport {
            state: ProgressState::Set,
            progress: Some(60),
        });

        assert_eq!(island.progress_started_at, Some(first));
        assert_eq!(island.progress_value, Some(60));
    }

    /// Each char = 1.0 wide, including the ellipsis. Easy arithmetic.
    fn fixed_unit_width(_c: char) -> f32 {
        1.0
    }

    fn rendered_width(s: &str, char_width: impl FnMut(char) -> f32) -> f32 {
        s.chars().map(char_width).sum()
    }

    #[test]
    fn title_fits_is_returned_unchanged() {
        assert_eq!(
            fit_title_with_widths("hello", 10.0, fixed_unit_width),
            "hello"
        );
        assert_eq!(fit_title_with_widths("hi", 2.0, fixed_unit_width), "hi");
    }

    #[test]
    fn title_that_fits_borrows_without_allocating() {
        // Confirms the zero-allocation "no truncation" hot path: when the
        // full title fits, the returned Cow must stay Borrowed so the
        // render loop doesn't allocate a new String every frame.
        let out = fit_title_with_widths("ok", 10.0, fixed_unit_width);
        assert!(
            matches!(out, Cow::Borrowed(_)),
            "expected borrowed, got {out:?}"
        );
    }

    #[test]
    fn title_zero_budget_returns_ellipsis() {
        // Historically this was short-circuited to return the full title;
        // now it falls through the loop and returns "…" consistently with
        // tiny-but-positive budgets.
        assert_eq!(fit_title_with_widths("abc", 0.0, fixed_unit_width), "…");
    }

    #[test]
    fn title_overflow_gets_ellipsized_and_fits_budget() {
        // "hello world" budgeted at 5 → best we can do without exceeding
        // is "hell" (4) + "…" (1) = 5. Anything more overflows.
        let out = fit_title_with_widths("hello world", 5.0, fixed_unit_width);
        assert_eq!(out, "hell…");
        assert!(
            rendered_width(&out, fixed_unit_width) <= 5.0,
            "truncated width {} must be ≤ budget 5",
            rendered_width(&out, fixed_unit_width)
        );
    }

    #[test]
    fn title_respects_budget_with_wide_chars() {
        // Mixed widths: 'W' = 2.0, others (including ellipsis) = 1.0.
        // Title "WxWxW", budget 4.0. Walk:
        // ix=0 W: before add, 0+1(suffix) ≤ 4 → truncate_ix=0; accum→2
        // ix=1 x: 2+1 ≤ 4 → truncate_ix=1; accum→3
        // ix=2 W: 3+1 ≤ 4 → truncate_ix=2; accum→5; 5>4 → cut.
        // Output: title[..2] + "…" = "Wx…", width 2+1+1 = 4 ≤ 4 ✓
        let widths = |c: char| if c == 'W' { 2.0 } else { 1.0 };
        let out = fit_title_with_widths("WxWxW", 4.0, widths);
        assert_eq!(out, "Wx…");
        assert!(rendered_width(&out, widths) <= 4.0);
    }

    #[test]
    fn title_truncation_preserves_utf8_boundaries() {
        // Each emoji/char = 2.0 wide; ellipsis = 2.0.
        // Title "🎟🎟🎟" = 6.0. Budget 4.0 → one emoji + "…" = 4.0 ≤ 4 ✓.
        // Crucial: the byte index we cut at must be on a UTF-8 boundary.
        let w = |_c: char| 2.0;
        let out = fit_title_with_widths("🎟🎟🎟", 4.0, w);
        assert_eq!(out, "🎟…");
        assert!(out.chars().count() == 2, "{out:?} should be 2 graphemes");
    }

    #[test]
    fn title_budget_smaller_than_ellipsis_still_returns_ellipsis() {
        // Budget 0.5 < ellipsis_width 1.0: first char overflows, prefix is
        // empty, we return just "…" so the user at least sees *something*
        // indicating truncation rather than a blank tab label.
        let out = fit_title_with_widths("abc", 0.5, fixed_unit_width);
        assert_eq!(out, "…");
    }

    #[test]
    fn title_empty_input_returned_as_is() {
        assert_eq!(fit_title_with_widths("", 10.0, fixed_unit_width), "");
    }

    #[test]
    fn title_exact_fit_not_truncated() {
        // Title "abcd" = 4.0, budget 4.0 → fits exactly, no truncation.
        assert_eq!(fit_title_with_widths("abcd", 4.0, fixed_unit_width), "abcd");
    }

    #[test]
    fn tab_strip_layout_geometry() {
        // 1000 physical px @ 2x scale → 500 logical px window.
        let layout = tab_strip_layout(1000.0, 2.0, 4);
        #[cfg(target_os = "macos")]
        {
            assert_eq!(layout.left_margin, ISLAND_MARGIN_LEFT_MACOS);
            assert_eq!(layout.available_width, 500.0 - 8.0 - 76.0);
            assert_eq!(layout.tab_width, (500.0 - 8.0 - 76.0) / 4.0);
        }
        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(layout.left_margin, 0.0);
            assert_eq!(layout.available_width, 492.0);
            assert_eq!(layout.tab_width, 123.0);
        }
        // Zero tabs clamps the divisor.
        assert!(tab_strip_layout(1000.0, 2.0, 0).tab_width.is_finite());
    }

    #[test]
    fn remap_tab_move_forward_rotates_indices() {
        // Move tab 1 → 3: tabs 2 and 3 shift left by one.
        assert_eq!(Island::remap_index(1, 1, 3), 3);
        assert_eq!(Island::remap_index(2, 1, 3), 1);
        assert_eq!(Island::remap_index(3, 1, 3), 2);
        assert_eq!(Island::remap_index(0, 1, 3), 0);
        assert_eq!(Island::remap_index(4, 1, 3), 4);
    }

    #[test]
    fn remap_tab_move_backward_rotates_indices() {
        // Move tab 3 → 0: tabs 0, 1, 2 shift right by one.
        assert_eq!(Island::remap_index(3, 3, 0), 0);
        assert_eq!(Island::remap_index(0, 3, 0), 1);
        assert_eq!(Island::remap_index(1, 3, 0), 2);
        assert_eq!(Island::remap_index(2, 3, 0), 3);
        assert_eq!(Island::remap_index(4, 3, 0), 4);
    }

    #[test]
    fn remap_tab_move_carries_picker_and_springs() {
        let mut island = test_island();
        island.color_picker_tab = Some(3);

        // Tab 1 → 3 (rotate): the open picker shifts 3 → 2. Per-tab colors
        // and titles now live on the tab in ContextManager (see
        // context::test::test_custom_color_* / test_custom_title_*), so they
        // no longer need remapping here.
        island.remap_tab_move(1, 3, 100.0);
        assert_eq!(island.color_picker_tab, Some(2));

        // Displaced tabs (now at 1 and 2) got slide springs of +width.
        assert_eq!(island.slide_springs.len(), 2);
        assert_eq!(island.slide_springs.get(&1).unwrap().position, 100.0);
        assert_eq!(island.slide_springs.get(&2).unwrap().position, 100.0);
    }

    #[test]
    fn drag_threshold_gates_start() {
        let mut island = test_island();
        island.start_drag(0, 10.0, 50.0);
        assert!(island.is_dragging());
        assert_eq!(island.drag_index(), None, "not started below threshold");
        assert!(!island.update_drag(52.0));
        assert!(island.update_drag(58.0), "8px exceeds threshold");
        assert_eq!(island.drag_index(), Some(0));
        island.cancel_drag();
        assert!(!island.is_dragging());
    }

    #[test]
    fn drag_center_clamps_to_strip() {
        let mut island = test_island();
        // Tab 0 grabbed 10px from its left edge, strip spans 0..400 with
        // 100-wide tabs.
        island.start_drag(0, 10.0, 50.0);
        island.update_drag(200.0); // started
        let center = island.drag_center(0.0, 400.0, 100.0).unwrap();
        assert_eq!(center, 190.0 + 50.0);

        // Dragged far right: floating left clamps to 300, center 350.
        island.update_drag(1000.0);
        assert_eq!(island.drag_center(0.0, 400.0, 100.0), Some(350.0));

        // Far left: clamps to 0, center 50.
        island.update_drag(-500.0);
        assert_eq!(island.drag_center(0.0, 400.0, 100.0), Some(50.0));
    }

    #[test]
    fn end_drag_seeds_settle_spring() {
        let mut island = test_island();
        island.start_drag(2, 0.0, 200.0);
        island.update_drag(250.0); // floating left = 250, slot x = 200
        island.end_drag(0.0, 400.0, 100.0);
        assert!(!island.is_dragging());
        let spring = island.slide_springs.get(&2).unwrap();
        assert_eq!(spring.position, 50.0);
    }

    #[test]
    fn progress_remove_clears_all_progress_state() {
        let mut island = test_island();
        island.set_progress_report(ProgressReport {
            state: ProgressState::Set,
            progress: Some(50),
        });
        island.set_progress_report(ProgressReport {
            state: ProgressState::Remove,
            progress: None,
        });
        assert!(island.progress_state.is_none());
        assert!(island.progress_value.is_none());
        assert!(island.progress_started_at.is_none());
        assert!(island.progress_last_seen.is_none());
    }
}
