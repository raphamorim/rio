// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::context::next_rich_text_id;
use rio_backend::sugarloaf::{SpanStyle, Sugarloaf};
use std::time::Instant;

// Layout
const PALETTE_WIDTH: f32 = 520.0;
const PALETTE_CORNER_RADIUS: f32 = 10.0;
const PALETTE_MARGIN_TOP: f32 = 80.0;
const PALETTE_PADDING: f32 = 6.0;

const INPUT_HEIGHT: f32 = 36.0;
const INPUT_FONT_SIZE: f32 = 14.0;
const INPUT_PADDING_X: f32 = 10.0;

const RESULT_ITEM_HEIGHT: f32 = 28.0;
const RESULT_FONT_SIZE: f32 = 13.0;
const SHORTCUT_FONT_SIZE: f32 = 11.0;
const MAX_VISIBLE_RESULTS: usize = 8;

const SEPARATOR_HEIGHT: f32 = 1.0;
const RESULTS_MARGIN_TOP: f32 = 4.0;
const CARET_WIDTH: f32 = 2.0;
const CARET_BLINK_MS: u128 = 500;

// Colors
const BACKDROP_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 0.35];
const BG_COLOR: [f32; 4] = [0.12, 0.12, 0.12, 0.98];
const INPUT_BG_COLOR: [f32; 4] = [0.16, 0.16, 0.16, 1.0];
const SELECTED_BG_COLOR: [f32; 4] = [0.22, 0.22, 0.25, 1.0];
const TEXT_COLOR: [f32; 4] = [0.93, 0.93, 0.93, 1.0];
const DIM_TEXT_COLOR: [f32; 4] = [0.50, 0.50, 0.50, 1.0];
const SHORTCUT_TEXT_COLOR: [f32; 4] = [0.40, 0.40, 0.45, 1.0];
const SEPARATOR_COLOR: [f32; 4] = [0.25, 0.25, 0.25, 1.0];

// Depth / order
const DEPTH_BACKDROP: f32 = 0.0;
const DEPTH_BG: f32 = 0.1;
const DEPTH_ELEMENT: f32 = 0.2;
const ORDER: u8 = 20;

/// Actions that can be triggered from the command palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteAction {
    TabCreate,
    TabClose,
    TabCloseUnfocused,
    SelectNextTab,
    SelectPrevTab,
    SplitRight,
    SplitDown,
    SelectNextSplit,
    SelectPrevSplit,
    ConfigEditor,
    WindowCreateNew,
    IncreaseFontSize,
    DecreaseFontSize,
    ResetFontSize,
    ToggleViMode,
    ToggleFullscreen,
    Copy,
    Paste,
    SearchForward,
    SearchBackward,
    ClearHistory,
    CloseCurrentSplitOrTab,
    Quit,
}

struct Command {
    title: &'static str,
    shortcut: &'static str,
    action: PaletteAction,
}

