mod batch;
mod compositor;
mod image_cache;
pub mod text;
pub mod util;

use crate::components::core::orthographic_projection;
use crate::components::rich_text::image_cache::{GlyphCache, ImageCache};
use crate::context::Context;
use crate::font::FontLibraryData;
use crate::layout::SugarDimensions;
use crate::sugarloaf::graphics::GraphicRenderRequest;
use crate::Graphics;
use compositor::{CachedRun, Compositor, DisplayList, Rect, Vertex};
use rustc_hash::FxHashMap;
use std::{borrow::Cow, mem};
use text::{Glyph, TextRunStyle};
use wgpu::util::DeviceExt;

// Note: currently it's using Indexed drawing instead of Instance drawing could be worth to
// evaluate if would make sense move to instance drawing instead
// https://math.hws.edu/graphicsbook/c9/s2.html
// https://docs.rs/wgpu/latest/wgpu/enum.VertexStepMode.html

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
    index_buffer: wgpu::Buffer,
    index_buffer_size: u64,
    current_transform: [f32; 16],
    comp: Compositor,
    draw_layout_cache: DrawLayoutCache,
    dlist: DisplayList,
    supported_vertex_buffer: usize,
    textures_version: usize,
    images: ImageCache,
    glyphs: GlyphCache,
}

impl RichTextBrush {
    pub fn new(context: &Context) -> Self {
        let device = &context.device;
        let dlist = DisplayList::new();
        let supported_vertex_buffer = 2_000;

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
                entry_point: "vs_main",
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
                entry_point: "fs_main",
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
            label: Some("rich_text::Instances Buffer"),
            size: mem::size_of::<Vertex>() as u64 * supported_vertex_buffer as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buffer_size: &[u32] = bytemuck::cast_slice(&dlist.indices);
        let index_buffer_size = index_buffer_size.len() as u64;
        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rich_text::Indices Buffer"),
            size: index_buffer_size,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        RichTextBrush {
            layout_bind_group,
            layout_bind_group_layout,
            constant_bind_group,
            index_buffer_size,
            index_buffer,
            comp: Compositor::new(),
            images,
            textures_version: 0,
            glyphs: GlyphCache::new(),
            draw_layout_cache: DrawLayoutCache::default(),
            dlist,
            transform,
            pipeline,
            vertex_buffer,
            supported_vertex_buffer,
            current_transform,
        }
    }

    #[inline]
    pub fn clean_cache(&mut self) {
        self.draw_layout_cache.clear();
    }

    #[inline]
    pub fn prepare(
        &mut self,
        context: &mut crate::context::Context,
        state: &crate::sugarloaf::state::SugarState,
        graphics: &mut Graphics,
    ) {
        // let start = std::time::Instant::now();

        if state.compositors.advanced.render_data.is_empty() {
            self.dlist.clear();
            return;
        }

        // Render
        self.comp.begin();

        let library = state.compositors.advanced.font_library();
        let font_library = { &library.inner.read().unwrap() };

        draw_layout(
            &mut self.comp,
            (
                &mut self.images,
                &mut self.glyphs,
                &mut self.draw_layout_cache,
            ),
            &state.compositors.advanced.render_data,
            state.current.layout.style.screen_position,
            font_library,
            &state.current.layout.dimensions,
            graphics,
        );
        self.draw_layout_cache.clear_on_demand();

        self.dlist.clear();
        self.images.process_events(context);
        self.images.process_atlases(context);
        self.comp.finish(&mut self.dlist);
        // let duration = start.elapsed();
        // println!(" - rich_text::prepare::draw_layout() is: {:?}", duration);

        // let duration = start.elapsed();
        // println!(" - rich_text::prepare() is: {:?}", duration);
    }

    #[inline]
    pub fn dimensions(
        &mut self,
        state: &crate::sugarloaf::state::SugarState,
    ) -> Option<SugarDimensions> {
        self.comp.begin();

        let library = state.compositors.advanced.font_library();
        let font_library = { &library.inner.read().unwrap() };

        let dimension = fetch_dimensions(
            &mut self.comp,
            (&mut self.images, &mut self.glyphs),
            &state.compositors.advanced.mocked_render_data,
            font_library,
        );
        if dimension.height > 0. && dimension.width > 0. {
            Some(dimension)
        } else {
            None
        }
    }

