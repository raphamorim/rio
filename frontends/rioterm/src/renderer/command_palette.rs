// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use rio_backend::sugarloaf::Sugarloaf;

// Design constants
const PALETTE_WIDTH: f32 = 600.0;
const PALETTE_MAX_HEIGHT: f32 = 400.0;
const PALETTE_PADDING: f32 = 16.0;
const PALETTE_CORNER_RADIUS: f32 = 12.0;
const PALETTE_MARGIN_TOP: f32 = 100.0;

const INPUT_HEIGHT: f32 = 48.0;
const INPUT_FONT_SIZE: f32 = 16.0;
const INPUT_PADDING_X: f32 = 16.0;

const RESULT_ITEM_HEIGHT: f32 = 40.0;
const RESULT_FONT_SIZE: f32 = 14.0;
const RESULT_SPACING: f32 = 4.0;
const MAX_VISIBLE_RESULTS: usize = 8;

const SCROLLBAR_WIDTH: f32 = 6.0;
const SCROLLBAR_MARGIN: f32 = 4.0;
const SCROLLBAR_MIN_HEIGHT: f32 = 20.0;

/// Command palette state
#[derive(Debug, Clone)]
pub struct CommandPaletteItem {
    pub title: String,
    pub description: Option<String>,
    pub action: String,
}

/// Command palette UI component (Raycast-style)
pub struct CommandPalette {
    /// Whether the palette is visible
    enabled: bool,
    /// Current search query
    pub query: String,
    /// Selected result index
    selected_index: usize,
    /// Scroll offset (which item is at the top of the visible area)
    scroll_offset: usize,
    /// Available commands
    commands: Vec<CommandPaletteItem>,
    /// Background color
    background_color: [f32; 4],
    /// Input background color
    input_background_color: [f32; 4],
    /// Selected item background color
    selected_background_color: [f32; 4],
    /// Text color
    text_color: [f32; 4],
    /// Description text color
    description_color: [f32; 4],
    /// Rich text ID for input field (None if not initialized)
    input_text_id: Option<usize>,
    /// Rich text IDs for result items
    result_text_ids: Vec<usize>,
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self {
            enabled: false,
            query: String::new(),
            selected_index: 0,
            scroll_offset: 0,
            commands: Vec::new(),
            background_color: [0.1, 0.1, 0.1, 0.95], // Dark semi-transparent
            input_background_color: [0.15, 0.15, 0.15, 1.0],
            selected_background_color: [0.25, 0.25, 0.25, 1.0],
            text_color: [1.0, 1.0, 1.0, 1.0],
            description_color: [0.6, 0.6, 0.6, 1.0],
            input_text_id: None,
            result_text_ids: Vec::new(),
        }
    }
}

