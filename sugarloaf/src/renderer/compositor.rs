// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// Compositor with vertex capture for text run caching

use crate::layout::{SpanStyleDecoration, UnderlineShape};
use crate::renderer::batch::{BatchManager, DrawCmd, QuadInstance, RunUnderline};
pub use crate::renderer::batch::{Rect, Vertex};
use crate::renderer::image_cache::glyph::GlyphCacheSession;
use crate::renderer::text::*;

pub struct Compositor {
    pub batches: BatchManager,
}

impl Compositor {
    pub fn new() -> Self {
        Self {
            batches: BatchManager::new(),
        }
    }

    /// Creates an underline decoration based on the style and rect
    pub fn create_underline_from_decoration(
        &self,
        style: &TextRunStyle,
    ) -> Option<RunUnderline> {
        match style.decoration {
            Some(SpanStyleDecoration::Underline(info)) => {
                // Use font metrics for thickness when available, otherwise fall back to shape-based defaults
                let underline_thickness = if style.underline_thickness > 0.0 {
                    style.underline_thickness
                } else {
                    // Fallback thickness based on font size
                    // Use approximately 8% of font size for all underline types
                    let base_thickness = style.font_size * 0.08;
                    base_thickness.max(1.0)
                };

                // Use real font metrics for proper underline positioning
                let underline_offset =
                    self.calculate_underline_offset(style, underline_thickness);

                Some(RunUnderline {
                    enabled: true,
                    offset: underline_offset,
                    size: underline_thickness,
                    color: style.decoration_color.unwrap_or(style.color),
                    is_doubled: info.is_doubled,
                    shape: info.shape,
                })
            }
            Some(SpanStyleDecoration::Strikethrough) => {
                // Strikethrough should be positioned through the middle of the text
                // Use font-provided strikeout_offset if available, otherwise x_height/2
                let strikethrough_offset = if style.strikeout_offset != 0.0 {
                    // Font provides strikeout_offset as distance from baseline
                    -style.strikeout_offset
                } else if style.x_height > 0.0 {
                    // x_height is the height of lowercase letters, strike through middle
                    -(style.x_height / 2.0)
                } else {
                    // Fallback: 25% of ascent above baseline
                    -(style.ascent * 0.25)
                };

                // Use font metrics for thickness when available
                let strikethrough_thickness = if style.underline_thickness > 0.0 {
                    style.underline_thickness
                } else {
                    1.5
                };

                Some(RunUnderline {
                    enabled: true,
                    offset: strikethrough_offset,
                    size: strikethrough_thickness,
                    color: style.decoration_color.unwrap_or(style.color),
                    is_doubled: false,
                    shape: UnderlineShape::Regular,
                })
            }
            _ => None,
        }
    }

    /// Calculate underline offset using font metrics, with fallback
    fn calculate_underline_offset(
        &self,
        style: &TextRunStyle,
        underline_thickness: f32,
    ) -> f32 {
        // Use font's built-in underline position when available
        if style.underline_offset != 0.0 {
            // Font provides underline_offset as distance from baseline to underline top
            // Negative values mean below baseline, which is what we want
            // But Rio's renderer expects positive offset for below baseline
            -style.underline_offset
        } else {
            // Fallback: place underline 1 thickness below baseline
            underline_thickness
        }
    }

    #[inline]
    pub fn finish(
        &mut self,
        instances: &mut Vec<QuadInstance>,
        vertices: &mut Vec<Vertex>,
        cmds: &mut Vec<DrawCmd>,
    ) {
        self.batches.build_display_list(instances, vertices, cmds);
        self.batches.reset();
    }

    /// Standard draw_run method (for compatibility)
    #[inline]
    pub fn draw_run(
        &mut self,
        session: &mut GlyphCacheSession,
        rect: impl Into<Rect>,
        depth: f32,
        style: &TextRunStyle,
        glyphs: &[Glyph],
        order: u8,
    ) {
        self.draw_run_internal(session, rect, depth, style, glyphs, order);
    }

