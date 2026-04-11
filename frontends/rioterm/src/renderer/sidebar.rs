// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::context::{next_rich_text_id, ContextManager};
use crate::renderer::utils::add_span_with_fallback;
use rio_backend::event::EventProxy;
use rio_backend::sugarloaf::{SpanStyle, Sugarloaf};
use rustc_hash::FxHashMap;

/// Height of each tab row in pixels
const TAB_ROW_HEIGHT: f32 = 28.0;

/// Height of each panel sub-item row
const PANEL_ROW_HEIGHT: f32 = 24.0;

/// Left padding for tab titles
const TAB_PADDING_LEFT: f32 = 12.0;

/// Extra left padding for panel sub-items
const PANEL_INDENT: f32 = 16.0;

/// Height of the collapse button area at the top
#[cfg(not(target_os = "macos"))]
const HEADER_HEIGHT: f32 = 38.0;

/// Font size for tab titles
const TAB_FONT_SIZE: f32 = 12.0;

/// Font size for panel sub-items
const PANEL_FONT_SIZE: f32 = 11.0;

/// Right padding before the sidebar border
#[allow(dead_code)]
const RIGHT_PADDING: f32 = 8.0;

/// Collapse button size
const COLLAPSE_BUTTON_SIZE: f32 = 16.0;

/// Width of the sidebar when collapsed (icon strip).
/// On macOS the sidebar fully collapses; on Linux/Windows it keeps
/// a narrow strip with just the toggle icon.
#[cfg(not(target_os = "macos"))]
const COLLAPSED_WIDTH: f32 = 36.0;

/// Sidebar border width
const BORDER_WIDTH: f32 = 0.5;

/// Scrollbar width
const SCROLLBAR_WIDTH: f32 = 4.0;

/// Scrollbar margin from edges
const SCROLLBAR_MARGIN: f32 = 2.0;

/// Minimum scrollbar thumb height
const MIN_THUMB_HEIGHT: f32 = 20.0;

/// Active tab left accent bar width
const ACCENT_BAR_WIDTH: f32 = 3.0;

/// macOS traffic light reserved height
#[cfg(target_os = "macos")]
const MACOS_TRAFFIC_LIGHT_HEIGHT: f32 = 38.0;

/// Active tab highlight — slightly brighter than the sidebar bg
const DEFAULT_ACTIVE_BG: [f32; 4] = [1.0, 1.0, 1.0, 0.15];

/// Hover highlight
const HOVER_BG: [f32; 4] = [1.0, 1.0, 1.0, 0.12];

struct TabSidebarData {
    text_id: usize,
    panel_text_ids: Vec<usize>,
}

pub struct Sidebar {
    pub width: f32,
    pub collapsed: bool,
    pub hide_if_single: bool,
    pub inactive_text_color: [f32; 4],
    pub active_text_color: [f32; 4],
    pub border_color: [f32; 4],
    bg_color: [f32; 4],
    tab_data: FxHashMap<usize, TabSidebarData>,
    scroll_offset: f32,
    pub hovered: SidebarHit,
}

#[allow(dead_code)]
impl Sidebar {
    pub fn new(
        width: f32,
        inactive_text_color: [f32; 4],
        active_text_color: [f32; 4],
        border_color: [f32; 4],
        background_color: [f32; 4],
        hide_if_single: bool,
    ) -> Self {
        // Slightly darken the terminal background for the sidebar
        let bg_color = [
            background_color[0] * 0.85,
            background_color[1] * 0.85,
            background_color[2] * 0.85,
            1.0,
        ];
        Self {
            width,
            collapsed: false,
            hide_if_single,
            inactive_text_color,
            active_text_color,
            border_color,
            bg_color,
            tab_data: FxHashMap::default(),
            scroll_offset: 0.0,
            hovered: SidebarHit::None,
        }
    }

    pub fn update_colors(
        &mut self,
        inactive_text_color: [f32; 4],
        active_text_color: [f32; 4],
        border_color: [f32; 4],
        background_color: [f32; 4],
    ) {
        self.inactive_text_color = inactive_text_color;
        self.active_text_color = active_text_color;
        self.border_color = border_color;
        self.bg_color = [
            background_color[0] * 0.85,
            background_color[1] * 0.85,
            background_color[2] * 0.85,
            1.0,
        ];
    }

    /// Toggle collapsed state
    pub fn toggle_collapsed(&mut self) {
        self.collapsed = !self.collapsed;
    }

