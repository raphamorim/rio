mod atlas;
mod raster;
pub mod types;

use crate::context::Context;
use atlas::Atlas;

use crate::components::core::buffer::Buffer;
use crate::components::core::orthographic_projection;
use crate::components::core::shapes::{Rectangle, Size};

use std::cell::RefCell;
use std::mem;

use bytemuck::{Pod, Zeroable};

use crate::components::core::image;

#[derive(Debug)]
pub struct LayerBrush {
    raster_cache: RefCell<raster::Cache>,
    pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    sampler: wgpu::Sampler,
    texture: wgpu::BindGroup,
    texture_version: usize,
    texture_atlas: Atlas,
    texture_layout: wgpu::BindGroupLayout,
    constant_layout: wgpu::BindGroupLayout,

    layers: Vec<Layer>,
    prepare_layer: usize,
}

#[derive(Debug)]
pub struct Layer {
    uniforms: wgpu::Buffer,
    constants: wgpu::BindGroup,
    instances: Buffer<Instance>,
    instance_count: usize,
}

impl Layer {
    fn new(
        device: &wgpu::Device,
        constant_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) -> Self {
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("image uniforms buffer"),
            size: mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let constants = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("image constants bind group"),
            layout: constant_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &uniforms,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });

        let instances = Buffer::new(
            device,
            "image instance buffer",
            Instance::INITIAL,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );

        Self {
            uniforms,
            constants,
            instances,
            instance_count: 0,
        }
    }

    fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[Instance],
        transformation: [f32; 16],
    ) {
        queue.write_buffer(
            &self.uniforms,
            0,
            bytemuck::bytes_of(&Uniforms {
                transform: transformation,
            }),
        );

        let _ = self.instances.resize(device, instances.len());
        let _ = self.instances.write(queue, 0, instances);

        self.instance_count = instances.len();
    }

    fn render<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        render_pass.set_bind_group(0, &self.constants, &[]);
        render_pass.set_vertex_buffer(1, self.instances.slice(..));

        render_pass.draw_indexed(
            0..QUAD_INDICES.len() as u32,
            0,
            0..self.instance_count as u32,
        );
    }
}