    /// Internal rendering implementation
    #[inline]
    fn draw_run_internal(
        &mut self,
        session: &mut GlyphCacheSession,
        rect: impl Into<Rect>,
        depth: f32,
        style: &TextRunStyle,
        glyphs: &[Glyph],
        order: u8,
    ) {
        let rect = rect.into();
        let underline = self.create_underline_from_decoration(style);

        let subpx_bias = (0.125, 0.);
        let color = style.color;

        if let Some(builtin_character) = style.drawable_char {
            if let Some(bg_color) = style.background_color {
                let bg_rect =
                    Rect::new(rect.x, style.topline, rect.width, style.line_height);
                self.batches.rect(&bg_rect, depth, &bg_color, 0);
            }

            if let Some(cursor) = style.cursor {
                // Calculate cursor dimensions based on font metrics, not line height
                let font_height = style.ascent + style.descent;
                let cursor_top = style.baseline - style.ascent;

                match cursor.kind {
                    crate::CursorKind::Block => {
                        let cursor_rect =
                            Rect::new(rect.x, cursor_top, rect.width, font_height);
                        self.batches.rect(
                            &cursor_rect,
                            depth,
                            &cursor.color,
                            cursor.order,
                        );
                    }
                    crate::CursorKind::HollowBlock => {
                        let outer_rect =
                            Rect::new(rect.x, cursor_top, rect.width, font_height);
                        self.batches.rect(
                            &outer_rect,
                            depth,
                            &cursor.color,
                            cursor.order,
                        );

                        if let Some(bg_color) = style.background_color {
                            let inner_rect = Rect::new(
                                rect.x + 1.0,
                                cursor_top + 1.0,
                                rect.width - 2.0,
                                font_height - 2.0,
                            );
                            self.batches.rect(
                                &inner_rect,
                                depth,
                                &bg_color,
                                cursor.order,
                            );
                        }
                    }
                    crate::CursorKind::Caret => {
                        let caret_rect = Rect::new(rect.x, cursor_top, 2.0, font_height);
                        self.batches.rect(
                            &caret_rect,
                            depth,
                            &cursor.color,
                            cursor.order,
                        );
                    }
                    crate::CursorKind::Underline => {
                        let caret_rect =
                            Rect::new(rect.x, style.baseline + 1.0, rect.width, 2.0);
                        self.batches.rect(
                            &caret_rect,
                            depth,
                            &cursor.color,
                            cursor.order,
                        );
                    }
                }
            }

            if let Some(underline) = underline {
                self.batches.draw_underline(
                    &underline,
                    rect.x,
                    rect.width,
                    style.baseline,
                    depth,
                    style.line_height,
                );
            }

            self.batches.draw_drawable_character(
                rect.x,
                style.topline,
                rect.width,
                builtin_character,
                color,
                depth,
                style.line_height,
                0,
            );
        } else {
            // Handle regular glyphs
            for glyph in glyphs {
                // Rasterize Nerd Font / PUA glyphs once at the nominal font
                // size. An earlier "rasterize at cells × font_size" trick
                // produced a 2× raster that the constraint math below then
                // tried to re-scale *from*, compounding into a ~2× oversized
                // glyph. With nominal rasterization, the constraint's
                // width/height factors land where they should.
                let entry = session.get(glyph.id);
                if let Some(entry) = entry {
                    if let Some(img) = session.get_image(entry.image) {
                        let gx = (glyph.x + subpx_bias.0).floor() + entry.left as f32;
                        let gy = (glyph.y + subpx_bias.1).floor() - entry.top as f32;

                        // Proportional fit in both directions. After the
                        // targeted rasterization above this is effectively a
                        // downscale-or-identity, which keeps output crisp.
                        // Matches ghostty's `.size = .fit` for PUA symbols.
                        let glyph_rect = if let Some((cell_w, cells)) =
                            style.scale_constraint
                        {
                            // If this codepoint has a per-glyph entry
                            // in the Nerd Fonts patcher table (ported
                            // from ghostty), defer to ghostty's
                            // `Constraint::constrain` for size and
                            // alignment. Otherwise fall back to Rio's
                            // generic cell-centered fit.
                            if let Some(constraint) = style.nerd_font_constraint {
                                compute_nerd_font_rect(NerdFontRectInput {
                                    constraint: &constraint,
                                    entry: &entry,
                                    glyph,
                                    cell_w,
                                    cells,
                                    line_height: style.line_height,
                                    topline: style.topline,
                                    baseline: style.baseline,
                                })
                            } else if style.is_custom_glyph_run {
                                // Glyph Protocol glyphs live at PUA
                                // codepoints with no Nerd Font patcher
                                // entry, so they'd otherwise take the
                                // "natural position" fallback and sit
                                // uncentered in their cell slot. Use the
                                // generic cell-centered fit that the
                                // pre-main compositor path gave to all
                                // unknown PUA glyphs.
                                let target_w = cell_w * cells as f32;
                                let target_h = style.line_height;
                                let orig_w = entry.width as f32;
                                let orig_h = entry.height as f32;
                                let scale =
                                    (target_w / orig_w).min(target_h / orig_h);
                                let sw = orig_w * scale;
                                let sh = orig_h * scale;
                                let cx = glyph.x + (target_w - sw) / 2.0;
                                let cy = style.topline
                                    + (style.line_height - sh) / 2.0;
                                Rect::new(cx, cy, sw, sh)
                            } else {
                                // No per-codepoint attribute: no scaling, no
                                // slot-centering. Glyph renders at its
                                // natural pen position and natural raster
                                // size.
                                let _ = (cell_w, cells);
                                Rect::new(gx, gy, entry.width as f32, entry.height as f32)
                            }
                        } else if entry.is_bitmap {
                            // Color bitmap (emoji) glyphs fall here when the
                            // shaper didn't attach an explicit constraint.
                            //
                            // `.cover` sizing with center alignment and
                            // 2.5 % horizontal padding. Cover scales the
                            // bitmap so it fills the advance × cell-height
                            // slot on at least one axis, rather than fit
                            // which leaves gaps.
                            //
                            // Vertical centering uses the font's *natural*
                            // cell (ascent + descent) rather than
                            // `line_height` — the latter picks up user
                            // line-height modifiers that shouldn't shift the
                            // emoji inside its cell.
                            const PAD_EACH: f32 = 0.025;
                            let orig_w = entry.width as f32;
                            let orig_h = entry.height as f32;
                            if orig_w > 0.0 && orig_h > 0.0 {
                                let cell_top = style.baseline - style.ascent;
                                let cell_h = style.ascent + style.descent;
                                let available_w = glyph.advance * (1.0 - 2.0 * PAD_EACH);
                                // Cover: pick the larger scale factor so the
                                // emoji fills the slot on at least one axis.
                                let scale = (available_w / orig_w).max(cell_h / orig_h);
                                let sw = orig_w * scale;
                                let sh = orig_h * scale;
                                let cx = (glyph.x + subpx_bias.0).floor()
                                    + (glyph.advance - sw) / 2.0;
                                let cy = cell_top + (cell_h - sh) / 2.0;
                                // Snap both edges to the pixel grid. Bitmap
                                // emoji (sbix — Apple Color Emoji) sampled
                                // at fractional offsets looks blurry;
                                // rounding cx/cy/sw/sh to whole pixels lets
                                // the sampler hit source texels cleanly.
                                // No-op for COLR glyphs whose scale already
                                // snapped.
                                let x0 = cx.round();
                                let x1 = (cx + sw).round();
                                let y0 = cy.round();
                                let y1 = (cy + sh).round();
                                Rect::new(x0, y0, x1 - x0, y1 - y0)
                            } else {
                                Rect::new(gx, gy, orig_w, orig_h)
                            }
                        } else {
                            Rect::new(gx, gy, entry.width as f32, entry.height as f32)
                        };
                        let coords = [img.min.0, img.min.1, img.max.0, img.max.1];

                        if entry.is_bitmap {
                            let bitmap_color = [1.0, 1.0, 1.0, 1.0];
                            // Get atlas index for this image (0-based), add 1 for layer (0 = no texture)
                            let atlas_layer = session
                                .get_atlas_index(entry.image)
                                .map(|idx| (idx + 1) as i32)
                                .unwrap_or(1);
                            self.batches.add_image_rect(
                                &glyph_rect,
                                depth,
                                &bitmap_color,
                                &coords,
                                atlas_layer,
                            );
                        } else {
                            self.batches.add_mask_rect_with_order(
                                &glyph_rect,
                                depth,
                                &color,
                                &coords,
                                true,
                                order,
                            );
                        }
                    }
                }
            }

            if let Some(bg_color) = style.background_color {
                let bg_rect =
                    Rect::new(rect.x, style.topline, rect.width, style.line_height);
                self.batches.rect(&bg_rect, depth, &bg_color, 0);
            }

            if let Some(cursor) = style.cursor {
                // Calculate cursor dimensions based on font metrics, not line height
                let font_height = style.ascent + style.descent;
                let cursor_top = style.baseline - style.ascent;

                match cursor.kind {
                    crate::CursorKind::Block => {
                        let cursor_rect =
                            Rect::new(rect.x, cursor_top, rect.width, font_height);
                        self.batches.rect(
                            &cursor_rect,
                            depth,
                            &cursor.color,
                            cursor.order,
                        );
                    }
                    crate::CursorKind::HollowBlock => {
                        let outer_rect =
                            Rect::new(rect.x, cursor_top, rect.width, font_height);
                        self.batches.rect(
                            &outer_rect,
                            depth,
                            &cursor.color,
                            cursor.order,
                        );

                        if let Some(bg_color) = style.background_color {
                            let inner_rect = Rect::new(
                                rect.x + 1.0,
                                cursor_top + 1.0,
                                rect.width - 2.0,
                                font_height - 2.0,
                            );
                            self.batches.rect(
                                &inner_rect,
                                depth,
                                &bg_color,
                                cursor.order,
                            );
                        }
                    }
                    crate::CursorKind::Caret => {
                        let caret_rect = Rect::new(rect.x, cursor_top, 2.0, font_height);
                        self.batches.rect(
                            &caret_rect,
                            depth,
                            &cursor.color,
                            cursor.order,
                        );
                    }
                    crate::CursorKind::Underline => {
                        let caret_rect =
                            Rect::new(rect.x, style.baseline + 1.0, rect.width, 2.0);
                        self.batches.rect(
                            &caret_rect,
                            depth,
                            &cursor.color,
                            cursor.order,
                        );
                    }
                }
            }

            if let Some(underline) = underline {
                self.batches.draw_underline(
                    &underline,
                    rect.x,
                    rect.width,
                    style.baseline,
                    depth,
                    style.line_height,
                );
            }
        }
    }
}

