// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// titlebar.rs was originally retired from boo editor
// which is licensed under MIT license.

use crate::context::ContextManager;
use rio_backend::event::EventProxy;
use rio_backend::sugarloaf::Sugarloaf;

/// Height of the titlebar in pixels
pub const TITLEBAR_HEIGHT: f32 = 40.0;

/// Margin from the right edge for the title text
const TITLE_MARGIN_RIGHT: f32 = 16.0;

/// Vertical centering offset for title text within titlebar
const TITLE_OFFSET_Y: f32 = 25.0;

/// Font size for the title text
const TITLE_FONT_SIZE: f32 = 13.0;

pub struct Titlebar {
    /// Whether the titlebar is enabled
    pub enabled: bool,
    /// Background color of the titlebar (RGBA)
    pub background_color: [f32; 4],
    /// Title text color (RGBA)
    pub title_color: [f32; 4],
    /// Whether to show shadow below titlebar
    pub show_shadow: bool,
}

impl Default for Titlebar {
    fn default() -> Self {
        Self {
            // Disabled by default - can be enabled via configuration
            enabled: false,
            // Very subtle dark overlay
            background_color: [0.0, 0.0, 0.0, 0.1],
            // Subtle text color
            title_color: [0.7, 0.7, 0.7, 0.8],
            show_shadow: true,
        }
    }
}

impl Titlebar {
    pub fn new() -> Self {
        Self::default()
    }

    /// Render the titlebar using GPU primitives
    #[inline]
    pub fn render(
        &self,
        sugarloaf: &mut Sugarloaf,
        dimensions: (f32, f32, f32),
        context_manager: &ContextManager<EventProxy>,
    ) {
        if !self.enabled {
            return;
        }

        let (window_width, _window_height, scale_factor) = dimensions;

        // Scale titlebar height
        let scaled_titlebar_height = TITLEBAR_HEIGHT * scale_factor;

        // Render titlebar background rectangle
        sugarloaf.rect(
            0.0,
            0.0,
            window_width / scale_factor,
            TITLEBAR_HEIGHT,
            self.background_color,
            0.9, // depth - above terminal content but below dialogs
        );

        // Render shadow below titlebar if enabled
        if self.show_shadow {
            // Subtle shadow gradient
            let shadow_height = 3.0;
            for i in 0..3 {
                let alpha = 0.2 * (1.0 - (i as f32 / 3.0));
                let shadow_color = [0.0, 0.0, 0.0, alpha];
                sugarloaf.rect(
                    0.0,
                    TITLEBAR_HEIGHT + (i as f32),
                    window_width / scale_factor,
                    1.0,
                    shadow_color,
                    0.9,
                );
            }
        }

        // Get the current terminal title
        let title = self.get_title_text(context_manager);

        if !title.is_empty() {
            // Create temporary rich text for the title
            let title_rt_id = sugarloaf.create_temp_rich_text();
            sugarloaf.set_rich_text_font_size(&title_rt_id, TITLE_FONT_SIZE);

            // Measure text width (approximate)
            // TODO: Use proper text measurement once available in Sugarloaf
            let approx_char_width = TITLE_FONT_SIZE * 0.6;
            let text_width = title.len() as f32 * approx_char_width;

            // Position title on the right side
            let title_x = (window_width / scale_factor) - text_width - TITLE_MARGIN_RIGHT;
            let title_y = TITLE_OFFSET_Y;

            // Add title text using Content API
            {
                use rio_backend::sugarloaf::FragmentStyle;
                let content = sugarloaf.content();
                content
                    .sel(title_rt_id)
                    .clear()
                    .new_line()
                    .add_text(
                        &title,
                        FragmentStyle {
                            color: self.title_color,
                            ..FragmentStyle::default()
                        },
                    );
            }

            // Show the title text at calculated position
            sugarloaf.show_rich_text(title_rt_id, title_x, title_y);
        }
    }

    /// Get the title text to display from the current context
    fn get_title_text(&self, context_manager: &ContextManager<EventProxy>) -> String {
        let current_idx = context_manager.current_index();

        if let Some(context_title) = context_manager.titles.titles.get(&current_idx) {
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

        // Default fallback
        String::from("Rio")
    }

    /// Set whether the titlebar is enabled
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Set the background color of the titlebar
    pub fn set_background_color(&mut self, color: [f32; 4]) {
        self.background_color = color;
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
