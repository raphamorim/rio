// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// This file was originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE

use super::builder::{FontLibraryBuilder, MmapHint};
use super::index::StaticIndex;
use std::sync::{Arc, RwLock};

/// Indexed collection of fonts used during layout for font selection and
/// fallback.
#[derive(Clone)]
pub struct FontLibrary {
    pub(super) inner: Arc<Inner>,
}

impl FontLibrary {
    /// Creates builder for a font library.
    pub fn builder() -> FontLibraryBuilder {
        FontLibraryBuilder::default()
    }

    pub(super) fn new(index: StaticIndex) -> Self {
        Self {
            inner: Arc::new(Inner {
                index: RwLock::new(Arc::new(index)),
            }),
        }
    }
}

impl Default for FontLibrary {
    fn default() -> Self {
        Self::builder()
            .mmap(MmapHint::Threshold(1024 * 1024))
            .add_system_fonts()
            .add_user_fonts()
            .map_generic_families(true)
            .map_fallbacks(true)
            .build()
    }
}

pub struct Inner {
    pub index: RwLock<Arc<StaticIndex>>,
}
