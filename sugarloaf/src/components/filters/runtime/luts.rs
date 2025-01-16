// This file was originally taken from https://github.com/SnowflakePowered/librashader
// The file has changed to avoid use atomic reference counter of wgpu Device and Queue structs
// SnowflakePowered/librashader is licensed under MPL-2.0
// https://github.com/SnowflakePowered/librashader/blob/master/LICENSE.md

use crate::components::filters::runtime::mipmap::MipmapGen;
use crate::components::filters::runtime::samplers::SamplerSet;
use crate::components::filters::runtime::texture::InputImage;
use librashader_common::{Size, WrapMode};
use librashader_presets::TextureMeta;
use librashader_runtime::image::Image;
use librashader_runtime::scaling::MipmapSize;
use std::sync::Arc;
use wgpu::TextureDescriptor;

pub(crate) struct LutTexture(InputImage);
impl AsRef<InputImage> for LutTexture {
    fn as_ref(&self) -> &InputImage {
        &self.0
    }
}

impl LutTexture {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cmd: &mut wgpu::CommandEncoder,
        image: Image,
        config: &TextureMeta,
        mipmapper: &mut MipmapGen,
        sampler_set: &SamplerSet,
    ) -> LutTexture {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some(&config.name),
            size: wgpu::Extent3d {
                width: image.size.width,
                height: image.size.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: if config.mipmap {
                image.size.calculate_miplevels()
            } else {
                1
            },
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                // need render attachment for mipmaps...
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &image.bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * image.size.width),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: image.size.width,
                height: image.size.height,
                depth_or_array_layers: 1,
            },
        );

        if config.mipmap {
            let wgpu_size = texture.size();
            let size = Size {
                width: wgpu_size.width,
                height: wgpu_size.height,
            };

            mipmapper.generate_mipmaps(
                device,
                cmd,
                &texture,
                &sampler_set.get(
                    WrapMode::ClampToEdge,
                    config.filter_mode,
                    config.filter_mode,
                ),
                size.calculate_miplevels(),
            );
        }

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let image = InputImage {
            image: Arc::new(texture),
            view: Arc::new(view),
            wrap_mode: config.wrap_mode,
            filter_mode: config.filter_mode,
            mip_filter: config.filter_mode,
        };

        Self(image)
    }
}
