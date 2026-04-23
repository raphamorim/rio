pub mod components;
pub mod context;
pub mod font;
mod font_cache;
pub mod font_introspector;
pub mod grid;
pub mod layout;
pub mod renderer;
mod sugarloaf;
pub mod text;

// Expose WGPU
pub use wgpu;

pub use font_introspector::{Attributes, Stretch, Style, Weight};

pub use crate::font_cache::ResolvedGlyph;
pub use crate::sugarloaf::{
    graphics::{
        ColorType, Graphic, GraphicData, GraphicDataEntry, GraphicId, GraphicOverlay,
        Graphics, ResizeCommand, ResizeParameter, MAX_GRAPHIC_DIMENSIONS,
    },
    primitives::{
        contains_braille_dot, drawable_character, is_private_user_area, Corners,
        CursorKind, DrawableChar, ImageProperties, Object, Quad, Rect, RichText,
        RichTextLinesRange, RichTextRenderData, SugarCursor,
    },
    Colorspace, Sugarloaf, SugarloafBackend, SugarloafErrors, SugarloafRenderer,
    SugarloafWindow, SugarloafWindowSize, SugarloafWithErrors,
};
pub use components::filters::Filter;
pub use layout::{
    Content, RichTextConfig, SpanStyle, SpanStyleDecoration, TextDimensions,
    UnderlineInfo, UnderlineShape,
};
