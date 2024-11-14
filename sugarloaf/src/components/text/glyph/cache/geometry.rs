// glyph module code along with comments was originally retired from glyph-brush
// https://github.com/alexheretic/glyph-brush
// glyph-brush was originally written Alex Butler (https://github.com/alexheretic)
// and licensed under Apache-2.0 license.

use core::ops;

/// A rectangle, with top-left corner at min, and bottom-right corner at max.
/// Both field are in `[offset from left, offset from top]` format.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Rectangle<N> {
    /// Min `[x, y]`.
    pub min: [N; 2],
    /// Max `[x, y]`.
    pub max: [N; 2],
}

impl<N: ops::Sub<Output = N> + Copy> Rectangle<N> {
    #[inline]
    pub fn width(&self) -> N {
        self.max[0] - self.min[0]
    }

    #[inline]
    pub fn height(&self) -> N {
        self.max[1] - self.min[1]
    }
}
