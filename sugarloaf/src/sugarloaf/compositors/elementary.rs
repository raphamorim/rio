// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::components::text::glyph::{FontId, OwnedSection};

use crate::sugarloaf::graphics;
use crate::sugarloaf::tree::SugarTree;
use crate::sugarloaf::{PxScale, Rect, SugarText};
use fnv::FnvHashMap;

#[derive(Copy, Clone, PartialEq)]
pub struct CachedSugar {
    font_id: FontId,
    char_width: f32,
    px_scale: Option<PxScale>,
}

#[allow(unused)]
struct GraphicRect {
    id: graphics::SugarGraphicId,
    height: u16,
    width: u16,
    pos_x: f32,
    pos_y: f32,
    columns: f32,
    start_row: f32,
    end_row: f32,
}

#[derive(Default)]
pub struct Elementary {
    sugar_cache: FnvHashMap<char, CachedSugar>,
    pub rects: Vec<Rect>,
    pub blocks_rects: Vec<Rect>,
    pub sections: Vec<OwnedSection>,
    pub blocks_sections: Vec<OwnedSection>,
    pub should_resize: bool,
    text_y: f32,
    current_row: u16,
    graphic_rects: FnvHashMap<crate::SugarGraphicId, GraphicRect>,
}

impl Elementary {
    #[inline]
    pub fn rects(&mut self) -> &Vec<Rect> {
        self.rects.extend(&self.blocks_rects);
        &self.rects
    }

    #[inline]
    pub fn clean_blocks(&mut self) {
        self.blocks_sections.clear();
        self.blocks_rects.clear();
    }

    #[inline]
    pub fn blocks_are_empty(&self) -> bool {
        self.blocks_sections.is_empty() && self.blocks_rects.is_empty()
    }

    #[inline]
    pub fn set_should_resize(&mut self) {
        self.should_resize = true;
    }

    #[inline]
    pub fn extend_block_rects(&mut self, rects: &Vec<Rect>) {
        self.blocks_rects.extend(rects);
    }

    #[inline]
    pub fn reset(&mut self) {
        // Clean font cache per instance
        self.sugar_cache.clear();
        self.clean();
    }

    #[inline]
    pub fn clean(&mut self) {
        self.rects.clear();
        self.sections.clear();
        self.graphic_rects.clear();
        self.current_row = 0;
        self.text_y = 0.0;
        self.should_resize = false;
    }

    #[inline]
    pub fn create_section_from_text(
        &mut self,
        sugar_text: &SugarText,
        tree: &SugarTree,
    ) -> &OwnedSection {
        let text = crate::components::text::OwnedText {
            text: sugar_text.content.to_owned(),
            scale: PxScale::from(sugar_text.font_size * tree.layout.dimensions.scale),
            font_id: crate::components::text::FontId(0),
            extra: crate::components::text::Extra {
                color: sugar_text.color,
                z: 0.0,
            },
        };

        let layout = if sugar_text.single_line {
            crate::components::text::glyph::Layout::default_single_line()
                .v_align(crate::components::text::glyph::VerticalAlign::Center)
                .h_align(crate::components::text::glyph::HorizontalAlign::Left)
        } else {
            crate::components::text::glyph::Layout::default()
                .v_align(crate::components::text::glyph::VerticalAlign::Center)
                .h_align(crate::components::text::glyph::HorizontalAlign::Left)
        };

        let section = crate::components::text::OwnedSection {
            screen_position: (
                sugar_text.position.0 * tree.layout.dimensions.scale,
                sugar_text.position.1 * tree.layout.dimensions.scale,
            ),
            bounds: (tree.layout.width, tree.layout.height),
            text: vec![text],
            layout,
        };

        self.blocks_sections.push(section);

        &self.blocks_sections[self.blocks_sections.len() - 1]
    }
}
