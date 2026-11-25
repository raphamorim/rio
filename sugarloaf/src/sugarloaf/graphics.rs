// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

// The graphic value types (GraphicData, GraphicId, ColorType, Resize*,
// GraphicOverlay, the image-key hashers) now live in the leaf crate
// `rio-graphics` so the terminal core can model images without pulling
// the renderer. They are re-exported here so `sugarloaf::graphics::*` and
// `sugarloaf::GraphicData` keep resolving. Only the render cache
// (GraphicDataEntry / Graphics), which owns a GPU-uploadable Handle,
// stays in sugarloaf.

use crate::sugarloaf::Handle;
use rustc_hash::FxHashMap;
use std::sync::Arc;

pub use rio_graphics::{
    atlas_image_key, image_key_route, kitty_image_key, route_image_key, ColorType,
    Graphic, GraphicData, GraphicId, GraphicOverlay, ResizeCommand, ResizeParameter,
    MAX_GRAPHIC_DIMENSIONS,
};

pub struct GraphicDataEntry {
    pub handle: Handle,
    pub width: f32,
    pub height: f32,
    pub transmit_time: std::time::Instant,
    /// No pixel of the image lets anything below it show.
    pub is_opaque: bool,
}

impl GraphicDataEntry {
    /// Create from a GraphicData that the terminal keeps a share of,
    /// without copying its pixels.
    pub fn from_shared(data: Arc<GraphicData>) -> Self {
        struct SharedPixels(Arc<GraphicData>);

        impl AsRef<[u8]> for SharedPixels {
            fn as_ref(&self) -> &[u8] {
                &self.0.pixels
            }
        }

        let display_w = data.display_width.unwrap_or(data.width) as f32;
        let display_h = data.display_height.unwrap_or(data.height) as f32;
        let (width, height) = (data.width as u32, data.height as u32);
        let transmit_time = data.transmit_time;
        let is_opaque = !data.maybe_transparent();
        Self {
            handle: Handle::from_pixels(width, height, SharedPixels(data)),
            width: display_w,
            height: display_h,
            transmit_time,
            is_opaque,
        }
    }

    /// Create from a GraphicData, taking ownership of pixel data.
    pub fn from_graphic_data(data: GraphicData) -> Self {
        let display_w = data.display_width.unwrap_or(data.width) as f32;
        let display_h = data.display_height.unwrap_or(data.height) as f32;
        let is_opaque = !data.maybe_transparent();
        Self {
            handle: Handle::from_pixels(
                data.width as u32,
                data.height as u32,
                data.pixels,
            ),
            width: display_w,
            height: display_h,
            transmit_time: data.transmit_time,
            is_opaque,
        }
    }
}

#[derive(Default)]
pub struct Graphics {
    inner: FxHashMap<GraphicId, GraphicDataEntry>,
}

impl Graphics {
    #[inline]
    pub fn get(&self, id: &GraphicId) -> Option<&GraphicDataEntry> {
        self.inner.get(id)
    }

    #[inline]
    pub fn insert(&mut self, graphic_data: GraphicData) {
        // Check if existing entry has the same generation (skip re-upload)
        if let Some(existing) = self.inner.get(&graphic_data.id) {
            if existing.transmit_time == graphic_data.transmit_time {
                return;
            }
        }

        let display_w = graphic_data.display_width.unwrap_or(graphic_data.width) as f32;
        let display_h = graphic_data.display_height.unwrap_or(graphic_data.height) as f32;
        let is_opaque = !graphic_data.maybe_transparent();
        self.inner.insert(
            graphic_data.id,
            GraphicDataEntry {
                handle: Handle::from_pixels(
                    graphic_data.width as u32,
                    graphic_data.height as u32,
                    graphic_data.pixels,
                ),
                width: display_w,
                height: display_h,
                transmit_time: graphic_data.transmit_time,
                is_opaque,
            },
        );
    }

    #[inline]
    pub fn remove(&mut self, graphic_id: &GraphicId) {
        self.inner.remove(graphic_id);
    }
}