impl LayerBrush {
    pub fn new(context: &Context) -> Self {
        let device = &context.device;
        use wgpu::util::DeviceExt;

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let constant_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("image constants layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(
                                mem::size_of::<Uniforms>() as u64,
                            ),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(
                            wgpu::SamplerBindingType::Filtering,
                        ),
                        count: None,
                    },
                ],
            });

        let texture_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("image texture atlas layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: context.get_optimal_texture_sample_type(),
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                }],
            });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("image pipeline layout"),
            push_constant_ranges: &[],
            bind_group_layouts: &[&constant_layout, &texture_layout],
        });

        let shader_source = if context.supports_f16() {
            include_str!("image.wgsl")
        } else {
            include_str!("image_f32.wgsl")
        };

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("layer image shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(shader_source)),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            cache: None,
            label: Some("image pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[wgpu::VertexAttribute {
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                        }],
                    },
                    wgpu::VertexBufferLayout {
                        array_stride: mem::size_of::<Instance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array!(
                            1 => Float32x2,
                            2 => Float32x2,
                            3 => Float32x2,
                            4 => Float32x2,
                            5 => Sint32,
                        ),
                    },
                ],
            },
            fragment: Some(wgpu::FragmentState {
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                module: &shader,
                entry_point: Some("fs_main"),
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

        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("image vertex buffer"),
            contents: bytemuck::cast_slice(&QUAD_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("image index buffer"),
            contents: bytemuck::cast_slice(&QUAD_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        let texture_atlas = Atlas::new(device, context.adapter_info.backend, context);

        let texture = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("image texture atlas bind group"),
            layout: &texture_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(texture_atlas.view()),
            }],
        });

        LayerBrush {
            raster_cache: RefCell::new(raster::Cache::default()),
            pipeline,
            vertices,
            indices,
            sampler,
            texture,
            texture_version: texture_atlas.layer_count(),
            texture_atlas,
            texture_layout,
            constant_layout,
            layers: Vec::new(),
            prepare_layer: 0,
        }
    }

    pub fn dimensions(&self, handle: &image::Handle) -> Size<u32> {
        let mut cache = self.raster_cache.borrow_mut();
        let memory = cache.load(handle);

        memory.dimensions()
    }

    pub fn prepare(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        ctx: &mut Context,
        images: &[&types::Raster],
    ) {
        let transformation: [f32; 16] =
            orthographic_projection(ctx.size.width, ctx.size.height);
        let device = &ctx.device;
        let queue = &ctx.queue;

        let instances: &mut Vec<Instance> = &mut Vec::new();
        let mut raster_cache = self.raster_cache.borrow_mut();

        for image in images {
            let bounds = image.bounds;
            if let Some(atlas_entry) = raster_cache.upload(
                device,
                encoder,
                &image.handle,
                &mut self.texture_atlas,
                ctx,
            ) {
                add_instances(
                    [bounds.x, bounds.y],
                    [bounds.width, bounds.height],
                    atlas_entry,
                    instances,
                );
            }
        }

        if instances.is_empty() {
            return;
        }

        let texture_version = self.texture_atlas.layer_count();

        if self.texture_version != texture_version {
            tracing::info!("Atlas has grown. Recreating bind group...");

            self.texture = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("image texture atlas bind group"),
                layout: &self.texture_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        self.texture_atlas.view(),
                    ),
                }],
            });

            self.texture_version = texture_version;
        }

        if self.layers.len() <= self.prepare_layer {
            self.layers
                .push(Layer::new(device, &self.constant_layout, &self.sampler));
        }

        let layer = &mut self.layers[self.prepare_layer];
        layer.prepare(device, queue, instances, transformation);

        self.prepare_layer += 1;
    }

    pub fn prepare_with_handle(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        ctx: &mut Context,
        handle: &image::Handle,
        bounds: &Rectangle,
    ) {
        let transformation: [f32; 16] =
            orthographic_projection(ctx.size.width, ctx.size.height);
        let device = &ctx.device;
        let queue = &ctx.queue;

        let instances: &mut Vec<Instance> = &mut Vec::new();
        let mut raster_cache = self.raster_cache.borrow_mut();

        if let Some(atlas_entry) =
            raster_cache.upload(device, encoder, handle, &mut self.texture_atlas, ctx)
        {
            add_instances(
                [bounds.x, bounds.y],
                [bounds.width, bounds.height],
                atlas_entry,
                instances,
            );
        }

        if instances.is_empty() {
            return;
        }

        let texture_version = self.texture_atlas.layer_count();

        if self.texture_version != texture_version {
            tracing::info!("Atlas has grown. Recreating bind group...");

            self.texture = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("image texture atlas bind group"),
                layout: &self.texture_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        self.texture_atlas.view(),
                    ),
                }],
            });

            self.texture_version = texture_version;
        }

        if self.layers.len() <= self.prepare_layer {
            self.layers
                .push(Layer::new(device, &self.constant_layout, &self.sampler));
        }

        let layer = &mut self.layers[self.prepare_layer];
        layer.prepare(device, queue, instances, transformation);

        self.prepare_layer += 1;
    }

    #[inline]
    pub fn render<'a>(
        &'a self,
        layer: usize,
        render_pass: &mut wgpu::RenderPass<'a>,
        rect_bounds: Option<Rectangle<u32>>,
    ) {
        if let Some(layer) = self.layers.get(layer) {
            render_pass.set_pipeline(&self.pipeline);

            if let Some(bounds) = rect_bounds {
                render_pass.set_scissor_rect(
                    bounds.x,
                    bounds.y,
                    bounds.width,
                    bounds.height,
                );
            }

            render_pass.set_bind_group(1, &self.texture, &[]);
            render_pass
                .set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.set_vertex_buffer(0, self.vertices.slice(..));

            layer.render(render_pass);
        }
    }

    pub fn render_with_encoder(
        &self,
        layer: usize,
        view: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        rect_bounds: Option<Rectangle<u32>>,
    ) {
        if let Some(layer) = self.layers.get(layer) {
            let mut render_pass =
                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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

            render_pass.set_pipeline(&self.pipeline);

            if let Some(bounds) = rect_bounds {
                render_pass.set_scissor_rect(
                    bounds.x,
                    bounds.y,
                    bounds.width,
                    bounds.height,
                );
            }

            render_pass.set_bind_group(1, &self.texture, &[]);
            render_pass
                .set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.set_vertex_buffer(0, self.vertices.slice(..));

            layer.render(&mut render_pass);

            drop(render_pass);
        }
    }

    pub fn clear_atlas(
        &mut self,
        device: &wgpu::Device,
        backend: wgpu::Backend,
        context: &crate::context::Context,
    ) {
        self.texture_atlas.clear(device, backend, context);
        self.texture_version = self.texture_atlas.layer_count();

        // Recreate the bind group with the new atlas
        self.texture = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("image texture atlas bind group"),
            layout: &self.texture_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(self.texture_atlas.view()),
            }],
        });

        // Clear the raster cache as well
        self.raster_cache.borrow_mut().clear();
    }

    pub fn end_frame(&mut self) {
        self.raster_cache.borrow_mut().trim(&mut self.texture_atlas);

        self.prepare_layer = 0;
    }
}

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct Vertex {
    _position: [f32; 2],
}

