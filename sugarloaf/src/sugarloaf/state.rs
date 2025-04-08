// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::font::FontLibrary;
use crate::layout::RootStyle;
use crate::sugarloaf::{QuadBrush, RectBrush, RichTextBrush, RichTextLayout};
use crate::{ComposedQuad, Content, Object, Rect, RichText, SugarDimensions};
use std::collections::HashSet;

// Layer points for each rect, quad or rt that will be used on that
// particular scene.
#[derive(Default, Debug)]
pub struct Layer {
    pub quads: Vec<usize>,
    pub rects: Vec<usize>,
    pub rich_texts: Vec<usize>,
}

pub struct SugarState {
    objects: Vec<Object>,
    pub rich_texts: Vec<RichText>,
    rich_text_repaint: HashSet<usize>,
    rich_text_to_be_removed: Vec<usize>,
    pub style: RootStyle,
    pub content: Content,
    pub rects: Vec<Rect>,
    pub quads: Vec<ComposedQuad>,
    pub layers: Vec<Layer>,
}

impl SugarState {
    pub fn new(
        style: RootStyle,
        font_library: &FontLibrary,
        font_features: &Option<Vec<String>>,
    ) -> SugarState {
        let mut content = Content::new(font_library);
        let found_font_features = SugarState::found_font_features(font_features);
        content.set_font_features(found_font_features);

        SugarState {
            layers: vec![Layer::default()],
            content: Content::new(font_library),
            rects: vec![],
            quads: vec![],
            style,
            objects: vec![],
            rich_texts: vec![],
            rich_text_to_be_removed: vec![],
            rich_text_repaint: HashSet::default(),
        }
    }

    pub fn found_font_features(
        font_features: &Option<Vec<String>>,
    ) -> Vec<crate::font_introspector::Setting<u16>> {
        let mut found_font_features = vec![];
        if let Some(features) = font_features {
            for feature in features {
                let setting: crate::font_introspector::Setting<u16> =
                    (feature.as_str(), 1).into();
                found_font_features.push(setting);
            }
        }

        found_font_features
    }

    #[inline]
    pub fn get_state_layout(&self, id: &usize) -> RichTextLayout {
        if let Some(builder_state) = self.content.get_state(id) {
            return builder_state.layout;
        }

        RichTextLayout::from_default_layout(&self.style)
    }

    #[inline]
    pub fn get_layer_quads(&self, layer_index: usize) -> Vec<ComposedQuad> {
        self.layers.get(layer_index).map_or_else(Vec::new, |layer| {
            layer
                .quads
                .iter()
                .filter_map(|&idx| self.quads.get(idx).copied())
                .collect()
        })
    }

    #[inline]
    pub fn get_layer_rich_texts(&self, layer_index: usize) -> Vec<RichText> {
        self.layers.get(layer_index).map_or_else(Vec::new, |layer| {
            layer
                .rich_texts
                .iter()
                .filter_map(|&idx| self.rich_texts.get(idx).copied())
                .collect()
        })
    }

    #[inline]
    pub fn get_layer_rects(&self, layer_index: usize) -> Vec<Rect> {
        self.layers.get(layer_index).map_or_else(Vec::new, |layer| {
            layer
                .rects
                .iter()
                .filter_map(|&idx| self.rects.get(idx).copied())
                .collect()
        })
    }

    #[inline]
    pub fn compute_layout_rescale(
        &mut self,
        scale: f32,
        advance_brush: &mut RichTextBrush,
    ) {
        self.style.scale_factor = scale;
        for (id, state) in &mut self.content.states {
            state.rescale(scale);
            state.layout.dimensions.height = 0.0;
            state.layout.dimensions.width = 0.0;

            self.rich_text_repaint.insert(*id);
        }

        self.process_rich_text_repaint(advance_brush);
    }

    #[inline]
    pub fn set_rich_text_font_size(
        &mut self,
        rich_text_id: &usize,
        font_size: f32,
        advance_brush: &mut RichTextBrush,
    ) {
        if let Some(rte) = self.content.get_state_mut(rich_text_id) {
            rte.layout.font_size = font_size;
            rte.update_font_size();

            rte.layout.dimensions.height = 0.0;
            rte.layout.dimensions.width = 0.0;
            self.rich_text_repaint.insert(*rich_text_id);
        }

        self.process_rich_text_repaint(advance_brush);
    }

