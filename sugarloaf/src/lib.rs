pub mod components;
pub mod context;
pub mod font;
pub mod font_introspector;
pub mod layout;
pub mod renderer;
mod sugarloaf;

// Expose WGPU
pub use wgpu;

pub use font_introspector::{Stretch, Style, Weight};

pub use crate::sugarloaf::{
    graphics::{
        ColorType, Graphic, GraphicData, GraphicId, Graphics, ResizeCommand,
        ResizeParameter, MAX_GRAPHIC_DIMENSIONS,
    },
    primitives::{
        contains_braille_dot, drawable_character, DrawableChar, ImageProperties, Object,
        Rect, RichText, RichTextLinesRange, RichTextRenderData, SugarCursor,
    },
    Colorspace, Sugarloaf, SugarloafBackend, SugarloafErrors, SugarloafRenderer,
    SugarloafWindow, SugarloafWindowSize, SugarloafWithErrors,
};
pub use components::filters::Filter;
pub use layout::{
    Content, SpanStyle, SpanStyleDecoration, RichTextConfig, TextDimensions,
    UnderlineInfo, UnderlineShape,
};
