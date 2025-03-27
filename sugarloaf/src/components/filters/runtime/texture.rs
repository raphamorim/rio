// This file was originally taken from https://github.com/SnowflakePowered/librashader
// The file has changed to avoid use atomic reference counter of wgpu Device and Queue structs
// SnowflakePowered/librashader is licensed under MPL-2.0
// https://github.com/SnowflakePowered/librashader/blob/master/LICENSE.md

use crate::components::filters::runtime::error::FilterChainError;
use crate::components::filters::runtime::mipmap::MipmapGen;
use crate::components::filters::runtime::{format_from_image_to_texture, WgpuOutputView};
use librashader_common::{FilterMode, GetSize, ImageFormat, Size, WrapMode};
use librashader_presets::Scale2D;
use librashader_runtime::scaling::{MipmapSize, ScaleFramebuffer, ViewportSize};
use std::sync::Arc;
use wgpu::TextureFormat;

pub struct OwnedImage {
    pub image: Arc<wgpu::Texture>,
    pub view: Arc<wgpu::TextureView>,
    pub max_miplevels: u32,
    #[allow(dead_code)]
    pub levels: u32,
    pub size: Size<u32>,
}

#[derive(Clone)]
pub struct InputImage {
    pub image: Arc<wgpu::Texture>,
    pub view: Arc<wgpu::TextureView>,
    pub wrap_mode: WrapMode,
    pub filter_mode: FilterMode,
    pub mip_filter: FilterMode,
}

impl AsRef<InputImage> for InputImage {
    fn as_ref(&self) -> &InputImage {
        self
    }
}

impl OwnedImage {
    pub fn new(
        device: &wgpu::Device,
        size: Size<u32>,
        max_miplevels: u32,
        format: TextureFormat,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: size.width,
                height: size.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: std::cmp::min(max_miplevels, size.calculate_miplevels()),
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[format],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: None,
            format: Some(format),
            dimension: Some(wgpu::TextureViewDimension::D2),
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
            ..Default::default()
        });

        Self {
            image: Arc::new(texture),
            view: Arc::new(view),
            max_miplevels,
            levels: std::cmp::min(max_miplevels, size.calculate_miplevels()),
            size,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn scale(
        &mut self,
        device: &wgpu::Device,
        scaling: Scale2D,
        format: TextureFormat,
        viewport_size: &Size<u32>,
        source_size: &Size<u32>,
        original_size: &Size<u32>,
        mipmap: bool,
    ) -> Size<u32> {
        let size = source_size.scale_viewport(
            scaling,
            *viewport_size,
            *original_size,
            Some(device.limits().max_texture_dimension_2d),
        );
        if self.size != size
            || (mipmap && self.max_miplevels == 1)
            || (!mipmap && self.max_miplevels != 1)
            || format != self.image.format()
        {
            let mut new = OwnedImage::new(device, size, self.max_miplevels, format);
            std::mem::swap(self, &mut new);
        }
        size
    }

    pub(crate) fn as_input(&self, filter: FilterMode, wrap_mode: WrapMode) -> InputImage {
        InputImage {
            image: Arc::clone(&self.image),
            view: Arc::clone(&self.view),
            wrap_mode,
            filter_mode: filter,
            mip_filter: filter,
        }
    }

    pub fn copy_from(
        &mut self,
        cmd: &mut wgpu::CommandEncoder,
        source: &wgpu::Texture,
        device: &wgpu::Device,
    ) {
        let source_size = source.size();
        let source_size = Size {
            width: source_size.width,
            height: source_size.height,
        };
        if source.format() != self.image.format() || self.size != source_size {
            let mut new =
                OwnedImage::new(device, source_size, self.max_miplevels, source.format());
            std::mem::swap(self, &mut new);
        }

        cmd.copy_texture_to_texture(
            source.as_image_copy(),
            self.image.as_image_copy(),
            source.size(),
        )
    }

    pub fn clear(&self, cmd: &mut wgpu::CommandEncoder) {
        cmd.clear_texture(&self.image, &wgpu::ImageSubresourceRange::default());
    }
    pub fn generate_mipmaps(
        &self,
        device: &wgpu::Device,
        cmd: &mut wgpu::CommandEncoder,
        mipmapper: &mut MipmapGen,
        sampler: &wgpu::Sampler,
    ) {
        mipmapper.generate_mipmaps(device, cmd, &self.image, sampler, self.max_miplevels);
    }
}

impl ScaleFramebuffer for OwnedImage {
    type Error = FilterChainError;
    type Context = wgpu::Device;

    fn scale(
        &mut self,
        scaling: Scale2D,
        format: ImageFormat,
        viewport_size: &Size<u32>,
        source_size: &Size<u32>,
        original_size: &Size<u32>,
        should_mipmap: bool,
        device: &Self::Context,
    ) -> Result<Size<u32>, Self::Error> {
        let format: Option<wgpu::TextureFormat> = format_from_image_to_texture(&format);
        let format = format.unwrap_or(TextureFormat::Bgra8Unorm);
        Ok(self.scale(
            device,
            scaling,
            format,
            viewport_size,
            source_size,
            original_size,
            should_mipmap,
        ))
    }
}

impl GetSize<u32> for WgpuOutputView<'_> {
    type Error = std::convert::Infallible;

    fn size(&self) -> Result<Size<u32>, Self::Error> {
        Ok(self.size)
    }
}
