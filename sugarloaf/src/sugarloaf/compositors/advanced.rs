// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// build_complex_content and update_layout was originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE

use crate::font::FontLibrary;
use crate::layout::{Content, FragmentStyle, LayoutContext, RenderData};
use crate::sugarloaf::state::SugarTree;

pub struct Advanced {
    pub render_data: RenderData,
    pub mocked_render_data: RenderData,
    content: Content,
    layout_context: LayoutContext,
}

impl Advanced {
    pub fn new(font_library: &FontLibrary) -> Self {
        Self {
            layout_context: LayoutContext::new(font_library),
            content: Content::default(),
            render_data: RenderData::new(),
            mocked_render_data: RenderData::new(),
        }
    }

    #[inline]
    pub fn reset(&mut self) {
        self.content = Content::default();
        self.render_data = RenderData::default();
        self.layout_context.clear_cache();
    }

    #[inline]
    pub fn clean(&mut self) {
        self.content = Content::default();
        self.render_data = RenderData::default();
    }

    #[inline]
    pub fn font_library(&self) -> &FontLibrary {
        self.layout_context.font_library()
    }

    #[inline]
    pub fn set_fonts(&mut self, fonts: &FontLibrary) {
        self.layout_context = LayoutContext::new(fonts);
    }

    #[inline]
    pub fn set_font_features(&mut self, font_features: &Option<Vec<String>>) {
        let mut found_font_features = vec![];
        if let Some(features) = font_features {
            for feature in features {
                let setting: swash::Setting<u16> = (feature.as_str(), 1).into();
                found_font_features.push(setting);
            }
        }

        self.layout_context.set_font_features(found_font_features);
    }

    #[inline]
    pub fn update_layout(&mut self, tree: &SugarTree) {
        // let start = std::time::Instant::now();
        self.render_data = RenderData::default();

        let mut lb = self
            .layout_context
            .builder(tree.layout.dimensions.scale, tree.layout.font_size);
        tree.content.layout(&mut lb);
        self.render_data.clear();
        lb.build_into(&mut self.render_data);
        self.render_data
            .break_lines()
            .break_without_advance_or_alignment();

        // let duration = start.elapsed();
        // println!(" - advanced::update_layout() is: {:?}", duration);
    }

    #[inline]
    pub fn calculate_dimensions(&mut self, tree: &SugarTree) {
        let mut content_builder = Content::builder();
        let style = FragmentStyle {
            ..Default::default()
        };
        // content_builder.enter_span(&[
        //     SpanStyle::FontId(0),
        //     SpanStyle::Size(tree.layout.font_size),
        //     // S::features(&[("dlig", 1).into(), ("hlig", 1).into()][..]),
        // ]);
        content_builder.add_char(' ', style);

        let mut lb = self
            .layout_context
            .builder(tree.layout.dimensions.scale, tree.layout.font_size);
        let content = content_builder.build_ref();
        content.layout(&mut lb);
        self.mocked_render_data.clear();
        lb.build_into(&mut self.mocked_render_data);

        self.mocked_render_data
            .break_lines()
            .break_without_advance_or_alignment()
    }

    #[inline]
    pub fn set_content(&mut self, content: Content) {
        self.content = content;
    }
}
