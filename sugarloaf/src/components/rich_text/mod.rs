mod batch;
mod compositor;
mod image_cache;
pub mod text;

use crate::components::core::orthographic_projection;
use crate::components::rich_text::compositor::{BatchOperation, LineCache};
use crate::components::rich_text::image_cache::{GlyphCache, ImageCache};
use crate::context::Context;
use crate::font::FontLibrary;
use crate::layout::{BuilderStateUpdate, RichTextLayout, SugarDimensions};
use crate::sugarloaf::graphics::GraphicRenderRequest;
use crate::Graphics;
use crate::RichTextLinesRange;
use compositor::{Compositor, Rect, Vertex};
use std::collections::HashSet;
use std::{borrow::Cow, mem};
use text::{Glyph, TextRunStyle};
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

pub struct RichTextBrush {
    vertex_buffer: wgpu::Buffer,
    constant_bind_group: wgpu::BindGroup,
    layout_bind_group: wgpu::BindGroup,
    layout_bind_group_layout: wgpu::BindGroupLayout,
    transform: wgpu::Buffer,
    pipeline: wgpu::RenderPipeline,
    current_transform: [f32; 16],
    comp: Compositor,
    vertices: Vec<Vertex>,
    supported_vertex_buffer: usize,
    textures_version: usize,
    images: ImageCache,
    glyphs: GlyphCache,
    line_cache: LineCache,
}

