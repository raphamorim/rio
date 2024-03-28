// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::sugarloaf::FontLibrary;

pub mod advanced;
pub mod elementary;

pub struct SugarCompositors {
    pub advanced: advanced::Advanced,
    pub elementary: elementary::Elementary,
}

impl SugarCompositors {
    pub fn new(font_library: &FontLibrary) -> Self {
        Self {
            elementary: elementary::Elementary::default(),
            advanced: advanced::Advanced::new(font_library),
        }
    }
}
