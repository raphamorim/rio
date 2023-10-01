// glyph module code along with comments was originally retired from glyph-brush
// https://github.com/alexheretic/glyph-brush
// glyph-brush was originally written Alex Butler (https://github.com/alexheretic)
// and licensed under Apache-2.0 license.

use ab_glyph::Font;

/// Id for a font.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct FontId(pub usize);

impl<F> std::ops::Index<FontId> for [F]
where
    F: Font,
{
    type Output = F;

    #[inline]
    fn index(&self, index: FontId) -> &Self::Output {
        self.index(index.0)
    }
}
impl<F> std::ops::Index<&FontId> for [F]
where
    F: Font,
{
    type Output = F;

    #[inline]
    fn index(&self, index: &FontId) -> &Self::Output {
        self.index(index.0)
    }
}
