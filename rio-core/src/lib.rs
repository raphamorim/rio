//! `rio-core` — the renderer-agnostic value types shared across the
//! [`canario`] terminal engine and its embedders.
//!
//! This is the leaf crate of the canario workspace and the *decoupling
//! landing zone*: pure data types that the engine needs but that today
//! live, coupled, inside Rio's `sugarloaf` (graphics payloads) and
//! `rio-backend::config` (the color model). None of these types carry any
//! GPU, font, window, or serde-config logic.
//!
//! Relocating them here (and re-exporting from their old homes) is what
//! lets the terminal engine stop depending on `sugarloaf` and `config`.
//! See `canario/DESIGN.md` §5 (Severances 1 & 3) and `canario/ROADMAP.md`
//! Phase 0.
//!
//! [`canario`]: https://github.com/raphamorim/rio

pub mod color;
pub mod geom;
pub mod graphics;

pub use color::{AnsiColor, ColorArray, ColorRgb, NamedColor, DIM_FACTOR};
pub use geom::{Column, Line, Pos, Side};
pub use graphics::{
    ColorType, GraphicData, GraphicId, ResizeCommand, ResizeParameter,
    MAX_GRAPHIC_DIMENSIONS,
};
