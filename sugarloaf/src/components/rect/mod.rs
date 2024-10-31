use crate::components::core::{orthographic_projection, uniforms::Uniforms};
use crate::context::Context;
use bytemuck::{Pod, Zeroable};
use std::{borrow::Cow, mem};
use wgpu::util::DeviceExt;

const INITIAL_QUANTITY: usize = 6;

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct Vertex {
    _position: [f32; 2],
}

fn vertex(pos: [f32; 2]) -> Vertex {
    Vertex {
        _position: [pos[0], pos[1]],
    }
}

const QUAD_INDICES: [u16; 6] = [0, 1, 2, 0, 2, 3];

#[derive(Debug, PartialEq, Default, Clone, Copy)]
#[repr(C)]
pub struct Rect {
    /// The position of the [`Rect`].
    pub position: [f32; 2],
    pub color: [f32; 4],
    pub size: [f32; 2],
}

#[allow(unsafe_code)]
unsafe impl bytemuck::Zeroable for Rect {}

#[allow(unsafe_code)]
unsafe impl bytemuck::Pod for Rect {}

// TODO: Implement square
fn create_vertices_rect() -> Vec<Vertex> {
    let vertex_data = [
        vertex([0.0, 0.0]),
        vertex([0.5, 0.0]),
        vertex([0.5, 1.0]),
        vertex([0.0, 1.0]),
    ];

    vertex_data.to_vec()
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

pub struct RectBrush {
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    instances: wgpu::Buffer,
    index_count: usize,
    bind_group: wgpu::BindGroup,
    transform: wgpu::Buffer,
    pipeline: wgpu::RenderPipeline,
    current_transform: [f32; 16],
    supported_quantity: usize,
}

impl RectBrush {
    pub fn init(context: &Context) -> Self {
        let device = &context.device;
        let vertex_data = create_vertices_rect();

        let transform = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: mem::size_of::<Uniforms>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertex_data),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(&QUAD_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        // Create pipeline layout
        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[wgpu::BindGroupLayoutEntry {
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
                }],
            });
        let pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        // Create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &transform,
                    offset: 0,
                    size: None,
                }),
            }],
            label: Some("rect::Pipeline uniforms"),
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("rect.wgsl"))),
        });

        let vertex_buffers = [
            wgpu::VertexBufferLayout {
                array_stride: mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                }],
            },
            wgpu::VertexBufferLayout {
                array_stride: mem::size_of::<Rect>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array!(
                    1 => Float32x2,
                    2 => Float32x4,
                    3 => Float32x2,
                ),
            },
        ];

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            cache: None,
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &vertex_buffers,
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
                front_face: wgpu::FrontFace::Cw,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });
        let supported_quantity = INITIAL_QUANTITY;
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Instances Buffer"),
            size: mem::size_of::<Rect>() as u64 * supported_quantity as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Done
        RectBrush {
            vertex_buf,
            index_buf,
            index_count: QUAD_INDICES.len(),
            bind_group,
            transform,
            pipeline,
            current_transform: [0.0; 16],
            instances,
            supported_quantity,
        }
    }

    #[inline]
    pub fn resize(&mut self, ctx: &mut Context) {
        let transform: [f32; 16] =
            orthographic_projection(ctx.size.width, ctx.size.height);
        // device.push_error_scope(wgpu::ErrorFilter::Validation);
        let scale = ctx.scale;
        let queue = &mut ctx.queue;

        if transform != self.current_transform {
            let uniforms = Uniforms::new(transform, scale);

            queue.write_buffer(&self.transform, 0, bytemuck::bytes_of(&uniforms));

            self.current_transform = transform;
        }
    }

    #[inline]
    pub fn render<'pass>(
        &'pass mut self,
        rpass: &mut wgpu::RenderPass<'pass>,
        state: &crate::sugarloaf::state::SugarState,
        ctx: &mut Context,
    ) {
        // let device = &ctx.device;
        let instances = &state.compositors.elementary.rects;
        let mut i = 0;
        let total = instances.len();

        if total == 0 {
            return;
        }

        if total > self.supported_quantity {
            self.instances.destroy();

            self.supported_quantity = total;
            self.instances = ctx.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sugarloaf::rect::Rect instances"),
                size: mem::size_of::<Rect>() as u64 * self.supported_quantity as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.bind_group, &[]);
        rpass.set_index_buffer(self.index_buf.slice(..), wgpu::IndexFormat::Uint16);
        rpass.set_vertex_buffer(0, self.vertex_buf.slice(..));
        rpass.set_vertex_buffer(1, self.instances.slice(..));

        let queue = &mut ctx.queue;
        while i < total {
            let end = (i + self.supported_quantity).min(total);
            let amount = end - i;

            let instance_bytes = bytemuck::cast_slice(&instances[i..end]);

            queue.write_buffer(&self.instances, 0, instance_bytes);
            rpass.draw_indexed(0..self.index_count as u32, 0, 0..amount as u32);
            i += self.supported_quantity;
        }

        // queue.submit(Some(encoder.finish()));
    }
}
