// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Glyph Protocol sizing and placement (spec §8.5): the values and the
//! arithmetic that places an outline by them. Outside the `glyph`
//! feature, because the wire parser needs the values even in builds
//! without the registry or outline decoder.

/// `size` scale policy (spec §8.5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GlyphSize {
    Height,
    Advance,
    #[default]
    Contain,
    Cover,
    Stretch,
}

/// Horizontal half of `align` (spec §8.5.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GlyphAlignH {
    Start,
    #[default]
    Center,
    End,
}

/// Vertical half of `align` (spec §8.5.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GlyphAlignV {
    Start,
    #[default]
    Center,
    End,
    Baseline,
}

/// Explicit sizing and placement for a registration (spec §8.5).
/// Defaults are the protocol's own.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GlyphLayout {
    /// Authored advance width in upm units.
    pub aw: Option<u16>,
    /// Authored line height in upm units.
    pub lh: Option<u16>,
    /// Bottom of the authored extent in upm units. If any of `aw`, `lh`
    /// or `ymin` is set the extent is declared and the rest default to
    /// `upm`, `upm` and `0`; otherwise it is the outline's bounds.
    pub ymin: Option<i16>,
    pub size: GlyphSize,
    pub align_h: GlyphAlignH,
    pub align_v: GlyphAlignV,
    /// Insets as `[top, right, bottom, left]` fractions. Never
    /// degenerate: the parser normalises those to zero.
    pub pad: [f32; 4],
}

/// An outline placed by the §8.5 model: `pad`, then `size`, then
/// `align`, all on the authored extent. A design point `(x, y)` lands
/// on pixel `(tx + x * sx, ty - y * sy)` of a bitmap exactly the size of
/// the render span, which is what clips the glyph to its span (§8.5.7).
pub struct SpanPlacement {
    pub sx: f32,
    pub sy: f32,
    pub tx: f32,
    pub ty: f32,
}

impl SpanPlacement {
    /// `bounds` yields the outline's `[x_min, y_min, x_max, y_max]`; it
    /// is only asked for when the extent isn't declared.
    pub fn new(
        layout: &GlyphLayout,
        upm: u16,
        span_w_px: u16,
        cell_h_px: u16,
        baseline_px: f32,
        bounds: impl FnOnce() -> Option<[f32; 4]>,
    ) -> Option<Self> {
        // §8.5.1: the declared extent, or the outline's bounds without one.
        let declared =
            layout.aw.is_some() || layout.lh.is_some() || layout.ymin.is_some();
        let (x0, aw, y0, lh) = if declared {
            (
                0.0,
                layout.aw.unwrap_or(upm) as f32,
                layout.ymin.unwrap_or(0) as f32,
                layout.lh.unwrap_or(upm) as f32,
            )
        } else {
            let [x_min, y_min, x_max, y_max] = bounds()?;
            (x_min, x_max - x_min, y_min, y_max - y_min)
        };
        if upm == 0 {
            return None;
        }
        // Without a declared extent the em keeps a set in proportion.
        let em_cap = (!declared).then_some(upm as f32);
        Self::for_extent(
            layout,
            [x0, aw, y0, lh],
            em_cap,
            span_w_px,
            cell_h_px,
            baseline_px,
        )
    }

    /// Place a known extent `[x0, aw, y0, lh]` (Y-up design units).
    /// `em_cap`, when set, keeps `contain` from scaling past the em.
    pub fn for_extent(
        layout: &GlyphLayout,
        [x0, aw, y0, lh]: [f32; 4],
        em_cap: Option<f32>,
        span_w_px: u16,
        cell_h_px: u16,
        baseline_px: f32,
    ) -> Option<Self> {
        if aw <= 0.0 || lh <= 0.0 || span_w_px == 0 || cell_h_px == 0 {
            return None;
        }

        // §8.5.2: the effective span.
        let [pad_top, pad_right, pad_bottom, pad_left] = layout.pad;
        let (span_w, span_h) = (span_w_px as f32, cell_h_px as f32);
        let (inset_x, inset_y) = (span_w * pad_left, span_h * pad_top);
        let w = span_w * (1.0 - pad_left - pad_right);
        let h = span_h * (1.0 - pad_top - pad_bottom);

        // §8.5.3: scale factors.
        let (fit_w, fit_h) = (w / aw, h / lh);
        let contain = match em_cap {
            None => fit_w.min(fit_h),
            Some(em) => fit_w.min(fit_h).min(w.min(h) / em),
        };
        let (sx, sy) = match layout.size {
            GlyphSize::Height => (fit_h, fit_h),
            GlyphSize::Advance => (fit_w, fit_w),
            GlyphSize::Contain => (contain, contain),
            GlyphSize::Cover => (fit_w.max(fit_h), fit_w.max(fit_h)),
            GlyphSize::Stretch => (fit_w, fit_h),
        };

        // §8.5.4: where the scaled extent sits in the span.
        let tx = inset_x - x0 * sx
            + match layout.align_h {
                GlyphAlignH::Start => 0.0,
                GlyphAlignH::Center => (w - aw * sx) / 2.0,
                GlyphAlignH::End => w - aw * sx,
            };
        // `stretch` fills the span, so `align` has no effect on it.
        let bottom = inset_y + h + y0 * sy;
        let ty = match layout.align_v {
            _ if layout.size == GlyphSize::Stretch => bottom,
            GlyphAlignV::Start => bottom,
            GlyphAlignV::Center => inset_y + h / 2.0 + (y0 + lh / 2.0) * sy,
            GlyphAlignV::End => inset_y + (y0 + lh) * sy,
            GlyphAlignV::Baseline => baseline_px,
        };

        Some(Self { sx, sy, tx, ty })
    }
}
