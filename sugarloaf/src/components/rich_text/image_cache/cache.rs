use crate::context::{Context, ContextType};
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
    /// X coordinate of the image in an atlas
    x: u16,
    /// Y coordinate of the image in an atlas
    y: u16,
    /// Width of the image.
    width: u16,
    /// Height of the image.
    height: u16,
    /// Which atlas this entry belongs to
    atlas_kind: AtlasKind,
    /// Which color atlas index (0 = first, 1 = second, etc) if atlas_kind == Color
    color_atlas_index: usize,
}

pub struct Atlas {
    alloc: AtlasAllocator,
    buffer: Vec<u8>,
    fresh: bool,
    dirty: bool,
    channels: usize, // 1 for mask, 4 for color
}

impl Atlas {
    fn new(kind: AtlasKind, size: u16) -> Self {
        let channels = match kind {
            AtlasKind::Mask => 1,
            AtlasKind::Color => 4,
        };

        Self {
            alloc: AtlasAllocator::new(size, size),
            buffer: vec![0; size as usize * size as usize * channels],
            fresh: true,
            dirty: false,
            channels,
        }
    }
}

pub const SIZE: u16 = 4096;

#[derive(Debug)]
pub enum ImageCacheType {
    Wgpu(WgpuImageCache),
    #[cfg(target_os = "macos")]
    Metal(MetalImageCache),
}

#[derive(Debug)]
pub struct WgpuImageCache {
    mask_texture: wgpu::Texture,
    color_texture: wgpu::Texture,
    pub mask_texture_view: wgpu::TextureView,
    pub color_texture_view: wgpu::TextureView,
    device: std::sync::Arc<wgpu::Device>,
    queue: std::sync::Arc<wgpu::Queue>,
}

#[cfg(target_os = "macos")]
#[derive(Debug)]
pub struct MetalImageCache {
    mask_texture: metal::Texture,
    color_texture: metal::Texture,
    device: metal::Device,
}

pub struct ImageCache {
    pub entries: Vec<Entry>,
    /// One atlas for mask/glyph data
    mask_atlas: Atlas,
    /// Multiple color atlases, each with own GPU texture (for glyphs + protocol graphics)
    /// When one fills, we create another
    color_atlases: Vec<ColorAtlasWithTexture>,
    max_texture_size: u16,
    device_queue: DeviceQueue,
}

/// Each color atlas has its own GPU texture
struct ColorAtlasWithTexture {
    atlas: Atlas,
    texture: ColorAtlasTexture,
}

enum ColorAtlasTexture {
    Wgpu(wgpu::Texture, wgpu::TextureView),
    #[cfg(target_os = "macos")]
    Metal(metal::Texture),
}

// Maximum number of texture array layers we support
const MAX_ATLAS_LAYERS: usize = 16;

enum DeviceQueue {
    Wgpu {
        device: std::sync::Arc<wgpu::Device>,
        queue: std::sync::Arc<wgpu::Queue>,
        mask_texture: wgpu::Texture,
        mask_texture_view: wgpu::TextureView,
    },
    #[cfg(target_os = "macos")]
    Metal {
        device: metal::Device,
        mask_texture: metal::Texture,
    },
}

#[inline]
pub fn buffer_size(width: u32, height: u32) -> Option<usize> {
    (width as usize)
        .checked_add(height as usize)?
        .checked_add(4)
}

