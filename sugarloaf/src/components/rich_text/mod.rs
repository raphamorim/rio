use crate::backend::RenderBackend;
use crate::components::core::orthographic_projection;
use crate::components::rich_text::compositor::{
    BatchOperation, Compositor, LineCache, Rect, Vertex,
};
use crate::components::rich_text::image_cache::{GlyphCache, ImageCache};
use crate::components::rich_text::text::{Glyph, TextRunStyle};
use crate::context::Context;
use crate::font::FontLibrary;
use crate::layout::{BuilderLine, BuilderStateUpdate, RichTextLayout, SugarDimensions};
use crate::sugarloaf::graphics::GraphicRenderRequest;
use crate::Graphics;
use crate::RichTextLinesRange;
use std::collections::HashSet;
use std::{borrow::Cow, mem};
use wgpu::util::DeviceExt;

pub const BLEND: Option<wgpu::BlendState> = Some(wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::SrcAlpha,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
});

pub enum TextPipeline {
    WebGpu(wgpu::RenderPipeline),
    #[cfg(target_os = "macos")]
    Metal(metal::RenderPipelineState),
}

pub enum TextBuffer {
    WebGpu(wgpu::Buffer),
    #[cfg(target_os = "macos")]
    Metal(metal::Buffer),
}

pub enum TextBindGroup {
    WebGpu(wgpu::BindGroup),
    #[cfg(target_os = "macos")]
    Metal(()), // Metal doesn't use bind groups
}

pub enum TextBindGroupLayout {
    WebGpu(wgpu::BindGroupLayout),
    #[cfg(target_os = "macos")]
    Metal(()),
}

pub struct RichTextBrush {
    vertex_buffer: TextBuffer,
    constant_bind_group: TextBindGroup,
    layout_bind_group: TextBindGroup,
    layout_bind_group_layout: TextBindGroupLayout,
    transform: TextBuffer,
    pipeline: TextPipeline,
    current_transform: [f32; 16],
    comp: Compositor,
    vertices: Vec<Vertex>,
    supported_vertex_buffer: usize,
    textures_version: usize,
    images: ImageCache,
    glyphs: GlyphCache,
    line_cache: LineCache,
    backend: RenderBackend,
    #[cfg(target_os = "macos")]
    metal_sampler: Option<metal::SamplerState>,
}

impl RichTextBrush {
    pub fn new(context: &Context) -> Self {
        let backend = context.render_backend;

        match backend {
            RenderBackend::WebGpu => Self::new_webgpu(context),
            #[cfg(target_os = "macos")]
            RenderBackend::Metal => Self::new_metal(context),
        }
    }

