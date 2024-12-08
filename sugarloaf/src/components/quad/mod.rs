use crate::components::core::{orthographic_projection, uniforms::Uniforms};
use crate::context::Context;

use bytemuck::{Pod, Zeroable};

use std::mem;

/// A quad filled with a ComposedQuad color.
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq)]
#[repr(C)]
pub struct ComposedQuad {
    /// The background color data of the quad.
    pub color: [f32; 4],

    /// The [`Quad`] data of the [`ComposedQuad`].
    pub quad: Quad,
}

/// The background of some element.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Background {
    /// A composed_quad color.
    Color([f32; 4]),
}

const INITIAL_QUANTITY: usize = 2;

/// The properties of a quad.
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq)]
#[repr(C)]
pub struct Quad {
    /// The position of the [`Quad`].
    pub position: [f32; 2],

    /// The size of the [`Quad`].
    pub size: [f32; 2],

    /// The border color of the [`Quad`], in __linear RGB__.
    pub border_color: [f32; 4],

    /// The border radii of the [`Quad`].
    pub border_radius: [f32; 4],

    /// The border width of the [`Quad`].
    pub border_width: f32,

    /// The shadow color of the [`Quad`].
    pub shadow_color: [f32; 4],

    /// The shadow offset of the [`Quad`].
    pub shadow_offset: [f32; 2],

    /// The shadow blur radius of the [`Quad`].
    pub shadow_blur_radius: f32,
}

#[derive(Debug)]
pub struct QuadBrush {
    pipeline: wgpu::RenderPipeline,
    current_transform: [f32; 16],
    constants: wgpu::BindGroup,
    transform: wgpu::Buffer,
    instances: wgpu::Buffer,
    // transform: wgpu::Buffer,
    supported_quantity: usize,
}

impl QuadBrush {
    pub fn new(context: &Context) -> QuadBrush {
        let supported_quantity = INITIAL_QUANTITY;
        let instances = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sugarloaf::quad Instances Buffer"),
            size: mem::size_of::<ComposedQuad>() as u64 * supported_quantity as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let constant_layout =
            context
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("sugarloaf::quad uniforms layout"),
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

        let transform = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sugarloaf::quad uniforms buffer"),
            size: mem::size_of::<Uniforms>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let constants = context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("sugarloaf::quad uniforms bind group"),
                layout: &constant_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: transform.as_entire_binding(),
                }],
            });

        let layout =
            context
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("sugarloaf::quad pipeline"),
                    push_constant_ranges: &[],
                    bind_group_layouts: &[&constant_layout],
                });

        let shader = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("sugarloaf::quad shader"),
                source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(concat!(
                    include_str!("./quad.wgsl"),
                    "\n",
                    include_str!("./vertex.wgsl"),
                    "\n",
                    include_str!("./composed_quad.wgsl"),
                ))),
            });

        let pipeline =
            context
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    cache: None,
                    label: Some("sugarloaf::quad render pipeline"),
                    layout: Some(&layout),
                    vertex: wgpu::VertexState {
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        module: &shader,
                        entry_point: Some("composed_quad_vs_main"),
                        buffers: &[wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<ComposedQuad>() as u64,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &wgpu::vertex_attr_array!(
                                // Color
                                0 => Float32x4,
                                // Position
                                1 => Float32x2,
                                // Size
                                2 => Float32x2,
                                // Border color
                                3 => Float32x4,
                                // Border radius
                                4 => Float32x4,
                                // Border width
                                5 => Float32,
                                // Shadow color
                                6 => Float32x4,
                                // Shadow offset
                                7 => Float32x2,
                                // Shadow blur radius
                                8 => Float32,
                            ),
                        }],
                    },
                    fragment: Some(wgpu::FragmentState {
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        module: &shader,
                        entry_point: Some("composed_quad_fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: context.format,
                            blend: Some(wgpu::BlendState {
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
                            }),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        front_face: wgpu::FrontFace::Cw,
                        ..Default::default()
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState {
                        count: 1,
                        mask: !0,
                        alpha_to_coverage_enabled: false,
                    },
                    multiview: None,
                });

        Self {
            supported_quantity,
            instances,
            constants,
            transform,
            pipeline,
            current_transform: [0.0; 16],
        }
    }

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

    pub fn render<'a>(
        &'a mut self,
        context: &mut Context,
        state: &crate::sugarloaf::state::SugarState,
        render_pass: &mut wgpu::RenderPass<'a>,
    ) {
        let instances = &state.compositors.elementary.quads;
        let total = instances.len();

        if total == 0 {
            return;
        }

        if total > self.supported_quantity {
            self.instances.destroy();

            self.supported_quantity = total;
            self.instances = context.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sugarloaf::quad instances"),
                size: mem::size_of::<ComposedQuad>() as u64
                    * self.supported_quantity as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        let instance_bytes = bytemuck::cast_slice(instances);
        context
            .queue
            .write_buffer(&self.instances, 0, instance_bytes);

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.constants, &[]);
        render_pass.set_vertex_buffer(0, self.instances.slice(..));

        render_pass.draw(0..6, 0..total as u32);
    }
}
