// This code was originally retired from iced-rs, which is licensed
// under MIT license https://github.com/iced-rs/iced/blob/master/LICENSE
// The code has suffered changes to fit on Sugarloaf architecture.

mod composed_quad;

use crate::components::core::orthographic_projection;
use crate::context::Context;
pub use composed_quad::ComposedQuad;

use bytemuck::{Pod, Zeroable};

use std::mem;

/// The background of some element.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Background {
    /// A composed_quad color.
    Color([f32; 4]),
}

const INITIAL_INSTANCES: usize = 2_000;

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
    composed_quad: composed_quad::Pipeline,
    constant_layout: wgpu::BindGroupLayout,
    layers: Vec<Layer>,
    prepare_layer: usize,
}

impl QuadBrush {
    pub fn new(context: &Context) -> QuadBrush {
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

        Self {
            composed_quad: composed_quad::Pipeline::new(
                &context.device,
                context.format,
                &constant_layout,
            ),
            layers: Vec::new(),
            prepare_layer: 0,
            constant_layout,
        }
    }

    pub fn prepare(
        &mut self,
        context: &Context,
        quads: &Batch,
        transformation: [f32; 16],
        scale: f32,
    ) {
        if self.layers.len() <= self.prepare_layer {
            self.layers
                .push(Layer::new(&context.device, &self.constant_layout));
        }

        let layer = &mut self.layers[self.prepare_layer];
        layer.prepare(context, quads, transformation, scale);

        self.prepare_layer += 1;
    }

    pub fn render<'a>(
        &'a self,
        layer: usize,
        quads: &Batch,
        render_pass: &mut wgpu::RenderPass<'a>,
    ) {
        if let Some(layer) = self.layers.get(layer) {
            // render_pass.set_scissor_rect(
            //     bounds.x,
            //     bounds.y,
            //     bounds.width,
            //     bounds.height,
            // );

            self.composed_quad
                .render(render_pass, &layer.constants, &layer.composed_quad);
        }
    }

    pub fn end_frame(&mut self) {
        self.prepare_layer = 0;
    }
}

#[derive(Debug)]
pub struct Layer {
    constants: wgpu::BindGroup,
    constants_buffer: wgpu::Buffer,
    composed_quad: composed_quad::Layer,
}

impl Layer {
    pub fn new(device: &wgpu::Device, constant_layout: &wgpu::BindGroupLayout) -> Self {
        let constants_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sugarloaf::quad uniforms buffer"),
            size: mem::size_of::<Uniforms>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let constants = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sugarloaf::quad uniforms bind group"),
            layout: constant_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: constants_buffer.as_entire_binding(),
            }],
        });

        Self {
            constants,
            constants_buffer,
            composed_quad: composed_quad::Layer::new(device),
        }
    }

    pub fn prepare(
        &mut self,
        context: &Context,
        quads: &Batch,
        transformation: [f32; 16],
        scale: f32,
    ) {
        self.update(context, transformation, scale);

        if !quads.composed_quads.is_empty() {
            self.composed_quad.prepare(context, &quads.composed_quads);
        }
    }

    pub fn update(&mut self, context: &Context, transformation: [f32; 16], scale: f32) {
        let uniforms = Uniforms::new(transformation, scale);
        let bytes = bytemuck::bytes_of(&uniforms);

        context.queue.write_buffer(&self.constants_buffer, 0, bytes);
    }
}

/// A group of [`Quad`]s rendered together.
#[derive(Default, Debug)]
pub struct Batch {
    /// The composed_quad quads of the [`Layer`].
    composed_quads: Vec<ComposedQuad>,
}

impl Batch {
    /// Returns true if there are no quads of any type in [`Quads`].
    pub fn is_empty(&self) -> bool {
        self.composed_quads.is_empty()
    }

    /// Adds a [`Quad`] with the provided `Background` type to the quad [`Layer`].
    pub fn add(&mut self, composed_quad: ComposedQuad) {
        self.composed_quads.push(composed_quad);
    }

    pub fn clear(&mut self) {
        self.composed_quads.clear();
    }
}

fn color_target_state(
    format: wgpu::TextureFormat,
) -> [Option<wgpu::ColorTargetState>; 1] {
    [Some(wgpu::ColorTargetState {
        format,
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
    })]
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
struct Uniforms {
    transform: [f32; 16],
    scale: f32,
    // Uniforms must be aligned to their largest member,
    // this uses a mat4x4<f32> which aligns to 16, so align to that
    _padding: [f32; 3],
}

impl Uniforms {
    fn new(transformation: [f32; 16], scale: f32) -> Uniforms {
        Self {
            transform: transformation,
            scale,
            _padding: [0.0; 3],
        }
    }
}

impl Default for Uniforms {
    fn default() -> Self {
        Self {
            transform: orthographic_projection(0.0, 0.0),
            scale: 1.0,
            _padding: [0.0; 3],
        }
    }
}
