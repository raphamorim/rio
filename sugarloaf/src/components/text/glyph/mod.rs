// glyph module code along with comments was originally retired from glyph-brush
// https://github.com/alexheretic/glyph-brush
// glyph-brush was originally written Alex Butler (https://github.com/alexheretic)
// and licensed under Apache-2.0 license.

mod brush;
mod cache;
mod calculator;
mod extra;
mod layout;
mod section;

pub use brush::*;
pub use cache::Rectangle;
pub use calculator::*;
pub use extra::*;
pub use layout::*;
pub use section::*;

use layout::ab_glyph::*;

/// A "practically collision free" `Section` hasher
#[cfg(not(target_arch = "wasm32"))]
pub type DefaultSectionHasher = twox_hash::RandomXxHashBuilder;
// Work around for rand issues in wasm #61
#[cfg(target_arch = "wasm32")]
pub type DefaultSectionHasher = std::hash::BuildHasherDefault<twox_hash::XxHash>;

#[test]
fn default_section_hasher() {
    use std::hash::BuildHasher;

    let section_a = Section::default().add_text(Text::new("Hovered Tile: Some((0, 0))"));
    let section_b = Section::default().add_text(Text::new("Hovered Tile: Some((1, 0))"));
    let hash = |s: &Section| DefaultSectionHasher::default().hash_one(s);
    assert_ne!(hash(&section_a), hash(&section_b));
}