    #[inline]
    pub fn render<'pass>(
        &'pass mut self,
        ctx: &mut Context,
        state: &crate::sugarloaf::state::SugarState,
        rpass: &mut wgpu::RenderPass<'pass>,
    ) {
        // let start = std::time::Instant::now();
        // There's nothing to render
        if self.dlist.vertices.is_empty() {
            return;
        }

        let queue = &mut ctx.queue;

        let transform = orthographic_projection(
            state.current.layout.width,
            state.current.layout.height,
        );
        let transform_has_changed = transform != self.current_transform;

        if transform_has_changed {
            queue.write_buffer(&self.transform, 0, bytemuck::bytes_of(&transform));
            self.current_transform = transform;
        }

        if self.dlist.vertices.len() > self.supported_vertex_buffer {
            self.vertex_buffer.destroy();

            self.supported_vertex_buffer = self.dlist.vertices.len();
            self.vertex_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sugarloaf::rich_text::Pipeline instances"),
                size: mem::size_of::<Vertex>() as u64
                    * self.supported_vertex_buffer as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        let vertices_bytes: &[u8] = bytemuck::cast_slice(&self.dlist.vertices);
        if !vertices_bytes.is_empty() {
            queue.write_buffer(&self.vertex_buffer, 0, vertices_bytes);
        }

        let indices_raw: &[u8] = bytemuck::cast_slice(&self.dlist.indices);
        let indices_raw_size = indices_raw.len() as u64;

        if self.index_buffer_size >= indices_raw_size {
            queue.write_buffer(&self.index_buffer, 0, indices_raw);
        } else {
            self.index_buffer.destroy();

            let size = next_copy_buffer_size(indices_raw_size);
            let buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rich_text::Indices"),
                size,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: true,
            });
            buffer.slice(..).get_mapped_range_mut()[..indices_raw.len()]
                .copy_from_slice(indices_raw);
            buffer.unmap();

            self.index_buffer = buffer;
            self.index_buffer_size = size;
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
        rpass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        rpass.draw_indexed(0..(self.dlist.indices.len() as u32), 0, 0..1);

        // let duration = start.elapsed();
        // println!(" - rich_text::render() is: {:?}", duration);
    }
}

#[derive(Default)]
struct DrawLayoutCache {
    inner: FxHashMap<u64, Vec<CachedRun>>,
}

impl DrawLayoutCache {
    #[inline]
    fn get(&self, hash: &u64) -> Option<&Vec<CachedRun>> {
        self.inner.get(hash)
    }

    #[inline]
    fn insert(&mut self, hash: u64, data: Vec<CachedRun>) {
        self.inner.insert(hash, data);
    }

    #[inline]
    fn clear_on_demand(&mut self) {
        if self.inner.len() > 128 {
            self.inner.clear();
        }
    }

    #[inline]
    fn clear(&mut self) {
        self.inner.clear();
    }
}

fn draw_layout(
    comp: &mut compositor::Compositor,
    caches: (&mut ImageCache, &mut GlyphCache, &mut DrawLayoutCache),
    render_data: &crate::layout::RenderData,
    pos: (f32, f32),
    font_library: &FontLibraryData,
    rect: &SugarDimensions,
    graphics: &mut Graphics,
) {
    // let start = std::time::Instant::now();
    let (x, y) = pos;
    let (image_cache, glyphs_cache, draw_layout_cache) = caches;
    let depth = 0.0;
    let mut glyphs = Vec::new();
    let mut current_font = 0;
    let mut current_font_size = 0.0;
    let mut current_font_coords: &[i16] = &[0, 0, 0, 0];
    if let Some(line) = render_data.lines().next() {
        if let Some(run) = line.runs().next() {
            current_font = *run.font();
            current_font_size = run.font_size();
            current_font_coords = run.normalized_coords();
        }
    }

    let mut session = glyphs_cache.session(
        image_cache,
        font_library[current_font].as_ref(),
        current_font_coords,
        current_font_size,
    );

    let mut last_rendered_graphic = None;
    for line in render_data.lines() {
        let hash = line.hash();
        let mut px = x + line.offset();
        let py = line.baseline() + y;
        if let Some(data) = draw_layout_cache.get(&hash) {
            comp.draw_cached_run(
                data,
                px,
                py,
                depth,
                rect,
                line,
                &mut last_rendered_graphic,
                graphics,
            );
            continue;
        }

        let mut cached_line_runs = Vec::new();
        for run in line.runs() {
            let char_width = run.char_width();
            let mut cached_run = CachedRun::new(char_width);
            let font = *run.font();
            let char_width = run.char_width();

            let run_x = px;
            glyphs.clear();
            for cluster in run.visual_clusters() {
                for glyph in cluster.glyphs() {
                    cached_run.glyphs_ids.push(glyph.id);

                    let x = px + glyph.x;
                    let y = py - glyph.y;
                    // px += glyph.advance
                    px += rect.width * char_width;
                    glyphs.push(Glyph { id: glyph.id, x, y });
                }
            }

            let line_height = line.ascent() + line.descent() + line.leading();
            let style = TextRunStyle {
                font: font_library[font].as_ref(),
                font_coords: run.normalized_coords(),
                font_size: run.font_size(),
                color: run.color(),
                cursor: run.cursor(),
                background_color: run.background_color(),
                baseline: py,
                topline: py - line.ascent(),
                line_height,
                advance: px - run_x,
                decoration: run.decoration(),
                decoration_color: run.decoration_color(),
            };

            if font != current_font
                || style.font_size != current_font_size
                || style.font_coords != current_font_coords
            {
                session = glyphs_cache.session(
                    image_cache,
                    style.font,
                    style.font_coords,
                    style.font_size,
                );

                current_font = font;
                current_font_coords = style.font_coords;
                current_font_size = style.font_size;
            }

            if let Some(graphic) = run.media() {
                if last_rendered_graphic != Some(graphic.id) {
                    let offset_x = graphic.offset_x as f32;
                    let offset_y = graphic.offset_y as f32;

                    graphics.top_layer.push(GraphicRenderRequest {
                        id: graphic.id,
                        pos_x: run_x - offset_x,
                        pos_y: style.topline - offset_y,
                        width: None,
                        height: None,
                    });
                    last_rendered_graphic = Some(graphic.id);
                }

                cached_run.graphics.insert(graphic);
            }

            comp.draw_run(
                &mut session,
                Rect::new(run_x, py, style.advance, 1.),
                depth,
                &style,
                glyphs.iter(),
                &mut cached_run,
            );

            cached_line_runs.push(cached_run);
        }

        if !cached_line_runs.is_empty() {
            draw_layout_cache.insert(hash, cached_line_runs);
        }
    }

    // let duration = start.elapsed();
    // println!(" - draw_layout() is: {:?}\n", duration);
}

