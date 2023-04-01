use crate::crosswords::Crosswords;
use crate::event::sync::FairMutex;
use crate::event::EventProxy;
use crate::event::Msg;
use crate::performer::Machine;
use crate::renderer::{Renderer, RendererStyles};
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
    renderer: Renderer,
}

impl RenderContext {
    pub fn new(
        device: wgpu::Device,
        device_copy: wgpu::Device,
        scale: f32,
        queue: wgpu::Queue,
        adapter: wgpu::Adapter,
        surface: wgpu::Surface,
        instance: wgpu::Instance,
        staging_belt: wgpu::util::StagingBelt,
        config: &Rc<config::Config>,
        width: u32,
        height: u32,
    ) -> RenderContext {
        let caps = surface.get_capabilities(&adapter);
        let formats = caps.formats;
        let format = *formats.last().expect("No supported formats for surface");
        let alpha_modes = caps.alpha_modes;
        let alpha_mode = alpha_modes[0];

        surface.configure(
            &device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width: width,
                height: height,
                view_formats: vec![],
                alpha_mode,
                present_mode: wgpu::PresentMode::AutoVsync,
            },
        );

        let renderer_styles =
            RendererStyles::new(scale, width, height, config.style.font_size);
        let renderer = Renderer::new(device_copy, format, config, renderer_styles)
            .expect("Create renderer");
        RenderContext {
            device,
            queue,
            adapter,
            surface,
            instance,
            staging_belt,
            renderer,
        }
    }

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
    terminal: Arc<FairMutex<Crosswords<EventProxy>>>,
    channel: crate::performer::channel::Sender<Msg>,
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

        let (device_copy, queue_copy) = (async {
            adapter
                .request_device(&wgpu::DeviceDescriptor::default(), None)
                .await
                .expect("Request device")
        })
        .await;

        let staging_belt = wgpu::util::StagingBelt::new(1024);

        let scale = winit_window.scale_factor() as f32;
        let size = winit_window.inner_size();

        let render_context = RenderContext::new(
            device,
            device_copy,
            scale,
            queue,
            adapter,
            surface,
            instance,
            staging_belt,
            config,
            size.width,
            size.height,
        );

        let event_proxy_clone = event_proxy.clone();
        let terminal: Arc<FairMutex<Crosswords<EventProxy>>> = Arc::new(FairMutex::new(
            Crosswords::new(config.columns.into(), config.rows.into(), event_proxy),
        ));

        let machine = Machine::new(Arc::clone(&terminal), pty, event_proxy_clone)?;
        let channel = machine.channel();
        machine.spawn();

        Ok(Term {
            render_context,
            terminal,
            channel,
        })
    }

    pub fn configure(&self) {
        // self.machine.spawn();
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.render_context.configure(new_size);
    }

    pub fn input_char(&mut self, character: char) {
        println!("input_char: {}", character);
        let val: Cow<'static, [u8]> =
            Cow::<'static, [u8]>::Owned((&[character as u8]).to_vec());
        // println!("{:?}", self.channel);
        self.channel.send(Msg::Input(val.into()));
    }

    pub fn skeleton(&mut self, color: wgpu::Color) {
        // TODO: WGPU caching
        let mut encoder = self.render_context.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("Skeleton"),
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
        self.render_context.renderer.draw_queued(
            &self.render_context.device,
            &mut self.render_context.staging_belt,
            &mut encoder,
            view,
        );
        self.render_context.staging_belt.finish();
        self.render_context.queue.submit(Some(encoder.finish()));
        frame.present();
        self.render_context.staging_belt.recall();
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

        let mut terminal = self.terminal.lock();
        let visible_rows = terminal.visible_rows();
        // println!("{:?}", terminal.visible_rows_to_string());
        drop(terminal);
        // let a =
        // std::sync::Mutex::unlock(terminal);

        // self.renderer.topbar(self.windows_title_arc.lock().unwrap().to_string());
        self.render_context.renderer.term(visible_rows);

        // drop(terminal);

        self.render_context.renderer.draw_queued(
            &self.render_context.device,
            &mut self.render_context.staging_belt,
            &mut encoder,
            view,
        );

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
