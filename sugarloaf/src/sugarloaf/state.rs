// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use super::compositors::SugarCompositors;
use crate::font::FontLibrary;
use crate::sugarloaf::{text, RectBrush, RichTextBrush, SugarloafLayout};
use crate::{Content, ContentState, Graphics, Object};

#[derive(Debug, PartialEq)]
pub enum SugarTreeDiff {
    Different,
    Repaint,
}

pub struct SugarState {
    latest_change: SugarTreeDiff,
    objects: Vec<Object>,
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
            latest_change: SugarTreeDiff::Repaint,
        };

        state.compositors.advanced.set_font_features(font_features);
        state
    }

    #[inline]
    pub fn compute_layout_resize(&mut self, width: u32, height: u32) {
        self.layout.resize(width, height).update();
        self.latest_change = SugarTreeDiff::Repaint;
    }

    #[inline]
    pub fn compute_layout_rescale(&mut self, scale: f32) {
        self.compositors.advanced.reset();
        self.layout.rescale(scale).update();
        self.layout.dimensions.height = 0.0;
        self.layout.dimensions.width = 0.0;
        self.latest_change = SugarTreeDiff::Repaint;
    }

    #[inline]
    pub fn compute_layout_font_size(&mut self, operation: u8) {
        let should_update = match operation {
            0 => self.layout.reset_font_size(),
            2 => self.layout.increase_font_size(),
            1 => self.layout.decrease_font_size(),
            _ => false,
        };

        if should_update {
            self.layout.update();
            self.layout.dimensions.height = 0.0;
            self.layout.dimensions.width = 0.0;
            self.latest_change = SugarTreeDiff::Repaint;
        }
    }

    #[inline]
    pub fn set_fonts(&mut self, fonts: &FontLibrary) {
        self.compositors.advanced.set_fonts(fonts);
        self.layout.dimensions.height = 0.0;
        self.layout.dimensions.width = 0.0;
        self.latest_change = SugarTreeDiff::Repaint;
    }

    #[inline]
    pub fn set_font_features(&mut self, font_features: &Option<Vec<String>>) {
        self.compositors.advanced.set_font_features(font_features);
        self.latest_change = SugarTreeDiff::Repaint;
    }

    #[inline]
    pub fn clean_screen(&mut self) {
        // self.content.clear();
        self.objects.clear();
        self.compositors.advanced.reset();
    }

    #[inline]
    pub fn compute_objects(&mut self, new_objects: Vec<Object>) {
        // Block are used only with elementary renderer
        self.objects = new_objects;
    }

    #[inline]
    pub fn reset_compositor(&mut self) {
        self.compositors.elementary.clean();
        self.compositors.advanced.reset();
    }

    #[inline]
    pub fn content(&mut self) -> &mut Content {
        self.compositors
            .advanced
            .content(self.layout.dimensions.scale, self.layout.font_size)
    }

    #[inline]
    pub fn set_content_state(&mut self, new_content_state: ContentState)  {
        self.compositors
            .advanced.set_content_state(new_content_state);
    }

    #[inline]
    pub fn compute_updates(
        &mut self,
        advance_brush: &mut RichTextBrush,
        elementary_brush: &mut text::GlyphBrush<()>,
        rect_brush: &mut RectBrush,
        context: &mut super::Context,
        graphics: &mut Graphics,
    ) -> bool {
        advance_brush.prepare(context, self, graphics);
        rect_brush.resize(context);

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
            }
        }

        true
    }

    #[inline]
    pub fn compute_dimensions(&mut self, advance_brush: &mut RichTextBrush) {
        // If layout is different or current has empty dimensions
        // then current will flip with next and will try to obtain
        // the dimensions.

        if self.latest_change != SugarTreeDiff::Repaint {
            return;
        }

        if let Some(dimension) = advance_brush.dimensions(self) {
            let mut dimensions_changed = false;
            if dimension.height != self.layout.dimensions.height {
                self.layout.dimensions.height = dimension.height;
                tracing::info!("prepare_render: changed height... {}", dimension.height);
                dimensions_changed = true;
            }

            if dimension.width != self.layout.dimensions.width {
                self.layout.dimensions.width = dimension.width;
                self.layout.update_columns_per_font_width();
                tracing::info!("prepare_render: changed width... {}", dimension.width);
                dimensions_changed = true;
            }

            if dimensions_changed {
                self.layout.update();
                tracing::info!("sugar_state: dimensions has changed");
            }
        }
    }

    #[inline]
    pub fn compute_changes(&mut self) {
        // If sugar dimensions are empty then need to find it
        if self.layout.dimensions.width == 0.0 || self.layout.dimensions.height == 0.0 {
            self.compositors.advanced.calculate_dimensions(&self.layout);

            self.compositors.advanced.update_render_data();

            self.latest_change = SugarTreeDiff::Repaint;
            tracing::info!("has empty dimensions, will try to find...");
            return;
        }

        tracing::info!("state compute_changes result: {:?}", self.latest_change);

        match &self.latest_change {
            SugarTreeDiff::Repaint => {
                self.compositors.advanced.calculate_dimensions(&self.layout);

                self.compositors.advanced.update_render_data();

                self.latest_change = SugarTreeDiff::Different;
            }
            SugarTreeDiff::Different => {
                self.compositors.advanced.update_render_data();
            }
        }
    }
}

// TODO: Write tests for compute layout updates
