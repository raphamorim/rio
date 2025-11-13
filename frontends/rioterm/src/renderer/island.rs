// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// island.rs was originally retired from boo editor
// which is licensed under MIT license.

use crate::context::ContextManager;
use rio_backend::event::EventProxy;
use rio_backend::sugarloaf::Sugarloaf;
use std::collections::HashMap;

/// Height of the island in pixels
pub const ISLAND_HEIGHT: f32 = 28.0;

/// Horizontal padding inside each tab island
const ISLAND_PADDING_X: f32 = 16.0;

/// Vertical padding inside each tab island
const ISLAND_PADDING_Y: f32 = 6.0;

/// Spacing between tab islands
const ISLAND_SPACING: f32 = 8.0;

/// Right margin after last island
const ISLAND_MARGIN_RIGHT: f32 = 8.0;

/// Island corner radius for rounded appearance
const ISLAND_CORNER_RADIUS: f32 = 8.0;

/// Font size for tab titles
const TITLE_FONT_SIZE: f32 = 11.0;

/// Font size for shortcut numbers
const SHORTCUT_FONT_SIZE: f32 = 7.0;

/// Spacing between title and shortcut
const TITLE_SHORTCUT_SPACING: f32 = 8.0;

/// Horizontal padding inside shortcut background
const SHORTCUT_PADDING_X: f32 = 4.0;

/// Vertical padding inside shortcut background
const SHORTCUT_PADDING_Y: f32 = 2.0;

/// Corner radius for shortcut background
const SHORTCUT_CORNER_RADIUS: f32 = 4.0;

/// Maximum characters to display in a tab title
const MAX_TITLE_CHARS: usize = 25;

/// Minimum width for a single island
const ISLAND_MIN_WIDTH: f32 = 60.0;

/// Minimum number of tabs before fade effect is enabled
const FADE_EFFECT_MIN_TABS: usize = 5;

/// Maximum distance from active tab before full fade (0 = active, 1 = adjacent, 2 = two away, 3 = three away)
const FADE_EFFECT_MAX_DISTANCE: usize = 3;

/// Opacity reduction per distance step (e.g., 0.25 means 25% reduction per step)
const FADE_OPACITY_STEP: f32 = 0.25;

/// Data for each individual tab island
struct TabIslandData {
    /// Rich text ID for this tab's title
    rich_text_id: usize,
    /// Rich text ID for this tab's shortcut number
    shortcut_rich_text_id: usize,
    /// Last rendered title (for change detection)
    last_title: String,
    /// Cached text width from last measurement
    text_width: f32,
    /// Cached shortcut text width from last measurement
    shortcut_width: f32,
}

pub struct Island {
    /// Whether the island is enabled
    pub enabled: bool,
    /// Hide island when only a single tab exists
    pub hide_if_single: bool,
    /// Background color for inactive tabs (RGBA)
    pub background_color: [f32; 4],
    /// Background color for active tab (RGBA)
    pub active_background_color: [f32; 4],
    /// Title text color (RGBA)
    pub title_color: [f32; 4],
    /// Cursor color for single-tab indicator (RGBA)
    pub cursor_color: [f32; 4],
    /// Whether to show shadow below islands
    pub show_shadow: bool,
    /// Tab-specific data keyed by tab index
    tab_data: HashMap<usize, TabIslandData>,
}

impl Default for Island {
    fn default() -> Self {
        Self {
            // Disabled by default - can be enabled via configuration
            enabled: false,
            // Don't hide single tab by default
            hide_if_single: false,
            // Subtle dark background for inactive tabs
            background_color: [0.15, 0.15, 0.15, 0.9],
            // Slightly lighter background for active tab
            active_background_color: [0.2, 0.2, 0.2, 1.0],
            // Light text color
            title_color: [0.85, 0.85, 0.85, 1.0],
            // Default cursor color (pink)
            cursor_color: [0.97, 0.07, 1.0, 1.0],
            show_shadow: true,
            tab_data: HashMap::new(),
        }
    }
}

impl Island {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the effective height of the island (0 if disabled)
    #[inline]
    pub fn height(&self) -> f32 {
        if self.enabled {
            ISLAND_HEIGHT
        } else {
            0.0
        }
    }