impl CommandPalette {
    pub fn new() -> Self {
        Self {
            // Add some default commands
            commands: vec![
                CommandPaletteItem {
                    title: "New Tab".to_string(),
                    description: Some("Create a new terminal tab".to_string()),
                    action: "new_tab".to_string(),
                },
                CommandPaletteItem {
                    title: "Close Tab".to_string(),
                    description: Some("Close the current tab".to_string()),
                    action: "close_tab".to_string(),
                },
                CommandPaletteItem {
                    title: "Split Horizontal".to_string(),
                    description: Some("Split terminal horizontally".to_string()),
                    action: "split_horizontal".to_string(),
                },
                CommandPaletteItem {
                    title: "Split Vertical".to_string(),
                    description: Some("Split terminal vertically".to_string()),
                    action: "split_vertical".to_string(),
                },
                CommandPaletteItem {
                    title: "Settings".to_string(),
                    description: Some("Open settings".to_string()),
                    action: "settings".to_string(),
                },
            ],
            ..Self::default()
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if enabled {
            self.query.clear();
            self.selected_index = 0;
            self.scroll_offset = 0;
        }
    }

    pub fn toggle(&mut self) {
        self.set_enabled(!self.enabled);
    }

    pub fn set_query(&mut self, query: String) {
        self.query = query;
        self.selected_index = 0;
        self.scroll_offset = 0; // Reset scroll when query changes
    }

    pub fn move_selection_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            // Scroll up if selection moves above visible area
            if self.selected_index < self.scroll_offset {
                self.scroll_offset = self.selected_index;
            }
            tracing::debug!(
                "Selection moved up: index={}, scroll={}",
                self.selected_index,
                self.scroll_offset
            );
        }
    }

    pub fn move_selection_down(&mut self) {
        let filtered_count = self.filtered_commands().len();
        if self.selected_index < filtered_count.saturating_sub(1) {
            self.selected_index += 1;
            // Scroll down if selection moves below visible area
            if self.selected_index >= self.scroll_offset + MAX_VISIBLE_RESULTS {
                self.scroll_offset = self.selected_index - MAX_VISIBLE_RESULTS + 1;
            }
            tracing::debug!(
                "Selection moved down: index={}, scroll={}, filtered={}",
                self.selected_index,
                self.scroll_offset,
                filtered_count
            );
        }
    }

    pub fn get_selected_action(&self) -> Option<String> {
        let filtered = self.filtered_commands();
        filtered
            .get(self.selected_index)
            .map(|cmd| cmd.action.clone())
    }

    fn filtered_commands(&self) -> Vec<&CommandPaletteItem> {
        if self.query.is_empty() {
            self.commands.iter().collect()
        } else {
            let query_lower = self.query.to_lowercase();
            self.commands
                .iter()
                .filter(|cmd| {
                    cmd.title.to_lowercase().contains(&query_lower)
                        || cmd
                            .description
                            .as_ref()
                            .is_some_and(|d| d.to_lowercase().contains(&query_lower))
                })
                .collect()
        }
    }

    pub fn height(&self) -> f32 {
        if !self.enabled {
            return 0.0;
        }

        // Always use MAX_VISIBLE_RESULTS for consistent height
        let results_height =
            (RESULT_ITEM_HEIGHT + RESULT_SPACING) * MAX_VISIBLE_RESULTS as f32;

        PALETTE_PADDING
            + INPUT_HEIGHT
            + PALETTE_PADDING
            + results_height
            + PALETTE_PADDING
    }

    pub fn render(&mut self, sugarloaf: &mut Sugarloaf, dimensions: (f32, f32, f32)) {
        if !self.enabled {
            return;
        }

        let (window_width, _window_height, scale_factor) = dimensions;
        let palette_width = PALETTE_WIDTH;
        let palette_height = self.height().min(PALETTE_MAX_HEIGHT);

        // Center horizontally, position near top
        let palette_x = (window_width / scale_factor - palette_width) / 2.0;
        let palette_y = PALETTE_MARGIN_TOP;

        // Render main background with rounded corners
        sugarloaf.rounded_rect(
            None,
            palette_x,
            palette_y,
            palette_width,
            palette_height,
            self.background_color,
            -0.3, // In front of everything
            PALETTE_CORNER_RADIUS,
            0,
        );

        // Render input field background
        let input_x = palette_x + PALETTE_PADDING;
        let input_y = palette_y + PALETTE_PADDING;
        let input_width = palette_width - (PALETTE_PADDING * 2.0);

        sugarloaf.rounded_rect(
            None,
            input_x,
            input_y,
            input_width,
            INPUT_HEIGHT,
            self.input_background_color,
            -0.4, // In front of palette background
            8.0,
            0,
        );

        // Render search query text
        let input_id = if let Some(id) = self.input_text_id {
            id
        } else {
            let id = sugarloaf.get_next_id();
            let _ = sugarloaf.text(Some(id));
            sugarloaf.set_use_grid_cell_size(id, false); // Proportional text
            sugarloaf.set_text_font_size(&id, INPUT_FONT_SIZE);
            self.input_text_id = Some(id);
            id
        };

        {
            use rio_backend::sugarloaf::SpanStyle;
            let display_text = if self.query.is_empty() {
                "Search commands..."
            } else {
                &self.query
            };

            let text_color = if self.query.is_empty() {
                self.description_color
            } else {
                self.text_color
            };

            let content = sugarloaf.content();
            content
                .sel(input_id)
                .clear()
                .new_line()
                .add_text(
                    display_text,
                    SpanStyle {
                        color: text_color,
                        ..SpanStyle::default()
                    },
                )
                .build();

            let text_x = input_x + INPUT_PADDING_X;
            let text_y = input_y + (INPUT_HEIGHT - INPUT_FONT_SIZE) / 2.0;
            sugarloaf.set_position(input_id, text_x, text_y);
            sugarloaf.set_visibility(input_id, true);
        }

        // Render results
        let results_y = input_y + INPUT_HEIGHT + PALETTE_PADDING;
        let visible_count = self.filtered_commands().len().min(MAX_VISIBLE_RESULTS);

        // Ensure we have enough rich text IDs for results
        while self.result_text_ids.len() < visible_count {
            let result_id = sugarloaf.get_next_id();
            let _ = sugarloaf.text(Some(result_id));
            sugarloaf.set_use_grid_cell_size(result_id, false); // Proportional text
            sugarloaf.set_text_font_size(&result_id, RESULT_FONT_SIZE);
            self.result_text_ids.push(result_id);
        }

        // Get filtered commands after modifying result_rich_text_ids
        let filtered = self.filtered_commands();
        let total_filtered = filtered.len();

        // Render visible items starting from scroll_offset
        for (display_i, cmd) in filtered
            .iter()
            .skip(self.scroll_offset)
            .take(visible_count)
            .enumerate()
        {
            let actual_index = self.scroll_offset + display_i;
            let item_y =
                results_y + (RESULT_ITEM_HEIGHT + RESULT_SPACING) * display_i as f32;
            let is_selected = actual_index == self.selected_index;

            // Render selection background
            if is_selected {
                sugarloaf.rounded_rect(
                    None,
                    input_x,
                    item_y,
                    input_width,
                    RESULT_ITEM_HEIGHT,
                    self.selected_background_color,
                    -0.4,
                    6.0,
                    0,
                );
            }

            // Render result text
            let result_id = self.result_text_ids[display_i];
            {
                use rio_backend::sugarloaf::SpanStyle;
                let content = sugarloaf.content();
                let mut builder = content.sel(result_id).clear().new_line();

                builder = builder.add_text(
                    &cmd.title,
                    SpanStyle {
                        color: self.text_color,
                        ..SpanStyle::default()
                    },
                );

                if let Some(desc) = &cmd.description {
                    builder = builder.add_text(
                        &format!(" — {}", desc),
                        SpanStyle {
                            color: self.description_color,
                            ..SpanStyle::default()
                        },
                    );
                }

                builder.build();

                let text_x = input_x + INPUT_PADDING_X;
                let text_y = item_y + (RESULT_ITEM_HEIGHT - RESULT_FONT_SIZE) / 2.0;
                sugarloaf.set_position(result_id, text_x, text_y);
                sugarloaf.set_visibility(result_id, true);
            }
        }

        // Render scrollbar if there are more items than visible
        if total_filtered > MAX_VISIBLE_RESULTS {
            let scrollbar_x = palette_x + palette_width
                - SCROLLBAR_WIDTH
                - SCROLLBAR_MARGIN
                - PALETTE_PADDING;
            let scrollbar_track_height =
                (RESULT_ITEM_HEIGHT + RESULT_SPACING) * MAX_VISIBLE_RESULTS as f32;

            // Calculate scrollbar thumb size and position
            let visible_ratio = MAX_VISIBLE_RESULTS as f32 / total_filtered as f32;
            let thumb_height =
                (scrollbar_track_height * visible_ratio).max(SCROLLBAR_MIN_HEIGHT);

            let max_scroll = total_filtered.saturating_sub(MAX_VISIBLE_RESULTS);
            let scroll_ratio = if max_scroll > 0 {
                self.scroll_offset as f32 / max_scroll as f32
            } else {
                0.0
            };
            let thumb_y =
                results_y + scroll_ratio * (scrollbar_track_height - thumb_height);

            // Render scrollbar thumb
            sugarloaf.rounded_rect(
                None,
                scrollbar_x,
                thumb_y,
                SCROLLBAR_WIDTH,
                thumb_height,
                [0.5, 0.5, 0.5, 0.6],  // Semi-transparent gray
                -0.5,                  // In front of everything else
                SCROLLBAR_WIDTH / 2.0, // Fully rounded ends
                0,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_palette() -> CommandPalette {
        CommandPalette {
            commands: vec![
                CommandPaletteItem {
                    title: "New Tab".to_string(),
                    description: Some("Create a new tab".to_string()),
                    action: "new_tab".to_string(),
                },
                CommandPaletteItem {
                    title: "Close Tab".to_string(),
                    description: Some("Close current tab".to_string()),
                    action: "close_tab".to_string(),
                },
                CommandPaletteItem {
                    title: "Split Horizontal".to_string(),
                    description: Some("Split horizontally".to_string()),
                    action: "split_h".to_string(),
                },
                CommandPaletteItem {
                    title: "Split Vertical".to_string(),
                    description: Some("Split vertically".to_string()),
                    action: "split_v".to_string(),
                },
                CommandPaletteItem {
                    title: "Settings".to_string(),
                    description: Some("Open settings".to_string()),
                    action: "settings".to_string(),
                },
                CommandPaletteItem {
                    title: "Theme".to_string(),
                    description: Some("Change theme".to_string()),
                    action: "theme".to_string(),
                },
                CommandPaletteItem {
                    title: "Help".to_string(),
                    description: Some("Show help".to_string()),
                    action: "help".to_string(),
                },
                CommandPaletteItem {
                    title: "About".to_string(),
                    description: Some("About Rio".to_string()),
                    action: "about".to_string(),
                },
                CommandPaletteItem {
                    title: "Quit".to_string(),
                    description: Some("Quit application".to_string()),
                    action: "quit".to_string(),
                },
            ],
            ..CommandPalette::default()
        }
    }

    #[test]
    fn test_toggle() {
        let mut palette = CommandPalette::new();
        assert!(!palette.is_enabled());

        palette.toggle();
        assert!(palette.is_enabled());

        palette.toggle();
        assert!(!palette.is_enabled());
    }

    #[test]
    fn test_set_enabled_resets_state() {
        let mut palette = CommandPalette::new();
        palette.set_query("test".to_string());
        palette.selected_index = 3;
        palette.scroll_offset = 2;

        palette.set_enabled(true);

        assert!(palette.query.is_empty());
        assert_eq!(palette.selected_index, 0);
        assert_eq!(palette.scroll_offset, 0);
    }

    #[test]
    fn test_filtered_commands_empty_query() {
        let palette = create_test_palette();
        let filtered = palette.filtered_commands();
        assert_eq!(filtered.len(), 9);
    }

    #[test]
    fn test_filtered_commands_by_title() {
        let mut palette = create_test_palette();
        palette.query = "split".to_string();
        let filtered = palette.filtered_commands();
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].title, "Split Horizontal");
        assert_eq!(filtered[1].title, "Split Vertical");
    }

    #[test]
    fn test_filtered_commands_by_description() {
        let mut palette = create_test_palette();
        palette.query = "open".to_string();
        let filtered = palette.filtered_commands();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].title, "Settings");
    }

    #[test]
    fn test_filtered_commands_case_insensitive() {
        let mut palette = create_test_palette();
        palette.query = "THEME".to_string();
        let filtered = palette.filtered_commands();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].title, "Theme");
    }

    #[test]
    fn test_set_query_resets_selection_and_scroll() {
        let mut palette = create_test_palette();
        palette.selected_index = 5;
        palette.scroll_offset = 3;

        palette.set_query("test".to_string());

        assert_eq!(palette.query, "test");
        assert_eq!(palette.selected_index, 0);
        assert_eq!(palette.scroll_offset, 0);
    }

    #[test]
    fn test_move_selection_down() {
        let mut palette = create_test_palette();
        palette.set_enabled(true);

        assert_eq!(palette.selected_index, 0);

        palette.move_selection_down();
        assert_eq!(palette.selected_index, 1);

        palette.move_selection_down();
        assert_eq!(palette.selected_index, 2);
    }

    #[test]
    fn test_move_selection_down_boundary() {
        let mut palette = create_test_palette();
        palette.set_enabled(true);
        palette.selected_index = 8; // Last item

        palette.move_selection_down();
        assert_eq!(palette.selected_index, 8); // Should not go beyond
    }

    #[test]
    fn test_move_selection_up() {
        let mut palette = create_test_palette();
        palette.set_enabled(true);
        palette.selected_index = 3;

        palette.move_selection_up();
        assert_eq!(palette.selected_index, 2);

        palette.move_selection_up();
        assert_eq!(palette.selected_index, 1);
    }

    #[test]
    fn test_move_selection_up_boundary() {
        let mut palette = create_test_palette();
        palette.set_enabled(true);
        palette.selected_index = 0;

        palette.move_selection_up();
        assert_eq!(palette.selected_index, 0); // Should not go below 0
    }

    #[test]
    fn test_scroll_offset_on_move_down() {
        let mut palette = create_test_palette();
        palette.set_enabled(true);

        // Move down until we hit the scroll threshold (MAX_VISIBLE_RESULTS = 8)
        for _ in 0..8 {
            palette.move_selection_down();
        }

        assert_eq!(palette.selected_index, 8);
        assert_eq!(palette.scroll_offset, 1); // Should have scrolled by 1
    }

    #[test]
    fn test_scroll_offset_on_move_up() {
        let mut palette = create_test_palette();
        palette.set_enabled(true);
        palette.selected_index = 1;
        palette.scroll_offset = 1;

        palette.move_selection_up();

        assert_eq!(palette.selected_index, 0);
        assert_eq!(palette.scroll_offset, 0); // Should scroll up when selection goes above visible area
    }

    #[test]
    fn test_get_selected_action() {
        let mut palette = create_test_palette();
        palette.selected_index = 0;
        assert_eq!(palette.get_selected_action(), Some("new_tab".to_string()));

        palette.selected_index = 2;
        assert_eq!(palette.get_selected_action(), Some("split_h".to_string()));
    }

    #[test]
    fn test_get_selected_action_with_filter() {
        let mut palette = create_test_palette();
        palette.set_query("split".to_string());
        palette.selected_index = 0;

        assert_eq!(palette.get_selected_action(), Some("split_h".to_string()));

        palette.selected_index = 1;
        assert_eq!(palette.get_selected_action(), Some("split_v".to_string()));
    }

    #[test]
    fn test_get_selected_action_out_of_bounds() {
        let mut palette = create_test_palette();
        palette.selected_index = 100;
        assert_eq!(palette.get_selected_action(), None);
    }

    #[test]
    fn test_height_when_disabled() {
        let palette = CommandPalette::new();
        assert_eq!(palette.height(), 0.0);
    }

    #[test]
    fn test_height_with_results() {
        let mut palette = create_test_palette();
        palette.set_enabled(true);

        let height = palette.height();
        // Should be: PALETTE_PADDING + INPUT_HEIGHT + PALETTE_PADDING + (8 results * height) + PALETTE_PADDING
        let expected_results_height = (RESULT_ITEM_HEIGHT + RESULT_SPACING) * 8.0;
        let expected = PALETTE_PADDING
            + INPUT_HEIGHT
            + PALETTE_PADDING
            + expected_results_height
            + PALETTE_PADDING;
        assert_eq!(height, expected);
    }

    #[test]
    fn test_height_with_filtered_results() {
        let mut palette = create_test_palette();
        palette.set_enabled(true);
        palette.set_query("split".to_string());

        let height = palette.height();
        // Height should remain constant even with filtered results
        let expected_results_height =
            (RESULT_ITEM_HEIGHT + RESULT_SPACING) * MAX_VISIBLE_RESULTS as f32;
        let expected = PALETTE_PADDING
            + INPUT_HEIGHT
            + PALETTE_PADDING
            + expected_results_height
            + PALETTE_PADDING;
        assert_eq!(height, expected);
    }
}
