// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::context::next_rich_text_id;
use crate::renderer::utils::add_span_with_fallback;
use rio_backend::sugarloaf::{SpanStyle, Sugarloaf};
use std::time::Instant;

// Layout
const PALETTE_WIDTH: f32 = 480.0;
const PALETTE_CORNER_RADIUS: f32 = 8.0;
const PALETTE_MARGIN_TOP: f32 = 80.0;
const PALETTE_PADDING: f32 = 4.0;

const INPUT_HEIGHT: f32 = 40.0;
const INPUT_FONT_SIZE: f32 = 14.0;
const INPUT_PADDING_X: f32 = 14.0;

const RESULT_ITEM_HEIGHT: f32 = 32.0;
const RESULT_FONT_SIZE: f32 = 13.0;
const SHORTCUT_FONT_SIZE: f32 = 11.0;
const MAX_VISIBLE_RESULTS: usize = 8;

// Copy icon (two overlapping page outlines with rounded corners,
// drawn by layering filled + cutout rounded rects). Sized to fit
// comfortably inside RESULT_ITEM_HEIGHT.
const COPY_ICON_PAGE_W: f32 = 10.0;
const COPY_ICON_PAGE_H: f32 = 12.0;
const COPY_ICON_OFFSET: f32 = 3.0;
const COPY_ICON_STROKE: f32 = 1.0;
const COPY_ICON_RADIUS: f32 = 2.0;
const COPY_ICON_W: f32 = COPY_ICON_PAGE_W + COPY_ICON_OFFSET; // 13
const COPY_ICON_H: f32 = COPY_ICON_PAGE_H + COPY_ICON_OFFSET; // 15

const SEPARATOR_HEIGHT: f32 = 1.0;
const RESULTS_MARGIN_TOP: f32 = 2.0;
const CARET_WIDTH: f32 = 1.5;
const CARET_BLINK_MS: u128 = 500;

// Colors — dark minimalist
const BACKDROP_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 0.50];
const BG_COLOR: [f32; 4] = [0.08, 0.08, 0.08, 0.98];
const SELECTED_BG_COLOR: [f32; 4] = [0.15, 0.15, 0.15, 1.0];
const TEXT_COLOR: [f32; 4] = [0.85, 0.85, 0.85, 1.0];
const DIM_TEXT_COLOR: [f32; 4] = [0.35, 0.35, 0.35, 1.0];
const SHORTCUT_TEXT_COLOR: [f32; 4] = [0.30, 0.30, 0.32, 1.0];
const SEPARATOR_COLOR: [f32; 4] = [0.15, 0.15, 0.15, 1.0];

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
    ToggleAppearanceTheme,
    Copy,
    Paste,
    SearchForward,
    SearchBackward,
    ClearHistory,
    CloseCurrentSplitOrTab,
    /// Browse the family names of every registered font. Does NOT
    /// execute a one-shot action — the palette stays open with the
    /// font list as its contents. Handled by `router`, not
    /// `Screen::execute_palette_action`.
    ListFonts,
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
        title: "Toggle Appearance Theme",
        shortcut: "",
        action: PaletteAction::ToggleAppearanceTheme,
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
        title: "List Fonts",
        shortcut: "",
        action: PaletteAction::ListFonts,
    },
    Command {
        title: "Quit",
        shortcut: "Cmd+Q",
        action: PaletteAction::Quit,
    },
];

/// What the palette is currently browsing and filtering over.
///
/// `Commands` is the default — fuzzy-matches against the static
/// `COMMANDS` list and dispatches a `PaletteAction` on Enter.
///
/// `Fonts` is entered via the `ListFonts` command. The palette stays
/// open, its content is replaced with the owned list of font family
/// names, and Enter closes the palette (no font-switching action yet).
/// The list is owned so the filter pass doesn't keep a borrow on the
/// sugarloaf FontLibrary.
enum PaletteMode {
    Commands,
    Fonts(Vec<String>),
}

/// One row in the filtered result list. Variants carry exactly the
/// data the render pass needs — no `&'static Command` vs `&str`
/// lifetime mixing.
enum PaletteRow<'a> {
    Command {
        title: &'a str,
        shortcut: &'a str,
        action: PaletteAction,
    },
    Font {
        family: &'a str,
    },
}