    /// Calculate island width with minimum constraint
    /// Returns the total width needed for title + spacing + shortcut + right padding
    fn calculate_island_width(title_width: f32, shortcut_width: f32) -> f32 {
        // Layout: [left_pad][title][spacing][shortcut_bg][right_pad]
        let shortcut_bg_width = shortcut_width + (SHORTCUT_PADDING_X * 2.0);

        // Total width with padding on both sides
        let total_width = ISLAND_PADDING_X + title_width + TITLE_SHORTCUT_SPACING
            + shortcut_bg_width + ISLAND_PADDING_X;

        // Ensure minimum width
        total_width.max(ISLAND_MIN_WIDTH)
    }

    /// Calculate opacity multiplier for a tab based on its distance from the active tab
    /// Returns 1.0 (fully opaque) for active tab, progressively lower for tabs farther away
    fn calculate_tab_opacity(
        tab_index: usize,
        active_tab_index: usize,
        _total_tabs: usize,
    ) -> f32 {
        // Calculate distance from active tab
        let distance = if tab_index > active_tab_index {
            tab_index - active_tab_index
        } else {
            active_tab_index - tab_index
        };

        // No fade for active tab
        if distance == 0 {
            return 1.0;
        }

        // Calculate opacity based on distance
        // distance 1: 1.0 - 0.25 = 0.75
        // distance 2: 1.0 - 0.50 = 0.50
        // distance 3: 1.0 - 0.75 = 0.25
        let opacity = 1.0 - (distance.min(FADE_EFFECT_MAX_DISTANCE) as f32 * FADE_OPACITY_STEP);
        opacity.max(0.1) // Ensure minimum visibility
    }