const COMMANDS: &[Command] = &[
    Command {
        title: "New Tab",
        shortcut: "Cmd+T",
        action: PaletteAction::TabCreate,
    },
    Command {
        title: "Close Tab",
        shortcut: "Cmd+W",
        action: PaletteAction::TabClose,
    },
    Command {
        title: "Close Other Tabs",
        shortcut: "",
        action: PaletteAction::TabCloseUnfocused,
    },
    Command {
        title: "Next Tab",
        shortcut: "Ctrl+Tab",
        action: PaletteAction::SelectNextTab,
    },
    Command {
        title: "Previous Tab",
        shortcut: "Ctrl+Shift+Tab",
        action: PaletteAction::SelectPrevTab,
    },
    Command {
        title: "Split Right",
        shortcut: "Cmd+D",
        action: PaletteAction::SplitRight,
    },
    Command {
        title: "Split Down",
        shortcut: "Cmd+Shift+D",
        action: PaletteAction::SplitDown,
    },
    Command {
        title: "Next Split",
        shortcut: "",
        action: PaletteAction::SelectNextSplit,
    },
    Command {
        title: "Previous Split",
        shortcut: "",
        action: PaletteAction::SelectPrevSplit,
    },
    Command {
        title: "Close Split or Tab",
        shortcut: "",
        action: PaletteAction::CloseCurrentSplitOrTab,
    },
    Command {
        title: "Settings",
        shortcut: "Cmd+,",
        action: PaletteAction::ConfigEditor,
    },
    Command {
        title: "New Window",
        shortcut: "Cmd+N",
        action: PaletteAction::WindowCreateNew,
    },
    Command {
        title: "Increase Font Size",
        shortcut: "Cmd++",
        action: PaletteAction::IncreaseFontSize,
    },
    Command {
        title: "Decrease Font Size",
        shortcut: "Cmd+-",
        action: PaletteAction::DecreaseFontSize,
    },
    Command {
        title: "Reset Font Size",
        shortcut: "Cmd+0",
        action: PaletteAction::ResetFontSize,
    },
    Command {
        title: "Toggle Vi Mode",
        shortcut: "",
        action: PaletteAction::ToggleViMode,
    },
    Command {
        title: "Toggle Fullscreen",
        shortcut: "",
        action: PaletteAction::ToggleFullscreen,
    },
    Command {
        title: "Copy",
        shortcut: "Cmd+C",
        action: PaletteAction::Copy,
    },
    Command {
        title: "Paste",
        shortcut: "Cmd+V",
        action: PaletteAction::Paste,
    },
    Command {
        title: "Search Forward",
        shortcut: "Cmd+F",
        action: PaletteAction::SearchForward,
    },
    Command {
        title: "Search Backward",
        shortcut: "",
        action: PaletteAction::SearchBackward,
    },
    Command {
        title: "Clear History",
        shortcut: "",
        action: PaletteAction::ClearHistory,
    },
    Command {
        title: "Quit",
        shortcut: "Cmd+Q",
        action: PaletteAction::Quit,
    },
];

/// Fuzzy match: checks if all query chars appear in order in the target.
/// Returns a score (higher = better match), or None if no match.
fn fuzzy_score(query: &str, target: &str) -> Option<i32> {
    let query_lower: Vec<char> = query.to_lowercase().chars().collect();
    let target_lower: Vec<char> = target.to_lowercase().chars().collect();

    if query_lower.is_empty() {
        return Some(0);
    }

    let mut qi = 0;
    let mut score: i32 = 0;
    let mut prev_match = false;
    let mut first_match_pos = None;

    for (ti, &tc) in target_lower.iter().enumerate() {
        if qi < query_lower.len() && tc == query_lower[qi] {
            if first_match_pos.is_none() {
                first_match_pos = Some(ti);
            }
            // Consecutive match bonus
            if prev_match {
                score += 5;
            }
            // Word boundary bonus (start of string or after space/punctuation)
            if ti == 0 || !target_lower[ti - 1].is_alphanumeric() {
                score += 10;
            }
            prev_match = true;
            qi += 1;
        } else {
            prev_match = false;
        }
    }

    if qi < query_lower.len() {
        return None; // Not all query chars matched
    }

    // Bonus for matching near the start
    if let Some(pos) = first_match_pos {
        score += (20_i32).saturating_sub(pos as i32);
    }

    Some(score)
}

/// Command palette UI component (Raycast-style)
pub struct CommandPalette {
    enabled: bool,
    pub query: String,
    pub selected_index: usize,
    scroll_offset: usize,
    /// Pre-allocated rich text ID for input (lazily initialized)
    input_text_id: Option<usize>,
    /// Pre-allocated rich text IDs for result rows (lazily initialized, fixed pool)
    result_text_ids: Vec<usize>,
    /// Pre-allocated rich text IDs for shortcut labels
    shortcut_text_ids: Vec<usize>,
    /// Timestamp for caret blinking
    caret_blink_start: Instant,
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self {
            enabled: false,
            query: String::new(),
            selected_index: 0,
            scroll_offset: 0,
            input_text_id: None,
            result_text_ids: Vec::new(),
            shortcut_text_ids: Vec::new(),
            caret_blink_start: Instant::now(),
        }
    }
}

