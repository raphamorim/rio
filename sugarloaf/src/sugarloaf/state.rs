// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use super::compositors::SugarCompositors;
use crate::font::FontLibrary;
use crate::sugarloaf::{text, RectBrush, RichTextBrush, SugarloafLayout};
use crate::Graphics;
use crate::{Content, Object};

#[derive(Debug, PartialEq)]
pub enum SugarTreeDiff {
    Equal,
    Different,
    Repaint,
}

#[derive(Clone, Default)]
pub struct SugarTree {
    pub content: Content,
    pub objects: Vec<Object>,
    pub layout: SugarloafLayout,
}

pub struct SugarState {
    pub current: SugarTree,
    latest_change: SugarTreeDiff,
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
            current: SugarTree {
                layout: initial_layout,
                ..Default::default()
            },
            latest_change: SugarTreeDiff::Repaint,
        };

        state.compositors.advanced.set_font_features(font_features);
        state
    }

    #[inline]
    pub fn compute_layout_resize(&mut self, width: u32, height: u32) {
        self.current.layout.resize(width, height).update();
        self.latest_change = SugarTreeDiff::Repaint;
    }

    #[inline]
    pub fn compute_layout_rescale(&mut self, scale: f32) {
        // In rescale case, we actually need to clean cache from the compositors
        // because it's based on sugarline hash which only consider the font size
        self.compositors.advanced.reset();
        self.current.layout.rescale(scale).update();
        self.current.layout.dimensions.height = 0.0;
        self.current.layout.dimensions.width = 0.0;
        self.latest_change = SugarTreeDiff::Repaint;
    }

    #[inline]
    pub fn compute_layout_font_size(&mut self, operation: u8) {
        let should_update = match operation {
            0 => self.current.layout.reset_font_size(),
            2 => self.current.layout.increase_font_size(),
            1 => self.current.layout.decrease_font_size(),
            _ => false,
        };

        if should_update {
            self.current.layout.update();
            self.current.layout.dimensions.height = 0.0;
            self.current.layout.dimensions.width = 0.0;
            self.latest_change = SugarTreeDiff::Repaint;
        }
    }

    #[inline]
    pub fn set_content(&mut self, new_content: Content) {
        if self.current.content != new_content {
            self.latest_change = SugarTreeDiff::Different;
        }
        self.current.content = new_content;
    }

    #[inline]
    pub fn set_fonts(&mut self, fonts: &FontLibrary) {
        self.compositors.advanced.set_fonts(fonts);
        self.current.layout.dimensions.height = 0.0;
        self.current.layout.dimensions.width = 0.0;
        self.latest_change = SugarTreeDiff::Repaint;
    }

    #[inline]
    pub fn set_font_features(&mut self, font_features: &Option<Vec<String>>) {
        self.compositors.advanced.set_font_features(font_features);
        self.latest_change = SugarTreeDiff::Repaint;
    }

    #[inline]
    pub fn mark_dirty(&mut self) {
        self.latest_change = SugarTreeDiff::Different;
    }

    #[inline]
    pub fn clean_screen(&mut self) {
        self.current.content.clear();
        self.current.objects.clear();
        self.compositors.advanced.reset();
    }

    #[inline]
    pub fn compute_objects(&mut self, new_objects: Vec<Object>) {
        if self.current.objects == new_objects {
            self.latest_change = SugarTreeDiff::Different;
        };

        // Block are used only with elementary renderer
        self.current.objects = new_objects;
    }

    #[inline]
    pub fn reset_compositor(&mut self) {
        self.compositors.elementary.clean();
        self.compositors.advanced.reset();
        self.latest_change = SugarTreeDiff::Equal;
    }

    #[inline]
    pub fn clean_compositor(&mut self) {
        self.compositors.elementary.clean();
        self.latest_change = SugarTreeDiff::Equal;
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
        #[cfg(not(feature = "always_dirty"))]
        if self.latest_change == SugarTreeDiff::Equal {
            self.compositors.advanced.clean();
            return false;
        }

        advance_brush.prepare(context, self, graphics);
        rect_brush.resize(context);

        // Elementary renderer is used for everything else in sugarloaf
        // like objects rendering (created by .text() or .append_rects())
        // ...
        // If current tree has objects and compositor has empty objects
        // It means that's either the first render or objects were erased on compute_diff() step
        for object in &self.current.objects {
            match object {
                Object::Text(text) => {
                    elementary_brush.queue(
                        &self
                            .compositors
                            .elementary
                            .create_section_from_text(text, &self.current),
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
            if dimension.height != self.current.layout.dimensions.height {
                self.current.layout.dimensions.height = dimension.height;
                tracing::info!("prepare_render: changed height... {}", dimension.height);
                dimensions_changed = true;
            }

            if dimension.width != self.current.layout.dimensions.width {
                self.current.layout.dimensions.width = dimension.width;
                self.current.layout.update_columns_per_font_width();
                tracing::info!("prepare_render: changed width... {}", dimension.width);
                dimensions_changed = true;
            }

            if dimensions_changed {
                self.current.layout.update();
                tracing::info!("sugar_state: dimensions has changed");
            }
        }
    }

    #[inline]
    pub fn compute_changes(&mut self) {
        // If sugar dimensions are empty then need to find it
        if self.current.layout.dimensions.width == 0.0
            || self.current.layout.dimensions.height == 0.0
        {
            self.compositors
                .advanced
                .calculate_dimensions(&self.current);

            self.compositors.advanced.update_layout(&self.current);

            self.latest_change = SugarTreeDiff::Repaint;
            tracing::info!("has empty dimensions, will try to find...");
            return;
        }

        let mut should_update = false;
        let mut should_compute_dimensions = false;

        match &self.latest_change {
            SugarTreeDiff::Equal => {
                // Do nothing
            }
            SugarTreeDiff::Repaint => {
                should_update = true;
                should_compute_dimensions = true;
            }
            SugarTreeDiff::Different => {
                should_update = true;
            }
        }

        tracing::info!("state compute_changes result: {:?}", self.latest_change);

        if should_update {
            if should_compute_dimensions {
                self.compositors
                    .advanced
                    .calculate_dimensions(&self.current);
            }

            self.compositors.advanced.update_layout(&self.current);
        }
    }
}

// TODO: Write tests for compute layout updates
