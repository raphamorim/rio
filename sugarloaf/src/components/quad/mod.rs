use crate::components::core::{orthographic_projection, uniforms::Uniforms};
use crate::components::gradient::LinearGradient;
use crate::context::Context;

use bytemuck::{Pod, Zeroable};

use std::mem;

/// The background of some element.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Background {
    /// A solid color.
    Color([f32; 4]),
    /// A linear gradient.
    Gradient(LinearGradient),
}

/// A quad primitive for rendering rectangles
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Quad {
    /// A solid color quad
    Solid {
        /// The position of the quad
        position: [f32; 2],
        /// The size of the quad
        size: [f32; 2],
        /// The background color
        color: [f32; 4],
        /// The border color
        border_color: [f32; 4],
        /// The border radius for each corner
        border_radius: [f32; 4],
        /// The border width
        border_width: f32,
        /// The shadow color
        shadow_color: [f32; 4],
        /// The shadow offset
        shadow_offset: [f32; 2],
        /// The shadow blur radius
        shadow_blur_radius: f32,
    },
    /// A gradient quad
    Gradient {
        /// The gradient definition
        gradient: LinearGradient,
        /// The position of the quad
        position: [f32; 2],
        /// The size of the quad
        size: [f32; 2],
        /// The border color
        border_color: [f32; 4],
        /// The border radius for each corner
        border_radius: [f32; 4],
        /// The border width
        border_width: f32,
    },
}

impl Quad {
    /// Creates a new solid color quad
    pub fn solid(position: [f32; 2], size: [f32; 2], color: [f32; 4]) -> Self {
        Self::Solid {
            position,
            size,
            color,
            border_color: [0.0; 4],
            border_radius: [0.0; 4],
            border_width: 0.0,
            shadow_color: [0.0; 4],
            shadow_offset: [0.0; 2],
            shadow_blur_radius: 0.0,
        }
    }

    /// Creates a new gradient quad
    pub fn gradient(gradient: LinearGradient, position: [f32; 2], size: [f32; 2]) -> Self {
        Self::Gradient {
            gradient,
            position,
            size,
            border_color: [0.0; 4],
            border_radius: [0.0; 4],
            border_width: 0.0,
        }
    }

    /// Sets the border properties for the quad
    pub fn with_border(mut self, color: [f32; 4], radius: [f32; 4], width: f32) -> Self {
        match &mut self {
            Self::Solid { border_color, border_radius, border_width, .. } => {
                *border_color = color;
                *border_radius = radius;
                *border_width = width;
            }
            Self::Gradient { border_color, border_radius, border_width, .. } => {
                *border_color = color;
                *border_radius = radius;
                *border_width = width;
            }
        }
        self
    }

    /// Sets shadow properties for solid quads
    pub fn with_shadow(mut self, color: [f32; 4], offset: [f32; 2], blur_radius: f32) -> Self {
        if let Self::Solid { shadow_color, shadow_offset, shadow_blur_radius, .. } = &mut self {
            *shadow_color = color;
            *shadow_offset = offset;
            *shadow_blur_radius = blur_radius;
        }
        self
    }
}

impl Default for Quad {
    fn default() -> Self {
        Self::solid([0.0; 2], [0.0; 2], [0.0; 4])
    }
}

/// Internal solid quad representation for GPU
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq, Default)]
#[repr(C)]
struct SolidQuadData {
    /// The background color data of the quad.
    color: [f32; 4],
    /// The position of the quad.
    position: [f32; 2],
    /// The size of the quad.
    size: [f32; 2],
    /// The border color of the quad.
    border_color: [f32; 4],
    /// The border radii of the quad.
    border_radius: [f32; 4],
    /// The border width of the quad.
    border_width: f32,
    /// The shadow color of the quad.
    shadow_color: [f32; 4],
    /// The shadow offset of the quad.
    shadow_offset: [f32; 2],
    /// The shadow blur radius of the quad.
    shadow_blur_radius: f32,
}

