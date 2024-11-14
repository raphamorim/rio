mod cache;

use crate::components::text::Region;
use cache::Cache;
use std::borrow::Cow;

use crate::components::text::glyph::ab_glyph::{point, Rect};
use bytemuck::{Pod, Zeroable};
use std::marker::PhantomData;
use std::mem;
use wgpu::util::DeviceExt;

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

pub struct Pipeline<Depth> {
    transform: wgpu::Buffer,
    sampler: wgpu::Sampler,
    cache: Cache,
    uniform_layout: wgpu::BindGroupLayout,
    uniforms: wgpu::BindGroup,
    raw: wgpu::RenderPipeline,
    instances: wgpu::Buffer,
    current_instances: usize,
    supported_instances: usize,
    current_transform: [f32; 16],
    depth: PhantomData<Depth>,
}

impl Pipeline<()> {
    pub fn new(
        device: &wgpu::Device,
        filter_mode: wgpu::FilterMode,
        multisample: wgpu::MultisampleState,
        render_format: wgpu::TextureFormat,
        cache_width: u32,
        cache_height: u32,
    ) -> Pipeline<()> {
        build(
            device,
            filter_mode,
            multisample,
            render_format,
            cache_width,
            cache_height,
        )
    }

    pub fn draw<'pass>(
        &'pass mut self,
        queue: &wgpu::Queue,
        rpass: &mut wgpu::RenderPass<'pass>,
        transform: [f32; 16],
        region: Option<Region>,
    ) {
        draw(self, (queue, rpass), transform, region);
    }
}

impl Pipeline<wgpu::DepthStencilState> {
    pub fn new(
        device: &wgpu::Device,
        filter_mode: wgpu::FilterMode,
        multisample: wgpu::MultisampleState,
        render_format: wgpu::TextureFormat,
        cache_width: u32,
        cache_height: u32,
    ) -> Pipeline<wgpu::DepthStencilState> {
        build(
            device,
            filter_mode,
            multisample,
            render_format,
            cache_width,
            cache_height,
        )
    }

    pub fn draw<'pass>(
        &'pass mut self,
        config: (&mut wgpu::Queue, &mut wgpu::RenderPass<'pass>),
        transform: [f32; 16],
        region: Option<Region>,
    ) {
        let (queue, rpass) = config;
        draw(self, (queue, rpass), transform, region);
    }
}

impl<Depth> Pipeline<Depth> {
    pub fn update_cache(
        &mut self,
        queue: &wgpu::Queue,
        offset: [u16; 2],
        size: [u16; 2],
        data: &[u8],
    ) {
        self.cache.update(queue, offset, size, data);
    }

    pub fn increase_cache_size(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) {
        self.cache = Cache::new(device, width, height);

        self.uniforms = create_uniforms(
            device,
            &self.uniform_layout,
            &self.transform,
            &self.sampler,
            &self.cache.view,
        );
    }

    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &mut [Instance],
    ) {
        if instances.is_empty() {
            self.current_instances = 0;
            return;
        }

        if instances.len() > self.supported_instances {
            self.instances = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sugarloaf::text::Pipeline instances"),
                size: mem::size_of::<Instance>() as u64 * instances.len() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            self.supported_instances = instances.len();
        }

        let instances_bytes = bytemuck::cast_slice(instances);

        if !instances_bytes.is_empty() {
            // let instances_buffer =
            //     device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            //         label: Some("sugarloaf::text::Pipeline instances"),
            //         contents: instances_bytes,
            //         usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_SRC,
            //     });

            // encoder.copy_buffer_to_buffer(
            //     &instances_buffer,
            //     0,
            //     &self.instances,
            //     0,
            //     mem::size_of::<Instance>() as u64 * instances.len() as u64,
            // );
            queue.write_buffer(&self.instances, 0, instances_bytes);
        }

        self.current_instances = instances.len();
    }
}

const IDENTITY_MATRIX: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

fn build<D>(
    device: &wgpu::Device,
    filter_mode: wgpu::FilterMode,
    multisample: wgpu::MultisampleState,
    render_format: wgpu::TextureFormat,
    cache_width: u32,
    cache_height: u32,
) -> Pipeline<D> {
    let transform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&IDENTITY_MATRIX),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: filter_mode,
        min_filter: filter_mode,
        mipmap_filter: filter_mode,
        ..Default::default()
    });

    let cache = Cache::new(device, cache_width, cache_height);

    let uniform_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("text::Pipeline uniforms"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            mem::size_of::<[f32; 16]>() as u64,
                        ),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

    let uniforms =
        create_uniforms(device, &uniform_layout, &transform, &sampler, &cache.view);

    let instances = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("text::Pipeline instances"),
        size: mem::size_of::<Instance>() as u64 * Instance::INITIAL_AMOUNT as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        push_constant_ranges: &[],
        bind_group_layouts: &[&uniform_layout],
    });

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Text Shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("text.wgsl"))),
    });

    let raw = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        cache: None,
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: mem::size_of::<Instance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![
                    0 => Float32x3,
                    1 => Float32x2,
                    2 => Float32x2,
                    3 => Float32x2,
                    4 => Float32x4,
                ],
            }],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            front_face: wgpu::FrontFace::Cw,
            strip_index_format: Some(wgpu::IndexFormat::Uint16),
            ..Default::default()
        },
        multisample,
        depth_stencil: None,
        fragment: Some(wgpu::FragmentState {
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: render_format,
                blend: BLEND,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
    });

    Pipeline {
        transform,
        sampler,
        cache,
        uniform_layout,
        uniforms,
        raw,
        instances,
        current_instances: 0,
        supported_instances: Instance::INITIAL_AMOUNT,
        current_transform: [0.0; 16],
        depth: PhantomData,
    }
}