impl<'a> PaletteRow<'a> {
    fn title(&self) -> &'a str {
        match *self {
            PaletteRow::Command { title, .. } => title,
            PaletteRow::Font { family } => family,
        }
    }

    fn shortcut(&self) -> &'a str {
        match *self {
            PaletteRow::Command { shortcut, .. } => shortcut,
            PaletteRow::Font { .. } => "",
        }
    }

    fn action(&self) -> Option<PaletteAction> {
        match *self {
            PaletteRow::Command { action, .. } => Some(action),
            PaletteRow::Font { .. } => None,
        }
    }
}

/// Paint a rounded-rect outline by layering two filled rounded rects:
/// the outer one in `stroke_color`, then a smaller one in `fill_color`
/// inset by `stroke` on all sides to carve out the interior. Sugarloaf
/// has no stroked-rect primitive, so this is how we get a 1px border
/// effect. Nine params is the irreducible minimum here — grouping them
/// into a struct would just shuffle the same fields.
#[allow(clippy::too_many_arguments)]
fn stroke_rounded_rect(
    sugarloaf: &mut Sugarloaf,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    stroke: f32,
    radius: f32,
    stroke_color: [f32; 4],
    fill_color: [f32; 4],
    depth: f32,
    order: u8,
) {
    sugarloaf.rounded_rect(
        None,
        x,
        y,
        width,
        height,
        stroke_color,
        depth,
        radius,
        order,
    );
    let inner_radius = (radius - stroke).max(0.0);
    // Inset fill carves out the interior. Painted slightly deeper so
    // it lands on top of the outer rect.
    sugarloaf.rounded_rect(
        None,
        x + stroke,
        y + stroke,
        (width - stroke * 2.0).max(0.0),
        (height - stroke * 2.0).max(0.0),
        fill_color,
        depth + 0.001,
        inner_radius,
        order,
    );
}

