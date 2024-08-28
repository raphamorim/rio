// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::components::core::image::Handle;
use rustc_hash::FxHashMap;

pub struct GraphicEntry {
    pub id: GraphicId,
    pub handle: Handle,
}

#[derive(Default)]
pub struct SugarloafGraphics {
    inner: FxHashMap<GraphicId, GraphicEntry>,
}

impl SugarloafGraphics {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[inline]
    pub fn get_mut(&mut self, id: &GraphicId) -> Option<&mut GraphicEntry> {
        self.inner.get_mut(id)
    }

    #[inline]
    pub fn get(&mut self, id: &GraphicId) -> Option<&GraphicEntry> {
        self.inner.get(id)
    }

    #[inline]
    pub fn keys(&self) -> Vec<GraphicId> {
        self.inner.keys().cloned().collect::<Vec<_>>()
    }

    #[inline]
    pub fn add(&mut self, graphic_data: GraphicData) {
        let handle = Handle::from_pixels(
            graphic_data.width as u32,
            graphic_data.height as u32,
            graphic_data.pixels.clone(),
        );
        self.inner.entry(graphic_data.id).or_insert(GraphicEntry {
            id: graphic_data.id,
            handle,
        });
    }

    #[inline]
    pub fn remove(&mut self, graphic_id: &GraphicId) {
        self.inner.remove(graphic_id);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Graphic {
    pub id: GraphicId,
    pub width: u16,
    pub height: u16,
}

/// Unique identifier for every graphic added to a grid.
#[derive(Eq, PartialEq, Clone, Debug, Copy, Hash, PartialOrd, Ord)]
pub struct GraphicId(pub u64);

/// Specifies the format of the pixel data.
#[derive(Eq, PartialEq, Clone, Debug, Copy)]
pub enum ColorType {
    /// 3 bytes per pixel (red, green, blue).
    Rgb,

    /// 4 bytes per pixel (red, green, blue, alpha).
    Rgba,
}

/// Defines a single graphic read from the PTY.
#[derive(Eq, PartialEq, Clone, Debug)]
pub struct GraphicData {
    /// Graphics identifier.
    pub id: GraphicId,

    /// Width, in pixels, of the graphic.
    pub width: usize,

    /// Height, in pixels, of the graphic.
    pub height: usize,

    /// Color type of the pixels.
    pub color_type: ColorType,

    /// Pixels data.
    pub pixels: Vec<u8>,

    /// Indicate if there are no transparent pixels.
    pub is_opaque: bool,
}

impl GraphicData {
    /// Check if the image may contain transparent pixels. If it returns
    /// `false`, it is guaranteed that there are no transparent pixels.
    #[inline]
    pub fn maybe_transparent(&self) -> bool {
        !self.is_opaque && self.color_type == ColorType::Rgba
    }

    /// Check if all pixels under a region are opaque.
    ///
    /// If the region exceeds the boundaries of the image it is considered as
    /// not filled.
    pub fn is_filled(&self, x: usize, y: usize, width: usize, height: usize) -> bool {
        // If there are pixels outside the picture we assume that the region is
        // not filled.
        if x + width >= self.width || y + height >= self.height {
            return false;
        }

        // Don't check actual pixels if the image does not contain an alpha
        // channel.
        if !self.maybe_transparent() {
            return true;
        }

        debug_assert!(self.color_type == ColorType::Rgba);

        for offset_y in y..y + height {
            let offset = offset_y * self.width * 4;
            let row = &self.pixels[offset..offset + width * 4];

            if row.chunks_exact(4).any(|pixel| pixel.last() != Some(&255)) {
                return false;
            }
        }

        true
    }
}
