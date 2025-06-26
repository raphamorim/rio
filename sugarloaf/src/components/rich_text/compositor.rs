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

    pub fn begin(&mut self) {
        self.batches.reset();
    }

    pub fn finish(&mut self, vertices: &mut Vec<Vertex>) {
        self.batches.build_display_list(vertices);
    }

    /// Get current vertex count (for capturing vertices)
    pub fn vertex_count(&self) -> usize {
        let mut count = 0;
        self.batches.build_display_list(&mut Vec::new()); // This is inefficient, but works for now
        count
    }

    /// Capture vertices that were added since a given count
    pub fn capture_vertices_since(&self, since_count: usize, output: &mut Vec<Vertex>) {
        let mut all_vertices = Vec::new();
        self.batches.build_display_list(&mut all_vertices);
        if all_vertices.len() > since_count {
            output.extend_from_slice(&all_vertices[since_count..]);
        }
    }

    /// Draw a text run and optionally capture the generated vertices
    #[inline]
    pub fn draw_run_with_capture(
        &mut self,
        session: &mut GlyphCacheSession,
        rect: impl Into<Rect>,
        depth: f32,
        style: &TextRunStyle,
        glyphs: &[Glyph],
        capture_vertices: bool,
    ) -> Option<Vec<Vertex>> {
        let vertices_before = if capture_vertices {
            let mut temp_vertices = Vec::new();
            self.batches.build_display_list(&mut temp_vertices);
            Some(temp_vertices.len())
        } else {
            None
        };

        // Perform the actual rendering
        self.draw_run_internal(session, rect, depth, style, glyphs);

        // Capture vertices if requested
        if let Some(before_count) = vertices_before {
            let mut all_vertices = Vec::new();
            self.batches.build_display_list(&mut all_vertices);
            if all_vertices.len() > before_count {
                return Some(all_vertices[before_count..].to_vec());
            }
        }

        None
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
        let underline = match style.decoration {
            Some(FragmentStyleDecoration::Underline(info)) => Some(RunUnderline {
                enabled: true,
                offset: info.offset.round() as i32,
                size: info.size,
                color: style.decoration_color.unwrap_or(style.color),
                is_doubled: info.is_doubled,
                shape: info.shape,
            }),
            Some(FragmentStyleDecoration::Strikethrough) => Some(RunUnderline {
                enabled: true,
                offset: (style.line_height_without_mod / 3.5).round() as i32,
                size: 2.0,
                color: style.decoration_color.unwrap_or(style.color),
                is_doubled: false,
                shape: UnderlineShape::Regular,
            }),
            _ => None,
        };

        let subpx_bias = (0.125, 0.);
        let color = style.color;

        if let Some(builtin_character) = style.drawable_char {
            if let Some(bg_color) = style.background_color {
                let bg_rect =
                    Rect::new(rect.x, style.topline, rect.width, style.line_height);
                self.batches.add_rect(&bg_rect, depth, &bg_color);
            }

            if let Some(cursor) = style.cursor {
                match cursor {
                    crate::SugarCursor::Block(cursor_color) => {
                        let cursor_rect = Rect::new(
                            rect.x,
                            style.topline,
                            rect.width,
                            style.line_height,
                        );
                        self.batches.add_rect(&cursor_rect, depth, &cursor_color);
                    }
                    crate::SugarCursor::HollowBlock(cursor_color) => {
                        let outer_rect = Rect::new(
                            rect.x,
                            style.topline,
                            rect.width,
                            style.line_height,
                        );
                        self.batches.add_rect(&outer_rect, depth, &cursor_color);

                        if let Some(bg_color) = style.background_color {
                            let inner_rect = Rect::new(
                                rect.x + 1.0,
                                style.topline + 1.0,
                                rect.width - 2.0,
                                style.line_height - 2.0,
                            );
                            self.batches.add_rect(&inner_rect, depth, &bg_color);
                        }
                    }
                    crate::SugarCursor::Caret(cursor_color) => {
                        let outer_rect = Rect::new(
                            rect.x,
                            style.topline,
                            rect.width,
                            style.line_height,
                        );
                        self.batches.add_rect(&outer_rect, depth, &cursor_color);

                        if let Some(bg_color) = style.background_color {
                            let inner_rect = Rect::new(
                                rect.x + 1.0,
                                style.topline + 1.0,
                                rect.width - 2.0,
                                style.line_height - 2.0,
                            );
                            self.batches.add_rect(&inner_rect, depth, &bg_color);
                        }
                    }
                    crate::SugarCursor::Underline(cursor_color) => {
                        let caret_rect = Rect::new(
                            rect.x,
                            style.baseline + 1.0,
                            rect.width,
                            2.0,
                        );
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
                match cursor {
                    crate::SugarCursor::Block(cursor_color) => {
                        let cursor_rect = Rect::new(
                            rect.x,
                            style.topline,
                            rect.width,
                            style.line_height,
                        );
                        self.batches.add_rect(&cursor_rect, depth, &cursor_color);
                    }
                    crate::SugarCursor::HollowBlock(cursor_color) => {
                        let outer_rect = Rect::new(
                            rect.x,
                            style.topline,
                            rect.width,
                            style.line_height,
                        );
                        self.batches.add_rect(&outer_rect, depth, &cursor_color);

                        if let Some(bg_color) = style.background_color {
                            let inner_rect = Rect::new(
                                rect.x + 1.0,
                                style.topline + 1.0,
                                rect.width - 2.0,
                                style.line_height - 2.0,
                            );
                            self.batches.add_rect(&inner_rect, depth, &bg_color);
                        }
                    }
                    crate::SugarCursor::Caret(cursor_color) => {
                        let outer_rect = Rect::new(
                            rect.x,
                            style.topline,
                            rect.width,
                            style.line_height,
                        );
                        self.batches.add_rect(&outer_rect, depth, &cursor_color);

                        if let Some(bg_color) = style.background_color {
                            let inner_rect = Rect::new(
                                rect.x + 1.0,
                                style.topline + 1.0,
                                rect.width - 2.0,
                                style.line_height - 2.0,
                            );
                            self.batches.add_rect(&inner_rect, depth, &bg_color);
                        }
                    }
                    crate::SugarCursor::Underline(cursor_color) => {
                        let caret_rect = Rect::new(
                            rect.x,
                            style.baseline + 1.0,
                            rect.width,
                            2.0,
                        );
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