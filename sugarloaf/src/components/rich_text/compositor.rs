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
use crate::{contains_braille_dot, DrawableChar, SugarCursor};

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

        if let Some(builtin_character) = style.drawable_char {
            // x: f32,
            // y: f32,
            // char: DrawableChar,
            // color: [f32; 4],
            // depth: f32,
            // line_width: f32,
            // line_height: f32,
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

            self.draw_drawable_character(
                rect.x,
                style.topline,
                rect.width,
                builtin_character,
                color,
                depth,
                style.line_height_without_mod,
            );
        } else {
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
                                &Rect::new(
                                    gx,
                                    gy,
                                    entry.width as f32,
                                    entry.height as f32,
                                ),
                                depth,
                                &color,
                                &coords,
                                entry.image.has_alpha(),
                            );
                        } else {
                            let coords = [img.min.0, img.min.1, img.max.0, img.max.1];
                            self.batches.add_mask_rect(
                                &Rect::new(
                                    gx,
                                    gy,
                                    entry.width as f32,
                                    entry.height as f32,
                                ),
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

    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn draw_drawable_character(
        &mut self,
        x: f32,
        y: f32,
        advance: f32,
        character: DrawableChar,
        color: [f32; 4],
        depth: f32,
        line_height: f32,
    ) {
        let half_size = advance / 2.0;
        let stroke = f32::clamp(line_height / 10., 1.0, 6.0).round();
        let center_x = x + half_size;
        let center_y = y + (line_height / 2.0);
        let line_width = advance;

        match character {
            DrawableChar::Horizontal => {
                let rect = Rect {
                    x,
                    y: center_y - (stroke / 2.0),
                    width: line_width,
                    height: stroke,
                };
                self.batches.add_rect(&rect, depth, &color);
            }
            DrawableChar::DoubleHorizontal => {
                // Calculate spacing between the two horizontal lines
                let gap = stroke * 1.5; // Adjust this value as needed for desired appearance

                // Top horizontal line
                let top_rect = Rect {
                    x,
                    y: center_y - gap,
                    width: line_width,
                    height: stroke,
                };

                // Bottom horizontal line
                let bottom_rect = Rect {
                    x,
                    y: center_y + gap - stroke,
                    width: line_width,
                    height: stroke,
                };

                // Draw both rectangles
                self.batches.add_rect(&top_rect, depth, &color);
                self.batches.add_rect(&bottom_rect, depth, &color);
            }
            DrawableChar::HeavyHorizontal => {
                let heavy_stroke = stroke * 2.0;
                let rect = Rect {
                    x,
                    y: center_y - heavy_stroke / 2.0,
                    width: line_width,
                    height: heavy_stroke,
                };
                self.batches.add_rect(&rect, depth, &color);
            }
            DrawableChar::Vertical => {
                let rect = Rect {
                    x: center_x - (stroke / 2.0),
                    y,
                    width: stroke,
                    height: line_height,
                };
                self.batches.add_rect(&rect, depth, &color);
            }
            DrawableChar::DoubleVertical => {
                // Calculate spacing between the two vertical lines
                let gap = stroke * 1.5; // Adjust this value as needed for desired appearance

                // Left vertical line
                let left_rect = Rect {
                    x: center_x - gap,
                    y,
                    width: stroke,
                    height: line_height,
                };

                // Right vertical line
                let right_rect = Rect {
                    x: center_x + gap - stroke,
                    y,
                    width: stroke,
                    height: line_height,
                };

                // Draw both rectangles
                self.batches.add_rect(&left_rect, depth, &color);
                self.batches.add_rect(&right_rect, depth, &color);
            }
            DrawableChar::HeavyVertical => {
                let heavy_stroke = stroke * 2.0;
                let rect = Rect {
                    x: center_x - heavy_stroke / 2.0,
                    y,
                    width: heavy_stroke,
                    height: line_height,
                };
                self.batches.add_rect(&rect, depth, &color);
            }
            DrawableChar::LowerOneEighthBlock => {
                // Lower One Eighth Block (▁) - fills bottom 1/8 of the cell
                let block_height = line_height / 8.0;
                let block_rect = Rect {
                    x,
                    y: y + line_height - block_height, // Position at bottom 1/8
                    width: line_width,
                    height: block_height,
                };
                self.batches.add_rect(&block_rect, depth, &color);
            }
            DrawableChar::LowerOneQuarterBlock => {
                // Lower One Quarter Block (▂) - fills bottom 1/4 of the cell
                let block_height = line_height / 4.0;
                let block_rect = Rect {
                    x,
                    y: y + line_height - block_height, // Position at bottom 1/4
                    width: line_width,
                    height: block_height,
                };
                self.batches.add_rect(&block_rect, depth, &color);
            }
            DrawableChar::LowerThreeEighthsBlock => {
                // Lower Three Eighths Block (▃) - fills bottom 3/8 of the cell
                let block_height = (line_height * 3.0) / 8.0;
                let block_rect = Rect {
                    x,
                    y: y + line_height - block_height, // Position at bottom 3/8
                    width: line_width,
                    height: block_height,
                };
                self.batches.add_rect(&block_rect, depth, &color);
            }

            DrawableChar::LeftOneQuarterBlock => {
                // Left One Quarter Block (▎) - fills left 1/4 of the cell
                let block_width = line_width / 4.0;
                let block_rect = Rect {
                    x,
                    y,
                    width: block_width,
                    height: line_height,
                };
                self.batches.add_rect(&block_rect, depth, &color);
            }
            DrawableChar::LeftThreeEighthsBlock => {
                // Left Three Eighths Block (▍) - fills left 3/8 of the cell
                let block_width = (line_width * 3.0) / 8.0;
                let block_rect = Rect {
                    x,
                    y,
                    width: block_width,
                    height: line_height,
                };
                self.batches.add_rect(&block_rect, depth, &color);
            }
            DrawableChar::LeftThreeQuartersBlock => {
                // Left Three Quarters Block (▊) - fills left 3/4 of the cell
                let block_width = (line_width * 3.0) / 4.0;
                let block_rect = Rect {
                    x,
                    y,
                    width: block_width,
                    height: line_height,
                };
                self.batches.add_rect(&block_rect, depth, &color);
            }
            DrawableChar::RightOneQuarterBlock => {
                // Right One Quarter Block (▕) - fills right 1/4 of the cell
                let block_width = line_width / 4.0;
                let block_rect = Rect {
                    x: x + line_width - block_width, // Position at right 1/4
                    y,
                    width: block_width,
                    height: line_height,
                };
                self.batches.add_rect(&block_rect, depth, &color);
            }

            DrawableChar::RightThreeEighthsBlock => {
                // Right Three Eighths Block (🮈) - fills right 3/8 of the cell
                let block_width = (line_width * 3.0) / 8.0;
                let block_rect = Rect {
                    x: x + line_width - block_width, // Position at right 3/8
                    y,
                    width: block_width,
                    height: line_height,
                };
                self.batches.add_rect(&block_rect, depth, &color);
            }
            DrawableChar::RightThreeQuartersBlock => {
                // Right Three Quarters Block (🮊) - fills right 3/4 of the cell
                let block_width = (line_width * 3.0) / 4.0;
                let block_rect = Rect {
                    x: x + line_width - block_width, // Position at right 3/4
                    y,
                    width: block_width,
                    height: line_height,
                };
                self.batches.add_rect(&block_rect, depth, &color);
            }
            DrawableChar::UpperOneEighthBlock => {
                // Upper One Eighth Block (▔) - fills top 1/8 of the cell
                let block_height = line_height / 8.0;
                let block_rect = Rect {
                    x,
                    y, // Position at top
                    width: line_width,
                    height: block_height,
                };
                self.batches.add_rect(&block_rect, depth, &color);
            }
            DrawableChar::UpperThreeEighthsBlock => {
                // Upper Three Eighths Block (🮃) - fills top 3/8 of the cell
                let block_height = (line_height * 3.0) / 8.0;
                let block_rect = Rect {
                    x,
                    y, // Position at top
                    width: line_width,
                    height: block_height,
                };
                self.batches.add_rect(&block_rect, depth, &color);
            }
            DrawableChar::UpperThreeQuartersBlock => {
                // Upper Three Quarters Block (🮅) - fills top 3/4 of the cell
                let block_height = (line_height * 3.0) / 4.0;
                let block_rect = Rect {
                    x,
                    y, // Position at top
                    width: line_width,
                    height: block_height,
                };
                self.batches.add_rect(&block_rect, depth, &color);
            }
            DrawableChar::QuadrantUpperLeft => {
                let rect = Rect {
                    x,
                    y: center_y - line_height / 2.0,
                    width: line_width / 2.0,
                    height: line_height / 2.0,
                };
                self.batches.add_rect(&rect, depth, &color);
            }
            DrawableChar::QuadrantUpperRight => {
                let rect = Rect {
                    x: x + line_width / 2.0,
                    y: center_y - line_height / 2.0,
                    width: line_width / 2.0,
                    height: line_height / 2.0,
                };
                self.batches.add_rect(&rect, depth, &color);
            }
            DrawableChar::QuadrantLowerLeft => {
                let rect = Rect {
                    x,
                    y: center_y,
                    width: line_width / 2.0,
                    height: line_height / 2.0,
                };
                self.batches.add_rect(&rect, depth, &color);
            }
            DrawableChar::QuadrantLowerRight => {
                let rect = Rect {
                    x: x + line_width / 2.0,
                    y: center_y,
                    width: line_width / 2.0,
                    height: line_height / 2.0,
                };
                self.batches.add_rect(&rect, depth, &color);
            }
            DrawableChar::UpperHalf => {
                let rect = Rect {
                    x,
                    y: center_y - line_height / 2.0,
                    width: line_width,
                    height: line_height / 2.0,
                };
                self.batches.add_rect(&rect, depth, &color);
            }
            DrawableChar::LowerHalf => {
                let rect = Rect {
                    x,
                    y: center_y,
                    width: line_width,
                    height: line_height / 2.0,
                };
                self.batches.add_rect(&rect, depth, &color);
            }
            DrawableChar::LeftHalf => {
                let rect = Rect {
                    x,
                    y: center_y - line_height / 2.0,
                    width: line_width / 2.0,
                    height: line_height,
                };
                self.batches.add_rect(&rect, depth, &color);
            }
            DrawableChar::RightHalf => {
                let rect = Rect {
                    x: x + line_width / 2.0,
                    y: center_y - line_height / 2.0,
                    width: line_width / 2.0,
                    height: line_height,
                };
                self.batches.add_rect(&rect, depth, &color);
            }
            DrawableChar::DownDoubleAndHorizontalSingle => {
                // Calculate spacing between the two vertical lines
                let gap = stroke * 1.5; // Adjust this value as needed

                // Left vertical line - goes all the way down
                let left_rect = Rect {
                    x: center_x - gap,
                    y: center_y,
                    width: stroke,
                    height: line_height / 2.0, // Only the bottom half
                };

                // Right vertical line - goes all the way down
                let right_rect = Rect {
                    x: center_x + gap - stroke,
                    y: center_y,
                    width: stroke,
                    height: line_height / 2.0, // Only the bottom half
                };

                // Horizontal single line
                let horiz_rect = Rect {
                    x,
                    y: center_y - (stroke / 2.0),
                    width: line_width,
                    height: stroke,
                };

                // Draw all rectangles
                self.batches.add_rect(&left_rect, depth, &color);
                self.batches.add_rect(&right_rect, depth, &color);
                self.batches.add_rect(&horiz_rect, depth, &color);
            }
            DrawableChar::DownSingleAndHorizontalDouble => {
                // Calculate spacing between the double horizontal lines
                let gap = stroke * 1.5; // Adjust this value as needed

                // Single vertical line going down from center
                let vertical_rect = Rect {
                    x: center_x - (stroke / 2.0),
                    y: center_y + gap,
                    width: stroke,
                    height: (line_height / 2.0) - gap, // Bottom half
                };

                // Double horizontal lines
                let top_horizontal_rect = Rect {
                    x,
                    y: center_y - gap,
                    width: line_width,
                    height: stroke,
                };

                let bottom_horizontal_rect = Rect {
                    x,
                    y: center_y + gap - stroke,
                    width: line_width,
                    height: stroke,
                };

                // Draw all rectangles
                self.batches.add_rect(&vertical_rect, depth, &color);
                self.batches.add_rect(&top_horizontal_rect, depth, &color);
                self.batches
                    .add_rect(&bottom_horizontal_rect, depth, &color);
            }
            DrawableChar::DoubleUpAndRight => {
                // Calculate spacing between the double lines
                let gap = stroke * 1.5; // Adjust this value as needed

                // Vertical double lines going up from center
                let left_vertical_rect = Rect {
                    x: center_x - gap,
                    y,
                    width: stroke,
                    height: line_height / 2.0, // Top half
                };

                let right_vertical_rect = Rect {
                    x: center_x + gap - stroke,
                    y,
                    width: stroke,
                    height: line_height / 2.0, // Top half
                };

                // Horizontal double lines going right from center
                let top_horizontal_rect = Rect {
                    x: center_x,
                    y: center_y - gap,
                    width: line_width / 2.0, // Right half
                    height: stroke,
                };

                let bottom_horizontal_rect = Rect {
                    x: center_x,
                    y: center_y + gap - stroke,
                    width: line_width / 2.0, // Right half
                    height: stroke,
                };

                // Draw all rectangles
                self.batches.add_rect(&left_vertical_rect, depth, &color);
                self.batches.add_rect(&right_vertical_rect, depth, &color);
                self.batches.add_rect(&top_horizontal_rect, depth, &color);
                self.batches
                    .add_rect(&bottom_horizontal_rect, depth, &color);
            }
            DrawableChar::DoubleUpAndLeft => {
                // Calculate spacing between the double lines
                let gap = stroke * 1.5; // Adjust this value as needed

                // Vertical double lines going up from center
                let left_vertical_rect = Rect {
                    x: center_x - gap,
                    y,
                    width: stroke,
                    height: line_height / 2.0, // Top half
                };

                let right_vertical_rect = Rect {
                    x: center_x + gap - stroke,
                    y,
                    width: stroke,
                    height: line_height / 2.0, // Top half
                };

                // Horizontal double lines going left from center
                let top_horizontal_rect = Rect {
                    x,
                    y: center_y - gap,
                    width: line_width / 2.0, // Left half
                    height: stroke,
                };

                let bottom_horizontal_rect = Rect {
                    x,
                    y: center_y + gap - stroke,
                    width: line_width / 2.0, // Left half
                    height: stroke,
                };

                // Draw all rectangles
                self.batches.add_rect(&left_vertical_rect, depth, &color);
                self.batches.add_rect(&right_vertical_rect, depth, &color);
                self.batches.add_rect(&top_horizontal_rect, depth, &color);
                self.batches
                    .add_rect(&bottom_horizontal_rect, depth, &color);
            }
            DrawableChar::UpSingleAndRightDouble => {
                // Calculate spacing between the double horizontal lines
                let gap = stroke * 1.5; // Adjust this value as needed

                // Single vertical line going up from center
                let vertical_rect = Rect {
                    x: center_x - (stroke / 2.0),
                    y,
                    width: stroke,
                    height: line_height / 2.0, // Top half
                };

                // Double horizontal lines going right from center
                let top_horizontal_rect = Rect {
                    x: center_x,
                    y: center_y - gap,
                    width: line_width / 2.0, // Right half
                    height: stroke,
                };

                let bottom_horizontal_rect = Rect {
                    x: center_x,
                    y: center_y + gap - stroke,
                    width: line_width / 2.0, // Right half
                    height: stroke,
                };

                // Draw all rectangles
                self.batches.add_rect(&vertical_rect, depth, &color);
                self.batches.add_rect(&top_horizontal_rect, depth, &color);
                self.batches
                    .add_rect(&bottom_horizontal_rect, depth, &color);
            }
            DrawableChar::UpSingleAndLeftDouble => {
                // Calculate spacing between the double horizontal lines
                let gap = stroke * 1.5; // Adjust this value as needed

                // Single vertical line going up from center
                let vertical_rect = Rect {
                    x: center_x - (stroke / 2.0),
                    y,
                    width: stroke,
                    height: line_height / 2.0, // Top half
                };

                // Double horizontal lines going left from center
                let top_horizontal_rect = Rect {
                    x,
                    y: center_y - gap,
                    width: line_width / 2.0, // Left half
                    height: stroke,
                };

                let bottom_horizontal_rect = Rect {
                    x,
                    y: center_y + gap - stroke,
                    width: line_width / 2.0, // Left half
                    height: stroke,
                };

                // Draw all rectangles
                self.batches.add_rect(&vertical_rect, depth, &color);
                self.batches.add_rect(&top_horizontal_rect, depth, &color);
                self.batches
                    .add_rect(&bottom_horizontal_rect, depth, &color);
            }
            DrawableChar::VerticalSingleAndHorizontalDouble => {
                // Calculate spacing between the double horizontal lines
                let gap = stroke * 1.5; // Adjust this value as needed

                // Single vertical line going through the full height
                let vertical_rect = Rect {
                    x: center_x - (stroke / 2.0),
                    y,
                    width: stroke,
                    height: line_height,
                };

                // Double horizontal lines going across the full width
                let top_horizontal_rect = Rect {
                    x,
                    y: center_y - gap,
                    width: line_width,
                    height: stroke,
                };

                let bottom_horizontal_rect = Rect {
                    x,
                    y: center_y + gap - stroke,
                    width: line_width,
                    height: stroke,
                };

                // Draw all rectangles
                self.batches.add_rect(&vertical_rect, depth, &color);
                self.batches.add_rect(&top_horizontal_rect, depth, &color);
                self.batches
                    .add_rect(&bottom_horizontal_rect, depth, &color);
            }
            DrawableChar::LightShade => {
                // For light shade (25% filled), create a sparse dot pattern
                // (░)
                let dot_size = stroke;
                let cols = 4;
                let rows = 8;
                let cell_width = line_width / cols as f32;
                let cell_height = line_height / rows as f32;

                for j in 0..rows {
                    for i in 0..cols {
                        // Place dots in alternating positions:
                        // If row is even (0, 2), place dots at even columns (0, 2)
                        // If row is odd (1, 3), place dots at odd columns (1, 3)
                        if (j % 2 == 0 && i % 2 == 0) || (j % 2 == 1 && i % 2 == 1) {
                            let dot_x =
                                x + i as f32 * cell_width + (cell_width - dot_size) / 2.0;
                            let dot_y = center_y - line_height / 2.0
                                + j as f32 * cell_height
                                + (cell_height - dot_size) / 2.0;

                            let rect = Rect {
                                x: dot_x,
                                y: dot_y,
                                width: dot_size,
                                height: dot_size,
                            };
                            self.batches.add_rect(&rect, depth, &color);
                        }
                    }
                }
            }
            DrawableChar::MediumShade => {
                // For medium shade (50% filled), create a denser pattern
                // (▒)
                let dot_size = stroke;
                let cols = 4;
                let rows = 8;
                let cell_width = line_width / cols as f32;
                let cell_height = line_height / rows as f32;

                // First layer - same as light shade
                for j in 0..rows {
                    for i in 0..cols {
                        if (j % 2 == 0 && i % 2 == 0) || (j % 2 == 1 && i % 2 == 1) {
                            let dot_x =
                                x + i as f32 * cell_width + (cell_width - dot_size) / 2.0;
                            let dot_y = center_y - line_height / 2.0
                                + j as f32 * cell_height
                                + (cell_height - dot_size) / 2.0;
                            let rect = Rect {
                                x: dot_x,
                                y: dot_y,
                                width: dot_size,
                                height: dot_size,
                            };
                            self.batches.add_rect(&rect, depth, &color);
                        }
                    }
                }

                // Second layer - offset pattern at half the size for medium shade
                let small_dot_size = dot_size * 0.75;
                for j in 0..rows {
                    for i in 0..cols {
                        if (j % 2 == 1 && i % 2 == 0) || (j % 2 == 0 && i % 2 == 1) {
                            let dot_x = x
                                + i as f32 * cell_width
                                + (cell_width - small_dot_size) / 2.0;
                            let dot_y = center_y - line_height / 2.0
                                + j as f32 * cell_height
                                + (cell_height - small_dot_size) / 2.0;
                            let rect = Rect {
                                x: dot_x,
                                y: dot_y,
                                width: small_dot_size,
                                height: small_dot_size,
                            };
                            self.batches.add_rect(&rect, depth, &color);
                        }
                    }
                }
            }
            DrawableChar::DarkShade => {
                // For dark shade (75% filled)
                // (▓)
                let dot_size = stroke;
                let cols = 4;
                let rows = 8;
                let cell_width = line_width / cols as f32;
                let cell_height = line_height / rows as f32;

                // Base layer - fill the entire rectangle with a semi-transparent color
                let rect = Rect {
                    x,
                    y: center_y - line_height / 2.0,
                    width: line_width,
                    height: line_height,
                };
                let base_color = [
                    color[0] * 0.6,
                    color[1] * 0.6,
                    color[2] * 0.6,
                    color[3] * 0.6,
                ];
                self.batches.add_rect(&rect, depth + 0.0001, &base_color);

                // Add dots everywhere
                for j in 0..rows {
                    for i in 0..cols {
                        let dot_x =
                            x + i as f32 * cell_width + (cell_width - dot_size) / 2.0;
                        let dot_y = center_y - line_height / 2.0
                            + j as f32 * cell_height
                            + (cell_height - dot_size) / 2.0;
                        let rect = Rect {
                            x: dot_x,
                            y: dot_y,
                            width: dot_size,
                            height: dot_size,
                        };
                        self.batches.add_rect(&rect, depth, &color);

                        // Skip some dots to create tiny gaps (only in a few positions)
                        if j % 4 == 0 && i % 4 == 0 {
                            // This creates small gaps in a regular pattern
                            continue;
                        }
                    }
                }
            }
            DrawableChar::FullBlock => {
                let rect = Rect {
                    x,
                    y: center_y - line_height / 2.0,
                    width: line_width,
                    height: line_height,
                };
                self.batches.add_rect(&rect, depth, &color);
            }
            DrawableChar::Cross => {
                // Horizontal part
                let rect_h = Rect {
                    x,
                    y: center_y - stroke / 2.0,
                    width: line_width,
                    height: stroke,
                };
                self.batches.add_rect(&rect_h, depth, &color);

                // Vertical part
                let rect_v = Rect {
                    x: center_x - stroke / 2.0,
                    y,
                    width: stroke,
                    height: line_height,
                };
                self.batches.add_rect(&rect_v, depth, &color);
            }
            DrawableChar::TopRight => {
                // Horizontal part (from center to right)
                let vertical_rect = Rect {
                    x: center_x - stroke / 2.0,
                    y,
                    width: stroke,
                    height: (line_height / 2.0) + (stroke / 2.0),
                };
                self.batches.add_rect(&vertical_rect, depth, &color);

                // Horizontal line from left to center
                let horizontal_rect = Rect {
                    x: center_x,
                    y: center_y - stroke / 2.0,
                    width: line_width / 2.0,
                    height: stroke,
                };
                self.batches.add_rect(&horizontal_rect, depth, &color);
            }
            DrawableChar::TopLeft => {
                let vertical_rect = Rect {
                    x: center_x - stroke / 2.0,
                    y,
                    width: stroke,
                    height: (line_height / 2.0) + (stroke / 2.0),
                };
                self.batches.add_rect(&vertical_rect, depth, &color);

                // Horizontal line from left to center
                let horizontal_rect = Rect {
                    x,
                    y: center_y - stroke / 2.0,
                    width: half_size,
                    height: stroke,
                };
                self.batches.add_rect(&horizontal_rect, depth, &color);
            }
            DrawableChar::BottomRight => {
                // Horizontal part (from center to right)
                let rect_h = Rect {
                    x: center_x,
                    y: center_y - stroke / 2.0,
                    width: half_size,
                    height: stroke,
                };
                self.batches.add_rect(&rect_h, depth, &color);

                // Vertical part (from center to bottom)
                let rect_v = Rect {
                    x: center_x - stroke / 2.0,
                    y: center_y,
                    width: stroke,
                    height: line_height / 2.0,
                };
                self.batches.add_rect(&rect_v, depth, &color);
            }
            DrawableChar::BottomLeft => {
                // Horizontal part (from left to center)
                let rect_h = Rect {
                    x,
                    y: center_y - stroke / 2.0,
                    width: half_size,
                    height: stroke,
                };
                self.batches.add_rect(&rect_h, depth, &color);

                // Vertical part (from center to bottom)
                let rect_v = Rect {
                    x: center_x - stroke / 2.0,
                    y: center_y,
                    width: stroke,
                    height: line_height / 2.0,
                };
                self.batches.add_rect(&rect_v, depth, &color);
            }
            DrawableChar::ArcTopLeft => {
                // Arc corner at bottom-right (╯)
                // Vertical line from top to center
                let radius = line_width / 4.0;
                let vertical_rect = Rect {
                    x: center_x - stroke / 2.0,
                    y,
                    width: stroke,
                    height: (line_height / 2.0) - radius,
                };
                self.batches.add_rect(&vertical_rect, depth, &color);

                // Horizontal line from left to center
                let horizontal_rect = Rect {
                    x,
                    y: center_y - stroke / 2.0,
                    width: (line_width / 2.0) - radius,
                    height: stroke,
                };
                self.batches.add_rect(&horizontal_rect, depth, &color);

                // Arc in the bottom-left quarter (connecting horizontal and vertical lines)
                self.batches.add_arc(
                    center_x - radius,
                    center_y - radius,
                    line_width / 4.0, // Smaller radius for better appearance
                    0.0,              // Start angle
                    90.0,             // End angle (quarter circle)
                    stroke,
                    depth,
                    &color,
                );
            }
            DrawableChar::ArcBottomRight => {
                // Arc corner at top-left (┌)
                // Vertical line from center to bottom
                let radius = line_width / 4.0;
                let vertical_rect = Rect {
                    x: center_x - stroke / 2.0,
                    y: center_y + radius,
                    width: stroke,
                    height: (line_height / 2.0) - radius,
                };
                self.batches.add_rect(&vertical_rect, depth, &color);
                // Horizontal line from center to right
                let horizontal_rect = Rect {
                    x: center_x + radius,
                    y: center_y - stroke / 2.0,
                    width: (line_width / 2.0) - radius,
                    height: stroke,
                };
                self.batches.add_rect(&horizontal_rect, depth, &color);
                // Arc in the top-left quarter (connecting horizontal and vertical lines)
                self.batches.add_arc(
                    center_x + radius,
                    center_y + radius,
                    radius, // Smaller radius for better appearance
                    180.0,  // Start angle
                    270.0,  // End angle (quarter circle)
                    stroke,
                    depth,
                    &color,
                );
            }

            DrawableChar::ArcBottomLeft => {
                // Arc corner at top-right (┐)
                // Vertical line from center to bottom
                let radius = line_width / 4.0;
                let vertical_rect = Rect {
                    x: center_x - stroke / 2.0,
                    y: center_y + radius,
                    width: stroke,
                    height: (line_height / 2.0) - radius,
                };
                self.batches.add_rect(&vertical_rect, depth, &color);
                // Horizontal line from left to center
                let horizontal_rect = Rect {
                    x,
                    y: center_y - stroke / 2.0,
                    width: center_x - radius - x,
                    height: stroke,
                };
                self.batches.add_rect(&horizontal_rect, depth, &color);
                // Arc in the top-right quarter (connecting horizontal and vertical lines)
                self.batches.add_arc(
                    center_x - radius,
                    center_y + radius,
                    radius, // Smaller radius for better appearance
                    270.0,  // Start angle
                    360.0,  // End angle (quarter circle)
                    stroke,
                    depth,
                    &color,
                );
            }
            DrawableChar::ArcTopRight => {
                // Arc corner at bottom-left (╰)
                // Vertical line from top to center
                let radius = line_width / 4.0;
                let vertical_rect = Rect {
                    x: center_x - stroke / 2.0,
                    y,
                    width: stroke,
                    height: center_y - radius - y,
                };
                self.batches.add_rect(&vertical_rect, depth, &color);
                // Horizontal line from center to right
                let horizontal_rect = Rect {
                    x: center_x + radius,
                    y: center_y - stroke / 2.0,
                    width: (line_width / 2.0) - radius,
                    height: stroke,
                };
                self.batches.add_rect(&horizontal_rect, depth, &color);
                // Arc in the bottom-right quarter (connecting horizontal and vertical lines)
                self.batches.add_arc(
                    center_x + radius,
                    center_y - radius,
                    radius, // Smaller radius for better appearance
                    90.0,   // Start angle
                    180.0,  // End angle (quarter circle)
                    stroke,
                    depth,
                    &color,
                );
            }
            DrawableChar::VerticalRight => {
                // Vertical line
                let rect_v = Rect {
                    x: center_x - stroke / 2.0,
                    y,
                    width: stroke,
                    height: line_height,
                };
                self.batches.add_rect(&rect_v, depth, &color);

                // Horizontal line (from center to right)
                let rect_h = Rect {
                    x: center_x + (stroke / 2.0),
                    y: center_y - stroke / 2.0,
                    width: half_size - (stroke / 2.0),
                    height: stroke,
                };
                self.batches.add_rect(&rect_h, depth, &color);
            }
            DrawableChar::VerticalLeft => {
                // Vertical line
                let rect_v = Rect {
                    x: center_x - stroke / 2.0,
                    y,
                    width: stroke,
                    height: line_height,
                };
                self.batches.add_rect(&rect_v, depth, &color);

                // Horizontal line (from left to center)
                let rect_h = Rect {
                    x,
                    y: center_y - stroke / 2.0,
                    width: half_size - (stroke / 2.0),
                    height: stroke,
                };
                self.batches.add_rect(&rect_h, depth, &color);
            }
            DrawableChar::HorizontalDown => {
                // Horizontal line
                let rect_h = Rect {
                    x,
                    y: center_y - stroke / 2.0,
                    width: advance,
                    height: stroke,
                };
                self.batches.add_rect(&rect_h, depth, &color);

                // Vertical line (from center to bottom)
                let rect_v = Rect {
                    x: center_x - stroke / 2.0,
                    y: center_y,
                    width: stroke,
                    height: line_height / 2.0,
                };
                self.batches.add_rect(&rect_v, depth, &color);
            }
            DrawableChar::HorizontalUp => {
                // Horizontal line
                let rect_h = Rect {
                    x,
                    y: center_y - stroke / 2.0,
                    width: advance,
                    height: stroke,
                };
                self.batches.add_rect(&rect_h, depth, &color);

                // Vertical line (from center to top)
                let rect_v = Rect {
                    x: center_x - stroke / 2.0,
                    y,
                    width: stroke,
                    height: line_height / 2.0,
                };
                self.batches.add_rect(&rect_v, depth, &color);
            }
            DrawableChar::PowerlineLeftSolid => {
                // PowerlineLeftSolid - solid triangle pointing left
                // Creates a filled triangle pointing to the left
                self.batches.add_triangle(
                    x + line_width,
                    y, // Top-right (x1, y1)
                    x + line_width,
                    y + line_height, // Bottom-right (x2, y2)
                    x,
                    y + line_height / 2.0, // Middle-left (x3, y3)
                    depth,
                    color,
                );
            }
            DrawableChar::PowerlineRightSolid => {
                // PowerlineRightSolid - solid triangle pointing right
                // Creates a filled triangle pointing to the right
                self.batches.add_triangle(
                    x,
                    y, // Top-left (x1, y1)
                    x,
                    y + line_height, // Bottom-left (x2, y2)
                    x + line_width,
                    y + line_height / 2.0, // Middle-right (x3, y3)
                    depth,
                    color,
                );
            }
            DrawableChar::PowerlineLeftHollow => {
                // PowerlineLeftHollow - hollow triangle pointing left

                // Define stroke width for the hollow triangle outline
                let stroke_width = line_width * 0.1; // Adjust as needed for desired thickness

                // Top edge: from top-right to middle-left
                self.batches.add_line(
                    x + line_width,
                    y, // Start point (top-right)
                    x,
                    y + line_height / 2.0, // End point (middle-left)
                    stroke_width,
                    depth,
                    color,
                );

                // Bottom edge: from middle-left to bottom-right
                self.batches.add_line(
                    x,
                    y + line_height / 2.0, // Start point (middle-left)
                    x + line_width,
                    y + line_height, // End point (bottom-right)
                    stroke_width,
                    depth,
                    color,
                );

                // // Right edge: from bottom-right to top-right
                // self.batches.add_line(
                //     x + line_width,
                //     y + line_height, // Start point (bottom-right)
                //     x + line_width,
                //     y, // End point (top-right)
                //     stroke_width,
                //     depth,
                //     &color,
                // );
            }
            DrawableChar::PowerlineRightHollow => {
                // PowerlineRightHollow - hollow triangle pointing right

                // Define stroke width for the hollow triangle outline
                let stroke_width = line_width * 0.1; // Adjust as needed for desired thickness

                // Top edge: from top-left to middle-right
                self.batches.add_line(
                    x,
                    y, // Start point (top-left)
                    x + line_width,
                    y + line_height / 2.0, // End point (middle-right)
                    stroke_width,
                    depth,
                    color,
                );

                // Bottom edge: from middle-right to bottom-left
                self.batches.add_line(
                    x + line_width,
                    y + line_height / 2.0, // Start point (middle-right)
                    x,
                    y + line_height, // End point (bottom-left)
                    stroke_width,
                    depth,
                    color,
                );

                // Left edge: from bottom-left to top-left
                // self.batches.add_line(
                //     x,
                //     y + line_height, // Start point (bottom-left)
                //     x,
                //     y, // End point (top-left)
                //     stroke_width,
                //     depth,
                //     &color,
                // );
            }
            DrawableChar::PowerlineCurvedLeftSolid => {
                // Number of segments to create a smooth curve
                let segments = 30;
                // Create points for the polygon
                let mut points = Vec::with_capacity(segments + 2);
                // Add the right side points first (straight edge)
                points.push((x + line_width, y)); // Top-right
                points.push((x + line_width, y + line_height)); // Bottom-right

                // Create the curved left side (half oval)
                for i in (0..=segments).rev() {
                    // Draw from bottom to top
                    let t = i as f32 / segments as f32; // Parameter from 0 to 1

                    // For a half oval, we use the parametric equation of an ellipse
                    // The horizontal radius is line_width
                    // The vertical radius is line_height/2

                    // Calculate y position (moving from bottom to top)
                    let y_pos = y + line_height * (1.0 - t);

                    // Calculate x position using the ellipse formula x = a * sqrt(1 - (y/b)²)
                    // Where a is the horizontal radius and b is the vertical radius
                    // We need to normalize y to be between -1 and 1 for the calculation
                    let normalized_y = 2.0 * t - 1.0;

                    // Calculate the x position based on the ellipse equation
                    let x_pos = x + line_width
                        - (line_width * (1.0 - normalized_y * normalized_y).sqrt());

                    points.push((x_pos, y_pos));
                }

                // Draw the filled polygon with all our points
                self.batches.add_polygon(&points, depth, color);
            }
            DrawableChar::PowerlineCurvedRightSolid => {
                // Number of segments to create a smooth curve
                let segments = 30;
                // Create points for the polygon
                let mut points = Vec::with_capacity(segments + 2);
                // Add the left side points first (straight edge)
                points.push((x, y)); // Top-left
                points.push((x, y + line_height)); // Bottom-left
                                                   // Create the curved right side (half oval)
                for i in (0..=segments).rev() {
                    // Draw from bottom to top
                    let t = i as f32 / segments as f32; // Parameter from 0 to 1
                                                        // For a half oval, we use the parametric equation of an ellipse
                                                        // The horizontal radius is line_width
                                                        // The vertical radius is line_height/2
                                                        // Calculate y position (moving from bottom to top)
                    let y_pos = y + line_height * (1.0 - t);
                    // Calculate x position using the ellipse formula x = a * sqrt(1 - (y/b)²)
                    // Where a is the horizontal radius and b is the vertical radius
                    // We need to normalize y to be between -1 and 1 for the calculation
                    let normalized_y = 2.0 * t - 1.0;
                    // Calculate the x position based on the ellipse equation
                    // For right curve, we add to x instead of subtracting from x + line_width
                    let x_pos =
                        x + (line_width * (1.0 - normalized_y * normalized_y).sqrt());
                    points.push((x_pos, y_pos));
                }
                // Draw the filled polygon with all our points
                self.batches.add_polygon(&points, depth, color);
            }
            DrawableChar::PowerlineCurvedLeftHollow => {
                // Number of segments to create a smooth curve
                let segments = 30;
                let line_thickness = stroke / 2.;

                // Draw the vertical line on the right side
                // self.batches.add_line(
                //     x + line_width, y,
                //     x + line_width, y + line_height,
                //     line_thickness, depth, color
                // );

                // Draw the curved left side from top to bottom
                for i in 0..segments {
                    let t1 = i as f32 / segments as f32;
                    let t2 = (i + 1) as f32 / segments as f32;

                    // Calculate positions
                    let y1 = y + line_height * t1;
                    let y2 = y + line_height * t2;

                    // Calculate x positions
                    let normalized_t1 = 2.0 * t1 - 1.0;
                    let normalized_t2 = 2.0 * t2 - 1.0;

                    let x_factor1 = f32::sqrt(1.0 - normalized_t1 * normalized_t1);
                    let x_factor2 = f32::sqrt(1.0 - normalized_t2 * normalized_t2);

                    let x1 = x + (line_width * (1.0 - x_factor1));
                    let x2 = x + (line_width * (1.0 - x_factor2));

                    // Draw segment of the curve
                    self.batches
                        .add_line(x1, y1, x2, y2, line_thickness, depth, color);
                }

                // Calculate endpoints for top and bottom
                let top_normalized_t = -1.0; // t=0 gives normalized_t = -1
                let bottom_normalized_t = 1.0; // t=1 gives normalized_t = 1

                let top_x_factor = f32::sqrt(1.0 - top_normalized_t * top_normalized_t);
                let bottom_x_factor =
                    f32::sqrt(1.0 - bottom_normalized_t * bottom_normalized_t);

                let top_x = x + (line_width * (1.0 - top_x_factor));
                let bottom_x = x + (line_width * (1.0 - bottom_x_factor));

                // Draw the horizontal line at the top
                self.batches.add_line(
                    top_x,
                    y,
                    x + line_width,
                    y,
                    line_thickness,
                    depth,
                    color,
                );

                // Draw the horizontal line at the bottom
                self.batches.add_line(
                    bottom_x,
                    y + line_height,
                    x + line_width,
                    y + line_height,
                    line_thickness,
                    depth,
                    color,
                );
            }
            DrawableChar::PowerlineCurvedRightHollow => {
                // Number of segments to create a smooth curve
                let segments = 30;
                let line_thickness = stroke / 2.;

                // Draw the vertical line on the left side
                // self.batches.add_line(
                //     x,
                //     y,
                //     x,
                //     y + line_height,
                //     line_thickness,
                //     depth,
                //     color,
                // );

                // Draw the curved right side from top to bottom
                for i in 0..segments {
                    let t1 = i as f32 / segments as f32;
                    let t2 = (i + 1) as f32 / segments as f32;

                    // Calculate positions
                    let y1 = y + line_height * t1;
                    let y2 = y + line_height * t2;

                    // Calculate x positions - flipped from left version
                    let normalized_t1 = 2.0 * t1 - 1.0;
                    let normalized_t2 = 2.0 * t2 - 1.0;

                    let x_factor1 = f32::sqrt(1.0 - normalized_t1 * normalized_t1);
                    let x_factor2 = f32::sqrt(1.0 - normalized_t2 * normalized_t2);

                    // For right curve, we add the factor instead of subtracting
                    let x1 = x + (line_width * x_factor1);
                    let x2 = x + (line_width * x_factor2);

                    // Draw segment of the curve
                    self.batches
                        .add_line(x1, y1, x2, y2, line_thickness, depth, color);
                }

                // Calculate endpoints for top and bottom
                let top_normalized_t = -1.0; // t=0 gives normalized_t = -1
                let bottom_normalized_t = 1.0; // t=1 gives normalized_t = 1

                let top_x_factor = f32::sqrt(1.0 - top_normalized_t * top_normalized_t);
                let bottom_x_factor =
                    f32::sqrt(1.0 - bottom_normalized_t * bottom_normalized_t);

                // For right curve, we add the factor instead of subtracting
                let top_x = x + (line_width * top_x_factor);
                let bottom_x = x + (line_width * bottom_x_factor);

                // Draw the horizontal line at the top
                self.batches
                    .add_line(x, y, top_x, y, line_thickness, depth, color);

                // Draw the horizontal line at the bottom
                self.batches.add_line(
                    x,
                    y + line_height,
                    bottom_x,
                    y + line_height,
                    line_thickness,
                    depth,
                    color,
                );
            }
            DrawableChar::PowerlineLowerLeftTriangle => {
                // POWERLINE_EXTRA_LOWER_LEFT_TRIANGLE - solid triangle pointing to lower left
                self.batches.add_triangle(
                    x,
                    y + line_height, // Bottom-left (x1, y1)
                    x + line_width,
                    y + line_height, // Bottom-right (x2, y2)
                    x,
                    y, // Top-left (x3, y3)
                    depth,
                    color,
                );
            }
            DrawableChar::PowerlineBackslashSeparator => {
                // POWERLINE_EXTRA_BACKSLASH_SEPARATOR - diagonal line from top right to bottom left
                let stroke_width = line_width * 0.1; // Adjust thickness as needed
                self.batches.add_line(
                    x + line_width,
                    y, // Start point (top-right)
                    x,
                    y + line_height, // End point (bottom-left)
                    stroke_width,
                    depth,
                    color,
                );
            }
            DrawableChar::PowerlineLowerRightTriangle => {
                // POWERLINE_EXTRA_LOWER_RIGHT_TRIANGLE - solid triangle pointing to lower right
                self.batches.add_triangle(
                    x,
                    y + line_height, // Bottom-left (x1, y1)
                    x + line_width,
                    y + line_height, // Bottom-right (x2, y2)
                    x + line_width,
                    y, // Top-right (x3, y3)
                    depth,
                    color,
                );
            }
            DrawableChar::PowerlineForwardslashSeparator => {
                // POWERLINE_EXTRA_FORWARDSLASH_SEPARATOR - diagonal line from top left to bottom right
                let stroke_width = line_width * 0.1; // Adjust thickness as needed
                self.batches.add_line(
                    x,
                    y, // Start point (top-left)
                    x + line_width,
                    y + line_height, // End point (bottom-right)
                    stroke_width,
                    depth,
                    color,
                );
            }
            DrawableChar::PowerlineUpperLeftTriangle => {
                // POWERLINE_EXTRA_UPPER_LEFT_TRIANGLE - solid triangle pointing to upper left
                self.batches.add_triangle(
                    x,
                    y, // Top-left (x1, y1)
                    x + line_width,
                    y, // Top-right (x2, y2)
                    x,
                    y + line_height, // Bottom-left (x3, y3)
                    depth,
                    color,
                );
            }
            DrawableChar::PowerlineForwardslashSeparatorRedundant => {
                // This appears to be another forward slash separator (redundant)
                // Using same implementation as PowerlineForwardslashSeparator
                let stroke_width = line_width * 0.1; // Adjust thickness as needed
                self.batches.add_line(
                    x,
                    y, // Start point (top-left)
                    x + line_width,
                    y + line_height, // End point (bottom-right)
                    stroke_width,
                    depth,
                    color,
                );
            }
            DrawableChar::PowerlineUpperRightTriangle => {
                // POWERLINE_EXTRA_UPPER_RIGHT_TRIANGLE - solid triangle pointing to upper right
                self.batches.add_triangle(
                    x,
                    y, // Top-left (x1, y1)
                    x + line_width,
                    y, // Top-right (x2, y2)
                    x + line_width,
                    y + line_height, // Bottom-right (x3, y3)
                    depth,
                    color,
                );
            }
            DrawableChar::PowerlineBackslashSeparatorRedundant => {
                // This appears to be another backslash separator (redundant)
                // Using same implementation as PowerlineBackslashSeparator
                let stroke_width = line_width * 0.1; // Adjust thickness as needed
                self.batches.add_line(
                    x + line_width,
                    y, // Start point (top-right)
                    x,
                    y + line_height, // End point (bottom-left)
                    stroke_width,
                    depth,
                    color,
                );
            }
            DrawableChar::HorizontalLightDash => {
                // ┄ - Light dashed horizontal line with 3 dashes
                let dash_count = 3;
                let total_space = dash_count - 1;
                let dash_width =
                    (line_width - (total_space as f32 * stroke)) / dash_count as f32;

                for i in 0..dash_count {
                    let dash_x = x + (i as f32) * (dash_width + stroke);
                    let rect = Rect {
                        x: dash_x,
                        y: center_y - (stroke / 2.0),
                        width: dash_width,
                        height: stroke,
                    };
                    self.batches.add_rect(&rect, depth, &color);
                }
            }
            DrawableChar::HorizontalHeavyDash => {
                // ┅ - Heavy dashed horizontal line with 3 dashes
                let heavy_stroke = stroke * 1.8;
                let dash_count = 3;
                let total_space = dash_count - 1;
                let dash_width =
                    (line_width - (total_space as f32 * stroke)) / dash_count as f32;

                for i in 0..dash_count {
                    let dash_x = x + (i as f32) * (dash_width + stroke);
                    let rect = Rect {
                        x: dash_x,
                        y: center_y - heavy_stroke / 2.0,
                        width: dash_width,
                        height: heavy_stroke,
                    };
                    self.batches.add_rect(&rect, depth, &color);
                }
            }
            DrawableChar::HorizontalLightDoubleDash => {
                // ┈ - Light double-dashed horizontal line with 4 dashes
                let dash_count = 4;
                let total_space = dash_count - 1;
                let dash_width =
                    (line_width - (total_space as f32 * stroke)) / dash_count as f32;

                for i in 0..dash_count {
                    let dash_x = x + (i as f32) * (dash_width + stroke);
                    let rect = Rect {
                        x: dash_x,
                        y: center_y - (stroke / 2.0),
                        width: dash_width,
                        height: stroke,
                    };
                    self.batches.add_rect(&rect, depth, &color);
                }
            }
            DrawableChar::HorizontalHeavyDoubleDash => {
                // ┉ - Heavy double-dashed horizontal line with 4 dashes
                let heavy_stroke = stroke * 1.8;
                let dash_count = 4;
                let total_space = dash_count - 1;
                let dash_width =
                    (line_width - (total_space as f32 * stroke)) / dash_count as f32;

                for i in 0..dash_count {
                    let dash_x = x + (i as f32) * (dash_width + stroke);
                    let rect = Rect {
                        x: dash_x,
                        y: center_y - heavy_stroke / 2.0,
                        width: dash_width,
                        height: heavy_stroke,
                    };
                    self.batches.add_rect(&rect, depth, &color);
                }
            }
            DrawableChar::HorizontalLightTripleDash => {
                // ╌ - Light triple-dashed horizontal line with 2 dashes
                let dash_count = 2;
                let total_space = dash_count - 1;
                let dash_width =
                    (line_width - (total_space as f32 * stroke)) / dash_count as f32;

                for i in 0..dash_count {
                    let dash_x = x + (i as f32) * (dash_width + stroke);
                    let rect = Rect {
                        x: dash_x,
                        y: center_y - (stroke / 2.0),
                        width: dash_width,
                        height: stroke,
                    };
                    self.batches.add_rect(&rect, depth, &color);
                }
            }
            DrawableChar::HorizontalHeavyTripleDash => {
                // ╍ - Heavy triple-dashed horizontal line with 2 dashes
                let heavy_stroke = stroke * 1.8;
                let dash_count = 2;
                let total_space = dash_count - 1;
                let dash_width =
                    (line_width - (total_space as f32 * stroke)) / dash_count as f32;

                for i in 0..dash_count {
                    let dash_x = x + (i as f32) * (dash_width + stroke);
                    let rect = Rect {
                        x: dash_x,
                        y: center_y - heavy_stroke / 2.0,
                        width: dash_width,
                        height: heavy_stroke,
                    };
                    self.batches.add_rect(&rect, depth, &color);
                }
            }
            DrawableChar::VerticalLightDash => {
                // ┆ - Light dashed vertical line with 3 dashes
                let dash_count = 3;
                let total_space = dash_count - 1;
                let dash_height =
                    (line_height - (total_space as f32 * stroke)) / dash_count as f32;

                for i in 0..dash_count {
                    let dash_y = y + (i as f32) * (dash_height + stroke);
                    let rect = Rect {
                        x: center_x - (stroke / 2.0),
                        y: dash_y,
                        width: stroke,
                        height: dash_height,
                    };
                    self.batches.add_rect(&rect, depth, &color);
                }
            }
            DrawableChar::VerticalHeavyDash => {
                // ┇ - Heavy dashed vertical line with 3 dashes
                let heavy_stroke = stroke * 1.8;
                let dash_count = 3;
                let total_space = dash_count - 1;
                let dash_height =
                    (line_height - (total_space as f32 * stroke)) / dash_count as f32;

                for i in 0..dash_count {
                    let dash_y = y + (i as f32) * (dash_height + stroke);
                    let rect = Rect {
                        x: center_x - heavy_stroke / 2.0,
                        y: dash_y,
                        width: heavy_stroke,
                        height: dash_height,
                    };
                    self.batches.add_rect(&rect, depth, &color);
                }
            }
            DrawableChar::VerticalLightDoubleDash => {
                // ┊ - Light double-dashed vertical line with 4 dashes
                let dash_count = 4;
                let total_space = dash_count - 1;
                let dash_height =
                    (line_height - (total_space as f32 * stroke)) / dash_count as f32;

                for i in 0..dash_count {
                    let dash_y = y + (i as f32) * (dash_height + stroke);
                    let rect = Rect {
                        x: center_x - (stroke / 2.0),
                        y: dash_y,
                        width: stroke,
                        height: dash_height,
                    };
                    self.batches.add_rect(&rect, depth, &color);
                }
            }
            DrawableChar::VerticalHeavyDoubleDash => {
                // ┋ - Heavy double-dashed vertical line with 4 dashes
                let heavy_stroke = stroke * 1.8;
                let dash_count = 4;
                let total_space = dash_count - 1;
                let dash_height =
                    (line_height - (total_space as f32 * stroke)) / dash_count as f32;

                for i in 0..dash_count {
                    let dash_y = y + (i as f32) * (dash_height + stroke);
                    let rect = Rect {
                        x: center_x - heavy_stroke / 2.0,
                        y: dash_y,
                        width: heavy_stroke,
                        height: dash_height,
                    };
                    self.batches.add_rect(&rect, depth, &color);
                }
            }
            DrawableChar::VerticalLightTripleDash => {
                // ╎ - Light triple-dashed vertical line with 2 dashes
                let dash_count = 2;
                let total_space = dash_count - 1;
                let dash_height =
                    (line_height - (total_space as f32 * stroke)) / dash_count as f32;

                for i in 0..dash_count {
                    let dash_y = y + (i as f32) * (dash_height + stroke);
                    let rect = Rect {
                        x: center_x - (stroke / 2.0),
                        y: dash_y,
                        width: stroke,
                        height: dash_height,
                    };
                    self.batches.add_rect(&rect, depth, &color);
                }
            }
            DrawableChar::VerticalHeavyTripleDash => {
                // ╏ - Heavy triple-dashed vertical line with 2 dashes
                let heavy_stroke = stroke * 1.8;
                let dash_count = 2;
                let total_space = dash_count - 1;
                let dash_height =
                    (line_height - (total_space as f32 * stroke)) / dash_count as f32;

                for i in 0..dash_count {
                    let dash_y = y + (i as f32) * (dash_height + stroke);
                    let rect = Rect {
                        x: center_x - heavy_stroke / 2.0,
                        y: dash_y,
                        width: heavy_stroke,
                        height: dash_height,
                    };
                    self.batches.add_rect(&rect, depth, &color);
                }
            }
            // Braille patterns
            DrawableChar::BrailleBlank => {
                // No dots to draw
            }
            DrawableChar::Braille(braille_dots) => {
                // Use stroke as the dot size base
                let dot_size = (stroke * 1.2).round();

                // Calculate cell dimensions
                let cell_width = advance;
                let cell_height = line_height;

                // Define the grid - 2×4 layout
                let grid_columns = 2;
                let grid_rows = 4;

                // Calculate single cell dimensions
                let cell_width_unit = cell_width / grid_columns as f32;
                let cell_height_unit = cell_height / grid_rows as f32;

                // Function to calculate dot position based on grid coordinates
                let get_dot_position = |col: usize, row: usize| -> (f32, f32) {
                    let dot_x =
                        x + (col as f32 * cell_width_unit) + (cell_width_unit / 2.0)
                            - (dot_size / 2.0);
                    let dot_y =
                        y + (row as f32 * cell_height_unit) + (cell_height_unit / 2.0)
                            - (dot_size / 2.0);
                    (dot_x, dot_y)
                };

                // Dot 1 (top-left): position [0,0]
                if contains_braille_dot(&braille_dots, 1) {
                    let (dot_x, dot_y) = get_dot_position(0, 0);
                    let dot_rect = Rect {
                        x: dot_x,
                        y: dot_y,
                        width: dot_size,
                        height: dot_size,
                    };
                    self.batches.add_rect(&dot_rect, depth, &color);
                }

                // Dot 2 (middle-top-left): position [0,1]
                if contains_braille_dot(&braille_dots, 2) {
                    let (dot_x, dot_y) = get_dot_position(0, 1);
                    let dot_rect = Rect {
                        x: dot_x,
                        y: dot_y,
                        width: dot_size,
                        height: dot_size,
                    };
                    self.batches.add_rect(&dot_rect, depth, &color);
                }

                // Dot 3 (middle-bottom-left): position [0,2]
                if contains_braille_dot(&braille_dots, 3) {
                    let (dot_x, dot_y) = get_dot_position(0, 2);
                    let dot_rect = Rect {
                        x: dot_x,
                        y: dot_y,
                        width: dot_size,
                        height: dot_size,
                    };
                    self.batches.add_rect(&dot_rect, depth, &color);
                }

                // Dot 7 (bottom-left): position [0,3]
                if contains_braille_dot(&braille_dots, 7) {
                    let (dot_x, dot_y) = get_dot_position(0, 3);
                    let dot_rect = Rect {
                        x: dot_x,
                        y: dot_y,
                        width: dot_size,
                        height: dot_size,
                    };
                    self.batches.add_rect(&dot_rect, depth, &color);
                }

                // Right column
                // Dot 4 (top-right): position [1,0]
                if contains_braille_dot(&braille_dots, 4) {
                    let (dot_x, dot_y) = get_dot_position(1, 0);
                    let dot_rect = Rect {
                        x: dot_x,
                        y: dot_y,
                        width: dot_size,
                        height: dot_size,
                    };
                    self.batches.add_rect(&dot_rect, depth, &color);
                }

                // Dot 5 (middle-top-right): position [1,1]
                if contains_braille_dot(&braille_dots, 5) {
                    let (dot_x, dot_y) = get_dot_position(1, 1);
                    let dot_rect = Rect {
                        x: dot_x,
                        y: dot_y,
                        width: dot_size,
                        height: dot_size,
                    };
                    self.batches.add_rect(&dot_rect, depth, &color);
                }

                // Dot 6 (middle-bottom-right): position [1,2]
                if contains_braille_dot(&braille_dots, 6) {
                    let (dot_x, dot_y) = get_dot_position(1, 2);
                    let dot_rect = Rect {
                        x: dot_x,
                        y: dot_y,
                        width: dot_size,
                        height: dot_size,
                    };
                    self.batches.add_rect(&dot_rect, depth, &color);
                }

                // Dot 8 (bottom-right): position [1,3]
                if contains_braille_dot(&braille_dots, 8) {
                    let (dot_x, dot_y) = get_dot_position(1, 3);
                    let dot_rect = Rect {
                        x: dot_x,
                        y: dot_y,
                        width: dot_size,
                        height: dot_size,
                    };
                    self.batches.add_rect(&dot_rect, depth, &color);
                }
            }
        }
    }
}
