use crate::context::Context;

use super::atlas::*;
use super::*;

#[derive(Default)]
pub struct Entry {
    allocated: bool,
    /// X coordinate of the image in an atlas.
    x: u16,
    /// Y coordinate of the image in an atlas.
    y: u16,
    /// Width of the image.
    width: u16,
    /// Height of the image.
    height: u16,
}

pub struct Atlas {
    alloc: AtlasAllocator,
    buffer: Vec<u8>,
    fresh: bool,
    dirty: bool,
}

pub struct ImageCache {
    pub entries: Vec<Entry>,
    atlas: Atlas,
    max_texture_size: u16,
    texture: wgpu::Texture,
    pub texture_view: wgpu::TextureView,
}

#[inline]
pub fn buffer_size(width: u32, height: u32) -> Option<usize> {
    (width as usize)
        .checked_add(height as usize)?
        .checked_add(4)
}

pub const SIZE: u16 = 2048;

impl ImageCache {
    /// Creates a new image cache.
    pub fn new(context: &Context) -> Self {
        let device = &context.device;
        // let max_texture_size = max_texture_size.clamp(1024, 8192);
        let max_texture_size = SIZE;

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rich_text create texture"),
            size: wgpu::Extent3d {
                width: SIZE as u32,
                height: SIZE as u32,
                depth_or_array_layers: 1,
            },
            view_formats: &[],
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            mip_level_count: 1,
            sample_count: 1,
        });
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let alloc = AtlasAllocator::new(max_texture_size, max_texture_size);

        Self {
            entries: Vec::new(),
            atlas: Atlas {
                alloc,
                buffer: vec![
                    0u8;
                    max_texture_size as usize * max_texture_size as usize * 4
                ],
                fresh: true,
                dirty: true,
            },
            max_texture_size,
            texture_view,
            texture,
        }
    }

    /// Allocates a new image and optionally fills it with the specified data.
    pub fn allocate(&mut self, request: AddImage) -> Option<ImageId> {
        let width = request.width;
        let height = request.height;

        // Check buffer size
        buffer_size(width as u32, height as u32)?;

        // Too big to allocate
        if !(width <= self.max_texture_size && height <= (self.max_texture_size / 4)) {
            return None;
        }

        let atlas_data = self.atlas.alloc.allocate(width, height);
        // if atlas_data.is_none() {
        // return None;
        // Grow atlas to fit
        // self.max_texture_size += SIZE;
        // self.atlas.fresh = true;
        // self.atlas.dirty = true;
        // self.entries.clear();
        // println!("{:?}", self.max_texture_size);
        // self.atlas.alloc = AtlasAllocator::new(self.max_texture_size, self.max_texture_size);
        // self.atlas.buffer = vec![
        //     0u8;
        //     self.max_texture_size as usize * self.max_texture_size as usize * 4
        // ];
        // atlas_data = self.atlas.alloc.allocate(width, height);
        // println!("{:?}", atlas_data);

        // if self.max_texture_size > MAX_SIZE {
        //     println!("should try to grow or reset atlas");
        // }
        // }
        let (x, y) = atlas_data?;
        let entry_index = self.entries.len();
        self.entries.push(Entry {
            allocated: true,
            x,
            y,
            width,
            height,
        });
        if let Some(data) = request.data() {
            fill(
                x,
                y,
                width,
                height,
                data,
                self.max_texture_size,
                &mut self.atlas.buffer,
            );
            self.atlas.dirty = true;
        }
        ImageId::new(entry_index as u32, request.has_alpha)
    }

    // Evaluate if does make sense to deallocate from atlas and if yes, which case?
    // considering that a terminal uses a short/limited of glyphs compared to a wide text editor
    // if deallocate an image then is necessary to cleanup cache of draw_layout fn
    /// Deallocates the specified image.
    #[allow(unused)]
    pub fn deallocate(&mut self, image: ImageId) -> Option<()> {
        let entry = self.entries.get_mut(image.index())?;
        if !entry.allocated {
            return None;
        }

        self.atlas.alloc.deallocate(entry.x, entry.y, entry.width);
        entry.allocated = false;
        Some(())
    }

    /// Retrieves the image for the specified handle and updates the epoch.
    pub fn get(&self, handle: &ImageId) -> Option<ImageLocation> {
        let entry = self.entries.get(handle.index())?;
        if !entry.allocated {
            return None;
        }
        let s = 1. / self.max_texture_size as f32;
        Some(ImageLocation {
            min: (entry.x as f32 * s, entry.y as f32 * s),
            max: (
                (entry.x + entry.width) as f32 * s,
                (entry.y + entry.height) as f32 * s,
            ),
        })
    }

    /// Returns true if the image is valid.
    pub fn is_valid(&self, image: ImageId) -> bool {
        if let Some(entry) = self.entries.get(image.index()) {
            entry.allocated
        } else {
            false
        }
    }

    /// Updates an image with the specified data.
    // pub fn update(&mut self, handle: ImageId, data: &[u8]) -> Option<()> {
    //     let entry = self.entries.get_mut(handle.index())?;
    //     if entry.flags & ENTRY_ALLOCATED == 0 {
    //         return None;
    //     }
    //         let atlas = self.atlases.get_mut(entry.owner as usize)?;
    //         fill(
    //             entry.x,
    //             entry.y,
    //             entry.width,
    //             entry.height,
    //             data,
    //             ATLAS_DIM,
    //             &mut atlas.buffer,
    //             4,
    //         );
    //         atlas.dirty = true;
    //     Some(())
    // }
    #[inline]
    pub fn process_atlases(&mut self, context: &mut Context) {
        if !self.atlas.dirty {
            return;
        }
        if self.atlas.fresh {
            let texture_size = wgpu::Extent3d {
                width: (self.max_texture_size).into(),
                height: (self.max_texture_size).into(),
                depth_or_array_layers: 1,
            };
            let new_texture = context.device.create_texture(&wgpu::TextureDescriptor {
                size: texture_size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST,
                label: Some("rich_text::fresh atlas"),
                view_formats: &[],
            });

            context.queue.write_texture(
                // Tells wgpu where to copy the pixel data
                wgpu::TexelCopyTextureInfo {
                    texture: &new_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                // The actual pixel data
                &self.atlas.buffer,
                // The layout of the texture
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some((self.max_texture_size * 4).into()),
                    rows_per_image: Some((self.max_texture_size).into()),
                },
                texture_size,
            );

            self.texture = new_texture;
            self.texture_view = self
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
        } else {
            let texture_size = wgpu::Extent3d {
                width: (self.max_texture_size).into(),
                height: (self.max_texture_size).into(),
                depth_or_array_layers: 1,
            };

            context.queue.write_texture(
                // Tells wgpu where to copy the pixel data
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y: 0, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                // The actual pixel data
                &self.atlas.buffer,
                // The layout of the texture
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some((self.max_texture_size * 4).into()),
                    rows_per_image: Some((self.max_texture_size).into()),
                },
                texture_size,
            );
        }
        self.atlas.fresh = false;
        self.atlas.dirty = false;
    }
}

fn fill(
    x: u16,
    y: u16,
    width: u16,
    _height: u16,
    image: &[u8],
    target_width: u16,
    target: &mut [u8],
) -> Option<()> {
    let channels = 4;
    let image_pitch = width as usize * channels;
    let buffer_pitch = target_width as usize * channels;
    let mut offset = y as usize * buffer_pitch + x as usize * channels;
    for row in image.chunks(image_pitch) {
        let dest = target.get_mut(offset..offset + image_pitch)?;
        dest.copy_from_slice(row);
        offset += buffer_pitch;
    }
    Some(())
}
