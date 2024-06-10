// glyph module code along with comments was originally retired from glyph-brush
// https://github.com/alexheretic/glyph-brush
// glyph-brush was originally written Alex Butler (https://github.com/alexheretic)
// and licensed under Apache-2.0 license.

mod builtin;
mod characters;
mod font;
mod linebreak;
mod lines;
mod section;
mod words;

/// Re-exported ab_glyph types.
pub mod ab_glyph {
    pub use ab_glyph::*;
}
pub use self::{builtin::*, font::*, linebreak::*, section::*};

use ::ab_glyph::*;
use std::hash::Hash;

/// Logic to calculate glyph positioning using [`Font`](struct.Font.html),
/// [`SectionGeometry`](struct.SectionGeometry.html) and
/// [`SectionText`](struct.SectionText.html).
pub trait GlyphPositioner: Hash {
    /// Calculate a sequence of positioned glyphs to render. Custom implementations should
    /// return the same result when called with the same arguments to allow layout caching.
    fn calculate_glyphs<F, S>(
        &self,
        fonts: &[F],
        geometry: &SectionGeometry,
        sections: &[S],
    ) -> Vec<SectionGlyph>
    where
        F: Font,
        S: ToSectionText;

    /// Return a screen rectangle according to the requested render position and bounds
    /// appropriate for the glyph layout.
    fn bounds_rect(&self, geometry: &SectionGeometry) -> Rect;

    /// Recalculate a glyph sequence after a change.
    ///
    /// The default implementation simply calls `calculate_glyphs` so must be implemented
    /// to provide benefits as such benefits are specific to the internal layout logic.
    fn recalculate_glyphs<F, S, P>(
        &self,
        previous: P,
        change: GlyphChange,
        fonts: &[F],
        geometry: &SectionGeometry,
        sections: &[S],
    ) -> Vec<SectionGlyph>
    where
        F: Font,
        S: ToSectionText,
        P: IntoIterator<Item = SectionGlyph>,
    {
        let _ = (previous, change);
        self.calculate_glyphs(fonts, geometry, sections)
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub enum GlyphChange {
    /// Only the geometry has changed, contains the old geometry
    Geometry(SectionGeometry),
    Unknown,
}