#[inline]
fn fetch_dimensions(
    comp: &mut compositor::Compositor,
    caches: (&mut ImageCache, &mut GlyphCache),
    render_data: &crate::layout::RenderData,
    font_library: &FontLibraryData,
) -> SugarDimensions {
    let x = 0.;
    let y = 0.;

    let (image_cache, glyphs_cache) = caches;
    let mut current_font = 0;
    let mut current_font_size = 0.0;
    let mut current_font_coords: Vec<i16> = Vec::with_capacity(4);
    if let Some(line) = render_data.lines().next() {
        if let Some(run) = line.runs().next() {
            current_font = *run.font();
            current_font_size = run.font_size();
            current_font_coords = run.normalized_coords().to_vec();
        }
    }

    let mut session = glyphs_cache.session(
        image_cache,
        font_library[current_font].as_ref(),
        &current_font_coords,
        current_font_size,
    );

    let mut glyphs = Vec::with_capacity(3);
    let mut dimension = SugarDimensions::default();
    for line in render_data.lines() {
        let mut px = x + line.offset();
        for run in line.runs() {
            let char_width = run.char_width();
            let mut cached_run = CachedRun::new(char_width);

            let font = run.font();
            let py = line.baseline() + y;
            let run_x = px;
            let line_height = line.ascent() + line.descent() + line.leading();
            glyphs.clear();
            for cluster in run.visual_clusters() {
                for glyph in cluster.glyphs() {
                    let x = px + glyph.x;
                    let y = py - glyph.y;
                    px += glyph.advance * char_width;
                    glyphs.push(Glyph { id: glyph.id, x, y });
                }
            }
            let color = run.color();

            let style = TextRunStyle {
                font: font_library[*font].as_ref(),
                font_coords: run.normalized_coords(),
                font_size: run.font_size(),
                color,
                cursor: run.cursor(),
                background_color: None,
                baseline: py,
                topline: py - line.ascent(),
                line_height,
                advance: px - run_x,
                decoration: None,
                decoration_color: None,
            };

            if style.advance > 0. && line_height > 0. {
                dimension.width = style.advance;
                dimension.height = line_height;
            }

            if font != &current_font
                || style.font_size != current_font_size
                || style.font_coords != current_font_coords
            {
                session = glyphs_cache.session(
                    image_cache,
                    style.font,
                    style.font_coords,
                    style.font_size,
                );

                current_font = *font;
                current_font_coords = style.font_coords.to_vec();
                current_font_size = style.font_size;
            }

            comp.draw_run(
                &mut session,
                Rect::new(run_x, py, style.advance, 1.),
                0.0,
                &style,
                glyphs.iter(),
                &mut cached_run,
            );
        }
    }

    dimension
}

#[inline]
fn next_copy_buffer_size(size: u64) -> u64 {
    let align_mask = wgpu::COPY_BUFFER_ALIGNMENT - 1;
    ((size.next_power_of_two() + align_mask) & !align_mask)
        .max(wgpu::COPY_BUFFER_ALIGNMENT)
}
