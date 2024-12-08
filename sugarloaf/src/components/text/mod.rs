pub mod glyph;

#[allow(dead_code)]
// From https://github.com/hecrj/wgpu_glyph
// #[deny(unused_results)]
mod builder;
mod pipeline;

/// A region of the screen.
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

use pipeline::{Instance, Pipeline};

pub use builder::GlyphBrushBuilder;
pub use glyph::ab_glyph;
pub use glyph::{
    BuiltInLineBreaker, Extra, FontId, GlyphCruncher, GlyphPositioner, HorizontalAlign,
    Layout, LineBreak, LineBreaker, OwnedSection, OwnedText, Section, SectionGeometry,
    SectionGlyph, SectionGlyphIter, SectionText, Text, VerticalAlign,
};

use crate::components::core::orthographic_projection;
use ab_glyph::{Font, Rect};
use core::hash::BuildHasher;
use std::borrow::Cow;

use glyph::{BrushAction, BrushError, DefaultSectionHasher};

/// Object allowing glyph drawing, containing cache state. Manages glyph positioning caching,
/// glyph draw caching & efficient GPU texture cache updating and re-sizing on demand.
///
/// Build using a [`GlyphBrushBuilder`](struct.GlyphBrushBuilder.html).
pub struct GlyphBrush<Depth, F = ab_glyph::FontArc, H = DefaultSectionHasher> {
    pipeline: Pipeline<Depth>,
    glyph_brush: glyph::GlyphBrush<Instance, Extra, F, H>,
}

impl<Depth, F: Font, H: BuildHasher> GlyphBrush<Depth, F, H> {
    #[inline]
    pub fn queue<'a, S>(&mut self, section: S)
    where
        S: Into<Cow<'a, Section<'a>>>,
    {
        self.glyph_brush.queue(section)
    }

    #[inline]
    pub fn keep_cached<'a, S>(&mut self, section: S)
    where
        S: Into<Cow<'a, Section<'a>>>,
    {
        self.glyph_brush.keep_cached(section)
    }

    #[inline]
    pub fn queue_custom_layout<'a, S, G>(&mut self, section: S, custom_layout: &G)
    where
        G: GlyphPositioner,
        S: Into<Cow<'a, Section<'a>>>,
    {
        self.glyph_brush.queue_custom_layout(section, custom_layout)
    }

    /// Returns the available fonts.
    ///
    /// The `FontId` corresponds to the index of the font data.
    // #[inline]
    pub fn fonts(&self) -> &[F] {
        self.glyph_brush.fonts()
    }

    /// Adds an additional font to the one(s) initially added on build.
    ///
    /// Returns a new [`FontId`](struct.FontId.html) to reference this font.
    pub fn add_font(&mut self, font: F) -> FontId {
        self.glyph_brush.add_font(font)
    }
}

impl<D, F, H> GlyphBrush<D, F, H>
where
    F: Font + Sync,
    H: BuildHasher,
{
    fn process_queued(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let pipeline = &mut self.pipeline;

        let mut brush_action;

        loop {
            brush_action = self.glyph_brush.process_queued(
                |rect, tex_data| {
                    let offset = [rect.min[0] as u16, rect.min[1] as u16];
                    let size = [rect.width() as u16, rect.height() as u16];

                    pipeline.update_cache(queue, offset, size, tex_data);
                },
                Instance::from_vertex,
            );

            match brush_action {
                Ok(_) => break,
                Err(BrushError::TextureTooSmall { suggested }) => {
                    // TODO: Obtain max texture dimensions using `wgpu`
                    // This is currently not possible I think. Ask!
                    // let max_image_dimension = 2048;
                    let max_image_dimension = 64;

                    let (new_width, new_height) = if (suggested.0 > max_image_dimension
                        || suggested.1 > max_image_dimension)
                        && (self.glyph_brush.texture_dimensions().0 < max_image_dimension
                            || self.glyph_brush.texture_dimensions().1
                                < max_image_dimension)
                    {
                        (max_image_dimension, max_image_dimension)
                    } else {
                        suggested
                    };

                    pipeline.increase_cache_size(device, new_width, new_height);
                    self.glyph_brush.resize_texture(new_width, new_height);
                }
            }
        }

        match brush_action.unwrap() {
            BrushAction::Draw(mut verts) => {
                self.pipeline.upload(device, queue, &mut verts);
            }
            BrushAction::ReDraw => {}
        };
    }
}

