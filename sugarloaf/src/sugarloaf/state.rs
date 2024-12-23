// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use super::compositors::SugarCompositors;
use crate::font::FontLibrary;
use crate::layout::RootStyle;
use crate::sugarloaf::{text, QuadBrush, RectBrush, RichTextBrush, RichTextLayout};
use crate::SugarDimensions;
use crate::{Content, Graphics, Object, RichText};
use std::collections::HashSet;

pub struct SugarState {
    objects: Vec<Object>,
    pub rich_texts: Vec<RichText>,
    rich_text_repaint: HashSet<usize>,
    pub style: RootStyle,
    pub compositors: SugarCompositors,
}

impl SugarState {
    pub fn new(
        style: RootStyle,
        font_library: &FontLibrary,
        font_features: &Option<Vec<String>>,
    ) -> SugarState {
        let mut state = SugarState {
            compositors: SugarCompositors::new(font_library),
            style,
            objects: vec![],
            rich_texts: vec![],
            rich_text_repaint: HashSet::default(),
        };

        state.compositors.advanced.set_font_features(font_features);
        state
    }

    #[inline]
    pub fn get_state_layout(&self, id: &usize) -> RichTextLayout {
        if let Some(builder_state) = self.compositors.advanced.content.get_state(id) {
            return builder_state.layout;
        }

        RichTextLayout::from_default_layout(&self.style)
    }

    #[inline]
    pub fn compute_layout_rescale(
        &mut self,
        scale: f32,
        advance_brush: &mut RichTextBrush,
    ) {
        self.style.scale_factor = scale;
        for (id, state) in &mut self.compositors.advanced.content.states {
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
        if let Some(rte) = self
            .compositors
            .advanced
            .content
            .get_state_mut(rich_text_id)
        {
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
        if let Some(rte) = self
            .compositors
            .advanced
            .content
            .get_state_mut(rich_text_id)
        {
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
        if let Some(rte) = self
            .compositors
            .advanced
            .content
            .get_state_mut(rich_text_id)
        {
            rte.layout.line_height = line_height;
        }
    }

    fn process_rich_text_repaint(&mut self, advance_brush: &mut RichTextBrush) {
        for rich_text in &self.rich_text_repaint {
            self.compositors
                .advanced
                .content
                .update_dimensions(rich_text, advance_brush);
        }

        self.rich_text_repaint.clear();
    }

    #[inline]
    pub fn set_fonts(&mut self, fonts: &FontLibrary, advance_brush: &mut RichTextBrush) {
        self.compositors.advanced.set_fonts(fonts);
        for (id, state) in &mut self.compositors.advanced.content.states {
            state.layout.dimensions.height = 0.0;
            state.layout.dimensions.width = 0.0;
            self.rich_text_repaint.insert(*id);
        }

        self.process_rich_text_repaint(advance_brush);
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
    }

    #[inline]
    pub fn clear_rich_text(&mut self, id: &usize) {
        self.compositors.advanced.clear_rich_text(id);
    }

    #[inline]
    pub fn create_rich_text(&mut self) -> usize {
        self.compositors
            .advanced
            .create_rich_text(&RichTextLayout::from_default_layout(&self.style))
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
                        &self.compositors.elementary.create_section_from_text(
                            text,
                            context,
                            &self.style,
                        ),
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
    pub fn get_rich_text_dimensions(
        &mut self,
        id: &usize,
        advance_brush: &mut RichTextBrush,
    ) -> SugarDimensions {
        self.compositors
            .advanced
            .content
            .update_dimensions(id, advance_brush);
        if let Some(rte) = self.compositors.advanced.content.get_state(id) {
            return rte.layout.dimensions;
        }

        SugarDimensions::default()
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
                    self.rich_text_repaint.insert(rich_text.id);

                    tracing::info!("has empty dimensions, will try to find...");
                }
            }
        }

        if self.rich_text_repaint.is_empty() {
            return;
        }
        for rich_text in &self.rich_text_repaint {
            self.compositors
                .advanced
                .content
                .update_dimensions(rich_text, advance_brush);
        }

        self.rich_text_repaint.clear();
    }
}