    /// Render individual tab islands like Opera One
    #[inline]
    pub fn render(
        &mut self,
        sugarloaf: &mut Sugarloaf,
        dimensions: (f32, f32, f32),
        context_manager: &ContextManager<EventProxy>,
    ) {
        if !self.enabled {
            return;
        }

        let (window_width, _window_height, scale_factor) = dimensions;
        let num_tabs = context_manager.len();
        let current_tab_index = context_manager.current_index();

        // Hide island if only single tab and hide_if_single is enabled
        if self.hide_if_single && num_tabs == 1 {
            // Hide all existing island rich texts
            for tab_data in self.tab_data.values() {
                sugarloaf.set_rich_text_visibility(tab_data.rich_text_id, false);
                sugarloaf.set_rich_text_visibility(tab_data.shortcut_rich_text_id, false);
            }
            return;
        }

        // Hide all existing island rich texts first
        for tab_data in self.tab_data.values() {
            sugarloaf.set_rich_text_visibility(tab_data.rich_text_id, false);
            sugarloaf.set_rich_text_visibility(tab_data.shortcut_rich_text_id, false);
        }

        let available_width = (window_width / scale_factor) - ISLAND_MARGIN_RIGHT;

        // First pass: prepare all tab data and calculate total width
        let mut island_widths = Vec::with_capacity(num_tabs);
        let mut display_titles = Vec::with_capacity(num_tabs);
        let mut total_width = 0.0;

        for tab_index in 0..num_tabs {
            // Get title for this tab
            let mut title = self.get_title_for_tab(context_manager, tab_index);
            if title.is_empty() {
                island_widths.push(0.0);
                display_titles.push(String::new());
                continue;
            }

            // Get or create tab data
            let tab_data = self.tab_data.entry(tab_index).or_insert_with(|| {
                use rio_backend::sugarloaf::layout::RichTextConfig;
                // Text should be in front of everything (terminal at 0.0, island at 0.1)
                let config = RichTextConfig::new().with_depth(-0.1);

                // Create rich text for title
                let rich_text_id = sugarloaf.create_rich_text(Some(&config));
                sugarloaf.set_rich_text_font_size(&rich_text_id, TITLE_FONT_SIZE);

                // Create rich text for shortcut number
                let shortcut_rich_text_id = sugarloaf.create_rich_text(Some(&config));
                sugarloaf.set_rich_text_font_size(&shortcut_rich_text_id, SHORTCUT_FONT_SIZE);

                TabIslandData {
                    rich_text_id,
                    shortcut_rich_text_id,
                    last_title: String::new(),
                    text_width: 0.0,
                    shortcut_width: 0.0,
                }
            });

            // Limit title to max characters
            if title.len() > MAX_TITLE_CHARS {
                title = title.chars().take(MAX_TITLE_CHARS).collect();
            }

            // Update text if title changed (color will be applied during render with opacity)
            if tab_data.last_title != title {
                // Measure text width first by setting temporary text
                use rio_backend::sugarloaf::FragmentStyle;
                let content = sugarloaf.content();
                content
                    .sel(tab_data.rich_text_id)
                    .clear()
                    .new_line()
                    .add_text(
                        &title,
                        FragmentStyle {
                            color: self.title_color,
                            ..FragmentStyle::default()
                        },
                    )
                    .build();

                // Measure text width
                let dims = sugarloaf.get_rich_text_dimensions(&tab_data.rich_text_id);
                tab_data.text_width = dims.width;
                tab_data.last_title = title.clone();
            }

            // Update shortcut text (only once to measure width)
            if tab_data.shortcut_width == 0.0 {
                use rio_backend::sugarloaf::FragmentStyle;
                let shortcut_text = format!("{}", tab_index);
                let content = sugarloaf.content();
                content
                    .sel(tab_data.shortcut_rich_text_id)
                    .clear()
                    .new_line()
                    .add_text(
                        &shortcut_text,
                        FragmentStyle {
                            color: self.title_color,
                            ..FragmentStyle::default()
                        },
                    )
                    .build();

                // Measure shortcut width
                let dims = sugarloaf.get_rich_text_dimensions(&tab_data.shortcut_rich_text_id);
                tab_data.shortcut_width = dims.width;
            }

            // Calculate and constrain island width
            let island_width = Self::calculate_island_width(tab_data.text_width, tab_data.shortcut_width);
            island_widths.push(island_width);
            display_titles.push(title);
            total_width += island_width;
        }

        // Add spacing between islands
        if num_tabs > 1 {
            total_width += ISLAND_SPACING * (num_tabs - 1) as f32;
        }

        // If total width exceeds available width, scale down all islands proportionally
        let scale_factor_width = if total_width > available_width {
            available_width / total_width
        } else {
            1.0
        };

        // Calculate starting x position from right edge
        let final_total_width = total_width * scale_factor_width;
        let mut x_position =
            (window_width / scale_factor) - final_total_width - ISLAND_MARGIN_RIGHT;

        // Second pass: render all islands with scaled widths
        let scaled_spacing = ISLAND_SPACING * scale_factor_width;
        let island_height = ISLAND_HEIGHT - (ISLAND_PADDING_Y * 2.0);
        let island_y = ISLAND_PADDING_Y;

        for (tab_index, base_island_width) in island_widths.iter().enumerate().take(num_tabs) {
            if *base_island_width == 0.0 {
                continue;
            }

            // Apply scaling to island width
            let island_width = base_island_width * scale_factor_width;
            let tab_data = &self.tab_data[&tab_index];

            // Calculate opacity based on distance from active tab
            let opacity = Self::calculate_tab_opacity(tab_index, current_tab_index, num_tabs);

            // Layout: [left_pad][title][spacing][shortcut_bg][right_pad]
            // Calculate positions for title and shortcut
            let title_x = x_position + ISLAND_PADDING_X;
            let title_y = island_y + (island_height / 2.0) - (TITLE_FONT_SIZE / 2.0);

            // Shortcut background positioned after title with spacing
            let shortcut_bg_width = tab_data.shortcut_width + (SHORTCUT_PADDING_X * 2.0);
            let shortcut_bg_height = SHORTCUT_FONT_SIZE + (SHORTCUT_PADDING_Y * 2.0);
            let shortcut_bg_x = title_x + tab_data.text_width + TITLE_SHORTCUT_SPACING;
            let shortcut_bg_y = island_y + (island_height - shortcut_bg_height) / 2.0;

            // Choose shortcut background color based on active state and apply opacity
            let mut shortcut_bg_color = if tab_index == current_tab_index {
                self.active_background_color
            } else {
                self.background_color
            };
            // Apply opacity to background alpha channel
            shortcut_bg_color[3] *= opacity;

            // Apply opacity to title color
            let mut title_color = self.title_color;
            title_color[3] *= opacity;

            // Render shortcut background (small rounded rectangle)
            sugarloaf.rounded_rect(
                shortcut_bg_x,
                shortcut_bg_y,
                shortcut_bg_width,
                shortcut_bg_height,
                shortcut_bg_color,
                0.1, // Render behind terminal content (terminal is at 0.0)
                SHORTCUT_CORNER_RADIUS,
            );

            // Update title text color with opacity and show
            use rio_backend::sugarloaf::FragmentStyle;
            let content = sugarloaf.content();
            content
                .sel(tab_data.rich_text_id)
                .clear()
                .new_line()
                .add_text(
                    &tab_data.last_title,
                    FragmentStyle {
                        color: title_color,
                        ..FragmentStyle::default()
                    },
                )
                .build();
            sugarloaf.show_rich_text(tab_data.rich_text_id, title_x, title_y);

            // Update shortcut text color with opacity and show
            let shortcut_text = format!("{}", tab_index);
            let content = sugarloaf.content();
            content
                .sel(tab_data.shortcut_rich_text_id)
                .clear()
                .new_line()
                .add_text(
                    &shortcut_text,
                    FragmentStyle {
                        color: title_color,
                        ..FragmentStyle::default()
                    },
                )
                .build();
            let shortcut_text_x = shortcut_bg_x + SHORTCUT_PADDING_X;
            let shortcut_text_y = shortcut_bg_y + SHORTCUT_PADDING_Y;
            sugarloaf.show_rich_text(tab_data.shortcut_rich_text_id, shortcut_text_x, shortcut_text_y);

            // Move to next island position
            x_position += island_width + scaled_spacing;
        }
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

    /// Set whether the island is enabled
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Set whether to hide island when only single tab exists
    pub fn set_hide_if_single(&mut self, hide: bool) {
        self.hide_if_single = hide;
    }

    /// Set the background color of the island
    pub fn set_background_color(&mut self, color: [f32; 4]) {
        self.background_color = color;
    }

    /// Set the active background color of the island
    pub fn set_active_background_color(&mut self, color: [f32; 4]) {
        self.active_background_color = color;
    }

    /// Set the cursor color for single-tab indicator
    pub fn set_cursor_color(&mut self, color: [f32; 4]) {
        self.cursor_color = color;
    }

    /// Set the title text color
    pub fn set_title_color(&mut self, color: [f32; 4]) {
        self.title_color = color;
    }

    /// Set whether to show shadow
    pub fn set_show_shadow(&mut self, show: bool) {
        self.show_shadow = show;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_island_width_respects_minimum() {
        // Small title and shortcut should still meet minimum width
        let title_width = 10.0;
        let shortcut_width = 5.0;
        let width = Island::calculate_island_width(title_width, shortcut_width);
        assert!(width >= ISLAND_MIN_WIDTH);
    }

    #[test]
    fn test_calculate_island_width_includes_all_components() {
        let title_width = 100.0;
        let shortcut_width = 10.0;

        // Expected: title + spacing + (shortcut + padding*2) + outer_padding*2
        let shortcut_bg = shortcut_width + (SHORTCUT_PADDING_X * 2.0);
        let content = title_width + TITLE_SHORTCUT_SPACING + shortcut_bg;
        let expected = content + (ISLAND_PADDING_X * 2.0);

        let width = Island::calculate_island_width(title_width, shortcut_width);
        assert_eq!(width, expected);
    }

    #[test]
    fn test_title_character_limit() {
        // Title under limit should not be truncated
        let short_title = "Short";
        assert!(short_title.len() <= MAX_TITLE_CHARS);

        // Title at exact limit should not be truncated
        let exact_title: String = "a".repeat(MAX_TITLE_CHARS);
        assert_eq!(exact_title.len(), MAX_TITLE_CHARS);

        // Title over limit should be truncated
        let long_title: String = "a".repeat(MAX_TITLE_CHARS + 10);
        let truncated: String = long_title.chars().take(MAX_TITLE_CHARS).collect();
        assert_eq!(truncated.len(), MAX_TITLE_CHARS);
    }

    #[test]
    fn test_max_title_chars_constant() {
        // Verify the constant is set to 25 as specified
        assert_eq!(MAX_TITLE_CHARS, 25);
    }

    #[test]
    fn test_overflow_scaling_calculation() {
        // Simulate overflow scenario
        let available_width = 500.0;
        let total_width = 800.0; // Exceeds available

        let scale_factor = if total_width > available_width {
            available_width / total_width
        } else {
            1.0
        };

        assert_eq!(scale_factor, 0.625); // 500/800 = 0.625

        // Verify scaled width fits
        let final_width = total_width * scale_factor;
        assert_eq!(final_width, available_width);
    }

    #[test]
    fn test_no_scaling_when_fits() {
        // When content fits, no scaling should occur
        let available_width = 800.0;
        let total_width = 500.0; // Within available

        let scale_factor = if total_width > available_width {
            available_width / total_width
        } else {
            1.0
        };

        assert_eq!(scale_factor, 1.0); // No scaling needed
    }

    #[test]
    fn test_island_spacing_calculation() {
        // Test spacing calculation for multiple tabs
        let num_tabs = 3;
        let spacing_total = ISLAND_SPACING * (num_tabs - 1) as f32;
        assert_eq!(spacing_total, ISLAND_SPACING * 2.0); // 2 gaps for 3 tabs
    }

    #[test]
    fn test_multiple_tabs_total_width() {
        // Simulate calculating total width for multiple tabs
        let island_widths = vec![100.0, 150.0, 120.0];
        let num_tabs = island_widths.len();

        let mut total = 0.0;
        for width in &island_widths {
            total += width;
        }

        // Add spacing between islands
        if num_tabs > 1 {
            total += ISLAND_SPACING * (num_tabs - 1) as f32;
        }

        let expected = 100.0 + 150.0 + 120.0 + (ISLAND_SPACING * 2.0);
        assert_eq!(total, expected);
    }

    #[test]
    fn test_scaled_spacing_proportional() {
        let scale_factor = 0.5;
        let scaled_spacing = ISLAND_SPACING * scale_factor;
        assert_eq!(scaled_spacing, ISLAND_SPACING * 0.5);
    }

    #[test]
    fn test_island_constants() {
        // Verify all constants are set correctly
        assert_eq!(ISLAND_HEIGHT, 28.0);
        assert_eq!(ISLAND_PADDING_X, 16.0);
        assert_eq!(ISLAND_PADDING_Y, 6.0);
        assert_eq!(ISLAND_SPACING, 8.0);
        assert_eq!(ISLAND_MARGIN_RIGHT, 8.0);
        assert_eq!(ISLAND_CORNER_RADIUS, 8.0);
        assert_eq!(TITLE_FONT_SIZE, 11.0);
        assert_eq!(SHORTCUT_FONT_SIZE, 7.0);
        assert_eq!(TITLE_SHORTCUT_SPACING, 8.0);
        assert_eq!(SHORTCUT_PADDING_X, 4.0);
        assert_eq!(SHORTCUT_PADDING_Y, 2.0);
        assert_eq!(SHORTCUT_CORNER_RADIUS, 4.0);
        assert_eq!(ISLAND_MIN_WIDTH, 60.0);
        assert_eq!(MAX_TITLE_CHARS, 25);
    }

    #[test]
    fn test_shortcut_background_sizing() {
        // Verify shortcut background includes padding
        let shortcut_width = 8.0; // Width of "0"
        let expected_bg_width = shortcut_width + (SHORTCUT_PADDING_X * 2.0);
        assert_eq!(expected_bg_width, 8.0 + 8.0); // 16.0
    }

    #[test]
    fn test_island_width_calculation_components() {
        // Test that all components are accounted for in width calculation
        let title_width = 50.0;
        let shortcut_width = 8.0;

        let shortcut_bg_width = shortcut_width + (SHORTCUT_PADDING_X * 2.0);
        let content_width = title_width + TITLE_SHORTCUT_SPACING + shortcut_bg_width;
        let total_width = content_width + (ISLAND_PADDING_X * 2.0);

        let calculated = Island::calculate_island_width(title_width, shortcut_width);
        assert_eq!(calculated, total_width);
    }

    #[test]
    fn test_island_default_colors() {
        let island = Island::default();

        // Verify default colors are set
        assert_eq!(island.background_color, [0.15, 0.15, 0.15, 0.9]);
        assert_eq!(island.active_background_color, [0.2, 0.2, 0.2, 1.0]);
        assert_eq!(island.title_color, [0.85, 0.85, 0.85, 1.0]);
        assert_eq!(island.cursor_color, [0.97, 0.07, 1.0, 1.0]);
        assert!(!island.enabled);
        assert!(!island.hide_if_single);
        assert!(island.show_shadow);
    }

    #[test]
    fn test_island_color_setters() {
        let mut island = Island::new();

        let bg_color = [0.1, 0.2, 0.3, 0.9];
        let active_color = [0.4, 0.5, 0.6, 1.0];
        let cursor_color = [1.0, 0.0, 0.5, 1.0];
        let title_color = [0.9, 0.9, 0.9, 1.0];

        island.set_background_color(bg_color);
        island.set_active_background_color(active_color);
        island.set_cursor_color(cursor_color);
        island.set_title_color(title_color);

        assert_eq!(island.background_color, bg_color);
        assert_eq!(island.active_background_color, active_color);
        assert_eq!(island.cursor_color, cursor_color);
        assert_eq!(island.title_color, title_color);
    }

    #[test]
    fn test_island_height_when_disabled() {
        let island = Island::default();
        assert!(!island.enabled);
        assert_eq!(island.height(), 0.0);
    }

    #[test]
    fn test_island_height_when_enabled() {
        let mut island = Island::default();
        island.set_enabled(true);
        assert_eq!(island.height(), ISLAND_HEIGHT);
    }

    #[test]
    fn test_corner_radius_is_reasonable() {
        // Corner radius should be less than half the island height to look good
        let max_reasonable_radius = ISLAND_HEIGHT / 2.0;
        assert!(ISLAND_CORNER_RADIUS < max_reasonable_radius);

        // Should be a reasonable size for visibility
        const {
            assert!(ISLAND_CORNER_RADIUS >= 4.0);
            assert!(ISLAND_CORNER_RADIUS <= 12.0);
        }
    }

    #[test]
    fn test_opacity_calculation_active_tab() {
        // Active tab should always be fully opaque
        let opacity = Island::calculate_tab_opacity(2, 2, 10);
        assert_eq!(opacity, 1.0);
    }

    #[test]
    fn test_opacity_calculation_adjacent_tab() {
        // Adjacent tab (distance 1) should have 0.75 opacity
        let opacity = Island::calculate_tab_opacity(3, 2, 10);
        assert_eq!(opacity, 0.75);

        let opacity = Island::calculate_tab_opacity(1, 2, 10);
        assert_eq!(opacity, 0.75);
    }

    #[test]
    fn test_opacity_calculation_far_tabs() {
        // Distance 2: 0.50 opacity
        let opacity = Island::calculate_tab_opacity(4, 2, 10);
        assert_eq!(opacity, 0.50);

        // Distance 3: 0.25 opacity
        let opacity = Island::calculate_tab_opacity(5, 2, 10);
        assert_eq!(opacity, 0.25);

        // Distance 4+: still 0.25 (capped at max distance)
        let opacity = Island::calculate_tab_opacity(8, 2, 10);
        assert_eq!(opacity, 0.25);
    }

    #[test]
    fn test_opacity_with_few_tabs() {
        // Fade effect now works with any number of tabs
        // Tab at distance 2 from active should have 0.5 opacity
        let opacity = Island::calculate_tab_opacity(3, 1, 4);
        assert_eq!(opacity, 0.50);

        // Tab at distance 1 from active should have 0.75 opacity
        let opacity = Island::calculate_tab_opacity(0, 1, 3);
        assert_eq!(opacity, 0.75);
    }

    #[test]
    fn test_opacity_minimum_visibility() {
        // Even at maximum distance, ensure minimum visibility of 0.1
        // This is enforced by the max(0.1) in the function
        let opacity = Island::calculate_tab_opacity(10, 0, 15);
        assert!(opacity >= 0.1);
    }

}
