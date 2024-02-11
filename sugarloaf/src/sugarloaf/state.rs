use crate::sugarloaf::Rect;
use super::compositors::{SugarCompositorLevel, SugarCompositors};
use super::graphics::SugarloafGraphics;
use super::tree::{SugarTree, SugarTreeDiff};
use crate::sugarloaf::text;
use crate::sugarloaf::RichTextBrush;
use crate::sugarloaf::SugarloafLayout;
use crate::SugarBlock;
use ab_glyph::FontArc;

pub struct SugarState {
    pub current: SugarTree,
    pub next: SugarTree,
    latest_change: SugarTreeDiff,
    dimensions_changed: bool,
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
        let mut next = SugarTree::default();
        // First time computing changes should obtain dimensions
        next.layout = initial_layout;
        SugarState {
            compositors: SugarCompositors::default(),
            level,
            graphics: SugarloafGraphics::default(),
            current: SugarTree::default(),
            next,
            dimensions_changed: false,
            latest_change: SugarTreeDiff::Equal,
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
    pub fn set_fonts(&mut self, fonts: Vec<FontArc>) {
        self.compositors.elementary.set_fonts(fonts);
    }

    #[inline]
    pub fn process(&mut self, block: &mut SugarBlock) {
        match self.level {
            SugarCompositorLevel::Elementary => self
                .compositors
                .elementary
                .update_tree_with_block(block, &mut self.next),
            SugarCompositorLevel::Advanced => self
                .compositors
                .advanced
                .update_tree_with_block(block, &mut self.next),
        }
    }

    #[inline]
    pub fn reset_compositor(&mut self) {
        match self.level {
            SugarCompositorLevel::Elementary => self.compositors.elementary.reset(),
            SugarCompositorLevel::Advanced => self.compositors.advanced.reset(),
        }

        self.dimensions_changed = false;
    }

    #[inline]
    pub fn clean_compositor(&mut self) {
        match self.level {
            SugarCompositorLevel::Elementary => self.compositors.elementary.clean(),
            SugarCompositorLevel::Advanced => self.compositors.advanced.clean(),
        }

        self.dimensions_changed = false;
    }

    #[inline]
    pub fn compute_updates(&mut self, elementary_brush: &mut text::GlyphBrush<()>, rects_to_render: &mut Vec<Rect>) {
        if !self.level.is_advanced() {
            let (sections, rects) = self.compositors.elementary.render_data();
            for section in sections {
                elementary_brush.queue(section);
            }

            rects_to_render.extend(rects);
        }
    }

    #[inline]
    pub fn compute_dimensions(
        &mut self,
        advance_brush: &mut RichTextBrush,
        elementary_brush: &mut text::GlyphBrush<()>,
        context: &mut super::Context,
    ) {
        // If layout is different or current has empty dimensions
        // then current will flip with next and will try to obtain
        // the dimensions.
        self.compute_changes();
        
        if self.latest_change == SugarTreeDiff::LayoutIsDifferent
                || self.current_has_empty_dimensions() {
            
            if self.level.is_advanced() {
                if let Some((width, height)) = advance_brush.dimensions(&self) {
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
                        self.dimensions_changed = dimensions_changed;
                        log::info!("sugar_state: dimensions has changed");
                    }
                }

                advance_brush.prepare(context, &self);
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
    }

    #[inline]
    pub fn dimensions_changed(&self) -> bool {
        self.dimensions_changed
    }

    #[inline]
    pub fn reset_next(&mut self) {
        self.next.inner.clear();
        self.next.layout = self.current.layout;
    }

    #[inline]
    pub fn compute_changes(&mut self) {
        // If sugar dimensions are empty then need to find it
        if self.current_has_empty_dimensions() {
            std::mem::swap(&mut self.current, &mut self.next);

            if self.level.is_advanced() {
                self.compositors
                    .advanced
                    .calculate_dimensions(&self.current);
            }
            self.reset_next();
            return;
        }

        if !self.current.is_empty() {
            self.latest_change = self.current.calculate_diff(&self.next);
            match &self.latest_change {
                SugarTreeDiff::Equal => {
                    // Do nothing
                }
                SugarTreeDiff::LayoutIsDifferent => {
                    std::mem::swap(&mut self.current, &mut self.next);
                    if self.level.is_advanced() {
                        self.compositors
                            .advanced
                            .calculate_dimensions(&self.current);
                        self.compositors.advanced.update_data();
                        self.compositors.advanced.update_layout(&self.current);
                        self.compositors.advanced.update_size(&self.current);
                    }
                }
                SugarTreeDiff::Changes(_changes) => {
                    // for change in changes {
                    //     // println!("change {:?}", change);
                    //     if let Some(offs) = self.content.insert(0, change.after.content) {
                    //         // inserted = Some(offs);
                    //         println!("{:?}", offs);
                    //     }
                    // }
                    // std::mem::swap(&mut self.current, &mut self.next);
                    std::mem::swap(&mut self.current, &mut self.next);
                    if self.level.is_advanced() {
                        self.compositors.advanced.update_data();
                        self.compositors.advanced.update_layout(&self.current);
                        self.compositors.advanced.update_size(&self.current);
                    }
                    // println!("changes: {:?}", changes);
                }
                _ => {
                    std::mem::swap(&mut self.current, &mut self.next);
                    if self.level.is_advanced() {
                        self.compositors.advanced.update_data();
                        self.compositors.advanced.update_layout(&self.current);
                        self.compositors.advanced.update_size(&self.current);
                    }
                }
            }
        } else if !self.next.is_empty() {
            std::mem::swap(&mut self.current, &mut self.next);
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