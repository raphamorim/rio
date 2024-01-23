mod batch;
pub mod color;
mod compositor;
pub mod doc;
mod image_cache;
pub mod layout;
pub mod text;
pub mod util;

use crate::components::core::orthographic_projection;
use crate::context::Context;
use bytemuck::Pod;
use bytemuck::Zeroable;
use color::Color;
use compositor::{
    Command, Compositor, DisplayList, Rect, TextureEvent, TextureId, Vertex,
};
use layout::*;
use layout::{Direction, LayoutContext, Paragraph, Selection};
use std::collections::HashMap;
use std::{borrow::Cow, mem};
use text::{Glyph, TextRunStyle, UnderlineStyle};
use wgpu::util::DeviceExt;
use wgpu::Texture;

// Note: currently it's using Indexed drawing instead of Instance drawing could be worth to
// evaluate if would make sense move to instance drawing instead
// https://math.hws.edu/graphicsbook/c9/s2.html
// https://docs.rs/wgpu/latest/wgpu/enum.VertexStepMode.html

const IDENTITY_MATRIX: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
struct Uniforms {
    transform: [f32; 16],
    _padding: [f32; 3],
}

impl Uniforms {
    fn new(transformation: [f32; 16]) -> Uniforms {
        Self {
            transform: transformation,
            // Ref: https://github.com/iced-rs/iced/blob/bc62013b6cde52174bf4c4286939cf170bfa7760/wgpu/src/quad.rs#LL295C6-L296C68
            // Uniforms must be aligned to their largest member,
            // this uses a mat4x4<f32> which aligns to 16, so align to that
            _padding: [0.0; 3],
        }
    }
}

impl Default for Uniforms {
    fn default() -> Self {
        Self {
            transform: IDENTITY_MATRIX,
            _padding: [0.0; 3],
        }
    }
}

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
    transform: wgpu::Buffer,
    pipeline: wgpu::RenderPipeline,
    textures: HashMap<TextureId, Texture>,
    index_buffer: wgpu::Buffer,
    index_buffer_size: u64,
    current_transform: [f32; 16],
    comp: Compositor,
    dlist: DisplayList,
    document: doc::Document,
    rich_text_layout: Paragraph,
    rich_text_layout_context: LayoutContext,
    needs_update: bool,
    size_changed: bool,
    first_run: bool,
    selection: Selection,
    selection_rects: Vec<[f32; 4]>,
    selecting: bool,
    selection_changed: bool,
    supported_vertex_buffer: usize,
    align: Alignment,
}

