mod atlas;
mod cache;
mod glyph;

use std::sync::atomic::{AtomicU64, Ordering};
// use std::sync::Arc;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PixelFormat {
    A8,
    Rgba8,
}

impl PixelFormat {
    pub fn buffer_size(&self, width: u32, height: u32) -> Option<usize> {
        let mult = match self {
            Self::A8 => 1,
            Self::Rgba8 => 4,
        };
        (width as usize)
            .checked_add(height as usize)?
            .checked_add(mult)
    }
}

pub use cache::ImageCache;
// pub use glyph::{GlyphCache, GlyphCacheSession, GlyphEntry};
pub use glyph::GlyphCache;

/// Frame counter for managing resource lifetimes.
#[derive(Copy, Clone, Default)]
pub struct Epoch(pub(crate) u64);

/// Identifier for a texture in GPU memory.
#[derive(Copy, Clone, PartialOrd, Ord, PartialEq, Eq, Hash, Debug)]
pub struct TextureId(pub u64);

impl TextureId {
    fn allocate() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

/// Identifier for an image in a cache.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct ImageId(u32);

impl ImageId {
    fn new(generation: u8, index: u32, alpha: bool) -> Option<Self> {
        if index & ID_INDEX_MASK != index {
            return None;
        }
        let mut handle = index & ID_INDEX_MASK;
        handle |= (generation as u32) << 24;
        if alpha {
            handle |= ID_ALPHA_BIT
        }
        Some(Self(handle))
    }

    fn generation(self) -> u8 {
        (self.0 >> 24) as u8
    }

    fn index(self) -> usize {
        (self.0 & ID_INDEX_MASK) as usize
    }

    /// Returns true if the image contains transparency.
    pub fn has_alpha(self) -> bool {
        self.0 & ID_ALPHA_BIT != 0
    }
}

/// Location of an image in a texture.
#[derive(Copy, Clone)]
pub struct ImageLocation {
    /// Texture that contains the image.
    pub texture_id: TextureId,
    /// Mininum x and y texture coordinates.
    pub min: (f32, f32),
    /// Maximum x and y texture coordinates.
    pub max: (f32, f32),
}

/// Data describing a request for caching an image.
#[derive(Clone, Copy)]
pub struct AddImage<'a> {
    /// Format of the image data.
    pub format: PixelFormat,
    /// Width of the image.
    pub width: u16,
    /// Height of the image.
    pub height: u16,
    /// True if the image makes use of an alpha channel.
    pub has_alpha: bool,
    /// True if the cache can evict this image.
    pub evictable: bool,
    /// The actual image data.
    pub data: ImageData<'a>,
}

impl<'a> AddImage<'a> {
    fn data(&'a self) -> Option<&'a [u8]> {
        self.data.data()
    }
}

/// Representations of image data for submission to a cache.
#[derive(Clone, Copy)]
pub enum ImageData<'a> {
    // None,
    Borrowed(&'a [u8]),
    // Owned(Vec<u8>),
    // Shared(Arc<Vec<u8>>),
}

impl<'a> ImageData<'a> {
    fn data(&'a self) -> Option<&'a [u8]> {
        Some(match self {
            // Self::None => return None,
            // Self::Borrowed(data) => *data,
            Self::Borrowed(data) => data,
            // Self::Owned(data) => data,
            // Self::Shared(data) => &*data,
        })
    }
}

/// Event that describes a change in an image cache.
#[derive(Copy, Clone)]
#[allow(clippy::enum_variant_names)]
pub enum TextureEvent<'a> {
    /// Texture creation event.
    CreateTexture {
        id: TextureId,
        format: PixelFormat,
        width: u16,
        height: u16,
        data: Option<&'a [u8]>,
    },
    /// Texture update event.
    UpdateTexture {
        id: TextureId,
        format: PixelFormat,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
        data: &'a [u8],
    },
    /// Texture destruction event.
    DestroyTexture(TextureId),
}

/// Limit on number of atlases before image allocation fails.
const MAX_ATLASES: u16 = 256;

/// Limit on number of standalone images.
// const MAX_IMAGES: u16 = i16::MAX as u16;

/// Limit on total number of images.
const MAX_ENTRIES: u32 = 0x007FFFFF;

/// Sentinel for end of free list.
const END_OF_LIST: u32 = !0;

const ID_INDEX_MASK: u32 = MAX_ENTRIES;
const ID_ALPHA_BIT: u32 = 0x00800000;

const ENTRY_ALLOCATED: u8 = 1;
const ENTRY_STANDALONE: u8 = 2;
const ENTRY_EVICTABLE: u8 = 4;