    #[inline]
    pub fn set_rich_text_font_size_based_on_action(
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
                self.rich_text_repaint.insert(*rich_text_id);
            }
        }

        self.process_rich_text_repaint(advance_brush);
    }

    #[inline]
    pub fn set_rich_text_line_height(&mut self, rich_text_id: &usize, line_height: f32) {
        if let Some(rte) = self.content.get_state_mut(rich_text_id) {
            rte.layout.line_height = line_height;
        }
    }

    fn process_rich_text_repaint(&mut self, advance_brush: &mut RichTextBrush) {
        for rich_text in &self.rich_text_repaint {
            self.content.update_dimensions(rich_text, advance_brush);
        }

        self.rich_text_repaint.clear();
    }

    #[inline]
    pub fn set_fonts(&mut self, fonts: &FontLibrary, advance_brush: &mut RichTextBrush) {
        self.content.set_font_library(fonts);
        for (id, state) in &mut self.content.states {
            state.layout.dimensions.height = 0.0;
            state.layout.dimensions.width = 0.0;
            self.rich_text_repaint.insert(*id);
        }

        self.process_rich_text_repaint(advance_brush);
    }

    #[inline]
    pub fn set_font_features(&mut self, font_features: &Option<Vec<String>>) {
        let found_font_features = SugarState::found_font_features(font_features);
        self.content.set_font_features(found_font_features);
    }

    #[inline]
    pub fn clean_screen(&mut self) {
        // self.content.clear();
        self.objects.clear();
        self.layers.clear();
        self.layers.push(Layer::default());
    }

    #[inline]
    pub fn compute_objects(&mut self, new_objects: Vec<Object>) {
        // Block are used only with elementary renderer
        let mut rich_texts: Vec<RichText> = vec![];
        let len = self.layers.len() - 1;
        for obj in &new_objects {
            if let Object::NewLayer = obj {
                self.layers.push(Layer::default());
                continue;
            }

            if let Object::RichText(rich_text, layer) = obj {
                rich_texts.push(*rich_text);

                if let Some(idx) = layer {
                    if let Some(layer) = self.layers.get_mut(*idx) {
                        layer.rich_texts.push(rich_texts.len() - 1);
                    }
                } else {
                    self.layers[len].rich_texts.push(rich_texts.len() - 1);
                }
            }
        }
        self.objects = new_objects;
        self.rich_texts = rich_texts
    }

    #[inline]
    pub fn reset(&mut self) {
        self.rects.clear();
        self.quads.clear();
        for rte_id in &self.rich_text_to_be_removed {
            self.content.remove_state(rte_id);
        }

        self.rich_text_to_be_removed.clear();

        self.layers.clear();
        self.layers.push(Layer::default());
    }

    #[inline]
    pub fn clear_rich_text(&mut self, id: &usize) {
        self.content.clear_state(id);
    }

    #[inline]
    pub fn create_rich_text(&mut self) -> usize {
        self.content
            .create_state(&RichTextLayout::from_default_layout(&self.style))
    }

    #[inline]
    pub fn create_temp_rich_text(&mut self) -> usize {
        let id = self
            .content
            .create_state(&RichTextLayout::from_default_layout(&self.style));
        self.rich_text_to_be_removed.push(id);
        id
    }

    pub fn content(&mut self) -> &mut Content {
        &mut self.content
    }

    #[inline]
    pub fn compute_updates(
        &mut self,
        rect_brush: &mut RectBrush,
        quad_brush: &mut QuadBrush,
        context: &mut super::Context,
    ) {
        let len = self.layers.len() - 1;
        rect_brush.resize(context);
        quad_brush.resize(context);

        // Elementary renderer is used for everything else in sugarloaf
        // like objects rendering (created by .text() or .append_rects())
        // ...
        // If current tree has objects and compositor has empty objects
        // It means that's either the first render or objects were erased on compute_diff() step
        for object in &self.objects {
            match object {
                Object::Rect(rect, layer) => {
                    self.rects.push(*rect);

                    if let Some(idx) = layer {
                        if let Some(layer) = self.layers.get_mut(*idx) {
                            layer.rects.push(self.rects.len() - 1);
                        }
                    } else {
                        self.layers[len].rects.push(self.rects.len() - 1);
                    }
                }
                Object::Quad(composed_quad, layer) => {
                    self.quads.push(*composed_quad);

                    if let Some(idx) = layer {
                        if let Some(layer) = self.layers.get_mut(*idx) {
                            layer.quads.push(self.quads.len() - 1);
                        }
                    } else {
                        self.layers[len].quads.push(self.quads.len() - 1);
                    }
                }
                // rich texts and layers have been already computed
                _ => {}
            }
        }

        println!("{:?}", self.layers);
    }

    #[inline]
    pub fn get_rich_text_dimensions(
        &mut self,
        id: &usize,
        advance_brush: &mut RichTextBrush,
    ) -> SugarDimensions {
        self.content.update_dimensions(id, advance_brush);
        if let Some(rte) = self.content.get_state(id) {
            return rte.layout.dimensions;
        }

        SugarDimensions::default()
    }

    #[inline]
    pub fn compute_dimensions(&mut self, advance_brush: &mut RichTextBrush) {
        // If sugar dimensions are empty then need to find it
        for rich_text in &self.rich_texts {
            if let Some(rte) = self.content.get_state(&rich_text.id) {
                if rte.layout.dimensions.width == 0.0
                    || rte.layout.dimensions.height == 0.0
                {
                    self.rich_text_repaint.insert(rich_text.id);

                    tracing::info!("has empty dimensions, will try to find...");
                }
            }
        }

        if self.rich_text_repaint.is_empty() {
            return;
        }
        for rich_text in &self.rich_text_repaint {
            self.content.update_dimensions(rich_text, advance_brush);
        }

        self.rich_text_repaint.clear();
    }
}
