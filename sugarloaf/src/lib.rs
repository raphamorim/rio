pub mod components;
pub mod context;
pub mod font;
pub mod layout;
mod sugarloaf;

pub use swash::{Stretch, Style, Weight};

pub use crate::sugarloaf::{
    compositors::SugarCompositors,
    graphics::{ColorType, Graphic, GraphicData, GraphicId, SugarloafGraphics},
    primitives::*,
    Sugarloaf, SugarloafErrors, SugarloafRenderer, SugarloafWindow, SugarloafWindowSize,
    SugarloafWithErrors,
};
pub use components::rect::Rect;
pub use layout::{
    Content, ContentBuilder, FragmentStyle, FragmentStyleDecoration, UnderlineInfo,
    UnderlineShape,
};