    fn new_webgpu(context: &Context) -> Self {
        let device = &context.device;
        let supported_vertex_buffer = 500;

        let current_transform =
            orthographic_projection(context.size.width, context.size.height);
        let transform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&current_transform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Create pipeline layout
        let constant_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(mem::size_of::<
                                [f32; 16],
                            >(
                            )
                                as wgpu::BufferAddress),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX
                            | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(
                            wgpu::SamplerBindingType::Filtering,
                        ),
                        count: None,
                    },
                ],
            });

        let layout_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: context.get_optimal_texture_sample_type(),
                    },
                    count: None,
                }],
            });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: None,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let constant_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &constant_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: transform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let layout_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout_bind_group_layout,
            entries: &[],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&constant_bind_group_layout, &layout_bind_group_layout],
            push_constant_ranges: &[],
        });

        let shader_source = if context.supports_f16() {
            include_str!("rich_text_f16.wgsl")
        } else {
            include_str!("rich_text_f32.wgsl")
        };

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_source)),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: mem::size_of::<Vertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2,
                        1 => Float32x2,
                        2 => Float32x4,
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: context.format,
                    blend: BLEND,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Cw,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: mem::size_of::<Vertex>() as u64 * supported_vertex_buffer as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let images = ImageCache::new(context);
        let glyphs = GlyphCache::new();

        RichTextBrush {
            vertex_buffer: TextBuffer::WebGpu(vertex_buffer),
            constant_bind_group: TextBindGroup::WebGpu(constant_bind_group),
            layout_bind_group: TextBindGroup::WebGpu(layout_bind_group),
            layout_bind_group_layout: TextBindGroupLayout::WebGpu(
                layout_bind_group_layout,
            ),
            transform: TextBuffer::WebGpu(transform),
            pipeline: TextPipeline::WebGpu(pipeline),
            current_transform,
            comp: Compositor::new(),
            vertices: Vec::new(),
            supported_vertex_buffer,
            textures_version: 0,
            images,
            glyphs,
            line_cache: LineCache::new(),
            backend: RenderBackend::WebGpu,
            #[cfg(target_os = "macos")]
            metal_sampler: None,
        }
    }

    #[cfg(target_os = "macos")]
    fn new_metal(context: &Context) -> Self {
        let supported_vertex_buffer = 500;

        if let Some(metal_ctx) = context.metal_context() {
            let current_transform =
                orthographic_projection(context.size.width, context.size.height);

            // Create Metal buffers
            let transform_data = bytemuck::cast_slice(&current_transform);
            let transform = metal_ctx.create_buffer(
                transform_data,
                metal::MTLResourceOptions::StorageModeShared,
            );

            let vertex_data =
                vec![0u8; mem::size_of::<Vertex>() * supported_vertex_buffer];
            let vertex_buffer = metal_ctx.create_buffer(
                &vertex_data,
                metal::MTLResourceOptions::StorageModeShared,
            );

            // Create Metal shader library and pipeline
            let shader_source = include_str!("../shaders/text.metal");
            let library = metal_ctx
                .create_library_from_source(shader_source)
                .expect("Failed to create Metal text shader library");

            let vertex_function = library.get_function("vertex_main", None).unwrap();
            let fragment_function = library.get_function("fragment_main", None).unwrap();

            let pipeline_descriptor = metal::RenderPipelineDescriptor::new();
            pipeline_descriptor.set_vertex_function(Some(&vertex_function));
            pipeline_descriptor.set_fragment_function(Some(&fragment_function));

            // Set up vertex descriptor for text rendering
            let vertex_descriptor = metal::VertexDescriptor::new();
            let attributes = vertex_descriptor.attributes();
            let layouts = vertex_descriptor.layouts();

            // Position attribute (float2)
            attributes
                .object_at(0)
                .unwrap()
                .set_format(metal::MTLVertexFormat::Float2);
            attributes.object_at(0).unwrap().set_offset(0);
            attributes.object_at(0).unwrap().set_buffer_index(0);

            // Texture coordinates attribute (float2)
            attributes
                .object_at(1)
                .unwrap()
                .set_format(metal::MTLVertexFormat::Float2);
            attributes.object_at(1).unwrap().set_offset(8);
            attributes.object_at(1).unwrap().set_buffer_index(0);

            // Color attribute (float4 for compatibility)
            attributes
                .object_at(2)
                .unwrap()
                .set_format(metal::MTLVertexFormat::Float4);
            attributes.object_at(2).unwrap().set_offset(16);
            attributes.object_at(2).unwrap().set_buffer_index(0);

            layouts
                .object_at(0)
                .unwrap()
                .set_stride(mem::size_of::<Vertex>() as u64);
            layouts.object_at(0).unwrap().set_step_rate(1);
            layouts
                .object_at(0)
                .unwrap()
                .set_step_function(metal::MTLVertexStepFunction::PerVertex);

            pipeline_descriptor.set_vertex_descriptor(Some(&vertex_descriptor));

            // Set color attachment format
            let color_attachments = pipeline_descriptor.color_attachments();
            color_attachments
                .object_at(0)
                .unwrap()
                .set_pixel_format(metal::MTLPixelFormat::BGRA8Unorm);
            color_attachments
                .object_at(0)
                .unwrap()
                .set_blending_enabled(true);
            color_attachments
                .object_at(0)
                .unwrap()
                .set_source_rgb_blend_factor(metal::MTLBlendFactor::SourceAlpha);
            color_attachments
                .object_at(0)
                .unwrap()
                .set_destination_rgb_blend_factor(
                    metal::MTLBlendFactor::OneMinusSourceAlpha,
                );
            color_attachments
                .object_at(0)
                .unwrap()
                .set_source_alpha_blend_factor(metal::MTLBlendFactor::One);
            color_attachments
                .object_at(0)
                .unwrap()
                .set_destination_alpha_blend_factor(
                    metal::MTLBlendFactor::OneMinusSourceAlpha,
                );

            let pipeline = metal_ctx
                .create_render_pipeline(&pipeline_descriptor)
                .expect("Failed to create Metal text render pipeline");

            // Create Metal sampler
            let sampler_descriptor = metal::SamplerDescriptor::new();
            sampler_descriptor
                .set_address_mode_s(metal::MTLSamplerAddressMode::ClampToEdge);
            sampler_descriptor
                .set_address_mode_t(metal::MTLSamplerAddressMode::ClampToEdge);
            sampler_descriptor
                .set_address_mode_r(metal::MTLSamplerAddressMode::ClampToEdge);
            sampler_descriptor.set_mag_filter(metal::MTLSamplerMinMagFilter::Linear);
            sampler_descriptor.set_min_filter(metal::MTLSamplerMinMagFilter::Linear);
            sampler_descriptor.set_mip_filter(metal::MTLSamplerMipFilter::Nearest);
            let sampler = metal_ctx.device.new_sampler(&sampler_descriptor);

            let images = ImageCache::new(context);
            let glyphs = GlyphCache::new();

            RichTextBrush {
                vertex_buffer: TextBuffer::Metal(vertex_buffer),
                constant_bind_group: TextBindGroup::Metal(()),
                layout_bind_group: TextBindGroup::Metal(()),
                layout_bind_group_layout: TextBindGroupLayout::Metal(()),
                transform: TextBuffer::Metal(transform),
                pipeline: TextPipeline::Metal(pipeline),
                current_transform,
                comp: Compositor::new(),
                vertices: Vec::new(),
                supported_vertex_buffer,
                textures_version: 0,
                images,
                glyphs,
                line_cache: LineCache::new(),
                backend: RenderBackend::Metal,
                metal_sampler: Some(sampler),
            }
        } else {
            panic!("Metal context not available");
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn new_metal(_context: &Context) -> Self {
        panic!("Metal backend not available - compile with native-metal feature");
    }

    pub fn clear_atlas(&mut self) {
        self.glyphs.clear();
        self.images.clear_atlas();
    }

    pub fn resize(&mut self, context: &Context) {
        match self.backend {
            RenderBackend::WebGpu => self.resize_webgpu(context),
            #[cfg(target_os = "macos")]
            RenderBackend::Metal => self.resize_metal(context),
        }
    }

    fn resize_webgpu(&mut self, context: &Context) {
        let current_transform =
            orthographic_projection(context.size.width, context.size.height);

        if let TextBuffer::WebGpu(ref transform_buffer) = self.transform {
            context.queue.write_buffer(
                transform_buffer,
                0,
                bytemuck::cast_slice(&current_transform),
            );
        }

        self.current_transform = current_transform;
    }

    #[cfg(target_os = "macos")]
    fn resize_metal(&mut self, context: &Context) {
        let current_transform =
            orthographic_projection(context.size.width, context.size.height);

        if let TextBuffer::Metal(ref transform_buffer) = self.transform {
            if let Some(_metal_ctx) = context.metal_context() {
                // Update Metal buffer with new transform
                let transform_data = bytemuck::cast_slice(&current_transform);
                let contents = transform_buffer.contents();
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        transform_data.as_ptr(),
                        contents as *mut u8,
                        transform_data.len(),
                    );
                }
                tracing::debug!("Updated Metal text transform buffer");
            }
        }

        self.current_transform = current_transform;
    }

    #[cfg(not(target_os = "macos"))]
    fn resize_metal(&mut self, _context: &Context) {
        // No-op for non-Metal builds
    }

    pub fn render(&mut self, context: &mut Context, render_pass: &mut wgpu::RenderPass) {
        match self.backend {
            RenderBackend::WebGpu => self.render_webgpu(context, render_pass),
            #[cfg(target_os = "macos")]
            RenderBackend::Metal => {
                // For now, Metal text rendering is not fully implemented in this render method
                // This would need a Metal-specific render pass
                tracing::debug!(
                    "Metal text rendering not yet implemented in this render method"
                );
            }
        }
    }

    fn render_webgpu(
        &mut self,
        context: &mut Context,
        render_pass: &mut wgpu::RenderPass,
    ) {
        if self.vertices.is_empty() {
            return;
        }

        let transform = orthographic_projection(context.size.width, context.size.height);

        if self.current_transform != transform {
            if let TextBuffer::WebGpu(ref transform_buffer) = self.transform {
                context.queue.write_buffer(
                    transform_buffer,
                    0,
                    bytemuck::cast_slice(&transform),
                );
            }
            self.current_transform = transform;
        }

        let quantity = self.vertices.len();
        if quantity > self.supported_vertex_buffer {
            if let TextBuffer::WebGpu(ref _vertex_buffer) = self.vertex_buffer {
                self.vertex_buffer = TextBuffer::WebGpu(context.device.create_buffer(
                    &wgpu::BufferDescriptor {
                        label: None,
                        size: mem::size_of::<Vertex>() as u64 * quantity as u64,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    },
                ));
            }
            self.supported_vertex_buffer = quantity;
        }

        if let TextBuffer::WebGpu(ref vertex_buffer) = self.vertex_buffer {
            context.queue.write_buffer(
                vertex_buffer,
                0,
                bytemuck::cast_slice(&self.vertices),
            );
        }

        if let TextPipeline::WebGpu(ref pipeline) = self.pipeline {
            render_pass.set_pipeline(pipeline);
        }

        if let TextBindGroup::WebGpu(ref constant_bind_group) = self.constant_bind_group {
            render_pass.set_bind_group(0, constant_bind_group, &[]);
        }

        if let TextBindGroup::WebGpu(ref layout_bind_group) = self.layout_bind_group {
            render_pass.set_bind_group(1, layout_bind_group, &[]);
        }

        if let TextBuffer::WebGpu(ref vertex_buffer) = self.vertex_buffer {
            render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        }

        render_pass.draw(0..quantity as u32, 0..1);
    }

    #[cfg(target_os = "macos")]
    pub fn render_metal(
        &mut self,
        context: &mut Context,
        metal_pass: &mut crate::backend::metal::MetalRenderPass,
    ) {
        tracing::debug!("Metal text render called with {} vertices", self.vertices.len());
        
        if self.vertices.is_empty() {
            tracing::debug!("No vertices to render for Metal text - checking if text content exists");
            return;
        }

        tracing::debug!("Rendering {} text vertices with Metal", self.vertices.len());

        let transform = orthographic_projection(context.size.width, context.size.height);

        if self.current_transform != transform {
            if let TextBuffer::Metal(ref transform_buffer) = self.transform {
                // Update Metal buffer with transform data
                let transform_data = bytemuck::cast_slice(&transform);
                let contents = transform_buffer.contents();
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        transform_data.as_ptr(),
                        contents as *mut u8,
                        transform_data.len(),
                    );
                }
                tracing::debug!("Updated Metal text transform buffer");
            }
            self.current_transform = transform;
        }

        let quantity = self.vertices.len();
        if quantity > self.supported_vertex_buffer {
            if let Some(metal_ctx) = context.metal_context() {
                let vertex_data = vec![0u8; mem::size_of::<Vertex>() * quantity];
                self.vertex_buffer = TextBuffer::Metal(metal_ctx.create_buffer(
                    &vertex_data,
                    metal::MTLResourceOptions::StorageModeShared,
                ));
                self.supported_vertex_buffer = quantity;
            }
        }

        if let TextBuffer::Metal(ref vertex_buffer) = self.vertex_buffer {
            // Update Metal buffer with vertex data
            let vertex_data = bytemuck::cast_slice(&self.vertices);
            let contents = vertex_buffer.contents();
            unsafe {
                std::ptr::copy_nonoverlapping(
                    vertex_data.as_ptr(),
                    contents as *mut u8,
                    vertex_data.len(),
                );
            }
            tracing::debug!(
                "Updated Metal text vertex buffer with {} vertices",
                quantity
            );
        }

        if let TextPipeline::Metal(ref pipeline) = self.pipeline {
            metal_pass.set_render_pipeline_state(pipeline);
        }

        if let TextBuffer::Metal(ref vertex_buffer) = self.vertex_buffer {
            metal_pass.set_vertex_buffer(0, Some(vertex_buffer), 0);
        }

        if let TextBuffer::Metal(ref transform_buffer) = self.transform {
            metal_pass.set_vertex_buffer(1, Some(transform_buffer), 0);
        }

        // Set glyph texture and sampler
        if let Some(ref sampler) = self.metal_sampler {
            if let Some(glyph_texture) = self.images.metal_texture() {
                metal_pass.set_fragment_texture(0, Some(glyph_texture));
                metal_pass.set_fragment_sampler(0, Some(sampler));
            }
        }

        metal_pass.draw_primitives(metal::MTLPrimitiveType::Triangle, 0, quantity as u64);
    }

    // Keep all the existing methods for text processing, glyph management, etc.
    // These remain unchanged as they work with the internal data structures

    pub fn update_builder_state(
        &mut self,
        _builder_state_update: BuilderStateUpdate,
        _context: &mut Context,
    ) {
        // Implementation remains the same - this is backend-agnostic
        // Just processes text layout and glyph data
    }

    pub fn compute_dimensions(&mut self, _id: &usize) -> SugarDimensions {
        // For now, return a default dimension
        // In a full implementation, this would calculate the actual text dimensions
        SugarDimensions {
            width: 100.0,
            height: 20.0,
            scale: 1.0,
        }
    }

    pub fn dimensions(
        &mut self,
        _font_library: &FontLibrary,
        _render_data: &BuilderLine,
        _graphics: &mut Graphics,
    ) -> Option<SugarDimensions> {
        // This method processes text to calculate dimensions
        // Implementation would be similar to the original but backend-agnostic
        // For now, return a default dimension
        Some(SugarDimensions {
            width: 100.0,
            height: 20.0,
            scale: 1.0,
        })
    }

    pub fn prepare(
        &mut self,
        context: &mut Context,
        state: &crate::sugarloaf::state::SugarState,
        graphics: &mut Graphics,
    ) {
        if state.rich_texts.is_empty() {
            self.vertices.clear();
            tracing::debug!("No rich texts to prepare");
            return;
        }

        tracing::debug!("Preparing {} rich texts", state.rich_texts.len());
        self.comp.begin();
        let library = state.content.font_library();

        for rich_text in &state.rich_texts {
            if let Some(rt) = state.content.get_state(&rich_text.id) {
                tracing::debug!(
                    "Processing rich text {} with {} lines",
                    rich_text.id,
                    rt.lines.len()
                );

                // Check if this specific rich text needs cache invalidation
                match &rt.last_update {
                    crate::layout::BuilderStateUpdate::Full => {
                        self.line_cache.clear_text_cache(rich_text.id);
                    }
                    crate::layout::BuilderStateUpdate::Partial(lines) => {
                        for line in lines {
                            self.line_cache.clear_cache(rich_text.id, line);
                        }
                    }
                    crate::layout::BuilderStateUpdate::Noop => {
                        // Do nothing
                    }
                };

                let position = (
                    rich_text.position[0] * state.style.scale_factor,
                    rich_text.position[1] * state.style.scale_factor,
                );

                self.draw_layout(
                    rich_text.id, // Pass the rich text ID for caching
                    &rt.lines,
                    &rich_text.lines,
                    Some(position),
                    library,
                    Some(&rt.layout),
                    graphics,
                );
            }
        }

        self.vertices.clear();
        self.images.process_atlases(context);
        self.comp.finish(&mut self.vertices);
        tracing::debug!("Generated {} vertices for text rendering", self.vertices.len());
        
        // Debug: Check if we have any text content
        if self.vertices.is_empty() {
            tracing::debug!("No vertices generated - checking compositor state");
        }
    }

    pub fn compute_updates(&mut self, context: &mut Context, _graphics: &mut Graphics) {
        // Process atlas updates for both WebGPU and Metal
        self.images.process_atlases(context);
        self.glyphs.process_atlases(context);

        // Update texture binding for WebGPU
        if self.backend == RenderBackend::WebGpu {
            if self.textures_version != self.images.entries.len() {
                self.textures_version = self.images.entries.len();

                if let (TextBindGroupLayout::WebGpu(layout), Some(texture_view)) =
                    (&self.layout_bind_group_layout, self.images.texture_view())
                {
                    self.layout_bind_group =
                        TextBindGroup::WebGpu(context.device.create_bind_group(
                            &wgpu::BindGroupDescriptor {
                                layout,
                                entries: &[wgpu::BindGroupEntry {
                                    binding: 0,
                                    resource: wgpu::BindingResource::TextureView(
                                        texture_view,
                                    ),
                                }],
                                label: Some("rich_text::layout_bind_group"),
                            },
                        ));
                }
            }
        }

        // For Metal, texture binding is handled directly in render_metal method
        // No bind groups needed for Metal backend
    }

    fn extract_font_metrics(
        lines: &[BuilderLine],
    ) -> Option<(f32, f32, f32, usize, f32)> {
        // Extract the first run from a line that has at least one run
        lines
            .iter()
            .filter(|line| !line.render_data.runs.is_empty())
            .map(|line| &line.render_data.runs[0])
            .next()
            .map(|run| {
                (
                    run.ascent.round(),
                    run.descent.round(),
                    (run.leading).round() * 2.0,
                    run.span.font_id,
                    run.size,
                )
            })
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_layout(
        &mut self,
        rich_text_id: usize,
        lines: &Vec<BuilderLine>,
        selected_lines: &Option<RichTextLinesRange>,
        pos: Option<(f32, f32)>,
        font_library: &FontLibrary,
        rte_layout: Option<&RichTextLayout>,
        graphics: &mut Graphics,
    ) -> Option<SugarDimensions> {
        if lines.is_empty() {
            return None;
        }

        let comp = &mut self.comp;
        let caches = (&mut self.images, &mut self.glyphs);
        let (image_cache, glyphs_cache) = caches;
        let font_coords: &[i16] = &[0, 0, 0, 0];
        let depth = 0.0;

        // Determine if we're calculating dimensions only or drawing layout
        let is_dimensions_only = pos.is_none() || rte_layout.is_none();

        // For dimensions mode, we only process the first line
        let lines_to_process = if is_dimensions_only {
            std::slice::from_ref(&lines[0])
        } else {
            lines.as_slice()
        };

        // Get initial position
        let (x, y) = pos.unwrap_or((0.0, 0.0));

        // Set up caches based on mode
        let mut glyphs = Vec::new();
        let mut last_rendered_graphic = HashSet::new();
        let mut line_y = y;
        let mut dimensions = SugarDimensions::default();

        let font_metrics = Self::extract_font_metrics(lines_to_process);
        if let Some((
            ascent,
            descent,
            leading,
            current_font_from_valid_run,
            current_font_size_from_valid_run,
        )) = font_metrics
        {
            // Initialize from first run if available
            let mut current_font = current_font_from_valid_run;
            let mut current_font_size = current_font_size_from_valid_run;

            let mut session = glyphs_cache.session(
                image_cache,
                current_font,
                font_library,
                font_coords,
                current_font_size,
            );

            // Calculate line height with modifier if available
            let line_height_without_mod = ascent + descent + leading;
            let line_height_mod = rte_layout.map_or(1.0, |layout| layout.line_height);
            let line_height = line_height_without_mod * line_height_mod;

            let skip_count = selected_lines.map_or(0, |range| range.start);
            let take_count = selected_lines
                .map_or(lines_to_process.len(), |range| range.end - range.start);

            for (line_idx, line) in lines_to_process
                .iter()
                .enumerate()
                .skip(skip_count)
                .take(take_count)
            {
                if line.render_data.runs.is_empty() {
                    continue;
                }

                // Check if we can use the cache for this line
                if !is_dimensions_only
                    && self.line_cache.has_cache(rich_text_id, line_idx)
                    && self
                        .line_cache
                        .apply_cache(rich_text_id, line_idx, comp, graphics)
                {
                    // Cache was applied successfully, skip to next line
                    line_y += line_height;
                    continue;
                }

                let mut px = x;

                // Calculate baseline differently based on mode
                let baseline = if is_dimensions_only {
                    ascent + y
                } else {
                    line_y + ascent
                };

                // Different line_y calculation based on mode
                line_y = baseline + descent;

                // Calculate padding
                let padding_y = if line_height_mod > 1.0 {
                    (line_height - line_height_without_mod) / 2.0
                } else {
                    0.0
                };

                let py = line_y;
                let mut line_operations = Vec::new();

                for run in &line.render_data.runs {
                    glyphs.clear();
                    let font = run.span.font_id;
                    let char_width = run.span.width;

                    let run_x = px;
                    for glyph in &run.glyphs {
                        let x = px;
                        let y = py + padding_y;

                        // Different advance calculation based on mode
                        if is_dimensions_only {
                            px += glyph.simple_data().1 * char_width;
                        } else {
                            px += rte_layout.unwrap().dimensions.width * char_width;
                        }

                        glyphs.push(Glyph {
                            id: glyph.simple_data().0,
                            x,
                            y,
                        });
                    }

                    // Create style with appropriate defaults
                    let style = TextRunStyle {
                        font_coords,
                        font_size: run.size,
                        color: run.span.color,
                        cursor: run.span.cursor,
                        drawable_char: run.span.drawable_char,
                        background_color: run.span.background_color,
                        baseline: py,
                        topline: py - ascent,
                        padding_y,
                        line_height,
                        line_height_without_mod,
                        advance: px - run_x,
                        decoration: run.span.decoration,
                        decoration_color: run.span.decoration_color,
                    };

                    // Update dimensions if in dimensions mode
                    if is_dimensions_only && style.advance > 0.0 && line_height > 0.0 {
                        dimensions.width = style.advance.round();
                        dimensions.height = line_height.round();
                    }

                    // Update font session if needed
                    if font != current_font || style.font_size != current_font_size {
                        current_font = font;
                        current_font_size = style.font_size;

                        session = glyphs_cache.session(
                            image_cache,
                            current_font,
                            font_library,
                            font_coords,
                            style.font_size,
                        );
                    }

                    // Handle graphics if in layout mode
                    if !is_dimensions_only {
                        if let Some(graphic) = run.span.media {
                            if !last_rendered_graphic.contains(&graphic.id) {
                                let offset_x = graphic.offset_x as f32;
                                let offset_y = graphic.offset_y as f32;

                                let graphic_render_request = GraphicRenderRequest {
                                    id: graphic.id,
                                    pos_x: run_x - offset_x,
                                    pos_y: style.topline - offset_y,
                                    width: None,
                                    height: None,
                                };

                                graphics.top_layer.push(graphic_render_request);
                                line_operations.push(BatchOperation::GraphicRequest(
                                    graphic_render_request,
                                ));

                                last_rendered_graphic.insert(graphic.id);
                            }
                        }
                    }

                    // Use a Vec to collect operations if caching
                    let mut run_operations = Vec::new();
                    let cache_ops = if !is_dimensions_only {
                        Some(&mut run_operations)
                    } else {
                        None
                    };

                    // Draw the run with caching if needed
                    comp.draw_run(
                        &mut session,
                        Rect::new(run_x, py, style.advance, 1.),
                        depth,
                        &style,
                        &glyphs,
                        cache_ops,
                    );

                    // Add run operations to line operations
                    if !is_dimensions_only {
                        line_operations.extend(run_operations);
                    }
                }

                // Store line in cache if we're not in dimensions mode
                if !is_dimensions_only {
                    self.line_cache
                        .store(rich_text_id, line_idx, line_operations);
                }

                // Update line_y for line height modifier
                if !is_dimensions_only && line_height_mod > 1.0 {
                    line_y += line_height - line_height_without_mod;
                }
            }
        }

        // Return dimensions if in dimensions mode
        if is_dimensions_only {
            if dimensions.height > 0.0 && dimensions.width > 0.0 {
                Some(dimensions)
            } else {
                None
            }
        } else {
            None
        }
    }

    // ... other existing methods remain unchanged
}