impl RichTextBrush {
    pub fn new(context: &Context) -> Self {
        let device = &context.device;
        let dlist = DisplayList::new();
        let supported_vertex_buffer = 1_000;

        let transform = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: mem::size_of::<Uniforms>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create pipeline layout
        let uniform_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(
                                mem::size_of::<Uniforms>() as wgpu::BufferAddress,
                            ),
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
                bind_group_layouts: &[&uniform_layout],
            });

        let color_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("texture view"),
            size: wgpu::Extent3d {
                width: context.size.width,
                height: context.size.height,
                depth_or_array_layers: 1,
            },
            view_formats: &[],
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            mip_level_count: 1,
            sample_count: 1,
        });
        let color_texture_view =
            color_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mask_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("texture view"),
            size: wgpu::Extent3d {
                width: context.size.width,
                height: context.size.height,
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
            // mag_filter: wgpu::FilterMode::Linear,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &uniform_layout,
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
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: context.format,
                    blend: BLEND,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            // primitive: wgpu::PrimitiveState {
            //     topology: wgpu::PrimitiveTopology::TriangleList,
            //     strip_index_format: None,
            //     front_face: wgpu::FrontFace::Ccw,
            //     cull_mode: None,
            //     polygon_mode: wgpu::PolygonMode::Fill,
            //     unclipped_depth: false,
            //     conservative: false,
            // },
            // primitive: wgpu::PrimitiveState {
            //     topology: wgpu::PrimitiveTopology::TriangleStrip,
            //     front_face: wgpu::FrontFace::Cw,
            //     strip_index_format: Some(wgpu::IndexFormat::Uint32),
            //     ..Default::default()
            // },
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

        let rich_text_layout = Paragraph::new();
        let document = build_document();
        let fonts = layout::FontLibrary::default();
        let rich_text_layout_context = LayoutContext::new(&fonts);

        RichTextBrush {
            index_buffer_size,
            index_buffer,
            textures: HashMap::default(),
            comp: Compositor::new(2048),
            dlist,
            rich_text_layout,
            rich_text_layout_context,
            document,
            bind_group,
            transform,
            pipeline,
            vertex_buffer,
            needs_update: false,
            size_changed: false,
            first_run: true,
            selection: Selection::default(),
            selection_rects: Vec::new(),
            selecting: false,
            selection_changed: false,
            align: Alignment::Start,
            supported_vertex_buffer,
            current_transform: IDENTITY_MATRIX,
        }
    }

    pub fn render(
        &mut self,
        ctx: &mut Context,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
    ) {
        let margin = 12. * ctx.scale;

        if self.first_run {
            self.needs_update = true;
        }
        let w = ctx.size.width;
        let _h = ctx.size.height;
        if self.needs_update {
            let mut lb = self.rich_text_layout_context.builder(
                Direction::LeftToRight,
                None,
                ctx.scale,
            );
            self.document.layout(&mut lb);
            self.rich_text_layout.clear();
            lb.build_into(&mut self.rich_text_layout);

            if self.first_run {
                self.selection = Selection::from_point(&self.rich_text_layout, 0., 0.);
            }

            self.first_run = false;
            self.needs_update = false;
            self.size_changed = true;
        }

        if self.size_changed {
            let lw = w as f32 - margin * ctx.scale;
            self.rich_text_layout
                .break_lines()
                .break_remaining(lw, self.align);
            self.size_changed = false;
            self.selection_changed = true;
        }

        let inserted = None;
        if let Some(offs) = inserted {
            self.selection = Selection::from_offset(&self.rich_text_layout, offs);
        }
        // inserted = None;

        if self.selection_changed {
            self.selection_rects.clear();
            self.selection.regions_with(&self.rich_text_layout, |r| {
                self.selection_rects.push(r);
            });
            self.selection_changed = false;
        }

        // Render
        self.comp.begin();
        let depth = 0.0;
        draw_layout(
            &mut self.comp,
            &self.rich_text_layout,
            margin,
            margin,
            depth,
            color::WHITE,
        );

        for r in &self.selection_rects {
            let rect = [r[0] + margin, r[1] + margin, r[2], r[3]];
            self.comp
                .draw_rect(rect, 600., Color::new(38, 79, 120, 255));
        }

        let (pt, ch, _rtl) = self.selection.cursor(&self.rich_text_layout);
        if ch != 0. {
            let rect = [
                pt[0].round() + margin,
                pt[1].round() + margin,
                1. * ctx.scale,
                ch,
            ];
            self.comp
                .draw_rect(rect, 0.1, Color::new(255, 255, 255, 255));
        }
        self.dlist.clear();
        self.finish_composition(ctx);

        let vertices: &[Vertex] = self.dlist.vertices();
        let indices: &[u32] = self.dlist.indices();

        let queue = &mut ctx.queue;

        let transform = orthographic_projection(ctx.size.width, ctx.size.height);
        if transform != self.current_transform {
            let uniforms = Uniforms::new(transform);
            queue.write_buffer(&self.transform, 0, bytemuck::bytes_of(&uniforms));

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

        let vertices_bytes: &[u8] = bytemuck::cast_slice(&vertices);
        if !vertices_bytes.is_empty() {
            self.vertex_buffer =
                ctx.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("sugarloaf::rich_text::Pipeline vertices"),
                        contents: vertices_bytes,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    });

            queue.write_buffer(&self.vertex_buffer, 0, vertices_bytes);

            // encoder.copy_buffer_to_buffer(
            //     &vertices_buffer,
            //     0,
            //     &self.vertex_buffer,
            //     0,
            //     mem::size_of::<Vertex>() as u64 * vertices.len() as u64,
            // );
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

        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
        });

        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.bind_group, &[]);
        rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        rpass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);

        // indices, base_vertex, instances
        rpass.draw_indexed(0..(vertices.len() as u32), 0, 0..1);

        // rpass.set_blend_constant

        // for command in self.dlist.commands() {
        //     match command {
        //         Command::BindPipeline(pipeline) => {
        //             println!("BindPipeline {:?}",pipeline);
        //             // match pipeline {
        //             //     Pipeline::Opaque => {
        //             //         unsafe {
        //             //             gl::DepthMask(1);
        //             //             gl::Disable(gl::BLEND);
        //             //             gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
        //             //             gl::BlendEquation(gl::FUNC_ADD);
        //             //         }
        //             //         self.base_shader.activate();
        //             //         self.base_shader.bind_attribs();
        //             //         self.base_shader.set_view_proj(&view_proj);
        //             //     }
        //             //     Pipeline::Transparent => {
        //             //         unsafe {
        //             //             gl::DepthMask(0);
        //             //             gl::Enable(gl::BLEND);
        //             //             gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
        //             //         }
        //             //         self.base_shader.activate();
        //             //         self.base_shader.bind_attribs();
        //             //         self.base_shader.set_view_proj(&view_proj);
        //             //     }
        //             //     Pipeline::Subpixel => {
        //             //         unsafe {
        //             //             gl::DepthMask(0);
        //             //             gl::Enable(gl::BLEND);
        //             //             gl::BlendFunc(gl::SRC1_COLOR, gl::ONE_MINUS_SRC1_COLOR);
        //             //         }
        //             //         self.subpx_shader.activate();
        //             //         self.subpx_shader.bind_attribs();
        //             //         self.subpx_shader.set_view_proj(&view_proj);
        //             //     }

        //             // }
        //         }
        //         Command::BindTexture(unit, id) => {
        //             println!("BindTexture {:?} {:?}",unit, id);
        //             // if let Some(tex) = self.textures.get(&id) {
        //             //     tex.bind(*unit);
        //             // }
        //         }
        //         Command::Draw { start, count } => {
        //             let end = start + count;
        //             rpass.draw(*start..end, 0..(vertices.len() as u32));
        //         }
        //     }
        // }

        drop(rpass);
    }

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
                            &data,
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
                    x,
                    y,
                    width,
                    height,
                    data,
                } => {
                    if let Some(texture) = self.textures.get(&id) {
                        let texture_size = wgpu::Extent3d {
                            width: width.into(),
                            height: height.into(),
                            depth_or_array_layers: 1,
                        };

                        // let channels = match format {
                        //     // Mask
                        //     image_cache::PixelFormat::A8 => 1,
                        //     // Color
                        //     image_cache::PixelFormat::Rgba8 => 4,
                        // };

                        ctx.queue.write_texture(
                            // Tells wgpu where to copy the pixel data
                            wgpu::ImageCopyTexture {
                                texture: &texture,
                                mip_level: 0,
                                origin: wgpu::Origin3d {
                                    x: u32::from(x),
                                    y: u32::from(y),
                                    z: 0,
                                },
                                aspect: wgpu::TextureAspect::All,
                            },
                            // The actual pixel data
                            &data,
                            // The layout of the texture
                            wgpu::ImageDataLayout {
                                offset: 0,
                                bytes_per_row: Some((width * 4).into()),
                                rows_per_image: Some(height.into()),
                            },
                            texture_size,
                        );
                    }
                }
                TextureEvent::DestroyTexture(id) => {
                    self.textures.remove(&id);
                }
            }
        });
    }
}