impl Default for Compositor {
    fn default() -> Self {
        Self::new()
    }
}

/// Inputs for `compute_nerd_font_rect`, bundled so the function signature
/// stays within the clippy arg-count lint while keeping call sites
/// readable.
struct NerdFontRectInput<'a> {
    constraint: &'a crate::font::nerd_font_attributes::Constraint,
    entry: &'a crate::renderer::image_cache::glyph::GlyphEntry,
    glyph: &'a Glyph,
    cell_w: f32,
    cells: u8,
    line_height: f32,
    topline: f32,
    baseline: f32,
}

/// Apply ghostty's per-glyph `Constraint` to a rasterized glyph entry and
/// return the target draw rectangle in screen space.
///
/// Ghostty's constraint model operates on a `GlyphSize { x, y, width,
/// height }` in cell-relative y-up coordinates:
/// `x` is the left bearing from cell left, `y` is the *bottom* of the
/// glyph bounding box measured from the cell *bottom* (y-up), and the
/// text baseline sits `cell_baseline` above the cell bottom. After
/// `constrain()` returns, ghostty's shader flips `y` via
/// `offset_y_final = cell_height − offset_y` to land in y-down screen
/// space. We do the same translation here so glyph drawing stays in Rio's
/// y-down frame.
///
/// Input mapping (swash → ghostty cell-bottom y-up):
/// `bitmap bottom = cell_baseline + entry.top − entry.height`,
/// `bitmap left = entry.left`.
///
/// Output mapping (ghostty y-up → screen y-down):
/// `screen top = topline + (cell_height − out.y − out.height)`,
/// `screen left = glyph.x + out.x`.
fn compute_nerd_font_rect(input: NerdFontRectInput<'_>) -> Rect {
    let NerdFontRectInput {
        constraint,
        entry,
        glyph,
        cell_w,
        cells,
        line_height,
        topline,
        baseline,
    } = input;
    use crate::font::nerd_font_attributes::{GlyphSize, Metrics};

    // Ghostty's `cell_baseline`: distance from cell bottom *up to* baseline.
    // Rio keeps `baseline` in y-down screen coords (same frame as `topline`),
    // so we flip via `(topline + line_height) − baseline`.
    let cell_baseline = ((topline + line_height) - baseline) as f64;
    let face_width = cell_w as f64;
    let face_height = line_height as f64;
    let metrics = Metrics {
        face_width,
        face_height,
        // `face_y`: offset from cell bottom up to the bottom of the face box.
        // Rio doesn't separately track a face box, so the face sits at the
        // cell bottom — good enough for the aligned-y math.
        face_y: 0.0,
        cell_width: cell_w.max(1.0) as u32,
        cell_height: line_height.max(1.0) as u32,
        // `Height::Icon` entries scale against the icon height from grid
        // metrics; ghostty exposes this via `adjust-icon-height`. Until Rio
        // has the knob, use 75% of face_height (ghostty's single-cell
        // default post-rounding).
        icon_height_single: 0.75 * face_height,
        icon_height: 0.75 * face_height,
    };

    // Swash's (`entry.left`, baseline-up `entry.top`, width, height) →
    // ghostty's cell-bottom y-up bounding box.
    let glyph_size = GlyphSize {
        width: entry.width as f64,
        height: entry.height as f64,
        x: entry.left as f64,
        y: cell_baseline + entry.top as f64 - entry.height as f64,
    };

    let out = constraint.constrain(glyph_size, metrics, cells.clamp(1, 2));

    // y-up cell-bottom → y-down screen top: `cell_height − (y + height)`.
    let cx = glyph.x + out.x as f32;
    let cy = topline + (line_height - (out.y as f32 + out.height as f32));
    Rect::new(cx, cy, out.width as f32, out.height as f32)
}
