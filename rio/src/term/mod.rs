mod cache;

// use crate::bar::{self, BarBrush};
use crate::shared;
use crate::text::{ab_glyph, GlyphBrush, GlyphBrushBuilder, Section, Text};
use cache::Cache;
use config::{Colors, Style};
// use core::num::NonZeroU64;
use std::error::Error;
use std::mem;
use std::sync::Arc;
use std::sync::Mutex;

pub struct Term {
    style: Style,
    colors: Colors,
    device: wgpu::Device,
    surface: wgpu::Surface,
    queue: wgpu::Queue,
    render_format: wgpu::TextureFormat,
    staging_belt: wgpu::util::StagingBelt,
    text_brush: GlyphBrush<()>,
    size: winit::dpi::PhysicalSize<u32>,
    scale: f32,
    // bar: BarBrush,
    #[allow(dead_code)]
    cache: Cache,
    uniform_layout: wgpu::BindGroupLayout,
    uniforms: wgpu::BindGroup,
    transform: wgpu::Buffer,
    current_transform: [f32; 16],
    pid: i32,
}

const IDENTITY_MATRIX: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

impl Term {
    pub async fn new(
        winit_window: &winit::window::Window,
        config: &config::Config,
        pid: i32,
    ) -> Result<Term, Box<dyn Error>> {
        let instance = wgpu::Instance::new(wgpu::Backends::all());
        let surface = unsafe { instance.create_surface(&winit_window) };

        let power_preference: wgpu::PowerPreference = match config.performance {
            config::Performance::High => wgpu::PowerPreference::HighPerformance,
            config::Performance::Low => wgpu::PowerPreference::LowPower,
        };

        let (device, queue) = (async {
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference,
                    compatible_surface: Some(&surface),
                    force_fallback_adapter: false,
                })
                .await
                .expect("Request adapter");

            adapter
                .request_device(&wgpu::DeviceDescriptor::default(), None)
                .await
                .expect("Request device")
        })
        .await;

        let staging_belt = wgpu::util::StagingBelt::new(64);
        let render_format = wgpu::TextureFormat::Bgra8UnormSrgb;

        let size = winit_window.inner_size();

        // let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        //     label: Some("Shader"),
        //     source: wgpu::ShaderSource::Wgsl(include_str!("../bar/bar.wgsl").into()),
        // });

        let scale = winit_window.scale_factor() as f32;
        // let bar: BarBrush = BarBrush::new(&device, shader, scale);

        surface.configure(
            &device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: render_format,
                width: size.width,
                height: size.height,
                present_mode: wgpu::PresentMode::AutoVsync,
            },
        );

        let font = match config.style.font {
            config::Font::Firamono => {
                ab_glyph::FontArc::try_from_slice(shared::FONT_FIRAMONO)?
            }
            config::Font::Novamono => {
                ab_glyph::FontArc::try_from_slice(shared::FONT_NOVAMONO)?
            }
        };

        let text_brush =
            GlyphBrushBuilder::using_font(font).build(&device, render_format);

        let cache = Cache::new(&device, 1024, 1024);

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
                            min_binding_size: wgpu::BufferSize::new(mem::size_of::<
                                [f32; 16],
                            >(
                            )
                                as u64),
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
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float {
                                filterable: true,
                            },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });

        use wgpu::util::DeviceExt;

        let transform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&IDENTITY_MATRIX),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        let uniforms = Self::create_uniforms(
            &device,
            &uniform_layout,
            &transform,
            &sampler,
            &cache.view,
        );

        let current_transform = Self::projection(size.width, size.height);

        Ok(Term {
            style: config.style,
            colors: config.colors,
            device,
            surface,
            staging_belt,
            text_brush,
            size,
            scale,
            render_format,
            // bar,
            queue,
            cache,
            uniforms,
            uniform_layout,
            current_transform,
            transform,
            pid,
        })
    }

    pub fn set_size(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.size = new_size;

        self.configure_surface();
    }

    // https://docs.rs/winit/latest/winit/dpi/
    pub fn set_scale(&mut self, new_scale: f32, new_size: winit::dpi::PhysicalSize<u32>) {
        self.scale = new_scale;
        self.size = new_size;

        self.configure_surface();

        // if self.is_modern() {
        //     let shader = self
        //         .device
        //         .create_shader_module(wgpu::ShaderModuleDescriptor {
        //             label: Some("Shader"),
        //             source: wgpu::ShaderSource::Wgsl(
        //                 include_str!("../bar/bar.wgsl").into(),
        //             ),
        //         });

        //     self.bar = BarBrush::new(&self.device, shader, self.scale);
        // }
    }

    pub fn get_scale(&self) -> f32 {
        self.scale
    }

    #[inline]
    fn configure_surface(&mut self) {
        self.surface.configure(
            &self.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: self.render_format,
                width: self.size.width,
                height: self.size.height,
                present_mode: wgpu::PresentMode::AutoVsync,
            },
        );
    }

    #[inline]
    fn create_encoder(&self) -> wgpu::CommandEncoder {
        self.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Redraw"),
            })
    }

    // #[inline]
    // fn create_render_pipeline(&self) -> wgpu::RenderPipeline {
    //     let render_pipeline_layout: wgpu::PipelineLayout = self
    //         .device
    //         .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
    //             label: Some("Term -> Render Pipeline Layout"),
    //             push_constant_ranges: &[],
    //             bind_group_layouts: &[&self.uniform_layout],
    //         });

    // self.device
    //     .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
    //         label: Some("Term -> Render Pipeline"),
    //         layout: Some(&render_pipeline_layout),
    //         vertex: wgpu::VertexState {
    //             module: &self.bar.shader,
    //             entry_point: "vs_main",
    //             buffers: &[bar::Vertex::desc()],
    //         },
    //         fragment: Some(wgpu::FragmentState {
    //             module: &self.bar.shader,
    //             entry_point: "fs_main",
    //             targets: &[Some(wgpu::ColorTargetState {
    //                 format: self.render_format,
    //                 blend: shared::gpu::BLEND,
    //                 write_mask: wgpu::ColorWrites::ALL,
    //             })],
    //         }),
    //         primitive: wgpu::PrimitiveState {
    //             topology: wgpu::PrimitiveTopology::TriangleList,
    //             strip_index_format: None,
    //             front_face: wgpu::FrontFace::Ccw,
    //             ..Default::default()
    //         },
    //         depth_stencil: None,
    //         multisample: wgpu::MultisampleState::default(),
    //         multiview: None,
    //     })
    // }

    #[inline]
    fn projection(width: u32, height: u32) -> [f32; 16] {
        // [2.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0]

        let h = (height as f32) / 10000.0;
        let w = (width as f32) / 1000.0;
        [
            w,
            0.0,
            0.0,
            0.0,
            w - 0.89,
            0.89 + h,
            0.0,
            0.0,
            w - 0.89,
            0.89 + h,
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ]
    }

    // TODO: Asynchronous update based on 2s
    // Idea? Prob move Term inside of TermUi that contains Tabs/Term
    // Allowing switch Terms
    fn get_command_name(&self) -> String {
        tty::command_per_pid(self.pid)
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

    #[inline]
    fn is_modern(&self) -> bool {
        self.style.theme == config::Theme::Modern
    }

    #[inline]
    fn is_basic(&self) -> bool {
        self.style.theme == config::Theme::Basic
    }

    pub fn draw(&mut self, output: &Arc<Mutex<String>>) {
        let mut encoder = self.create_encoder();

        let frame = self.surface.get_current_texture().expect("Get next frame");
        let view = &frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // if self.is_modern() {
        //     let render_pipeline = self.create_render_pipeline();

        //     {
        //         let new_transform = Self::projection(self.size.width, self.size.height);

        //         if new_transform != self.current_transform {
        //             let mut transform_view = self.staging_belt.write_buffer(
        //                 &mut encoder,
        //                 &self.transform,
        //                 0,
        //                 unsafe { NonZeroU64::new_unchecked(16 * 4) },
        //                 &self.device,
        //             );

        //             transform_view.copy_from_slice(bytemuck::cast_slice(&new_transform));

        //             self.current_transform = new_transform;
        //         }

        //         let mut render_pass =
        //             encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        //                 label: Some("Term -> Clear frame"),
        //                 color_attachments: &[Some(wgpu::RenderPassColorAttachment {
        //                     view,
        //                     resolve_target: None,
        //                     ops: wgpu::Operations {
        //                         load: wgpu::LoadOp::Clear(
        //                             self.colors.background,
        //                         ),
        //                         store: true,
        //                     },
        //                 })],
        //                 depth_stencil_attachment: None,
        //             });

        //         if self.is_modern() {
        //             render_pass.set_pipeline(&render_pipeline);
        //             render_pass.set_bind_group(0, &self.uniforms, &[]);
        //             render_pass.set_vertex_buffer(0, self.bar.buffers.0.slice(..));
        //             render_pass.set_index_buffer(
        //                 self.bar.buffers.1.slice(..),
        //                 wgpu::IndexFormat::Uint16,
        //             );
        //             render_pass.draw(0..self.bar.num_indices, 0..1);
        //         }
        //     }
        // } else {

        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Term -> Clear frame"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(self.colors.background),
                    store: true,
                },
            })],
            depth_stencil_attachment: None,
        });

        let yspacing = if self.is_modern() { 60.0 } else { 30.0 };
        {
            self.text_brush.queue(Section {
                screen_position: (10.0 * self.scale, (yspacing * self.scale)),
                bounds: (
                    (self.size.width as f32)
                        - ((self.style.font_size + 5.0) * self.scale),
                    (self.size.height as f32) * self.scale,
                ),
                text: vec![Text::new(&output.lock().unwrap())
                    .with_color([1.0, 1.0, 1.0, 1.0])
                    .with_scale(self.style.font_size * self.scale)],
                ..Section::default()
            });

            if self.is_basic() {
                self.text_brush.queue(Section {
                    screen_position: (80.0 * self.scale, (8.0 * self.scale)),
                    bounds: (
                        (self.size.width as f32) - (40.0 * self.scale),
                        (self.size.height as f32) * self.scale,
                    ),
                    text: vec![Text::new(
                        format!("■ {}", self.get_command_name()).as_str(),
                    )
                    // #CD5E98
                    .with_color([0.81569, 0.39608, 0.56863, 1.0])
                    .with_scale(14.0 * self.scale)],
                    ..Section::default()
                });

                // self.text_brush.queue(Section {
                //     screen_position: (124.0 * self.scale, (8.0 * self.scale)),
                //     bounds: (
                //         (self.size.width as f32) - (40.0 * self.scale),
                //         (self.size.height as f32) * self.scale,
                //     ),
                //     text: vec![Text::new("■ vim ■ zsh ■ docker")
                //         //(157,165,237)
                //         .with_color([0.89020, 0.54118, 0.33725, 1.0])
                //         .with_scale(14.0 * self.scale)],
                //     ..Section::default()
                // });
            }

            self.text_brush
                .draw_queued(
                    &self.device,
                    &mut self.staging_belt,
                    &mut encoder,
                    view,
                    (self.size.width, self.size.height),
                )
                .expect("Draw queued");
        }

        self.staging_belt.finish();
        self.queue.submit(Some(encoder.finish()));
        frame.present();

        // Recall unused staging buffers
        self.staging_belt.recall();
    }
}
