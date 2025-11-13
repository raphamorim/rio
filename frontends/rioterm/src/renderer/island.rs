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

/// Height of the tab bar in pixels
pub const ISLAND_HEIGHT: f32 = 34.0;

/// Font size for tab titles
const TITLE_FONT_SIZE: f32 = 12.0;

/// Left/right padding inside tab text
const TAB_PADDING_X: f32 = 24.0;

/// Right margin after last tab
const ISLAND_MARGIN_RIGHT: f32 = 8.0;

/// Left margin on macOS to account for traffic light buttons
#[cfg(target_os = "macos")]
const ISLAND_MARGIN_LEFT_MACOS: f32 = 76.0;

/// Data for each individual tab
struct TabIslandData {
    /// Rich text ID for this tab's title
    rich_text_id: usize,
    /// Last rendered title (for change detection)
    last_title: String,
}

pub struct Island {
    /// Whether the island is enabled
    pub enabled: bool,
    /// Hide island when only a single tab exists
    pub hide_if_single: bool,
    /// Text color for inactive tabs (RGBA)
    pub inactive_text_color: [f32; 4],
    /// Text color for active tab (RGBA)
    pub active_text_color: [f32; 4],
    /// Border color (RGBA)
    pub border_color: [f32; 4],
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
            // Default inactive tab color: #ccc (0.8, 0.8, 0.8)
            inactive_text_color: [0.8, 0.8, 0.8, 1.0],
            // Default active tab color: #fff (1.0, 1.0, 1.0)
            active_text_color: [1.0, 1.0, 1.0, 1.0],
            // Default border color: #ccc (0.8, 0.8, 0.8)
            border_color: [0.8, 0.8, 0.8, 1.0],
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

    /// Render tabs using equal-width layout
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

        // Hide tabs if only single tab and hide_if_single is enabled
        if self.hide_if_single && num_tabs == 1 {
            // Hide all existing tab rich texts
            for tab_data in self.tab_data.values() {
                sugarloaf.set_rich_text_visibility(tab_data.rich_text_id, false);
            }
            return;
        }

        // Hide all existing tab rich texts first
        for tab_data in self.tab_data.values() {
            sugarloaf.set_rich_text_visibility(tab_data.rich_text_id, false);
        }

        // Calculate left margin (macOS needs space for traffic light buttons)
        #[cfg(target_os = "macos")]
        let left_margin = ISLAND_MARGIN_LEFT_MACOS;
        #[cfg(not(target_os = "macos"))]
        let left_margin = 0.0;

        // Calculate equal width for all tabs
        let available_width = (window_width / scale_factor) - ISLAND_MARGIN_RIGHT - left_margin;
        let tab_width = available_width / num_tabs as f32;

        // Starting from left edge (with margin on macOS for traffic lights)
        let mut x_position = left_margin;

        // Draw bottom border for the left margin area (traffic light space on macOS)
        if left_margin > 0.0 {
            sugarloaf.rect(
                0.0,
                ISLAND_HEIGHT - 1.0,
                left_margin,
                1.0,
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
                use rio_backend::sugarloaf::layout::RichTextConfig;
                // Text should be in front of everything (terminal at 0.0)
                let config = RichTextConfig::new().with_depth(-0.1);
                let rich_text_id = sugarloaf.create_rich_text(Some(&config));
                sugarloaf.set_rich_text_font_size(&rich_text_id, TITLE_FONT_SIZE);

                TabIslandData {
                    rich_text_id,
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
            use rio_backend::sugarloaf::FragmentStyle;
            let content = sugarloaf.content();
            content
                .sel(tab_data.rich_text_id)
                .clear()
                .new_line()
                .add_text(
                    &title,
                    FragmentStyle {
                        color: text_color,
                        ..FragmentStyle::default()
                    },
                )
                .build();
            tab_data.last_title = title.clone();

            // Get text dimensions to center it
            let text_dims = sugarloaf.get_rich_text_dimensions(&tab_data.rich_text_id);

            // Position text centered horizontally and vertically in the tab
            let text_x = x_position + (tab_width - text_dims.width) / 2.0;
            let text_y = (ISLAND_HEIGHT / 2.0) - (TITLE_FONT_SIZE / 2.0);
            sugarloaf.show_rich_text(tab_data.rich_text_id, text_x, text_y);

            // Draw vertical left border (separator between tabs) - skip for first tab
            if tab_index > 0 {
                sugarloaf.rect(
                    x_position,
                    0.0, // Start from top
                    1.0, // 1px width
                    ISLAND_HEIGHT,
                    self.border_color,
                    0.1, // Same depth as other island elements
                );
            }

            // Draw bottom border for inactive tabs (active tabs have no border)
            if !is_active {
                sugarloaf.rect(
                    x_position,
                    ISLAND_HEIGHT - 1.0, // 1px from bottom
                    tab_width,
                    1.0, // 1px height
                    self.border_color,
                    0.1, // Same depth as other island elements
                );
            }

            // Move to next tab position
            x_position += tab_width;
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

    /// Set the inactive text color
    pub fn set_inactive_text_color(&mut self, color: [f32; 4]) {
        self.inactive_text_color = color;
    }

    /// Set the active text color
    pub fn set_active_text_color(&mut self, color: [f32; 4]) {
        self.active_text_color = color;
    }

    /// Set the border color
    pub fn set_border_color(&mut self, color: [f32; 4]) {
        self.border_color = color;
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
    fn test_island_default_colors() {
        let island = Island::default();

        // Verify default colors are set correctly
        assert_eq!(island.inactive_text_color, [0.8, 0.8, 0.8, 1.0]); // #ccc
        assert_eq!(island.active_text_color, [1.0, 1.0, 1.0, 1.0]); // #fff
        assert_eq!(island.border_color, [0.8, 0.8, 0.8, 1.0]); // #ccc
        assert!(!island.enabled);
        assert!(!island.hide_if_single);
    }

    #[test]
    fn test_island_color_setters() {
        let mut island = Island::new();

        let inactive_color = [0.5, 0.5, 0.5, 1.0];
        let active_color = [0.9, 0.9, 0.9, 1.0];
        let border_color = [0.7, 0.7, 0.7, 1.0];

        island.set_inactive_text_color(inactive_color);
        island.set_active_text_color(active_color);
        island.set_border_color(border_color);

        assert_eq!(island.inactive_text_color, inactive_color);
        assert_eq!(island.active_text_color, active_color);
        assert_eq!(island.border_color, border_color);
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
}