    /// Update hover state from mouse position (logical coords).
    /// Returns true if the hover changed (needs redraw).
    pub fn update_hover(
        &mut self,
        mouse_x: f32,
        mouse_y: f32,
        context_manager: &ContextManager<EventProxy>,
    ) -> bool {
        let new_hover = if mouse_x <= self.interactive_width() {
            self.hit_test(mouse_x, mouse_y, context_manager)
        } else {
            SidebarHit::None
        };
        if new_hover != self.hovered {
            println!(
                "sidebar hover: {:?} -> {:?} (mouse {:.0},{:.0})",
                self.hovered, new_hover, mouse_x, mouse_y
            );
            self.hovered = new_hover;
            true
        } else {
            false
        }
    }

    /// The effective width used for terminal content offset.
    #[inline]
    pub fn effective_width(&self) -> f32 {
        if self.collapsed {
            #[cfg(target_os = "macos")]
            {
                0.0
            }
            #[cfg(not(target_os = "macos"))]
            {
                COLLAPSED_WIDTH
            }
        } else {
            self.width
        }
    }

    /// The clickable/hoverable width — includes the icon area
    /// even when collapsed on macOS.
    #[inline]
    pub fn interactive_width(&self) -> f32 {
        if self.collapsed {
            #[cfg(target_os = "macos")]
            {
                // Icon at x=76, size=16, pad=6 → need ~98px
                76.0 + COLLAPSE_BUTTON_SIZE + 6.0
            }
            #[cfg(not(target_os = "macos"))]
            {
                COLLAPSED_WIDTH
            }
        } else {
            self.width
        }
    }

    /// Top offset for the tab list content (below header/traffic lights)
    #[inline]
    fn content_top(&self) -> f32 {
        #[cfg(target_os = "macos")]
        {
            MACOS_TRAFFIC_LIGHT_HEIGHT
        }
        #[cfg(not(target_os = "macos"))]
        {
            HEADER_HEIGHT
        }
    }

    /// Calculate total content height (all tabs + their panels)
    fn total_content_height(&self, context_manager: &ContextManager<EventProxy>) -> f32 {
        let num_tabs = context_manager.len();
        let mut height = 0.0;
        for tab_index in 0..num_tabs {
            height += TAB_ROW_HEIGHT;
            let panel_count = context_manager.panel_titles(tab_index).len();
            height += panel_count as f32 * PANEL_ROW_HEIGHT;
        }
        height
    }

    /// Clamp scroll offset to valid range
    fn clamp_scroll(&mut self, visible_height: f32, total_height: f32) {
        let max_scroll = (total_height - visible_height).max(0.0);
        self.scroll_offset = self.scroll_offset.clamp(0.0, max_scroll);
    }

    /// Handle scroll input within sidebar region
    pub fn scroll(
        &mut self,
        delta: f32,
        window_height: f32,
        scale_factor: f32,
        context_manager: &ContextManager<EventProxy>,
    ) {
        let visible_height = window_height / scale_factor - self.content_top();
        let total_height = self.total_content_height(context_manager);
        self.scroll_offset -= delta;
        self.clamp_scroll(visible_height, total_height);
    }

