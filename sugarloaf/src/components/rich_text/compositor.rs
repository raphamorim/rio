// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// compositor.rs was originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE
//
// Eventually the file had updates to support other features like background-color,
// text color, underline color and etc.

use crate::components::rich_text::batch::{BatchManager, RunUnderline};
pub use crate::components::rich_text::batch::{Rect, Vertex};
use crate::components::rich_text::image_cache::glyph::GlyphCacheSession;
use crate::components::rich_text::image_cache::ImageCache;
pub use crate::components::rich_text::image_cache::ImageId;
use crate::components::rich_text::text::*;
use crate::layout::{FragmentStyleDecoration, UnderlineShape};
use crate::sugarloaf::graphics::GraphicRenderRequest;
use crate::Graphics;
use crate::{DrawableChar, SugarCursor};
use halfbrown::HashMap;
use tracing::debug;

// First, let's define a structure to store the cached draw operations
pub struct LineCache {
    // Maps rich_text_id -> line_index -> cached batches
    caches: HashMap<usize, HashMap<usize, Vec<BatchOperation>>>,
}

// This will represent operations we need to cache
pub enum BatchOperation {
    Rect {
        rect: Rect,
        depth: f32,
        color: [f32; 4],
    },
    MaskRect {
        rect: Rect,
        depth: f32,
        color: [f32; 4],
        coords: [f32; 4],
        has_alpha: bool,
    },
    ImageRect {
        rect: Rect,
        depth: f32,
        color: [f32; 4],
        coords: [f32; 4],
        has_alpha: bool,
    },
    DrawableChar {
        x: f32,
        y: f32,
        width: f32,
        char_type: DrawableChar,
        color: [f32; 4],
        depth: f32,
        line_height: f32,
    },
    Underline {
        info: RunUnderline,
        x: f32,
        width: f32,
        baseline: f32,
        depth: f32,
        line_height: f32,
    },
    GraphicRequest(GraphicRenderRequest),
}

impl LineCache {
    pub fn new() -> Self {
        Self {
            caches: HashMap::new(),
        }
    }

    // Clear cache for a specific rich text and line
    #[inline]
    pub fn clear_cache(&mut self, rich_text_id: usize, line_number: &usize) {
        if let Some(text_cache) = self.caches.get_mut(&rich_text_id) {
            text_cache.remove(line_number);
        }
    }

    // Clear all caches for a specific rich text
    #[inline]
    pub fn clear_text_cache(&mut self, rich_text_id: usize) {
        self.caches.remove(&rich_text_id);
    }

    // Clear all caches
    #[inline]
    pub fn clear_all(&mut self) {
        self.caches.clear();
    }

    // Check if a cache entry exists
    #[inline]
    pub fn has_cache(&self, rich_text_id: usize, line_number: usize) -> bool {
        self.caches
            .get(&rich_text_id)
            .is_some_and(|text_cache| text_cache.contains_key(&line_number))
    }

    // Store operations in cache
    #[inline]
    pub fn store(
        &mut self,
        rich_text_id: usize,
        line_number: usize,
        operations: Vec<BatchOperation>,
    ) {
        self.caches
            .entry(rich_text_id)
            .or_insert_with(HashMap::new)
            .insert(line_number, operations);
    }

    // Apply cached operations to batches
    #[inline]
    pub fn apply_cache(
        &self,
        rich_text_id: usize,
        line_number: usize,
        comp: &mut Compositor,
        graphics: &mut Graphics,
    ) -> bool {
        if let Some(text_cache) = self.caches.get(&rich_text_id) {
            if let Some(operations) = text_cache.get(&line_number) {
                for op in operations {
                    match op {
                        BatchOperation::Rect { rect, depth, color } => {
                            comp.batches.add_rect(rect, *depth, color);
                        }
                        BatchOperation::MaskRect {
                            rect,
                            depth,
                            color,
                            coords,
                            has_alpha,
                        } => {
                            comp.batches
                                .add_mask_rect(rect, *depth, color, coords, *has_alpha);
                        }
                        BatchOperation::ImageRect {
                            rect,
                            depth,
                            color,
                            coords,
                            has_alpha,
                        } => {
                            comp.batches
                                .add_image_rect(rect, *depth, color, coords, *has_alpha);
                        }
                        BatchOperation::DrawableChar {
                            x,
                            y,
                            width,
                            char_type,
                            color,
                            depth,
                            line_height,
                        } => {
                            comp.batches.draw_drawable_character(
                                *x,
                                *y,
                                *width,
                                *char_type,
                                *color,
                                *depth,
                                *line_height,
                            );
                        }
                        BatchOperation::Underline {
                            info,
                            x,
                            width,
                            baseline,
                            depth,
                            line_height,
                        } => {
                            comp.batches.draw_underline(
                                info,
                                *x,
                                *width,
                                *baseline,
                                *depth,
                                *line_height,
                            );
                        }
                        BatchOperation::GraphicRequest(graphic_request) => {
                            if !graphics.top_layer.contains(graphic_request) {
                                graphics.top_layer.push(*graphic_request);
                            }
                        }
                    }
                }
                return true;
            }
        }

        // Log cache miss for debugging
        debug!(
            "LineCache miss for rich_text_id={} line_number={}",
            rich_text_id, line_number
        );
        false
    }
}

