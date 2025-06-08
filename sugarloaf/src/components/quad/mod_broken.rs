use crate::backend::RenderBackend;
use crate::components::core::{orthographic_projection, uniforms::Uniforms};
use crate::context::Context;
use crate::sugarloaf::state::SugarState;

use bytemuck::{Pod, Zeroable};
use std::mem;

/// The background of some element.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Background {
    /// A composed_quad color.
    Color([f32; 4]),
}

const INITIAL_QUANTITY: usize = 2;

/// The properties of a quad.
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq, Default)]
#[repr(C)]
pub struct Quad {
    /// The background color data of the quad.
    pub color: [f32; 4],

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

pub enum QuadPipeline {
    WebGpu(wgpu::RenderPipeline),
    #[cfg(feature = "native-metal")]
    Metal(metal::RenderPipelineState),
}

pub enum QuadBuffer {
    WebGpu(wgpu::Buffer),
    #[cfg(feature = "native-metal")]
    Metal(metal::Buffer),
}

pub enum QuadBindGroup {
    WebGpu(wgpu::BindGroup),
    #[cfg(feature = "native-metal")]
    Metal(()), // Metal doesn't use bind groups
}

#[derive(Debug)]
pub struct QuadBrush {
    pipeline: QuadPipeline,
    current_transform: [f32; 16],
    constants: QuadBindGroup,
    transform: QuadBuffer,
    instances: QuadBuffer,
    supported_quantity: usize,
    backend: RenderBackend,
}

impl QuadBrush {
    pub fn new(context: &Context) -> QuadBrush {
        let backend = context.render_backend;
        
        match backend {
            RenderBackend::WebGpu => Self::new_webgpu(context),
            #[cfg(feature = "native-metal")]
            RenderBackend::Metal => Self::new_metal(context),
        }
    }

    fn new_webgpu(context: &Context) -> QuadBrush {
        let supported_quantity = INITIAL_QUANTITY;
        let instances = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sugarloaf::quad Instances Buffer"),
            size: mem::size_of::<Quad>() as u64 * supported_quantity as u64,
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

        let layout = context
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("sugarloaf::quad pipeline layout"),
                bind_group_layouts: &[&constant_layout],
                push_constant_ranges: &[],
            });

