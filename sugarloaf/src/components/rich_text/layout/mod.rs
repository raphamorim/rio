//! Experimental paragraph layout engine.

pub mod font;

mod bidi;
mod builder;
mod builder_data;
mod layout;
mod layout_data;
mod line_breaker;
mod nav;
mod span_style;

pub use swash;

pub use font::prelude::*;

#[doc(inline)]
pub use swash::text::Language;

/// Iterators over elements of a paragraph.
pub mod iter {
    pub use super::layout::{Clusters, Glyphs, Lines, Runs};
}

#[doc(inline)]
pub use font::{Font, FontLibrary, FontLibraryBuilder};
pub use builder::{ParagraphBuilder, LayoutContext};
pub use layout::{Cluster, Glyph, Line, Run};
pub use line_breaker::{Alignment, BreakLines};
pub use nav::{Selection, Erase, ExtendTo};
pub use span_style::*;

use layout_data::{LayoutData, LineLayoutData};

/// Collection of text, organized into lines, runs and clusters.
#[derive(Clone, Default)]
pub struct Paragraph {
    data: LayoutData,
    line_data: LineLayoutData,
}

/// Largest allowable span or fragment identifier.
const MAX_ID: u32 = i32::MAX as u32;

/// Index of a span in sequential order of submission to a paragraph builder.
#[derive(Copy, Clone, PartialOrd, Ord, PartialEq, Eq, Hash, Default, Debug)]
pub struct SpanId(pub u32);

impl SpanId {
    /// Converts the span identifier to an index.
    pub fn to_usize(self) -> usize {
        self.0 as usize
    }
}
