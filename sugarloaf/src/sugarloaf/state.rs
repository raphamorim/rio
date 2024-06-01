// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use super::compositors::SugarCompositors;
use super::graphics::SugarloafGraphics;
use super::tree::{SugarTree, SugarTreeDiff};
use crate::font::FontLibrary;
use crate::sugarloaf::{text, RectBrush, RichTextBrush, SugarloafLayout};
use crate::{SugarBlock, SugarLine};

pub struct SugarState {
    pub current: Box<SugarTree>,
    pub next: Box<SugarTree>,
    latest_change: SugarTreeDiff,
    dimensions_changed: bool,
    current_line: usize,
    pub is_dirty: bool,
    pub compositors: SugarCompositors,
    // TODO: Decide if graphics should be in SugarTree or SugarState
    pub graphics: SugarloafGraphics,
}

impl SugarState {
    pub fn new(
        initial_layout: SugarloafLayout,
        font_library: &FontLibrary,
    ) -> SugarState {
        // First time computing changes should obtain dimensions
        let next = SugarTree {
            layout: initial_layout,
            ..Default::default()
        };
        SugarState {
            is_dirty: false,
            current_line: 0,
            compositors: SugarCompositors::new(font_library),
            graphics: SugarloafGraphics::default(),
            current: Box::<SugarTree>::default(),
            next: Box::new(next),
            dimensions_changed: false,
            latest_change: SugarTreeDiff::LayoutIsDifferent,
        }
    }

    #[inline]
    pub fn compute_layout_resize(&mut self, width: u32, height: u32) {
        self.next.layout.resize(width, height).update();
    }

    #[inline]
    pub fn compute_layout_rescale(&mut self, scale: f32) {
        // In rescale case, we actually need to clean cache from the compositors
        // because it's based on sugarline hash which only consider the font size
        self.compositors.advanced.clean();
        self.next.layout.rescale(scale).update();
    }

    #[inline]
    pub fn compute_layout_font_size(&mut self, operation: u8) {
        let should_update = match operation {
            0 => self.next.layout.reset_font_size(),
            2 => self.next.layout.increase_font_size(),
            1 => self.next.layout.decrease_font_size(),
            _ => false,
        };

        if should_update {
            self.next.layout.update();
        }
    }

    #[inline]
    pub fn compute_line_start(&mut self) {
        self.next.lines.push(SugarLine::default());
        self.current_line = self.next.lines.len() - 1;
    }

    #[inline]
    pub fn compute_line_end(&mut self) {
        self.next.lines[self.current_line].mark_hash_key();
        self.compositors
            .advanced
            .update_tree_with_new_line(self.current_line, &self.next);
    }

    #[inline]
    pub fn insert_on_current_line(&mut self, sugar: &crate::Sugar) {
        self.next.lines[self.current_line].insert(sugar);
    }

    #[inline]
    pub fn insert_on_current_line_from_vec(&mut self, sugar_vec: &Vec<&crate::Sugar>) {
        for sugar in sugar_vec {
            self.next.lines[self.current_line].insert(sugar);
        }
    }

    #[inline]
    pub fn insert_on_current_line_from_vec_owned(
        &mut self,
        sugar_vec: &Vec<crate::Sugar>,
    ) {
        for sugar in sugar_vec {
            self.next.lines[self.current_line].insert(sugar);
        }
    }

    #[inline]
    pub fn set_fonts(&mut self, fonts: &FontLibrary) {
        self.compositors.advanced.set_fonts(fonts);
    }

    #[inline]
    pub fn compute_block(&mut self, block: SugarBlock) {
        // Block are used only with elementary renderer
        self.next.blocks.push(block);
    }

    #[inline]
    pub fn reset_compositor(&mut self) {
        // if self.level.is_advanced() {
        //     self.compositors.advanced.reset();
        // }

        self.compositors.elementary.reset();
        self.dimensions_changed = false;
    }

    #[inline]
    pub fn clean_compositor(&mut self) {
        // if self.level.is_advanced() {
        //     self.compositors.advanced.clean();
        // }

        self.compositors.elementary.clean();
        self.dimensions_changed = false;
    }