        let shader = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("sugarloaf::quad shader"),
                source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!(
                    "quad_f32_combined.wgsl"
                ))),
            });

        let pipeline = context
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("sugarloaf::quad pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: "vs_main",
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: mem::size_of::<Quad>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![
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
                        ],
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                    }],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: "fs_main",
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

        QuadBrush {
            pipeline: QuadPipeline::WebGpu(pipeline),
            current_transform: [0.0; 16],
            constants: QuadBindGroup::WebGpu(constants),
            transform: QuadBuffer::WebGpu(transform),
            instances: QuadBuffer::WebGpu(instances),
            supported_quantity,
            backend: RenderBackend::WebGpu,
        }
    }

    #[cfg(feature = "native-metal")]
    fn new_metal(context: &Context) -> QuadBrush {
        let supported_quantity = INITIAL_QUANTITY;
        
        if let Some(metal_ctx) = context.metal_context() {
            // Create Metal buffers
            let instance_data = vec![0u8; mem::size_of::<Quad>() * supported_quantity];
            let instances = metal_ctx.create_buffer(&instance_data, metal::MTLResourceOptions::StorageModeShared);
            
            let transform_data = vec![0u8; mem::size_of::<Uniforms>()];
            let transform = metal_ctx.create_buffer(&transform_data, metal::MTLResourceOptions::StorageModeShared);

            // Create Metal shader library and pipeline
            let shader_source = include_str!("../shaders/quad.metal");
            let library = metal_ctx.create_library_from_source(shader_source)
                .expect("Failed to create Metal shader library");
            
            let vertex_function = library.get_function("vertex_main", None).unwrap();
            let fragment_function = library.get_function("fragment_main", None).unwrap();

            let pipeline_descriptor = metal::RenderPipelineDescriptor::new();
            pipeline_descriptor.set_vertex_function(Some(&vertex_function));
            pipeline_descriptor.set_fragment_function(Some(&fragment_function));
            
            // Set up vertex descriptor
            let vertex_descriptor = metal::VertexDescriptor::new();
            let attributes = vertex_descriptor.attributes();
            let layouts = vertex_descriptor.layouts();
            
            // Position attribute
            attributes.object_at(0).unwrap().set_format(metal::MTLVertexFormat::Float2);
            attributes.object_at(0).unwrap().set_offset(0);
            attributes.object_at(0).unwrap().set_buffer_index(0);
            
            // Color attribute  
            attributes.object_at(1).unwrap().set_format(metal::MTLVertexFormat::Float4);
            attributes.object_at(1).unwrap().set_offset(8);
            attributes.object_at(1).unwrap().set_buffer_index(0);
            
            layouts.object_at(0).unwrap().set_stride(mem::size_of::<Quad>() as u64);
            layouts.object_at(0).unwrap().set_step_rate(1);
            layouts.object_at(0).unwrap().set_step_function(metal::MTLVertexStepFunction::PerInstance);
            
            pipeline_descriptor.set_vertex_descriptor(Some(&vertex_descriptor));
            
            // Set color attachment format
            let color_attachments = pipeline_descriptor.color_attachments();
            color_attachments.object_at(0).unwrap().set_pixel_format(metal::MTLPixelFormat::BGRA8Unorm);
            color_attachments.object_at(0).unwrap().set_blending_enabled(true);
            color_attachments.object_at(0).unwrap().set_source_rgb_blend_factor(metal::MTLBlendFactor::SourceAlpha);
            color_attachments.object_at(0).unwrap().set_destination_rgb_blend_factor(metal::MTLBlendFactor::OneMinusSourceAlpha);
            
            let pipeline = metal_ctx.create_render_pipeline(&pipeline_descriptor)
                .expect("Failed to create Metal render pipeline");

            QuadBrush {
                pipeline: QuadPipeline::Metal(pipeline),
                current_transform: [0.0; 16],
                constants: QuadBindGroup::Metal(()),
                transform: QuadBuffer::Metal(transform),
                instances: QuadBuffer::Metal(instances),
                supported_quantity,
                backend: RenderBackend::Metal,
            }
        } else {
            panic!("Metal context not available");
        }
    }

    #[cfg(not(feature = "native-metal"))]
    fn new_metal(_context: &Context) -> QuadBrush {
        panic!("Metal backend not available - compile with native-metal feature");
    }

    pub fn render(
        &mut self,
        context: &mut Context,
        state: &SugarState,
        render_pass: &mut wgpu::RenderPass,
    ) {
        match self.backend {
            RenderBackend::WebGpu => self.render_webgpu(context, state, render_pass),
            #[cfg(feature = "native-metal")]
            RenderBackend::Metal => {
                // For now, Metal rendering is not fully implemented in this render method
                // This would need a Metal-specific render pass
                tracing::debug!("Metal quad rendering not yet implemented in this render method");
            }
        }
    }

    fn render_webgpu(
        &mut self,
        context: &mut Context,
        state: &SugarState,
        render_pass: &mut wgpu::RenderPass,
    ) {
        if state.quads.is_empty() {
            return;
        }

        let transform = orthographic_projection(context.size().width, context.size().height);

        if self.current_transform != transform {
            let transform_uniforms = Uniforms { transform };

            if let QuadBuffer::WebGpu(ref transform_buffer) = self.transform {
                context.queue.write_buffer(
                    transform_buffer,
                    0,
                    bytemuck::cast_slice(&[transform_uniforms]),
                );
            }

            self.current_transform = transform;
        }

        let quantity = state.quads.len();
        if quantity > self.supported_quantity {
            if let QuadBuffer::WebGpu(ref instances_buffer) = self.instances {
                self.instances = QuadBuffer::WebGpu(context.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("sugarloaf::quad Instances Buffer"),
                    size: mem::size_of::<Quad>() as u64 * quantity as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
            }

            self.supported_quantity = quantity;
        }

        if let QuadBuffer::WebGpu(ref instances_buffer) = self.instances {
            context.queue.write_buffer(
                instances_buffer,
                0,
                bytemuck::cast_slice(&state.quads),
            );
        }

        if let QuadPipeline::WebGpu(ref pipeline) = self.pipeline {
            render_pass.set_pipeline(pipeline);
        }

        if let QuadBindGroup::WebGpu(ref constants) = self.constants {
            render_pass.set_bind_group(0, constants, &[]);
        }

        if let QuadBuffer::WebGpu(ref instances_buffer) = self.instances {
            render_pass.set_vertex_buffer(0, instances_buffer.slice(..));
        }

        render_pass.draw(0..6, 0..quantity as u32);
    }

    #[cfg(feature = "native-metal")]
    pub fn render_metal(
        &mut self,
        context: &mut Context,
        state: &SugarState,
        metal_pass: &mut crate::backend::metal::MetalRenderPass,
    ) {
        if state.quads.is_empty() {
            return;
        }

        let transform = orthographic_projection(context.size().width, context.size().height);

        if self.current_transform != transform {
            let transform_uniforms = Uniforms { transform };

            if let QuadBuffer::Metal(ref transform_buffer) = self.transform {
                // Update Metal buffer with transform data
                // Note: In a real implementation, you'd use Metal's buffer update methods
                tracing::debug!("Updating Metal transform buffer");
            }

            self.current_transform = transform;
        }

        let quantity = state.quads.len();
        if quantity > self.supported_quantity {
            if let Some(metal_ctx) = context.metal_context() {
                let instance_data = vec![0u8; mem::size_of::<Quad>() * quantity];
                self.instances = QuadBuffer::Metal(
                    metal_ctx.create_buffer(&instance_data, metal::MTLResourceOptions::StorageModeShared)
                );
                self.supported_quantity = quantity;
            }
        }

        if let QuadBuffer::Metal(ref instances_buffer) = self.instances {
            // Update Metal buffer with quad data
            // Note: In a real implementation, you'd copy the quad data to the Metal buffer
            tracing::debug!("Updating Metal instances buffer with {} quads", quantity);
        }

        if let QuadPipeline::Metal(ref pipeline) = self.pipeline {
            metal_pass.set_render_pipeline_state(pipeline);
        }

        if let QuadBuffer::Metal(ref instances_buffer) = self.instances {
            metal_pass.set_vertex_buffer(0, Some(instances_buffer), 0);
        }

        if let QuadBuffer::Metal(ref transform_buffer) = self.transform {
            metal_pass.set_vertex_buffer(1, Some(transform_buffer), 0);
        }

        metal_pass.draw_primitives(metal::MTLPrimitiveType::Triangle, 0, 6 * quantity as u64);
    }
}