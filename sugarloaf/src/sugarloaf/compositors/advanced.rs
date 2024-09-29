// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// build_complex_content and update_layout was originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE

use crate::font::FontLibrary;
use crate::layout::{BuilderState, Content, FragmentStyle, RenderData};
use crate::sugarloaf::SugarloafLayout;

pub struct Advanced {
    pub content: Content,
    pub mocked_render_data: RenderData,
}

impl Advanced {
    pub fn new(font_library: &FontLibrary) -> Self {
        Self {
            content: Content::new(font_library),
            mocked_render_data: RenderData::new(),
        }
    }

    #[inline]
    pub fn reset(&mut self) {
        // self.render_data.clear();
    }

    #[inline]
    pub fn font_library(&self) -> &FontLibrary {
        self.content.font_library()
    }

    #[inline]
    pub fn set_fonts(&mut self, fonts: &FontLibrary) {
        self.content = Content::new(fonts);
    }

    #[inline]
    pub fn set_font_features(&mut self, font_features: &Option<Vec<String>>) {
        let mut found_font_features = vec![];
        if let Some(features) = font_features {
            for feature in features {
                let setting: crate::font_introspector::Setting<u16> =
                    (feature.as_str(), 1).into();
                found_font_features.push(setting);
            }
        }

        self.content.set_font_features(found_font_features);
    }

    #[inline]
    pub fn clear_rich_text(&mut self, id: &usize, layout: &SugarloafLayout) {
        self.content
            .clear_state(id, layout.dimensions.scale, layout.font_size);
    }

    #[inline]
    pub fn get_rich_text(&self, id: &usize) -> Option<&BuilderState> {
        self.content.get_state(id)
    }

    #[inline]
    pub fn create_rich_text(&mut self, layout: &SugarloafLayout) -> usize {
        self.content
            .create_state(layout.dimensions.scale, layout.font_size)
    }

    #[inline]
    pub fn update_render_data(&mut self, rich_text_id: usize) {
        self.content.resolve(&rich_text_id);
    }

    #[inline]
    pub fn calculate_dimensions(&mut self, layout: &SugarloafLayout) {
        self.mocked_render_data = RenderData::default();
        let mut content = Content::new(self.content.font_library());
        let id = content.create_state(layout.dimensions.scale, layout.font_size);
        content
            .sel(id)
            .new_line()
            .add_text(" ", FragmentStyle::default());
        self.mocked_render_data.clear();
        content.resolve(&id);
        self.mocked_render_data = content.get_state(&id).unwrap().render_data.clone();
    }
}
