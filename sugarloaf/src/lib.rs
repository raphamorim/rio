pub mod components;
pub mod context;
pub mod font;
mod font_cache;
pub mod grid;
pub mod layout;
pub mod renderer;
mod sugarloaf;
pub mod text;

/// Single public entry point for the Glyph Protocol — registry types,
/// rasteriser, and atlas-key helpers. Wraps the relevant pieces from
/// `font::glyph_registry` and `renderer::image_cache::colr_raster` so
/// downstream crates have one stable place to import from.
pub mod glyph_protocol {
    pub use crate::font::glyph_registry::{
        is_pua, pack_atlas_glyph_id, GlyphRegistry, RegisterRejection, RegisteredGlyph,
        StoredPayload, CUSTOM_GLYPH_FONT_ID, CUSTOM_GLYPH_FONT_ID_U32, GLOSSARY_CAPACITY,
    };
    pub use crate::renderer::image_cache::colr_raster::{
        rasterize_payload, RasterizedPayload,
    };
}

// Re-export upstream swash so call sites can use `sugarloaf::swash::*`.
// This path was used by the in-tree fork; preserve it for stability.
pub use swash;

// Expose WGPU when the `wgpu` feature is enabled. Downstream code
// that needs `wgpu::Color` etc. picks it up via `sugarloaf::wgpu::…`.
#[cfg(feature = "wgpu")]
pub use wgpu;

pub use swash::{Attributes, Stretch, Style, Weight};

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
    Color, Colorspace, Sugarloaf, SugarloafBackend, SugarloafErrors, SugarloafRenderer,
    SugarloafWindow, SugarloafWindowSize, SugarloafWithErrors,
};
// `Filter` is the librashader CRT/scanline-filter wrapper — wgpu-only.
#[cfg(feature = "wgpu")]
pub use components::filters::Filter;
pub use layout::{
    Content, RichTextConfig, SpanStyle, SpanStyleDecoration, TextDimensions,
    UnderlineInfo, UnderlineShape,
};
