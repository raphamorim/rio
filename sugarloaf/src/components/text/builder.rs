use core::hash::BuildHasher;

use crate::components::text::glyph::ab_glyph::Font;
// use crate::components::text::glyph::delegate_glyph_brush_builder_fns;
use crate::components::text::glyph::DefaultSectionHasher;

use super::GlyphBrush;

/// Builder for a [`GlyphBrush`](struct.GlyphBrush.html).
pub struct GlyphBrushBuilder<D, F, H = DefaultSectionHasher> {
    inner: crate::components::text::glyph::GlyphBrushBuilder<F, H>,
    texture_filter_method: wgpu::FilterMode,
    multisample_state: wgpu::MultisampleState,
    depth: D,
}

impl<F, H> From<crate::components::text::glyph::GlyphBrushBuilder<F, H>>
    for GlyphBrushBuilder<(), F, H>
{
    fn from(inner: crate::components::text::glyph::GlyphBrushBuilder<F, H>) -> Self {
        GlyphBrushBuilder {
            inner,
            texture_filter_method: wgpu::FilterMode::Linear,
            multisample_state: wgpu::MultisampleState::default(),
            depth: (),
        }
    }
}

impl GlyphBrushBuilder<(), ()> {
    /// Specifies the default font used to render glyphs.
    /// Referenced with `FontId(0)`, which is default.
    #[inline]
    pub fn using_font<F: Font>(font: F) -> GlyphBrushBuilder<(), F> {
        Self::using_fonts(vec![font])
    }

    pub fn using_fonts<F: Font>(fonts: Vec<F>) -> GlyphBrushBuilder<(), F> {
        GlyphBrushBuilder {
            inner: crate::components::text::glyph::GlyphBrushBuilder::using_fonts(fonts),
            texture_filter_method: wgpu::FilterMode::Linear,
            multisample_state: wgpu::MultisampleState::default(),
            depth: (),
        }
    }

    // pub fn using_scaled_fonts<F: Font>(fonts: Vec<PxScaleFont<FontArc>>) -> GlyphBrushBuilder<(), F> {
    //     GlyphBrushBuilder {
    //         inner: crate::components::text::glyph::GlyphBrushBuilder::using_fonts(fonts),
    //         texture_filter_method: wgpu::FilterMode::Linear,
    //         multisample_state: wgpu::MultisampleState::default(),
    //         depth: (),
    //     }
    // }
}

impl<F: Font, D, H: BuildHasher> GlyphBrushBuilder<D, F, H> {
    // delegate_glyph_brush_builder_fns!(inner);

    /// When multiple CPU cores are available spread rasterization work across
    /// all cores.
    ///
    /// Significantly reduces worst case latency in multicore environments.
    ///
    /// By default, this feature is __enabled__.
    ///
    /// # Platform-specific behaviour
    ///
    /// This option has no effect on wasm32.
    // pub fn draw_cache_multithread(mut self, multithread: bool) -> Self {
    //     self.inner.draw_cache_builder =
    //         self.inner.draw_cache_builder.multithread(multithread);
    //     self
    // }
    /// Sets the texture filtering method.
    // pub fn texture_filter_method(mut self, filter_method: wgpu::FilterMode) -> Self {
    //     self.texture_filter_method = filter_method;
    //     self
    // }
    pub fn multisample_state(
        mut self,
        multisample_state: wgpu::MultisampleState,
    ) -> Self {
        self.multisample_state = multisample_state;
        self
    }

    /// Sets the section hasher. `GlyphBrush` cannot handle absolute section
    /// hash collisions so use a good hash algorithm.
    ///
    /// This hasher is used to distinguish sections, rather than for hashmap
    /// internal use.
    ///
    /// Defaults to [xxHash](https://docs.rs/twox-hash).
    // pub fn section_hasher<T: BuildHasher>(
    //     self,
    //     section_hasher: T,
    // ) -> GlyphBrushBuilder<D, F, T> {
    //     GlyphBrushBuilder {
    //         inner: self.inner.section_hasher(section_hasher),
    //         texture_filter_method: self.texture_filter_method,
    //         depth: self.depth,
    //     }
    // }
    /// Sets the depth stencil.
    pub fn depth_stencil_state(
        self,
        depth_stencil_state: wgpu::DepthStencilState,
    ) -> GlyphBrushBuilder<wgpu::DepthStencilState, F, H> {
        GlyphBrushBuilder {
            inner: self.inner,
            texture_filter_method: self.texture_filter_method,
            multisample_state: self.multisample_state,
            depth: depth_stencil_state,
        }
    }
}

impl<F: Font + Sync, H: BuildHasher> GlyphBrushBuilder<(), F, H> {
    /// Builds a `GlyphBrush` using the given `wgpu::Device` that can render
    /// text for texture views with the given `render_format`.
    pub fn build(
        self,
        device: &wgpu::Device,
        render_format: wgpu::TextureFormat,
    ) -> GlyphBrush<(), F, H> {
        GlyphBrush::<(), F, H>::new(
            device,
            self.texture_filter_method,
            self.multisample_state,
            render_format,
            self.inner,
        )
    }
}

impl<F: Font + Sync, H: BuildHasher> GlyphBrushBuilder<wgpu::DepthStencilState, F, H> {
    /// Builds a `GlyphBrush` using the given `wgpu::Device` that can render
    /// text for texture views with the given `render_format`.
    pub fn build(
        self,
        device: &wgpu::Device,
        render_format: wgpu::TextureFormat,
    ) -> GlyphBrush<wgpu::DepthStencilState, F, H> {
        GlyphBrush::<wgpu::DepthStencilState, F, H>::new(
            device,
            self.texture_filter_method,
            self.multisample_state,
            render_format,
            self.inner,
        )
    }
}
