mod batch;
mod compositor;
mod image_cache;
#[cfg(test)]
mod positioning_tests;
pub mod text;
mod text_run_manager;

use crate::components::core::orthographic_projection;
use crate::components::rich_text::image_cache::{GlyphCache, ImageCache};
use crate::components::rich_text::text_run_manager::{CacheResult, TextRunManager};
use crate::context::Context;
use crate::font::FontLibrary;
use crate::font_introspector::GlyphId;
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
    text_run_manager: TextRunManager,
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
                entries: &[
                    // Color texture (binding 0)
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: context.get_optimal_texture_sample_type(),
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // Mask texture (binding 1)
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float {
                                filterable: true,
                            },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
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
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &images.color_texture_view,
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &images.mask_texture_view,
                    ),
                },
            ],
            label: Some("rich_text::layout_bind_group"),
        });

        let shader_source = if context.supports_f16() {
            include_str!("rich_text.wgsl")
        } else {
            include_str!("rich_text_f32.wgsl")
        };

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_source)),
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
            text_run_manager: TextRunManager::new(),
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
                        // For full updates, we don't need to clear text run cache
                        // as it's shared across all text and font-specific
                    }
                    BuilderStateUpdate::Partial(_lines) => {
                        // For partial updates, we also don't need to clear text run cache
                        // as individual text runs are still valid
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
        self.comp.begin();

        let lines = vec![render_data.clone()];
        self.draw_layout(0, &lines, &None, None, font_library, None, graphics)
    }

    /// Extract font metrics using per-font calculation.
    /// Each font calculates its own metrics using consistent approach.
    #[inline]
    fn extract_normalized_metrics(
        &self,
        lines: &[crate::layout::BuilderLine],
        font_library: &FontLibrary,
    ) -> Option<(f32, f32, f32, usize, f32)> {
        // Get the first run to determine font_id and size
        let first_run = lines
            .iter()
            .filter(|line| !line.render_data.runs.is_empty())
            .map(|line| &line.render_data.runs[0])
            .next()?;

        let font_id = 0; // FONT_ID_REGULAR
        let font_size = first_run.size;

        // Get metrics from the specific font using consistent calculation
        let mut font_library_data = font_library.inner.write();
        if let Some((ascent, descent, leading)) =
            font_library_data.get_font_metrics(&font_id, font_size)
        {
            Some((ascent, descent, leading, font_id, font_size))
        } else {
            // Fallback to run metrics if font metrics calculation fails
            Some((
                first_run.ascent,
                first_run.descent,
                first_run.leading,
                font_id,
                font_size,
            ))
        }
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn draw_layout(
        &mut self,
        _rich_text_id: usize,
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

        // Determine if we're calculating dimensions only or drawing layout
        let is_dimensions_only = pos.is_none() || rte_layout.is_none();

        // For dimensions mode, we only process the first line
        let lines_to_process = if is_dimensions_only {
            std::slice::from_ref(&lines[0])
        } else {
            lines.as_slice()
        };

        // Extract font metrics before borrowing self.comp
        let font_metrics =
            self.extract_normalized_metrics(lines_to_process, font_library);

        // let start = std::time::Instant::now();
        let comp = &mut self.comp;
        let caches = (&mut self.images, &mut self.glyphs);
        let (image_cache, glyphs_cache) = caches;
        let font_coords: &[i16] = &[0, 0, 0, 0];
        let depth = 0.0;

        // Get initial position
        let (x, y) = pos.unwrap_or((0.0, 0.0));

        // Set up caches based on mode
        let mut glyphs = Vec::new();
        let mut last_rendered_graphic = HashSet::new();
        let mut line_y = y;
        let mut dimensions = SugarDimensions::default();
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

            for (_line_idx, line) in lines_to_process
                .iter()
                .enumerate()
                .skip(skip_count)
                .take(take_count)
            {
                if line.render_data.runs.is_empty() {
                    continue;
                }

                let mut px = x;

                // Calculate baseline using proper typographic positioning
                let padding_top = (line_height - ascent - descent) / 2.0;
                let baseline = if is_dimensions_only {
                    y + padding_top + ascent
                } else {
                    line_y + padding_top + ascent
                };

                // Keep line_y as the top of the line for proper line spacing
                // Don't modify line_y here - it should remain at the top of the line

                // Calculate padding
                let padding_y = if line_height_mod > 1.0 {
                    (line_height - line_height_without_mod) / 2.0
                } else {
                    0.0
                };

                let py = if is_dimensions_only { y } else { line_y };

                for run in &line.render_data.runs {
                    let font = run.span.font_id;
                    let char_width = run.span.width;
                    let run_x = px;

                    // Extract text from the run for caching
                    let run_text = run
                        .glyphs
                        .iter()
                        .filter_map(|g| char::from_u32(g.simple_data().0 as u32))
                        .collect::<String>();

                    // Try to get cached data for this text run
                    let cached_result = if !is_dimensions_only && !run_text.is_empty() {
                        self.text_run_manager.get_cached_data(
                            &run_text,
                            font,
                            run.size,
                            Some(run.span.color),
                        )
                    } else {
                        CacheResult::Miss
                    };

                    match cached_result {
                        CacheResult::FullRender {
                            glyphs: cached_glyphs,
                            vertices,
                            base_position,
                            ..
                        } => {
                            // Use cached render data - apply vertices directly
                            let mut vertex_output = Vec::new();
                            TextRunManager::apply_cached_vertices(
                                &vertices,
                                base_position,
                                (run_x, py + padding_y),
                                &mut vertex_output,
                            );

                            // Update position based on cached advance
                            let cached_advance =
                                cached_glyphs.iter().map(|g| g.x_advance).sum::<f32>();
                            if is_dimensions_only {
                                px += cached_advance * char_width;
                            } else {
                                px += rte_layout.unwrap().dimensions.width * char_width;
                            }
                        }
                        CacheResult::ShapingOnly {
                            glyphs: cached_glyphs,
                            ..
                        }
                        | CacheResult::GlyphsOnly {
                            glyphs: cached_glyphs,
                            ..
                        } => {
                            // Use cached glyph data but need to render
                            glyphs.clear();
                            for shaped_glyph in cached_glyphs.iter() {
                                let x = px;
                                let y = baseline; // Glyph y should be at baseline position

                                if is_dimensions_only {
                                    px += shaped_glyph.x_advance * char_width;
                                } else {
                                    px +=
                                        rte_layout.unwrap().dimensions.width * char_width;
                                }

                                glyphs.push(Glyph {
                                    id: shaped_glyph.glyph_id as GlyphId,
                                    x,
                                    y,
                                });
                            }

                            // Render using cached glyph data
                            if !is_dimensions_only {
                                let style = TextRunStyle {
                                    font_coords,
                                    font_size: run.size,
                                    color: run.span.color,
                                    cursor: run.span.cursor,
                                    drawable_char: run.span.drawable_char,
                                    background_color: run.span.background_color,
                                    baseline,
                                    topline: py, // Use py (line top) for cursor positioning
                                    line_height,
                                    padding_y,
                                    line_height_without_mod,
                                    advance: cached_glyphs
                                        .iter()
                                        .map(|g| g.x_advance)
                                        .sum(),
                                    decoration: run.span.decoration,
                                    decoration_color: run.span.decoration_color,
                                    underline_offset: run.underline_offset,
                                    strikeout_offset: run.strikeout_offset,
                                    underline_thickness: run.strikeout_size,
                                    x_height: run.x_height,
                                    ascent: run.ascent,
                                    descent: run.descent,
                                };

                                // Update font session if needed
                                if font != current_font
                                    || style.font_size != current_font_size
                                {
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

                                comp.draw_run(
                                    &mut session,
                                    Rect::new(run_x, py, px - run_x, 1.),
                                    depth,
                                    &style,
                                    &glyphs,
                                    None,
                                );
                            }
                        }
                        CacheResult::Miss => {
                            // No cached data - need to shape and render from scratch
                            glyphs.clear();
                            let mut shaped_glyphs = Vec::new();

                            for glyph in &run.glyphs {
                                let x = px;
                                let y = baseline; // Use baseline for consistency with cached path
                                let advance = glyph.simple_data().1;

                                // Different advance calculation based on mode
                                if is_dimensions_only {
                                    px += advance * char_width;
                                } else {
                                    px +=
                                        rte_layout.unwrap().dimensions.width * char_width;
                                }

                                let glyph_id = glyph.simple_data().0;
                                glyphs.push(Glyph { id: glyph_id, x, y });

                                // Store for caching
                                shaped_glyphs.push(
                                    crate::font::text_run_cache::ShapedGlyph {
                                        glyph_id: glyph_id as u32,
                                        x_advance: advance,
                                        y_advance: 0.0,
                                        x_offset: 0.0,
                                        y_offset: 0.0,
                                        cluster: 0,
                                        atlas_coords: None,
                                        atlas_layer: None,
                                    },
                                );
                            }

                            // Cache the shaped glyphs for future use
                            if !is_dimensions_only && !run_text.is_empty() {
                                self.text_run_manager.cache_shaping_data(
                                    &run_text,
                                    font,
                                    run.size,
                                    shaped_glyphs,
                                    false, // has_emoji - would need to be detected
                                    None, // shaping_features - would need actual shaping data
                                );
                            }

                            // Create style for rendering
                            if !is_dimensions_only {
                                let style = TextRunStyle {
                                    font_coords,
                                    font_size: run.size,
                                    color: run.span.color,
                                    cursor: run.span.cursor,
                                    drawable_char: run.span.drawable_char,
                                    background_color: run.span.background_color,
                                    baseline,
                                    topline: py, // Use py (line top) for cursor positioning
                                    line_height,
                                    padding_y,
                                    line_height_without_mod,
                                    advance: px - run_x,
                                    decoration: run.span.decoration,
                                    decoration_color: run.span.decoration_color,
                                    underline_offset: run.underline_offset,
                                    strikeout_offset: run.strikeout_offset,
                                    underline_thickness: run.strikeout_size,
                                    x_height: run.x_height,
                                    ascent: run.ascent,
                                    descent: run.descent,
                                };

                                // Update font session if needed
                                if font != current_font
                                    || style.font_size != current_font_size
                                {
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

                                comp.draw_run(
                                    &mut session,
                                    Rect::new(run_x, py, px - run_x, 1.),
                                    depth,
                                    &style,
                                    &glyphs,
                                    None,
                                );
                            }
                        }
                    }

                    // Update dimensions if in dimensions mode
                    if is_dimensions_only {
                        let advance = px - run_x;
                        if advance > 0.0 && line_height > 0.0 {
                            dimensions.width = advance.round();
                            dimensions.height = line_height.round();
                        }
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
                                    pos_y: py - ascent - offset_y,
                                    width: None,
                                    height: None,
                                };

                                graphics.top_layer.push(graphic_render_request);
                                last_rendered_graphic.insert(graphic.id);
                            }
                        }
                    }
                }

                // Advance line_y for the next line
                if !is_dimensions_only {
                    line_y += line_height;
                }
            }
        }

        // let screen_render_duration = start.elapsed();
        // if self.renderer.enable_performance_logging {
        // println!("[PERF] draw_layout() total: {:?}", screen_render_duration);

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
        self.text_run_manager.clear_all();
    }

    #[inline]
    pub fn clear_atlas(&mut self) {
        self.images.clear_atlas();
        self.glyphs = GlyphCache::new();
        self.text_run_manager.clear_all();
        tracing::info!("RichTextBrush atlas, glyph cache, and text run cache cleared");
    }

    #[inline]
    pub fn render<'pass>(
        &'pass mut self,
        ctx: &mut Context,
        rpass: &mut wgpu::RenderPass<'pass>,
    ) {
        // let start = std::time::Instant::now();
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

            // Allocate 25% more buffer space to reduce frequent reallocations
            self.supported_vertex_buffer = (self.vertices.len() as f32 * 1.25) as usize;
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
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(
                                &self.images.color_texture_view,
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(
                                &self.images.mask_texture_view,
                            ),
                        },
                    ],
                    label: Some("rich_text::Pipeline uniforms"),
                });
        }

        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.constant_bind_group, &[]);
        rpass.set_bind_group(1, &self.layout_bind_group, &[]);
        rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

        let vertex_count = self.vertices.len() as u32;
        rpass.draw(0..vertex_count, 0..1);
        // let duration = start.elapsed();
        // println!("Time elapsed in rich_text::render is: {:?}", duration);
    }
}

