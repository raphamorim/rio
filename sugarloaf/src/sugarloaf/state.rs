// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use super::compositors::{SugarCompositorLevel, SugarCompositors};
use super::graphics::SugarloafGraphics;
use super::tree::{SugarTree, SugarTreeDiff};
use crate::font::FontLibrary;
use crate::sugarloaf::{text, RectBrush, RichTextBrush, SugarloafLayout};
use crate::{SugarBlock, SugarLine};

pub struct SugarState {
    pub current: SugarTree,
    pub next: SugarTree,
    latest_change: SugarTreeDiff,
    dimensions_changed: bool,
    current_line: usize,
    pub compositors: SugarCompositors,
    level: SugarCompositorLevel,
    // TODO: Decide if graphics should be in SugarTree or SugarState
    pub graphics: SugarloafGraphics,
}

impl SugarState {
    pub fn new(
        level: SugarCompositorLevel,
        initial_layout: SugarloafLayout,
    ) -> SugarState {
        // First time computing changes should obtain dimensions
        let next = SugarTree {
            layout: initial_layout,
            ..Default::default()
        };
        SugarState {
            current_line: 0,
            compositors: SugarCompositors::default(),
            level,
            graphics: SugarloafGraphics::default(),
            current: SugarTree::default(),
            next,
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
        self.next.layout.rescale(scale).update();
    }

    #[inline]
    pub fn compute_line_start(&mut self) {
        self.next.lines.push(SugarLine::default());
        self.current_line = self.next.lines.len() - 1;
    }

    #[inline]
    pub fn compute_line_end(&mut self) {
        match self.level {
            SugarCompositorLevel::Elementary => self
                .compositors
                .elementary
                .update_tree_with_new_line(self.current_line, &self.next),
            SugarCompositorLevel::Advanced => self
                .compositors
                .advanced
                .update_tree_with_new_line(self.current_line, &self.next),
        }
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
    pub fn set_fonts(&mut self, fonts: FontLibrary) {
        self.compositors.elementary.set_fonts(fonts.font_arcs());
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
        if self.latest_change == SugarTreeDiff::Equal {
            return false;
        }

        // let start = std::time::Instant::now();

        if self.level.is_advanced() {
            advance_brush.prepare(context, self);
        } else {
            for section in &self.compositors.elementary.sections {
                elementary_brush.queue(section);
            }
        }

        for section in &self.compositors.elementary.blocks_sections {
            elementary_brush.queue(section);
            elementary_brush.keep_cached(section);
        }

        if self.compositors.elementary.should_resize {
            rect_brush.resize(context);
        }

        // let duration = start.elapsed();
        // println!(
        //     "Time elapsed in state.compute_updates() is: {:?} \n",
        //     duration
        // );

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
    pub fn compute_dimensions(
        &mut self,
        advance_brush: &mut RichTextBrush,
        elementary_brush: &mut text::GlyphBrush<()>,
    ) {
        // If layout is different or current has empty dimensions
        // then current will flip with next and will try to obtain
        // the dimensions.

        if self.latest_change != SugarTreeDiff::LayoutIsDifferent
        // || !self.current_has_empty_dimensions()
        {
            return;
        }

        if self.level.is_advanced() {
            if let Some((width, height)) = advance_brush.dimensions(self) {
                let mut dimensions_changed = false;
                if height != self.current.layout.dimensions.height {
                    self.current.layout.dimensions.height = height;
                    log::info!("prepare_render: changed height... {}", height);
                    dimensions_changed = true;
                }

                if width != self.current.layout.dimensions.width {
                    self.current.layout.dimensions.width = width;
                    self.current.layout.update_columns_per_font_width();
                    log::info!("prepare_render: changed width... {}", width);
                    dimensions_changed = true;
                }

                if dimensions_changed {
                    self.current.layout.update();
                    self.next.layout = self.current.layout;
                    self.dimensions_changed = true;
                    log::info!("sugar_state: dimensions has changed");
                }
            }
        } else {
            let font_bound = self.compositors.elementary.calculate_dimensions(
                ' ',
                crate::font::FONT_ID_REGULAR,
                &self.current,
                elementary_brush,
            );

            let mut dimensions_changed = false;
            if font_bound.0 != self.current.layout.dimensions.width {
                dimensions_changed = true;
                self.current.layout.dimensions.width = font_bound.0;
                self.current.layout.update_columns_per_font_width();
            }

            if font_bound.1 != self.current.layout.dimensions.height {
                dimensions_changed = true;
                self.current.layout.dimensions.height = font_bound.1;
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
    }

    #[inline]
    pub fn compute_changes(&mut self) {
        // If sugar dimensions are empty then need to find it
        if self.current_has_empty_dimensions() {
            std::mem::swap(&mut self.current, &mut self.next);

            if self.level.is_advanced() {
                self.compositors.advanced.calculate_dimensions(&self.next);
            }

            self.compositors.elementary.set_should_resize();
            return;
        }

        let mut should_update = false;
        let mut should_clean_blocks = false;
        let mut should_resize = false;
        let mut should_compute_dimensions = false;

        self.latest_change = self.current.calculate_diff(&self.next);
        match &self.latest_change {
            SugarTreeDiff::Equal => {
                // Do nothing
            }
            SugarTreeDiff::LayoutIsDifferent => {
                should_update = true;
                should_compute_dimensions = true;
                should_clean_blocks = true;
                should_resize = true;
                println!("LayoutIsDifferent");
            }
            SugarTreeDiff::BlocksAreDifferent => {
                should_clean_blocks = true;
                should_update = true;
            }
            SugarTreeDiff::ColumnsLengthIsDifferent(_different) => {
                println!("ColumnsLengthIsDifferent");
                should_update = true;
            }
            SugarTreeDiff::Changes(_changes) => {
                println!("Changes");
                should_update = true;
            }
            _ => {
                println!("should_update");
                should_update = true;
            }
        }

        println!("{:?}", self.latest_change);

        if should_update {
            std::mem::swap(&mut self.current, &mut self.next);

            if should_compute_dimensions {
                self.compositors
                    .advanced
                    .calculate_dimensions(&self.current);
            }

            if self.level.is_advanced() {
                self.compositors.advanced.update_data();
                self.compositors.advanced.update_layout(&self.current);
                self.compositors.advanced.update_size(&self.current);
            }
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
