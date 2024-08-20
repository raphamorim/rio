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
pub use crate::components::rich_text::batch::{
    // Command, DisplayList, Pipeline, Rect, Vertex,
    Command,
    DisplayList,
    Rect,
    Vertex,
};
use crate::components::rich_text::image_cache::glyph::GlyphCacheSession;
use crate::components::rich_text::image_cache::glyph::GlyphEntry;
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
use crate::layout::{FragmentStyleDecoration, Line, SugarDimensions, UnderlineShape};
use crate::SugarCursor;
use rustc_hash::FxHashMap;
use std::borrow::Borrow;

pub struct ComposedRect {
    coords: [f32; 4],
    color: [f32; 4],
    has_alpha: bool,
    image: TextureId,
}

pub enum Instruction {
    Image(ComposedRect),
    Mask(ComposedRect),
}

pub enum InstructionCallback {
    Background([f32; 4]),
    CaretCursor([f32; 4]),
    BlockCursor([f32; 4]),
}

#[derive(Default)]
pub struct CachedRunInstructions {
    pub instructions: Vec<Instruction>,
    pub desc_ink: Option<(f32, f32)>,
    pub entry: Option<GlyphEntry>,
}

#[derive(Default)]
pub struct CachedRunUnderline {
    enabled: bool,
    offset: i32,
    size: f32,
    color: [f32; 4],
    is_doubled: bool,
    shape: UnderlineShape,
}

pub struct CachedRun {
    pub glyphs_ids: Vec<u16>,
    instruction_set: FxHashMap<usize, CachedRunInstructions>,
    instruction_set_callback: Vec<InstructionCallback>,
    underline: CachedRunUnderline,
    char_width: f32,
}

impl CachedRun {
    pub fn new(char_width: f32) -> Self {
        Self {
            underline: CachedRunUnderline::default(),
            char_width,
            glyphs_ids: Vec::new(),
            instruction_set_callback: Vec::new(),
            instruction_set: FxHashMap::default(),
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
    pub fn draw_cached_run(
        &mut self,
        cache_line: &Vec<CachedRun>,
        px: f32,
        py: f32,
        depth: f32,
        rect: SugarDimensions,
        line: Line,
    ) {
        let mut px = px;
        let subpx_bias = (0.125, 0.);

        let line_height = line.ascent() + line.descent() + line.leading();
        let topline = py - line.ascent();

        for cached_run in cache_line {
            let mut glyphs = Vec::new();
            let run_x = px;

            for glyph in &cached_run.glyphs_ids {
                let x = px;
                // let y = py - glyph.y;
                let y = py;
                // px += glyph.advance;
                px += rect.width * cached_run.char_width;
                glyphs.push(Glyph { id: *glyph, x, y });
            }

            let advance = px - run_x;

            if cached_run.underline.enabled {
                self.intercepts.clear();
            }

            let mut index = 0;

            for glyph in &glyphs {
                if let Some(set) = cached_run.instruction_set.get(&index) {
                    if set.entry.is_none() {
                        continue;
                    }
                    let entry = set.entry.unwrap();

                    for instruction in &set.instructions {
                        let gx = (glyph.x + subpx_bias.0).floor() + entry.left as f32;
                        let gy = (glyph.y + subpx_bias.1).floor() - entry.top as f32;

                        match instruction {
                            Instruction::Image(data) => {
                                self.batches.add_image_rect(
                                    &Rect::new(
                                        gx,
                                        gy,
                                        entry.width as f32,
                                        entry.height as f32,
                                    ),
                                    depth,
                                    &data.color,
                                    &data.coords,
                                    data.image,
                                    data.has_alpha,
                                );
                            }
                            Instruction::Mask(data) => {
                                self.batches.add_mask_rect(
                                    &Rect::new(
                                        gx,
                                        gy,
                                        entry.width as f32,
                                        entry.height as f32,
                                    ),
                                    depth,
                                    &data.color,
                                    &data.coords,
                                    data.image,
                                    data.has_alpha,
                                );
                            }
                        }

                        if let Some(desc_ink) = set.desc_ink {
                            self.intercepts.push((desc_ink.0 + gx, desc_ink.1 + gx));
                        }
                    }
                }

                index += 1;
            }

            for instruction_callback in &cached_run.instruction_set_callback {
                match instruction_callback {
                    InstructionCallback::Background(color) => {
                        self.batches.add_rect(
                            &Rect::new(run_x, topline, advance, line_height),
                            depth,
                            color,
                        );
                    }
                    InstructionCallback::BlockCursor(cursor_color) => {
                        self.batches.add_rect(
                            &Rect::new(run_x, topline, advance, line_height),
                            depth,
                            cursor_color,
                        );
                    }
                    InstructionCallback::CaretCursor(cursor_color) => {
                        self.batches.add_rect(
                            &Rect::new(run_x, topline, 3.0, line_height),
                            depth,
                            cursor_color,
                        );
                    }
                }
            }

            self.draw_underline(
                &cached_run.underline,
                run_x,
                advance,
                py,
                depth,
                line_height,
            );
        }
    }

    /// Draws a text run.
    #[inline]
    pub fn draw_run<I>(
        &mut self,
        session: &mut GlyphCacheSession,
        rect: impl Into<Rect>,
        depth: f32,
        style: &TextRunStyle,
        glyphs: I,
        cached_run: &mut CachedRun,
    ) where
        I: Iterator,
        I::Item: Borrow<Glyph>,
    {
        let rect = rect.into();

        match style.decoration {
            Some(FragmentStyleDecoration::Underline(info)) => {
                cached_run.underline = CachedRunUnderline {
                    enabled: true,
                    offset: info.offset.round() as i32,
                    size: info.size,
                    color: style.decoration_color.unwrap_or(style.color),
                    is_doubled: info.is_doubled,
                    shape: info.shape,
                }
            }
            Some(FragmentStyleDecoration::Strikethrough) => {
                cached_run.underline = CachedRunUnderline {
                    enabled: true,
                    offset: (style.line_height / 3.5).round() as i32,
                    size: 3.0,
                    color: style.decoration_color.unwrap_or(style.color),
                    is_doubled: false,
                    shape: UnderlineShape::Regular,
                }
            }
            _ => {}
        };

        if cached_run.underline.enabled {
            self.intercepts.clear();
        }

        let subpx_bias = (0.125, 0.);
        let color = style.color;

        for (glyph_acc, g) in glyphs.enumerate() {
            let mut cached_run_instructions = CachedRunInstructions::default();
            let glyph = g.borrow();
            let entry = session.get(glyph.id);
            cached_run_instructions.entry = entry;
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
                            img.texture_id,
                            entry.image.has_alpha(),
                        );
                        cached_run_instructions
                            .instructions
                            .push(Instruction::Image(ComposedRect {
                                color,
                                coords,
                                image: img.texture_id,
                                has_alpha: entry.image.has_alpha(),
                            }));
                    } else {
                        let coords = [img.min.0, img.min.1, img.max.0, img.max.1];
                        self.batches.add_mask_rect(
                            &Rect::new(gx, gy, entry.width as f32, entry.height as f32),
                            depth,
                            &color,
                            &coords,
                            img.texture_id,
                            true,
                        );
                        cached_run_instructions.instructions.push(Instruction::Mask(
                            ComposedRect {
                                color,
                                coords,
                                image: img.texture_id,
                                has_alpha: true,
                            },
                        ));
                    }

                    if cached_run.underline.enabled
                        && entry.top - cached_run.underline.offset < entry.height as i32
                    {
                        if let Some(mut desc_ink) = entry.desc.range() {
                            cached_run_instructions.desc_ink = Some(desc_ink);

                            desc_ink.0 += gx;
                            desc_ink.1 += gx;
                            self.intercepts.push(desc_ink);
                        }
                    }
                }
            }