    #[inline]
    pub fn render(
        &mut self,
        sugarloaf: &mut Sugarloaf,
        dimensions: (f32, f32, f32),
        context_manager: &ContextManager<EventProxy>,
    ) {
        let (_window_width, window_height, scale_factor) = dimensions;
        let num_tabs = context_manager.len();
        let current_tab_index = context_manager.current_index();

        if self.collapsed {
            for tab_data in self.tab_data.values() {
                sugarloaf.set_visibility(tab_data.text_id, false);
                for &pid in &tab_data.panel_text_ids {
                    sugarloaf.set_visibility(pid, false);
                }
            }

            // On macOS, draw just the collapse icon (no strip bg)
            #[cfg(target_os = "macos")]
            {
                let btn_x = 76.0_f32;
                let btn_y = 16.0 - COLLAPSE_BUTTON_SIZE / 2.0;
                let is_hovered = self.hovered == SidebarHit::CollapseButton;
                if is_hovered {
                    let pad = 4.0;
                    sugarloaf.rect(
                        None,
                        btn_x - pad,
                        btn_y - pad,
                        COLLAPSE_BUTTON_SIZE + pad * 2.0,
                        COLLAPSE_BUTTON_SIZE + pad * 2.0,
                        HOVER_BG,
                        0.065,
                        2,
                    );
                }
                let icon_color = if is_hovered {
                    self.active_text_color
                } else {
                    self.inactive_text_color
                };
                self.draw_collapse_icon(sugarloaf, btn_x, btn_y, icon_color);
            }

            // On Linux/Windows, draw a narrow strip with the toggle icon
            #[cfg(not(target_os = "macos"))]
            {
                let view_h = window_height / scale_factor;
                // Strip background
                sugarloaf.rect(
                    None,
                    0.0,
                    0.0,
                    COLLAPSED_WIDTH,
                    view_h,
                    self.bg_color,
                    0.05,
                    1,
                );
                // Right border
                sugarloaf.rect(
                    None,
                    COLLAPSED_WIDTH - BORDER_WIDTH,
                    0.0,
                    BORDER_WIDTH,
                    view_h,
                    self.border_color,
                    0.05,
                    1,
                );
                let btn_x = (COLLAPSED_WIDTH - COLLAPSE_BUTTON_SIZE) / 2.0;
                let btn_y = (HEADER_HEIGHT - COLLAPSE_BUTTON_SIZE) / 2.0;
                self.draw_collapse_icon(
                    sugarloaf,
                    btn_x,
                    btn_y,
                    self.inactive_text_color,
                );
            }

            return;
        }

        if self.hide_if_single && num_tabs == 1 {
            for tab_data in self.tab_data.values() {
                sugarloaf.set_visibility(tab_data.text_id, false);
                for &pid in &tab_data.panel_text_ids {
                    sugarloaf.set_visibility(pid, false);
                }
            }
            return;
        }

        let sidebar_w = self.width;
        let view_h = window_height / scale_factor;

        // Background
        sugarloaf.rect(None, 0.0, 0.0, sidebar_w, view_h, self.bg_color, 0.05, 1);

        // Right border
        sugarloaf.rect(
            None,
            sidebar_w - BORDER_WIDTH,
            0.0,
            BORDER_WIDTH,
            view_h,
            self.border_color,
            0.05,
            1,
        );

        // Collapse button — drawn with rects (no font dependency).
        // Looks like a sidebar toggle icon: a rectangle with a
        // vertical divider on the left third.
        // On macOS, place it right of the traffic light buttons.
        #[cfg(target_os = "macos")]
        let (btn_x, btn_y) = (76.0, 16.0 - COLLAPSE_BUTTON_SIZE / 2.0);
        #[cfg(not(target_os = "macos"))]
        let (btn_x, btn_y) = (
            TAB_PADDING_LEFT,
            (self.content_top() - COLLAPSE_BUTTON_SIZE) / 2.0,
        );
        let is_collapse_hovered = self.hovered == SidebarHit::CollapseButton;

        // Hover background for collapse button
        if is_collapse_hovered {
            let pad = 4.0;
            sugarloaf.rect(
                None,
                btn_x - pad,
                btn_y - pad,
                COLLAPSE_BUTTON_SIZE + pad * 2.0,
                COLLAPSE_BUTTON_SIZE + pad * 2.0,
                HOVER_BG,
                0.065,
                2,
            );
        }

        let icon_color = if is_collapse_hovered {
            self.active_text_color
        } else {
            self.inactive_text_color
        };

        self.draw_collapse_icon(sugarloaf, btn_x, btn_y, icon_color);

        // Tab list
        let content_top = self.content_top();
        let visible_height = view_h - content_top;
        let total_height = self.total_content_height(context_manager);
        self.clamp_scroll(visible_height, total_height);

        let mut y = content_top - self.scroll_offset;

        // Hide all existing text first
        for tab_data in self.tab_data.values() {
            sugarloaf.set_visibility(tab_data.text_id, false);
            for &pid in &tab_data.panel_text_ids {
                sugarloaf.set_visibility(pid, false);
            }
        }

        let max_text_width = sidebar_w - TAB_PADDING_LEFT - RIGHT_PADDING;

        for tab_index in 0..num_tabs {
            let is_active = tab_index == current_tab_index;
            let title = self.get_title_for_tab(context_manager, tab_index);

            // Tab row
            if y + TAB_ROW_HEIGHT > content_top && y < view_h {
                let is_hovered = self.hovered == SidebarHit::Tab(tab_index);

                if is_active {
                    sugarloaf.rect(
                        None,
                        0.0,
                        y,
                        ACCENT_BAR_WIDTH,
                        TAB_ROW_HEIGHT,
                        self.active_text_color,
                        0.06,
                        1,
                    );
                    sugarloaf.rect(
                        None,
                        0.0,
                        y,
                        sidebar_w - BORDER_WIDTH,
                        TAB_ROW_HEIGHT,
                        DEFAULT_ACTIVE_BG,
                        0.055,
                        1,
                    );
                } else if is_hovered {
                    sugarloaf.rect(
                        None,
                        0.0,
                        y,
                        sidebar_w - BORDER_WIDTH,
                        TAB_ROW_HEIGHT,
                        HOVER_BG,
                        0.055,
                        1,
                    );
                }

                let tab_data = self.tab_data.entry(tab_index).or_insert_with(|| {
                    let text_id = next_rich_text_id();
                    let _ = sugarloaf.text(Some(text_id));
                    sugarloaf.set_use_grid_cell_size(text_id, false);
                    sugarloaf.set_text_font_size(&text_id, TAB_FONT_SIZE);
                    sugarloaf.set_order(text_id, 10);
                    TabSidebarData {
                        text_id,
                        panel_text_ids: Vec::new(),
                    }
                });

                let text_color = if is_active {
                    self.active_text_color
                } else {
                    self.inactive_text_color
                };

                let style = SpanStyle {
                    color: text_color,
                    ..SpanStyle::default()
                };

                // Truncate title to fit
                let display_title = if title.len() > (max_text_width / 6.5) as usize {
                    let max_chars = (max_text_width / 6.5) as usize;
                    format!("{}...", &title[..max_chars.saturating_sub(3)])
                } else {
                    title.clone()
                };

                sugarloaf.content().sel(tab_data.text_id).clear().new_line();
                add_span_with_fallback(sugarloaf, &display_title, style);
                sugarloaf.content().build();

                let text_y = y + (TAB_ROW_HEIGHT - TAB_FONT_SIZE) / 2.0;
                sugarloaf.set_position(tab_data.text_id, TAB_PADDING_LEFT, text_y);
                sugarloaf.set_visibility(tab_data.text_id, true);
            }

            y += TAB_ROW_HEIGHT;

            // Panel sub-items — each panel shows its own terminal title
            let panel_titles = context_manager.panel_titles(tab_index);
            let panel_count = panel_titles.len();
            {
                let tab_data = self.tab_data.get_mut(&tab_index).unwrap();

                // Ensure enough panel text ids
                while tab_data.panel_text_ids.len() < panel_count {
                    let pid = next_rich_text_id();
                    let _ = sugarloaf.text(Some(pid));
                    sugarloaf.set_use_grid_cell_size(pid, false);
                    sugarloaf.set_text_font_size(&pid, PANEL_FONT_SIZE);
                    sugarloaf.set_order(pid, 10);
                    tab_data.panel_text_ids.push(pid);
                }

                for (panel_idx, panel_label) in panel_titles.iter().enumerate() {
                    if y + PANEL_ROW_HEIGHT > content_top && y < view_h {
                        let dim_color = [
                            self.inactive_text_color[0],
                            self.inactive_text_color[1],
                            self.inactive_text_color[2],
                            self.inactive_text_color[3] * 0.7,
                        ];
                        let panel_style = SpanStyle {
                            color: dim_color,
                            ..SpanStyle::default()
                        };

                        let pid = tab_data.panel_text_ids[panel_idx];
                        sugarloaf.content().sel(pid).clear().new_line();
                        add_span_with_fallback(sugarloaf, panel_label, panel_style);
                        sugarloaf.content().build();

                        let panel_text_y = y + (PANEL_ROW_HEIGHT - PANEL_FONT_SIZE) / 2.0;
                        sugarloaf.set_position(
                            pid,
                            TAB_PADDING_LEFT + PANEL_INDENT,
                            panel_text_y,
                        );
                        sugarloaf.set_visibility(pid, true);
                    }
                    y += PANEL_ROW_HEIGHT;
                }
            }
        }

        // Scrollbar — only if content overflows
        if total_height > visible_height {
            let track_top = content_top + SCROLLBAR_MARGIN;
            let track_height = visible_height - SCROLLBAR_MARGIN * 2.0;
            let thumb_ratio = visible_height / total_height;
            let thumb_height = (track_height * thumb_ratio).max(MIN_THUMB_HEIGHT);
            let max_scroll = (total_height - visible_height).max(1.0);
            let thumb_y = track_top
                + (track_height - thumb_height) * (self.scroll_offset / max_scroll);

            let scrollbar_x = sidebar_w - SCROLLBAR_WIDTH - SCROLLBAR_MARGIN;

            // Thumb
            sugarloaf.rect(
                None,
                scrollbar_x,
                thumb_y,
                SCROLLBAR_WIDTH,
                thumb_height,
                [1.0, 1.0, 1.0, 0.3],
                0.07,
                2,
            );
        }
    }

