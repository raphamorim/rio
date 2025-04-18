// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::sugarloaf::types;
use crate::sugarloaf::Handle;
use image_rs::DynamicImage;
use rustc_hash::FxHashMap;
use std::cmp;

/// Max allowed dimensions (width, height) for the graphic, in pixels.
pub const MAX_GRAPHIC_DIMENSIONS: [usize; 2] = [4096, 4096];

pub struct GraphicDataEntry {
    pub handle: Handle,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Copy, Clone, PartialEq)]
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

    /// Render graphic in a different size.
    pub resize: Option<ResizeCommand>,
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

    pub fn from_dynamic_image(id: GraphicId, image: DynamicImage) -> Self {
        let color_type;
        let width;
        let height;
        let pixels;

        match image {
            // Sugarloaf only accepts rgba8 now
            // DynamicImage::ImageRgb8(image) => {
            //     color_type = ColorType::Rgb;
            //     width = image.width() as usize;
            //     height = image.height() as usize;
            //     pixels = image.into_raw();
            // }
            DynamicImage::ImageRgba8(image) => {
                color_type = ColorType::Rgba;
                width = image.width() as usize;
                height = image.height() as usize;
                pixels = image.into_raw();
            }

            _ => {
                // Non-RGB image. Convert it to RGBA.
                let image = image.into_rgba8();
                color_type = ColorType::Rgba;
                width = image.width() as usize;
                height = image.height() as usize;
                pixels = image.into_raw();
            }
        }

        GraphicData {
            id,
            width,
            height,
            color_type,
            pixels,
            is_opaque: false,
            resize: None,
        }
    }

    /// Resize the graphic according to the dimensions in the `resize` field.
    pub fn resized(
        self,
        cell_width: usize,
        cell_height: usize,
        view_width: usize,
        view_height: usize,
    ) -> Option<Self> {
        let resize = match self.resize {
            Some(resize) => resize,
            None => return Some(self),
        };

        if (resize.width == ResizeParameter::Auto
            && resize.height == ResizeParameter::Auto)
            || self.height == 0
            || self.width == 0
        {
            return Some(self);
        }

        let mut width = match resize.width {
            ResizeParameter::Auto => 1,
            ResizeParameter::Pixels(n) => n as usize,
            ResizeParameter::Cells(n) => n as usize * cell_width,
            ResizeParameter::WindowPercent(n) => n as usize * view_width / 100,
        };

        let mut height = match resize.height {
            ResizeParameter::Auto => 1,
            ResizeParameter::Pixels(n) => n as usize,
            ResizeParameter::Cells(n) => n as usize * cell_height,
            ResizeParameter::WindowPercent(n) => n as usize * view_height / 100,
        };

        if width == 0 || height == 0 {
            return None;
        }

        // Compute "auto" dimensions.
        if resize.width == ResizeParameter::Auto {
            width = self.width * height / self.height;
        }

        if resize.height == ResizeParameter::Auto {
            height = self.height * width / self.width;
        }

        // Limit size to MAX_GRAPHIC_DIMENSIONS.
        width = cmp::min(width, MAX_GRAPHIC_DIMENSIONS[0]);
        height = cmp::min(height, MAX_GRAPHIC_DIMENSIONS[1]);

        tracing::trace!("Resize new graphic to width={}, height={}", width, height,);

        // Create a new DynamicImage to resize the graphic.
        let dynimage = match self.color_type {
            ColorType::Rgb => {
                let buffer = image_rs::RgbImage::from_raw(
                    self.width as u32,
                    self.height as u32,
                    self.pixels,
                )?;
                DynamicImage::ImageRgb8(buffer)
            }

            ColorType::Rgba => {
                let buffer = image_rs::RgbaImage::from_raw(
                    self.width as u32,
                    self.height as u32,
                    self.pixels,
                )?;
                DynamicImage::ImageRgba8(buffer)
            }
        };

        // Finally, use `resize` or `resize_exact` to make the new image.
        let width = width as u32;
        let height = height as u32;
        // https://doc.servo.org/image/imageops/enum.FilterType.html
        let filter = image_rs::imageops::FilterType::Triangle;

        let new_image = if resize.preserve_aspect_ratio {
            dynimage.resize(width, height, filter)
        } else {
            dynimage.resize_exact(width, height, filter)
        };

        Some(Self::from_dynamic_image(self.id, new_image))
    }
}

/// Unit to specify a dimension to resize the graphic.
#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum ResizeParameter {
    /// Dimension is computed from the original graphic dimensions.
    Auto,

    /// Size is specified in number of grid cells.
    Cells(u32),

    /// Size is specified in number pixels.
    Pixels(u32),

    /// Size is specified in a percent of the window.
    WindowPercent(u32),
}

/// Dimensions to resize a graphic.
#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub struct ResizeCommand {
    pub width: ResizeParameter,

    pub height: ResizeParameter,

    pub preserve_aspect_ratio: bool,
}

#[test]
fn check_opaque_region() {
    let graphic = GraphicData {
        id: GraphicId(0),
        width: 10,
        height: 10,
        color_type: ColorType::Rgb,
        pixels: vec![255; 10 * 10 * 3],
        is_opaque: true,
        resize: None,
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
        id: GraphicId(0),
        pixels,
        width: 10,
        height: 10,
        color_type: ColorType::Rgba,
        is_opaque: false,
        resize: None,
    };

    assert!(graphic.is_filled(0, 0, 3, 3));
    assert!(!graphic.is_filled(1, 1, 4, 4));
}