/// Internal gradient quad representation for GPU
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq)]
#[repr(C)]
struct GradientQuadData {
    /// Packed gradient colors (8 colors, 2 channels per u32)
    colors_1: [u32; 4],
    colors_2: [u32; 4],
    /// Packed gradient offsets (8 offsets, 2 per u32)
    offsets: [u32; 4],
    /// Gradient direction [start_x, start_y, end_x, end_y]
    direction: [f32; 4],
    /// The position and size of the quad [x, y, width, height]
    position_and_scale: [f32; 4],
    /// The border color of the quad
    border_color: [f32; 4],
    /// The border radii of the quad
    border_radius: [f32; 4],
    /// The border width of the quad
    border_width: f32,
    /// Whether to snap to pixel boundaries
    snap: u32,
}

impl Default for GradientQuadData {
    fn default() -> Self {
        Self {
            colors_1: [0; 4],
            colors_2: [0; 4],
            offsets: [0; 4],
            direction: [0.0; 4],
            position_and_scale: [0.0; 4],
            border_color: [0.0; 4],
            border_radius: [0.0; 4],
            border_width: 0.0,
            snap: 0,
        }
    }
}

impl From<&Quad> for SolidQuadData {
    fn from(quad: &Quad) -> Self {
        match quad {
            Quad::Solid {
                position,
                size,
                color,
                border_color,
                border_radius,
                border_width,
                shadow_color,
                shadow_offset,
                shadow_blur_radius,
            } => Self {
                color: *color,
                position: *position,
                size: *size,
                border_color: *border_color,
                border_radius: *border_radius,
                border_width: *border_width,
                shadow_color: *shadow_color,
                shadow_offset: *shadow_offset,
                shadow_blur_radius: *shadow_blur_radius,
            },
            Quad::Gradient { .. } => {
                // This shouldn't happen, but provide a default
                Self::default()
            }
        }
    }
}

impl From<&Quad> for GradientQuadData {
    fn from(quad: &Quad) -> Self {
        match quad {
            Quad::Gradient {
                gradient,
                position,
                size,
                border_color,
                border_radius,
                border_width,
            } => {
                let packed = gradient.pack();
                Self {
                    colors_1: [
                        packed.colors[0][0], packed.colors[0][1],
                        packed.colors[1][0], packed.colors[1][1],
                    ],
                    colors_2: [
                        packed.colors[2][0], packed.colors[2][1],
                        packed.colors[3][0], packed.colors[3][1],
                    ],
                    offsets: packed.offsets,
                    direction: packed.direction,
                    position_and_scale: [position[0], position[1], size[0], size[1]],
                    border_color: *border_color,
                    border_radius: *border_radius,
                    border_width: *border_width,
                    snap: 1,
                }
            }
            Quad::Solid { .. } => {
                // This shouldn't happen, but provide a default
                Self::default()
            }
        }
    }
}

const INITIAL_QUANTITY: usize = 2;

#[derive(Debug)]
pub struct QuadBrush {
    solid_pipeline: wgpu::RenderPipeline,
    gradient_pipeline: wgpu::RenderPipeline,
    current_transform: [f32; 16],
    constants: wgpu::BindGroup,
    transform: wgpu::Buffer,
    solid_instances: wgpu::Buffer,
    gradient_instances: wgpu::Buffer,
    supported_solid_quantity: usize,
    supported_gradient_quantity: usize,
}

