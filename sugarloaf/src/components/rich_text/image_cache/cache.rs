use crate::context::Context;

use super::atlas::*;
use super::*;

pub struct ImageCache {
    pub entries: Vec<Entry>,
    atlas: Atlas,
    images: Vec<Standalone>,
    buffered_data: Vec<u8>,
    events: Vec<Event>,
    free_entries: u32,
    free_images: u32,
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

pub const SIZE: u32 = 2048;

impl ImageCache {
    /// Creates a new image cache.
    pub fn new(context: &Context) -> Self {
        let device = &context.device;
        // let max_texture_size = max_texture_size.clamp(1024, 8192);
        let max_texture_size = SIZE;

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rich_text create texture"),
            size: wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
            view_formats: &[],
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            mip_level_count: 1,
            sample_count: 1,
        });
        let texture_view =
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            });

        let max_texture_size_u16 = max_texture_size as u16;
        let alloc = AtlasAllocator::new(max_texture_size_u16, max_texture_size_u16);

        Self {
            entries: Vec::new(),
            atlas: Atlas {
                alloc,
                buffer: vec![0u8; max_texture_size as usize * max_texture_size as usize * 4],
                fresh: true,
                dirty: true,
            },
            images: Vec::new(),
            buffered_data: Vec::new(),
            events: Vec::new(),
            free_entries: END_OF_LIST,
            free_images: END_OF_LIST,
            max_texture_size: max_texture_size_u16,
            texture_view,
            texture,
        }
    }

    /// Allocates a new image and optionally fills it with the specified data.
    pub fn allocate(&mut self, request: AddImage) -> Option<ImageId> {
        let width = request.width;
        let height = request.height;
        let _req_data_size = buffer_size(width as u32, height as u32)?;
        let use_atlas = width <= self.max_texture_size
            && height <= (self.max_texture_size / 4);
        let base_flags = if request.evictable {
            ENTRY_EVICTABLE
        } else {
            0
        };
        if !use_atlas {
            println!("should not use atlas");

            // Simply allocate a new texture.
            let has_alpha = request.has_alpha;
            let entry_index = self.alloc_entry()?;
            let image_index = self.alloc_standalone(request)?;
            let entry = self.entries.get_mut(entry_index)?;
            entry.generation = entry.generation.wrapping_add(1);
            entry.flags = base_flags | ENTRY_ALLOCATED | ENTRY_STANDALONE;
            entry.owner = image_index as u16;
            entry.x = 0;
            entry.y = 0;
            entry.width = width;
            entry.height = height;
            return ImageId::new(entry.generation, entry_index as u32, has_alpha);
        }
        // let mut atlas_data = self.alloc_from_atlases(format, width, height);
        // if atlas_data.is_none() {
        //     atlas_data = self.alloc_from_atlases(format, width, height);
        // }

        let atlas_data = self.atlas.alloc.allocate(width, height);

        if atlas_data.is_none() {
            println!("should try to grow or reset atlas");
            // let dim = self.max_texture_size;
            // let atlas_index = self.atlases.len();
            // if atlas_index >= MAX_ATLASES as usize {
            //     return None;
            // }
            // let mut alloc = AtlasAllocator::new(dim, dim);
            // if let Some((x, y)) = alloc.allocate(width, height) {
            //     let buffer = vec![0u8; dim as usize * dim as usize * 4];
            //     let texture_id = TextureId::allocate();
            //     self.atlases.push(Atlas {
            //         format,
            //         alloc,
            //         buffer,
            //         fresh: true,
            //         dirty: true,
            //         texture_id,
            //     });
            //     atlas_data = Some((x, y));
            // } else {
            //     return None;
            // }
        }
        let (x, y) = atlas_data?;
        let entry_index = self.alloc_entry()?;
        let entry = self.entries.get_mut(entry_index)?;
        entry.generation = entry.generation.wrapping_add(1);
        entry.flags = base_flags | ENTRY_ALLOCATED;
        entry.owner = 0;
        entry.x = x;
        entry.y = y;
        entry.width = width;
        entry.height = height;
        if let Some(data) = request.data() {
            fill(
                x,
                y,
                width,
                height,
                data,
                self.max_texture_size,
                &mut self.atlas.buffer,
                4,
            );
            self.atlas.dirty = true;
        }
        ImageId::new(entry.generation, entry_index as u32, request.has_alpha)
    }

    // Evaluate if does make sense to deallocate from atlas and if yes, which case?
    // considering that a terminal uses a short/limited of glyphs compared to a wide text editor
    // if deallocate an image then is necessary to cleanup cache of draw_layout fn
    /// Deallocates the specified image.
    pub fn deallocate(&mut self, image: ImageId) -> Option<()> {
        let entry = self.entries.get_mut(image.index())?;
        if entry.flags & ENTRY_ALLOCATED == 0 || entry.generation != image.generation() {
            return None;
        }
        if entry.flags & ENTRY_STANDALONE != 0 {
            let standalone = self.images.get_mut(entry.owner as usize)?;
            standalone.next = self.free_images;
            self.free_images = entry.owner as u32;
            self.events
                .push(Event::DestroyTexture);
        } else {
            self.atlas.alloc.deallocate(entry.x, entry.y, entry.width);
        }
        entry.flags = 0;
        self.free_entries = image.index() as u32;
        Some(())
    }

    /// Retrieves the image for the specified handle and updates the epoch.
    pub fn get(&self, handle: &ImageId) -> Option<ImageLocation> {
        let entry = self.entries.get(handle.index())?;
        if entry.flags & ENTRY_ALLOCATED == 0 || entry.generation != handle.generation() {
            return None;
        }
        Some(if entry.flags & ENTRY_STANDALONE != 0 {
            ImageLocation {
                min: (0., 0.),
                max: (1., 1.),
            }
        } else {
            let s = 1. / self.max_texture_size as f32;
            ImageLocation {
                min: (entry.x as f32 * s, entry.y as f32 * s),
                max: (
                    (entry.x + entry.width) as f32 * s,
                    (entry.y + entry.height) as f32 * s,
                ),
            }
        })
    }

    /// Returns true if the image is valid.
    pub fn is_valid(&self, image: ImageId) -> bool {
        if let Some(entry) = self.entries.get(image.index()) {
            entry.flags & ENTRY_ALLOCATED != 0 && entry.generation == image.generation()
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
    //     if entry.flags & ENTRY_STANDALONE != 0 {
    //         let image = self.images.get(entry.owner as usize)?;
    //         let texture = image.texture.as_ref()?;
    //         texture.update(data);
    //     } else {
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
    //     }
    //     Some(())
    // }

    #[inline]
    pub fn process_events(&mut self, context: &mut Context) {
        for event in &self.events {
            match event {
                Event::CreateTexture(width, height, data) => {
                    println!("bbb CreateTexture");
                    let data = match &data {
                        Some(PendingData::Inline(data)) => data.data(),
                        Some(PendingData::Buffered(start, end)) => {
                            self.buffered_data.get(*start..*end)
                        }
                        None => None,
                    };
                    let texture_size = wgpu::Extent3d {
                        width: (*width).into(),
                        height: (*height).into(),
                        depth_or_array_layers: 1,
                    };
                    let new_texture =
                        context.device.create_texture(&wgpu::TextureDescriptor {
                            size: texture_size,
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: wgpu::TextureFormat::Rgba8Unorm,
                            usage: wgpu::TextureUsages::TEXTURE_BINDING
                                | wgpu::TextureUsages::COPY_DST,
                            label: Some("rich_text::Cache"),
                            view_formats: &[],
                        });

                    if let Some(data) = data {
                        context.queue.write_texture(
                            // Tells wgpu where to copy the pixel data
                            wgpu::ImageCopyTexture {
                                texture: &new_texture,
                                mip_level: 0,
                                origin: wgpu::Origin3d::ZERO,
                                aspect: wgpu::TextureAspect::All,
                            },
                            // The actual pixel data
                            &data,
                            // The layout of the texture
                            wgpu::ImageDataLayout {
                                offset: 0,
                                bytes_per_row: Some(
                                    (self.max_texture_size * 4).into(),
                                ),
                                rows_per_image: Some((self.max_texture_size).into()),
                            },
                            texture_size,
                        );
                    }

                    self.texture = new_texture;
                    self.texture_view = self.texture.create_view(&wgpu::TextureViewDescriptor {
                        dimension: Some(wgpu::TextureViewDimension::D2Array),
                        ..Default::default()
                    });
                }
                Event::UpdateTexture(region, data) => {
                    println!("bbb UpdateTexture");
                    let [x, y, width, height] = region;
                    let data = match &data {
                        Some(PendingData::Inline(data)) => data.data().unwrap_or(&[]),
                        Some(PendingData::Buffered(start, end)) => {
                            self.buffered_data.get(*start..*end).unwrap_or(&[])
                        }
                        None => &[],
                    };

                    let texture_size = wgpu::Extent3d {
                        width: (*width).into(),
                        height: (*height).into(),
                        depth_or_array_layers: 1,
                    };

                    context.queue.write_texture(
                        // Tells wgpu where to copy the pixel data
                        wgpu::ImageCopyTexture {
                            texture: &self.texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d {
                                x: u32::from(*x),
                                y: u32::from(*y),
                                z: 0,
                            },
                            aspect: wgpu::TextureAspect::All,
                        },
                        // The actual pixel data
                        &data,
                        // The layout of the texture
                        wgpu::ImageDataLayout {
                            offset: 0,
                            bytes_per_row: Some((self.max_texture_size * 4).into()),
                            rows_per_image: Some((self.max_texture_size).into()),
                        },
                        texture_size,
                    );
                }
                Event::DestroyTexture => {
                    // self.textures.remove(&id);
                }
            }
        }
        self.events.clear();
        self.buffered_data.clear();
    }

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
                label: Some("rich_text::Cache"),
                view_formats: &[],
            });

            println!("aaa CreateTexture");

            context.queue.write_texture(
                // Tells wgpu where to copy the pixel data
                wgpu::ImageCopyTexture {
                    texture: &new_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                // The actual pixel data
                &self.atlas.buffer,
                // The layout of the texture
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(
                        (self.max_texture_size * 4).into(),
                    ),
                    rows_per_image: Some((self.max_texture_size).into()),
                },
                texture_size,
            );

            self.texture = new_texture;
            self.texture_view = self.texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            });

            // self.textures.insert(atlas.texture_id, texture);
        } else {
            println!("aaa UpdateTexture");
            // if let Some(texture) = self.textures.get(&atlas.texture_id) {
                // self.bind_group_needs_update = true;
                let texture_size = wgpu::Extent3d {
                    width: (self.max_texture_size).into(),
                    height: (self.max_texture_size).into(),
                    depth_or_array_layers: 1,
                };

                context.queue.write_texture(
                    // Tells wgpu where to copy the pixel data
                    wgpu::ImageCopyTexture {
                        texture: &self.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: 0,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    // The actual pixel data
                    &self.atlas.buffer,
                    // The layout of the texture
                    wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some((self.max_texture_size * 4).into()),
                        rows_per_image: Some((self.max_texture_size).into()),
                    },
                    texture_size,
                );
            // }
        }
        self.atlas.fresh = false;
        self.atlas.dirty = false;
    }

    fn alloc_entry(&mut self) -> Option<usize> {
        Some(if self.free_entries != END_OF_LIST {
            self.free_entries as usize
            // let entry = self.entries.get(index)?;
            // self.free_entries = entry.epoch as u32;
        } else {
            let index = self.entries.len();
            if index >= MAX_ENTRIES as usize {
                return None;
            }
            self.entries.push(Entry::default());
            index
        })
    }

    fn alloc_standalone(&mut self, request: AddImage) -> Option<usize> {
        let width = request.width;
        let height = request.height;
        let index = if self.free_images != END_OF_LIST {
            let index = self.free_images as usize;
            self.free_images = self.images.get(index)?.next;
            index
        } else {
            let index = self.images.len();
            self.images.push(Standalone {
                used: false,
                next: 0,
            });
            index
        };
        let pending_data = match request.data {
            // ImageData::None => None,
            ImageData::Owned(data) => Some(PendingData::Inline(ImageData::Owned(data))),
            ImageData::Shared(data) => Some(PendingData::Inline(ImageData::Shared(data))),
            ImageData::Borrowed(data) => {
                let start = self.buffered_data.len();
                self.buffered_data.extend_from_slice(data);
                let end = self.buffered_data.len();
                Some(PendingData::Buffered(start, end))
            }
        };
        let image = self.images.get_mut(index)?;
        image.used = true;
        self.events.push(Event::CreateTexture(
            width,
            height,
            pending_data,
        ));
        Some(index)
    }
}

#[derive(Default)]
pub struct Entry {
    /// Zero if the entry is free.
    flags: u8,
    /// Generation of this entry. Used to detect stale handles.
    generation: u8,
    /// Owner of the entry. Index into atlases or images depending
    /// on the ENTRY_STANDALONE flag.
    owner: u16,
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

struct Standalone {
    used: bool,
    next: u32,
}

#[allow(clippy::enum_variant_names)]
enum Event {
    CreateTexture(u16, u16, Option<PendingData>),
    #[allow(unused)]
    UpdateTexture([u16; 4], Option<PendingData>),
    DestroyTexture,
}

enum PendingData {
    #[allow(unused)]
    Inline(ImageData<'static>),
    Buffered(usize, usize),
}

#[allow(clippy::too_many_arguments)]
fn fill(
    x: u16,
    y: u16,
    width: u16,
    _height: u16,
    image: &[u8],
    target_width: u16,
    target: &mut [u8],
    channels: u16,
) -> Option<()> {
    let channels = channels as usize;
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