pub struct Compositor {
    batches: BatchManager,
}

impl Compositor {
    /// Creates a new compositor.
    pub fn new() -> Self {
        Self {
            batches: BatchManager::new(),
        }
    }

    /// Advances the epoch for the compositor and clears all batches.
    #[inline]
    pub fn begin(&mut self) {
        // TODO: Write a better prune system that doesn't rely on epoch
        // self.glyphs.prune(&mut self.images);
        self.batches.reset();
    }

    /// Builds a display list for the current batched geometry and enumerates
    /// all texture events with the specified closure.
    pub fn finish(&mut self, list: &mut Vec<Vertex>) {
        self.batches.build_display_list(list);
    }

    // Adds an image to the compositor.
    // pub fn add_image(
    //     &mut self,
    //     images: &mut ImageCache,
    //     graphic: &GraphicData,
    // ) -> Option<ImageId> {
    //     images.allocate(AddImage {
    //         width: graphic.width as u16,
    //         height: graphic.height as u16,
    //         has_alpha: graphic.is_opaque,
    //         data: ImageData::Borrowed(&graphic.pixels),
    //     })
    // }

    // Returns the image associated with the specified identifier.
    // #[allow(unused)]
    // pub fn get_image(
    //     &mut self,
    //     images: &mut ImageCache,
    //     image: ImageId,
    // ) -> Option<ImageLocation> {
    //     images.get(&image)
    // }

    // Removes the image from the compositor.
    // pub fn remove_image(&mut self, images: &mut ImageCache, image: ImageId) -> bool {
    // images.deallocate(image).is_some()
    // }

    /// Draws a rectangle with the specified depth and color.
    #[allow(unused)]
    pub fn draw_rect(&mut self, rect: impl Into<Rect>, depth: f32, color: &[f32; 4]) {
        self.batches.add_rect(&rect.into(), depth, color);
    }

    /// Draws an image with the specified rectangle, depth and color.
    #[allow(unused)]
    pub fn draw_image(
        &mut self,
        images: &ImageCache,
        rect: impl Into<Rect>,
        depth: f32,
        color: &[f32; 4],
        image: &ImageId,
    ) {
        if let Some(img) = images.get(image) {
            self.batches.add_image_rect(
                &rect.into(),
                depth,
                color,
                &[img.min.0, img.min.1, img.max.0, img.max.1],
                image.has_alpha(),
            );
        }
    }

    // Draws an image with the specified rectangle, depth and color.
    // #[inline]
    // pub fn draw_image_from_data(
    //     &mut self,
    //     rect: impl Into<Rect>,
    //     coords: &[f32; 4],
    //     has_alpha: bool,
    // ) {
    //     self.batches.add_image_rect(
    //         &rect.into(),
    //         0.0,
    //         &[0.0, 0.0, 0.0, 0.0],
    //         coords,
    //         has_alpha,
    //     );
    // }

    #[inline]
    pub fn draw_run(
        &mut self,
        session: &mut GlyphCacheSession,
        rect: impl Into<Rect>,
        depth: f32,
        style: &TextRunStyle,
        glyphs: &[Glyph],
        mut cache_operations: Option<&mut Vec<BatchOperation>>,
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
                if let Some(cache) = &mut cache_operations {
                    cache.push(BatchOperation::Rect {
                        rect: bg_rect,
                        depth,
                        color: bg_color,
                    });
                }
            }

