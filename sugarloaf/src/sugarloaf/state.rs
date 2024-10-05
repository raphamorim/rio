// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use super::compositors::SugarCompositors;
use crate::font::FontLibrary;
use crate::sugarloaf::{text, QuadBrush, RectBrush, RichTextBrush, SugarloafLayout};
use crate::{Content, Graphics, Object, RichText};

pub struct SugarState {
    objects: Vec<Object>,
    pub rich_texts: Vec<RichText>,
    rich_text_repaint: Vec<usize>,
    pub layout: SugarloafLayout,
    pub compositors: SugarCompositors,
}

impl SugarState {
    pub fn new(
        initial_layout: SugarloafLayout,
        font_library: &FontLibrary,
        font_features: &Option<Vec<String>>,
    ) -> SugarState {
        let mut state = SugarState {
            compositors: SugarCompositors::new(font_library),
            // First time computing changes should obtain dimensions
            layout: initial_layout,
            objects: vec![],
            rich_texts: vec![],
            rich_text_repaint: vec![],
        };

        state.compositors.advanced.set_font_features(font_features);
        state
    }

    #[inline]
    pub fn compute_layout_resize(&mut self, width: u32, height: u32) {
        self.layout.resize(width, height);
        // self.latest_change = SugarTreeDiff::Repaint;
    }

    #[inline]
    pub fn compute_layout_rescale(&mut self, scale: f32) {
        self.compositors.advanced.reset();
        self.layout.scale_factor = scale;
        for (id, state) in &mut self.compositors.advanced.content.states {
            state.layout.rescale(scale).update(&self.layout);
            state.layout.dimensions.height = 0.0;
            state.layout.dimensions.width = 0.0;

            self.rich_text_repaint.push(*id);
        }
    }

    #[inline]
    pub fn compute_layout_font_size(&mut self, rich_text_id: &usize, operation: u8) {
        if let Some(rte) = self
            .compositors
            .advanced
            .content
            .get_state_mut(rich_text_id)
        {
            let should_update = match operation {
                0 => rte.layout.reset_font_size(),
                2 => rte.layout.increase_font_size(),
                1 => rte.layout.decrease_font_size(),
                _ => false,
            };

            if should_update {
                rte.layout.dimensions.height = 0.0;
                rte.layout.dimensions.width = 0.0;
                self.rich_text_repaint.push(*rich_text_id);
            }
        }
    }

    #[inline]
    pub fn set_fonts(&mut self, fonts: &FontLibrary) {
        self.compositors.advanced.set_fonts(fonts);
        for (id, state) in &mut self.compositors.advanced.content.states {
            state.layout.dimensions.height = 0.0;
            state.layout.dimensions.width = 0.0;
            self.rich_text_repaint.push(*id);
        }
    }

    #[inline]
    pub fn set_font_features(&mut self, font_features: &Option<Vec<String>>) {
        self.compositors.advanced.set_font_features(font_features);
    }

    #[inline]
    pub fn clean_screen(&mut self) {
        // self.content.clear();
        self.objects.clear();
    }

    #[inline]
    pub fn compute_objects(&mut self, new_objects: Vec<Object>) {
        // Block are used only with elementary renderer
        let mut rich_texts: Vec<RichText> = vec![];
        for obj in &new_objects {
            if let Object::RichText(rich_text) = obj {
                rich_texts.push(*rich_text);
            }
        }
        self.objects = new_objects;
        self.rich_texts = rich_texts
    }

    #[inline]
    pub fn reset_compositors(&mut self) {
        self.compositors.elementary.clean();
        self.compositors.advanced.reset();
    }

    #[inline]
    pub fn clear_rich_text(&mut self, id: &usize) {
        self.compositors.advanced.clear_rich_text(id);
    }

    #[inline]
    pub fn create_rich_text(&mut self) -> usize {
        self.compositors
            .advanced
            .create_rich_text(&self.layout.default_rich_text)
    }

    pub fn content(&mut self) -> &mut Content {
        &mut self.compositors.advanced.content
    }

    #[inline]
    pub fn compute_updates(
        &mut self,
        advance_brush: &mut RichTextBrush,
        elementary_brush: &mut text::GlyphBrush<()>,
        rect_brush: &mut RectBrush,
        quad_brush: &mut QuadBrush,
        context: &mut super::Context,
        graphics: &mut Graphics,
    ) {
        advance_brush.prepare(context, self, graphics);
        rect_brush.resize(context);
        quad_brush.resize(context);

        // Elementary renderer is used for everything else in sugarloaf
        // like objects rendering (created by .text() or .append_rects())
        // ...
        // If current tree has objects and compositor has empty objects
        // It means that's either the first render or objects were erased on compute_diff() step
        for object in &self.objects {
            match object {
                Object::Text(text) => {
                    elementary_brush.queue(
                        &self
                            .compositors
                            .elementary
                            .create_section_from_text(text, &self.layout),
                    );
                }
                Object::Rect(rect) => {
                    self.compositors.elementary.rects.push(*rect);
                }
                Object::Quad(composed_quad) => {
                    self.compositors.elementary.quads.push(*composed_quad);
                }
                Object::RichText(_rich_text) => {
                    // self.rich_texts.push(*rich_text);
                }
            }
        }
    }

    #[inline]
    pub fn compute_dimensions(&mut self, advance_brush: &mut RichTextBrush) {
        // If sugar dimensions are empty then need to find it
        for rich_text in &self.rich_texts {
            if let Some(rte) = self.compositors.advanced.content.get_state(&rich_text.id)
            {
                if rte.layout.dimensions.width == 0.0
                    || rte.layout.dimensions.height == 0.0
                {
                    self.rich_text_repaint.push(rich_text.id);

                    tracing::info!("has empty dimensions, will try to find...");
                }
            }
        }

        if self.rich_text_repaint.is_empty() {
            return;
        }
        for rich_text in &self.rich_text_repaint {
            self.compositors.advanced.content.update_dimensions(
                &rich_text,
                &self.layout,
                advance_brush,
            );
        }

        self.rich_text_repaint.clear();
    }
}

// TODO: Write tests for compute layout updates
