mod cache;
use ansi_machine::{process, Row, VisibleRows};
use cache::Cache;
use renderer::{Renderer, RendererStyles};
use std::borrow::Cow;
use std::error::Error;
use teletypewriter::{pty, Process};

use std::sync::Arc;
use std::sync::Mutex;

pub struct Term {
    device: wgpu::Device,
    surface: wgpu::Surface,
    queue: wgpu::Queue,
    format: wgpu::TextureFormat,
    alpha_mode: wgpu::CompositeAlphaMode,
    staging_belt: wgpu::util::StagingBelt,
    pub renderer: Renderer,
    size: winit::dpi::PhysicalSize<u32>,
    #[allow(dead_code)]
    cache: Cache,
    #[allow(dead_code)]
    pid: i32,
    pub write_process: Process,
    data_arc: VisibleRows,
}

impl Term {
    pub async fn new(
        winit_window: &winit::window::Window,
        config: config::Config,
    ) -> Result<Term, Box<dyn Error>> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            dx12_shader_compiler: wgpu::Dx12Compiler::Fxc,
        });

        let surface: wgpu::Surface =
            unsafe { instance.create_surface(&winit_window).unwrap() };
        let power_preference: wgpu::PowerPreference = match config.performance {
            config::Performance::High => wgpu::PowerPreference::HighPerformance,
            config::Performance::Low => wgpu::PowerPreference::LowPower,
        };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("Request adapter");

        let (device, queue) = (async {
            adapter
                .request_device(&wgpu::DeviceDescriptor::default(), None)
                .await
                .expect("Request device")
        })
        .await;

        let staging_belt = wgpu::util::StagingBelt::new(64);

        // TODO:
        // Bgra8UnormSrgb is the only texture format that is guaranteed to be
        // natively supported by the swapchains of all the APIs/platforms
        // This should be allowed to be configured by Rio
        // https://github.com/gfx-rs/wgpu-rs/issues/123
        // https://github.com/gfx-rs/wgpu/commit/ae3e5057aff64a8e6f13e75be661c0f8a98abcd5
        // let render_format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let format = wgpu::TextureFormat::Rgb10a2Unorm;

        let size = winit_window.inner_size();

        // let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        //     label: Some("Shader"),
        //     source: wgpu::ShaderSource::Wgsl(include_str!("../bar/bar.wgsl").into()),
        // });

        let scale = winit_window.scale_factor() as f32;
        // let bar: BarBrush = BarBrush::new(&device, shader, scale);
        let caps = surface.get_capabilities(&adapter);
        // [Bgra8UnormSrgb, Bgra8Unorm, Rgba16Float, Rgb10a2Unorm]
        // let formats = caps.formats;
        let formats = vec![format];
        let alpha_modes = caps.alpha_modes;
        let alpha_mode = alpha_modes[0];

        surface.configure(
            &device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width: size.width,
                height: size.height,
                view_formats: formats,
                alpha_mode,
                present_mode: wgpu::PresentMode::AutoVsync,
            },
        );

        let renderer_styles = RendererStyles::new(
            size.width as f32,
            size.height as f32,
            scale,
            config.style.font_size,
        );
        let data_arc: VisibleRows = Arc::new(Mutex::from(vec![Row::default()]));
        let data_arc_clone: VisibleRows = Arc::clone(&data_arc);

        let renderer = match Renderer::new(&device, format, config, renderer_styles) {
            Ok(r) => r,
            Err(e) => panic!("{e:?}"),
        };

        let cache = Cache::new(&device, 1024, 1024);

        let shell: String = match std::env::var("SHELL") {
            Ok(val) => val,
            Err(..) => String::from("bash"),
        };

        let (read_process, write_process, _ptyname, pid) = pty(
            &Cow::Borrowed(&shell),
            renderer.config.columns,
            renderer.config.rows,
        );

        let columns = renderer.config.columns;
        let rows = renderer.config.rows;

        let term = Term {
            device,
            surface,
            staging_belt,
            renderer,
            size,
            format,
            alpha_mode,
            queue,
            cache,
            pid,
            write_process,
            data_arc,
        };

        tokio::spawn(async move {
            process(read_process, data_arc_clone, columns.into(), rows.into());
        });

        Ok(term)
    }

    pub fn set_size(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.size = new_size;

        self.configure_surface();
    }

    // https://docs.rs/winit/latest/winit/dpi/
    pub fn set_scale(&mut self, new_scale: f32, new_size: winit::dpi::PhysicalSize<u32>) {
        if self.renderer.get_current_scale() != new_scale {
            // self.scale = new_scale;
            self.renderer.refresh_styles(
                new_size.width as f32,
                new_size.height as f32,
                new_scale,
            );
            self.size = new_size;

            self.configure_surface();
        }
    }

    #[inline]
    fn configure_surface(&mut self) {
        self.surface.configure(
            &self.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: self.format,
                width: self.size.width,
                height: self.size.height,
                view_formats: vec![self.format],
                alpha_mode: self.alpha_mode,
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

    // TODO: Asynchronous update based on 2s
    // Idea? Prob move Term inside of TermUi that contains Tabs/Term
    // Allowing switch Terms
    fn get_command_name(&self) -> String {
        // format!("■ {:?}", teletypewriter::command_per_pid(self.pid))
        format!("{} zsh ", self.renderer.config.advanced.tab_character)
    }

    pub fn draw(&mut self) {
        let mut encoder = self.create_encoder();

        let frame = self.surface.get_current_texture().expect("Get next frame");
        let view = &frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Term -> Clear frame"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(self.renderer.config.colors.background),
                    store: true,
                },
            })],
            depth_stencil_attachment: None,
        });

        self.renderer.topbar(self.get_command_name());
        self.renderer.term(self.data_arc.lock().unwrap().to_vec());

        self.renderer
            .brush
            .draw_queued(
                &self.device,
                &mut self.staging_belt,
                &mut encoder,
                view,
                (self.size.width, self.size.height),
            )
            .expect("Draw queued");

        self.staging_belt.finish();
        self.queue.submit(Some(encoder.finish()));
        frame.present();

        // Recall unused staging buffers
        self.staging_belt.recall();
    }
}