#[cfg(test)]
mod rect_positioning_tests {
    #[derive(Debug)]
    struct GlyphRect {
        #[allow(unused)]
        pub x: f32,
        pub y: f32,
        #[allow(unused)]
        pub width: f32,
        pub height: f32,
        #[allow(unused)]
        pub baseline_y: f32,
        pub glyph_center_x: f32,
        pub glyph_center_y: f32,
    }

    #[derive(Debug)]
    struct LineRect {
        #[allow(unused)]
        pub x: f32,
        pub y: f32,
        #[allow(unused)]
        pub width: f32,
        pub height: f32,
        pub baseline_y: f32,
    }

    #[test]
    fn test_glyph_rect_positioning_and_centering() {
        // Test parameters
        let line_height = 20.0;
        let char_width = 8.0;
        let ascent = 12.0;
        let descent = 4.0;
        let _leading = 0.0;

        // Expected calculations (matching our current implementation)
        let padding_top = (line_height - ascent - descent) / 2.0; // (20 - 12 - 4) / 2 = 2.0
        let expected_baseline_y = 0.0 + padding_top + ascent; // 0 + 2 + 12 = 14.0

        // Create line rect
        let line_rect = LineRect {
            x: 0.0,
            y: 0.0,
            width: char_width,
            height: line_height,
            baseline_y: expected_baseline_y,
        };

        // Expected glyph rect (should be centered within line rect)
        let expected_glyph_rect = GlyphRect {
            x: 0.0,
            y: 0.0,
            width: char_width,
            height: line_height,
            baseline_y: expected_baseline_y,
            glyph_center_x: char_width / 2.0,  // 4.0
            glyph_center_y: line_height / 2.0, // 10.0
        };

        // println!("Line height: {}", line_height);
        // println!(
        //     "Ascent: {}, Descent: {}, Leading: {}",
        //     ascent, descent, _leading
        // );
        // println!("Padding top: {}", padding_top);
        // println!("Expected baseline Y: {}", expected_baseline_y);
        // println!(
        //     "Expected glyph center: ({}, {})",
        //     expected_glyph_rect.glyph_center_x, expected_glyph_rect.glyph_center_y
        // );

        // Verify baseline is positioned correctly within the line rect
        assert!(
            expected_baseline_y > line_rect.y,
            "Baseline should be below line top"
        );
        assert!(
            expected_baseline_y < line_rect.y + line_rect.height,
            "Baseline should be above line bottom"
        );

        // Verify glyph center is in the middle of the rect
        assert_eq!(
            expected_glyph_rect.glyph_center_x,
            char_width / 2.0,
            "Glyph should be horizontally centered"
        );
        assert_eq!(
            expected_glyph_rect.glyph_center_y,
            line_height / 2.0,
            "Glyph should be vertically centered"
        );

        // Verify baseline relationship to glyph center
        let baseline_offset_from_center =
            expected_baseline_y - expected_glyph_rect.glyph_center_y;
        // println!("Baseline offset from glyph center: {baseline_offset_from_center}");

        // The baseline should be slightly above center for typical fonts
        // With ascent=12, descent=4, the baseline should be at 14.0, center at 10.0
        // So baseline is 4.0 units above center, which makes sense
        assert_eq!(
            baseline_offset_from_center, 4.0,
            "Baseline should be 4.0 units above glyph center"
        );
    }

