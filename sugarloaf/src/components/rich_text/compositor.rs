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

use rustc_hash::FxHashMap;
use crate::layout::SugarDimensions;
use crate::layout::iter::Glyphs;
use crate::components::rich_text::batch::BatchManager;
pub use crate::components::rich_text::batch::{
    // Command, DisplayList, Pipeline, Rect, Vertex,
    Command,
    DisplayList,
    Rect,
    Vertex,
};
use crate::components::rich_text::image_cache::glyph::GlyphCacheSession;
use crate::components::rich_text::image_cache::ImageCache;
pub use crate::components::rich_text::image_cache::{
    AddImage,
    ImageId,
    ImageLocation,
    TextureEvent,
    TextureId,
    // AddImage, Epoch, ImageData, ImageId, ImageLocation, TextureEvent, TextureId,
};
use crate::components::rich_text::text::*;
use crate::SugarCursor;
use std::borrow::Borrow;

pub struct ComposedRect {
    rect: Rect,
    coords: [f32; 4],
    color: [f32; 4],
    has_alpha: bool,
    image: TextureId,
}

pub enum CachedRect {
    Image(ComposedRect),
    Mask(ComposedRect),
    Standard((Rect, [f32; 4])),
}

pub struct CachedRunGlyph {
    pub id: u16,
    pub x: f32,
    pub y: f32,
}

pub struct CachedRun {
    pub glyphs: Vec<CachedRunGlyph>,
    pub rects_per_glyph: FxHashMap<usize, Vec<CachedRect>>,
    char_width: f32
}

impl CachedRun {
    pub fn new(char_width: f32) -> Self {
        Self {
            char_width,
            glyphs: Vec::new(),
            rects_per_glyph: FxHashMap::default(),
        }
    }
}

pub struct Compositor {
    batches: BatchManager,
    intercepts: Vec<(f32, f32)>,
}