            match style.cursor {
                Some(SugarCursor::Block(cursor_color)) => {
                    let cursor_rect = Rect::new(
                        rect.x,
                        style.topline + style.padding_y,
                        rect.width,
                        style.line_height_without_mod,
                    );

                    self.batches.add_rect(&cursor_rect, depth, &cursor_color);
                    if let Some(cache) = &mut cache_operations {
                        cache.push(BatchOperation::Rect {
                            rect: cursor_rect,
                            depth,
                            color: cursor_color,
                        });
                    }
                }
                Some(SugarCursor::HollowBlock(cursor_color)) => {
                    let outer_rect = Rect::new(
                        rect.x,
                        style.topline + style.padding_y,
                        rect.width,
                        style.line_height_without_mod,
                    );

                    self.batches.add_rect(&outer_rect, depth, &cursor_color);
                    if let Some(cache) = &mut cache_operations {
                        cache.push(BatchOperation::Rect {
                            rect: outer_rect,
                            depth,
                            color: cursor_color,
                        });
                    }

                    if let Some(bg_color) = style.background_color {
                        let inner_rect = Rect::new(
                            rect.x + 2.0,
                            style.topline + style.padding_y + 2.0,
                            rect.width - 4.0,
                            style.line_height_without_mod - 4.0,
                        );

                        self.batches.add_rect(&inner_rect, depth, &bg_color);
                        if let Some(cache) = &mut cache_operations {
                            cache.push(BatchOperation::Rect {
                                rect: inner_rect,
                                depth,
                                color: bg_color,
                            });
                        }
                    }
                }
                Some(SugarCursor::Caret(cursor_color)) => {
                    let caret_rect = Rect::new(
                        rect.x,
                        style.topline + style.padding_y,
                        3.0,
                        style.line_height_without_mod,
                    );

                    self.batches.add_rect(&caret_rect, depth, &cursor_color);
                    if let Some(cache) = &mut cache_operations {
                        cache.push(BatchOperation::Rect {
                            rect: caret_rect,
                            depth,
                            color: cursor_color,
                        });
                    }
                }
                _ => {}
            }

