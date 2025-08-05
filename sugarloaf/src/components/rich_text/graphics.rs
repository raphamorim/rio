// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::components::core::image::Handle;
use rustc_hash::FxHashMap;

/// Max allowed dimensions (width, height) for the graphic, in pixels.
pub const MAX_GRAPHIC_DIMENSIONS: [usize; 2] = [4096, 4096];

/// Unique identifier for every graphic added to a grid.
#[derive(Eq, PartialEq, Clone, Debug, Copy, Hash, PartialOrd, Ord)]
pub struct GraphicId(pub u64);

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub struct Graphic {
    pub id: GraphicId,
    pub offset_x: u16,
    pub offset_y: u16,
}

/// Specifies the format of the pixel data.
#[derive(Eq, PartialEq, Clone, Debug, Copy)]
pub enum ColorType {
    /// 3 bytes per pixel (red, green, blue).
    Rgb,
    /// 4 bytes per pixel (red, green, blue, alpha).
    Rgba,
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

/// Defines a single graphic read from the PTY.
#[derive(Eq, PartialEq, Clone, Debug)]
pub struct GraphicData {
    /// Graphics identifier.
    pub id: GraphicId,
    /// Width of the graphic, in pixels.
    pub width: usize,
    /// Height of the graphic, in pixels.
    pub height: usize,
    /// Pixel data.
    pub pixels: Vec<u8>,
    /// Color format of the pixel data.
    pub color_type: ColorType,
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

        let bytes_per_pixel = 4;
        let stride = self.width * bytes_per_pixel;

        for row in y..y + height {
            let row_offset = row * stride;
            for col in x..x + width {
                let offset = row_offset + col * bytes_per_pixel;
                let alpha = self.pixels[offset + 3];
                if alpha != 255 {
                    return false;
                }
            }
        }

        true
    }

    /// Create a new graphic from a dynamic image.
    pub fn from_dynamic_image(id: GraphicId, image: image_rs::DynamicImage) -> Self {
        use image_rs::DynamicImage;

        let (width, height) = (image.width() as usize, image.height() as usize);

        let (pixels, color_type, is_opaque) = match image {
            DynamicImage::ImageRgb8(buffer) => {
                (buffer.into_raw(), ColorType::Rgb, true)
            }
            DynamicImage::ImageRgba8(buffer) => {
                let is_opaque = buffer.pixels().all(|p| p[3] == 255);
                (buffer.into_raw(), ColorType::Rgba, is_opaque)
            }
            _ => {
                let buffer = image.to_rgba8();
                let is_opaque = buffer.pixels().all(|p| p[3] == 255);
                (buffer.into_raw(), ColorType::Rgba, is_opaque)
            }
        };

        GraphicData {
            id,
            width,
            height,
            color_type,
            pixels,
            is_opaque,
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
        } else if resize.height == ResizeParameter::Auto {
            height = self.height * width / self.width;
        }

        // Limit dimensions to prevent OOM.
        width = std::cmp::min(width, MAX_GRAPHIC_DIMENSIONS[0]);
        height = std::cmp::min(height, MAX_GRAPHIC_DIMENSIONS[1]);

        // Don't resize if the dimensions are the same.
        if width == self.width && height == self.height {
            return Some(self);
        }

        // Create a dynamic image from the pixels.
        use image_rs::{DynamicImage, ImageBuffer};
        let dynimage = match self.color_type {
            ColorType::Rgb => {
                let buffer = ImageBuffer::from_raw(
                    self.width as u32,
                    self.height as u32,
                    self.pixels,
                )?;
                DynamicImage::ImageRgb8(buffer)
            }
            ColorType::Rgba => {
                let buffer = ImageBuffer::from_raw(
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

pub struct GraphicDataEntry {
    pub handle: Handle,
    pub width: f32,
    pub height: f32,
}

#[derive(Default)]
pub struct GraphicsStorage {
    inner: FxHashMap<GraphicId, GraphicDataEntry>,
}

impl GraphicsStorage {
    #[inline]
    pub fn get(&self, id: &GraphicId) -> Option<&GraphicDataEntry> {
        self.inner.get(id)
    }

    #[inline]
    pub fn insert(&mut self, graphic_data: GraphicData) {
        use std::cmp;
        
        // Limit graphic dimensions to prevent OOM.
        let width = cmp::min(graphic_data.width, MAX_GRAPHIC_DIMENSIONS[0]);
        let height = cmp::min(graphic_data.height, MAX_GRAPHIC_DIMENSIONS[1]);

        if width == 0 || height == 0 {
            return;
        }

        self.inner.insert(
            graphic_data.id,
            GraphicDataEntry {
                handle: Handle::from_pixels(
                    width as u32,
                    height as u32,
                    graphic_data.pixels,
                ),
                width: width as f32,
                height: height as f32,
            },
        );
    }

    #[inline]
    pub fn remove(&mut self, graphic_id: &GraphicId) {
        self.inner.remove(graphic_id);
    }

    #[inline]
    pub fn clear(&mut self) {
        self.inner.clear();
    }
}