    fn draw_collapse_icon(
        &self,
        sugarloaf: &mut Sugarloaf,
        btn_x: f32,
        btn_y: f32,
        icon_color: [f32; 4],
    ) {
        // Outer frame
        sugarloaf.rect(
            None,
            btn_x,
            btn_y,
            COLLAPSE_BUTTON_SIZE,
            COLLAPSE_BUTTON_SIZE,
            icon_color,
            0.07,
            2,
        );
        // Inner fill
        sugarloaf.rect(
            None,
            btn_x + 1.0,
            btn_y + 1.0,
            COLLAPSE_BUTTON_SIZE - 2.0,
            COLLAPSE_BUTTON_SIZE - 2.0,
            self.bg_color,
            0.07,
            2,
        );
        // Vertical divider at ~1/3
        let divider_x = btn_x + COLLAPSE_BUTTON_SIZE * 0.33;
        sugarloaf.rect(
            None,
            divider_x,
            btn_y + 1.0,
            1.0,
            COLLAPSE_BUTTON_SIZE - 2.0,
            icon_color,
            0.07,
            2,
        );
        // Two horizontal lines in the left panel
        let line_x = btn_x + 3.0;
        let line_w = divider_x - btn_x - 4.0;
        sugarloaf.rect(None, line_x, btn_y + 5.0, line_w, 1.0, icon_color, 0.07, 2);
        sugarloaf.rect(None, line_x, btn_y + 9.0, line_w, 1.0, icon_color, 0.07, 2);
    }

