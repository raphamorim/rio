// Sizing and placement model for Glyph Protocol registrations
// (specs/glyph-protocol.md §8.5, introduced in spec v1.7).
//
// Every registered outline passes through three transforms at render
// time, in order: pad (compute the effective render span), size (pick
// scale factors), align (position the scaled outline within the span).
// This module holds both the parameter types the wire parser produces
// and the pure math that turns them into an affine design-units →
// cell-pixels transform. It performs no I/O and touches no GPU state,
// so the whole §8.5 table is unit-testable with plain numbers.

/// Scale policy (§8.5.3). Given the authored extent `aw × lh` and the
/// effective (padded) span `W' × H'`:
///
/// | mode      | sx              | sy              | aspect    |
/// |-----------|-----------------|-----------------|-----------|
/// | `height`  | `H' / lh`       | `H' / lh`       | preserved |
/// | `advance` | `W' / aw`       | `W' / aw`       | preserved |
/// | `contain` | `min(W'/aw, H'/lh)` | same        | preserved |
/// | `cover`   | `max(W'/aw, H'/lh)` | same        | preserved |
/// | `stretch` | `W' / aw`       | `H' / lh`       | free      |
///
/// `height` is the default because it matches how characters behave:
/// the cell's vertical pixels drive, the horizontal footprint is
/// whatever the glyph's own advance dictates (and may overflow).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SizeMode {
    #[default]
    Height,
    Advance,
    Contain,
    Cover,
    Stretch,
}

/// Horizontal alignment of the scaled extent within the span (§8.5.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HAlign {
    Start,
    #[default]
    Center,
    End,
}

/// Vertical alignment of the scaled extent within the span (§8.5.4).
/// `Baseline` aligns the outline's `y=0` with the terminal's text
/// baseline — preferred for character-like glyphs; descenders extend
/// below it naturally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VAlign {
    Start,
    #[default]
    Center,
    End,
    Baseline,
}

/// Fixed-point denominator for [`PlacementParams::pad`]. Pads ride the
/// wire as decimal fractions (`0.0`–`1.0`); storing them ×10000 keeps
/// the structs `Eq`/hashable (no floats) at 1e-4 precision — far below
/// a pixel at any realistic cell size.
pub const PAD_DENOM: f32 = 10_000.0;

/// Sizing/placement parameters carried by an `r` registration (§6.1).
/// All fields have spec-defined defaults, so a pre-v1.7 client that
/// sends only `upm` gets `with_upm(upm)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacementParams {
    /// Authored advance width in upm units — the intended horizontal
    /// extent, NOT the outline's bounding box. Defaults to `upm`.
    pub aw: u16,
    /// Authored line height in upm units (descender-to-ascender), NOT
    /// the outline's bounding box. Defaults to `upm`.
    pub lh: u16,
    /// The codepoint's terminal width in cells (`1` or `2`), in the
    /// wcwidth sense. Authoritative for layout: cursor advance,
    /// wrapping, selection geometry.
    pub width: u8,
    pub size: SizeMode,
    pub align: (HAlign, VAlign),
    /// Insets from the render span edges as fractions ×[`PAD_DENOM`],
    /// in `top, right, bottom, left` order. Top/bottom are fractions
    /// of cell height, left/right of render span width.
    pub pad: [u16; 4],
}

impl PlacementParams {
    /// Spec defaults for a registration authored at `upm` units per em:
    /// `aw=lh=upm`, narrow width, `size=height`, `align=center,center`,
    /// no padding.
    pub fn with_upm(upm: u16) -> Self {
        Self {
            aw: upm,
            lh: upm,
            width: 1,
            size: SizeMode::Height,
            align: (HAlign::Center, VAlign::Center),
            pad: [0; 4],
        }
    }
}

/// Cell geometry the placement is computed against, all in physical
/// pixels of the target grid.
#[derive(Debug, Clone, Copy)]
pub struct CellGeometry {
    pub cell_width: f32,
    pub cell_height: f32,
    /// Baseline position measured from the cell top (the primary
    /// font's ascent at the current size). Only consumed by
    /// `align=<h>,baseline`.
    pub ascent: f32,
    /// Horizontal cells actually available at render time (1 or 2).
    /// Grid truth: when a cell was written before its registration
    /// declared `width=2`, the cell is narrow and the span stays 1 so
    /// the glyph never overdraws an occupied neighbour.
    pub span_cells: u8,
}

