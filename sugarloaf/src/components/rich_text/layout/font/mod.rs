/*!
Font management.
*/

pub mod prelude;

mod builder;
mod context;
mod fallback;
mod family;
mod index;
mod index_data;
mod library;
mod shared_data;
mod system;
mod types;

pub(crate) mod internal {
    pub use super::context::{FontContext, FontGroupId};
}

pub use builder::{FontLibraryBuilder, MmapHint};
pub use family::FamilyList;
pub use index::{FamilyEntry, FontEntry, SourceEntry};
pub use library::FontLibrary;
pub use types::{FamilyId, FamilyKey, FontId, FontKey, SourceId};

use swash::{iter::*, CacheKey, *};

/// Shared reference to a font.
#[derive(Clone)]
pub struct Font {
    data: shared_data::SharedData,
    offset: u32,
    attributes: Attributes,
    key: CacheKey,
}

impl Font {
    /// Returns the primary attributes for the font.
    pub fn attributes(&self) -> Attributes {
        self.as_ref().attributes()
    }

    /// Returns the requested attributes for the font.
    pub fn requested_attributes(&self) -> Attributes {
        self.attributes
    }

    /// Returns an iterator over the localized strings for the font.
    pub fn localized_strings(&self) -> LocalizedStrings {
        self.as_ref().localized_strings()
    }

    /// Returns an iterator over the variations for the font.
    pub fn variations(&self) -> Variations {
        self.as_ref().variations()
    }

    /// Returns an iterator over the named instances for the font.
    pub fn instances(&self) -> Instances {
        self.as_ref().instances()
    }

    /// Returns an iterator over writing systems supported by the font.
    pub fn writing_systems(&self) -> WritingSystems {
        self.as_ref().writing_systems()
    }

    /// Returns an iterator over the features supported by a font.
    pub fn features(&self) -> Features {
        self.as_ref().features()
    }

    /// Returns metrics for the font and the specified normalized variation
    /// coordinates.
    pub fn metrics(&self, coords: &[NormalizedCoord]) -> Metrics {
        self.as_ref().metrics(coords)
    }

    /// Returns glyph metrics for the font and the specified normalized
    /// variation coordinates.
    pub fn glyph_metrics<'a>(
        &'a self,
        coords: &'a [NormalizedCoord],
    ) -> GlyphMetrics<'a> {
        self.as_ref().glyph_metrics(coords)
    }

    /// Returns the character map for the font.
    pub fn charmap(&self) -> Charmap {
        self.as_ref().charmap()
    }

    /// Returns an iterator over the color palettes for the font.
    pub fn color_palettes(&self) -> ColorPalettes {
        self.as_ref().color_palettes()
    }

    /// Returns an iterator over the alpha bitmap strikes for the font.
    pub fn alpha_strikes(&self) -> BitmapStrikes {
        self.as_ref().alpha_strikes()
    }

    /// Returns an iterator over the color bitmap strikes for the font.
    pub fn color_strikes(&self) -> BitmapStrikes {
        self.as_ref().color_strikes()
    }

    /// Returns a unique key for identifying this font.
    pub fn cache_key(&self) -> CacheKey {
        self.key
    }

    /// Returns a borrowed reference to the font.
    pub fn as_ref<'a>(&'a self) -> FontRef<'a> {
        FontRef {
            data: self.data.as_bytes(),
            offset: self.offset,
            key: self.key,
        }
    }
}

impl PartialEq for Font {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Eq for Font {}

impl<'a> From<&'a Font> for FontRef<'a> {
    fn from(f: &'a Font) -> FontRef<'a> {
        f.as_ref()
    }
}
