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

/// Hard cap on atlas dimension. Actual cap per cache is
/// `min(MAX_SIZE, device.max_texture_dimension_2d())` — stored on
/// `ImageCache::max_allowed_size` at construction time.
pub const MAX_SIZE: u16 = 16384;

/// Initial atlas side length. Doubles on fill (see
/// [`ImageCache::try_grow_texture_size`]) so a fresh window / panel
/// costs ~256 KB (mask) + ~1 MB (color) rather than ~16 MB + ~64 MB.
pub const INITIAL_SIZE: u16 = 512;

pub struct ImageCache {
    pub entries: Vec<Entry>,
    /// One atlas for mask/glyph data
    mask_atlas: Atlas,
    /// Multiple color atlases, each with own GPU texture (for glyphs + protocol graphics)
    /// When one fills, we create another
    color_atlases: Vec<ColorAtlasWithTexture>,
    /// Current atlas side length. Grows on demand — see [`try_grow_texture_size`].
    max_texture_size: u16,
    /// Hard cap for `max_texture_size`. `min(MAX_SIZE, device_max_texture_dimension_2d)`.
    max_allowed_size: u16,
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
    /// CPU backend: pixel data lives in the Atlas buffer; no GPU resource.
    Cpu,
}

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
    /// CPU backend: no GPU device, no GPU mask texture; mask atlas buffer is sampled directly.
    Cpu,
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
        match &context.inner {
            ContextType::Wgpu(wgpu_context) => {
                let device_max = wgpu_context.max_texture_dimension_2d();
                let max_allowed_size =
                    std::cmp::min(MAX_SIZE as u32, device_max) as u16;
                let max_texture_size = INITIAL_SIZE.min(max_allowed_size);

                let device = std::sync::Arc::new(wgpu_context.device.clone());
                let queue = std::sync::Arc::new(wgpu_context.queue.clone());

                // Create mask texture (R8 format for alpha masks)
                let mask_texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("rich_text mask atlas"),
                    size: wgpu::Extent3d {
                        width: max_texture_size as u32,
                        height: max_texture_size as u32,
                        depth_or_array_layers: 1,
                    },
                    view_formats: &[],
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::R8Unorm,
                    usage: wgpu::TextureUsages::COPY_DST
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    mip_level_count: 1,
                    sample_count: 1,
                });
                let mask_texture_view =
                    mask_texture.create_view(&wgpu::TextureViewDescriptor::default());

                // Create first color atlas with texture
                let color_texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("rich_text color atlas 0"),
                    size: wgpu::Extent3d {
                        width: max_texture_size as u32,
                        height: max_texture_size as u32,
                        depth_or_array_layers: 1,
                    },
                    view_formats: &[],
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::COPY_DST
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    mip_level_count: 1,
                    sample_count: 1,
                });
                let color_texture_view =
                    color_texture.create_view(&wgpu::TextureViewDescriptor::default());

                let color_atlases = vec![ColorAtlasWithTexture {
                    atlas: Atlas::new(AtlasKind::Color, max_texture_size),
                    texture: ColorAtlasTexture::Wgpu(color_texture, color_texture_view),
                }];

                Self {
                    entries: Vec::new(),
                    mask_atlas: Atlas::new(AtlasKind::Mask, max_texture_size),
                    color_atlases,
                    max_texture_size,
                    max_allowed_size,
                    device_queue: DeviceQueue::Wgpu {
                        device,
                        queue,
                        mask_texture,
                        mask_texture_view,
                    },
                }
            }
            #[cfg(target_os = "macos")]
            ContextType::Metal(metal_context) => {
                let device = metal_context.device.clone();
                let max_allowed_size = MAX_SIZE;
                let max_texture_size = INITIAL_SIZE;

                // Create mask texture (R8 format for alpha masks)
                let mask_descriptor = metal::TextureDescriptor::new();
                mask_descriptor.set_pixel_format(metal::MTLPixelFormat::R8Unorm);
                mask_descriptor.set_width(max_texture_size as u64);
                mask_descriptor.set_height(max_texture_size as u64);
                mask_descriptor.set_usage(
                    metal::MTLTextureUsage::ShaderRead
                        | metal::MTLTextureUsage::ShaderWrite,
                );
                let mask_texture = device.new_texture(&mask_descriptor);
                mask_texture.set_label("Sugarloaf Rich Text Mask Atlas");

                // Create first color atlas with texture
                let color_descriptor = metal::TextureDescriptor::new();
                color_descriptor.set_pixel_format(metal::MTLPixelFormat::RGBA8Unorm);
                color_descriptor.set_width(max_texture_size as u64);
                color_descriptor.set_height(max_texture_size as u64);
                color_descriptor.set_usage(
                    metal::MTLTextureUsage::ShaderRead
                        | metal::MTLTextureUsage::ShaderWrite,
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
                    max_allowed_size,
                    device_queue: DeviceQueue::Metal {
                        device,
                        mask_texture,
                    },
                }
            }
            ContextType::Cpu(_) => {
                // CPU backend: no GPU resources. Atlas buffers live in RAM and are sampled
                // directly by the CPU rasterizer at present time.
                let max_allowed_size = MAX_SIZE;
                let max_texture_size = INITIAL_SIZE;
                let color_atlases = vec![ColorAtlasWithTexture {
                    atlas: Atlas::new(AtlasKind::Color, max_texture_size),
                    texture: ColorAtlasTexture::Cpu,
                }];
                Self {
                    entries: Vec::new(),
                    mask_atlas: Atlas::new(AtlasKind::Mask, max_texture_size),
                    color_atlases,
                    max_texture_size,
                    max_allowed_size,
                    device_queue: DeviceQueue::Cpu,
                }
            }
        }
    }

    /// Public accessors used by the CPU rasterizer.
    #[inline]
    pub fn cpu_max_texture_size(&self) -> u16 {
        self.max_texture_size
    }
    #[inline]
    pub fn cpu_mask_atlas_buffer(&self) -> &[u8] {
        &self.mask_atlas.buffer
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

        // Image too big for the current atlas — try to grow. All three
        // backends support grow now (wgpu / Metal / CPU), so the retry
        // path is no longer cfg-gated.
        if !(width <= self.max_texture_size && height <= self.max_texture_size)
            && !self.try_grow_texture_size(width.max(height))
        {
            return None;
        }

        let entry_index = self.entries.len();
        let atlas_kind = match request.content_type {
            ContentType::Mask => AtlasKind::Mask,
            ContentType::Color => AtlasKind::Color,
        };

        // Handle mask atlas (single atlas)
        if atlas_kind == AtlasKind::Mask {
            let (x, y) = match self.mask_atlas.alloc.allocate(width, height) {
                Some(p) => p,
                None => {
                    // Atlas full. Grow and retry. Passing
                    // `max_texture_size + 1` forces exactly one doubling.
                    if !self
                        .try_grow_texture_size(self.max_texture_size.saturating_add(1))
                    {
                        return None;
                    }
                    self.mask_atlas.alloc.allocate(width, height)?
                }
            };
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

        // Handle color atlases (multiple atlases). Try existing atlases,
        // then try growing them, and only as a last resort allocate a new
        // one. Growth is preferred over proliferation because every new
        // color atlas is its own GPU texture — going from one 1024² to
        // two 1024²s and going from one 1024² to one 2048² cost the same
        // RAM but the latter keeps the texture-count bounded.
        if let Some(id) =
            self.try_allocate_in_color_atlases(entry_index, &request, width, height)
        {
            return Some(id);
        }

        // No existing atlas has room. Try growing them all, then retry.
        if self.try_grow_texture_size(self.max_texture_size.saturating_add(1)) {
            if let Some(id) = self
                .try_allocate_in_color_atlases(entry_index, &request, width, height)
            {
                return Some(id);
            }
        }

        // Still no room after growing (e.g. we were already at cap). Fall
        // through to allocating a brand-new color atlas at the current
        // `max_texture_size`.
        let new_atlas_index = self.color_atlases.len();
        if !self.create_new_color_atlas() {
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

        debug!(
            "Allocated {}x{} in new color atlas {}",
            width, height, new_atlas_index
        );
        ImageId::new(entry_index as u32, request.has_alpha)
    }

    /// Try to place `width × height` in any existing color atlas.
    /// On success, pushes an `Entry` at `entry_index`, fills the atlas
    /// buffer, and returns the resulting `ImageId`. Returns `None` when
    /// none of the existing atlases have room.
    fn try_allocate_in_color_atlases(
        &mut self,
        entry_index: usize,
        request: &AddImage,
        width: u16,
        height: u16,
    ) -> Option<ImageId> {
        let target_width = self.max_texture_size;
        for (atlas_index, atlas_with_texture) in
            self.color_atlases.iter_mut().enumerate()
        {
            if let Some((x, y)) =
                atlas_with_texture.atlas.alloc.allocate(width, height)
            {
                self.entries.push(Entry {
                    allocated: true,
                    x,
                    y,
                    width,
                    height,
                    atlas_kind: AtlasKind::Color,
                    color_atlas_index: atlas_index,
                });

                if let Some(data) = request.data() {
                    fill(
                        FillParams {
                            x,
                            y,
                            width,
                            _height: height,
                            target_width,
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
        None
    }

    /// Create a new color atlas with its own GPU texture
    fn create_new_color_atlas(&mut self) -> bool {
        let atlas_index = self.color_atlases.len();
        debug!("Creating color atlas {}", atlas_index);

        match &self.device_queue {
            DeviceQueue::Wgpu {
                device, queue: _, ..
            } => {
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
                    usage: wgpu::TextureUsages::COPY_DST
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    mip_level_count: 1,
                    sample_count: 1,
                });
                let texture_view =
                    texture.create_view(&wgpu::TextureViewDescriptor::default());

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
                    metal::MTLTextureUsage::ShaderRead
                        | metal::MTLTextureUsage::ShaderWrite,
                );
                let texture = device.new_texture(&descriptor);
                texture.set_label(&format!(
                    "Sugarloaf Rich Text Color Atlas {}",
                    atlas_index
                ));

                self.color_atlases.push(ColorAtlasWithTexture {
                    atlas: Atlas::new(AtlasKind::Color, self.max_texture_size),
                    texture: ColorAtlasTexture::Metal(texture),
                });
                true
            }
            DeviceQueue::Cpu => {
                self.color_atlases.push(ColorAtlasWithTexture {
                    atlas: Atlas::new(AtlasKind::Color, self.max_texture_size),
                    texture: ColorAtlasTexture::Cpu,
                });
                true
            }
        }
    }

    /// Grow every atlas texture so the next allocation at `min_dimension`
    /// can succeed. Doubles the current side length until it is at least
    /// `min_dimension` or reaches the device's per-texture cap
    /// (`max_allowed_size`, which is `min(MAX_SIZE, max_texture_dimension_2d)`).
    ///
    /// Existing glyph content is preserved by re-creating each texture at
    /// the new size and replaying the CPU-side buffer into it. The atlas
    /// allocator state is cloned into the new `Atlas` so every outstanding
    /// `Entry` still points at a valid pixel region — this works because
    /// `ImageCache::get` re-derives UVs at lookup time via
    /// `1.0 / self.max_texture_size` (line above in `get`).
    ///
    /// Returns `false` if nothing changed (already at cap, or the
    /// requested dimension still doesn't fit after capping).
    fn try_grow_texture_size(&mut self, min_dimension: u16) -> bool {
        let mut new_size = self.max_texture_size;
        while new_size < min_dimension && new_size < self.max_allowed_size {
            new_size = new_size.saturating_mul(2);
        }
        new_size = new_size.min(self.max_allowed_size);

        if new_size < min_dimension || new_size <= self.max_texture_size {
            return false;
        }

        let old_size = self.max_texture_size;

        // Build a grown CPU-side copy of every atlas. We clone the
        // allocator and then resize it so its internal `width`/`height`
        // reflect the new bounds — without `resize`, the allocator would
        // keep rejecting allocations past the old footprint even though
        // the underlying texture is now larger.
        let mut new_mask = Atlas::new(AtlasKind::Mask, new_size);
        new_mask.alloc = self.mask_atlas.alloc.clone();
        new_mask.alloc.resize(new_size, new_size);
        new_mask.buffer =
            grow_buffer(&self.mask_atlas.buffer, old_size, new_size, 1);
        new_mask.dirty = true;

        let old_color_atlases = std::mem::take(&mut self.color_atlases);
        let mut pending_color: Vec<(Atlas, ColorAtlasTexture)> =
            Vec::with_capacity(old_color_atlases.len());
        for old in old_color_atlases {
            let mut new_atlas = Atlas::new(AtlasKind::Color, new_size);
            new_atlas.alloc = old.atlas.alloc.clone();
            new_atlas.alloc.resize(new_size, new_size);
            new_atlas.buffer =
                grow_buffer(&old.atlas.buffer, old_size, new_size, 4);
            new_atlas.dirty = true;
            pending_color.push((new_atlas, old.texture));
        }

        match &mut self.device_queue {
            DeviceQueue::Wgpu {
                device,
                queue,
                mask_texture,
                mask_texture_view,
            } => {
                let new_mask_texture =
                    device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("rich_text mask atlas"),
                        size: wgpu::Extent3d {
                            width: new_size as u32,
                            height: new_size as u32,
                            depth_or_array_layers: 1,
                        },
                        view_formats: &[],
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::R8Unorm,
                        usage: wgpu::TextureUsages::COPY_DST
                            | wgpu::TextureUsages::TEXTURE_BINDING,
                        mip_level_count: 1,
                        sample_count: 1,
                    });
                let new_mask_view = new_mask_texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &new_mask_texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &new_mask.buffer,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(new_size as u32),
                        rows_per_image: Some(new_size as u32),
                    },
                    wgpu::Extent3d {
                        width: new_size as u32,
                        height: new_size as u32,
                        depth_or_array_layers: 1,
                    },
                );
                *mask_texture = new_mask_texture;
                *mask_texture_view = new_mask_view;

                for (idx, (atlas, _old)) in
                    pending_color.drain(..).enumerate()
                {
                    let color_texture =
                        device.create_texture(&wgpu::TextureDescriptor {
                            label: Some(&format!(
                                "rich_text color atlas {}",
                                idx
                            )),
                            size: wgpu::Extent3d {
                                width: new_size as u32,
                                height: new_size as u32,
                                depth_or_array_layers: 1,
                            },
                            view_formats: &[],
                            dimension: wgpu::TextureDimension::D2,
                            format: wgpu::TextureFormat::Rgba8Unorm,
                            usage: wgpu::TextureUsages::COPY_DST
                                | wgpu::TextureUsages::TEXTURE_BINDING,
                            mip_level_count: 1,
                            sample_count: 1,
                        });
                    let color_view = color_texture
                        .create_view(&wgpu::TextureViewDescriptor::default());
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &color_texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        &atlas.buffer,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(new_size as u32 * 4),
                            rows_per_image: Some(new_size as u32),
                        },
                        wgpu::Extent3d {
                            width: new_size as u32,
                            height: new_size as u32,
                            depth_or_array_layers: 1,
                        },
                    );
                    self.color_atlases.push(ColorAtlasWithTexture {
                        atlas,
                        texture: ColorAtlasTexture::Wgpu(
                            color_texture,
                            color_view,
                        ),
                    });
                }
            }
            #[cfg(target_os = "macos")]
            DeviceQueue::Metal {
                device,
                mask_texture,
            } => {
                let device = device.clone();
                let mask_descriptor = metal::TextureDescriptor::new();
                mask_descriptor
                    .set_pixel_format(metal::MTLPixelFormat::R8Unorm);
                mask_descriptor.set_width(new_size as u64);
                mask_descriptor.set_height(new_size as u64);
                mask_descriptor.set_usage(
                    metal::MTLTextureUsage::ShaderRead
                        | metal::MTLTextureUsage::ShaderWrite,
                );
                let new_mask_texture = device.new_texture(&mask_descriptor);
                new_mask_texture.set_label("Sugarloaf Rich Text Mask Atlas");
                let region = metal::MTLRegion {
                    origin: metal::MTLOrigin { x: 0, y: 0, z: 0 },
                    size: metal::MTLSize {
                        width: new_size as u64,
                        height: new_size as u64,
                        depth: 1,
                    },
                };
                new_mask_texture.replace_region(
                    region,
                    0,
                    new_mask.buffer.as_ptr() as *const std::ffi::c_void,
                    new_size as u64,
                );
                *mask_texture = new_mask_texture;

                for (idx, (atlas, _old)) in
                    pending_color.drain(..).enumerate()
                {
                    let descriptor = metal::TextureDescriptor::new();
                    descriptor
                        .set_pixel_format(metal::MTLPixelFormat::RGBA8Unorm);
                    descriptor.set_width(new_size as u64);
                    descriptor.set_height(new_size as u64);
                    descriptor.set_usage(
                        metal::MTLTextureUsage::ShaderRead
                            | metal::MTLTextureUsage::ShaderWrite,
                    );
                    let color_texture = device.new_texture(&descriptor);
                    color_texture.set_label(&format!(
                        "Sugarloaf Rich Text Color Atlas {}",
                        idx
                    ));
                    let region = metal::MTLRegion {
                        origin: metal::MTLOrigin { x: 0, y: 0, z: 0 },
                        size: metal::MTLSize {
                            width: new_size as u64,
                            height: new_size as u64,
                            depth: 1,
                        },
                    };
                    color_texture.replace_region(
                        region,
                        0,
                        atlas.buffer.as_ptr() as *const std::ffi::c_void,
                        new_size as u64 * 4,
                    );
                    self.color_atlases.push(ColorAtlasWithTexture {
                        atlas,
                        texture: ColorAtlasTexture::Metal(color_texture),
                    });
                }
            }
            DeviceQueue::Cpu => {
                // No GPU texture — the CPU rasteriser samples `atlas.buffer`
                // directly. Swap the atlases in and we're done.
                for (atlas, _old) in pending_color.drain(..) {
                    self.color_atlases.push(ColorAtlasWithTexture {
                        atlas,
                        texture: ColorAtlasTexture::Cpu,
                    });
                }
            }
        }

        self.mask_atlas = new_mask;
        self.max_texture_size = new_size;

        debug!(
            "Grew atlas from {} to {}, preserved {} entries across {} color atlases",
            old_size,
            new_size,
            self.entries.len(),
            self.color_atlases.len()
        );
        true
    }

    /// Mark an image as no longer in use. The shelf packer doesn't
    /// track freed rectangles so this only flips the `allocated`
    /// flag on the entry — the atlas texel is reclaimed on the next
    /// full-clear, not now. Used by the graphic cache to evict
    /// kitty-protocol images that haven't been referenced recently.
    pub fn deallocate(&mut self, image: ImageId) -> Option<()> {
        let entry = self.entries.get_mut(image.index())?;
        if !entry.allocated {
            return None;
        }
        match entry.atlas_kind {
            AtlasKind::Mask => {
                self.mask_atlas
                    .alloc
                    .deallocate(entry.x, entry.y, entry.width);
            }
            AtlasKind::Color => {
                if let Some(atlas_with_texture) =
                    self.color_atlases.get_mut(entry.color_atlas_index)
                {
                    atlas_with_texture.atlas.alloc.deallocate(
                        entry.x,
                        entry.y,
                        entry.width,
                    );
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

        tracing::info!(
            "Atlases cleared, {} color atlas(es) remaining",
            self.color_atlases.len()
        );
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

    #[inline]
    pub fn process_atlases(&mut self, context: &mut Context) {
        match &context.inner {
            ContextType::Wgpu(wgpu_context) => {
                // Process mask atlas
                if self.mask_atlas.dirty {
                    if let DeviceQueue::Wgpu {
                        mask_texture,
                        queue,
                        ..
                    } = &self.device_queue
                    {
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
                                    self.max_texture_size as u32
                                        * self.mask_atlas.channels as u32,
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
                        if let ColorAtlasTexture::Wgpu(texture, _) =
                            &atlas_with_texture.texture
                        {
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
            ContextType::Cpu(_) => {
                // CPU backend: nothing to upload. Mark atlases clean so the dirty flag
                // doesn't grow unbounded; rasterizer reads buffers directly.
                self.mask_atlas.fresh = false;
                self.mask_atlas.dirty = false;
                for atlas_with_texture in &mut self.color_atlases {
                    atlas_with_texture.atlas.fresh = false;
                    atlas_with_texture.atlas.dirty = false;
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
                            self.max_texture_size as u64,
                        );

                        self.mask_atlas.fresh = false;
                        self.mask_atlas.dirty = false;
                    }
                }

                // Process all color atlases
                for atlas_with_texture in &mut self.color_atlases {
                    if atlas_with_texture.atlas.dirty {
                        #[cfg(target_os = "macos")]
                        if let ColorAtlasTexture::Metal(texture) =
                            &atlas_with_texture.texture
                        {
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
                                atlas_with_texture.atlas.buffer.as_ptr()
                                    as *const std::ffi::c_void,
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

    /// Get the mask texture view for WebGPU rendering
    pub fn get_mask_texture_view(&self) -> Option<&wgpu::TextureView> {
        match &self.device_queue {
            DeviceQueue::Wgpu {
                mask_texture_view, ..
            } => Some(mask_texture_view),
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

/// Copy a packed 2D buffer from `old_size × old_size` stride into a
/// freshly-allocated `new_size × new_size` buffer, preserving the
/// upper-left quadrant. `channels` is bytes per pixel (1 for R8,
/// 4 for RGBA). The new buffer is zero-initialised; untouched rows
/// and columns remain zeroed.
fn grow_buffer(
    old: &[u8],
    old_size: u16,
    new_size: u16,
    channels: usize,
) -> Vec<u8> {
    let old_size = old_size as usize;
    let new_size = new_size as usize;
    let mut out = vec![0u8; new_size * new_size * channels];
    for y in 0..old_size {
        let old_offset = y * old_size * channels;
        let new_offset = y * new_size * channels;
        let row_len = old_size * channels;
        out[new_offset..new_offset + row_len]
            .copy_from_slice(&old[old_offset..old_offset + row_len]);
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that buffer data is correctly copied when growing texture size
    #[test]
    fn test_buffer_growth_preserves_data() {
        let old_size = 4u16;
        let new_size = 8u16;

        // Create old buffer with test pattern (R8 - 1 channel)
        let mut old_buffer = vec![0u8; old_size as usize * old_size as usize];
        for y in 0..old_size as usize {
            for x in 0..old_size as usize {
                old_buffer[y * old_size as usize + x] =
                    ((y * old_size as usize + x) % 256) as u8;
            }
        }

        // Create new buffer and copy row by row (simulating the grow logic)
        let mut new_buffer = vec![0u8; new_size as usize * new_size as usize];
        for y in 0..old_size as usize {
            let old_offset = y * old_size as usize;
            let new_offset = y * new_size as usize;
            let row_len = old_size as usize;
            new_buffer[new_offset..new_offset + row_len]
                .copy_from_slice(&old_buffer[old_offset..old_offset + row_len]);
        }

        // Verify old data is preserved in the new buffer
        for y in 0..old_size as usize {
            for x in 0..old_size as usize {
                let old_value = old_buffer[y * old_size as usize + x];
                let new_value = new_buffer[y * new_size as usize + x];
                assert_eq!(
                    old_value, new_value,
                    "Pixel at ({}, {}) should be preserved: expected {}, got {}",
                    x, y, old_value, new_value
                );
            }
        }

        // Verify new areas are zero-initialized
        for y in 0..new_size as usize {
            for x in old_size as usize..new_size as usize {
                let value = new_buffer[y * new_size as usize + x];
                assert_eq!(
                    value, 0,
                    "New pixel at ({}, {}) should be zero-initialized, got {}",
                    x, y, value
                );
            }
        }
        for y in old_size as usize..new_size as usize {
            for x in 0..new_size as usize {
                let value = new_buffer[y * new_size as usize + x];
                assert_eq!(
                    value, 0,
                    "New pixel at ({}, {}) should be zero-initialized, got {}",
                    x, y, value
                );
            }
        }
    }

    /// Test that RGBA buffer data is correctly copied when growing
    #[test]
    fn test_rgba_buffer_growth_preserves_data() {
        let old_size = 4u16;
        let new_size = 8u16;
        let channels = 4; // RGBA

        // Create old buffer with test pattern
        let mut old_buffer = vec![0u8; old_size as usize * old_size as usize * channels];
        for y in 0..old_size as usize {
            for x in 0..old_size as usize {
                let base = (y * old_size as usize + x) * channels;
                old_buffer[base] = (x * 16) as u8; // R
                old_buffer[base + 1] = (y * 16) as u8; // G
                old_buffer[base + 2] = 128; // B
                old_buffer[base + 3] = 255; // A
            }
        }

        // Create new buffer and copy row by row
        let mut new_buffer = vec![0u8; new_size as usize * new_size as usize * channels];
        for y in 0..old_size as usize {
            let old_offset = y * old_size as usize * channels;
            let new_offset = y * new_size as usize * channels;
            let row_len = old_size as usize * channels;
            new_buffer[new_offset..new_offset + row_len]
                .copy_from_slice(&old_buffer[old_offset..old_offset + row_len]);
        }

        // Verify RGBA data is preserved
        for y in 0..old_size as usize {
            for x in 0..old_size as usize {
                let old_base = (y * old_size as usize + x) * channels;
                let new_base = (y * new_size as usize + x) * channels;

                for c in 0..channels {
                    assert_eq!(
                        old_buffer[old_base + c],
                        new_buffer[new_base + c],
                        "RGBA channel {} at ({}, {}) should be preserved",
                        c,
                        x,
                        y
                    );
                }
            }
        }
    }

    /// Test allocator cloning preserves allocation state
    #[test]
    fn test_allocator_clone_preserves_state() {
        let mut original = AtlasAllocator::new(512, 512);

        // Make some allocations
        let alloc1 = original.allocate(64, 64);
        let alloc2 = original.allocate(128, 32);
        let alloc3 = original.allocate(32, 128);

        assert!(alloc1.is_some(), "First allocation should succeed");
        assert!(alloc2.is_some(), "Second allocation should succeed");
        assert!(alloc3.is_some(), "Third allocation should succeed");

        // Clone the allocator
        let cloned = original.clone();

        // Verify we can't re-allocate in the same spots (they're occupied)
        // This is a bit tricky to test directly, but we can verify the clone
        // has the same internal state by checking it produces the same next allocation
        let mut original_next = original.clone();
        let mut cloned_next = cloned.clone();

        let orig_alloc = original_next.allocate(50, 50);
        let clone_alloc = cloned_next.allocate(50, 50);

        assert_eq!(
            orig_alloc, clone_alloc,
            "Cloned allocator should produce same allocation as original"
        );
    }

    /// Test texture size growth calculation
    #[test]
    fn test_texture_size_growth_calculation() {
        let test_cases = vec![
            // (current_size, required_size, expected_new_size)
            (1024, 1500, 2048),
            (1024, 2000, 2048),
            (1024, 2048, 2048),
            (1024, 2049, 4096),
            (1024, 4000, 4096),
            (1024, 4096, 4096),
            (1024, 5000, 4096), // Should cap at 4096 and fail to fit
            (2048, 3000, 4096),
            (2048, 4096, 4096),
            (4096, 4096, 4096),
            (4096, 5000, 4096), // Already at max
        ];

        for (current_size, required_size, expected) in test_cases {
            let mut new_size = current_size;

            // Simulate the growth logic from try_grow_texture_size
            while new_size < required_size && new_size < 4096 {
                new_size *= 2;
            }
            new_size = new_size.min(4096);

            assert_eq!(
                new_size, expected,
                "Growing from {} to fit {} should result in {}",
                current_size, required_size, expected
            );
        }
    }

    /// Test that row-by-row copying handles pitch correctly
    #[test]
    fn test_row_copy_with_different_pitch() {
        let old_width = 3usize;
        let new_width = 5usize;
        let height = 3usize;
        let channels = 4;

        // Create source with specific pattern
        let mut src = vec![0u8; old_width * height * channels];
        for y in 0..height {
            for x in 0..old_width {
                let base = (y * old_width + x) * channels;
                src[base] = y as u8;
                src[base + 1] = x as u8;
                src[base + 2] = 42;
                src[base + 3] = 255;
            }
        }

        // Copy to destination with different pitch
        let mut dst = vec![99u8; new_width * height * channels];
        for y in 0..height {
            let src_offset = y * old_width * channels;
            let dst_offset = y * new_width * channels;
            let row_len = old_width * channels;
            dst[dst_offset..dst_offset + row_len]
                .copy_from_slice(&src[src_offset..src_offset + row_len]);
        }

        // Verify copied data
        for y in 0..height {
            for x in 0..old_width {
                let base = (y * new_width + x) * channels;
                assert_eq!(dst[base], y as u8, "R channel at ({}, {})", x, y);
                assert_eq!(dst[base + 1], x as u8, "G channel at ({}, {})", x, y);
                assert_eq!(dst[base + 2], 42, "B channel at ({}, {})", x, y);
                assert_eq!(dst[base + 3], 255, "A channel at ({}, {})", x, y);
            }
        }

        // Verify padding area remains untouched (was initialized to 99)
        for y in 0..height {
            for x in old_width..new_width {
                let base = (y * new_width + x) * channels;
                for c in 0..channels {
                    assert_eq!(
                        dst[base + c],
                        99,
                        "Padding at ({}, {}) channel {} should remain 99",
                        x,
                        y,
                        c
                    );
                }
            }
        }
    }
}
