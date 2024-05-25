use super::atlas::*;
use super::*;

#[derive(Default)]
pub struct ImageCache {
    entries: Vec<Entry>,
    atlases: Vec<Atlas>,
    images: Vec<Standalone>,
    buffered_data: Vec<u8>,
    events: Vec<Event>,
    free_entries: u32,
    free_images: u32,
    max_texture_size: u16,
}

impl ImageCache {
    /// Creates a new image cache.
    pub fn new(max_texture_size: u16) -> Self {
        let max_texture_size = max_texture_size.min(4096).max(1024);
        Self {
            entries: Vec::new(),
            atlases: Vec::new(),
            images: Vec::new(),
            buffered_data: Vec::new(),
            events: Vec::new(),
            free_entries: END_OF_LIST,
            free_images: END_OF_LIST,
            max_texture_size,
        }
    }

    /// Allocates a new image and optionally fills it with the specified data.
    pub fn allocate(&mut self, epoch: Epoch, request: AddImage) -> Option<ImageId> {
        let format = request.format;
        let width = request.width;
        let height = request.height;
        let _req_data_size = request.format.buffer_size(width as u32, height as u32)?;
        let use_atlas = width <= self.max_texture_size
            && height <= (self.max_texture_size / 4)
            && (format == PixelFormat::Rgba8 || format == PixelFormat::A8);
        let base_flags = if request.evictable {
            ENTRY_EVICTABLE
        } else {
            0
        };
        if !use_atlas {
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
            entry.epoch = epoch.0;
            return ImageId::new(entry.generation, entry_index as u32, has_alpha);
        }
        let mut atlas_data = self.alloc_from_atlases(format, width, height);
        if atlas_data.is_none() {
            if epoch.0 > 1 && self.evict_from_atlases(epoch.0 - 1) > 0 {
                atlas_data = self.alloc_from_atlases(format, width, height);
            }

            if atlas_data.is_none() && epoch.0 > 0 && self.evict_from_atlases(epoch.0) > 0
            {
                atlas_data = self.alloc_from_atlases(format, width, height);
            }
        }
        if atlas_data.is_none() {
            let dim = self.max_texture_size;
            let atlas_index = self.atlases.len();
            if atlas_index >= MAX_ATLASES as usize {
                return None;
            }
            let mut alloc = AtlasAllocator::new(dim, dim);
            if let Some((x, y)) = alloc.allocate(width, height) {
                let buffer = vec![0u8; dim as usize * dim as usize * 4];
                let texture_id = TextureId::allocate();
                self.atlases.push(Atlas {
                    format,
                    alloc,
                    buffer,
                    fresh: true,
                    dirty: true,
                    texture_id,
                });
                atlas_data = Some((atlas_index, x, y));
            } else {
                return None;
            }
        }
        let (atlas_index, x, y) = atlas_data?;
        let entry_index = self.alloc_entry()?;
        let entry = self.entries.get_mut(entry_index)?;
        entry.generation = entry.generation.wrapping_add(1);
        entry.flags = base_flags | ENTRY_ALLOCATED;
        entry.owner = atlas_index as u16;
        entry.x = x;
        entry.y = y;
        entry.width = width;
        entry.height = height;
        entry.epoch = epoch.0;
        if let Some(data) = request.data() {
            let atlas = self.atlases.get_mut(atlas_index)?;
            fill(
                x,
                y,
                width,
                height,
                data,
                self.max_texture_size,
                &mut atlas.buffer,
                4,
            );
            atlas.dirty = true;
        }
        ImageId::new(entry.generation, entry_index as u32, request.has_alpha)
    }

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
                .push(Event::DestroyTexture(standalone.texture_id));
        } else {
            let atlas = self.atlases.get_mut(entry.owner as usize)?;
            atlas.alloc.deallocate(entry.x, entry.y, entry.width);
        }
        entry.flags = 0;
        entry.epoch = self.free_entries as u64;
        self.free_entries = image.index() as u32;
        Some(())
    }

    /// Retrieves the image for the specified handle and updates the epoch.
    pub fn get(&mut self, epoch: Epoch, handle: ImageId) -> Option<ImageLocation> {
        let entry = self.entries.get_mut(handle.index())?;
        if entry.flags & ENTRY_ALLOCATED == 0 || entry.generation != handle.generation() {
            return None;
        }
        entry.epoch = epoch.0;
        Some(if entry.flags & ENTRY_STANDALONE != 0 {
            let image = self.images.get(entry.owner as usize)?;
            let texture_id = image.texture_id;
            ImageLocation {
                texture_id,
                min: (0., 0.),
                max: (1., 1.),
            }
        } else {
            let atlas = self.atlases.get(entry.owner as usize)?;
            let texture_id = atlas.texture_id;
            let s = 1. / self.max_texture_size as f32;
            ImageLocation {
                texture_id,
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

    pub fn drain_events(&mut self, mut f: impl FnMut(TextureEvent)) {
        for event in self.events.drain(..) {
            match event {
                Event::CreateTexture(id, format, width, height, data) => {
                    let data = match &data {
                        Some(PendingData::Inline(data)) => data.data(),
                        Some(PendingData::Buffered(start, end)) => {
                            self.buffered_data.get(*start..*end)
                        }
                        None => None,
                    };
                    f(TextureEvent::CreateTexture {
                        id,
                        format,
                        width,
                        height,
                        data,
                    })
                }
                Event::UpdateTexture(id, format, region, data) => {
                    f(TextureEvent::UpdateTexture {
                        id,
                        format,
                        x: region[0],
                        y: region[1],
                        width: region[2],
                        height: region[3],
                        data: match &data {
                            Some(PendingData::Inline(data)) => data.data().unwrap_or(&[]),
                            Some(PendingData::Buffered(start, end)) => {
                                self.buffered_data.get(*start..*end).unwrap_or(&[])
                            }
                            None => &[],
                        },
                    })
                }
                Event::DestroyTexture(id) => {
                    f(TextureEvent::DestroyTexture(id));
                }
            }
        }
        self.buffered_data.clear();
        for atlas in &mut self.atlases {
            if !atlas.dirty {
                continue;
            }
            if atlas.fresh {
                f(TextureEvent::CreateTexture {
                    id: atlas.texture_id,
                    format: atlas.format,
                    width: self.max_texture_size,
                    height: self.max_texture_size,
                    data: Some(&atlas.buffer),
                });
            } else {
                f(TextureEvent::UpdateTexture {
                    id: atlas.texture_id,
                    format: atlas.format,
                    x: 0,
                    y: 0,
                    width: self.max_texture_size,
                    height: self.max_texture_size,
                    data: &atlas.buffer,
                })
            }
            atlas.fresh = false;
            atlas.dirty = false;
        }
    }

    fn evict_from_atlases(&mut self, epoch: u64) -> usize {
        let len = self.entries.len();
        let mut count = 0;
        for i in 0..len {
            if let Some((flags, entry_gen, entry_epoch)) = self
                .entries
                .get(i)
                .map(|e| (e.flags, e.generation, e.epoch))
            {
                if flags & (ENTRY_EVICTABLE | ENTRY_ALLOCATED)
                    != (ENTRY_EVICTABLE | ENTRY_ALLOCATED)
                {
                    continue;
                }
                if entry_epoch < epoch {
                    let handle = ImageId::new(entry_gen, i as u32, false).unwrap();
                    if self.deallocate(handle).is_some() {
                        count += 1;
                    }
                }
            }
        }
        log::info!("rich_text::atlases::cache: evicted {}", count);
        count
    }

    fn alloc_from_atlases(
        &mut self,
        format: PixelFormat,
        width: u16,
        height: u16,
    ) -> Option<(usize, u16, u16)> {
        for (i, atlas) in self.atlases.iter_mut().enumerate() {
            if atlas.format != format {
                continue;
            }
            if let Some((x, y)) = atlas.alloc.allocate(width, height) {
                return Some((i, x, y));
            }
        }
        None
    }

    fn alloc_entry(&mut self) -> Option<usize> {
        Some(if self.free_entries != END_OF_LIST {
            let index = self.free_entries as usize;
            let entry = self.entries.get(index)?;
            self.free_entries = entry.epoch as u32;
            index
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
        let format = request.format;
        let width = request.width;
        let height = request.height;
        let index = if self.free_images != END_OF_LIST {
            let index = self.free_images as usize;
            self.free_images = self.images.get(index)?.next;
            index
        } else {
            let index = self.images.len();
            self.images.push(Standalone {
                texture_id: TextureId(0),
                used: false,
                next: 0,
            });
            index
        };
        let texture_id = TextureId::allocate();
        let pending_data = match request.data {
            // ImageData::None => None,
            // ImageData::Owned(data) => Some(PendingData::Inline(ImageData::Owned(data))),
            // ImageData::Shared(data) => Some(PendingData::Inline(ImageData::Shared(data))),
            ImageData::Borrowed(data) => {
                let start = self.buffered_data.len();
                self.buffered_data.extend_from_slice(data);
                let end = self.buffered_data.len();
                Some(PendingData::Buffered(start, end))
            }
        };
        let image = self.images.get_mut(index)?;
        image.texture_id = texture_id;
        image.used = true;
        self.events.push(Event::CreateTexture(
            texture_id,
            format,
            width,
            height,
            pending_data,
        ));
        Some(index)
    }
}

#[derive(Default)]
struct Entry {
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
    /// Last epoch when this entry was used if allocated. Otherwise,
    /// index of next entry in the free list.
    epoch: u64,
}

struct Atlas {
    format: PixelFormat,
    alloc: AtlasAllocator,
    buffer: Vec<u8>,
    fresh: bool,
    dirty: bool,
    texture_id: TextureId,
}

struct Standalone {
    texture_id: TextureId,
    used: bool,
    next: u32,
}

#[allow(clippy::enum_variant_names)]
enum Event {
    CreateTexture(TextureId, PixelFormat, u16, u16, Option<PendingData>),
    #[allow(unused)]
    UpdateTexture(TextureId, PixelFormat, [u16; 4], Option<PendingData>),
    DestroyTexture(TextureId),
}

enum PendingData {
    #[allow(unused)]
    Inline(ImageData<'static>),
    Buffered(usize, usize),
}

#[derive(Default)]
#[allow(unused)]
struct DirtyRect {
    min: (u16, u16),
    max: (u16, u16),
    empty: bool,
}

#[allow(unused)]
impl DirtyRect {
    fn new() -> Self {
        let mut this = Self::default();
        this.clear();
        this
    }

    fn clear(&mut self) {
        self.empty = true;
        self.min = (u16::MAX, u16::MAX);
        self.max = (u16::MIN, u16::MIN);
    }

    fn add(&mut self, x: u16, y: u16, width: u16, height: u16) {
        self.empty = false;
        let xmax = x + width;
        let ymax = y + height;
        self.min.0 = self.min.0.min(x);
        self.min.1 = self.min.1.min(y);
        self.max.0 = self.max.0.max(xmax);
        self.max.1 = self.max.1.max(ymax);
    }
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
