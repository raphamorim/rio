// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::context::next_rich_text_id;
use rio_backend::error::{RioError, RioErrorLevel};
use rio_backend::sugarloaf::{SpanStyle, Sugarloaf};

// Layout
const OVERLAY_WIDTH: f32 = 480.0;
const OVERLAY_CORNER_RADIUS: f32 = 10.0;
const OVERLAY_MARGIN_TOP: f32 = 8.0;
const OVERLAY_MARGIN_RIGHT: f32 = 8.0;
const OVERLAY_PADDING: f32 = 16.0;

const HEADING_FONT_SIZE: f32 = 16.0;
const BODY_FONT_SIZE: f32 = 12.0;
const BUTTON_FONT_SIZE: f32 = 14.0;
const LINK_FONT_SIZE: f32 = 12.0;

const BUTTON_SIZE: f32 = 24.0;
const BUTTON_CORNER_RADIUS: f32 = 4.0;

const LINE_HEIGHT: f32 = 18.0;
const HEADING_HEIGHT: f32 = 28.0;
const LINK_ROW_HEIGHT: f32 = 24.0;
const MAX_VISIBLE_LINES: usize = 16;

const DOCS_URL: &str = "rioterm.com/docs/config";

// Colors
const BACKDROP_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 0.35];
const BG_COLOR: [f32; 4] = [0.12, 0.12, 0.12, 0.98];
const HEADING_COLOR_ERROR: [f32; 4] = [1.0, 0.07, 0.38, 1.0];
const HEADING_COLOR_WARNING: [f32; 4] = [0.99, 0.73, 0.16, 1.0];
const TEXT_COLOR: [f32; 4] = [0.85, 0.85, 0.85, 1.0];
const LINK_COLOR: [f32; 4] = [0.40, 0.60, 1.0, 1.0];
const BUTTON_TEXT_COLOR: [f32; 4] = [0.70, 0.70, 0.70, 1.0];
const BUTTON_HOVER_BG: [f32; 4] = [0.25, 0.25, 0.28, 1.0];

// Depth / order
const DEPTH_BACKDROP: f32 = 0.0;
const DEPTH_BG: f32 = 0.1;
const DEPTH_ELEMENT: f32 = 0.2;
const ORDER: u8 = 20;

/// Actions triggered by clicking assistant overlay buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssistantOverlayAction {
    Close,
    OpenDocs,
}

pub struct AssistantOverlay {
    error: Option<RioError>,
    heading_text_id: Option<usize>,
    body_text_ids: Vec<usize>,
    link_text_id: Option<usize>,
    close_text_id: Option<usize>,
    hovered_button: Option<AssistantOverlayAction>,
    link_button_width: f32,
}

impl Default for AssistantOverlay {
    fn default() -> Self {
        Self {
            error: None,
            heading_text_id: None,
            body_text_ids: Vec::new(),
            link_text_id: None,
            close_text_id: None,
            hovered_button: None,
            link_button_width: 0.0,
        }
    }
}

impl AssistantOverlay {
    #[inline]
    pub fn is_active(&self) -> bool {
        self.error.is_some()
    }

    #[inline]
    pub fn set_error(&mut self, error: RioError) {
        self.error = Some(error);
    }

    #[inline]
    pub fn clear(&mut self) {
        self.error = None;
    }

    /// Returns (overlay_x, overlay_y, overlay_width, overlay_height) in logical coords.
    fn overlay_rect(&self, window_width: f32, scale_factor: f32) -> (f32, f32, f32, f32) {
        let logical_width = window_width / scale_factor;
        let x = logical_width - OVERLAY_WIDTH - OVERLAY_MARGIN_RIGHT;
        let y = OVERLAY_MARGIN_TOP;
        let line_count = self.body_line_count().min(MAX_VISIBLE_LINES);
        let h = OVERLAY_PADDING
            + HEADING_HEIGHT
            + (line_count as f32 * LINE_HEIGHT)
            + LINK_ROW_HEIGHT
            + OVERLAY_PADDING;
        (x, y, OVERLAY_WIDTH, h)
    }

    fn body_line_count(&self) -> usize {
        if let Some(error) = &self.error {
            error.report.to_string().lines().count().max(1)
        } else {
            0
        }
    }

    /// Returns the close button rect.
    fn close_button_rect(
        &self,
        overlay_x: f32,
        overlay_y: f32,
        overlay_width: f32,
    ) -> (f32, f32, f32, f32) {
        let bx = overlay_x + overlay_width - OVERLAY_PADDING - BUTTON_SIZE;
        let by = overlay_y + OVERLAY_PADDING / 2.0;
        (bx, by, BUTTON_SIZE, BUTTON_SIZE)
    }

    /// Returns the docs link button rect (covers the link text area).
    fn docs_button_rect(&self, overlay_x: f32, overlay_y: f32) -> (f32, f32, f32, f32) {
        let line_count = self.body_line_count().min(MAX_VISIBLE_LINES);
        let by = overlay_y
            + OVERLAY_PADDING
            + HEADING_HEIGHT
            + (line_count as f32 * LINE_HEIGHT);
        let bx = overlay_x + OVERLAY_PADDING - 4.0;
        let bw = self.link_button_width + 8.0;
        (bx, by, bw, LINK_ROW_HEIGHT)
    }