    fn get_title_for_tab(
        &self,
        context_manager: &ContextManager<EventProxy>,
        tab_index: usize,
    ) -> String {
        if let Some(context_title) = context_manager.titles.titles.get(&tab_index) {
            if !context_title.content.is_empty() {
                return context_title.content.clone();
            }
            if let Some(ref extra) = context_title.extra {
                if !extra.program.is_empty() {
                    return extra.program.clone();
                }
            }
        }
        format!("Tab {}", tab_index + 1)
    }

    /// Hit test: returns Some(tab_index) if a tab row was clicked,
    /// or None for collapse button, scrollbar, etc.
    pub fn hit_test(
        &self,
        x: f32,
        y: f32,
        context_manager: &ContextManager<EventProxy>,
    ) -> SidebarHit {
        // When collapsed, only the icon area is interactive
        if self.collapsed {
            #[cfg(not(target_os = "macos"))]
            if x < COLLAPSED_WIDTH {
                return SidebarHit::CollapseButton;
            }
            #[cfg(target_os = "macos")]
            {
                let btn_x = 76.0_f32;
                let btn_y = 16.0 - COLLAPSE_BUTTON_SIZE / 2.0;
                let hit_pad = 6.0;
                if x >= btn_x - hit_pad
                    && x <= btn_x + COLLAPSE_BUTTON_SIZE + hit_pad
                    && y >= btn_y - hit_pad
                    && y <= btn_y + COLLAPSE_BUTTON_SIZE + hit_pad
                {
                    return SidebarHit::CollapseButton;
                }
            }
            return SidebarHit::None;
        }

        // Collapse button hit area — padded around the icon
        #[cfg(target_os = "macos")]
        let (btn_x, btn_y) = (76.0_f32, 16.0 - COLLAPSE_BUTTON_SIZE / 2.0);
        #[cfg(not(target_os = "macos"))]
        let (btn_x, btn_y) = (
            TAB_PADDING_LEFT,
            (self.content_top() - COLLAPSE_BUTTON_SIZE) / 2.0,
        );
        let hit_pad = 6.0;
        if x >= btn_x - hit_pad
            && x <= btn_x + COLLAPSE_BUTTON_SIZE + hit_pad
            && y >= btn_y - hit_pad
            && y <= btn_y + COLLAPSE_BUTTON_SIZE + hit_pad
        {
            return SidebarHit::CollapseButton;
        }

        // Header area (above tab list) — not clickable
        let content_top = self.content_top();
        if y < content_top {
            return SidebarHit::None;
        }

        let mut row_y = content_top - self.scroll_offset;
        let num_tabs = context_manager.len();

        for tab_index in 0..num_tabs {
            if y >= row_y && y < row_y + TAB_ROW_HEIGHT {
                return SidebarHit::Tab(tab_index);
            }
            row_y += TAB_ROW_HEIGHT;

            let panel_count = context_manager.panel_titles(tab_index).len();
            for panel_idx in 0..panel_count {
                if y >= row_y && y < row_y + PANEL_ROW_HEIGHT {
                    return SidebarHit::Panel(tab_index, panel_idx);
                }
                row_y += PANEL_ROW_HEIGHT;
            }
        }

        SidebarHit::None
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SidebarHit {
    None,
    CollapseButton,
    Tab(usize),
    Panel(usize, usize),
}
