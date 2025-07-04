pub mod components;
pub mod context;
pub mod font;
pub mod font_introspector;
pub mod layout;
mod sugarloaf;

// Expose WGPU
pub use wgpu;

pub use font_introspector::{Stretch, Style, Weight};

pub use crate::sugarloaf::{
    graphics::{
        ColorType, Graphic, GraphicData, GraphicId, Graphics, ResizeCommand,
        ResizeParameter, MAX_GRAPHIC_DIMENSIONS,
    },
    primitives::{RichText, RichTextLinesRange, SugarCursor, DrawableChar, contains_braille_dot, Object, QuadItem, ImageProperties, drawable_character},
    tree::{RenderTree, ObjectHandle, NodeId},
    Colorspace, Sugarloaf, SugarloafErrors, SugarloafRenderer, SugarloafWindow,
    SugarloafWindowSize, SugarloafWithErrors,
};
// For backward compatibility
pub use crate::sugarloaf::primitives::QuadItem as Quad;
pub use components::filters::Filter;
pub use components::rich_text::{BatchManager, Rect};
pub use layout::{
    Content, FragmentStyle, FragmentStyleDecoration, SugarDimensions, UnderlineInfo,
    UnderlineShape,
};