impl Compositor {
    /// Creates a new compositor.
    pub fn new() -> Self {
        Self {
            batches: BatchManager::new(),
            intercepts: Vec::new(),
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
    pub fn finish(
        &mut self,
        list: &mut DisplayList,
        images: &mut ImageCache,
        events: impl FnMut(TextureEvent),
    ) {
        images.drain_events(events);
        self.batches.build_display_list(list);
    }
}

/// Image management.
impl Compositor {
    /// Adds an image to the compositor.
    #[allow(unused)]
    pub fn add_image(
        &mut self,
        images: &mut ImageCache,
        request: AddImage,
    ) -> Option<ImageId> {
        images.allocate(request)
    }

    /// Returns the image associated with the specified identifier.
    #[allow(unused)]
    pub fn get_image(
        &mut self,
        images: &mut ImageCache,
        image: ImageId,
    ) -> Option<ImageLocation> {
        images.get(image)
    }

    /// Removes the image from the compositor.
    #[allow(unused)]
    pub fn remove_image(&mut self, images: &mut ImageCache, image: ImageId) -> bool {
        images.deallocate(image).is_some()
    }
}

/// Drawing.
impl Compositor {
    /// Draws a rectangle with the specified depth and color.
    #[allow(unused)]
    pub fn draw_rect(&mut self, rect: impl Into<Rect>, depth: f32, color: &[f32; 4]) {
        self.batches.add_rect(&rect.into(), depth, color);
    }

    /// Draws an image with the specified rectangle, depth and color.
    #[allow(unused)]
    pub fn draw_image(
        &mut self,
        images: &mut ImageCache,
        rect: impl Into<Rect>,
        depth: f32,
        color: &[f32; 4],
        image: ImageId,
    ) {
        if let Some(img) = images.get(image) {
            self.batches.add_image_rect(
                &rect.into(),
                depth,
                color,
                &[img.min.0, img.min.1, img.max.0, img.max.1],
                img.texture_id,
                image.has_alpha(),
            );
        }
    }

    #[inline]
    pub fn draw_glyphs_from_cache(&mut self, cache_line: &Vec<CachedRun>, px: f32, py: f32, depth: f32, rect: SugarDimensions) {
        let mut glyphs = Vec::new();
        let mut px = px;
        let subpx_bias = (0.125, 0.);

        for cached_run in cache_line {
            for glyph in &cached_run.glyphs {
                let x = px + glyph.x;
                let y = py - glyph.y;
                // px += glyph.advance;
                px += rect.width * cached_run.char_width;
                glyphs.push(Glyph { id: glyph.id, x, y });
            }
            
            let mut index = 0;
            for glyph in &glyphs {
                if let Some(rects) = cached_run.rects_per_glyph.get(&index) {
                    for rect in rects {
                        match rect {
                            CachedRect::Image(data) => {
                                let gx = (glyph.x + subpx_bias.0).floor() + data.rect.x;
                                let gy = (glyph.y + subpx_bias.1).floor() - data.rect.y;
                                let rect = Rect::new(gx, gy, data.rect.width, data.rect.height);

                                self.batches.add_image_rect(
                                    &rect,
                                    depth,
                                    &data.color,
                                    &data.coords,
                                    data.image,
                                    data.has_alpha,
                                );
                            }
                            CachedRect::Mask(data) => {
                                let gx = (glyph.x + subpx_bias.0).floor() + data.rect.x;
                                let gy = (glyph.y + subpx_bias.1).floor() - data.rect.y;
                                println!("cached {:?} {:?}", gx, gy);
                                let rect = Rect::new(gx, gy, data.rect.width, data.rect.height);

                                self.batches.add_mask_rect(
                                    &rect,
                                    depth,
                                    &data.color,
                                    &data.coords,
                                    data.image,
                                    data.has_alpha,
                                );
                            }
                            _ => {}
                        }
                    }
                }

                index += 1;
        //         CachedRect::Mask(data) => {
        //             self.batches.add_mask_rect(
        //                 &data.rect,
        //                 depth,
        //                 &data.color,
        //                 &data.coords,
        //                 data.image,
        //                 data.has_alpha,
        //             );
        //         }
        //         CachedRect::Standard((rect, bg_color)) => {
        //             self.batches.add_rect(rect, depth, bg_color);
        //         }
            }
        }
    }

    /// Draws a text run.
    #[inline]
    pub fn draw_glyphs<I>(
        &mut self,
        session: &mut GlyphCacheSession,
        rect: impl Into<Rect>,
        depth: f32,
        style: &TextRunStyle,
        glyphs: I,
        rects_per_glyph: &mut Option<&mut FxHashMap<usize, Vec<CachedRect>>>, 
        // dimension: SugarDimensions,
    )
    where
        I: Iterator,
        I::Item: Borrow<Glyph>,
    {
        // let start = std::time::Instant::now();

        let rect = rect.into();
        let (underline, underline_offset, underline_size, underline_color) =
            match style.underline {
                Some(underline) => (
                    true,
                    underline.offset.round() as i32,
                    underline.size.round().max(1.),
                    underline.color,
                ),
                _ => (false, 0, 0., [0.0, 0.0, 0.0, 0.0]),
            };
        if underline {
            self.intercepts.clear();
        }
        let subpx_bias = (0.125, 0.);
        let color = style.color;
        let x = rect.x;
        let mut glyph_acc = 0;
        for g in glyphs {
            let mut glyphs_to_cache = Vec::new();
            let glyph = g.borrow();
            let entry = session.get(glyph.id);
            if let Some(entry) = entry {
                if let Some(img) = session.get_image(entry.image) {
                    let gx = (glyph.x + subpx_bias.0).floor() + entry.left as f32;
                    let gy = (glyph.y + subpx_bias.1).floor() - entry.top as f32;

                    if entry.is_bitmap {
                        let rect =
                            Rect::new(gx, gy, entry.width as f32, entry.height as f32);
                        let color = [1.0, 1.0, 1.0, 1.0];
                        let coords = [img.min.0, img.min.1, img.max.0, img.max.1];
                        self.batches.add_image_rect(
                            &rect,
                            depth,
                            &color,
                            &coords,
                            img.texture_id,
                            entry.image.has_alpha(),
                        );
                        glyphs_to_cache.push(CachedRect::Image(ComposedRect {
                            rect: Rect::new(entry.left as f32, entry.top as f32, entry.width as f32, entry.height as f32),
                            color,
                            coords,
                            image: img.texture_id,
                            has_alpha: entry.image.has_alpha(),
                        }));
                    } else {
                        println!("{:?} {:?}", gx, gy);

                        let rect =
                            Rect::new(gx, gy, entry.width as f32, entry.height as f32);
                        let coords = [img.min.0, img.min.1, img.max.0, img.max.1];
                        self.batches.add_mask_rect(
                            &rect,
                            depth,
                            &color,
                            &coords,
                            img.texture_id,
                            true,
                        );
                        glyphs_to_cache.push(CachedRect::Mask(ComposedRect {
                            rect: Rect::new(entry.left as f32, entry.top as f32, entry.width as f32, entry.height as f32),
                            color,
                            coords,
                            image: img.texture_id,
                            has_alpha: true,
                        }));
                    }

                    if let Some(bg_color) = style.background_color {
                        let rect = Rect::new(
                            rect.x,
                            style.topline,
                            rect.width,
                            style.line_height,
                        );
                        self.batches.add_rect(&rect, depth, &bg_color);
                        glyphs_to_cache.push(CachedRect::Standard((rect, bg_color)));
                    }

                    match style.cursor {
                        SugarCursor::Block(cursor_color) => {
                            let rect = Rect::new(
                                rect.x,
                                style.topline,
                                rect.width,
                                style.line_height,
                            );
                            self.batches.add_rect(&rect, depth, &cursor_color);
                            glyphs_to_cache.push(CachedRect::Standard((rect, cursor_color)));
                        }
                        SugarCursor::Caret(cursor_color) => {
                            let rect =
                                Rect::new(rect.x, style.topline, 3.0, style.line_height);
                            self.batches.add_rect(&rect, depth, &cursor_color);
                            glyphs_to_cache.push(CachedRect::Standard((rect, cursor_color)));
                        }
                        _ => {}
                    }

                    if underline && entry.top - underline_offset < entry.height as i32 {
                        if let Some(mut desc_ink) = entry.desc.range() {
                            desc_ink.0 += gx;
                            desc_ink.1 += gx;
                            self.intercepts.push(desc_ink);
                        }
                    }
                }
            }

            if let Some(cached_rects) = rects_per_glyph {
                cached_rects.insert(glyph_acc, glyphs_to_cache);
            }
            glyph_acc += 1;
        }
        if underline {
            for range in self.intercepts.iter_mut() {
                range.0 -= 1.;
                range.1 += 1.;
            }
            let mut ux = x;
            let uy = style.baseline - underline_offset as f32;
            for range in self.intercepts.iter() {
                if ux < range.0 {
                    let rect = Rect::new(ux, uy, range.0 - ux, underline_size);
                    self.batches.add_rect(&rect, depth, &underline_color);
                    // result.push(CachedRect::Standard((rect, underline_color)));
                }
                ux = range.1;
            }
            let end = x + rect.width;
            if ux < end {
                let rect = Rect::new(ux, uy, end - ux, underline_size);
                self.batches.add_rect(&rect, depth, &underline_color);
                // result.push(CachedRect::Standard((rect, underline_color)));
            }
        }

        // let duration = start.elapsed();
        // println!(" - draw_glyphs() is: {:?}", duration);
    }
}