impl RichTextBrush {
    pub fn new(context: &Context) -> Self {
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
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                }],
            });

        let pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                push_constant_ranges: &[],
                bind_group_layouts: &[
                    &constant_bind_group_layout,
                    &layout_bind_group_layout,
                ],
            });

        let images = ImageCache::new(context);

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: 0f32,
            lod_max_clamp: 0f32,
            ..Default::default()
        });

        let constant_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &constant_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &transform,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
            label: Some("rich_text::constant_bind_group"),
        });

        let layout_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &layout_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&images.texture_view),
            }],
            label: Some("rich_text::layout_bind_group"),
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(
                "rich_text.wgsl"
            ))),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            cache: None,
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: mem::size_of::<Vertex>() as u64,
                    // https://docs.rs/wgpu/latest/wgpu/enum.VertexStepMode.html
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array!(
                        0 => Float32x3,
                        1 => Float32x4,
                        2 => Float32x2,
                        3 => Sint32x2,
                    ),
                }],
            },
            fragment: Some(wgpu::FragmentState {
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: context.format,
                    blend: BLEND,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rich_text::Vertices Buffer"),
            size: mem::size_of::<Vertex>() as u64 * supported_vertex_buffer as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        RichTextBrush {
            line_cache: LineCache::new(),
            layout_bind_group,
            layout_bind_group_layout,
            constant_bind_group,
            comp: Compositor::new(),
            images,
            textures_version: 0,
            glyphs: GlyphCache::new(),
            vertices: vec![],
            transform,
            pipeline,
            vertex_buffer,
            supported_vertex_buffer,
            current_transform,
        }
    }

    #[inline]
    pub fn prepare(
        &mut self,
        context: &mut crate::context::Context,
        state: &crate::sugarloaf::state::SugarState,
        graphics: &mut Graphics,
    ) {
        if state.rich_texts.is_empty() {
            self.vertices.clear();
            return;
        }

        self.comp.begin();
        let library = state.content.font_library();

        for rich_text in &state.rich_texts {
            if let Some(rt) = state.content.get_state(&rich_text.id) {
                // Check if this specific rich text needs cache invalidation
                match &rt.last_update {
                    BuilderStateUpdate::Full => {
                        self.line_cache.clear_text_cache(rich_text.id);
                    }
                    BuilderStateUpdate::Partial(lines) => {
                        for line in lines {
                            self.line_cache.clear_cache(rich_text.id, line);
                        }
                    }
                    BuilderStateUpdate::Noop => {
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
    }

    #[inline]
    pub fn dimensions(
        &mut self,
        font_library: &FontLibrary,
        render_data: &crate::layout::BuilderLine,
        graphics: &mut Graphics,
    ) -> Option<SugarDimensions> {
        // Every dimension request will lead to clear lines cache
        self.line_cache.clear_all();
        self.comp.begin();

        let lines = vec![render_data.clone()];
        self.draw_layout(0, &lines, &None, None, font_library, None, graphics)
    }

    fn extract_font_metrics(lines: &[crate::layout::BuilderLine]) -> Option<(f32, f32, f32, usize, f32)> {
        // Extract the first run from a line that has at least one run
        lines.iter()
            .filter(|line| !line.render_data.runs.is_empty())
            .map(|line| &line.render_data.runs[0])
            .next()
            .map(|run| (
                run.ascent.round(),
                run.descent.round(),
                (run.leading).round() * 2.0,
                run.span.font_id,
                run.size
            ))
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn draw_layout(
        &mut self,
        rich_text_id: usize,
        lines: &Vec<crate::layout::BuilderLine>,
        selected_lines: &Option<RichTextLinesRange>,
        pos: Option<(f32, f32)>,
        font_library: &FontLibrary,
        rte_layout: Option<&RichTextLayout>,
        graphics: &mut Graphics,
    ) -> Option<SugarDimensions> {
        if lines.is_empty() {
            return None;
        }

        // let start = std::time::Instant::now();
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
        let mut current_font = 0;
        let mut current_font_size = 0.0;

        // Initialize from first run if available
        if let Some(line) = lines_to_process.first() {
            if let Some(first_run) = line.render_data.runs.first() {
                current_font = first_run.span.font_id;
                current_font_size = first_run.size;
            }
        }

        let mut session = glyphs_cache.session(
            image_cache,
            current_font,
            font_library,
            font_coords,
            current_font_size,
        );

        let mut glyphs = Vec::new();
        let mut last_rendered_graphic = HashSet::new();
        let mut line_y = y;
        let mut dimensions = SugarDimensions::default();

        let font_metrics = Self::extract_font_metrics(&lines);
        if let Some((ascent, descent, leading, current_font_from_valid_run, current_font_size_from_valid_run)) = font_metrics {
            // Initialize from first run if available
            current_font = current_font_from_valid_run;
            current_font_size = current_font_size_from_valid_run;

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

    #[inline]
    pub fn reset(&mut self) {
        self.glyphs = GlyphCache::new();
    }

    #[inline]
    pub fn render<'pass>(
        &'pass mut self,
        ctx: &mut Context,
        rpass: &mut wgpu::RenderPass<'pass>,
    ) {
        // There's nothing to render
        if self.vertices.is_empty() {
            return;
        }

        let queue = &mut ctx.queue;

        let transform = orthographic_projection(ctx.size.width, ctx.size.height);
        let transform_has_changed = transform != self.current_transform;

        if transform_has_changed {
            queue.write_buffer(&self.transform, 0, bytemuck::bytes_of(&transform));
            self.current_transform = transform;
        }

        if self.vertices.len() > self.supported_vertex_buffer {
            self.vertex_buffer.destroy();

            self.supported_vertex_buffer = self.vertices.len();
            self.vertex_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sugarloaf::rich_text::Pipeline vertices"),
                size: mem::size_of::<Vertex>() as u64
                    * self.supported_vertex_buffer as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        let vertices_bytes: &[u8] = bytemuck::cast_slice(&self.vertices);
        if !vertices_bytes.is_empty() {
            queue.write_buffer(&self.vertex_buffer, 0, vertices_bytes);
        }

        if self.textures_version != self.images.entries.len() {
            self.textures_version = self.images.entries.len();
            self.layout_bind_group =
                ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    layout: &self.layout_bind_group_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            &self.images.texture_view,
                        ),
                    }],
                    label: Some("rich_text::Pipeline uniforms"),
                });
        }

        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.constant_bind_group, &[]);
        rpass.set_bind_group(1, &self.layout_bind_group, &[]);
        rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

        // Use draw instead of draw_indexed
        let vertex_count = self.vertices.len() as u32;
        rpass.draw(0..vertex_count, 0..1);
    }
}
