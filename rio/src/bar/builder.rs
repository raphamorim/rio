/// Builder for a [`GlyphBrush`](struct.GlyphBrush.html).
pub struct BarBuilder<D, F, H = DefaultSectionHasher> {
    inner: glyph_brush::GlyphBrushBuilder<F, H>,
    texture_filter_method: wgpu::FilterMode,
    depth: D,
}

// let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
//         label: Some("Vertex Buffer"),
//         contents: bytemuck::cast_slice(VERTICES),
//         usage: wgpu::BufferUsages::VERTEX,
//     });

//     let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
//         label: Some("Index Buffer"),
//         contents: bytemuck::cast_slice(INDICES),
//         usage: wgpu::BufferUsages::INDEX,
//     });
//     let num_indices = INDICES.len() as u32;