// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::components::text::glyph::OwnedSection;
use crate::sugarloaf::Context;
use crate::sugarloaf::RootStyle;
use crate::sugarloaf::{PxScale, Rect};
use crate::{ComposedQuad, Text};

#[derive(Default)]
pub struct Elementary {
    pub rects: Vec<Rect>,
    pub quads: Vec<ComposedQuad>,
    text_y: f32,
    current_row: u16,
}

impl Elementary {
    #[inline]
    pub fn rects(&mut self) -> &Vec<Rect> {
        &self.rects
    }

    #[inline]
    pub fn clean(&mut self) {
        self.current_row = 0;
        self.text_y = 0.0;
        self.rects.clear();
        self.quads.clear();
    }

    #[inline]
    pub fn create_section_from_text(
        &mut self,
        sugar_text: &Text,
        context: &mut Context,
        style: &RootStyle,
    ) -> OwnedSection {
        let text = crate::components::text::OwnedText {
            text: sugar_text.content.to_owned(),
            scale: PxScale::from(sugar_text.font_size * style.scale_factor),
            font_id: crate::components::text::FontId(0),
            extra: crate::components::text::Extra {
                color: sugar_text.color,
                z: 0.0,
            },
        };

        let text_layout = if sugar_text.single_line {
            crate::components::text::glyph::Layout::default_single_line()
                .v_align(crate::components::text::glyph::VerticalAlign::Center)
                .h_align(crate::components::text::glyph::HorizontalAlign::Left)
        } else {
            crate::components::text::glyph::Layout::default()
                .v_align(crate::components::text::glyph::VerticalAlign::Center)
                .h_align(crate::components::text::glyph::HorizontalAlign::Left)
        };

        crate::components::text::OwnedSection {
            screen_position: (
                sugar_text.position.0 * style.scale_factor,
                sugar_text.position.1 * style.scale_factor,
            ),
            bounds: (context.size.width, context.size.height),
            text: vec![text],
            layout: text_layout,
        }
    }
}
