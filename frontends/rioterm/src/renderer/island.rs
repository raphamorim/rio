// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// island.rs was originally retired from boo editor
// which is licensed under MIT license.

use crate::context::ContextManager;
use rio_backend::event::{EventProxy, ProgressReport, ProgressState};
use rio_backend::sugarloaf::Sugarloaf;
use std::collections::HashMap;
use std::time::Instant;

/// Height of the tab bar in pixels
pub const ISLAND_HEIGHT: f32 = 34.0;

/// Height of the progress bar in pixels
const PROGRESS_BAR_HEIGHT: f32 = 3.0;

/// Timeout in seconds for auto-dismissing stale progress bars
const PROGRESS_BAR_TIMEOUT_SECS: u64 = 15;

const TITLE_FONT_SIZE: f32 = 12.0;

/// Left/right padding inside tab text
const TAB_PADDING_X: f32 = 24.0;

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
    tab_data: HashMap<usize, TabIslandData>,
    /// Current progress bar state
    progress_state: Option<ProgressState>,
    /// Current progress value (0-100)
    progress_value: Option<u8>,
    /// Time of the last progress update (for timeout)
    progress_last_update: Option<Instant>,
    /// Progress bar color
    pub progress_bar_color: [f32; 4],
    /// Progress bar error color
    pub progress_bar_error_color: [f32; 4],
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
            tab_data: HashMap::new(),
            progress_state: None,
            progress_value: None,
            progress_last_update: None,
            // Default progress bar color (blue-ish)
            progress_bar_color: [0.3, 0.6, 1.0, 1.0],
            // Default error color (red-ish)
            progress_bar_error_color: [1.0, 0.3, 0.3, 1.0],
        }
    }

    /// Update the progress bar state from an OSC 9;4 report
    pub fn set_progress_report(&mut self, report: ProgressReport) {
        match report.state {
            ProgressState::Remove => {
                // Clear progress bar
                self.progress_state = None;
                self.progress_value = None;
                self.progress_last_update = None;
            }
            _ => {
                self.progress_state = Some(report.state);
                self.progress_value = report.progress;
                self.progress_last_update = Some(Instant::now());
            }
        }
    }

    /// Check if the progress bar needs continuous rendering (for animations)
    pub fn needs_redraw(&self) -> bool {
        matches!(self.progress_state, Some(ProgressState::Indeterminate))
    }

    /// Check if the progress bar should be auto-dismissed due to timeout
    fn check_progress_timeout(&mut self) {
        if let Some(last_update) = self.progress_last_update {
            if last_update.elapsed().as_secs() >= PROGRESS_BAR_TIMEOUT_SECS {
                // Auto-dismiss stale progress bar
                self.progress_state = None;
                self.progress_value = None;
                self.progress_last_update = None;
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
                return;
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
                    );
                }
            }
            ProgressState::Indeterminate => {
                // For indeterminate, show a pulsing/moving indicator
                // Simple implementation: show a 20% wide bar that moves based on time
                let elapsed = self
                    .progress_last_update
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
            );
        }

        // Render each tab
        for tab_index in 0..num_tabs {
            let is_active = tab_index == current_tab_index;

            // Get title for this tab
            let title = self.get_title_for_tab(context_manager, tab_index);
            if title.is_empty() {
                x_position += tab_width;
                continue;
            }

            // Get or create tab data
            let tab_data = self.tab_data.entry(tab_index).or_insert_with(|| {
                // Text should be in front of everything (terminal at 0.0)
                let text_id = sugarloaf.get_next_id();
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

            // Update text (always update to handle active state changes)
            use rio_backend::sugarloaf::SpanStyle;
            let content = sugarloaf.content();
            content
                .sel(tab_data.text_id)
                .clear()
                .new_line()
                .add_text(
                    &title,
                    SpanStyle {
                        color: text_color,
                        ..SpanStyle::default()
                    },
                )
                .build();
            tab_data.last_title = title.clone();

            // Get text dimensions to center it
            let text_dims = sugarloaf
                .get_text_dimensions(&tab_data.text_id)
                .unwrap_or_default();

            // Position text centered horizontally and vertically in the tab
            let text_x = x_position + (tab_width - text_dims.width) / 2.0;
            let text_y = (ISLAND_HEIGHT / 2.0) - (TITLE_FONT_SIZE / 2.);
            sugarloaf.set_position(tab_data.text_id, text_x, text_y);
            sugarloaf.set_visibility(tab_data.text_id, true);

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
                );
            }

            // Draw bottom border for inactive tabs (active tabs have no border)
            if !is_active {
                sugarloaf.rect(
                    None,
                    x_position,
                    ISLAND_HEIGHT - 0.5,
                    tab_width,
                    0.5, // 1px height
                    self.border_color,
                    0.1, // Same depth as other island elements
                );
            }

            // Move to next tab position
            x_position += tab_width;
        }

        // Render the progress bar below the island
        self.render_progress_bar(sugarloaf, window_width, scale_factor);
    }

    /// Get the title text for a specific tab index
    fn get_title_for_tab(
        &self,
        context_manager: &ContextManager<EventProxy>,
        tab_index: usize,
    ) -> String {
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
        format!("Tab {}", tab_index + 1)
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
}
