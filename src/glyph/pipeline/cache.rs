use core::num::NonZeroU64;
use std::num::NonZeroU32;

pub struct Cache {
    texture: wgpu::Texture,
    pub(super) view: wgpu::TextureView,
    upload_buffer: wgpu::Buffer,
    upload_buffer_size: u64,
}

impl Cache {
    const INITIAL_UPLOAD_BUFFER_SIZE: u64 =
        wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as u64 * 100;

    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Cache {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph::Cache"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            mip_level_count: 1,
            sample_count: 1,
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let upload_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("glyph::Cache upload buffer"),
            size: Self::INITIAL_UPLOAD_BUFFER_SIZE,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        Cache {
            texture,
            view,
            upload_buffer,
            upload_buffer_size: Self::INITIAL_UPLOAD_BUFFER_SIZE,
        }
    }

    pub fn update(
        &mut self,
        device: &wgpu::Device,
        staging_belt: &mut wgpu::util::StagingBelt,
        encoder: &mut wgpu::CommandEncoder,
        offset: [u16; 2],
        size: [u16; 2],
        data: &[u8],
    ) {
        let width = size[0] as usize;
        let height = size[1] as usize;

        // It is a webgpu requirement that:
        //  BufferCopyView.layout.bytes_per_row % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT == 0
        // So we calculate padded_width by rounding width
        // up to the next multiple of wgpu::COPY_BYTES_PER_ROW_ALIGNMENT.
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
        let padded_width_padding = (align - width % align) % align;
        let padded_width = width + padded_width_padding;

        let padded_data_size = (padded_width * height) as u64;

        if self.upload_buffer_size < padded_data_size {
            self.upload_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("glyph::Cache upload buffer"),
                size: padded_data_size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });

            self.upload_buffer_size = padded_data_size;
        }

        let mut padded_data = staging_belt.write_buffer(
            encoder,
            &self.upload_buffer,
            0,
            NonZeroU64::new(padded_data_size).unwrap(),
            device,
        );

        for row in 0..height {
            padded_data[row * padded_width..row * padded_width + width]
                .copy_from_slice(&data[row * width..(row + 1) * width])
        }

        // TODO: Move to use Queue for less buffer usage
        encoder.copy_buffer_to_texture(
            wgpu::ImageCopyBuffer {
                buffer: &self.upload_buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: NonZeroU32::new(padded_width as u32),
                    rows_per_image: NonZeroU32::new(height as u32),
                },
            },
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: u32::from(offset[0]),
                    y: u32::from(offset[1]),
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: size[0] as u32,
                height: size[1] as u32,
                depth_or_array_layers: 1,
            },
        );
    }
}