impl ImageCache {
    /// Creates a new image cache with mask atlas + initial color atlas
    pub fn new(context: &Context) -> Self {
        let max_texture_size = SIZE;

        match &context.inner {
            ContextType::Wgpu(wgpu_context) => {
                let device = std::sync::Arc::new(wgpu_context.device.clone());
                let queue = std::sync::Arc::new(wgpu_context.queue.clone());

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
                let mask_texture_view = mask_texture.create_view(&wgpu::TextureViewDescriptor::default());

                // Create first color atlas with texture
                let color_texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("rich_text color atlas 0"),
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
                let color_texture_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());

                let color_atlases = vec![ColorAtlasWithTexture {
                    atlas: Atlas::new(AtlasKind::Color, max_texture_size),
                    texture: ColorAtlasTexture::Wgpu(color_texture, color_texture_view),
                }];

                Self {
                    entries: Vec::new(),
                    mask_atlas: Atlas::new(AtlasKind::Mask, max_texture_size),
                    color_atlases,
                    max_texture_size,
                    device_queue: DeviceQueue::Wgpu { device, queue, mask_texture, mask_texture_view },
                }
            }
            #[cfg(target_os = "macos")]
            ContextType::Metal(metal_context) => {
                let device = metal_context.device.clone();

                // Create mask texture (R8 format for alpha masks)
                let mask_descriptor = metal::TextureDescriptor::new();
                mask_descriptor.set_pixel_format(metal::MTLPixelFormat::R8Unorm);
                mask_descriptor.set_width(max_texture_size as u64);
                mask_descriptor.set_height(max_texture_size as u64);
                mask_descriptor.set_usage(
                    metal::MTLTextureUsage::ShaderRead | metal::MTLTextureUsage::ShaderWrite,
                );
                let mask_texture = device.new_texture(&mask_descriptor);
                mask_texture.set_label("Sugarloaf Rich Text Mask Atlas");

                // Create first color atlas with texture
                let color_descriptor = metal::TextureDescriptor::new();
                color_descriptor.set_pixel_format(metal::MTLPixelFormat::RGBA8Unorm);
                color_descriptor.set_width(max_texture_size as u64);
                color_descriptor.set_height(max_texture_size as u64);
                color_descriptor.set_usage(
                    metal::MTLTextureUsage::ShaderRead | metal::MTLTextureUsage::ShaderWrite,
                );
                let color_texture = device.new_texture(&color_descriptor);
                color_texture.set_label("Sugarloaf Rich Text Color Atlas 0");

                let color_atlases = vec![ColorAtlasWithTexture {
                    atlas: Atlas::new(AtlasKind::Color, max_texture_size),
                    texture: ColorAtlasTexture::Metal(color_texture),
                }];

                Self {
                    entries: Vec::new(),
                    mask_atlas: Atlas::new(AtlasKind::Mask, max_texture_size),
                    color_atlases,
                    max_texture_size,
                    device_queue: DeviceQueue::Metal { device, mask_texture },
                }
            }
        }
    }

    /// Allocates a new image and optionally fills it with the specified data.
    /// For color images: tries all existing color atlases, creates new one if all full
    /// For mask images: uses the single mask atlas
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

        let entry_index = self.entries.len();
        let atlas_kind = match request.content_type {
            ContentType::Mask => AtlasKind::Mask,
            ContentType::Color => AtlasKind::Color,
        };

        // Handle mask atlas (single atlas)
        if atlas_kind == AtlasKind::Mask {
            let atlas_data = self.mask_atlas.alloc.allocate(width, height);
            if atlas_data.is_none() {
                debug!("Mask atlas full for {}x{}", width, height);
                return None;
            }

            let (x, y) = atlas_data?;
            self.entries.push(Entry {
                allocated: true,
                x,
                y,
                width,
                height,
                atlas_kind,
                color_atlas_index: 0, // Not used for mask
            });

            if let Some(data) = request.data() {
                fill(
                    FillParams {
                        x,
                        y,
                        width,
                        _height: height,
                        target_width: self.max_texture_size,
                        channels: self.mask_atlas.channels,
                    },
                    data,
                    &mut self.mask_atlas.buffer,
                );
                self.mask_atlas.dirty = true;
            }

            return ImageId::new(entry_index as u32, request.has_alpha);
        }

        // Handle color atlases (multiple atlases, Ghostty-style)
        // Try all existing color atlases first
        for (atlas_index, atlas_with_texture) in self.color_atlases.iter_mut().enumerate() {
            if let Some((x, y)) = atlas_with_texture.atlas.alloc.allocate(width, height) {
                // Found space in existing atlas
                self.entries.push(Entry {
                    allocated: true,
                    x,
                    y,
                    width,
                    height,
                    atlas_kind,
                    color_atlas_index: atlas_index,
                });

                if let Some(data) = request.data() {
                    fill(
                        FillParams {
                            x,
                            y,
                            width,
                            _height: height,
                            target_width: self.max_texture_size,
                            channels: atlas_with_texture.atlas.channels,
                        },
                        data,
                        &mut atlas_with_texture.atlas.buffer,
                    );
                    atlas_with_texture.atlas.dirty = true;
                }

                debug!(
                    "Allocated {}x{} in existing color atlas {}",
                    width, height, atlas_index
                );
                return ImageId::new(entry_index as u32, request.has_alpha);
            }
        }

        // All existing atlases full - create a new one
        debug!("All color atlases full, creating new atlas for {}x{}", width, height);
        let new_atlas_index = self.color_atlases.len();

        if !self.create_new_color_atlas() {
            debug!("Failed to create new color atlas");
            return None;
        }

        // Try allocation in the new atlas
        let atlas_with_texture = self.color_atlases.last_mut()?;
        let (x, y) = atlas_with_texture.atlas.alloc.allocate(width, height)?;

        self.entries.push(Entry {
            allocated: true,
            x,
            y,
            width,
            height,
            atlas_kind,
            color_atlas_index: new_atlas_index,
        });

        if let Some(data) = request.data() {
            fill(
                FillParams {
                    x,
                    y,
                    width,
                    _height: height,
                    target_width: self.max_texture_size,
                    channels: atlas_with_texture.atlas.channels,
                },
                data,
                &mut atlas_with_texture.atlas.buffer,
            );
            atlas_with_texture.atlas.dirty = true;
        }

        debug!("Allocated {}x{} in new color atlas {}", width, height, new_atlas_index);
        ImageId::new(entry_index as u32, request.has_alpha)
    }

    /// Create a new color atlas with its own GPU texture
    fn create_new_color_atlas(&mut self) -> bool {
        let atlas_index = self.color_atlases.len();
        debug!("Creating color atlas {}", atlas_index);

        match &self.device_queue {
            DeviceQueue::Wgpu { device, queue: _, .. } => {
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(&format!("rich_text color atlas {}", atlas_index)),
                    size: wgpu::Extent3d {
                        width: self.max_texture_size as u32,
                        height: self.max_texture_size as u32,
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

                self.color_atlases.push(ColorAtlasWithTexture {
                    atlas: Atlas::new(AtlasKind::Color, self.max_texture_size),
                    texture: ColorAtlasTexture::Wgpu(texture, texture_view),
                });
                true
            }
            #[cfg(target_os = "macos")]
            DeviceQueue::Metal { device, .. } => {
                let descriptor = metal::TextureDescriptor::new();
                descriptor.set_pixel_format(metal::MTLPixelFormat::RGBA8Unorm);
                descriptor.set_width(self.max_texture_size as u64);
                descriptor.set_height(self.max_texture_size as u64);
                descriptor.set_usage(
                    metal::MTLTextureUsage::ShaderRead | metal::MTLTextureUsage::ShaderWrite,
                );
                let texture = device.new_texture(&descriptor);
                texture.set_label(&format!("Sugarloaf Rich Text Color Atlas {}", atlas_index));

                self.color_atlases.push(ColorAtlasWithTexture {
                    atlas: Atlas::new(AtlasKind::Color, self.max_texture_size),
                    texture: ColorAtlasTexture::Metal(texture),
                });
                true
            }
        }
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

        match entry.atlas_kind {
            AtlasKind::Mask => {
                self.mask_atlas.alloc.deallocate(entry.x, entry.y, entry.width);
            }
            AtlasKind::Color => {
                if let Some(atlas_with_texture) = self.color_atlases.get_mut(entry.color_atlas_index) {
                    atlas_with_texture.atlas.alloc.deallocate(entry.x, entry.y, entry.width);
                }
            }
        }

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

        // All entries use atlas coordinates
        let s = 1. / self.max_texture_size as f32;
        Some(ImageLocation {
            min: (entry.x as f32 * s, entry.y as f32 * s),
            max: (
                (entry.x + entry.width) as f32 * s,
                (entry.y + entry.height) as f32 * s,
            ),
        })
    }

    /// Clears all entries and resets atlases. Used when fonts change.
    pub fn clear_atlas(&mut self) {
        self.entries.clear();

        // Reset mask atlas
        self.mask_atlas = Atlas::new(AtlasKind::Mask, self.max_texture_size);

        // Keep only first color atlas, reset others
        if let Some(first) = self.color_atlases.first_mut() {
            first.atlas = Atlas::new(AtlasKind::Color, self.max_texture_size);
        }
        self.color_atlases.truncate(1);

        tracing::info!("Atlases cleared, {} color atlas(es) remaining", self.color_atlases.len());
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
        match &context.inner {
            ContextType::Wgpu(wgpu_context) => {
                // Process mask atlas
                if self.mask_atlas.dirty {
                    #[cfg_attr(not(target_os = "macos"), expect(irrefutable_let_patterns))]
                    if let DeviceQueue::Wgpu { mask_texture, queue, .. } = &self.device_queue {
                        let texture_size = wgpu::Extent3d {
                            width: self.max_texture_size as u32,
                            height: self.max_texture_size as u32,
                            depth_or_array_layers: 1,
                        };

                        queue.write_texture(
                            wgpu::TexelCopyTextureInfo {
                                texture: mask_texture,
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
                }

                // Process all color atlases
                for atlas_with_texture in &mut self.color_atlases {
                    if atlas_with_texture.atlas.dirty {
                        #[cfg_attr(not(target_os = "macos"), expect(irrefutable_let_patterns))]
                        if let ColorAtlasTexture::Wgpu(texture, _) = &atlas_with_texture.texture {
                            let texture_size = wgpu::Extent3d {
                                width: self.max_texture_size as u32,
                                height: self.max_texture_size as u32,
                                depth_or_array_layers: 1,
                            };

                            wgpu_context.queue.write_texture(
                                wgpu::TexelCopyTextureInfo {
                                    texture,
                                    mip_level: 0,
                                    origin: wgpu::Origin3d::ZERO,
                                    aspect: wgpu::TextureAspect::All,
                                },
                                &atlas_with_texture.atlas.buffer,
                                wgpu::TexelCopyBufferLayout {
                                    offset: 0,
                                    bytes_per_row: Some(
                                        self.max_texture_size as u32
                                            * atlas_with_texture.atlas.channels as u32,
                                    ),
                                    rows_per_image: Some(self.max_texture_size as u32),
                                },
                                texture_size,
                            );

                            atlas_with_texture.atlas.fresh = false;
                            atlas_with_texture.atlas.dirty = false;
                        }
                    }
                }
            }
            #[cfg(target_os = "macos")]
            ContextType::Metal(_metal_context) => {
                // Process mask atlas
                if self.mask_atlas.dirty {
                    if let DeviceQueue::Metal { mask_texture, .. } = &self.device_queue {
                        let region = metal::MTLRegion {
                            origin: metal::MTLOrigin { x: 0, y: 0, z: 0 },
                            size: metal::MTLSize {
                                width: self.max_texture_size as u64,
                                height: self.max_texture_size as u64,
                                depth: 1,
                            },
                        };

                        mask_texture.replace_region(
                            region,
                            0,
                            self.mask_atlas.buffer.as_ptr() as *const std::ffi::c_void,
                            self.max_texture_size as u64 * 1, // 1 byte per pixel for R8
                        );

                        self.mask_atlas.fresh = false;
                        self.mask_atlas.dirty = false;
                    }
                }

                // Process all color atlases
                for atlas_with_texture in &mut self.color_atlases {
                    if atlas_with_texture.atlas.dirty {
                        #[cfg(target_os = "macos")]
                        if let ColorAtlasTexture::Metal(texture) = &atlas_with_texture.texture {
                            let region = metal::MTLRegion {
                                origin: metal::MTLOrigin { x: 0, y: 0, z: 0 },
                                size: metal::MTLSize {
                                    width: self.max_texture_size as u64,
                                    height: self.max_texture_size as u64,
                                    depth: 1,
                                },
                            };

                            texture.replace_region(
                                region,
                                0,
                                atlas_with_texture.atlas.buffer.as_ptr() as *const std::ffi::c_void,
                                self.max_texture_size as u64 * 4, // 4 bytes per pixel for RGBA8
                            );

                            atlas_with_texture.atlas.fresh = false;
                            atlas_with_texture.atlas.dirty = false;
                        }
                    }
                }
            }
        }
    }

    /// Get all texture views for WebGPU rendering (for texture array)
    pub fn get_texture_views(&self) -> Vec<&wgpu::TextureView> {
        self.color_atlases
            .iter()
            .filter_map(|atlas_with_texture| {
                #[cfg_attr(not(target_os = "macos"), expect(irrefutable_let_patterns))]
                if let ColorAtlasTexture::Wgpu(_, view) = &atlas_with_texture.texture {
                    Some(view)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get all Metal textures for Metal rendering (for texture array)
    #[cfg(target_os = "macos")]
    pub fn get_metal_textures(&self) -> Vec<&metal::Texture> {
        self.color_atlases
            .iter()
            .filter_map(|atlas_with_texture| {
                if let ColorAtlasTexture::Metal(texture) = &atlas_with_texture.texture {
                    Some(texture)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get the number of color atlases (for array size)
    pub fn get_atlas_count(&self) -> usize {
        self.color_atlases.len()
    }

    /// Get the mask texture view for WebGPU rendering
    pub fn get_mask_texture_view(&self) -> Option<&wgpu::TextureView> {
        match &self.device_queue {
            DeviceQueue::Wgpu { mask_texture_view, .. } => Some(mask_texture_view),
            #[cfg(target_os = "macos")]
            _ => None,
        }
    }

    /// Get the mask texture for Metal rendering
    #[cfg(target_os = "macos")]
    pub fn get_mask_texture(&self) -> Option<&metal::Texture> {
        match &self.device_queue {
            DeviceQueue::Metal { mask_texture, .. } => Some(mask_texture),
            _ => None,
        }
    }

    /// Get the atlas index for a given image (for setting vertex layer)
    pub fn get_atlas_index(&self, image: ImageId) -> Option<usize> {
        let entry = self.entries.get(image.index())?;
        if !entry.allocated {
            return None;
        }
        if entry.atlas_kind == AtlasKind::Color {
            Some(entry.color_atlas_index)
        } else {
            // Mask atlas is always index 0 (but we may want to handle this differently)
            None
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
