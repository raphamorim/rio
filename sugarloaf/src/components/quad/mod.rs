use crate::components::core::{orthographic_projection, uniforms::Uniforms};
use crate::context::metal::MetalContext;
use crate::context::webgpu::WgpuContext;
use crate::context::Context;
use crate::context::ContextType;

use bytemuck::{Pod, Zeroable};
use metal::*;

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

#[derive(Debug)]
pub enum BrushType {
    Wgpu(WgpuQuadBrush),
    Metal(MetalQuadBrush),
}

#[derive(Debug)]
pub struct WgpuQuadBrush {
    pipeline: wgpu::RenderPipeline,
    constants: wgpu::BindGroup,
    transform: wgpu::Buffer,
    instances: wgpu::Buffer,
    supported_quantity: usize,
}

#[derive(Debug)]
pub struct MetalQuadBrush {
    pipeline_state: RenderPipelineState,
    vertex_buffer: Buffer,
    uniform_buffer: Buffer,
    supported_quantity: usize,
}

#[derive(Debug)]
pub struct QuadBrush {
    current_transform: [f32; 16],
    brush_type: BrushType,
}

impl QuadBrush {
    pub fn new(context: &Context) -> QuadBrush {
        let brush_type = match &context.inner {
            ContextType::Wgpu(wgpu_context) => {
                BrushType::Wgpu(WgpuQuadBrush::new(wgpu_context))
            }
            ContextType::Metal(metal_context) => {
                BrushType::Metal(MetalQuadBrush::new(metal_context))
            }
        };

        QuadBrush {
            current_transform: [0.0; 16],
            brush_type,
        }
    }

    pub fn resize(&mut self, ctx: &mut Context) {
        let transform: [f32; 16] = match &ctx.inner {
            ContextType::Wgpu(wgpu_ctx) => {
                orthographic_projection(wgpu_ctx.size.width, wgpu_ctx.size.height)
            }
            ContextType::Metal(metal_ctx) => {
                orthographic_projection(metal_ctx.size.width, metal_ctx.size.height)
            }
        };

        if transform != self.current_transform {
            match &mut self.brush_type {
                BrushType::Wgpu(wgpu_brush) => {
                    let (scale, queue) = match &ctx.inner {
                        ContextType::Wgpu(wgpu_ctx) => (wgpu_ctx.scale, &wgpu_ctx.queue),
                        _ => unreachable!(),
                    };

                    let uniforms = Uniforms::new(transform, scale);
                    queue.write_buffer(
                        &wgpu_brush.transform,
                        0,
                        bytemuck::bytes_of(&uniforms),
                    );
                }
                BrushType::Metal(metal_brush) => {
                    let scale = match &ctx.inner {
                        ContextType::Metal(metal_ctx) => metal_ctx.scale,
                        _ => unreachable!(),
                    };

                    let uniforms = Uniforms::new(transform, scale);
                    let contents = metal_brush.uniform_buffer.contents() as *mut Uniforms;
                    unsafe {
                        *contents = uniforms;
                    }
                }
            }

            self.current_transform = transform;
        }
    }

    pub fn render_wgpu<'a>(
        &'a mut self,
        context: &mut WgpuContext,
        state: &crate::sugarloaf::state::SugarState,
        render_pass: &mut wgpu::RenderPass<'a>,
    ) {
        if let BrushType::Wgpu(brush) = &mut self.brush_type {
            brush.render(context, state, render_pass);
        }
    }

    pub fn render_metal(
        &mut self,
        context: &MetalContext,
        state: &crate::sugarloaf::state::SugarState,
        render_encoder: &RenderCommandEncoderRef,
    ) {
        if let BrushType::Metal(brush) = &mut self.brush_type {
            brush.render(context, state, render_encoder);
        }
    }
}

impl WgpuQuadBrush {
    pub fn new(context: &WgpuContext) -> WgpuQuadBrush {
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
            size: mem::size_of::<Uniforms>() as u64,
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

        let shader_source = if context.supports_f16() {
            include_str!("./quad_f16.wgsl")
        } else {
            include_str!("./quad_f32_combined.wgsl")
        };

        let shader = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("sugarloaf::quad shader"),
                source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(
                    shader_source,
                )),
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
                            array_stride: std::mem::size_of::<Quad>() as u64,
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

