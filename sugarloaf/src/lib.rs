pub mod components;
pub mod context;
pub mod font;
pub mod font_introspector;
pub mod layout;
mod sugarloaf;

pub use font_introspector::{Stretch, Style, Weight};

pub use crate::sugarloaf::{
    compositors::SugarCompositors,
    graphics::{MAX_GRAPHIC_DIMENSIONS, ColorType, Graphic, GraphicData, GraphicId, Graphics, ResizeParameter, ResizeCommand},
    primitives::*,
    Sugarloaf, SugarloafErrors, SugarloafRenderer, SugarloafWindow, SugarloafWindowSize,
    SugarloafWithErrors,
};
pub use components::rect::Rect;
pub use layout::{
    Content, ContentBuilder, FragmentStyle, FragmentStyleDecoration, UnderlineInfo,
    UnderlineShape,
};
