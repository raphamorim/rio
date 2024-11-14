// This file was originally taken from https://github.com/SnowflakePowered/librashader
// SnowflakePowered/librashader is licensed under MPL-2.0
// https://github.com/SnowflakePowered/librashader/blob/master/LICENSE.md

use crate::components::filters::runtime::handle::Handle;
use crate::components::filters::runtime::texture::OwnedImage;
use librashader_common::Size;
use wgpu::TextureViewDescriptor;

/// A wgpu `TextureView` with size and texture information to output.
pub struct WgpuOutputView<'a> {
    pub(crate) size: Size<u32>,
    pub(crate) view: Handle<'a, wgpu::TextureView>,
    pub(crate) format: wgpu::TextureFormat,
}

impl<'a> WgpuOutputView<'a> {
    /// Create an output view from an existing texture view, size, and format.
    pub fn new_from_raw(
        view: &'a wgpu::TextureView,
        size: Size<u32>,
        format: wgpu::TextureFormat,
    ) -> Self {
        Self {
            size,
            view: Handle::Borrowed(view),
            format,
        }
    }
}

#[doc(hidden)]
impl<'a> From<&'a OwnedImage> for WgpuOutputView<'a> {
    fn from(image: &'a OwnedImage) -> Self {
        Self {
            size: image.size,
            view: Handle::Borrowed(&image.view),
            format: image.image.format(),
        }
    }
}

impl From<&wgpu::Texture> for WgpuOutputView<'static> {
    fn from(image: &wgpu::Texture) -> Self {
        let image_size = image.size();
        Self {
            size: Size {
                width: image_size.width,
                height: image_size.height,
            },
            view: Handle::Owned(image.create_view(&TextureViewDescriptor::default())),
            format: image.format(),
        }
    }
}
