// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::sugarloaf::types;
use crate::sugarloaf::Handle;
use rustc_hash::FxHashMap;

pub struct GraphicDataEntry {
    pub handle: Handle,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug)]
pub struct GraphicRenderRequest {
    pub id: GraphicId,
    pub pos_x: f32,
    pub pos_y: f32,
    pub width: Option<f32>,
    pub height: Option<f32>,
}

pub struct BottomLayer {
    pub data: types::Raster,
    pub should_fit: bool,
}

#[derive(Default)]
pub struct Graphics {
    inner: FxHashMap<GraphicId, GraphicDataEntry>,
    pub bottom_layer: Option<BottomLayer>,
    pub top_layer: Vec<GraphicRenderRequest>,
}

impl Graphics {
    #[inline]
    pub fn has_graphics_on_top_layer(&self) -> bool {
        !self.top_layer.is_empty()
    }

    #[inline]
    pub fn clear_top_layer(&mut self) {
        self.top_layer.clear();
    }

    #[inline]
    pub fn get(&self, id: &GraphicId) -> Option<&GraphicDataEntry> {
        self.inner.get(id)
    }

    #[inline]
    pub fn insert(&mut self, graphic_data: GraphicData) {
        if self.inner.contains_key(&graphic_data.id) {
            return;
        }

        self.inner.insert(
            graphic_data.id,
            GraphicDataEntry {
                handle: Handle::from_pixels(
                    graphic_data.width as u32,
                    graphic_data.height as u32,
                    graphic_data.pixels,
                ),
                width: graphic_data.width as f32,
                height: graphic_data.height as f32,
            },
        );
    }

    #[inline]
    pub fn remove(&mut self, graphic_id: &GraphicId) {
        self.inner.remove(graphic_id);
    }
}

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub struct Graphic {
    pub id: GraphicId,
    pub offset_x: u16,
    pub offset_y: u16,
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
