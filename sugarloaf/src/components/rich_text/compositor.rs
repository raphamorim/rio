// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// Compositor with vertex capture for text run caching

use crate::components::rich_text::batch::{BatchManager, RunUnderline};
pub use crate::components::rich_text::batch::{Rect, Vertex};
use crate::components::rich_text::image_cache::glyph::GlyphCacheSession;
use crate::components::rich_text::text::*;
use crate::layout::{FragmentStyleDecoration, UnderlineShape};

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
        rect: &Rect,
    ) -> Option<RunUnderline> {
        match style.decoration {
            Some(FragmentStyleDecoration::Underline(info)) => {
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
            Some(FragmentStyleDecoration::Strikethrough) => {
                // Use font's built-in strikeout offset if available, otherwise use x-height/2
                // This matches standard typographic positioning for strikethrough
                let strikethrough_offset = if style.strikeout_offset != 0.0 {
                    // Font provides explicit strikeout position (negative because it's above baseline)
                    -style.strikeout_offset
                } else if style.x_height > 0.0 {
                    // Use half of x-height above baseline - this is the typographically correct position
                    -(style.x_height / 2.0)
                } else {
                    // Final fallback: approximate x-height position
                    -(rect.height * 0.3)
                };

                // Use font metrics for thickness when available
                let strikethrough_thickness = if style.underline_thickness > 0.0 {
                    style.underline_thickness
                } else {
                    1.5 // Slightly thinner than before for better appearance
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

    pub fn begin(&mut self) {
        self.batches.reset();
    }

    pub fn finish(&mut self, vertices: &mut Vec<Vertex>) {
        self.batches.build_display_list(vertices);
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
        _cache_operations: Option<&mut Vec<()>>, // Ignored - legacy parameter
    ) {
        self.draw_run_internal(session, rect, depth, style, glyphs);
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
    ) {
        let rect = rect.into();
        let underline = self.create_underline_from_decoration(style, &rect);

        let subpx_bias = (0.125, 0.);
        let color = style.color;

        if let Some(builtin_character) = style.drawable_char {
            if let Some(bg_color) = style.background_color {
                let bg_rect =
                    Rect::new(rect.x, style.topline, rect.width, style.line_height);
                self.batches.add_rect(&bg_rect, depth, &bg_color);
            }

            if let Some(cursor) = style.cursor {
                // Calculate cursor dimensions based on font metrics, not line height
                let font_height = style.ascent + style.descent;
                let cursor_top = style.baseline - style.ascent;

                match cursor {
                    crate::SugarCursor::Block(cursor_color) => {
                        let cursor_rect =
                            Rect::new(rect.x, cursor_top, rect.width, font_height);
                        self.batches.add_rect(&cursor_rect, depth, &cursor_color);
                    }
                    crate::SugarCursor::HollowBlock(cursor_color) => {
                        let outer_rect =
                            Rect::new(rect.x, cursor_top, rect.width, font_height);
                        self.batches.add_rect(&outer_rect, depth, &cursor_color);

                        if let Some(bg_color) = style.background_color {
                            let inner_rect = Rect::new(
                                rect.x + 1.0,
                                cursor_top + 1.0,
                                rect.width - 2.0,
                                font_height - 2.0,
                            );
                            self.batches.add_rect(&inner_rect, depth, &bg_color);
                        }
                    }
                    crate::SugarCursor::Caret(cursor_color) => {
                        let caret_rect = Rect::new(rect.x, cursor_top, 2.0, font_height);
                        self.batches.add_rect(&caret_rect, depth, &cursor_color);
                    }
                    crate::SugarCursor::Underline(cursor_color) => {
                        let caret_rect =
                            Rect::new(rect.x, style.baseline + 1.0, rect.width, 2.0);
                        self.batches.add_rect(&caret_rect, depth, &cursor_color);
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
            );
        } else {
            // Handle regular glyphs
            for glyph in glyphs {
                let entry = session.get(glyph.id);
                if let Some(entry) = entry {
                    if let Some(img) = session.get_image(entry.image) {
                        let gx = (glyph.x + subpx_bias.0).floor() + entry.left as f32;
                        let gy = (glyph.y + subpx_bias.1).floor() - entry.top as f32;
                        let glyph_rect =
                            Rect::new(gx, gy, entry.width as f32, entry.height as f32);
                        let coords = [img.min.0, img.min.1, img.max.0, img.max.1];

                        if entry.is_bitmap {
                            let bitmap_color = [1.0, 1.0, 1.0, 1.0];
                            self.batches.add_image_rect(
                                &glyph_rect,
                                depth,
                                &bitmap_color,
                                &coords,
                                entry.image.has_alpha(),
                            );
                        } else {
                            self.batches.add_mask_rect(
                                &glyph_rect,
                                depth,
                                &color,
                                &coords,
                                true,
                            );
                        }
                    }
                }
            }

            if let Some(bg_color) = style.background_color {
                let bg_rect =
                    Rect::new(rect.x, style.topline, rect.width, style.line_height);
                self.batches.add_rect(&bg_rect, depth, &bg_color);
            }

            if let Some(cursor) = style.cursor {
                // Calculate cursor dimensions based on font metrics, not line height
                let font_height = style.ascent + style.descent;
                let cursor_top = style.baseline - style.ascent;

                match cursor {
                    crate::SugarCursor::Block(cursor_color) => {
                        let cursor_rect =
                            Rect::new(rect.x, cursor_top, rect.width, font_height);
                        self.batches.add_rect(&cursor_rect, depth, &cursor_color);
                    }
                    crate::SugarCursor::HollowBlock(cursor_color) => {
                        let outer_rect =
                            Rect::new(rect.x, cursor_top, rect.width, font_height);
                        self.batches.add_rect(&outer_rect, depth, &cursor_color);

                        if let Some(bg_color) = style.background_color {
                            let inner_rect = Rect::new(
                                rect.x + 1.0,
                                cursor_top + 1.0,
                                rect.width - 2.0,
                                font_height - 2.0,
                            );
                            self.batches.add_rect(&inner_rect, depth, &bg_color);
                        }
                    }
                    crate::SugarCursor::Caret(cursor_color) => {
                        let caret_rect = Rect::new(rect.x, cursor_top, 2.0, font_height);
                        self.batches.add_rect(&caret_rect, depth, &cursor_color);
                    }
                    crate::SugarCursor::Underline(cursor_color) => {
                        let caret_rect =
                            Rect::new(rect.x, style.baseline + 1.0, rect.width, 2.0);
                        self.batches.add_rect(&caret_rect, depth, &cursor_color);
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
