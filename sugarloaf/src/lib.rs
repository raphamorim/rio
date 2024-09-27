pub mod components;
pub mod context;
pub mod font;
pub mod font_introspector;
pub mod layout;
mod sugarloaf;

pub use font_introspector::{Stretch, Style, Weight};

pub use crate::sugarloaf::{
    compositors::SugarCompositors,
    graphics::{
        ColorType, Graphic, GraphicData, GraphicId, Graphics, ResizeCommand,
        ResizeParameter, MAX_GRAPHIC_DIMENSIONS,
    },
    primitives::*,
    Sugarloaf, SugarloafErrors, SugarloafRenderer, SugarloafWindow, SugarloafWindowSize,
    SugarloafWithErrors,
};
pub use components::rect::Rect;
pub use components::quad::{ComposedQuad, Quad};
pub use layout::{
    Content, FragmentStyle, FragmentStyleDecoration, UnderlineInfo, UnderlineShape,
};
