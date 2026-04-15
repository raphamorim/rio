// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// island.rs was originally retired from boo editor
// which is licensed under MIT license.

use crate::context::{next_rich_text_id, ContextManager};
use crate::renderer::utils::add_span_with_fallback;
use rio_backend::event::{EventProxy, ProgressReport, ProgressState};
use rio_backend::sugarloaf::{Attributes, SpanStyle, Sugarloaf};
use rustc_hash::FxHashMap;
use std::borrow::Cow;
use std::time::Instant;

/// Height of the tab bar in pixels
pub const ISLAND_HEIGHT: f32 = 34.0;

/// Height of the progress bar in pixels
const PROGRESS_BAR_HEIGHT: f32 = 3.0;

/// Timeout in seconds for auto-dismissing stale progress bars
const PROGRESS_BAR_TIMEOUT_SECS: u64 = 15;

const TITLE_FONT_SIZE: f32 = 12.0;

/// Left/right padding inside each tab — kept as breathing room around the
/// title text so it never butts against the tab separator lines.
const TAB_PADDING_X: f32 = 24.0;

/// Suffix used when truncating a title that doesn't fit in its tab.
const TITLE_ELLIPSIS: char = '…';

/// Truncate `title` to fit within `max_width` pixels at the tab font,
/// appending `…` when characters have to be dropped. Thin adapter that
/// asks sugarloaf's cached glyph advance for each char. Returns
/// `Cow::Borrowed(title)` when the full string fits so the common
/// "no truncation needed" path avoids allocating.
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

/// Pure-logic truncation: walks `title` left to right, summing per-char
/// widths from the supplied closure, appending `…` the first moment the
/// running total would exceed `max_width`. Separated from sugarloaf so
/// tests can feed synthetic widths without a GPU context.
///
/// Returns `Cow::Borrowed(title)` when the full string fits, so the
/// hot "no truncation needed" path does zero allocation.
///
/// `max_width <= 0.0` falls through the loop naturally: the first
/// char's accumulated width already exceeds the budget, `truncate_ix`
/// stays 0, and we return just `"…"` — a consistent sentinel that
/// at least signals "there was content here". Empty input returns
/// `Cow::Borrowed("")`.
///
/// Approximate (isolated per-char advances — no kerning, no ligatures,
/// no emoji cluster formation). Fine for short labels where a pixel or
/// two of slack is invisible.
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
    [0.86, 0.26, 0.27, 1.0], // red
    [0.90, 0.57, 0.22, 1.0], // orange
    [0.85, 0.78, 0.25, 1.0], // yellow
    [0.34, 0.70, 0.38, 1.0], // green
    [0.30, 0.55, 0.85, 1.0], // blue
    [0.68, 0.40, 0.80, 1.0], // purple
];

/// Right margin after last tab
const ISLAND_MARGIN_RIGHT: f32 = 8.0;

/// Left margin on macOS to account for traffic light buttons
#[cfg(target_os = "macos")]
const ISLAND_MARGIN_LEFT_MACOS: f32 = 76.0;

struct TabIslandData {
    text_id: usize,
    last_title: String,
}

pub struct Island {
    pub hide_if_single: bool,
    pub inactive_text_color: [f32; 4],
    pub active_text_color: [f32; 4],
    pub border_color: [f32; 4],
    tab_data: FxHashMap<usize, TabIslandData>,
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
    /// Per-tab background colors
    tab_colors: FxHashMap<usize, [f32; 4]>,
    /// Per-tab custom titles (user overrides)
    tab_custom_titles: FxHashMap<usize, String>,
    /// Current rename input text while picker is open
    rename_input: String,
    /// Rich text ID for the rename input
    rename_text_id: Option<usize>,
    /// Caret blink timer
    rename_caret_time: Instant,
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
            tab_data: FxHashMap::default(),
            progress_state: None,
            progress_value: None,
            progress_started_at: None,
            progress_last_seen: None,
            // Default progress bar color (blue-ish)
            progress_bar_color: [0.3, 0.6, 1.0, 1.0],
            // Default error color (red-ish)
            progress_bar_error_color: [1.0, 0.3, 0.3, 1.0],
            color_picker_tab: None,
            tab_colors: FxHashMap::default(),
            tab_custom_titles: FxHashMap::default(),
            rename_input: String::new(),
            rename_text_id: None,
            rename_caret_time: Instant::now(),
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
        // Clear cached titles to force re-render with new colors
        for tab_data in self.tab_data.values_mut() {
            tab_data.last_title.clear();
        }
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

