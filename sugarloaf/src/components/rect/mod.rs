use crate::Renderable;
use crate::context::Context;
use bytemuck::{Pod, Zeroable};
use std::{borrow::Cow, mem};
use wgpu::util::DeviceExt;

const MAX_INSTANCES: usize = 10_000;

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

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct Quad {
    /// The position of the [`Quad`].
    pub position: [f32; 2],
    pub color: [f32; 4],
    pub size: [f32; 2],
}

#[allow(unsafe_code)]
unsafe impl bytemuck::Zeroable for Quad {}

#[allow(unsafe_code)]
unsafe impl bytemuck::Pod for Quad {}

fn create_vertices() -> Vec<Vertex> {
    let vertex_data = [
        vertex([0.0, 0.0]),
        vertex([0.025, 0.0]),
        vertex([0.025, 0.05]),
        vertex([0.0, 0.05]),
    ];

    vertex_data.to_vec()
}

pub struct Rect {
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    instances: wgpu::Buffer,
    index_count: usize,
    bind_group: wgpu::BindGroup,
    uniform_buf: wgpu::Buffer,
    pipeline: wgpu::RenderPipeline,
    current_transform: [f32; 16],
}

const IDENTITY_MATRIX: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

impl Renderable for Rect {
    fn init<'a>(
        context: &'a Context,
    ) -> Self {
        let width = &context.size.width;
        let height = &context.size.height;
        // let_adapter: &wgpu::Adapter,
        let device = &context.device;
        let _queue = &context.queue;
        let view_formats = context.format;

        // Create the vertex and index buffers
        let vertex_size = mem::size_of::<Vertex>();
        let vertex_data = create_vertices();

        let transform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&IDENTITY_MATRIX),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
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
                        min_binding_size: wgpu::BufferSize::new(64),
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

        // Create other resources
        // let mx_total = Self::generate_matrix(*width as f32, *height as f32);
        // let mx_ref: &[f32; 16] = mx_total.as_ref();
        let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniform Buffer"),
            contents: bytemuck::cast_slice(&IDENTITY_MATRIX),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniform_buf,
                    offset: 0,
                    size: None,
                }),
            }],
            label: None,
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
                array_stride: mem::size_of::<Quad>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array!(
                    1 => Float32x2,
                    2 => Float32x4,
                    3 => Float32x2,
                ),
            },
        ];

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &vertex_buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(view_formats.into())],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Instances Buffer"),
            size: mem::size_of::<Quad>() as u64 * MAX_INSTANCES as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Done
        Rect {
            vertex_buf,
            index_buf,
            index_count: QUAD_INDICES.len(),
            bind_group,
            uniform_buf,
            pipeline,
            current_transform: [0.0; 16],
            instances,
        }
    }

    fn update(&mut self, _event: winit::event::WindowEvent) {
        //empty
    }

    fn resize(
        &mut self,
        config: &wgpu::SurfaceConfiguration,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        queue.write_buffer(&self.uniform_buf, 0, bytemuck::cast_slice(&IDENTITY_MATRIX));
    }

    // fn draw(&mut self,
    //     device: &wgpu::Device,
    //     view: &wgpu::TextureView,
    //     instances: &[Quad],
    //     encoder: &mut wgpu::CommandEncoder,
    //     staging_belt: &mut wgpu::util::StagingBelt) {

    // }

    fn queue_render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        device: &wgpu::Device,
        view: &wgpu::TextureView,
        queue: &wgpu::Queue,
        _staging_belt: &mut wgpu::util::StagingBelt,
    ) {
        device.push_error_scope(wgpu::ErrorFilter::Validation);

        // if transform != pipeline.current_transform {
        //     let mut transform_view = staging_belt.write_buffer(
        //         encoder,
        //         &pipeline.transform,
        //         0,
        //         unsafe { NonZeroU64::new_unchecked(16 * 4) },
        //         device,
        //     );

        //     transform_view.copy_from_slice(bytemuck::cast_slice(&transform));

        //     pipeline.current_transform = transform;
        // }

        let instances = [
            Quad {
                position: [0.0, 0.0],
                color: [1.0, 1.0, 0.0, 1.0],
                size: [1.0, 1.0],
            },
            Quad {
                position: [0.6, -0.3],
                color: [0.0, 1.0, 0.0, 1.0],
                size: [0.0, 0.0],
            },
            Quad {
                position: [1.3, -1.3],
                color: [0.0, 1.0, 1.0, 1.0],
                size: [0.10, 0.10],
            },
        ];

        let mut i = 0;
        let total = instances.len();

        while i < total {
            let end = (i + MAX_INSTANCES).min(total);
            let amount = end - i;

            let instance_bytes = bytemuck::cast_slice(&instances[i..end]);

            queue.write_buffer(&self.instances, 0, instance_bytes);

            // let mut instance_buffer = staging_belt.write_buffer(
            //     encoder,
            //     &self.instances,
            //     0,
            //     wgpu::BufferSize::new(instance_bytes.len() as u64).unwrap(),
            //     device,
            // );

            // instance_buffer.copy_from_slice(instance_bytes);

            {
                let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: true,
                        },
                    })],
                    depth_stencil_attachment: None,
                });
                rpass.push_debug_group("Prepare data for draw.");
                rpass.set_pipeline(&self.pipeline);
                rpass.set_bind_group(0, &self.bind_group, &[]);
                rpass.set_index_buffer(
                    self.index_buf.slice(..),
                    wgpu::IndexFormat::Uint16,
                );
                rpass.set_vertex_buffer(0, self.vertex_buf.slice(..));
                rpass.set_vertex_buffer(1, self.instances.slice(..));
                rpass.pop_debug_group();
                rpass.insert_debug_marker("Draw!");
                rpass.draw_indexed(0..self.index_count as u32, 0, 0..amount as u32);
            }

            i += MAX_INSTANCES;
        }

        // queue.submit(Some(encoder.finish()));
    }
}

// fn main() {
// framework::run::<Example>("cube");
// }

// wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

// #[test]
// #[wasm_bindgen_test::wasm_bindgen_test]
// fn cube() {
//     framework::test::<Example>(framework::FrameworkRefTest {
//         image_path: "/examples/cube/screenshot.png",
//         width: 1024,
//         height: 768,
//         optional_features: wgpu::Features::default(),
//         base_test_parameters: framework::test_common::TestParameters::default(),
//         tolerance: 1,
//         max_outliers: 1225, // Bounded by swiftshader
//     });
// }

// #[test]
// #[wasm_bindgen_test::wasm_bindgen_test]
// fn cube_lines() {
//     framework::test::<Example>(framework::FrameworkRefTest {
//         image_path: "/examples/cube/screenshot-lines.png",
//         width: 1024,
//         height: 768,
//         optional_features: wgpu::Features::POLYGON_MODE_LINE,
//         base_test_parameters: framework::test_common::TestParameters::default(),
//         tolerance: 2,
//         max_outliers: 1250, // Bounded by swiftshader
//     });
// }