    #[inline]
    pub fn hovered_button(&self) -> Option<AssistantOverlayAction> {
        self.hovered_button
    }

    fn hit_test_button(
        mouse_x: f32,
        mouse_y: f32,
        bx: f32,
        by: f32,
        bw: f32,
        bh: f32,
    ) -> bool {
        mouse_x >= bx && mouse_x <= bx + bw && mouse_y >= by && mouse_y <= by + bh
    }

    /// Hit-test a mouse click. Returns Some(action) if a button was clicked.
    /// Returns Err(()) if clicked outside the overlay entirely.
    pub fn hit_test(
        &self,
        mouse_x: f32,
        mouse_y: f32,
        window_width: f32,
        scale_factor: f32,
    ) -> Result<Option<AssistantOverlayAction>, ()> {
        if !self.is_active() {
            return Err(());
        }

        let (ox, oy, ow, oh) = self.overlay_rect(window_width, scale_factor);

        if mouse_x < ox || mouse_x > ox + ow || mouse_y < oy || mouse_y > oy + oh {
            return Err(());
        }

        let (bx, by, bw, bh) = self.close_button_rect(ox, oy, ow);
        if Self::hit_test_button(mouse_x, mouse_y, bx, by, bw, bh) {
            return Ok(Some(AssistantOverlayAction::Close));
        }

        let (bx, by, bw, bh) = self.docs_button_rect(ox, oy);
        if Self::hit_test_button(mouse_x, mouse_y, bx, by, bw, bh) {
            return Ok(Some(AssistantOverlayAction::OpenDocs));
        }

        Ok(None)
    }

    /// Update hover state based on mouse position. Returns true if changed.
    pub fn hover(
        &mut self,
        mouse_x: f32,
        mouse_y: f32,
        window_width: f32,
        scale_factor: f32,
    ) -> bool {
        if !self.is_active() {
            return false;
        }

        let (ox, oy, ow, _oh) = self.overlay_rect(window_width, scale_factor);

        let (bx, by, bw, bh) = self.close_button_rect(ox, oy, ow);
        let mut new_hover = if Self::hit_test_button(mouse_x, mouse_y, bx, by, bw, bh) {
            Some(AssistantOverlayAction::Close)
        } else {
            None
        };

        if new_hover.is_none() {
            let (bx, by, bw, bh) = self.docs_button_rect(ox, oy);
            if Self::hit_test_button(mouse_x, mouse_y, bx, by, bw, bh) {
                new_hover = Some(AssistantOverlayAction::OpenDocs);
            }
        }

        if new_hover != self.hovered_button {
            self.hovered_button = new_hover;
            return true;
        }
        false
    }

    fn ensure_text_ids(&mut self, sugarloaf: &mut Sugarloaf) {
        if self.heading_text_id.is_none() {
            let id = next_rich_text_id();
            let _ = sugarloaf.text(Some(id));
            sugarloaf.set_use_grid_cell_size(id, false);
            sugarloaf.set_text_font_size(&id, HEADING_FONT_SIZE);
            sugarloaf.set_order(id, ORDER);
            self.heading_text_id = Some(id);
        }

        let needed = self.body_line_count().min(MAX_VISIBLE_LINES);
        while self.body_text_ids.len() < needed {
            let id = next_rich_text_id();
            let _ = sugarloaf.text(Some(id));
            sugarloaf.set_use_grid_cell_size(id, false);
            sugarloaf.set_text_font_size(&id, BODY_FONT_SIZE);
            sugarloaf.set_order(id, ORDER);
            self.body_text_ids.push(id);
        }

        if self.link_text_id.is_none() {
            let id = next_rich_text_id();
            let _ = sugarloaf.text(Some(id));
            sugarloaf.set_use_grid_cell_size(id, false);
            sugarloaf.set_text_font_size(&id, LINK_FONT_SIZE);
            sugarloaf.set_order(id, ORDER);
            self.link_text_id = Some(id);
        }

        if self.close_text_id.is_none() {
            let id = next_rich_text_id();
            let _ = sugarloaf.text(Some(id));
            sugarloaf.set_use_grid_cell_size(id, false);
            sugarloaf.set_text_font_size(&id, BUTTON_FONT_SIZE);
            sugarloaf.set_order(id, ORDER);
            self.close_text_id = Some(id);
        }
    }

    fn hide_all_text_ids(&self, sugarloaf: &mut Sugarloaf) {
        if let Some(id) = self.heading_text_id {
            sugarloaf.set_visibility(id, false);
        }
        for &id in &self.body_text_ids {
            sugarloaf.set_visibility(id, false);
        }
        if let Some(id) = self.link_text_id {
            sugarloaf.set_visibility(id, false);
        }
        if let Some(id) = self.close_text_id {
            sugarloaf.set_visibility(id, false);
        }
    }

