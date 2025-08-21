use crate::context::Context;
use tracing::debug;

use super::atlas::*;
use super::ContentType;
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum AtlasKind {
    #[default]
    Mask, // R8 format for alpha masks
    Color, // RGBA format for color glyphs
}

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
    /// Which atlas this entry belongs to
    atlas_kind: AtlasKind,
}

pub struct Atlas {
    alloc: AtlasAllocator,
    buffer: Vec<u8>,
    fresh: bool,
    dirty: bool,
    channels: usize, // 1 for mask, 4 for color
}

impl Atlas {
    fn new(kind: AtlasKind) -> Self {
        let channels = match kind {
            AtlasKind::Mask => 1,
            AtlasKind::Color => 4, // Always 4 for Rgba8Unorm
        };

        Self {
            alloc: AtlasAllocator::new(SIZE, SIZE),
            buffer: vec![0; SIZE as usize * SIZE as usize * channels],
            fresh: true,
            dirty: false,
            channels,
        }
    }
}

pub struct ImageCache {
    pub entries: Vec<Entry>,
    mask_atlas: Atlas,
    color_atlas: Atlas,
    max_texture_size: u16,
    mask_texture: wgpu::Texture,
    color_texture: wgpu::Texture,
    pub mask_texture_view: wgpu::TextureView,
    pub color_texture_view: wgpu::TextureView,
}

#[inline]
pub fn buffer_size(width: u32, height: u32) -> Option<usize> {
    (width as usize)
        .checked_add(height as usize)?
        .checked_add(4)
}

pub const SIZE: u16 = 4096;

impl ImageCache {
    /// Creates a new image cache with dual atlases.
    pub fn new(context: &Context) -> Self {
        let device = &context.device;
        let max_texture_size = SIZE;

        // Create mask texture (R8 format for alpha masks)
        let mask_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rich_text mask atlas"),
            size: wgpu::Extent3d {
                width: SIZE as u32,
                height: SIZE as u32,
                depth_or_array_layers: 1,
            },
            view_formats: &[],
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            mip_level_count: 1,
            sample_count: 1,
        });
        let mask_texture_view =
            mask_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Create color texture (RGBA8 format for color glyphs - simpler than f16)
        let color_texture_format = wgpu::TextureFormat::Rgba8Unorm;
        let color_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rich_text color atlas"),
            size: wgpu::Extent3d {
                width: SIZE as u32,
                height: SIZE as u32,
                depth_or_array_layers: 1,
            },
            view_formats: &[],
            dimension: wgpu::TextureDimension::D2,
            format: color_texture_format,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            mip_level_count: 1,
            sample_count: 1,
        });
        let color_texture_view =
            color_texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            entries: Vec::new(),
            mask_atlas: Atlas::new(AtlasKind::Mask),
            color_atlas: Atlas::new(AtlasKind::Color), // Always 4 bytes per pixel for Rgba8Unorm
            max_texture_size,
            mask_texture,
            color_texture,
            mask_texture_view,
            color_texture_view,
        }
    }

    /// Allocates a new image and optionally fills it with the specified data.
    pub fn allocate(&mut self, request: AddImage) -> Option<ImageId> {
        let width = request.width;
        let height = request.height;

        // Reject zero-sized images
        if width == 0 || height == 0 {
            return None;
        }

        // Check buffer size
        buffer_size(width as u32, height as u32)?;

        // Too big to allocate
        if !(width <= self.max_texture_size && height <= self.max_texture_size) {
            return None;
        }

        // Choose the appropriate atlas based on content type
        let atlas_kind = match request.content_type {
            ContentType::Mask => AtlasKind::Mask,
            ContentType::Color => AtlasKind::Color,
        };

        let atlas = match atlas_kind {
            AtlasKind::Mask => &mut self.mask_atlas,
            AtlasKind::Color => &mut self.color_atlas,
        };

        let atlas_data = atlas.alloc.allocate(width, height);

        // Log cache miss when allocation fails
        if atlas_data.is_none() {
            debug!(
                "ImageCache allocation failed for {}x{} - {:?} atlas full",
                width, height, atlas_kind
            );
            return None;
        }

        let (x, y) = atlas_data?;
        let entry_index = self.entries.len();
        self.entries.push(Entry {
            allocated: true,
            x,
            y,
            width,
            height,
            atlas_kind,
        });

        if let Some(data) = request.data() {
            fill(
                FillParams {
                    x,
                    y,
                    width,
                    _height: height,
                    target_width: self.max_texture_size,
                    channels: atlas.channels,
                },
                data,
                &mut atlas.buffer,
            );
            atlas.dirty = true;
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

        let atlas = match entry.atlas_kind {
            AtlasKind::Mask => &mut self.mask_atlas,
            AtlasKind::Color => &mut self.color_atlas,
        };

        atlas.alloc.deallocate(entry.x, entry.y, entry.width);
        entry.allocated = false;
        Some(())
    }

    /// Retrieves the image for the specified handle and updates the epoch.
    pub fn get(&self, handle: &ImageId) -> Option<ImageLocation> {
        // Empty images have no location (for zero-sized glyphs)
        if handle.is_empty() {
            return None;
        }

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

    /// Clears all entries and resets the atlas. Used when fonts change.
    pub fn clear_atlas(&mut self) {
        // Clear all entries
        self.entries.clear();

        // Reset both atlases
        self.mask_atlas = Atlas::new(AtlasKind::Mask);
        self.color_atlas = Atlas::new(AtlasKind::Color);

        tracing::info!("Dual atlases cleared due to font change");
    }

    /// Returns true if the image is valid.
    pub fn is_valid(&self, image: ImageId) -> bool {
        // Empty images are always valid (for zero-sized glyphs)
        if image.is_empty() {
            return true;
        }

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
        // Process mask atlas
        if self.mask_atlas.dirty {
            let texture_size = wgpu::Extent3d {
                width: self.max_texture_size as u32,
                height: self.max_texture_size as u32,
                depth_or_array_layers: 1,
            };

            context.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.mask_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &self.mask_atlas.buffer,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(
                        self.max_texture_size as u32 * self.mask_atlas.channels as u32,
                    ),
                    rows_per_image: Some(self.max_texture_size as u32),
                },
                texture_size,
            );

            self.mask_atlas.fresh = false;
            self.mask_atlas.dirty = false;
        }

        // Process color atlas
        if self.color_atlas.dirty {
            let texture_size = wgpu::Extent3d {
                width: self.max_texture_size as u32,
                height: self.max_texture_size as u32,
                depth_or_array_layers: 1,
            };

            context.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.color_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &self.color_atlas.buffer,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(
                        self.max_texture_size as u32 * self.color_atlas.channels as u32,
                    ),
                    rows_per_image: Some(self.max_texture_size as u32),
                },
                texture_size,
            );

            self.color_atlas.fresh = false;
            self.color_atlas.dirty = false;
        }
    }
}

struct FillParams {
    x: u16,
    y: u16,
    width: u16,
    _height: u16,
    target_width: u16,
    channels: usize,
}

fn fill(params: FillParams, image: &[u8], target: &mut [u8]) -> Option<()> {
    let image_pitch = params.width as usize * params.channels;
    let buffer_pitch = params.target_width as usize * params.channels;
    let mut offset =
        params.y as usize * buffer_pitch + params.x as usize * params.channels;
    for row in image.chunks(image_pitch) {
        let dest = target.get_mut(offset..offset + image_pitch)?;
        dest.copy_from_slice(row);
        offset += buffer_pitch;
    }
    Some(())
}