fn draw_layout(
    comp: &mut compositor::Compositor,
    layout: &Paragraph,
    x: f32,
    y: f32,
    depth: f32,
    color: Color,
) {
    let mut glyphs = Vec::new();
    for line in layout.lines() {
        let mut px = x + line.offset();
        for run in line.runs() {
            let font = run.font();
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
            let style = TextRunStyle {
                font: font.as_ref(),
                font_coords: run.normalized_coords(),
                font_size: run.font_size(),
                color,
                baseline: py,
                advance: px - run_x,
                underline: if run.underline() {
                    Some(UnderlineStyle {
                        offset: run.underline_offset(),
                        size: run.underline_size(),
                        color,
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

fn build_document() -> doc::Document {
    use layout::*;
    let mut db = doc::Document::builder();

    use SpanStyle as S;

    let underline = &[
        S::Underline(true),
        S::UnderlineOffset(Some(-1.)),
        S::UnderlineSize(Some(1.)),
    ];

    db.enter_span(&[
        S::family_list("Victor Mono, times, georgia, serif"),
        S::Size(18.),
        S::features(&[("dlig", 1).into(), ("hlig", 1).into()][..]),
    ]);
    db.enter_span(&[S::Size(48.)]);
    db.add_text("rio");
    db.leave_span();
    db.enter_span(&[S::LineSpacing(1.2)]);
    db.enter_span(&[S::family_list("fira code, serif"), S::Size(22.)]);
    db.add_text("According to Wikipedia, the foremost expert on any subject,\n\n");
    db.leave_span();
    db.enter_span(&[S::Weight(Weight::BOLD)]);
    db.add_text("Typography");
    db.leave_span();
    db.add_text(" is the ");
    db.enter_span(&[S::Style(Style::Italic)]);
    db.add_text("art and technique");
    db.leave_span();
    db.add_text(" of arranging type to make ");
    db.enter_span(underline);
    db.add_text("written language");
    db.leave_span();
    db.add_text(" ");
    db.enter_span(underline);
    db.add_text("legible");
    db.leave_span();
    db.add_text(", ");
    db.enter_span(underline);
    db.add_text("readable");
    db.leave_span();
    db.add_text(" and ");
    db.enter_span(underline);
    db.add_text("appealing");
    db.leave_span();
    db.enter_span(&[S::LineSpacing(1.)]);
    db.add_text(
        " Furthermore, العربية نص جميل. द क्विक ब्राउन फ़ॉक्स jumps over the lazy 🐕.\n\n",
    );
    db.leave_span();
    db.enter_span(&[S::family_list("verdana, sans-serif"), S::LineSpacing(1.)]);
    db.add_text("A true ");
    db.enter_span(&[S::Size(48.)]);
    db.add_text("🕵🏽‍♀️");
    db.leave_span();
    db.add_text(" will spot the tricky selection in this BiDi text: ");
    db.enter_span(&[S::Size(22.)]);
    db.add_text("ניפגש ב09:35 בחוף הים");
    db.leave_span();
    db.build()
}

fn next_copy_buffer_size(size: u64) -> u64 {
    let align_mask = wgpu::COPY_BUFFER_ALIGNMENT - 1;
    ((size.next_power_of_two() + align_mask) & !align_mask)
        .max(wgpu::COPY_BUFFER_ALIGNMENT)
}
