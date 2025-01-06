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

use crate::components::rich_text::batch::BatchManager;
pub use crate::components::rich_text::batch::{DisplayList, Rect, Vertex};
use crate::components::rich_text::image_cache::glyph::GlyphCacheSession;
use crate::components::rich_text::image_cache::ImageCache;
pub use crate::components::rich_text::image_cache::ImageId;
use crate::components::rich_text::text::*;
use crate::layout::{FragmentStyleDecoration, UnderlineShape};
use crate::SugarCursor;

#[derive(Default)]
pub struct RunUnderline {
    enabled: bool,
    offset: i32,
    size: f32,
    color: [f32; 4],
    is_doubled: bool,
    shape: UnderlineShape,
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
    pub fn finish(&mut self, list: &mut DisplayList) {
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

    /// Draws a text run.
    #[inline]
    pub fn draw_run(
        &mut self,
        session: &mut GlyphCacheSession,
        rect: impl Into<Rect>,
        depth: f32,
        style: &TextRunStyle,
        glyphs: &[Glyph],
        // cached_run: &mut CachedRun,
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

        for glyph in glyphs {
            let entry = session.get(glyph.id);
            if let Some(entry) = entry {
                if let Some(img) = session.get_image(entry.image) {
                    let gx = (glyph.x + subpx_bias.0).floor() + entry.left as f32;
                    let gy = (glyph.y + subpx_bias.1).floor() - entry.top as f32;

                    if entry.is_bitmap {
                        let color = [1.0, 1.0, 1.0, 1.0];
                        let coords = [img.min.0, img.min.1, img.max.0, img.max.1];
                        self.batches.add_image_rect(
                            &Rect::new(gx, gy, entry.width as f32, entry.height as f32),
                            depth,
                            &color,
                            &coords,
                            entry.image.has_alpha(),
                        );
                    } else {
                        let coords = [img.min.0, img.min.1, img.max.0, img.max.1];
                        self.batches.add_mask_rect(
                            &Rect::new(gx, gy, entry.width as f32, entry.height as f32),
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
            self.batches.add_rect(
                &Rect::new(rect.x, style.topline, rect.width, style.line_height),
                depth,
                &bg_color,
            );
        }

        match style.cursor {
            Some(SugarCursor::Block(cursor_color)) => {
                self.batches.add_rect(
                    &Rect::new(
                        rect.x,
                        style.topline + style.padding_y,
                        rect.width,
                        style.line_height_without_mod,
                    ),
                    depth,
                    &cursor_color,
                );
            }
            Some(SugarCursor::HollowBlock(cursor_color)) => {
                self.batches.add_rect(
                    &Rect::new(
                        rect.x,
                        style.topline + style.padding_y,
                        rect.width,
                        style.line_height_without_mod,
                    ),
                    depth,
                    &cursor_color,
                );

                if let Some(bg_color) = style.background_color {
                    self.batches.add_rect(
                        &Rect::new(
                            rect.x + 2.0,
                            style.topline + style.padding_y + 2.0,
                            rect.width - 4.0,
                            style.line_height_without_mod - 4.0,
                        ),
                        depth,
                        &bg_color,
                    );
                }
            }
            Some(SugarCursor::Caret(cursor_color)) => {
                self.batches.add_rect(
                    &Rect::new(
                        rect.x,
                        style.topline + style.padding_y,
                        3.0,
                        style.line_height_without_mod,
                    ),
                    depth,
                    &cursor_color,
                );
            }
            _ => {}
        }

        if let Some(underline) = underline {
            self.draw_underline(
                &underline,
                rect.x,
                rect.width,
                style.baseline,
                depth,
                style.line_height_without_mod,
            );
        }

        // let duration = start.elapsed();
        // println!(" - draw_glyphs() is: {:?}", duration);
    }

    #[inline]
    fn draw_underline(
        &mut self,
        underline: &RunUnderline,
        x: f32,
        advance: f32,
        baseline: f32,
        depth: f32,
        line_height: f32,
    ) {
        if underline.enabled {
            let ux = x;
            let uy = baseline - underline.offset as f32;
            let end = x + advance;
            if ux < end {
                match underline.shape {
                    UnderlineShape::Regular => {
                        self.batches.add_rect(
                            &Rect::new(ux, uy, end - ux, underline.size),
                            depth,
                            &underline.color,
                        );
                        if underline.is_doubled {
                            self.batches.add_rect(
                                &Rect::new(
                                    ux,
                                    uy - (underline.size * 2.),
                                    end - ux,
                                    underline.size,
                                ),
                                depth,
                                &underline.color,
                            );
                        }
                    }
                    UnderlineShape::Dashed => {
                        let mut start = ux;
                        while start < end {
                            start = start.min(end);
                            self.batches.add_rect(
                                &Rect::new(start, uy, 6.0, underline.size),
                                depth,
                                &underline.color,
                            );
                            start += 8.0;
                        }
                    }
                    UnderlineShape::Dotted => {
                        let mut start = ux;
                        while start < end {
                            start = start.min(end);
                            self.batches.add_rect(
                                &Rect::new(start, uy, 2.0, underline.size),
                                depth,
                                &underline.color,
                            );
                            start += 4.0;
                        }
                    }
                    UnderlineShape::Curly => {
                        let style_line_height = (line_height / 10.).clamp(2.0, 16.0);
                        let size = (style_line_height / 1.5).clamp(1.0, 4.0);
                        let offset = style_line_height * 1.6;

                        let mut curly_width = ux;
                        let mut rect_width = 1.0f32.min(end - curly_width);

                        while curly_width < end {
                            rect_width = rect_width.min(end - curly_width);

                            let dot_bottom_offset = match curly_width as u32 % 8 {
                                3..=5 => offset + style_line_height,
                                2 | 6 => offset + 2.0 * style_line_height / 3.0,
                                1 | 7 => offset + 1.0 * style_line_height / 3.0,
                                _ => offset,
                            };

                            self.batches.add_rect(
                                &Rect::new(
                                    curly_width,
                                    uy - (dot_bottom_offset - offset),
                                    rect_width,
                                    size,
                                ),
                                depth,
                                &underline.color,
                            );

                            curly_width += rect_width;
                        }
                    }
                }
            }
        }
    }
}