/// Paint a "copy" icon (two overlapping rounded page outlines)
/// anchored at `(x, y)`. Drawn from rects only — no font glyph
/// dependency — so it renders consistently regardless of what the
/// user's font stack can produce for ⎘ / 📋 / similar.
///
/// `row_fill_color` is the background behind the icon (palette BG when
/// the row is idle, selection highlight when hovered/selected); it's
/// used to cut out the page interiors so the outlines read as a
/// proper border rather than two solid blobs. Back page painted
/// slightly below the front via depth so the front's cutout
/// correctly hides the overlapping portion of the back's stroke.
#[allow(clippy::too_many_arguments)]
fn draw_copy_icon(
    sugarloaf: &mut Sugarloaf,
    x: f32,
    y: f32,
    stroke_color: [f32; 4],
    row_fill_color: [f32; 4],
    depth: f32,
    order: u8,
) {
    // Back page (upper-left).
    stroke_rounded_rect(
        sugarloaf,
        x,
        y,
        COPY_ICON_PAGE_W,
        COPY_ICON_PAGE_H,
        COPY_ICON_STROKE,
        COPY_ICON_RADIUS,
        stroke_color,
        row_fill_color,
        depth,
        order,
    );
    // Front page (offset down-right), painted above the back so its
    // cutout hides the back's overlapping interior.
    stroke_rounded_rect(
        sugarloaf,
        x + COPY_ICON_OFFSET,
        y + COPY_ICON_OFFSET,
        COPY_ICON_PAGE_W,
        COPY_ICON_PAGE_H,
        COPY_ICON_STROKE,
        COPY_ICON_RADIUS,
        stroke_color,
        row_fill_color,
        depth + 0.01,
        order,
    );
}

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
    pub has_adaptive_theme: bool,
    /// Which list the palette is showing (commands or fonts).
    mode: PaletteMode,
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
            has_adaptive_theme: false,
            mode: PaletteMode::Commands,
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
            // Always re-open into Commands mode — a stale Fonts list
            // from a previous session would be misleading (fonts may
            // have changed) and surprising (user toggles palette and
            // finds themselves on the font list).
            self.mode = PaletteMode::Commands;
        }
    }

    pub fn toggle(&mut self) {
        self.set_enabled(!self.enabled);
    }

    /// Swap the palette into font-browsing mode with the given family
    /// list. Clears the query so the full list is visible, keeps the
    /// palette open. Called by the router after the user picks the
    /// `List Fonts` command.
    pub fn enter_fonts_mode(&mut self, fonts: Vec<String>) {
        self.mode = PaletteMode::Fonts(fonts);
        self.query.clear();
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.caret_blink_start = Instant::now();
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
        let count = self.filtered_rows().len();
        if self.selected_index < count.saturating_sub(1) {
            self.selected_index += 1;
            if self.selected_index >= self.scroll_offset + MAX_VISIBLE_RESULTS {
                self.scroll_offset = self.selected_index - MAX_VISIBLE_RESULTS + 1;
            }
        }
    }

    pub fn get_selected_action(&self) -> Option<PaletteAction> {
        self.filtered_rows()
            .get(self.selected_index)
            .and_then(|(_, row)| row.action())
    }

    /// Selected family name if (and only if) the palette is in fonts
    /// mode and the selection points at a valid row. Owned `String`
    /// so the caller can mutate the palette state (`set_enabled`) in
    /// the same statement without fighting the borrow checker.
    pub fn get_selected_font(&self) -> Option<String> {
        self.filtered_rows().get(self.selected_index).and_then(
            |(_, row)| match row {
                PaletteRow::Font { family } => Some((*family).to_owned()),
                PaletteRow::Command { .. } => None,
            },
        )
    }

    /// Filtered list of rows for the current mode. Both modes share
    /// the same fuzzy-score + sort pipeline so typing behaves
    /// identically in either view.
    fn filtered_rows(&self) -> Vec<(i32, PaletteRow<'_>)> {
        let mut results: Vec<(i32, PaletteRow<'_>)> = match &self.mode {
            PaletteMode::Commands => {
                let has_adaptive = self.has_adaptive_theme;
                COMMANDS
                    .iter()
                    .filter(|cmd| {
                        if cmd.action == PaletteAction::ToggleAppearanceTheme {
                            return has_adaptive;
                        }
                        true
                    })
                    .filter_map(|cmd| {
                        let score = fuzzy_score(&self.query, cmd.title)?;
                        Some((
                            score,
                            PaletteRow::Command {
                                title: cmd.title,
                                shortcut: cmd.shortcut,
                                action: cmd.action,
                            },
                        ))
                    })
                    .collect()
            }
            PaletteMode::Fonts(fonts) => fonts
                .iter()
                .filter_map(|family| {
                    let score = fuzzy_score(&self.query, family)?;
                    Some((score, PaletteRow::Font { family }))
                })
                .collect(),
        };

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
        let filtered_count = self.filtered_rows().len();
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

        // No separate input background — blends with palette bg for minimalism

        let input_id = self.input_text_id.unwrap();
        let placeholder = match self.mode {
            PaletteMode::Commands => "Type a command...",
            PaletteMode::Fonts(_) => "Type a font name...",
        };
        let display_text = if self.query.is_empty() {
            placeholder
        } else {
            &self.query
        };
        let text_color = if self.query.is_empty() {
            DIM_TEXT_COLOR
        } else {
            TEXT_COLOR
        };

        let input_style = SpanStyle {
            color: text_color,
            ..SpanStyle::default()
        };
        sugarloaf.content().sel(input_id).clear().new_line();
        add_span_with_fallback(sugarloaf, display_text, input_style);
        sugarloaf.content().build();

        let text_x = input_x + INPUT_PADDING_X;
        let text_y = input_y + (INPUT_HEIGHT - INPUT_FONT_SIZE) / 2.0;
        sugarloaf.set_position(input_id, text_x, text_y);
        sugarloaf.set_visibility(input_id, true);

        let elapsed_ms = self.caret_blink_start.elapsed().as_millis();
        let caret_visible = (elapsed_ms / CARET_BLINK_MS).is_multiple_of(2);

        if caret_visible {
            let text_width = if self.query.is_empty() {
                0.0
            } else {
                sugarloaf.get_text_rendered_width(&input_id)
            };

            let caret_x = text_x + text_width;
            let caret_height = INPUT_FONT_SIZE + 4.0;
            let caret_y = input_y + (INPUT_HEIGHT - caret_height) / 2.0 + 2.0;

            sugarloaf.rect(
                None,
                caret_x,
                caret_y,
                CARET_WIDTH,
                caret_height,
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
        let filtered = self.filtered_rows();
        let visible_count = filtered
            .iter()
            .skip(self.scroll_offset)
            .take(MAX_VISIBLE_RESULTS)
            .count();

        for (display_i, (_, row)) in filtered
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
                    4.0,
                    ORDER,
                );
            }

            let result_id = self.result_text_ids[display_i];
            let result_style = SpanStyle {
                color: if is_selected {
                    TEXT_COLOR
                } else {
                    [0.55, 0.55, 0.55, 1.0]
                },
                ..SpanStyle::default()
            };
            sugarloaf.content().sel(result_id).clear().new_line();
            add_span_with_fallback(sugarloaf, row.title(), result_style);
            sugarloaf.content().build();

            let row_text_x = input_x + INPUT_PADDING_X;
            let row_text_y = item_y + (RESULT_ITEM_HEIGHT - RESULT_FONT_SIZE) / 2.0;
            sugarloaf.set_position(result_id, row_text_x, row_text_y);
            sugarloaf.set_visibility(result_id, true);

            // Right-side hint: shortcut for commands, copy icon for
            // font rows (signals "Enter copies this to clipboard").
            let shortcut_id = self.shortcut_text_ids[display_i];
            let shortcut = row.shortcut();
            let is_font_row = matches!(row, PaletteRow::Font { .. });
            if !shortcut.is_empty() {
                let shortcut_style = SpanStyle {
                    color: SHORTCUT_TEXT_COLOR,
                    ..SpanStyle::default()
                };
                sugarloaf.content().sel(shortcut_id).clear().new_line();
                add_span_with_fallback(sugarloaf, shortcut, shortcut_style);
                sugarloaf.content().build();

                let shortcut_width = shortcut.len() as f32 * 6.5;
                let shortcut_x = input_x + input_width - INPUT_PADDING_X - shortcut_width;
                let shortcut_y = item_y + (RESULT_ITEM_HEIGHT - SHORTCUT_FONT_SIZE) / 2.0;
                sugarloaf.set_position(shortcut_id, shortcut_x, shortcut_y);
                sugarloaf.set_visibility(shortcut_id, true);
            } else {
                sugarloaf.set_visibility(shortcut_id, false);
            }

            if is_font_row {
                let stroke_color = if is_selected {
                    TEXT_COLOR
                } else {
                    SHORTCUT_TEXT_COLOR
                };
                // Cutout inside each page uses the row's own background
                // so the border reads as a clean outline on either
                // palette-bg (idle) or selection-highlight-bg (hovered).
                let row_fill_color = if is_selected {
                    SELECTED_BG_COLOR
                } else {
                    BG_COLOR
                };
                let icon_x = input_x + input_width - INPUT_PADDING_X - COPY_ICON_W;
                let icon_y = item_y + (RESULT_ITEM_HEIGHT - COPY_ICON_H) / 2.0;
                draw_copy_icon(
                    sugarloaf,
                    icon_x,
                    icon_y,
                    stroke_color,
                    row_fill_color,
                    DEPTH_ELEMENT,
                    ORDER,
                );
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
        let filtered = palette.filtered_rows();
        // ToggleAppearanceTheme is hidden when has_adaptive_theme is false
        assert_eq!(filtered.len(), COMMANDS.len() - 1);
    }

    #[test]
    fn test_filtered_commands_by_title() {
        let mut palette = CommandPalette::new();
        palette.query = "split".to_string();
        let filtered = palette.filtered_rows();
        assert!(filtered.len() >= 2);
        for (_, row) in &filtered {
            assert!(row.title().to_lowercase().contains("split"));
        }
    }

    #[test]
    fn test_filtered_commands_case_insensitive() {
        let mut palette = CommandPalette::new();
        palette.query = "QUIT".to_string();
        let filtered = palette.filtered_rows();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].1.title(), "Quit");
    }

    #[test]
    fn test_fuzzy_matching() {
        let mut palette = CommandPalette::new();
        palette.query = "nt".to_string(); // Should match "New Tab", "Next Tab", etc.
        let filtered = palette.filtered_rows();
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
        let count = palette.filtered_rows().len();
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

    // --- Fonts-mode tests ------------------------------------------------

    #[test]
    fn enter_fonts_mode_switches_to_font_list() {
        let mut palette = CommandPalette::new();
        palette.set_enabled(true);
        palette.set_query("ab".to_string());
        palette.selected_index = 2;

        let fonts = vec![
            "JetBrains Mono".to_string(),
            "Fira Code".to_string(),
            "Cascadia Code".to_string(),
        ];
        palette.enter_fonts_mode(fonts);

        // Query cleared, selection reset, full list visible.
        assert!(palette.query.is_empty());
        assert_eq!(palette.selected_index, 0);
        assert_eq!(palette.filtered_rows().len(), 3);
        // Every row is a Font row, so no executable action.
        assert!(palette.get_selected_action().is_none());
    }

    #[test]
    fn fonts_mode_filters_by_fuzzy_score() {
        let mut palette = CommandPalette::new();
        palette.enter_fonts_mode(vec![
            "JetBrains Mono".to_string(),
            "Fira Code".to_string(),
            "Cascadia Code".to_string(),
        ]);
        palette.set_query("cas".to_string());
        let filtered = palette.filtered_rows();
        assert!(filtered.iter().any(|(_, r)| r.title() == "Cascadia Code"));
        assert!(filtered.iter().all(|(_, r)| {
            r.title().to_lowercase().contains('c')
                && r.title().to_lowercase().contains('a')
                && r.title().to_lowercase().contains('s')
        }));
    }

    #[test]
    fn fonts_mode_row_has_no_shortcut_column() {
        let mut palette = CommandPalette::new();
        palette.enter_fonts_mode(vec!["Fira Code".to_string()]);
        let filtered = palette.filtered_rows();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].1.shortcut(), "");
    }

    #[test]
    fn set_enabled_resets_fonts_mode_to_commands() {
        // Re-opening the palette with the keyboard must drop any stale
        // font list — reopening otherwise would land the user on fonts
        // they saw yesterday, which is surprising.
        let mut palette = CommandPalette::new();
        palette.enter_fonts_mode(vec!["Fira Code".to_string()]);
        palette.enabled = true;
        palette.set_enabled(false);
        palette.set_enabled(true);
        assert!(matches!(palette.mode, PaletteMode::Commands));
        // Commands list is back (non-empty modulo adaptive-theme filter).
        assert!(!palette.filtered_rows().is_empty());
    }

    #[test]
    fn get_selected_font_returns_family_in_fonts_mode() {
        let mut palette = CommandPalette::new();
        palette.enter_fonts_mode(vec![
            "JetBrains Mono".to_string(),
            "Fira Code".to_string(),
        ]);
        // First row (sorted alphabetically by fuzzy_score tie-break:
        // both score 0 with empty query, so first-inserted wins).
        let selected = palette.get_selected_font();
        assert!(selected.is_some());
        // The returned name must be one of the inputs, irrespective
        // of fuzzy-sort ordering.
        let s = selected.unwrap();
        assert!(s == "JetBrains Mono" || s == "Fira Code");
    }

    #[test]
    fn get_selected_font_none_in_commands_mode() {
        let palette = CommandPalette::new();
        // Default mode is Commands; no font to copy.
        assert!(palette.get_selected_font().is_none());
    }

    #[test]
    fn get_selected_font_none_when_empty_filter() {
        let mut palette = CommandPalette::new();
        palette.enter_fonts_mode(vec!["Fira Code".to_string()]);
        palette.set_query("zzzz".to_string());
        // Query doesn't match anything → no selected font.
        assert!(palette.get_selected_font().is_none());
    }

    #[test]
    fn list_fonts_command_is_present_and_actionable() {
        // Confirms `List Fonts` shows up in the command list and
        // reports the correct action when selected.
        let mut palette = CommandPalette::new();
        palette.set_query("list fonts".to_string());
        let filtered = palette.filtered_rows();
        assert!(!filtered.is_empty());
        assert_eq!(filtered[0].1.title(), "List Fonts");
        palette.selected_index = 0;
        assert_eq!(
            palette.get_selected_action(),
            Some(PaletteAction::ListFonts)
        );
    }
}
