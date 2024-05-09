// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::components::core::image::{Data, Handle};
use fnv::FnvHashMap;

pub struct SugarGraphicEntry {
    pub id: SugarGraphicId,
    pub handle: Handle,
}

#[derive(Default)]
pub struct SugarloafGraphics {
    inner: FnvHashMap<SugarGraphicId, SugarGraphicEntry>,
}

impl SugarloafGraphics {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[inline]
    pub fn get_mut(&mut self, id: &SugarGraphicId) -> Option<&mut SugarGraphicEntry> {
        self.inner.get_mut(id)
    }

    #[inline]
    pub fn get(&mut self, id: &SugarGraphicId) -> Option<&SugarGraphicEntry> {
        self.inner.get(id)
    }

    #[inline]
    pub fn keys(&self) -> Vec<SugarGraphicId> {
        self.inner.keys().cloned().collect::<Vec<_>>()
    }

    #[inline]
    pub fn add(&mut self, graphic_data: SugarGraphicData) {
        let handle = Handle::new(Data::Image(graphic_data.data));
        self.inner
            .entry(graphic_data.id)
            .or_insert(SugarGraphicEntry {
                id: graphic_data.id,
                handle,
            });
    }

    #[inline]
    pub fn remove(&mut self, graphic_id: &SugarGraphicId) {
        self.inner.remove(graphic_id);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SugarGraphic {
    pub id: SugarGraphicId,
    pub width: u16,
    pub height: u16,
}

/// Unique identifier for every graphic added to a grid.
#[derive(Eq, PartialEq, Clone, Debug, Copy, Hash, PartialOrd, Ord)]
pub struct SugarGraphicId(pub u64);

/// Specifies the format of the pixel data.
#[derive(Eq, PartialEq, Clone, Debug, Copy)]
pub enum ColorType {
    /// 3 bytes per pixel (red, green, blue).
    Rgb,

    /// 4 bytes per pixel (red, green, blue, alpha).
    Rgba,
}

/// Defines a single graphic read from the PTY.
#[derive(PartialEq, Clone, Debug)]
pub struct SugarGraphicData {
    /// Graphics identifier.
    pub id: SugarGraphicId,

    /// The actual image data
    pub data: image::DynamicImage,
}

impl SugarGraphicData {
    /// Check if all pixels under a region are opaque.
    ///
    /// If the region exceeds the boundaries of the image it is considered as
    /// not filled.
    pub fn is_filled(&self, x: usize, y: usize, width: usize, height: usize) -> bool {
        let image_width = self.data.width() as usize;
        let image_height = self.data.height() as usize;

        // If there are pixels outside the picture we assume that the region is
        // not filled.
        if x + width >= image_width || y + height >= image_height {
            return false;
        }

        true
    }
}
