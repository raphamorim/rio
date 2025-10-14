// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::font::FontLibrary;
use crate::layout::{RichTextLayout, RootStyle};
use crate::sugarloaf::RichTextBrush;
use crate::Graphics;
use crate::{Content, SugarDimensions};

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
    pub fn contains_rich_text(&self, rich_text_id: &usize) -> bool {
        self.content.states.contains_key(rich_text_id)
    }

    #[inline]
    pub fn new_layer(&mut self) {}

    #[inline]
    pub fn content(&mut self) -> &mut Content {
        &mut self.content
    }

    #[inline]
    pub fn get_state_layout(&self, id: &usize) -> RichTextLayout {
        if let Some(state) = self.content.get_state(id) {
            println!(
                "get_state_layout returning dimensions {}x{} for state {}",
                state.layout.dimensions.width, state.layout.dimensions.height, id
            );
            state.layout.clone()
        } else {
            println!(
                "get_state_layout: state {} not found, returning default",
                id
            );
            RichTextLayout::from_default_layout(&self.style)
        }
    }

    #[inline]
    pub fn get_rich_text_dimensions(
        &mut self,
        id: &usize,
        advance_brush: &mut RichTextBrush,
    ) -> SugarDimensions {
        // Mark for repaint directly in render_data
        if let Some(state) = self.content.states.get_mut(id) {
            state.render_data.needs_repaint = true;
        }

        if let Some(state) = self.content.get_state(id) {
            let layout = &state.layout;
            SugarDimensions {
                scale: layout.dimensions.scale, // Use the actual scale, not font_size/line_height!
                width: layout.dimensions.width,
                height: layout.dimensions.height,
            }
        } else {
            SugarDimensions::default()
        }
    }

    #[inline]
    pub fn clean_screen(&mut self) {
        // Mark all rich text states for removal - they'll be re-added by route screen functions
        for (_, state) in self.content.states.iter_mut() {
            state.render_data.mark_for_removal();
        }
    }

    #[inline]
    pub fn update_rich_text_style(
        &mut self,
        rich_text_id: &usize,
        operation: u8,
        advance_brush: &mut RichTextBrush,
    ) {
        if let Some(rte) = self.content.get_state_mut(rich_text_id) {
            let should_update = match operation {
                0 => rte.reset_font_size(),
                2 => rte.increase_font_size(),
                1 => rte.decrease_font_size(),
                _ => false,
            };

            if should_update {
                rte.layout.dimensions.height = 0.0;
                rte.layout.dimensions.width = 0.0;
                // Mark for repaint directly in render_data
                if let Some(state) = self.content.states.get_mut(rich_text_id) {
                    state.render_data.needs_repaint = true;
                }
            }
        }

        self.compute_dimensions(advance_brush);
    }

    #[inline]
    pub fn compute_dimensions(&mut self, advance_brush: &mut RichTextBrush) {
        // Collect IDs that need repaint first
        let ids_to_repaint: Vec<usize> = self
            .content
            .states
            .iter()
            .filter_map(|(id, state)| {
                if state.render_data.needs_repaint {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();

        // If nothing needs repainting, return early
        if ids_to_repaint.is_empty() {
            return;
        }

        // Process each ID
        for rich_text_id in &ids_to_repaint {
            self.content.update_dimensions(rich_text_id);
        }

        // Clear repaint flags after processing
        for id in ids_to_repaint {
            if let Some(state) = self.content.states.get_mut(&id) {
                state.render_data.clear_repaint_flag();
            }
        }
    }

    pub fn compute_updates(
        &mut self,
        advance_brush: &mut RichTextBrush,
        context: &mut super::Context,
        graphics: &mut Graphics,
    ) {
        advance_brush.prepare(context, self, graphics);
        // Rectangles are now rendered directly via add_rect() calls
        // No object processing needed anymore
    }

    #[inline]
    pub fn reset(&mut self) {
        // Remove states marked for removal and clear repaint flags
        let mut to_remove = Vec::new();
        for (id, state) in &self.content.states {
            if state.render_data.should_remove {
                to_remove.push(*id);
            }
        }

        for id in to_remove {
            self.content.remove_state(&id);
        }

        self.content.mark_states_clean();
    }

    #[inline]
    pub fn clear_rich_text(&mut self, id: &usize) {
        self.content.clear_state(id);
    }

    #[inline]
    pub fn create_rich_text(&mut self) -> usize {
        let layout = RichTextLayout::from_default_layout(&self.style);
        let id = self.content.create_state(&layout);

        // Dimensions are now calculated eagerly during create_state
        // No need to mark for repaint since we have valid dimensions immediately

        id
    }

    #[inline]
    pub fn create_temp_rich_text(&mut self) -> usize {
        let id = self
            .content
            .create_state(&RichTextLayout::from_default_layout(&self.style));
        // Mark as temporary (for removal) directly in render_data
        if let Some(state) = self.content.states.get_mut(&id) {
            state.render_data.should_remove = true;
        }
        id
    }

    pub fn set_rich_text_visibility_and_position(
        &mut self,
        id: usize,
        x: f32,
        y: f32,
        hidden: bool,
    ) {
        if let Some(state) = self.content.states.get_mut(&id) {
            state.render_data.set_position(x, y);
            state.render_data.set_hidden(hidden);
            state.render_data.should_remove = false; // Ensure it's not marked for removal
        }
    }

    #[inline]
    pub fn set_rich_text_hidden(&mut self, id: usize, hidden: bool) {
        if let Some(state) = self.content.states.get_mut(&id) {
            state.render_data.set_hidden(hidden);
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
        _advance_brush: &mut RichTextBrush,
    ) {
        // Simplified - fonts are handled elsewhere in the unified system
    }

    #[inline]
    pub fn set_rich_text_font_size_based_on_action(
        &mut self,
        rich_text_id: &usize,
        operation: u8,
        advance_brush: &mut RichTextBrush,
    ) {
        self.update_rich_text_style(rich_text_id, operation, advance_brush);
    }

    #[inline]
    pub fn set_rich_text_font_size(
        &mut self,
        rt_id: &usize,
        _font_size: f32,
        advance_brush: &mut RichTextBrush,
    ) {
        // Mark for repaint directly in render_data
        if let Some(state) = self.content.states.get_mut(rt_id) {
            state.render_data.needs_repaint = true;
        }
        self.compute_dimensions(advance_brush);
    }

    #[inline]
    pub fn set_rich_text_line_height(&mut self, _rt_id: &usize, _line_height: f32) {
        // Simplified - line height changes handled elsewhere
    }

    #[inline]
    pub fn compute_layout_rescale(
        &mut self,
        _scale: f32,
        advance_brush: &mut RichTextBrush,
    ) {
        // Simplified - rescaling handled elsewhere
        self.compute_dimensions(advance_brush);
    }
}