            cached_run
                .instruction_set
                .insert(glyph_acc, cached_run_instructions);
        }

        if let Some(bg_color) = style.background_color {
            self.batches.add_rect(
                &Rect::new(rect.x, style.topline, rect.width, style.line_height),
                depth,
                &bg_color,
            );
            cached_run
                .instruction_set_callback
                .push(InstructionCallback::Background(bg_color));
        }

        match style.cursor {
            SugarCursor::Block(cursor_color) => {
                self.batches.add_rect(
                    &Rect::new(rect.x, style.topline, rect.width, style.line_height),
                    depth,
                    &cursor_color,
                );
                cached_run
                    .instruction_set_callback
                    .push(InstructionCallback::BlockCursor(cursor_color));
            }
            SugarCursor::Caret(cursor_color) => {
                self.batches.add_rect(
                    &Rect::new(rect.x, style.topline, 3.0, style.line_height),
                    depth,
                    &cursor_color,
                );
                cached_run
                    .instruction_set_callback
                    .push(InstructionCallback::CaretCursor(cursor_color));
            }
            _ => {}
        }

        self.draw_underline(
            &cached_run.underline,
            rect.x,
            rect.width,
            style.baseline,
            depth,
            style.line_height,
        );

        // let duration = start.elapsed();
        // println!(" - draw_glyphs() is: {:?}", duration);
    }

    #[inline]
    fn draw_underline(
        &mut self,
        underline: &CachedRunUnderline,
        x: f32,
        advance: f32,
        baseline: f32,
        depth: f32,
        line_height: f32,
    ) {
        if underline.enabled {
            for range in self.intercepts.iter_mut() {
                range.0 -= 1.;
                range.1 += 1.;
            }
            let mut ux = x;
            let uy = baseline - underline.offset as f32;
            for range in self.intercepts.iter() {
                if ux < range.0 {
                    match underline.shape {
                        UnderlineShape::Regular => {
                            self.batches.add_rect(
                                &Rect::new(ux, uy, range.0 - ux, underline.size),
                                depth,
                                &underline.color,
                            );

                            if underline.is_doubled {
                                self.batches.add_rect(
                                    &Rect::new(
                                        ux,
                                        baseline,
                                        range.0 - ux,
                                        underline.size,
                                    ),
                                    depth,
                                    &underline.color,
                                );
                            }
                        }
                        UnderlineShape::Dashed => {
                            let mut start = ux;
                            let end = range.0 - ux;
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
                            let end = range.0 - ux;
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
                            let style_line_height = (line_height / 12.).min(3.0);
                            let offset = style_line_height * 1.5;

                            let mut curly_width = ux;
                            let end = range.0 - ux;
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
                                        uy - ((dot_bottom_offset - offset)
                                            + underline.offset as f32),
                                        rect_width,
                                        style_line_height,
                                    ),
                                    depth,
                                    &underline.color,
                                );

                                curly_width += rect_width;
                            }
                        }
                    }
                }
                ux = range.1;
            }
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
                                &Rect::new(ux, baseline, end - ux, underline.size),
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
                        let style_line_height = (line_height / 12.).min(3.0);
                        let offset = style_line_height * 1.5;

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
                                    uy - ((dot_bottom_offset - offset)
                                        + underline.offset as f32),
                                    rect_width,
                                    style_line_height,
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