            // Handle underline
            if let Some(underline) = underline {
                self.batches.draw_underline(
                    &underline,
                    rect.x,
                    rect.width,
                    style.baseline,
                    depth,
                    style.line_height_without_mod,
                );
                if let Some(cache) = &mut cache_operations {
                    cache.push(BatchOperation::Underline {
                        info: underline,
                        x: rect.x,
                        width: rect.width,
                        baseline: style.baseline,
                        depth,
                        line_height: style.line_height_without_mod,
                    });
                }
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
            if let Some(cache) = &mut cache_operations {
                cache.push(BatchOperation::DrawableChar {
                    x: rect.x,
                    y: style.topline,
                    width: rect.width,
                    char_type: builtin_character,
                    color,
                    depth,
                    line_height: style.line_height,
                });
            }
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

                            if let Some(cache) = &mut cache_operations {
                                cache.push(BatchOperation::ImageRect {
                                    rect: glyph_rect,
                                    depth,
                                    color: bitmap_color,
                                    coords,
                                    has_alpha: entry.image.has_alpha(),
                                });
                            }
                        } else {
                            self.batches.add_mask_rect(
                                &glyph_rect,
                                depth,
                                &color,
                                &coords,
                                true,
                            );

                            if let Some(cache) = &mut cache_operations {
                                cache.push(BatchOperation::MaskRect {
                                    rect: glyph_rect,
                                    depth,
                                    color,
                                    coords,
                                    has_alpha: true,
                                });
                            }
                        }
                    }
                }
            }

            if let Some(bg_color) = style.background_color {
                let bg_rect =
                    Rect::new(rect.x, style.topline, rect.width, style.line_height);

                self.batches.add_rect(&bg_rect, depth, &bg_color);
                if let Some(cache) = &mut cache_operations {
                    cache.push(BatchOperation::Rect {
                        rect: bg_rect,
                        depth,
                        color: bg_color,
                    });
                }
            }

            // Handle cursor
            match style.cursor {
                Some(SugarCursor::Block(cursor_color)) => {
                    let cursor_rect = Rect::new(
                        rect.x,
                        style.topline + style.padding_y,
                        rect.width,
                        style.line_height_without_mod,
                    );

                    self.batches.add_rect(&cursor_rect, depth, &cursor_color);
                    if let Some(cache) = &mut cache_operations {
                        cache.push(BatchOperation::Rect {
                            rect: cursor_rect,
                            depth,
                            color: cursor_color,
                        });
                    }
                }
                Some(SugarCursor::HollowBlock(cursor_color)) => {
                    let outer_rect = Rect::new(
                        rect.x,
                        style.topline + style.padding_y,
                        rect.width,
                        style.line_height_without_mod,
                    );

                    self.batches.add_rect(&outer_rect, depth, &cursor_color);
                    if let Some(cache) = &mut cache_operations {
                        cache.push(BatchOperation::Rect {
                            rect: outer_rect,
                            depth,
                            color: cursor_color,
                        });
                    }

                    if let Some(bg_color) = style.background_color {
                        let inner_rect = Rect::new(
                            rect.x + 2.0,
                            style.topline + style.padding_y + 2.0,
                            rect.width - 4.0,
                            style.line_height_without_mod - 4.0,
                        );

                        self.batches.add_rect(&inner_rect, depth, &bg_color);
                        if let Some(cache) = &mut cache_operations {
                            cache.push(BatchOperation::Rect {
                                rect: inner_rect,
                                depth,
                                color: bg_color,
                            });
                        }
                    }
                }
                Some(SugarCursor::Caret(cursor_color)) => {
                    let caret_rect = Rect::new(
                        rect.x,
                        style.topline + style.padding_y,
                        3.0,
                        style.line_height_without_mod,
                    );

                    self.batches.add_rect(&caret_rect, depth, &cursor_color);
                    if let Some(cache) = &mut cache_operations {
                        cache.push(BatchOperation::Rect {
                            rect: caret_rect,
                            depth,
                            color: cursor_color,
                        });
                    }
                }
                _ => {}
            }

            // Handle underline
            if let Some(underline) = underline {
                self.batches.draw_underline(
                    &underline,
                    rect.x,
                    rect.width,
                    style.baseline,
                    depth,
                    style.line_height_without_mod,
                );
                if let Some(cache) = &mut cache_operations {
                    cache.push(BatchOperation::Underline {
                        info: underline,
                        x: rect.x,
                        width: rect.width,
                        baseline: style.baseline,
                        depth,
                        line_height: style.line_height_without_mod,
                    });
                }
            }
        }
    }

    // Draws a text run.
    // #[inline]
    // pub fn draw_run(
    //     &mut self,
    //     session: &mut GlyphCacheSession,
    //     rect: impl Into<Rect>,
    //     depth: f32,
    //     style: &TextRunStyle,
    //     glyphs: &[Glyph],
    //     // cached_run: &mut CachedRun,
    // ) {
    //     let rect = rect.into();

    //     let underline = match style.decoration {
    //         Some(FragmentStyleDecoration::Underline(info)) => Some(RunUnderline {
    //             enabled: true,
    //             offset: info.offset.round() as i32,
    //             size: info.size,
    //             color: style.decoration_color.unwrap_or(style.color),
    //             is_doubled: info.is_doubled,
    //             shape: info.shape,
    //         }),
    //         Some(FragmentStyleDecoration::Strikethrough) => Some(RunUnderline {
    //             enabled: true,
    //             offset: (style.line_height_without_mod / 3.5).round() as i32,
    //             size: 2.0,
    //             color: style.decoration_color.unwrap_or(style.color),
    //             is_doubled: false,
    //             shape: UnderlineShape::Regular,
    //         }),
    //         _ => None,
    //     };

    //     let subpx_bias = (0.125, 0.);
    //     let color = style.color;

    //     if let Some(builtin_character) = style.drawable_char {
    //         // x: f32,
    //         // y: f32,
    //         // char: DrawableChar,
    //         // color: [f32; 4],
    //         // depth: f32,
    //         // line_width: f32,
    //         // line_height: f32,
    //         if let Some(bg_color) = style.background_color {
    //             self.batches.add_rect(
    //                 &Rect::new(rect.x, style.topline, rect.width, style.line_height),
    //                 depth,
    //                 &bg_color,
    //             );
    //         }

    //         match style.cursor {
    //             Some(SugarCursor::Block(cursor_color)) => {
    //                 self.batches.add_rect(
    //                     &Rect::new(
    //                         rect.x,
    //                         style.topline + style.padding_y,
    //                         rect.width,
    //                         style.line_height_without_mod,
    //                     ),
    //                     depth,
    //                     &cursor_color,
    //                 );
    //             }
    //             Some(SugarCursor::HollowBlock(cursor_color)) => {
    //                 self.batches.add_rect(
    //                     &Rect::new(
    //                         rect.x,
    //                         style.topline + style.padding_y,
    //                         rect.width,
    //                         style.line_height_without_mod,
    //                     ),
    //                     depth,
    //                     &cursor_color,
    //                 );

    //                 if let Some(bg_color) = style.background_color {
    //                     self.batches.add_rect(
    //                         &Rect::new(
    //                             rect.x + 2.0,
    //                             style.topline + style.padding_y + 2.0,
    //                             rect.width - 4.0,
    //                             style.line_height_without_mod - 4.0,
    //                         ),
    //                         depth,
    //                         &bg_color,
    //                     );
    //                 }
    //             }
    //             Some(SugarCursor::Caret(cursor_color)) => {
    //                 self.batches.add_rect(
    //                     &Rect::new(
    //                         rect.x,
    //                         style.topline + style.padding_y,
    //                         3.0,
    //                         style.line_height_without_mod,
    //                     ),
    //                     depth,
    //                     &cursor_color,
    //                 );
    //             }
    //             _ => {}
    //         }

    //         if let Some(underline) = underline {
    //             self.draw_underline(
    //                 &underline,
    //                 rect.x,
    //                 rect.width,
    //                 style.baseline,
    //                 depth,
    //                 style.line_height_without_mod,
    //             );
    //         }

    //         self.batches.draw_drawable_character(
    //             rect.x,
    //             style.topline,
    //             rect.width,
    //             builtin_character,
    //             color,
    //             depth,
    //             style.line_height_without_mod,
    //         );
    //     } else {
    //         for glyph in glyphs {
    //             let entry = session.get(glyph.id);
    //             if let Some(entry) = entry {
    //                 if let Some(img) = session.get_image(entry.image) {
    //                     let gx = (glyph.x + subpx_bias.0).floor() + entry.left as f32;
    //                     let gy = (glyph.y + subpx_bias.1).floor() - entry.top as f32;

    //                     if entry.is_bitmap {
    //                         let color = [1.0, 1.0, 1.0, 1.0];
    //                         let coords = [img.min.0, img.min.1, img.max.0, img.max.1];
    //                         self.batches.add_image_rect(
    //                             &Rect::new(
    //                                 gx,
    //                                 gy,
    //                                 entry.width as f32,
    //                                 entry.height as f32,
    //                             ),
    //                             depth,
    //                             &color,
    //                             &coords,
    //                             entry.image.has_alpha(),
    //                         );
    //                     } else {
    //                         let coords = [img.min.0, img.min.1, img.max.0, img.max.1];
    //                         self.batches.add_mask_rect(
    //                             &Rect::new(
    //                                 gx,
    //                                 gy,
    //                                 entry.width as f32,
    //                                 entry.height as f32,
    //                             ),
    //                             depth,
    //                             &color,
    //                             &coords,
    //                             true,
    //                         );
    //                     }
    //                 }
    //             }
    //         }

    //         if let Some(bg_color) = style.background_color {
    //             self.batches.add_rect(
    //                 &Rect::new(rect.x, style.topline, rect.width, style.line_height),
    //                 depth,
    //                 &bg_color,
    //             );
    //         }

    //         match style.cursor {
    //             Some(SugarCursor::Block(cursor_color)) => {
    //                 self.batches.add_rect(
    //                     &Rect::new(
    //                         rect.x,
    //                         style.topline + style.padding_y,
    //                         rect.width,
    //                         style.line_height_without_mod,
    //                     ),
    //                     depth,
    //                     &cursor_color,
    //                 );
    //             }
    //             Some(SugarCursor::HollowBlock(cursor_color)) => {
    //                 self.batches.add_rect(
    //                     &Rect::new(
    //                         rect.x,
    //                         style.topline + style.padding_y,
    //                         rect.width,
    //                         style.line_height_without_mod,
    //                     ),
    //                     depth,
    //                     &cursor_color,
    //                 );

    //                 if let Some(bg_color) = style.background_color {
    //                     self.batches.add_rect(
    //                         &Rect::new(
    //                             rect.x + 2.0,
    //                             style.topline + style.padding_y + 2.0,
    //                             rect.width - 4.0,
    //                             style.line_height_without_mod - 4.0,
    //                         ),
    //                         depth,
    //                         &bg_color,
    //                     );
    //                 }
    //             }
    //             Some(SugarCursor::Caret(cursor_color)) => {
    //                 self.batches.add_rect(
    //                     &Rect::new(
    //                         rect.x,
    //                         style.topline + style.padding_y,
    //                         3.0,
    //                         style.line_height_without_mod,
    //                     ),
    //                     depth,
    //                     &cursor_color,
    //                 );
    //             }
    //             _ => {}
    //         }

    //         if let Some(underline) = underline {
    //             self.draw_underline(
    //                 &underline,
    //                 rect.x,
    //                 rect.width,
    //                 style.baseline,
    //                 depth,
    //                 style.line_height_without_mod,
    //             );
    //         }
    //     }

    // let duration = start.elapsed();
    // println!(" - draw_glyphs() is: {:?}", duration);
    // }
}