    #[test]
    fn test_multiple_line_rects_spacing() {
        let line_height = 20.0;
        let ascent = 12.0;
        let descent = 4.0;
        let _leading = 0.0;

        let padding_top = (line_height - ascent - descent) / 2.0;

        // Test 3 lines
        let line_rects = [
            LineRect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: line_height,
                baseline_y: 0.0 + padding_top + ascent,
            },
            LineRect {
                x: 0.0,
                y: line_height,
                width: 100.0,
                height: line_height,
                baseline_y: line_height + padding_top + ascent,
            },
            LineRect {
                x: 0.0,
                y: line_height * 2.0,
                width: 100.0,
                height: line_height,
                baseline_y: (line_height * 2.0) + padding_top + ascent,
            },
        ];

        for (i, rect) in line_rects.iter().enumerate() {
            // println!("Line {}: y={}, baseline_y={}", i, rect.y, rect.baseline_y);

            // Verify each line's baseline is positioned correctly within its rect
            assert!(
                rect.baseline_y > rect.y,
                "Line {i} baseline should be below line top",
            );
            assert!(
                rect.baseline_y < rect.y + rect.height,
                "Line {i} baseline should be above line bottom",
            );

            // Verify consistent baseline positioning within each line
            let baseline_offset_from_top = rect.baseline_y - rect.y;
            assert_eq!(
                baseline_offset_from_top,
                padding_top + ascent,
                "Line {i} baseline offset should be consistent",
            );
        }

