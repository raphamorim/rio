mod batch;
mod compositor;
mod image_cache;
pub mod text;
pub mod util;

use crate::components::core::orthographic_projection;
use crate::context::Context;
use crate::font::FontLibraryData;
use crate::layout::SugarDimensions;
use compositor::{
    Command, Compositor, DisplayList, Rect, TextureEvent, TextureId, Vertex,
};
use fnv::FnvHashMap;
use std::{borrow::Cow, mem};
use text::{Glyph, TextRunStyle, UnderlineStyle};
use wgpu::util::DeviceExt;
use wgpu::Texture;

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
    bind_group: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    color_texture_view: wgpu::TextureView,
    mask_texture_view: wgpu::TextureView,
    transform: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    textures: FnvHashMap<TextureId, Texture>,
    index_buffer: wgpu::Buffer,
    index_buffer_size: u64,
    current_transform: [f32; 16],
    comp: Compositor,
    dlist: DisplayList,
    bind_group_needs_update: bool,
    first_run: bool,
    supported_vertex_buffer: usize,
}

impl RichTextBrush {
    pub fn new(context: &Context) -> Self {
        let device = &context.device;
        let dlist = DisplayList::new();
        let supported_vertex_buffer = 5_000;

        let current_transform =
            orthographic_projection(context.size.width, context.size.height);
        let transform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&current_transform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Create pipeline layout
        let bind_group_layout =
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
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float {
                                filterable: true,
                            },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::VERTEX
                            | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float {
                                filterable: true,
                            },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::VERTEX
                            | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(
                            wgpu::SamplerBindingType::Filtering,
                        ),
                        count: None,
                    },
                ],
            });

        let pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                push_constant_ranges: &[],
                bind_group_layouts: &[&bind_group_layout],
            });

        let color_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rich_text create color_texture"),
            size: wgpu::Extent3d {
                width: context.size.width as u32,
                height: context.size.height as u32,
                depth_or_array_layers: 1,
            },
            view_formats: &[],
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            mip_level_count: 1,
            sample_count: 1,
        });
        let color_texture_view =
            color_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mask_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rich_text create mask_texture"),
            size: wgpu::Extent3d {
                width: context.size.width as u32,
                height: context.size.height as u32,
                depth_or_array_layers: 1,
            },
            view_formats: &[],
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            mip_level_count: 1,
            sample_count: 1,
        });
        let mask_texture_view =
            mask_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            // mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: 0f32,
            lod_max_clamp: 0f32,
            ..Default::default()
        });

        // Create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
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
                    resource: wgpu::BindingResource::TextureView(&color_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&mask_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
            label: Some("rich_text::Pipeline uniforms"),
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(
                "rich_text.wgsl"
            ))),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                        0 => Float32x4,
                        1 => Float32x4,
                        2 => Float32x2,
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
            primitive: wgpu::PrimitiveState::default(),
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

        let index_buffer_size: &[u32] = bytemuck::cast_slice(dlist.indices());
        let index_buffer_size = index_buffer_size.len() as u64;
        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rich_text::Indices Buffer"),
            size: index_buffer_size,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        RichTextBrush {
            bind_group_layout,
            index_buffer_size,
            index_buffer,
            color_texture_view,
            mask_texture_view,
            sampler,
            textures: FnvHashMap::default(),
            comp: Compositor::new(2048),
            dlist,
            bind_group,
            transform,
            pipeline,
            vertex_buffer,
            first_run: true,
            bind_group_needs_update: true,
            supported_vertex_buffer,
            current_transform,
        }
    }

    #[inline]
    pub fn prepare(
        &mut self,
        ctx: &mut Context,
        state: &crate::sugarloaf::state::SugarState,
    ) {
        // Render
        self.comp.begin();

        let library = state.compositors.advanced.font_library();
        let font_library = { &library.inner.read().unwrap() };

        draw_layout(
            &mut self.comp,
            &state.compositors.advanced.render_data,
            state.current.layout.style.screen_position.0,
            // TODO: Fix position
            state.current.layout.style.screen_position.1,
            font_library,
            state.current.layout.dimensions,
        );
        self.dlist.clear();
        self.finish_composition(ctx);
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
        let vertices: &[Vertex] = self.dlist.vertices();
        let indices: &[u32] = self.dlist.indices();

        // There's nothing to render
        if vertices.is_empty() {
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

        if vertices.len() > self.supported_vertex_buffer {
            self.vertex_buffer.destroy();

            self.supported_vertex_buffer = vertices.len();
            self.vertex_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sugarloaf::rich_text::Pipeline instances"),
                size: mem::size_of::<Vertex>() as u64
                    * self.supported_vertex_buffer as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        let vertices_bytes: &[u8] = bytemuck::cast_slice(vertices);
        if !vertices_bytes.is_empty() {
            queue.write_buffer(&self.vertex_buffer, 0, vertices_bytes);
        }

        let indices_raw: &[u8] = bytemuck::cast_slice(indices);
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

        let mut ranges = vec![];

        let mut color_texture_updated: Option<&TextureId> = None;
        let mut mask_texture_updated: Option<&TextureId> = None;

        for command in self.dlist.commands() {
            match command {
                Command::BindPipeline(pipeline) => {
                    log::info!("BindPipeline {:?}", pipeline);
                    // TODO:
                    // rpass.set_blend_constant

                    // match pipeline {
                    //     Pipeline::Opaque => {
                    //         unsafe {
                    //             gl::DepthMask(1);
                    //             gl::Disable(gl::BLEND);
                    //             gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
                    //             gl::BlendEquation(gl::FUNC_ADD);
                    //         }
                    //     }
                    //     Pipeline::Transparent => {
                    //         unsafe {
                    //             gl::DepthMask(0);
                    //             gl::Enable(gl::BLEND);
                    //             gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
                    //         }
                    //     }
                    //     Pipeline::Subpixel => {
                    //         unsafe {
                    //             gl::DepthMask(0);
                    //             gl::Enable(gl::BLEND);
                    //             gl::BlendFunc(gl::SRC1_COLOR, gl::ONE_MINUS_SRC1_COLOR);
                    //         }
                    //     }
                    // }
                }
                Command::BindTexture(unit, id) => {
                    if self.bind_group_needs_update {
                        match unit {
                            // color_texture
                            0 => {
                                // if color_texture_updated.is_none() {
                                if let Some(texture) = self.textures.get(id) {
                                    log::info!("rich_text::BindTexture, set color_texture_view {:?} {:?}", unit, id);
                                    self.color_texture_view = texture.create_view(
                                        &wgpu::TextureViewDescriptor::default(),
                                    );
                                    color_texture_updated = Some(id);
                                }
                                // }
                            }
                            // mask_texture
                            1 => {
                                // if mask_texture_updated.is_none() {
                                if let Some(texture) = self.textures.get(id) {
                                    log::info!("rich_text::BindTexture, set mask_texture_view {:?} {:?}", unit, id);
                                    self.mask_texture_view = texture.create_view(
                                        &wgpu::TextureViewDescriptor::default(),
                                    );
                                    mask_texture_updated = Some(id);
                                }
                                // }
                            }
                            _ => {
                                // Noop
                            }
                        }
                    };
                }
                Command::Draw { start, count } => {
                    let end = start + count;
                    ranges.push((*start, end));
                }
            }
        }

        // Ensure texture views are not empty in the first run
        if self.first_run && mask_texture_updated.is_none() {
            if let Some(texture) = self
                .textures
                .get(color_texture_updated.unwrap_or(&TextureId(0)))
            {
                self.mask_texture_view =
                    texture.create_view(&wgpu::TextureViewDescriptor::default());
            }
        }
        if self.first_run && color_texture_updated.is_none() {
            if let Some(texture) = self
                .textures
                .get(mask_texture_updated.unwrap_or(&TextureId(0)))
            {
                self.color_texture_view =
                    texture.create_view(&wgpu::TextureViewDescriptor::default());
            }
        }

        if self.bind_group_needs_update {
            self.bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &self.transform,
                            offset: 0,
                            size: None,
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(
                            &self.color_texture_view,
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(
                            &self.mask_texture_view,
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
                label: Some("rich_text::Pipeline uniforms"),
            });
        }

        // let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        //     label: None,
        //     timestamp_writes: None,
        //     occlusion_query_set: None,
        //     color_attachments: &[Some(wgpu::RenderPassColorAttachment {
        //         view,
        //         resolve_target: None,
        //         ops: wgpu::Operations {
        //             load: wgpu::LoadOp::Load,
        //             store: wgpu::StoreOp::Store,
        //         },
        //     })],
        //     depth_stencil_attachment: None,
        // });
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.bind_group, &[]);
        rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        rpass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        for items in ranges {
            rpass.draw_indexed(items.0..items.1, 0, 0..1);
        }

        // drop(rpass);
        self.bind_group_needs_update = false;
        self.first_run = false;
    }

    #[inline]
    fn finish_composition(&mut self, ctx: &mut Context) {
        self.comp.finish(&mut self.dlist, |event| {
            match event {
                TextureEvent::CreateTexture {
                    id,
                    format,
                    width,
                    height,
                    data,
                } => {
                    log::info!(
                        "rich_text::CreateTexture with id ({:?}) and format {:?}",
                        id,
                        format
                    );
                    let texture_size = wgpu::Extent3d {
                        width: width.into(),
                        height: height.into(),
                        depth_or_array_layers: 1,
                    };
                    let texture = ctx.device.create_texture(&wgpu::TextureDescriptor {
                        size: texture_size,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: match format {
                            image_cache::PixelFormat::A8 => wgpu::TextureFormat::R8Unorm,
                            image_cache::PixelFormat::Rgba8 => {
                                wgpu::TextureFormat::Rgba8Unorm
                            }
                        },
                        usage: wgpu::TextureUsages::TEXTURE_BINDING
                            | wgpu::TextureUsages::COPY_DST,
                        label: Some("rich_text::Cache"),
                        view_formats: &[],
                    });

                    if let Some(data) = data {
                        self.bind_group_needs_update = true;
                        let channels = match format {
                            // Mask
                            image_cache::PixelFormat::A8 => 1,
                            // Color
                            image_cache::PixelFormat::Rgba8 => 4,
                        };

                        ctx.queue.write_texture(
                            // Tells wgpu where to copy the pixel data
                            wgpu::ImageCopyTexture {
                                texture: &texture,
                                mip_level: 0,
                                origin: wgpu::Origin3d::ZERO,
                                aspect: wgpu::TextureAspect::All,
                            },
                            // The actual pixel data
                            data,
                            // The layout of the texture
                            wgpu::ImageDataLayout {
                                offset: 0,
                                bytes_per_row: Some((width * channels).into()),
                                rows_per_image: Some(height.into()),
                            },
                            texture_size,
                        );
                    }

                    self.textures.insert(id, texture);
                }
                TextureEvent::UpdateTexture {
                    id,
                    format,
                    x,
                    y,
                    width,
                    height,
                    data,
                } => {
                    log::info!("rich_text::UpdateTexture id ({:?})", id);
                    if let Some(texture) = self.textures.get(&id) {
                        self.bind_group_needs_update = true;
                        let texture_size = wgpu::Extent3d {
                            width: width.into(),
                            height: height.into(),
                            depth_or_array_layers: 1,
                        };

                        let channels = match format {
                            // Mask
                            image_cache::PixelFormat::A8 => 1,
                            // Color
                            image_cache::PixelFormat::Rgba8 => 4,
                        };

                        ctx.queue.write_texture(
                            // Tells wgpu where to copy the pixel data
                            wgpu::ImageCopyTexture {
                                texture,
                                mip_level: 0,
                                origin: wgpu::Origin3d {
                                    x: u32::from(x),
                                    y: u32::from(y),
                                    z: 0,
                                },
                                aspect: wgpu::TextureAspect::All,
                            },
                            // The actual pixel data
                            data,
                            // The layout of the texture
                            wgpu::ImageDataLayout {
                                offset: 0,
                                bytes_per_row: Some((width * channels).into()),
                                rows_per_image: Some(height.into()),
                            },
                            texture_size,
                        );
                    }
                }
                TextureEvent::DestroyTexture(id) => {
                    log::info!("rich_text::DestroyTexture id ({:?})", id);
                    self.textures.remove(&id);
                }
            }
        });
    }
}

