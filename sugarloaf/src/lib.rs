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
    primitives::*,
    Colorspace, Sugarloaf, SugarloafErrors, SugarloafRenderer, SugarloafWindow,
    SugarloafWindowSize, SugarloafWithErrors,
};
pub use components::filters::Filter;
pub use components::quad::Quad;
pub use layout::{
    Content, FragmentStyle, FragmentStyleDecoration, SugarDimensions, UnderlineInfo,
    UnderlineShape,
};