/// Affine mapping from design units (Y-up, `y=0` at baseline, §8.5.1)
/// to cell-local pixels (Y-down, origin at the cell's top-left corner):
///
/// ```text
/// px_x = tx + sx * x
/// px_y = ty - sy * y
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedTransform {
    pub sx: f32,
    pub sy: f32,
    pub tx: f32,
    pub ty: f32,
}

impl PlacedTransform {
    #[inline]
    pub fn map_x(&self, x: f32) -> f32 {
        self.tx + self.sx * x
    }
    #[inline]
    pub fn map_y(&self, y: f32) -> f32 {
        self.ty - self.sy * y
    }
}

/// Compute the §8.5 transform: pad → size → align.
///
/// `outline_y_min` is the payload's design-space bbox minimum Y, used
/// to anchor the authored extent vertically. The spec pins the extent
/// as `[0, aw] × [y_min, y_max]` with `lh = y_max − y_min` but ships
/// only the difference, so the implementation uses
/// `y_min = min(0, outline bbox y_min)`: icons authored entirely above
/// the baseline get the extent `[0, lh]` sitting on it, and glyphs
/// with descender geometry extend below it naturally.
pub fn place(
    params: &PlacementParams,
    geom: &CellGeometry,
    outline_y_min: f32,
) -> PlacedTransform {
    let span_cells = params.width.min(geom.span_cells).max(1) as f32;
    let w = span_cells * geom.cell_width;
    let h = geom.cell_height;

    // §8.5.2 — pad shrinks the span. A degenerate request (an axis
    // fully padded away) is treated as no padding at all; the parser
    // applies the same rule, this is defence for direct API callers.
    let [t, r, b, l] = params.pad.map(|p| p as f32 / PAD_DENOM);
    let (t, b) = if t + b >= 1.0 { (0.0, 0.0) } else { (t, b) };
    let (l, r) = if l + r >= 1.0 { (0.0, 0.0) } else { (l, r) };

    // Span box in cell-local top-down pixels. Top/bottom pads are
    // fractions of cell height, left/right of render span width.
    let mut left = l * w;
    let mut right = w - r * w;
    let mut top = t * h;
    let mut bottom = h - b * h;

    // Stretched glyphs are usually meant to tile across cell
    // boundaries (box-drawing-style); snap the span to whole pixels so
    // adjacent stretch glyphs meet without seams from fractional
    // edges. Other modes keep fractional spans — they preserve exact
    // centering and glyphs there don't tile.
    if params.size == SizeMode::Stretch {
        left = left.round();
        right = right.round();
        top = top.round();
        bottom = bottom.round();
    }

    let eff_w = right - left;
    let eff_h = bottom - top;

    // §8.5.3 — scale factors from the authored extent. Zero extents
    // are rejected at parse time; clamp anyway so a direct caller
    // can't produce an infinite scale.
    let aw = f32::from(params.aw.max(1));
    let lh = f32::from(params.lh.max(1));
    let (sx, sy) = match params.size {
        SizeMode::Height => {
            let s = eff_h / lh;
            (s, s)
        }
        SizeMode::Advance => {
            let s = eff_w / aw;
            (s, s)
        }
        SizeMode::Contain => {
            let s = (eff_w / aw).min(eff_h / lh);
            (s, s)
        }
        SizeMode::Cover => {
            let s = (eff_w / aw).max(eff_h / lh);
            (s, s)
        }
        SizeMode::Stretch => (eff_w / aw, eff_h / lh),
    };

    // §8.5.1 — anchor the extent's vertical range.
    let y_min_ext = outline_y_min.min(0.0);
    let y_max_ext = y_min_ext + lh;

    // §8.5.4 — alignment. Horizontal maps design x ∈ [0, aw]; vertical
    // maps design y ∈ [y_min_ext, y_max_ext] with the Y flip baked
    // into `map_y`.
    let tx = match params.align.0 {
        HAlign::Start => left,
        HAlign::Center => (left + right) * 0.5 - sx * aw * 0.5,
        HAlign::End => right - sx * aw,
    };
    let ty = match params.align.1 {
        // Outline's y_min lands on the span's bottom edge.
        VAlign::Start => bottom + sy * y_min_ext,
        VAlign::Center => (top + bottom) * 0.5 + sy * (y_min_ext + y_max_ext) * 0.5,
        // Outline's y_max lands on the span's top edge.
        VAlign::End => top + sy * y_max_ext,
        // Outline's y=0 lands on the terminal text baseline.
        VAlign::Baseline => geom.ascent,
    };

    PlacedTransform { sx, sy, tx, ty }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-4;

    fn assert_close(a: f32, b: f32, what: &str) {
        assert!((a - b).abs() < EPS, "{what}: {a} != {b}");
    }

    /// 10×20 px cell, baseline 16 px from the top, narrow span.
    fn geom() -> CellGeometry {
        CellGeometry {
            cell_width: 10.0,
            cell_height: 20.0,
            ascent: 16.0,
            span_cells: 1,
        }
    }

    fn params() -> PlacementParams {
        PlacementParams::with_upm(1000)
    }

    #[test]
    fn defaults_match_spec() {
        let p = PlacementParams::with_upm(2048);
        assert_eq!(p.aw, 2048);
        assert_eq!(p.lh, 2048);
        assert_eq!(p.width, 1);
        assert_eq!(p.size, SizeMode::Height);
        assert_eq!(p.align, (HAlign::Center, VAlign::Center));
        assert_eq!(p.pad, [0; 4]);
    }

    #[test]
    fn height_mode_maps_lh_to_cell_height() {
        let t = place(&params(), &geom(), 0.0);
        assert_close(t.sy, 20.0 / 1000.0, "sy");
        assert_close(t.sx, t.sy, "aspect preserved");
        // Extent [0, 1000] fills the cell vertically when centered.
        assert_close(t.map_y(1000.0), 0.0, "extent top at cell top");
        assert_close(t.map_y(0.0), 20.0, "extent bottom at cell bottom");
        // Horizontal extent scales to 20 px in a 10 px cell — centered
        // overflow of 5 px per side (height mode may overflow).
        assert_close(t.map_x(0.0), -5.0, "left overflow");
        assert_close(t.map_x(1000.0), 15.0, "right overflow");
    }

    #[test]
    fn advance_mode_maps_aw_to_span_width() {
        let mut p = params();
        p.size = SizeMode::Advance;
        let t = place(&p, &geom(), 0.0);
        assert_close(t.sx, 10.0 / 1000.0, "sx");
        assert_close(t.map_x(0.0), 0.0, "extent left");
        assert_close(t.map_x(1000.0), 10.0, "extent right");
        // Vertically centered: extent scales to 10 px in a 20 px cell.
        assert_close(t.map_y(1000.0), 5.0, "extent top");
        assert_close(t.map_y(0.0), 15.0, "extent bottom");
    }

    #[test]
    fn contain_fits_inside_span() {
        let mut p = params();
        p.size = SizeMode::Contain;
        let t = place(&p, &geom(), 0.0);
        // min(10/1000, 20/1000)
        assert_close(t.sx, 0.01, "sx");
        assert_close(t.sy, 0.01, "sy");
        // Fits: no point of the extent leaves the cell box.
        assert!(t.map_x(0.0) >= 0.0 && t.map_x(1000.0) <= 10.0);
        assert!(t.map_y(1000.0) >= 0.0 && t.map_y(0.0) <= 20.0);
    }

    #[test]
    fn cover_fills_span() {
        let mut p = params();
        p.size = SizeMode::Cover;
        let t = place(&p, &geom(), 0.0);
        // max(10/1000, 20/1000)
        assert_close(t.sx, 0.02, "sx");
        assert_close(t.sy, 0.02, "sy");
    }

    #[test]
    fn stretch_fills_both_axes_regardless_of_align() {
        for h in [HAlign::Start, HAlign::Center, HAlign::End] {
            let mut p = params();
            p.size = SizeMode::Stretch;
            p.align = (h, VAlign::Start);
            let t = place(&p, &geom(), 0.0);
            assert_close(t.map_x(0.0), 0.0, "left edge");
            assert_close(t.map_x(1000.0), 10.0, "right edge");
            assert_close(t.map_y(0.0), 20.0, "bottom edge");
            assert_close(t.map_y(1000.0), 0.0, "top edge");
        }
    }

    #[test]
    fn stretch_snaps_span_to_pixel_edges() {
        let mut p = params();
        p.size = SizeMode::Stretch;
        // 12.5% pads on a 10×20 cell give fractional edges (1.25 /
        // 2.5); stretch snaps them so adjacent glyphs tile seamlessly.
        p.pad = [1250, 1250, 1250, 1250];
        let g = geom();
        let t = place(&p, &g, 0.0);
        let left = t.map_x(0.0);
        let right = t.map_x(1000.0);
        let top = t.map_y(1000.0);
        let bottom = t.map_y(0.0);
        for (v, what) in [
            (left, "left"),
            (right, "right"),
            (top, "top"),
            (bottom, "bottom"),
        ] {
            assert_close(v, v.round(), what);
        }
    }

    #[test]
    fn align_start_and_end() {
        let mut p = params();
        p.size = SizeMode::Contain;
        p.align = (HAlign::Start, VAlign::Start);
        let t = place(&p, &geom(), 0.0);
        assert_close(t.map_x(0.0), 0.0, "h start");
        assert_close(t.map_y(0.0), 20.0, "v start: y_min on span bottom");

        p.align = (HAlign::End, VAlign::End);
        let t = place(&p, &geom(), 0.0);
        assert_close(t.map_x(1000.0), 10.0, "h end");
        assert_close(t.map_y(1000.0), 0.0, "v end: y_max on span top");
    }

    #[test]
    fn baseline_align_puts_design_zero_on_ascent() {
        let mut p = params();
        p.align = (HAlign::Center, VAlign::Baseline);
        let t = place(&p, &geom(), 0.0);
        assert_close(t.map_y(0.0), 16.0, "baseline at ascent");
        // A descender at -200 design units extends below the baseline.
        assert!(t.map_y(-200.0) > 16.0);
    }

    #[test]
    fn descender_outline_anchors_extent_below_baseline() {
        // Outline reaching y=-200: extent becomes [-200, 800].
        let mut p = params();
        p.size = SizeMode::Contain;
        p.align = (HAlign::Start, VAlign::Start);
        let t = place(&p, &geom(), -200.0);
        // v=start: the *extent's* y_min (the descender depth) sits on
        // the span bottom, so the descender tip touches the bottom.
        assert_close(t.map_y(-200.0), 20.0, "descender tip at bottom");
        assert_close(t.map_y(800.0), 10.0, "extent top");
    }

    #[test]
    fn pad_shrinks_span() {
        let mut p = params();
        p.size = SizeMode::Stretch;
        // top 10%, right 20%, bottom 10%, left 20% — of cell height /
        // span width respectively.
        p.pad = [1000, 2000, 1000, 2000];
        let t = place(&p, &geom(), 0.0);
        assert_close(t.map_x(0.0), 2.0, "left inset");
        assert_close(t.map_x(1000.0), 8.0, "right inset");
        assert_close(t.map_y(1000.0), 2.0, "top inset");
        assert_close(t.map_y(0.0), 18.0, "bottom inset");
    }

    #[test]
    fn degenerate_pad_treated_as_zero() {
        let mut p = params();
        p.size = SizeMode::Stretch;
        // l + r = 1.0 — §8.5.2 says treat as pad=0 on that axis pair.
        p.pad = [0, 5000, 0, 5000];
        let t = place(&p, &geom(), 0.0);
        assert_close(t.map_x(0.0), 0.0, "left");
        assert_close(t.map_x(1000.0), 10.0, "right");
    }

    #[test]
    fn wide_span_doubles_width() {
        let mut p = params();
        p.width = 2;
        p.size = SizeMode::Stretch;
        let g = CellGeometry {
            span_cells: 2,
            ..geom()
        };
        let t = place(&p, &g, 0.0);
        assert_close(t.map_x(1000.0), 20.0, "span covers two cells");
    }

    #[test]
    fn render_span_clamped_to_grid_truth() {
        // Registration says wide, but the cell was written while
        // narrow: the available span wins so the glyph never paints
        // over an occupied neighbour.
        let mut p = params();
        p.width = 2;
        p.size = SizeMode::Stretch;
        let t = place(&p, &geom(), 0.0); // geom().span_cells == 1
        assert_close(t.map_x(1000.0), 10.0, "clamped to one cell");
    }

    #[test]
    fn aspect_ratio_preserved_for_uniform_modes() {
        for mode in [
            SizeMode::Height,
            SizeMode::Advance,
            SizeMode::Contain,
            SizeMode::Cover,
        ] {
            let mut p = params();
            p.size = mode;
            // Non-square extent to make sx≠sy detectable.
            p.aw = 500;
            let t = place(&p, &geom(), 0.0);
            assert_close(t.sx, t.sy, "uniform scale");
        }
    }
}