impl QuadBrush {
    pub fn new(context: &Context) -> QuadBrush {
        let supported_solid_quantity = INITIAL_QUANTITY;
        let supported_gradient_quantity = INITIAL_QUANTITY;
        
        let solid_instances = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sugarloaf::quad Solid Instances Buffer"),
            size: mem::size_of::<SolidQuadData>() as u64 * supported_solid_quantity as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let gradient_instances = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sugarloaf::quad Gradient Instances Buffer"),
            size: mem::size_of::<GradientQuadData>() as u64 * supported_gradient_quantity as u64,
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

        let solid_shader_source = if context.supports_f16() {
            include_str!("./quad_f16.wgsl")
        } else {
            include_str!("./quad_f32_combined.wgsl")
        };

        let solid_shader = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("sugarloaf::quad solid shader"),
                source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(
                    solid_shader_source,
                )),
            });

        let gradient_shader = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("sugarloaf::quad gradient shader"),
                source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(
                    include_str!("./gradient.wgsl"),
                )),
            });

        let solid_pipeline =
            context
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    cache: None,
                    label: Some("sugarloaf::quad solid render pipeline"),
                    layout: Some(&layout),
                    vertex: wgpu::VertexState {
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        module: &solid_shader,
                        entry_point: Some("composed_quad_vs_main"),
                        buffers: &[wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<SolidQuadData>() as u64,
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
                        module: &solid_shader,
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

        let gradient_pipeline =
            context
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    cache: None,
                    label: Some("sugarloaf::quad gradient render pipeline"),
                    layout: Some(&layout),
                    vertex: wgpu::VertexState {
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        module: &gradient_shader,
                        entry_point: Some("gradient_vs_main"),
                        buffers: &[wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<GradientQuadData>() as u64,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &wgpu::vertex_attr_array!(
                                // Colors 1
                                0 => Uint32x4,
                                // Colors 2
                                1 => Uint32x4,
                                // Offsets
                                2 => Uint32x4,
                                // Direction
                                3 => Float32x4,
                                // Position and scale
                                4 => Float32x4,
                                // Border color
                                5 => Float32x4,
                                // Border radius
                                6 => Float32x4,
                                // Border width
                                7 => Float32,
                                // Snap
                                8 => Uint32,
                            ),
                        }],
                    },
                    fragment: Some(wgpu::FragmentState {
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        module: &gradient_shader,
                        entry_point: Some("gradient_fs_main"),
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
            supported_solid_quantity,
            supported_gradient_quantity,
            solid_instances,
            gradient_instances,
            constants,
            transform,
            solid_pipeline,
            gradient_pipeline,
            current_transform: [0.0; 16],
        }
    }

    pub fn resize(&mut self, ctx: &mut Context) {
        let transform: [f32; 16] =
            orthographic_projection(ctx.size.width, ctx.size.height);
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
        // Separate quads by type
        let mut solid_quads = Vec::new();
        let mut gradient_quads = Vec::new();

        for quad in &state.quads {
            match quad {
                Quad::Solid { .. } => solid_quads.push(SolidQuadData::from(quad)),
                Quad::Gradient { .. } => gradient_quads.push(GradientQuadData::from(quad)),
            }
        }

        // Render solid quads
        if !solid_quads.is_empty() {
            let total = solid_quads.len();

            if total > self.supported_solid_quantity {
                self.solid_instances.destroy();
                self.supported_solid_quantity = total;
                self.solid_instances = context.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("sugarloaf::quad solid instances"),
                    size: mem::size_of::<SolidQuadData>() as u64 * self.supported_solid_quantity as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
            }

            let instance_bytes = bytemuck::cast_slice(&solid_quads);
            context.queue.write_buffer(&self.solid_instances, 0, instance_bytes);

            render_pass.set_pipeline(&self.solid_pipeline);
            render_pass.set_bind_group(0, &self.constants, &[]);
            render_pass.set_vertex_buffer(0, self.solid_instances.slice(..));
            render_pass.draw(0..6, 0..total as u32);
        }

        // Render gradient quads
        if !gradient_quads.is_empty() {
            let total = gradient_quads.len();

            if total > self.supported_gradient_quantity {
                self.gradient_instances.destroy();
                self.supported_gradient_quantity = total;
                self.gradient_instances = context.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("sugarloaf::quad gradient instances"),
                    size: mem::size_of::<GradientQuadData>() as u64 * self.supported_gradient_quantity as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
            }

            let instance_bytes = bytemuck::cast_slice(&gradient_quads);
            context.queue.write_buffer(&self.gradient_instances, 0, instance_bytes);

            render_pass.set_pipeline(&self.gradient_pipeline);
            render_pass.set_bind_group(0, &self.constants, &[]);
            render_pass.set_vertex_buffer(0, self.gradient_instances.slice(..));
            render_pass.draw(0..6, 0..total as u32);
        }
    }
}