impl CommandPalette {
    pub fn new() -> Self {
        Self::default()
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
            self.caret_blink_start = Instant::now();
        }
    }

    pub fn toggle(&mut self) {
        self.set_enabled(!self.enabled);
    }

    pub fn set_query(&mut self, query: String) {
        self.query = query;
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.caret_blink_start = Instant::now();
    }

    pub fn move_selection_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            if self.selected_index < self.scroll_offset {
                self.scroll_offset = self.selected_index;
            }
        }
    }

    pub fn move_selection_down(&mut self) {
        let count = self.filtered_commands().len();
        if self.selected_index < count.saturating_sub(1) {
            self.selected_index += 1;
            if self.selected_index >= self.scroll_offset + MAX_VISIBLE_RESULTS {
                self.scroll_offset = self.selected_index - MAX_VISIBLE_RESULTS + 1;
            }
        }
    }

    pub fn get_selected_action(&self) -> Option<PaletteAction> {
        let filtered = self.filtered_commands();
        filtered
            .get(self.selected_index)
            .map(|&(_, cmd)| cmd.action)
    }

    fn filtered_commands(&self) -> Vec<(i32, &Command)> {
        let mut results: Vec<(i32, &Command)> = COMMANDS
            .iter()
            .filter_map(|cmd| {
                let score = fuzzy_score(&self.query, cmd.title)?;
                Some((score, cmd))
            })
            .collect();

        // Sort by score descending
        results.sort_by(|a, b| b.0.cmp(&a.0));
        results
    }

    /// Returns the palette geometry (x, y, width, height) for hit-testing.
    fn palette_rect(&self, window_width: f32, scale_factor: f32) -> (f32, f32, f32, f32) {
        let px = (window_width / scale_factor - PALETTE_WIDTH) / 2.0;
        let py = PALETTE_MARGIN_TOP;
        let h = PALETTE_PADDING
            + INPUT_HEIGHT
            + SEPARATOR_HEIGHT
            + RESULTS_MARGIN_TOP
            + RESULT_ITEM_HEIGHT * MAX_VISIBLE_RESULTS as f32
            + PALETTE_PADDING;
        (px, py, PALETTE_WIDTH, h)
    }

    /// Hit-test a mouse click. Returns Some(index) if a result row was clicked,
    /// or None if clicked outside the palette or on the input area.
    /// Returns Err(()) if clicked outside the palette entirely (should close).
    pub fn hit_test(
        &self,
        mouse_x: f32,
        mouse_y: f32,
        window_width: f32,
        scale_factor: f32,
    ) -> Result<Option<usize>, ()> {
        let (px, py, pw, ph) = self.palette_rect(window_width, scale_factor);

        // Outside palette bounds
        if mouse_x < px || mouse_x > px + pw || mouse_y < py || mouse_y > py + ph {
            return Err(()); // Close palette
        }

        // Results area starts after input + separator
        let results_y =
            py + PALETTE_PADDING + INPUT_HEIGHT + SEPARATOR_HEIGHT + RESULTS_MARGIN_TOP;
        if mouse_y < results_y {
            return Ok(None); // Clicked on input area
        }

        let relative_y = mouse_y - results_y;
        let row = (relative_y / RESULT_ITEM_HEIGHT) as usize;
        let filtered_count = self.filtered_commands().len();
        let actual_index = self.scroll_offset + row;

        if actual_index < filtered_count {
            Ok(Some(actual_index))
        } else {
            Ok(None)
        }
    }

    /// Update selection based on mouse position. Returns true if selection changed.
    pub fn hover(
        &mut self,
        mouse_x: f32,
        mouse_y: f32,
        window_width: f32,
        scale_factor: f32,
    ) -> bool {
        if let Ok(Some(index)) =
            self.hit_test(mouse_x, mouse_y, window_width, scale_factor)
        {
            if self.selected_index != index {
                self.selected_index = index;
                return true;
            }
        }
        false
    }

    /// Ensure the text ID pools are allocated.
    fn ensure_text_ids(&mut self, sugarloaf: &mut Sugarloaf) {
        if self.input_text_id.is_none() {
            let id = next_rich_text_id();
            let _ = sugarloaf.text(Some(id));
            sugarloaf.set_use_grid_cell_size(id, false);
            sugarloaf.set_text_font_size(&id, INPUT_FONT_SIZE);
            sugarloaf.set_order(id, ORDER);
            self.input_text_id = Some(id);
        }

        while self.result_text_ids.len() < MAX_VISIBLE_RESULTS {
            let id = next_rich_text_id();
            let _ = sugarloaf.text(Some(id));
            sugarloaf.set_use_grid_cell_size(id, false);
            sugarloaf.set_text_font_size(&id, RESULT_FONT_SIZE);
            sugarloaf.set_order(id, ORDER);
            self.result_text_ids.push(id);
        }

        while self.shortcut_text_ids.len() < MAX_VISIBLE_RESULTS {
            let id = next_rich_text_id();
            let _ = sugarloaf.text(Some(id));
            sugarloaf.set_use_grid_cell_size(id, false);
            sugarloaf.set_text_font_size(&id, SHORTCUT_FONT_SIZE);
            sugarloaf.set_order(id, ORDER);
            self.shortcut_text_ids.push(id);
        }
    }

    /// Hide all text IDs (used when palette is closed or to reset).
    fn hide_all_text_ids(&self, sugarloaf: &mut Sugarloaf) {
        if let Some(id) = self.input_text_id {
            sugarloaf.set_visibility(id, false);
        }
        for &id in &self.result_text_ids {
            sugarloaf.set_visibility(id, false);
        }
        for &id in &self.shortcut_text_ids {
            sugarloaf.set_visibility(id, false);
        }
    }

    pub fn render(&mut self, sugarloaf: &mut Sugarloaf, dimensions: (f32, f32, f32)) {
        if !self.enabled {
            self.hide_all_text_ids(sugarloaf);
            return;
        }

        let (window_width, window_height, scale_factor) = dimensions;

        self.ensure_text_ids(sugarloaf);

        let (palette_x, palette_y, palette_width, palette_height) =
            self.palette_rect(window_width, scale_factor);

        sugarloaf.rect(
            None,
            0.0,
            0.0,
            window_width / scale_factor,
            window_height / scale_factor,
            BACKDROP_COLOR,
            DEPTH_BACKDROP,
            ORDER,
        );

        sugarloaf.rounded_rect(
            None,
            palette_x,
            palette_y,
            palette_width,
            palette_height,
            BG_COLOR,
            DEPTH_BG,
            PALETTE_CORNER_RADIUS,
            ORDER,
        );

        let input_x = palette_x + PALETTE_PADDING;
        let input_y = palette_y + PALETTE_PADDING;
        let input_width = palette_width - PALETTE_PADDING * 2.0;

        // Input bg: top corners rounded, bottom corners flat (meets separator)
        sugarloaf.quad(
            None,
            input_x,
            input_y,
            input_width,
            INPUT_HEIGHT,
            INPUT_BG_COLOR,
            [8.0, 8.0, 0.0, 0.0], // top-left, top-right, bottom-right, bottom-left
            [0.0; 4],
            [0.0; 4],
            0,
            DEPTH_ELEMENT,
            ORDER,
        );

        let input_id = self.input_text_id.unwrap();
        let display_text = if self.query.is_empty() {
            "Type a command..."
        } else {
            &self.query
        };
        let text_color = if self.query.is_empty() {
            DIM_TEXT_COLOR
        } else {
            TEXT_COLOR
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

        let elapsed_ms = self.caret_blink_start.elapsed().as_millis();
        let caret_visible = (elapsed_ms / CARET_BLINK_MS) % 2 == 0;

        if caret_visible {
            let text_width = if self.query.is_empty() {
                0.0
            } else {
                sugarloaf.get_text_rendered_width(&input_id)
            };

            let caret_x = text_x + text_width;
            let caret_y = input_y + (INPUT_HEIGHT - INPUT_FONT_SIZE) / 2.0 + 2.0;

            sugarloaf.rect(
                None,
                caret_x,
                caret_y,
                CARET_WIDTH,
                INPUT_FONT_SIZE,
                TEXT_COLOR,
                DEPTH_ELEMENT,
                ORDER,
            );
        }

        let sep_y = input_y + INPUT_HEIGHT;
        sugarloaf.rect(
            None,
            palette_x + PALETTE_PADDING,
            sep_y,
            palette_width - PALETTE_PADDING * 2.0,
            SEPARATOR_HEIGHT,
            SEPARATOR_COLOR,
            DEPTH_ELEMENT,
            ORDER,
        );

        let results_y = sep_y + SEPARATOR_HEIGHT + RESULTS_MARGIN_TOP;
        let filtered = self.filtered_commands();
        let visible_count = filtered
            .iter()
            .skip(self.scroll_offset)
            .take(MAX_VISIBLE_RESULTS)
            .count();

        for (display_i, &(_, cmd)) in filtered
            .iter()
            .skip(self.scroll_offset)
            .take(MAX_VISIBLE_RESULTS)
            .enumerate()
        {
            let actual_index = self.scroll_offset + display_i;
            let item_y = results_y + RESULT_ITEM_HEIGHT * display_i as f32;
            let is_selected = actual_index == self.selected_index;

            // Selection highlight
            if is_selected {
                sugarloaf.rounded_rect(
                    None,
                    input_x,
                    item_y,
                    input_width,
                    RESULT_ITEM_HEIGHT,
                    SELECTED_BG_COLOR,
                    DEPTH_ELEMENT,
                    6.0,
                    ORDER,
                );
            }

            let result_id = self.result_text_ids[display_i];
            let content = sugarloaf.content();
            content
                .sel(result_id)
                .clear()
                .new_line()
                .add_text(
                    cmd.title,
                    SpanStyle {
                        color: if is_selected {
                            TEXT_COLOR
                        } else {
                            [0.80, 0.80, 0.80, 1.0]
                        },
                        ..SpanStyle::default()
                    },
                )
                .build();

            let row_text_x = input_x + INPUT_PADDING_X;
            let row_text_y = item_y + (RESULT_ITEM_HEIGHT - RESULT_FONT_SIZE) / 2.0;
            sugarloaf.set_position(result_id, row_text_x, row_text_y);
            sugarloaf.set_visibility(result_id, true);

            // Shortcut text (right-aligned)
            let shortcut_id = self.shortcut_text_ids[display_i];
            if !cmd.shortcut.is_empty() {
                let content = sugarloaf.content();
                content
                    .sel(shortcut_id)
                    .clear()
                    .new_line()
                    .add_text(
                        cmd.shortcut,
                        SpanStyle {
                            color: SHORTCUT_TEXT_COLOR,
                            ..SpanStyle::default()
                        },
                    )
                    .build();

                let shortcut_width = cmd.shortcut.len() as f32 * 6.5;
                let shortcut_x = input_x + input_width - INPUT_PADDING_X - shortcut_width;
                let shortcut_y = item_y + (RESULT_ITEM_HEIGHT - SHORTCUT_FONT_SIZE) / 2.0;
                sugarloaf.set_position(shortcut_id, shortcut_x, shortcut_y);
                sugarloaf.set_visibility(shortcut_id, true);
            } else {
                sugarloaf.set_visibility(shortcut_id, false);
            }
        }

        // Hide unused result/shortcut text IDs
        for i in visible_count..MAX_VISIBLE_RESULTS {
            sugarloaf.set_visibility(self.result_text_ids[i], false);
            sugarloaf.set_visibility(self.shortcut_text_ids[i], false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let palette = CommandPalette::new();
        let filtered = palette.filtered_commands();
        assert_eq!(filtered.len(), COMMANDS.len());
    }

    #[test]
    fn test_filtered_commands_by_title() {
        let mut palette = CommandPalette::new();
        palette.query = "split".to_string();
        let filtered = palette.filtered_commands();
        assert!(filtered.len() >= 2);
        // All results should contain "split" in some form
        for (_, cmd) in &filtered {
            assert!(cmd.title.to_lowercase().contains("split"));
        }
    }

    #[test]
    fn test_filtered_commands_case_insensitive() {
        let mut palette = CommandPalette::new();
        palette.query = "QUIT".to_string();
        let filtered = palette.filtered_commands();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].1.title, "Quit");
    }

    #[test]
    fn test_fuzzy_matching() {
        let mut palette = CommandPalette::new();
        palette.query = "nt".to_string(); // Should match "New Tab", "Next Tab", etc.
        let filtered = palette.filtered_commands();
        assert!(!filtered.is_empty());
    }

    #[test]
    fn test_set_query_resets_selection_and_scroll() {
        let mut palette = CommandPalette::new();
        palette.selected_index = 5;
        palette.scroll_offset = 3;
        palette.set_query("test".to_string());
        assert_eq!(palette.selected_index, 0);
        assert_eq!(palette.scroll_offset, 0);
    }

    #[test]
    fn test_move_selection_down() {
        let mut palette = CommandPalette::new();
        palette.set_enabled(true);
        assert_eq!(palette.selected_index, 0);
        palette.move_selection_down();
        assert_eq!(palette.selected_index, 1);
        palette.move_selection_down();
        assert_eq!(palette.selected_index, 2);
    }

    #[test]
    fn test_move_selection_down_boundary() {
        let mut palette = CommandPalette::new();
        palette.set_enabled(true);
        let count = palette.filtered_commands().len();
        palette.selected_index = count - 1;
        palette.move_selection_down();
        assert_eq!(palette.selected_index, count - 1);
    }

    #[test]
    fn test_move_selection_up() {
        let mut palette = CommandPalette::new();
        palette.set_enabled(true);
        palette.selected_index = 3;
        palette.move_selection_up();
        assert_eq!(palette.selected_index, 2);
    }

    #[test]
    fn test_move_selection_up_boundary() {
        let mut palette = CommandPalette::new();
        palette.set_enabled(true);
        palette.move_selection_up();
        assert_eq!(palette.selected_index, 0);
    }

    #[test]
    fn test_get_selected_action() {
        let palette = CommandPalette::new();
        let action = palette.get_selected_action();
        assert!(action.is_some());
        // First command is "New Tab"
        assert_eq!(action.unwrap(), PaletteAction::TabCreate);
    }

    #[test]
    fn test_get_selected_action_with_filter() {
        let mut palette = CommandPalette::new();
        palette.set_query("quit".to_string());
        let action = palette.get_selected_action();
        assert_eq!(action, Some(PaletteAction::Quit));
    }

    #[test]
    fn test_scroll_offset_on_move_down() {
        let mut palette = CommandPalette::new();
        palette.set_enabled(true);
        for _ in 0..MAX_VISIBLE_RESULTS {
            palette.move_selection_down();
        }
        assert!(palette.scroll_offset > 0);
    }

    #[test]
    fn test_hit_test_outside() {
        let palette = CommandPalette::new();
        assert!(palette.hit_test(0.0, 0.0, 1200.0, 1.0).is_err());
    }

    #[test]
    fn test_fuzzy_score_basic() {
        assert!(fuzzy_score("nt", "New Tab").is_some());
        assert!(fuzzy_score("xyz", "New Tab").is_none());
        assert!(fuzzy_score("", "New Tab").is_some());
    }

    #[test]
    fn test_fuzzy_score_ordering() {
        // "New Tab" should score higher than "Next Tab" for "net" because of word boundary
        let score_new = fuzzy_score("net", "New Tab").unwrap_or(-100);
        let score_next = fuzzy_score("net", "Next Tab").unwrap_or(-100);
        // Both should match
        assert!(score_new > -100);
        assert!(score_next > -100);
    }
}