    /// Check if the progress bar needs continuous rendering (for animations)
    pub fn needs_redraw(&self) -> bool {
        matches!(self.progress_state, Some(ProgressState::Indeterminate))
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
    ) {
        let (window_width, _window_height, scale_factor) = dimensions;
        let num_tabs = context_manager.len();
        let current_tab_index = context_manager.current_index();

        // Always hide rename text first — render_color_picker will re-show if needed
        if let Some(id) = self.rename_text_id {
            sugarloaf.set_visibility(id, false);
        }

        // Hide tabs if only single tab and hide_if_single is enabled
        if self.hide_if_single && num_tabs == 1 {
            // Hide all existing tab rich texts
            for tab_data in self.tab_data.values() {
                sugarloaf.set_visibility(tab_data.text_id, false);
            }
            // Still render the progress bar even when tabs are hidden
            self.render_progress_bar(sugarloaf, window_width, scale_factor);
            return;
        }

        // Hide all existing tab rich texts first
        for tab_data in self.tab_data.values() {
            sugarloaf.set_visibility(tab_data.text_id, false);
        }

        // Calculate left margin (macOS needs space for traffic light buttons)
        #[cfg(target_os = "macos")]
        let left_margin = ISLAND_MARGIN_LEFT_MACOS;
        #[cfg(not(target_os = "macos"))]
        let left_margin = 0.0;

        // Calculate equal width for all tabs
        let available_width =
            (window_width / scale_factor) - ISLAND_MARGIN_RIGHT - left_margin;
        let tab_width = available_width / num_tabs as f32;

        // Starting from left edge (with margin on macOS for traffic lights)
        let mut x_position = left_margin;

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
            let is_active = tab_index == current_tab_index;

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

            // Get or create tab data
            let tab_data = self.tab_data.entry(tab_index).or_insert_with(|| {
                // Text should be in front of everything (terminal at 0.0)
                let text_id = next_rich_text_id();
                let _ = sugarloaf.text(Some(text_id));
                sugarloaf.set_use_grid_cell_size(text_id, false); // Proportional text for tabs
                sugarloaf.set_text_font_size(&text_id, TITLE_FONT_SIZE);

                TabIslandData {
                    text_id,
                    last_title: String::new(),
                }
            });

            // Choose text color based on active state
            let text_color = if is_active {
                self.active_text_color
            } else {
                self.inactive_text_color
            };

            // Update text with per-font spans for font fallback
            let base_style = SpanStyle {
                color: text_color,
                ..SpanStyle::default()
            };

            sugarloaf.content().sel(tab_data.text_id).clear().new_line();
            add_span_with_fallback(sugarloaf, &title, base_style);
            sugarloaf.content().build();
            tab_data.last_title = title.into_owned();

            // Position text to measure, then re-center using actual rendered width
            sugarloaf.set_position(tab_data.text_id, x_position, 0.0);
            sugarloaf.set_visibility(tab_data.text_id, true);
            let text_width = sugarloaf.get_text_rendered_width(&tab_data.text_id);

            // Position text centered horizontally and vertically in the tab
            let text_x = x_position + (tab_width - text_width) / 2.0;
            let text_y = (ISLAND_HEIGHT / 2.0) - (TITLE_FONT_SIZE / 2.);
            sugarloaf.set_position(tab_data.text_id, text_x, text_y);
            sugarloaf.set_visibility(tab_data.text_id, true);

            // Draw tab background color if set
            if let Some(bg_color) = self.tab_colors.get(&tab_index) {
                sugarloaf.rect(
                    None,
                    x_position,
                    0.0,
                    tab_width,
                    ISLAND_HEIGHT,
                    *bg_color,
                    0.05,
                    0,
                );
            }

            // Draw vertical left border (separator between tabs)
            // Skip for first tab UNLESS it's active (then draw to separate from traffic lights)
            if tab_index > 0 || (tab_index == 0 && is_active && left_margin > 0.0) {
                sugarloaf.rect(
                    None,
                    x_position,
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
                    x_position,
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

        // Render color picker if open
        if let Some(picker_tab) = self.color_picker_tab {
            if picker_tab < num_tabs {
                let picker_tab_x = left_margin + picker_tab as f32 * tab_width;
                self.render_color_picker(sugarloaf, picker_tab_x, tab_width);
            }
        }

        // Render the progress bar below the island
        self.render_progress_bar(sugarloaf, window_width, scale_factor);
    }

    /// Toggle the color picker for a given tab index
    pub fn toggle_color_picker(&mut self, tab_index: usize, current_title: &str) {
        if self.color_picker_tab == Some(tab_index) {
            self.apply_rename();
            self.color_picker_tab = None;
        } else {
            self.color_picker_tab = Some(tab_index);
            // Initialize rename input with custom title or current displayed title
            self.rename_input = self
                .tab_custom_titles
                .get(&tab_index)
                .cloned()
                .unwrap_or_else(|| current_title.to_string());
            self.rename_caret_time = Instant::now();
        }
    }

    /// Close the color picker, applying any pending rename
    pub fn close_color_picker(&mut self) {
        if self.color_picker_tab.is_some() {
            self.apply_rename();
        }
        self.color_picker_tab = None;
    }

    /// Apply the rename input as a custom title for the current picker tab
    fn apply_rename(&mut self) {
        if let Some(tab) = self.color_picker_tab {
            let trimmed = self.rename_input.trim().to_string();
            if trimmed.is_empty() {
                self.tab_custom_titles.remove(&tab);
            } else {
                self.tab_custom_titles.insert(tab, trimmed);
            }
        }
    }

    /// Handle keyboard input while the color picker (with rename field) is open.
    /// Returns true if input was consumed.
    pub fn handle_rename_input(
        &mut self,
        key_event: &rio_window::event::KeyEvent,
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
                self.apply_rename();
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

    /// Check if the picker needs continuous redraw (caret blink)
    pub fn needs_rename_redraw(&self) -> bool {
        self.color_picker_tab.is_some()
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
    ) -> bool {
        let picker_tab = match self.color_picker_tab {
            Some(t) => t,
            None => return false,
        };

        let mouse_x_unscaled = mouse_x / scale_factor;
        let mouse_y_unscaled = mouse_y / scale_factor;

        // Compute the same tab layout as render()
        #[cfg(target_os = "macos")]
        let left_margin = ISLAND_MARGIN_LEFT_MACOS;
        #[cfg(not(target_os = "macos"))]
        let left_margin = 0.0;

        let available_width =
            (window_width / scale_factor) - ISLAND_MARGIN_RIGHT - left_margin;
        let tab_width = available_width / num_tabs as f32;
        let tab_x = left_margin + picker_tab as f32 * tab_width;

        // Picker is rendered just below the island
        let picker_y = ISLAND_HEIGHT;

        // Check if click is within picker vertical range
        if mouse_y_unscaled < picker_y || mouse_y_unscaled > picker_y + PICKER_HEIGHT {
            // Click outside picker — apply rename and close
            self.apply_rename();
            self.color_picker_tab = None;
            return false;
        }

        // Total picker width
        let total_swatches_width = PICKER_COLORS.len() as f32 * PICKER_SWATCH_SIZE
            + (PICKER_COLORS.len() - 1) as f32 * PICKER_SWATCH_GAP;
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
                self.tab_colors.insert(picker_tab, *color);
                self.apply_rename();
                self.color_picker_tab = None;
                return true;
            }
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
    ) {
        let padding = PICKER_PADDING;
        let bg_y = ISLAND_HEIGHT;

        // Compute total swatches width to derive the consistent inner content width
        let total_swatches_width = PICKER_COLORS.len() as f32 * PICKER_SWATCH_SIZE
            + (PICKER_COLORS.len() - 1) as f32 * PICKER_SWATCH_GAP;
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
        let picker_tab = self.color_picker_tab.unwrap_or(0);
        let selected_color = self.tab_colors.get(&picker_tab);
        for (i, color) in PICKER_COLORS.iter().enumerate() {
            let sx = content_x + i as f32 * (PICKER_SWATCH_SIZE + PICKER_SWATCH_GAP);
            let is_selected = selected_color == Some(color);

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

        // Ensure rename text ID exists
        if self.rename_text_id.is_none() {
            let id = next_rich_text_id();
            let _ = sugarloaf.text(Some(id));
            sugarloaf.set_use_grid_cell_size(id, false);
            sugarloaf.set_text_font_size(&id, PICKER_INPUT_FONT_SIZE);
            sugarloaf.set_order(id, 10);
            self.rename_text_id = Some(id);
        }

        let text_id = self.rename_text_id.unwrap();

        let text_inset = 6.0;
        let text_x = input_x + text_inset;
        let max_text_width = input_width - text_inset * 2.0;
        let text_y = input_y + (PICKER_INPUT_HEIGHT - PICKER_INPUT_FONT_SIZE) / 2.0;

        let text_color = if self.rename_input.is_empty() {
            [0.45, 0.45, 0.45, 1.0]
        } else {
            [0.93, 0.93, 0.93, 1.0]
        };

        // Determine visible text: trim from the front if it overflows
        let display_text: String = if self.rename_input.is_empty() {
            "Tab title...".to_string()
        } else {
            // Try full string first, trim chars from front until it fits
            let input = &self.rename_input;
            let chars: Vec<char> = input.chars().collect();
            let mut start = 0;

            // Measure full text
            let set_and_measure = |text: &str, sugarloaf: &mut Sugarloaf| {
                let content = sugarloaf.content();
                content
                    .sel(text_id)
                    .clear()
                    .new_line()
                    .add_span(
                        text,
                        SpanStyle {
                            color: text_color,
                            ..SpanStyle::default()
                        },
                    )
                    .build();
                sugarloaf.set_position(text_id, text_x, text_y);
                sugarloaf.get_text_rendered_width(&text_id)
            };

            let full_width = set_and_measure(input, sugarloaf);
            if full_width > max_text_width {
                // Binary search for the right start index
                let mut lo = 0;
                let mut hi = chars.len();
                while lo < hi {
                    let mid = (lo + hi) / 2;
                    let substr: String = chars[mid..].iter().collect();
                    let w = set_and_measure(&substr, sugarloaf);
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

        let content = sugarloaf.content();
        content
            .sel(text_id)
            .clear()
            .new_line()
            .add_span(
                &display_text,
                SpanStyle {
                    color: text_color,
                    ..SpanStyle::default()
                },
            )
            .build();

        sugarloaf.set_position(text_id, text_x, text_y);
        sugarloaf.set_visibility(text_id, true);

        let rendered_width = if self.rename_input.is_empty() {
            0.0
        } else {
            sugarloaf.get_text_rendered_width(&text_id)
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
        if let Some(custom) = self.tab_custom_titles.get(&tab_index) {
            return custom.clone();
        }

        if let Some(context_title) = context_manager.titles.titles.get(&tab_index) {
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
        assert_eq!(fit_title_with_widths("hello", 10.0, fixed_unit_width), "hello");
        assert_eq!(fit_title_with_widths("hi", 2.0, fixed_unit_width), "hi");
    }

    #[test]
    fn title_that_fits_borrows_without_allocating() {
        // Confirms the zero-allocation "no truncation" hot path: when the
        // full title fits, the returned Cow must stay Borrowed so the
        // render loop doesn't allocate a new String every frame.
        let out = fit_title_with_widths("ok", 10.0, fixed_unit_width);
        assert!(matches!(out, Cow::Borrowed(_)), "expected borrowed, got {out:?}");
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
        //   ix=0 W: before add, 0+1(suffix) ≤ 4 → truncate_ix=0; accum→2
        //   ix=1 x: 2+1 ≤ 4 → truncate_ix=1; accum→3
        //   ix=2 W: 3+1 ≤ 4 → truncate_ix=2; accum→5; 5>4 → cut.
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