    pub fn render(&mut self, sugarloaf: &mut Sugarloaf, dimensions: (f32, f32, f32)) {
        if !self.is_active() {
            self.hide_all_text_ids(sugarloaf);
            return;
        }

        let (window_width, window_height, scale_factor) = dimensions;

        self.ensure_text_ids(sugarloaf);

        let (ox, oy, ow, oh) = self.overlay_rect(window_width, scale_factor);

        // Backdrop
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

        // Background
        sugarloaf.rounded_rect(
            None,
            ox,
            oy,
            ow,
            oh,
            BG_COLOR,
            DEPTH_BG,
            OVERLAY_CORNER_RADIUS,
            ORDER,
        );

        let error = self.error.clone().unwrap();
        let is_error = error.level == RioErrorLevel::Error;
        let heading_color = if is_error {
            HEADING_COLOR_ERROR
        } else {
            HEADING_COLOR_WARNING
        };

        // Heading
        let heading_id = self.heading_text_id.unwrap();
        let heading_text = if is_error { "Error" } else { "Warning" };

        let content = sugarloaf.content();
        content
            .sel(heading_id)
            .clear()
            .new_line()
            .add_text(
                heading_text,
                SpanStyle {
                    color: heading_color,
                    ..SpanStyle::default()
                },
            )
            .build();

        let text_x = ox + OVERLAY_PADDING;
        let heading_y = oy + OVERLAY_PADDING;
        sugarloaf.set_position(heading_id, text_x, heading_y);
        sugarloaf.set_visibility(heading_id, true);

        // Body lines
        let body_y_start = heading_y + HEADING_HEIGHT;
        let report_text = error.report.to_string();
        let lines: Vec<&str> = report_text.lines().collect();
        let visible_count = lines.len().min(MAX_VISIBLE_LINES);

        for (i, line_text) in lines.iter().take(visible_count).enumerate() {
            let body_id = self.body_text_ids[i];
            let content = sugarloaf.content();
            content
                .sel(body_id)
                .clear()
                .new_line()
                .add_text(
                    line_text,
                    SpanStyle {
                        color: TEXT_COLOR,
                        ..SpanStyle::default()
                    },
                )
                .build();

            let line_y = body_y_start + (i as f32 * LINE_HEIGHT);
            sugarloaf.set_position(body_id, text_x, line_y);
            sugarloaf.set_visibility(body_id, true);
        }

        // Hide unused body text ids
        for i in visible_count..self.body_text_ids.len() {
            sugarloaf.set_visibility(self.body_text_ids[i], false);
        }

        // Docs link button — render text first to measure, then draw hover bg
        let link_id = self.link_text_id.unwrap();
        let content = sugarloaf.content();
        content
            .sel(link_id)
            .clear()
            .new_line()
            .add_text(
                DOCS_URL,
                SpanStyle {
                    color: LINK_COLOR,
                    ..SpanStyle::default()
                },
            )
            .build();

        let line_count = visible_count;
        let link_area_y =
            oy + OVERLAY_PADDING + HEADING_HEIGHT + (line_count as f32 * LINE_HEIGHT);
        let link_x = ox + OVERLAY_PADDING;
        let link_y = link_area_y + (LINK_ROW_HEIGHT - LINK_FONT_SIZE) / 2.0;
        sugarloaf.set_position(link_id, link_x, link_y);
        sugarloaf.set_visibility(link_id, true);

        let rendered_width = sugarloaf.get_text_rendered_width(&link_id);
        self.link_button_width = rendered_width;

        let (dbx, dby, dbw, dbh) = self.docs_button_rect(ox, oy);
        let docs_hovered = self.hovered_button == Some(AssistantOverlayAction::OpenDocs);

        if docs_hovered {
            sugarloaf.rounded_rect(
                None,
                dbx,
                dby,
                dbw,
                dbh,
                BUTTON_HOVER_BG,
                DEPTH_ELEMENT,
                BUTTON_CORNER_RADIUS,
                ORDER,
            );
        }

        // Close button
        let (bx, by, bw, bh) = self.close_button_rect(ox, oy, ow);
        let is_hovered = self.hovered_button == Some(AssistantOverlayAction::Close);

        if is_hovered {
            sugarloaf.rounded_rect(
                None,
                bx,
                by,
                bw,
                bh,
                BUTTON_HOVER_BG,
                DEPTH_ELEMENT,
                BUTTON_CORNER_RADIUS,
                ORDER,
            );
        }

        let close_id = self.close_text_id.unwrap();
        let content = sugarloaf.content();
        content
            .sel(close_id)
            .clear()
            .new_line()
            .add_text(
                "\u{2022}",
                SpanStyle {
                    color: BUTTON_TEXT_COLOR,
                    ..SpanStyle::default()
                },
            )
            .build();

        let label_x = bx + (bw - BUTTON_FONT_SIZE * 0.6) / 2.0;
        let label_y = by + (bh - BUTTON_FONT_SIZE) / 2.0;
        sugarloaf.set_position(close_id, label_x, label_y);
        sugarloaf.set_visibility(close_id, true);
    }
}