impl<F: Font + Sync, H: BuildHasher> GlyphBrush<(), F, H> {
    fn new(
        device: &wgpu::Device,
        filter_mode: wgpu::FilterMode,
        multisample: wgpu::MultisampleState,
        render_format: wgpu::TextureFormat,
        raw_builder: glyph::GlyphBrushBuilder<F, H>,
    ) -> Self {
        let glyph_brush = raw_builder.build();
        let (cache_width, cache_height) = glyph_brush.texture_dimensions();
        GlyphBrush {
            pipeline: Pipeline::<()>::new(
                device,
                filter_mode,
                multisample,
                render_format,
                cache_width,
                cache_height,
            ),
            glyph_brush,
        }
    }

    /// Draws all queued sections onto a render target.
    /// See [`queue`](struct.GlyphBrush.html#method.queue).
    ///
    /// It __does not__ submit the encoder command buffer to the device queue.
    ///
    /// Trims the cache, see [caching behaviour](#caching-behaviour).
    ///
    /// # Panics
    /// Panics if the provided `target` has a texture format that does not match
    /// the `render_format` provided on creation of the `GlyphBrush`.
    #[inline]
    pub fn render<'pass>(
        &'pass mut self,
        context: &mut crate::context::Context,
        rpass: &mut wgpu::RenderPass<'pass>,
    ) {
        let device = &context.device;
        let queue = &context.queue;
        self.draw_queued_with_transform(
            device,
            queue,
            rpass,
            orthographic_projection(context.size.width, context.size.height),
        );
    }

    /// Draws all queued sections onto a render target, applying a position
    /// transform (e.g. a projection).
    /// See [`queue`](struct.GlyphBrush.html#method.queue).
    ///
    /// It __does not__ submit the encoder command buffer to the device queue.
    ///
    /// Trims the cache, see [caching behaviour](#caching-behaviour).
    ///
    /// # Panics
    /// Panics if the provided `target` has a texture format that does not match
    /// the `render_format` provided on creation of the `GlyphBrush`.
    #[inline]
    pub fn draw_queued_with_transform<'pass>(
        &'pass mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rpass: &mut wgpu::RenderPass<'pass>,
        transform: [f32; 16],
    ) {
        self.process_queued(device, queue);
        self.pipeline.draw(queue, rpass, transform, None);
    }

    /// Draws all queued sections onto a render target, applying a position
    /// transform (e.g. a projection) and a scissoring region.
    /// See [`queue`](struct.GlyphBrush.html#method.queue).
    ///
    /// It __does not__ submit the encoder command buffer to the device queue.
    ///
    /// Trims the cache, see [caching behaviour](#caching-behaviour).
    ///
    /// # Panics
    /// Panics if the provided `target` has a texture format that does not match
    /// the `render_format` provided on creation of the `GlyphBrush`.
    #[inline]
    pub fn _draw_queued_with_transform_and_scissoring<'pass>(
        &'pass mut self,
        device: &wgpu::Device,
        queue: &mut wgpu::Queue,
        rpass: &mut wgpu::RenderPass<'pass>,
        transform: [f32; 16],
        region: Region,
    ) -> Result<(), String> {
        self.process_queued(device, queue);
        self.pipeline.draw(queue, rpass, transform, Some(region));

        Ok(())
    }
}

impl<F: Font + Sync, H: BuildHasher> GlyphBrush<wgpu::DepthStencilState, F, H> {
    fn new(
        device: &wgpu::Device,
        filter_mode: wgpu::FilterMode,
        multisample: wgpu::MultisampleState,
        render_format: wgpu::TextureFormat,
        raw_builder: glyph::GlyphBrushBuilder<F, H>,
    ) -> Self {
        let glyph_brush = raw_builder.build();
        let (cache_width, cache_height) = glyph_brush.texture_dimensions();
        GlyphBrush {
            pipeline: Pipeline::<wgpu::DepthStencilState>::new(
                device,
                filter_mode,
                multisample,
                render_format,
                cache_width,
                cache_height,
            ),
            glyph_brush,
        }
    }