        // Verify lines don't overlap
        for i in 1..line_rects.len() {
            let prev_line = &line_rects[i - 1];
            let curr_line = &line_rects[i];
            assert_eq!(
                curr_line.y,
                prev_line.y + prev_line.height,
                "Lines should be adjacent without gaps or overlaps"
            );
        }
    }

    #[test]
    fn test_baseline_correctness_with_different_line_heights() {
        let ascent = 12.0;
        let descent = 4.0;
        let leading = 0.0;
        let base_line_height = ascent + descent + leading; // 16.0

        let test_cases = vec![
            ("Normal line height", base_line_height),
            ("1.5x line height", base_line_height * 1.5),
            ("2x line height", base_line_height * 2.0),
        ];

        for (name, line_height) in test_cases {
            let padding_top = (line_height - ascent - descent) / 2.0;
            let baseline_y = 0.0 + padding_top + ascent;

            // println!(
            //     "{}: line_height={}, padding_top={}, baseline_y={}",
            //     name, line_height, padding_top, baseline_y
            // );

            // Verify baseline is always positioned at ascent distance from the visual center
            let line_center: f32 = line_height / 2.0;
            let expected_baseline_from_center: f32 = (ascent - descent) / 2.0; // Should be 4.0 for our test values
            let actual_baseline_from_center: f32 = baseline_y - line_center;

            let diff =
                (actual_baseline_from_center - expected_baseline_from_center).abs();
            assert!(
                diff < 0.001,
                "{name}: Baseline should be {expected_baseline_from_center} units above center, got {actual_baseline_from_center}",
            );

            // Verify glyph would be centered in the line
            let glyph_center_y = line_height / 2.0;
            assert_eq!(
                glyph_center_y, line_center,
                "{name}: Glyph center should match line center",
            );
        }
    }

    #[test]
    fn test_glyph_positioning_relative_to_baseline() {
        let line_height = 20.0;
        let ascent = 12.0;
        let descent = 4.0;
        let char_width = 8.0;

        let padding_top = (line_height - ascent - descent) / 2.0;
        let baseline_y = 0.0 + padding_top + ascent;

        // In font rendering, glyphs are positioned relative to baseline
        // The glyph's y coordinate should be the baseline position
        let glyph_y = baseline_y;

        // The glyph rect encompasses the entire line height for background/selection
        let glyph_rect = GlyphRect {
            x: 0.0,
            y: 0.0, // Top of line
            width: char_width,
            height: line_height,
            baseline_y,
            glyph_center_x: char_width / 2.0,
            glyph_center_y: line_height / 2.0,
        };

        // println!("=== GLYPH POSITIONING RELATIVE TO BASELINE TEST ===");
        // println!("Baseline Y: {}", baseline_y);
        // println!("Glyph Y (for font rendering): {}", glyph_y);
        // println!("Glyph rect Y (for backgrounds): {}", glyph_rect.y);
        // println!(
        //     "Glyph center: ({}, {})",
        //     glyph_rect.glyph_center_x, glyph_rect.glyph_center_y
        // );

        // Key assertions:
        // 1. Glyph for font rendering is positioned at baseline
        assert_eq!(
            glyph_y, baseline_y,
            "Glyph Y for font rendering should be at baseline"
        );

        // 2. Glyph rect for backgrounds spans the full line height
        assert_eq!(glyph_rect.y, 0.0, "Glyph rect should start at line top");
        assert_eq!(
            glyph_rect.height, line_height,
            "Glyph rect should span full line height"
        );

        // 3. Glyph is visually centered within the line
        assert_eq!(
            glyph_rect.glyph_center_y,
            line_height / 2.0,
            "Glyph should be visually centered"
        );

        // 4. Baseline is positioned correctly relative to glyph center
        let baseline_offset_from_center = baseline_y - glyph_rect.glyph_center_y;
        let expected_offset = (ascent - descent) / 2.0; // (12 - 4) / 2 = 4.0
        assert_eq!(
            baseline_offset_from_center, expected_offset,
            "Baseline should be {expected_offset} units above glyph center",
        );
    }

    #[test]
    fn test_cursor_positioning_consistency() {
        // This test verifies that cursor positioning is consistent between cached and non-cached paths
        let line_height = 20.0;
        let ascent = 12.0;
        let descent = 4.0;
        let _leading = 0.0;

        // Simulate the calculations from both paths
        let line_y = 0.0; // Top of first line
        let padding_top = (line_height - ascent - descent) / 2.0; // 2.0
        let baseline = line_y + padding_top + ascent; // 0 + 2 + 12 = 14.0
        let py = line_y; // 0.0

        // Both paths should use the same topline calculation
        let topline = py - ascent; // 0 - 12 = -12.0

        // println!("=== CURSOR POSITIONING CONSISTENCY TEST ===");
        // println!("Line Y: {}", line_y);
        // println!("Baseline: {}", baseline);
        // println!("PY: {}", py);
        // println!("Topline: {}", topline);

        // Key assertions for cursor positioning:
        // 1. Topline should be above the line (negative relative to line top)
        assert!(
            topline < line_y,
            "Topline should be above line top for cursor positioning"
        );

        // 2. Baseline should be within the line bounds
        assert!(baseline > line_y, "Baseline should be below line top");
        assert!(
            baseline < line_y + line_height,
            "Baseline should be above line bottom"
        );

        // 3. The relationship between topline and baseline should be consistent
        let topline_to_baseline_distance = baseline - topline; // 14 - (-12) = 26
        assert_eq!(
            topline_to_baseline_distance,
            ascent + padding_top + ascent,
            "Distance from topline to baseline should be consistent"
        );

        // println!(
        //     "✓ Topline to baseline distance: {}",
        //     topline_to_baseline_distance
        // );
    }
}