        WgpuQuadBrush {
            supported_quantity,
            instances,
            constants,
            transform,
            pipeline,
        }
    }

    pub fn render<'a>(
        &'a mut self,
        context: &mut WgpuContext,
        state: &crate::sugarloaf::state::SugarState,
        render_pass: &mut wgpu::RenderPass<'a>,
    ) {
        let instances = &state.quads;
        let total = instances.len();

        if total == 0 {
            return;
        }

        if total > self.supported_quantity {
            self.instances.destroy();

            self.supported_quantity = total;
            self.instances = context.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sugarloaf::quad instances"),
                size: mem::size_of::<Quad>() as u64 * self.supported_quantity as u64,
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

impl MetalQuadBrush {
    pub fn new(context: &MetalContext) -> MetalQuadBrush {
        let supported_quantity = INITIAL_QUANTITY;

        // Create vertex buffer for quad instances
        let vertex_buffer = context.device.new_buffer(
            (mem::size_of::<Quad>() * supported_quantity) as u64,
            MTLResourceOptions::StorageModeShared,
        );
        vertex_buffer.set_label("sugarloaf::quad vertex buffer");

        // Create uniform buffer (back to Uniforms like WGPU)
        let uniform_buffer = context.device.new_buffer(
            mem::size_of::<Uniforms>() as u64,
            MTLResourceOptions::StorageModeShared,
        );
        uniform_buffer.set_label("sugarloaf::quad uniform buffer");

        // Create shader library from the Metal shader source
        let shader_source = include_str!("./quad.metal");

        let library = context
            .device
            .new_library_with_source(shader_source, &CompileOptions::new())
            .expect("Failed to create shader library");

        // let function_names = library.function_names();
        // println!("Available Metal functions: {:?}", function_names);

        let vertex_function = library
            .get_function("vertex_main", None)
            .expect("Failed to get vertex function");
        let fragment_function = library
            .get_function("fragment_main", None)
            .expect("Failed to get fragment function");

        // Create vertex descriptor for proper instanced rendering
        let vertex_descriptor = VertexDescriptor::new();
        let attributes = vertex_descriptor.attributes();

        // Per-instance attributes (Quad data)
        // Color (attribute 0) - [f32; 4]
        attributes
            .object_at(0)
            .unwrap()
            .set_format(MTLVertexFormat::Float4);
        attributes.object_at(0).unwrap().set_offset(0);
        attributes.object_at(0).unwrap().set_buffer_index(0);

        // Position (attribute 1) - [f32; 2]
        attributes
            .object_at(1)
            .unwrap()
            .set_format(MTLVertexFormat::Float2);
        attributes.object_at(1).unwrap().set_offset(16);
        attributes.object_at(1).unwrap().set_buffer_index(0);

        // Size (attribute 2) - [f32; 2]
        attributes
            .object_at(2)
            .unwrap()
            .set_format(MTLVertexFormat::Float2);
        attributes.object_at(2).unwrap().set_offset(24);
        attributes.object_at(2).unwrap().set_buffer_index(0);

        // Border color (attribute 3) - [f32; 4]
        attributes
            .object_at(3)
            .unwrap()
            .set_format(MTLVertexFormat::Float4);
        attributes.object_at(3).unwrap().set_offset(32);
        attributes.object_at(3).unwrap().set_buffer_index(0);

        // Border radius (attribute 4) - [f32; 4]
        attributes
            .object_at(4)
            .unwrap()
            .set_format(MTLVertexFormat::Float4);
        attributes.object_at(4).unwrap().set_offset(48);
        attributes.object_at(4).unwrap().set_buffer_index(0);

        // Border width (attribute 5) - f32
        attributes
            .object_at(5)
            .unwrap()
            .set_format(MTLVertexFormat::Float);
        attributes.object_at(5).unwrap().set_offset(64);
        attributes.object_at(5).unwrap().set_buffer_index(0);

        // Shadow color (attribute 6) - [f32; 4]
        attributes
            .object_at(6)
            .unwrap()
            .set_format(MTLVertexFormat::Float4);
        attributes.object_at(6).unwrap().set_offset(68);
        attributes.object_at(6).unwrap().set_buffer_index(0);

        // Shadow offset (attribute 7) - [f32; 2]
        attributes
            .object_at(7)
            .unwrap()
            .set_format(MTLVertexFormat::Float2);
        attributes.object_at(7).unwrap().set_offset(84);
        attributes.object_at(7).unwrap().set_buffer_index(0);

        // Shadow blur radius (attribute 8) - f32
        attributes
            .object_at(8)
            .unwrap()
            .set_format(MTLVertexFormat::Float);
        attributes.object_at(8).unwrap().set_offset(92);
        attributes.object_at(8).unwrap().set_buffer_index(0);

        // Set up buffer layout for per-instance data
        let layouts = vertex_descriptor.layouts();
        layouts
            .object_at(0)
            .unwrap()
            .set_stride(std::mem::size_of::<Quad>() as u64);
        layouts
            .object_at(0)
            .unwrap()
            .set_step_function(MTLVertexStepFunction::PerInstance);
        layouts.object_at(0).unwrap().set_step_rate(1);

        // Create render pipeline descriptor
        let pipeline_descriptor = RenderPipelineDescriptor::new();
        pipeline_descriptor.set_vertex_function(Some(&vertex_function));
        pipeline_descriptor.set_fragment_function(Some(&fragment_function));
        pipeline_descriptor.set_vertex_descriptor(Some(&vertex_descriptor));

        // Set up color attachment
        let color_attachments = pipeline_descriptor.color_attachments();
        color_attachments
            .object_at(0)
            .unwrap()
            .set_pixel_format(context.layer.pixel_format());
        color_attachments
            .object_at(0)
            .unwrap()
            .set_blending_enabled(true);

        // Set up alpha blending
        color_attachments
            .object_at(0)
            .unwrap()
            .set_source_rgb_blend_factor(MTLBlendFactor::SourceAlpha);
        color_attachments
            .object_at(0)
            .unwrap()
            .set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
        color_attachments
            .object_at(0)
            .unwrap()
            .set_rgb_blend_operation(MTLBlendOperation::Add);

        color_attachments
            .object_at(0)
            .unwrap()
            .set_source_alpha_blend_factor(MTLBlendFactor::One);
        color_attachments
            .object_at(0)
            .unwrap()
            .set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
        color_attachments
            .object_at(0)
            .unwrap()
            .set_alpha_blend_operation(MTLBlendOperation::Add);

        color_attachments
            .object_at(0)
            .unwrap()
            .set_write_mask(MTLColorWriteMask::All);

        // Create the pipeline state
        let pipeline_state = context
            .device
            .new_render_pipeline_state(&pipeline_descriptor)
            .expect("Failed to create render pipeline state");

        let metal_brush = MetalQuadBrush {
            pipeline_state,
            vertex_buffer,
            uniform_buffer,
            supported_quantity,
        };

        metal_brush
    }

    pub fn render(
        &mut self,
        context: &MetalContext,
        state: &crate::sugarloaf::state::SugarState,
        render_encoder: &RenderCommandEncoderRef,
    ) {
        let instances = &state.quads;
        let total = instances.len();

        if total == 0 {
            return;
        }

        // Resize buffer if needed
        if total > self.supported_quantity {
            self.supported_quantity = total;
            self.vertex_buffer = context.device.new_buffer(
                (mem::size_of::<Quad>() * self.supported_quantity) as u64,
                MTLResourceOptions::StorageModeShared,
            );
            self.vertex_buffer
                .set_label("sugarloaf::quad vertex buffer");
        }

        if total == 0 {
            return;
        }
        let vertex_data = self.vertex_buffer.contents() as *mut Quad;
        unsafe {
            std::ptr::copy_nonoverlapping(instances.as_ptr(), vertex_data, total);
        }

        // Set up render state with proper vertex descriptor approach
        render_encoder.set_render_pipeline_state(&self.pipeline_state);
        render_encoder.set_vertex_buffer(0, Some(&self.vertex_buffer), 0);
        render_encoder.set_vertex_buffer(1, Some(&self.uniform_buffer), 0);

        // Draw quads (6 vertices per quad instance)
        render_encoder.draw_primitives_instanced(
            MTLPrimitiveType::Triangle,
            0,
            6,
            total as u64,
        );
    }
}
