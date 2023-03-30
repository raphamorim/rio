use crate::crosswords::Crosswords;
use crate::event::sync::FairMutex;
use crate::event::EventProxy;
use crate::performer::Machine;
use std::borrow::Cow;
use std::error::Error;
use std::rc::Rc;
use std::sync::Arc;
use teletypewriter::create_pty;

struct RenderContext {
    device: wgpu::Device,
    surface: wgpu::Surface,
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    queue: wgpu::Queue,
    staging_belt: wgpu::util::StagingBelt,
}

impl RenderContext {
    pub fn configure(&self, size: winit::dpi::PhysicalSize<u32>) {
        let caps = self.surface.get_capabilities(&self.adapter);
        let formats = caps.formats;
        let format = *formats.last().expect("No supported formats for surface");
        let alpha_modes = caps.alpha_modes;
        let alpha_mode = alpha_modes[0];

        self.surface.configure(
            &self.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width: size.width,
                height: size.height,
                view_formats: vec![],
                alpha_mode,
                present_mode: wgpu::PresentMode::AutoVsync,
            },
        );
    }
}

pub struct Term {
    render_context: RenderContext,
}

impl Term {
    pub async fn new(
        winit_window: &winit::window::Window,
        config: &Rc<config::Config>,
        event_proxy: EventProxy,
    ) -> Result<Term, Box<dyn Error>> {
        let shell = std::env::var("SHELL")?;
        let pty = create_pty(&Cow::Borrowed(&shell), config.columns, config.rows);

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

        let render_context = RenderContext {
            device,
            queue,
            adapter,
            surface,
            instance,
            staging_belt,
        };

        let size = winit_window.inner_size();
        // let scale = winit_window.scale_factor() as f32;
        render_context.configure(size);

        let event_proxy_clone = event_proxy.clone();
        let event_proxy_clone_2 = event_proxy.clone();
        let terminal: Arc<FairMutex<Crosswords<EventProxy>>> =
            Arc::new(FairMutex::new(Crosswords::new(80, 25, event_proxy)));

        let machine = Machine::new(terminal, pty, event_proxy_clone)?;

        machine.spawn();
        // terminal: Arc<FairMutex<Crosswords<U>>>, pty: T, event_proxy: U

        Ok(Term { render_context })
    }

    pub fn configure(&self) {
        // self.machine.spawn();
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.render_context.configure(new_size);
    }

    pub fn render(&mut self, color: wgpu::Color) {
        println!("rendering");

        let mut encoder = self.render_context.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("Redraw"),
            },
        );

        let frame = self
            .render_context
            .surface
            .get_current_texture()
            .expect("Get next frame");
        let view = &frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render -> Clear frame"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(color),
                    store: true,
                },
            })],
            depth_stencil_attachment: None,
        });

        // self.renderer.topbar(self.windows_title_arc.lock().unwrap().to_string());
        // self.renderer
        //     .term(self.visible_rows_arc.lock().unwrap().to_vec());

        // self.renderer
        //     .brush
        //     .draw_queued(
        //         &self.device,
        //         &mut self.staging_belt,
        //         &mut encoder,
        //         view,
        //         (self.size.width, self.size.height),
        //     )
        //     .expect("Draw queued");

        self.render_context.staging_belt.finish();
        self.render_context.queue.submit(Some(encoder.finish()));
        frame.present();
        self.render_context.staging_belt.recall();
    }

    // https://docs.rs/winit/latest/winit/dpi/
    pub fn set_scale(&mut self, new_scale: f32, new_size: winit::dpi::PhysicalSize<u32>) {
        // if self.renderer.get_current_scale() != new_scale {
        //     // self.scale = new_scale;
        //     self.renderer.refresh_styles(
        //         new_size.width as f32,
        //         new_size.height as f32,
        //         new_scale,
        //     );
        //     self.size = new_size;

        //     self.configure_surface();
        // }
    }
}
