// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::sugarloaf::Handle;
use rustc_hash::FxHashMap;

// The renderer-agnostic graphics value types now live in `rio-core`. Re-export
// them here so existing `sugarloaf::sugarloaf::graphics::*` paths keep working.
pub use rio_core::graphics::{
    ColorType, GraphicData, GraphicId, ResizeCommand, ResizeParameter,
    MAX_GRAPHIC_DIMENSIONS,
};

pub struct GraphicDataEntry {
    pub handle: Handle,
    pub width: f32,
    pub height: f32,
    pub transmit_time: std::time::Instant,
}

impl GraphicDataEntry {
    /// Create from a GraphicData, taking ownership of pixel data.
    pub fn from_graphic_data(data: GraphicData) -> Self {
        let display_w = data.display_width.unwrap_or(data.width) as f32;
        let display_h = data.display_height.unwrap_or(data.height) as f32;
        Self {
            handle: Handle::from_pixels(
                data.width as u32,
                data.height as u32,
                data.pixels,
            ),
            width: display_w,
            height: display_h,
            transmit_time: data.transmit_time,
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

/// An overlay image placement.
/// Used by the renderer to draw images on top of (or behind) terminal content.
#[derive(Debug, Clone)]
pub struct GraphicOverlay {
    /// Image identifier (kitty protocol image_id).
    pub image_id: u32,
    /// Screen position (physical pixels).
    pub x: f32,
    pub y: f32,
    /// Display dimensions (physical pixels).
    pub width: f32,
    pub height: f32,
    /// Z-index for layering.
    pub z_index: i32,
    /// Source rectangle in normalised texture coordinates `[u0, v0, u1, v1]`.
    /// `[0.0, 0.0, 1.0, 1.0]` (the default) draws the whole image; other
    /// values draw a slice — used by the kitty Unicode-placeholder path
    /// where each placeholder cell shows one slice of the image.
    pub source_rect: [f32; 4],
}

impl GraphicOverlay {
    /// Default source rect — full image.
    pub const FULL_SOURCE_RECT: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
}

// `graphic_data_from_dynamic_image` (which depends on the `image` crate) moved
// into the `canario` engine crate alongside the iTerm2 decoder that was its only
// caller. `graphic_data_resized` was removed with it — it had no remaining
// callers in the workspace.

#[test]
fn check_opaque_region() {
    let graphic = GraphicData {
        id: GraphicId::new(1),
        width: 10,
        height: 10,
        color_type: ColorType::Rgb,
        pixels: vec![255; 10 * 10 * 3],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };

    assert!(graphic.is_filled(1, 1, 3, 3));
    assert!(!graphic.is_filled(8, 8, 10, 10));

    let pixels = {
        // Put a transparent 3x3 box inside the picture.
        let mut data = vec![255; 10 * 10 * 4];
        for y in 3..6 {
            let offset = y * 10 * 4;
            data[offset..offset + 3 * 4].fill(0);
        }
        data
    };

    let graphic = GraphicData {
        id: GraphicId::new(1),
        pixels,
        width: 10,
        height: 10,
        color_type: ColorType::Rgba,
        is_opaque: false,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };

    assert!(graphic.is_filled(0, 0, 3, 3));
    assert!(!graphic.is_filled(1, 1, 4, 4));
}
