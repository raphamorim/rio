// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// Modules bidi, builder, builder_data, layout, layout_data, line_breaker
// nav and span_style were originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE

pub mod span;

pub use span::{
    FontSettingCache, FontSettingKey, SpanStyle, SpanStyleDecoration, UnderlineInfo,
    UnderlineShape, EMPTY_FONT_SETTINGS,
};

/// Index of a span in sequential order of submission to a paragraph content.
#[derive(Copy, Clone, PartialOrd, Ord, PartialEq, Eq, Hash, Default, Debug)]
pub struct SpanId(pub usize);

impl SpanId {
    /// Converts the span identifier to an index.
    pub fn to_usize(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Delta<T: Default> {
    pub x: T,
    pub top_y: T,
    pub bottom_y: T,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TextDimensions {
    pub width: f32,
    pub height: f32,
    pub scale: f32,
}

impl Default for TextDimensions {
    fn default() -> Self {
        Self {
            width: 8.0,   // Reasonable character cell width fallback
            height: 16.0, // Reasonable character cell height fallback
            scale: 1.0,
        }
    }
}

/// Canonical cell metrics in physical pixels. Rounded `u32` cell
/// width / height / baseline are the single source of truth for the
/// GPU grid uniform, the col/row count math, and mouse hit testing.
/// The unrounded `f64` `face_width / face_height` are retained for
/// downstream subpixel math (image positioning, baseline-relative
/// offsets).
///
/// Important invariants:
/// - `cell_width = round(face_width)` (half-away-from-zero)
/// - `cell_height = round(face_height)` (half-away-from-zero) —
///   `face_height` already has the user's `line_height` multiplier
///   baked in; consumers MUST NOT re-apply it.
/// - `cell_baseline` is pixels from the **bottom** of the cell to
///   the text baseline. Centered in the rounded cell so the glyph
///   doesn't drift up/down by half a pixel after rounding.
/// - `face_y = cell_baseline - face_baseline` — offset between the
///   actual (rounded) baseline and the unrounded face baseline.
///   Used by underline / strikethrough position conversion when
///   moving from baseline-relative to cell-top-relative coords.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct CellMetrics {
    pub cell_width: u32,
    pub cell_height: u32,
    pub cell_baseline: u32,
    pub face_width: f64,
    pub face_height: f64,
    pub face_y: f64,
}

impl Default for CellMetrics {
    fn default() -> Self {
        Self {
            cell_width: 8,
            cell_height: 16,
            cell_baseline: 4,
            face_width: 8.0,
            face_height: 16.0,
            face_y: 0.0,
        }
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub struct TextLayout {
    pub line_height: f32,
    pub font_size: f32,
    pub original_font_size: f32,
    pub dimensions: TextDimensions,
    /// Canonical cell metrics. Single source of truth for the
    /// integer cell stride and baseline. Consumers should prefer
    /// this over `dimensions` for any cell-coordinate math (renderer
    /// grid uniform, layout col/row count, mouse hit testing) —
    /// `dimensions` is kept for legacy callers that read the raw f32
    /// width/height.
    pub cell: CellMetrics,
}

impl TextLayout {
    #[inline]
    pub fn rescale(&mut self, scale_factor: f32) -> &mut Self {
        self.dimensions.width *= scale_factor;
        self.dimensions.height *= scale_factor;
        self.dimensions.scale = scale_factor;
        self
    }

    pub fn from_default_layout(default_layout: &RootStyle) -> Self {
        Self {
            line_height: default_layout.line_height,
            font_size: default_layout.font_size,
            original_font_size: default_layout.font_size,
            dimensions: TextDimensions {
                scale: default_layout.scale_factor,
                ..TextDimensions::default()
            },
            cell: CellMetrics::default(),
        }
    }
}

impl Default for TextLayout {
    fn default() -> Self {
        Self {
            line_height: 1.0,
            font_size: 0.0,
            original_font_size: 0.0,
            dimensions: TextDimensions::default(),
            cell: CellMetrics::default(),
        }
    }
}

/// Configuration for creating rich text with custom properties
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct RichTextConfig {
    /// Initial position [x, y] where the rich text should be rendered
    pub position: Option<[f32; 2]>,
    /// Depth value for z-ordering (more negative = closer to camera/in front)
    pub depth: f32,
}

impl Default for RichTextConfig {
    fn default() -> Self {
        Self {
            position: None,
            depth: 0.0,
        }
    }
}

impl RichTextConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_position(mut self, x: f32, y: f32) -> Self {
        self.position = Some([x, y]);
        self
    }

    pub fn with_depth(mut self, depth: f32) -> Self {
        self.depth = depth;
        self
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub struct RootStyle {
    pub scale_factor: f32,
    pub font_size: f32,
    pub line_height: f32,
}

impl Default for RootStyle {
    fn default() -> Self {
        Self {
            line_height: 1.0,
            scale_factor: 1.0,
            font_size: 14.,
        }
    }
}

impl RootStyle {
    pub fn new(scale_factor: f32, font_size: f32, line_height: f32) -> RootStyle {
        // Line height cannot be under 1
        let line_height = if line_height <= 1.0 { 1.0 } else { line_height };

        RootStyle {
            scale_factor,
            font_size,
            line_height,
        }
    }
}

/// Pure helper: compute canonical [`CellMetrics`] from already-scaled
/// face dimensions. Used by [`compute_cell_metrics`] and is the same
/// formula the previous `Content::set_text` ran inline.
///
/// Centering invariant: if `face_height` is `33.4` and rounds to
/// `33`, the baseline shifts up by `0.2` so the glyph stays centered
/// in the rounded cell — matches the half-rounding-delta adjustment
/// used elsewhere for vertical pixel-snapping.
#[inline]
pub fn canonical_cell_metrics(
    face_width: f64,
    face_height: f64,
    descent_phys: f64,
    leading_phys: f64,
) -> CellMetrics {
    let cell_width = face_width.round().max(1.0) as u32;
    let cell_height = face_height.round().max(1.0) as u32;
    let face_baseline = leading_phys * 0.5 + descent_phys;
    let baseline_centered = face_baseline - (cell_height as f64 - face_height) * 0.5;
    let cell_baseline = baseline_centered.round().max(0.0) as u32;
    let face_y = cell_baseline as f64 - face_baseline;
    CellMetrics {
        cell_width,
        cell_height,
        cell_baseline,
        face_width,
        face_height,
        face_y,
    }
}

/// Compute the canonical [`CellMetrics`] for a `(font_size,
/// line_height, scale_factor)` triple using `font_library`'s primary
/// font. Pure function — no per-id state, no caching, no
/// side effects. Callers (rioterm's `ContextDimension`, future panel
/// owners) recompute on font / size / scale change and store the
/// result themselves.
///
/// `line_height` is the user's config multiplier; it's applied to
/// `face_height` here, so callers MUST NOT re-apply it.
///
/// Mirrors `Content::calculate_character_cell_dimensions`'s formula
/// exactly so dimensions stay byte-identical across the migration.
/// Once `Content` is deleted that helper goes with it.
#[inline]
pub fn compute_cell_metrics(
    font_library: &crate::font::FontLibrary,
    font_size: f32,
    line_height: f32,
    scale_factor: f32,
) -> (TextDimensions, CellMetrics) {
    let scale_f64 = scale_factor as f64;
    let line_height_mod = line_height as f64;

    let raw: Option<(f64, f64, f64, f64)> = {
        #[cfg(target_os = "macos")]
        {
            font_library.ct_font(0).map(|handle| {
                let m = crate::font::macos::font_metrics(&handle, font_size);
                let cw = crate::font::macos::max_ascii_advance_px(&handle, font_size)
                    .or_else(|| {
                        crate::font::macos::advance_units_for_char(&handle, ' ')
                            .map(|(units, upem)| units * font_size / upem as f32)
                    })
                    .unwrap_or(font_size);
                (
                    cw as f64,
                    m.ascent as f64,
                    m.descent as f64,
                    m.leading as f64,
                )
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            font_library.inner.try_read().and_then(|lib| {
                let id = 0;
                let (data, offset, _key) = lib.get_data(&id)?;
                let font_ref = swash::FontRef::from_index(&data, offset as usize)?;
                let m = font_ref.metrics(&[]);
                let upem = m.units_per_em as f32;
                let s = font_size / upem;
                let glyph = font_ref.charmap().map(' ' as u32);
                let advance = font_ref.glyph_metrics(&[]).advance_width(glyph);
                let cw = if advance > 0.0 {
                    advance * s
                } else {
                    font_size
                };
                Some((
                    cw as f64,
                    (m.ascent * s) as f64,
                    (m.descent.abs() * s) as f64,
                    (m.leading * s) as f64,
                ))
            })
        }
    };

    let (face_width, face_height, descent_phys, leading_phys) =
        if let Some((cw, ascent, descent, leading)) = raw {
            let face_width = cw * scale_f64;
            let face_height = (ascent + descent + leading) * line_height_mod * scale_f64;
            (
                face_width,
                face_height,
                descent * line_height_mod * scale_f64,
                leading * line_height_mod * scale_f64,
            )
        } else {
            let fw = font_size as f64 * scale_f64;
            let fh = font_size as f64 * line_height_mod * scale_f64;
            (fw, fh, 0.0, 0.0)
        };

    let cell =
        canonical_cell_metrics(face_width, face_height, descent_phys, leading_phys);
    let dims = TextDimensions {
        width: cell.cell_width as f32,
        height: cell.cell_height as f32,
        scale: scale_factor,
    };
    (dims, cell)
}
