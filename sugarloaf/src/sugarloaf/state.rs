// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::font::FontLibrary;
use crate::layout::{RootStyle, TextLayout};
use crate::renderer::Renderer;
use crate::Graphics;
use crate::{Content, TextDimensions};

pub struct SugarState {
    // Rich text metadata now managed directly in content.states[].render_data
    pub style: RootStyle,
    pub content: Content,
    pub visual_bell_overlay: Option<crate::sugarloaf::primitives::Rect>,
}

impl SugarState {
    pub fn new(
        style: RootStyle,
        font_library: &FontLibrary,
        font_features: &Option<Vec<String>>,
    ) -> SugarState {
        let found_font_features = SugarState::found_font_features(font_features);
        let mut content = Content::new(font_library);
        content.set_font_features(found_font_features);

        SugarState {
            content,
            style,
            visual_bell_overlay: None,
        }
    }

    pub fn found_font_features(
        font_features: &Option<Vec<String>>,
    ) -> Vec<crate::font_introspector::Setting<u16>> {
        // Simplified for now - TODO: Implement proper font feature parsing
        vec![]
    }

    #[inline]
    pub fn contains_rich_text(&self, text_id: &usize) -> bool {
        self.content.get_text_by_id(*text_id).is_some()
    }

    #[inline]
    pub fn contains_id(&self, id: &usize) -> bool {
        self.content.states.contains_key(id)
    }

    #[inline]
    pub fn new_layer(&mut self) {}

    #[inline]
    pub fn content(&mut self) -> &mut Content {
        &mut self.content
    }

    #[inline]
    pub fn get_state_layout(&self, id: &usize) -> TextLayout {
        if let Some(state) = self.content.get_state(id) {
            state.layout
        } else {
            TextLayout::from_default_layout(&self.style)
        }
    }

    #[inline]
    pub fn get_text_dimensions(&mut self, id: &usize) -> TextDimensions {
        // Mark for repaint
        if let Some(content_state) = self.content.states.get_mut(id) {
            content_state.render_data.needs_repaint = true;
        }

        if let Some(text_state) = self.content.get_state(id) {
            let layout = &text_state.layout;
            TextDimensions {
                scale: layout.dimensions.scale,
                width: layout.dimensions.width,
                height: layout.dimensions.height,
            }
        } else {
            TextDimensions::default()
        }
    }

    #[inline]
    pub fn clean_screen(&mut self) {
        // Mark all states for removal - they'll be re-added by route screen functions
        for (_, content_state) in self.content.states.iter_mut() {
            content_state.render_data.mark_for_removal();
        }
    }

    #[inline]
    pub fn update_text_style(&mut self, text_id: &usize, operation: u8) {
        if let Some(text_state) = self.content.get_state_mut(text_id) {
            let should_update = match operation {
                0 => text_state.reset_font_size(),
                2 => text_state.increase_font_size(),
                1 => text_state.decrease_font_size(),
                _ => false,
            };

            if should_update {
                text_state.layout.dimensions.height = 0.0;
                text_state.layout.dimensions.width = 0.0;
            }
        }

        // Mark for repaint
        if let Some(content_state) = self.content.states.get_mut(text_id) {
            content_state.render_data.needs_repaint = true;
        }

        self.compute_dimensions();
    }

    #[inline]
    pub fn compute_dimensions(&mut self) {
        // Collect text IDs that need repaint
        let ids_to_repaint: Vec<usize> = self
            .content
            .states
            .iter()
            .filter_map(|(id, content_state)| {
                // Only process text content that needs repaint
                if content_state.render_data.needs_repaint && content_state.is_text() {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();

        if ids_to_repaint.is_empty() {
            return;
        }

        // Process each text ID
        for text_id in &ids_to_repaint {
            self.content.update_dimensions(text_id);
        }

        // Clear repaint flags after processing
        for id in ids_to_repaint {
            if let Some(content_state) = self.content.states.get_mut(&id) {
                content_state.render_data.clear_repaint_flag();
            }
        }
    }

    #[inline]
    pub fn compute_updates(
        &mut self,
        advance_brush: &mut Renderer,
        context: &mut super::Context,
        graphics: &mut Graphics,
    ) {
        // Shape transient texts before rendering
        self.content.build_transient_texts();
        advance_brush.prepare(context, self, graphics);
    }

    #[inline]
    pub fn reset(&mut self) {
        // Remove states marked for removal
        let mut to_remove = Vec::new();
        for (id, content_state) in &self.content.states {
            if content_state.render_data.should_remove {
                to_remove.push(*id);
            }
        }

        for id in to_remove {
            self.content.remove_state(&id);
        }

        // Clear all transient texts (they get recreated each frame)
        self.content.clear_transient_texts();

        self.content.mark_states_clean();
    }

    #[inline]
    pub fn clear_text(&mut self, id: &usize) {
        self.content.clear_state(id);
    }

    #[inline]
    pub fn set_content_position(&mut self, id: usize, x: f32, y: f32) {
        if let Some(content_state) = self.content.states.get_mut(&id) {
            content_state.render_data.set_position(x, y);
        }
    }

    #[inline]
    pub fn set_content_visibility_and_position(
        &mut self,
        id: usize,
        x: f32,
        y: f32,
        hidden: bool,
    ) {
        if let Some(content_state) = self.content.states.get_mut(&id) {
            content_state.render_data.set_position(x, y);
            content_state.render_data.set_hidden(hidden);
            content_state.render_data.should_remove = false;
        }
    }

    #[inline]
    pub fn set_content_hidden(&mut self, id: usize, hidden: bool) {
        if let Some(content_state) = self.content.states.get_mut(&id) {
            content_state.render_data.set_hidden(hidden);
        }
    }

    pub fn set_visual_bell_overlay(
        &mut self,
        overlay: Option<crate::sugarloaf::primitives::Rect>,
    ) {
        self.visual_bell_overlay = overlay;
    }

    #[inline]
    pub fn set_fonts(
        &mut self,
        _font_library: &FontLibrary,
        _advance_brush: &mut Renderer,
    ) {
        // Simplified - fonts are handled elsewhere in the unified system
    }

    #[inline]
    pub fn set_text_font_size_based_on_action(&mut self, text_id: &usize, operation: u8) {
        self.update_text_style(text_id, operation);
    }

    #[inline]
    pub fn set_text_font_size(&mut self, rt_id: &usize, font_size: f32) {
        if let Some(content_state) = self.content.states.get_mut(rt_id) {
            if let Some(text_state) = content_state.as_text_mut() {
                text_state.layout.font_size = font_size;
                text_state.scaled_font_size = font_size * self.style.scale_factor;
            }
            content_state.render_data.needs_repaint = true;
        }
        self.compute_dimensions();
    }

    #[inline]
    pub fn set_text_line_height(&mut self, _rt_id: &usize, _line_height: f32) {
        // Simplified - line height changes handled elsewhere
    }

    #[inline]
    pub fn compute_layout_rescale(&mut self, _scale: f32) {
        // Simplified - rescaling handled elsewhere
        self.compute_dimensions();
    }
}
