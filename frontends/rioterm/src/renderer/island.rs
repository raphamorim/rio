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

/// Font size for tab titles
const RIO_INDICATOR_FONT_SIZE: f32 = 2.0;

/// Maximum characters to display in a tab title
const MAX_TITLE_CHARS: usize = 25;

/// Minimum width for a single island
const ISLAND_MIN_WIDTH: f32 = 60.0;

/// Width of the small indicator inside single tab
const SINGLE_TAB_INDICATOR_WIDTH: f32 = 5.0;

/// Height of the small indicator inside single tab
const SINGLE_TAB_INDICATOR_HEIGHT: f32 = 16.0;

/// Width of the island container when showing single tab indicator
const SINGLE_TAB_ISLAND_WIDTH: f32 = 18.0;

/// Data for each individual tab island
struct TabIslandData {
    /// Rich text ID for this tab's title
    rich_text_id: usize,
    /// Last rendered title (for change detection)
    last_title: String,
    /// Cached text width from last measurement
    text_width: f32,
}

pub struct Island {
    /// Whether the island is enabled
    pub enabled: bool,
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
    /// Rich text ID for the single-tab indicator Unicode character
    indicator_rich_text_id: Option<usize>,
}

impl Default for Island {
    fn default() -> Self {
        Self {
            // Disabled by default - can be enabled via configuration
            enabled: false,
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
            indicator_rich_text_id: None,
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
    /// Returns (island_width, actual_padding_x)
    fn calculate_island_width(text_width: f32) -> (f32, f32) {
        let natural_width = text_width + (ISLAND_PADDING_X * 2.0);

        if natural_width >= ISLAND_MIN_WIDTH {
            // Text is long enough, use standard padding
            (natural_width, ISLAND_PADDING_X)
        } else {
            // Text is short, increase padding to meet minimum width
            let extra_space = ISLAND_MIN_WIDTH - text_width;
            let padding = extra_space / 2.0;
            (ISLAND_MIN_WIDTH, padding)
        }
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

        // Hide all existing island rich texts first
        for tab_data in self.tab_data.values() {
            sugarloaf.hide_rich_text(tab_data.rich_text_id);
        }
        // Hide indicator rich text if it exists
        if let Some(rich_text_id) = self.indicator_rich_text_id {
            sugarloaf.hide_rich_text(rich_text_id);
        }

        // Always render the single-tab indicator (leftmost element)
        let indicator_island_width = (Self::calculate_island_width(RIO_INDICATOR_FONT_SIZE)).0;
        let island_height = ISLAND_HEIGHT - (ISLAND_PADDING_Y * 2.0);
        let island_y = ISLAND_PADDING_Y;

        // Calculate starting position (will be adjusted based on total width)
        let mut indicator_x =
            (window_width / scale_factor) - indicator_island_width - ISLAND_MARGIN_RIGHT;

        // If we have multiple tabs, we need to account for their width too
        let available_width = if num_tabs > 1 {
            (window_width / scale_factor)
                - indicator_island_width
                - ISLAND_SPACING
                - ISLAND_MARGIN_RIGHT
        } else {
            (window_width / scale_factor) - ISLAND_MARGIN_RIGHT
        };

        // First pass: prepare all tab data and calculate total width
        // Skip this if we only have 1 tab (we'll just show the indicator)
        let mut island_widths = Vec::with_capacity(num_tabs);
        let mut display_titles = Vec::with_capacity(num_tabs);
        let mut total_width = 0.0;

        if num_tabs > 1 {
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
                    let rich_text_id = sugarloaf.create_rich_text(Some(&config));
                    sugarloaf.set_rich_text_font_size(&rich_text_id, TITLE_FONT_SIZE);
                    TabIslandData {
                        rich_text_id,
                        last_title: String::new(),
                        text_width: 0.0,
                    }
                });

                // Limit title to max characters
                if title.len() > MAX_TITLE_CHARS {
                    title = title.chars().take(MAX_TITLE_CHARS).collect();
                }

                // Update text if title changed
                if tab_data.last_title != title {
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

                // Calculate and constrain island width
                let (island_width, _padding_x) =
                    Self::calculate_island_width(tab_data.text_width);
                island_widths.push(island_width);
                display_titles.push(title);
                total_width += island_width;
            }

            // Add spacing between islands
            total_width += ISLAND_SPACING * (num_tabs - 1) as f32;
        }

        // If total width exceeds available width, scale down all islands proportionally
        let scale_factor_width = if total_width > available_width {
            available_width / total_width
        } else {
            1.0
        };

        // Calculate starting x position from right edge
        // If we have tabs, leave space for indicator on the right
        let final_total_width = total_width * scale_factor_width;
        let mut x_position = if num_tabs > 1 {
            (window_width / scale_factor)
                - final_total_width
                - indicator_island_width
                - ISLAND_SPACING
                - ISLAND_MARGIN_RIGHT
        } else {
            (window_width / scale_factor) - final_total_width - ISLAND_MARGIN_RIGHT
        };

        // Second pass: render all islands with scaled widths (only if num_tabs > 1)
        if num_tabs > 1 {
            let scaled_spacing = ISLAND_SPACING * scale_factor_width;

            for tab_index in 0..num_tabs {
                let base_island_width = island_widths[tab_index];
                if base_island_width == 0.0 {
                    continue;
                }

                // Apply scaling to island width
                let island_width = base_island_width * scale_factor_width;
                let is_active = tab_index == current_tab_index;
                let island_height = ISLAND_HEIGHT - (ISLAND_PADDING_Y * 2.0);
                let island_y = ISLAND_PADDING_Y;

                // Choose background color based on active state
                let bg_color = if is_active {
                    self.active_background_color
                } else {
                    self.background_color
                };

                // Render island background (rounded rectangle)
                sugarloaf.rounded_rect(
                    x_position,
                    island_y,
                    island_width,
                    island_height,
                    bg_color,
                    0.1, // Render behind terminal content (terminal is at 0.0)
                    ISLAND_CORNER_RADIUS,
                );

                // Position and show title text
                let tab_data = &self.tab_data[&tab_index];
                // Recalculate padding for the scaled island width to keep text centered
                let text_padding = (island_width - tab_data.text_width) / 2.0;
                let text_x = x_position + text_padding;
                let text_y = island_y + (island_height / 2.0) - (TITLE_FONT_SIZE / 2.0);
                sugarloaf.show_rich_text(tab_data.rich_text_id, text_x, text_y);

                // Move to next island position
                x_position += island_width + scaled_spacing;
            }

            // Position indicator after all tabs with spacing
            indicator_x = x_position;
        }

        // Create indicator rich text if it doesn't exist
        if self.indicator_rich_text_id.is_none() {
            use rio_backend::sugarloaf::layout::RichTextConfig;
            // Text should be in front of everything (terminal at 0.0, island at 0.1)
            let config = RichTextConfig::new().with_depth(-0.1);
            let rich_text_id = sugarloaf.create_rich_text(Some(&config));
            sugarloaf.set_rich_text_font_size(&rich_text_id, RIO_INDICATOR_FONT_SIZE);
            self.indicator_rich_text_id = Some(rich_text_id);
            use rio_backend::sugarloaf::{FragmentStyle, drawable_character};
            let content = sugarloaf.content();

            let mut style = FragmentStyle {
                color: self.title_color,
                ..FragmentStyle::default()
            };

            // Check if this character should be rendered as a drawable
            if let Some(character) = drawable_character('\u{1CC6D}') {
                style.drawable_char = Some(character);
                style.width = 1.0;
            }

            content
                .sel(rich_text_id)
                .clear()
                .new_line()
                .add_text(
                    "\u{1CC6D}",
                    style,
                )
                .add_text(
                    "\u{1CC6D}",
                    style,
                )
                .build();
        }

        // Render indicator Unicode character
        if let Some(rich_text_id) = self.indicator_rich_text_id {
            // Position the indicator centered in the island
            let indicator_x = indicator_x + (indicator_island_width / 2.0) - (RIO_INDICATOR_FONT_SIZE / 2.0);
            let indicator_y = island_y + (island_height / 2.0) - (RIO_INDICATOR_FONT_SIZE / 2.0);

            sugarloaf.show_rich_text(rich_text_id, indicator_x, indicator_y);
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
        // Width below minimum should return minimum width with adjusted padding
        let (width, padding) = Island::calculate_island_width(10.0);
        assert_eq!(width, ISLAND_MIN_WIDTH);
        // Padding should be increased to center the text: (60 - 10) / 2 = 25
        assert_eq!(padding, 25.0);

        // Width above minimum should return calculated width with standard padding
        let text_width = 100.0;
        let expected = text_width + (ISLAND_PADDING_X * 2.0);
        let (width, padding) = Island::calculate_island_width(text_width);
        assert_eq!(width, expected);
        assert_eq!(padding, ISLAND_PADDING_X);
    }

    #[test]
    fn test_calculate_island_width_adds_padding() {
        let text_width = 100.0;
        let expected = text_width + (ISLAND_PADDING_X * 2.0); // 100 + 32 = 132
        let (width, padding) = Island::calculate_island_width(text_width);
        assert_eq!(width, expected);
        assert_eq!(padding, ISLAND_PADDING_X);
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
        assert_eq!(ISLAND_MIN_WIDTH, 60.0);
        assert_eq!(MAX_TITLE_CHARS, 25);
        assert_eq!(SINGLE_TAB_INDICATOR_WIDTH, 5.0);
        assert_eq!(SINGLE_TAB_INDICATOR_HEIGHT, 16.0);
        assert_eq!(SINGLE_TAB_ISLAND_WIDTH, 18.0);
    }

    #[test]
    fn test_padding_centers_short_text() {
        // For very short text like "Tab 3" (assume ~20px width)
        let short_text_width = 20.0;
        let (width, padding) = Island::calculate_island_width(short_text_width);

        assert_eq!(width, ISLAND_MIN_WIDTH);
        // Padding should be: (60 - 20) / 2 = 20.0
        assert_eq!(padding, 20.0);

        // Verify text is centered: padding + text_width + padding = island_width
        assert_eq!(padding * 2.0 + short_text_width, width);
    }

    #[test]
    fn test_padding_for_medium_text() {
        // For medium text like "zsh" (assume ~35px width)
        // With standard padding (16px each side), natural width = 35 + 32 = 67px
        // This exceeds minimum (60px), so use natural width with standard padding
        let medium_text_width = 35.0;
        let (width, padding) = Island::calculate_island_width(medium_text_width);

        // Natural width with standard padding
        let expected_width = medium_text_width + (ISLAND_PADDING_X * 2.0);
        assert_eq!(width, expected_width); // 67.0
        assert_eq!(padding, ISLAND_PADDING_X); // 16.0

        // Verify text is centered
        assert_eq!(padding * 2.0 + medium_text_width, width);
    }

    #[test]
    fn test_padding_at_minimum_threshold() {
        // When natural width exactly equals minimum
        let text_width = ISLAND_MIN_WIDTH - (ISLAND_PADDING_X * 2.0); // 60 - 32 = 28
        let (width, padding) = Island::calculate_island_width(text_width);

        // Should return minimum width with standard padding
        assert_eq!(width, ISLAND_MIN_WIDTH);
        assert_eq!(padding, ISLAND_PADDING_X);
    }

    #[test]
    fn test_padding_just_above_minimum() {
        // When text is just slightly larger than minimum threshold
        let text_width = ISLAND_MIN_WIDTH - (ISLAND_PADDING_X * 2.0) + 0.1; // Just above threshold
        let (width, padding) = Island::calculate_island_width(text_width);

        // Should return calculated width with standard padding
        let expected_width = text_width + (ISLAND_PADDING_X * 2.0);
        assert_eq!(width, expected_width);
        assert_eq!(padding, ISLAND_PADDING_X);
    }

    #[test]
    fn test_padding_consistency() {
        // Test that padding always results in centered text
        let test_cases = vec![5.0, 10.0, 15.0, 20.0, 25.0];

        for text_width in test_cases {
            let (island_width, padding) = Island::calculate_island_width(text_width);

            // For all short text, padding * 2 + text_width should equal island_width
            if island_width == ISLAND_MIN_WIDTH {
                let total = padding * 2.0 + text_width;
                assert_eq!(
                    total, island_width,
                    "Text width {} should be centered with padding {}",
                    text_width, padding
                );
            }
        }
    }

    #[test]
    fn test_single_tab_indicator_fits_in_island() {
        // Verify the indicator fits within the island container
        assert!(SINGLE_TAB_INDICATOR_WIDTH < SINGLE_TAB_ISLAND_WIDTH);
        assert!(SINGLE_TAB_INDICATOR_HEIGHT <= ISLAND_HEIGHT - (ISLAND_PADDING_Y * 2.0));
    }

    #[test]
    fn test_single_tab_indicator_centering() {
        // Calculate centering for indicator
        let island_width = SINGLE_TAB_ISLAND_WIDTH;
        let island_height = ISLAND_HEIGHT - (ISLAND_PADDING_Y * 2.0);

        // Horizontal centering
        let x_padding = (island_width - SINGLE_TAB_INDICATOR_WIDTH) / 2.0;
        assert_eq!(x_padding, (18.0 - 5.0) / 2.0); // 6.5px on each side

        // Vertical centering
        let y_padding = (island_height - SINGLE_TAB_INDICATOR_HEIGHT) / 2.0;
        assert_eq!(y_padding, (16.0 - 16.0) / 2.0); // 0.0px on top and bottom (fits exactly)
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
        assert!(ISLAND_CORNER_RADIUS >= 4.0);
        assert!(ISLAND_CORNER_RADIUS <= 12.0);
    }

    #[test]
    fn test_indicator_proportions() {
        // Indicator should be taller than it is wide (vertical orientation)
        assert!(SINGLE_TAB_INDICATOR_HEIGHT > SINGLE_TAB_INDICATOR_WIDTH);

        // Height should be roughly 2-4x the width for good proportions
        let ratio = SINGLE_TAB_INDICATOR_HEIGHT / SINGLE_TAB_INDICATOR_WIDTH;
        assert!(
            ratio >= 2.0 && ratio <= 4.0,
            "Ratio {} should be between 2 and 4",
            ratio
        );
    }

    #[test]
    fn test_single_tab_island_width_reasonable() {
        // Island width should provide adequate padding around indicator
        let min_padding = (SINGLE_TAB_ISLAND_WIDTH - SINGLE_TAB_INDICATOR_WIDTH) / 2.0;
        assert!(
            min_padding >= 4.0,
            "Padding {} should be at least 4px",
            min_padding
        );

        // But not be excessively wide
        assert!(SINGLE_TAB_ISLAND_WIDTH <= 30.0);
    }
}
