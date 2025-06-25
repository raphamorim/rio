// Unified rendering system that handles all rendering types with a single shader

use crate::context::Context;
use crate::shaders::UnifiedVertex;
use std::borrow::Cow;
use std::mem;
use wgpu::util::DeviceExt;

pub struct UnifiedRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    constant_bind_group: wgpu::BindGroup,
    texture_bind_group: wgpu::BindGroup,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    transform_buffer: wgpu::Buffer,
    current_transform: [f32; 16],
    vertices: Vec<UnifiedVertex>,
    supported_vertex_buffer: usize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    transform: [f32; 16],
    scale: f32,
    _padding: [f32; 3],
}

impl UnifiedRenderer {
    pub fn new(context: &Context) -> Self {
        let device = &context.device;
        let supported_vertex_buffer = 10000; // Large buffer for all rendering

        let current_transform = crate::components::core::orthographic_projection(
            context.size.width,
            context.size.height,
        );

        let transform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("unified transform buffer"),
            contents: bytemuck::cast_slice(&[Uniforms {
                transform: current_transform,
                scale: 1.0,
                _padding: [0.0; 3],
            }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Create a dummy texture for now - this should be replaced with actual texture management
        let dummy_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("unified dummy texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let dummy_texture_view = dummy_texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Create bind group layouts
        let constant_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("unified constant layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(mem::size_of::<Uniforms>() as u64),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("unified texture layout"),
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

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("unified pipeline layout"),
            push_constant_ranges: &[],
            bind_group_layouts: &[&constant_bind_group_layout, &texture_bind_group_layout],
        });

        let constant_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &constant_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &transform_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
            label: Some("unified constant bind group"),
        });

        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &texture_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&dummy_texture_view),
            }],
            label: Some("unified texture bind group"),
        });

        let shader_source = crate::shaders::get_unified_shader(context.supports_f16());
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("unified shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_source)),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            cache: None,
            label: Some("unified render pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: mem::size_of::<UnifiedVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array!(
                        0 => Float32x4, // position
                        1 => Float32x4, // color
                        2 => Float32x4, // uv_layer
                        3 => Float32x4, // size_border
                        4 => Float32x4, // extended
                    ),
                }],
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
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("unified vertex buffer"),
            size: mem::size_of::<UnifiedVertex>() as u64 * supported_vertex_buffer as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            vertex_buffer,
            constant_bind_group,
            texture_bind_group,
            texture_bind_group_layout,
            transform_buffer,
            current_transform,
            vertices: Vec::new(),
            supported_vertex_buffer,
        }
    }

    pub fn add_quad(&mut self, position: [f32; 2], size: [f32; 2], color: [f32; 4], border_radius: f32) {
        self.vertices.push(UnifiedVertex {
            position: [position[0], position[1], 0.0, 0.0], // render_mode = 0 for quad
            color,
            uv_layer: [0.0, 0.0, 0.0, 0.0],
            size_border: [size[0], size[1], 0.0, border_radius],
            extended: [0.0, 0.0, 0.0, 0.0],
        });
    }

    pub fn add_text(&mut self, position: [f32; 2], color: [f32; 4], uv: [f32; 2], layer: f32) {
        self.vertices.push(UnifiedVertex {
            position: [position[0], position[1], 0.0, 1.0], // render_mode = 1 for text
            color,
            uv_layer: [uv[0], uv[1], layer, 0.0],
            size_border: [0.0, 0.0, 0.0, 0.0],
            extended: [0.0, 0.0, 0.0, 0.0],
        });
    }

    pub fn add_image(&mut self, position: [f32; 2], size: [f32; 2], uv: [f32; 2], layer: f32) {
        self.vertices.push(UnifiedVertex {
            position: [position[0], position[1], 0.0, 2.0], // render_mode = 2 for image
            color: [1.0, 1.0, 1.0, 1.0],
            uv_layer: [uv[0], uv[1], layer, 0.0],
            size_border: [size[0], size[1], 0.0, 0.0],
            extended: [0.0, 0.0, 0.0, 0.0],
        });
    }

    pub fn clear(&mut self) {
        self.vertices.clear();
    }

    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.vertices.is_empty() {
            return;
        }

        // Resize buffer if needed
        if self.vertices.len() > self.supported_vertex_buffer {
            self.supported_vertex_buffer = self.vertices.len() * 2;
            self.vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("unified vertex buffer"),
                size: mem::size_of::<UnifiedVertex>() as u64 * self.supported_vertex_buffer as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        queue.write_buffer(
            &self.vertex_buffer,
            0,
            bytemuck::cast_slice(&self.vertices),
        );
    }

    pub fn render<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        if self.vertices.is_empty() {
            return;
        }

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.constant_bind_group, &[]);
        render_pass.set_bind_group(1, &self.texture_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        
        // Draw 6 vertices (2 triangles) per instance
        render_pass.draw(0..6, 0..self.vertices.len() as u32);
    }

    pub fn update_transform(&mut self, queue: &wgpu::Queue, transform: [f32; 16], scale: f32) {
        if self.current_transform != transform {
            self.current_transform = transform;
            queue.write_buffer(
                &self.transform_buffer,
                0,
                bytemuck::cast_slice(&[Uniforms {
                    transform,
                    scale,
                    _padding: [0.0; 3],
                }]),
            );
        }
    }
}