#[inline]
fn draw_layout(
    comp: &mut compositor::Compositor,
    render_data: &crate::layout::RenderData,
    x: f32,
    y: f32,
    font_library: &FontLibraryData,
    _rect: SugarDimensions,
) {
    let depth = 0.0;
    let mut glyphs = Vec::new();
    for line in render_data.lines() {
        let mut px = x + line.offset();
        for run in line.runs() {
            let mut font = *run.font();
            if font == 0 {
                font = run.font_id_based_on_attr();
            }

            let py = line.baseline() + y;
            let run_x = px;
            glyphs.clear();
            for cluster in run.visual_clusters() {
                for glyph in cluster.glyphs() {
                    let x = px + glyph.x;
                    let y = py - glyph.y;
                    px += glyph.advance;
                    glyphs.push(Glyph { id: glyph.id, x, y });
                }
            }
            let color = run.color();

            let line_height = line.ascent() + line.descent() + line.leading();
            let style = TextRunStyle {
                font: font_library[font].as_ref(),
                font_coords: run.normalized_coords(),
                font_size: run.font_size(),
                color,
                cursor: run.cursor(),
                background_color: run.background_color(),
                baseline: py,
                topline: py - line.ascent(),
                line_height,
                advance: px - run_x,
                underline: if run.underline() {
                    Some(UnderlineStyle {
                        offset: run.underline_offset(),
                        size: run.underline_size(),
                        color: run.underline_color(),
                    })
                } else {
                    None
                },
            };

            comp.draw_glyphs(
                Rect::new(run_x, py, style.advance, 1.),
                depth,
                &style,
                glyphs.iter(),
            );
        }
    }
}

#[inline]
fn fetch_dimensions(
    comp: &mut compositor::Compositor,
    render_data: &crate::layout::RenderData,
    font_library: &FontLibraryData,
) -> SugarDimensions {
    let x = 0.;
    let y = 0.;
    let mut glyphs = Vec::with_capacity(3);
    let mut dimension = SugarDimensions::default();
    for line in render_data.lines() {
        let mut px = x + line.offset();
        for run in line.runs() {
            let font = run.font();
            let py = line.baseline() + y;
            let run_x = px;
            let line_height = line.ascent() + line.descent() + line.leading();
            glyphs.clear();
            for cluster in run.visual_clusters() {
                for glyph in cluster.glyphs() {
                    let x = px + glyph.x;
                    let y = py - glyph.y;
                    px += glyph.advance;
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
                underline: None,
            };

            if style.advance > 0. && line_height > 0. {
                dimension.width = style.advance;
                dimension.height = line_height;
            }

            comp.draw_glyphs(
                Rect::new(run_x, py, style.advance, 1.),
                0.0,
                &style,
                glyphs.iter(),
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