const QUAD_INDICES: [u16; 6] = [0, 1, 2, 0, 2, 3];

const QUAD_VERTICES: [Vertex; 4] = [
    Vertex {
        _position: [0.0, 0.0],
    },
    Vertex {
        _position: [1.0, 0.0],
    },
    Vertex {
        _position: [1.0, 1.0],
    },
    Vertex {
        _position: [0.0, 1.0],
    },
];

#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
struct Instance {
    _position: [f32; 2],
    _size: [f32; 2],
    _position_in_atlas: [f32; 2],
    _size_in_atlas: [f32; 2],
    _layer: u32,
}

impl Instance {
    pub const INITIAL: usize = 1_000;
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
struct Uniforms {
    transform: [f32; 16],
}

fn add_instances(
    image_position: [f32; 2],
    image_size: [f32; 2],
    entry: &atlas::Entry,
    instances: &mut Vec<Instance>,
) {
    match entry {
        atlas::Entry::Contiguous(allocation) => {
            add_instance(image_position, image_size, allocation, instances);
        }
        atlas::Entry::Fragmented { fragments, size } => {
            let scaling_x = image_size[0] / size.width as f32;
            let scaling_y = image_size[1] / size.height as f32;

            for fragment in fragments {
                let allocation = &fragment.allocation;

                let [x, y] = image_position;
                let (fragment_x, fragment_y) = fragment.position;
                let Size {
                    width: fragment_width,
                    height: fragment_height,
                } = allocation.size();

                let position = [
                    x + fragment_x as f32 * scaling_x,
                    y + fragment_y as f32 * scaling_y,
                ];

                let size = [
                    fragment_width as f32 * scaling_x,
                    fragment_height as f32 * scaling_y,
                ];

                add_instance(position, size, allocation, instances);
            }
        }
    }
}

#[inline]
fn add_instance(
    position: [f32; 2],
    size: [f32; 2],
    allocation: &atlas::Allocation,
    instances: &mut Vec<Instance>,
) {
    let (x, y) = allocation.position();
    let Size { width, height } = allocation.size();
    let layer = allocation.layer();

    let instance = Instance {
        _position: position,
        _size: size,
        _position_in_atlas: [
            (x as f32 + 0.5) / atlas::SIZE as f32,
            (y as f32 + 0.5) / atlas::SIZE as f32,
        ],
        _size_in_atlas: [
            (width as f32 - 1.0) / atlas::SIZE as f32,
            (height as f32 - 1.0) / atlas::SIZE as f32,
        ],
        _layer: layer as u32,
    };

    instances.push(instance);
}
