mod atlas;
mod cache;
pub mod glyph;

use std::sync::Arc;

pub use cache::ImageCache;
pub use glyph::GlyphCache;

/// Identifier for an image in a cache.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct ImageId(u32);

impl ImageId {
    fn new(index: u32, alpha: bool) -> Option<Self> {
        if index & ID_INDEX_MASK != index {
            return None;
        }
        let mut handle = index & ID_INDEX_MASK;
        if alpha {
            handle |= ID_ALPHA_BIT
        }
        Some(Self(handle))
    }

    /// Creates an empty image ID for zero-sized glyphs.
    pub fn empty() -> Self {
        Self(0)
    }

    fn index(self) -> usize {
        (self.0 & ID_INDEX_MASK) as usize
    }

    /// Returns true if this is an empty image ID.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Returns true if the image contains transparency.
    pub fn has_alpha(self) -> bool {
        self.0 & ID_ALPHA_BIT != 0
    }
}

/// Location of an image in a texture.
#[derive(Copy, Clone)]
pub struct ImageLocation {
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
    /// The actual image data.
    pub data: ImageData<'a>,
    /// Content type for atlas selection
    pub content_type: ContentType,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ContentType {
    Mask,  // Alpha mask data (1 channel)
    Color, // Color data (4 channels)
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

const ID_INDEX_MASK: u32 = 0x007FFFFF;
const ID_ALPHA_BIT: u32 = 0x00800000;
