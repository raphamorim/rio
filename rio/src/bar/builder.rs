/// Builder for a [`GlyphBrush`](struct.GlyphBrush.html).
pub struct BarBuilder<D, F, H = DefaultSectionHasher> {
    inner: glyph_brush::GlyphBrushBuilder<F, H>,
    texture_filter_method: wgpu::FilterMode,
    depth: D,
}
