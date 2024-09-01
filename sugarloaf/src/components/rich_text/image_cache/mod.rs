mod atlas;
mod cache;
pub mod glyph;

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;

pub use cache::ImageCache;
pub use glyph::GlyphCache;

/// Identifier for a texture in GPU memory.
#[derive(Copy, Clone, PartialOrd, Ord, PartialEq, Eq, Hash, Debug)]
pub struct TextureId(pub i32);

impl TextureId {
    fn allocate() -> Self {
        static COUNTER: AtomicI32 = AtomicI32::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    #[inline]
    pub fn val(&self) -> i32 {
        self.0
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
    /// Minimum x and y texture coordinates.
    pub min: (f32, f32),
    /// Maximum x and y texture coordinates.
    pub max: (f32, f32),
}

/// Data describing a request for caching an image.
#[derive(Clone)]
pub struct AddImage<'a> {
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
#[derive(Clone)]
pub enum ImageData<'a> {
    // None,
    Borrowed(&'a [u8]),
    #[allow(unused)]
    Owned(Vec<u8>),
    #[allow(unused)]
    Shared(Arc<Vec<u8>>),
}

impl<'a> ImageData<'a> {
    fn data(&'a self) -> Option<&'a [u8]> {
        Some(match self {
            // Self::None => return None,
            // Self::Borrowed(data) => *data,
            Self::Borrowed(data) => data,
            Self::Owned(data) => data,
            Self::Shared(data) => data,
        })
    }
}

/// Limit on total number of images.
const MAX_ENTRIES: u32 = 0x007FFFFF;

/// Sentinel for end of free list.
const END_OF_LIST: u32 = !0;

const ID_INDEX_MASK: u32 = MAX_ENTRIES;
const ID_ALPHA_BIT: u32 = 0x00800000;

const ENTRY_ALLOCATED: u8 = 1;
const ENTRY_STANDALONE: u8 = 2;
const ENTRY_EVICTABLE: u8 = 4;