    /// Draws all queued sections onto a render target.
    /// See [`queue`](struct.GlyphBrush.html#method.queue).
    ///
    /// It __does not__ submit the encoder command buffer to the device queue.
    ///
    /// Trims the cache, see [caching behaviour](#caching-behaviour).
    ///
    /// # Panics
    /// Panics if the provided `target` has a texture format that does not match
    /// the `render_format` provided on creation of the `GlyphBrush`.
    #[inline]
    pub fn _draw_queued<'pass>(
        &'pass mut self,
        device: &wgpu::Device,
        queue: &mut wgpu::Queue,
        rpass: &mut wgpu::RenderPass<'pass>,
        w_h: (f32, f32),
    ) -> Result<(), String> {
        self.draw_queued_with_transform(
            device,
            queue,
            rpass,
            orthographic_projection(w_h.0, w_h.1),
        )
    }

    /// Draws all queued sections onto a render target, applying a position
    /// transform (e.g. a projection).
    /// See [`queue`](struct.GlyphBrush.html#method.queue).
    ///
    /// It __does not__ submit the encoder command buffer to the device queue.
    ///
    /// Trims the cache, see [caching behaviour](#caching-behaviour).
    ///
    /// # Panics
    /// Panics if the provided `target` has a texture format that does not match
    /// the `render_format` provided on creation of the `GlyphBrush`.
    #[inline]
    #[allow(dead_code)]
    pub fn draw_queued_with_transform<'pass>(
        &'pass mut self,
        device: &wgpu::Device,
        queue: &mut wgpu::Queue,
        rpass: &mut wgpu::RenderPass<'pass>,
        transform: [f32; 16],
    ) -> Result<(), String> {
        self.process_queued(device, queue);
        self.pipeline.draw((queue, rpass), transform, None);

        Ok(())
    }

    /// Draws all queued sections onto a render target, applying a position
    /// transform (e.g. a projection) and a scissoring region.
    /// See [`queue`](struct.GlyphBrush.html#method.queue).
    ///
    /// It __does not__ submit the encoder command buffer to the device queue.
    ///
    /// Trims the cache, see [caching behaviour](#caching-behaviour).
    ///
    /// # Panics
    /// Panics if the provided `target` has a texture format that does not match
    /// the `render_format` provided on creation of the `GlyphBrush`.
    #[inline]
    pub fn _draw_queued_with_transform_and_scissoring<'pass>(
        &'pass mut self,
        // config: (device, staging_belt, encoder, target),
        config: (
            &wgpu::Device,
            &mut wgpu::Queue,
            &mut wgpu::RenderPass<'pass>,
        ),
        transform: [f32; 16],
        region: Region,
    ) -> Result<(), String> {
        let (device, queue, encoder) = config;

        self.process_queued(device, queue);

        self.pipeline
            .draw((queue, encoder), transform, Some(region));

        Ok(())
    }
}

impl<D, F: Font, H: BuildHasher> GlyphCruncher<F> for GlyphBrush<D, F, H> {
    #[inline]
    fn glyphs_custom_layout<'a, 'b, S, L>(
        &'b mut self,
        section: S,
        custom_layout: &L,
    ) -> SectionGlyphIter<'b>
    where
        L: GlyphPositioner + std::hash::Hash,
        S: Into<Cow<'a, Section<'a>>>,
    {
        self.glyph_brush
            .glyphs_custom_layout(section, custom_layout)
    }

    #[inline]
    fn fonts(&self) -> &[F] {
        self.glyph_brush.fonts()
    }

    #[inline]
    fn glyph_bounds_custom_layout<'a, S, L>(
        &mut self,
        section: S,
        custom_layout: &L,
    ) -> Option<Rect>
    where
        L: GlyphPositioner + std::hash::Hash,
        S: Into<Cow<'a, Section<'a>>>,
    {
        self.glyph_brush
            .glyph_bounds_custom_layout(section, custom_layout)
    }
}