    #[inline]
    pub fn compute_updates(
        &mut self,
        advance_brush: &mut RichTextBrush,
        elementary_brush: &mut text::GlyphBrush<()>,
        rect_brush: &mut RectBrush,
        context: &mut super::Context,
    ) -> bool {
        advance_brush.prepare(context, self);

        for section in &self.compositors.elementary.blocks_sections {
            elementary_brush.queue(section);
        }

        if self.compositors.elementary.should_resize {
            rect_brush.resize(context);
        }

        // Elementary renderer is used for everything else in sugarloaf
        // like blocks rendering (created by .text() or .append_rects())
        // ...
        // If current tree has blocks and compositor has empty blocks
        // It means that's either the first render or blocks were erased on compute_diff() step
        if !self.current.blocks.is_empty()
            && self.compositors.elementary.blocks_are_empty()
        {
            for block in &self.current.blocks {
                if let Some(text) = &block.text {
                    elementary_brush.queue(
                        self.compositors
                            .elementary
                            .create_section_from_text(text, &self.current),
                    );
                }

                if !block.rects.is_empty() {
                    self.compositors.elementary.extend_block_rects(&block.rects);
                }
            }
        }

        // Add block rects to main rects
        self.compositors
            .elementary
            .rects
            .extend(&self.compositors.elementary.blocks_rects);

        true
    }

    #[inline]
    pub fn layout_was_updated(&self) -> bool {
        self.latest_change == SugarTreeDiff::LayoutIsDifferent
    }

    #[inline]
    pub fn compute_dimensions(&mut self, advance_brush: &mut RichTextBrush) {
        // If layout is different or current has empty dimensions
        // then current will flip with next and will try to obtain
        // the dimensions.

        if !self.layout_was_updated() {
            return;
        }

        if let Some(dimension) = advance_brush.dimensions(self) {
            let mut dimensions_changed = false;
            if dimension.height != self.current.layout.dimensions.height {
                self.current.layout.dimensions.height = dimension.height;
                log::info!("prepare_render: changed height... {}", dimension.height);
                dimensions_changed = true;
            }

            if dimension.width != self.current.layout.dimensions.width {
                self.current.layout.dimensions.width = dimension.width;
                self.current.layout.update_columns_per_font_width();
                log::info!("prepare_render: changed width... {}", dimension.width);
                dimensions_changed = true;
            }

            if dimensions_changed {
                self.current.layout.update();
                self.next.layout = self.current.layout;
                self.dimensions_changed = true;
                log::info!("sugar_state: dimensions has changed");
            }
        }
    }

    #[inline]
    pub fn dimensions_changed(&self) -> bool {
        self.dimensions_changed
    }

    #[inline]
    pub fn reset_next(&mut self) {
        self.next.layout = self.current.layout;
        self.current_line = 0;
        self.next.lines.clear();
        self.next.blocks.clear();
        self.is_dirty = false;
    }

    #[inline]
    pub fn compute_changes(&mut self) {
        // If sugar dimensions are empty then need to find it
        if self.current_has_empty_dimensions() {
            self.current = Box::new(std::mem::take(&mut self.next));

            self.compositors
                .advanced
                .calculate_dimensions(&self.current);

            self.compositors.elementary.set_should_resize();
            self.reset_next();
            self.latest_change = SugarTreeDiff::LayoutIsDifferent;
            log::info!("current_has_empty_dimensions, will try to find...");
            return;
        }

        let mut should_update = false;
        let mut should_clean_blocks = false;
        let mut should_resize = false;
        let mut should_compute_dimensions = false;

        self.latest_change =
            self.current
                .calculate_diff(&self.next, false, self.is_dirty);
        match &self.latest_change {
            SugarTreeDiff::Equal => {
                // Do nothing
            }
            SugarTreeDiff::LayoutIsDifferent => {
                should_update = true;
                should_compute_dimensions = true;
                should_clean_blocks = true;
                should_resize = true;
            }
            SugarTreeDiff::BlocksAreDifferent => {
                should_clean_blocks = true;
                should_update = true;
            }
            SugarTreeDiff::LineQuantity(_) => {
                should_update = true;
                should_compute_dimensions = true;
            }
            SugarTreeDiff::Different => {
                should_update = true;
            }
            SugarTreeDiff::Changes(_changes) => {
                should_update = true;
            }
        }

        log::info!("state compute_changes result: {:?}", self.latest_change);

        if should_update {
            self.current = std::mem::take(&mut self.next);

            if should_compute_dimensions {
                self.compositors
                    .advanced
                    .calculate_dimensions(&self.current);
            }

            self.compositors.advanced.update_layout(&self.current);
        }

        if should_clean_blocks {
            self.compositors.elementary.clean_blocks();
        }

        if should_resize {
            self.compositors.elementary.set_should_resize();
        }

        self.reset_next();
    }

    #[inline]
    pub fn current_has_empty_dimensions(&self) -> bool {
        self.current.layout.dimensions.width == 0.0
            || self.current.layout.dimensions.height == 0.0
    }
}

// TODO: Write tests for compute layout updates