fn draw<'pass, D>(
    pipeline: &'pass mut Pipeline<D>,
    config: (&wgpu::Queue, &mut wgpu::RenderPass<'pass>),
    transform: [f32; 16],
    region: Option<Region>,
) {
    let (queue, render_pass) = config;
    if transform != pipeline.current_transform {
        queue.write_buffer(&pipeline.transform, 0, bytemuck::cast_slice(&transform));

        pipeline.current_transform = transform;
    }

    // let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
    //     label: Some("text::pipeline render pass"),
    //     timestamp_writes: None,
    //     occlusion_query_set: None,
    //     color_attachments: &[Some(wgpu::RenderPassColorAttachment {
    //         view: target,
    //         resolve_target: None,
    //         ops: wgpu::Operations {
    //             load: wgpu::LoadOp::Load,
    //             store: wgpu::StoreOp::Store,
    //         },
    //     })],
    //     depth_stencil_attachment,
    // });

    render_pass.set_pipeline(&pipeline.raw);
    render_pass.set_bind_group(0, &pipeline.uniforms, &[]);
    render_pass.set_vertex_buffer(0, pipeline.instances.slice(..));

    if let Some(region) = region {
        render_pass.set_scissor_rect(region.x, region.y, region.width, region.height);
    }

    render_pass.draw(0..4, 0..pipeline.current_instances as u32);
}

fn create_uniforms(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    transform: &wgpu::Buffer,
    sampler: &wgpu::Sampler,
    cache: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("text::Pipeline uniforms"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: transform,
                    offset: 0,
                    size: None,
                }),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(cache),
            },
        ],
    })
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
pub struct Instance {
    left_top: [f32; 3],
    right_bottom: [f32; 2],
    tex_left_top: [f32; 2],
    tex_right_bottom: [f32; 2],
    color: [f32; 4],
}

impl Instance {
    const INITIAL_AMOUNT: usize = 6;

    #[inline]
    pub fn from_vertex(
        crate::components::text::glyph::GlyphVertex {
            mut tex_coords,
            pixel_coords,
            bounds,
            extra,
        }: crate::components::text::glyph::GlyphVertex,
    ) -> Instance {
        let gl_bounds = bounds;

        let mut gl_rect = Rect {
            min: point(pixel_coords.min.x, pixel_coords.min.y),
            max: point(pixel_coords.max.x, pixel_coords.max.y),
        };

        // handle overlapping bounds, modify uv_rect to preserve texture aspect
        if gl_rect.max.x > gl_bounds.max.x {
            let old_width = gl_rect.width();
            gl_rect.max.x = gl_bounds.max.x;
            tex_coords.max.x =
                tex_coords.min.x + tex_coords.width() * gl_rect.width() / old_width;
        }

        if gl_rect.min.x < gl_bounds.min.x {
            let old_width = gl_rect.width();
            gl_rect.min.x = gl_bounds.min.x;
            tex_coords.min.x =
                tex_coords.max.x - tex_coords.width() * gl_rect.width() / old_width;
        }

        if gl_rect.max.y > gl_bounds.max.y {
            let old_height = gl_rect.height();
            gl_rect.max.y = gl_bounds.max.y;
            tex_coords.max.y =
                tex_coords.min.y + tex_coords.height() * gl_rect.height() / old_height;
        }

        if gl_rect.min.y < gl_bounds.min.y {
            let old_height = gl_rect.height();
            gl_rect.min.y = gl_bounds.min.y;
            tex_coords.min.y =
                tex_coords.max.y - tex_coords.height() * gl_rect.height() / old_height;
        }

        Instance {
            left_top: [gl_rect.min.x, gl_rect.max.y, extra.z],
            right_bottom: [gl_rect.max.x, gl_rect.min.y],
            tex_left_top: [tex_coords.min.x, tex_coords.max.y],
            tex_right_bottom: [tex_coords.max.x, tex_coords.min.y],
            color: extra.color,
        }
    }
}
