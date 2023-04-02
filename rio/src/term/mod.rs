mod ansi;
mod messenger;

use crate::crosswords::Crosswords;
use crate::event::sync::FairMutex;
use crate::event::EventProxy;
use crate::performer::Machine;
use crate::renderer::{Renderer, RendererStyles};
use crate::term::messenger::Messenger;
use std::borrow::Cow;
use std::error::Error;
use std::rc::Rc;
use std::sync::Arc;
use teletypewriter::create_pty;

struct RenderContext {
    device: wgpu::Device,
    surface: wgpu::Surface,
    adapter: wgpu::Adapter,
    queue: wgpu::Queue,
    staging_belt: wgpu::util::StagingBelt,
    renderer: Renderer,
}

impl RenderContext {
    pub async fn new(
        scale: f32,
        adapter: wgpu::Adapter,
        surface: wgpu::Surface,
        config: &Rc<config::Config>,
        size: winit::dpi::PhysicalSize<u32>,
    ) -> RenderContext {
        let caps = surface.get_capabilities(&adapter);
        let formats = caps.formats;
        let format = *formats.last().expect("No supported formats for surface");
        let alpha_modes = caps.alpha_modes;
        let alpha_mode = alpha_modes[0];

        let (device, queue) = (async {
            adapter
                .request_device(&wgpu::DeviceDescriptor::default(), None)
                .await
                .expect("Request device")
        })
        .await;

        let (device_copy, _queue_copy) = (async {
            adapter
                .request_device(&wgpu::DeviceDescriptor::default(), None)
                .await
                .expect("Request device")
        })
        .await;

        let staging_belt = wgpu::util::StagingBelt::new(1024);

        surface.configure(
            &device,
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

        let renderer_styles =
            RendererStyles::new(scale, size.width, size.height, config.style.font_size);
        let renderer = Renderer::new(device_copy, format, config, renderer_styles)
            .expect("Create renderer");
        RenderContext {
            device,
            queue,
            adapter,
            surface,
            staging_belt,
            renderer,
        }
    }

    pub fn update_size(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        self.renderer.update_size(size.width, size.height);
        self.configure();
    }

    pub fn update_scale(&mut self, size: winit::dpi::PhysicalSize<u32>, scale: f32) {
        self.renderer.update_scale(size.width, size.height, scale);
        self.configure();
    }

    pub fn configure(&self) {
        let caps = self.surface.get_capabilities(&self.adapter);
        let formats = caps.formats;
        let format = *formats.last().expect("No supported formats for surface");
        let alpha_modes = caps.alpha_modes;
        let alpha_mode = alpha_modes[0];
        let (width, height) = self.renderer.size();

        self.surface.configure(
            &self.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width,
                height,
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
    messenger: Messenger,
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

        let scale = winit_window.scale_factor() as f32;
        let size = winit_window.inner_size();

        let render_context =
            RenderContext::new(scale, adapter, surface, config, size).await;

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
            messenger: Messenger::new(channel),
        })
    }

    #[inline]
    pub fn propagate_modifiers_state(&mut self, state: winit::event::ModifiersState) {
        self.messenger.set_modifiers(state);
    }

    #[inline]
    pub fn input_char(&mut self, character: char) {
        if self.render_context.renderer.config.developer.enable_logs {
            println!("input_char: Received character {}", character);
        }

        self.messenger.send_character(character);
    }

    #[inline]
    pub fn input_keycode(
        &mut self,
        // _scancode: u32,
        virtual_keycode: Option<winit::event::VirtualKeyCode>,
    ) {
        let logs = self.render_context.renderer.config.developer.enable_logs;
        if logs {
            println!("input_keycode: received keycode {:?}", virtual_keycode);
        }

        if let Some(keycode) = virtual_keycode {
            let _ = self.messenger.send_keycode(keycode);
        } else if logs {
            println!("input_keycode: keycode not as Some");
        }
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
        drop(terminal);

        // self.renderer.topbar(self.windows_title_arc.lock().unwrap().to_string());
        self.render_context.renderer.term(visible_rows);

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

    #[inline]
    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.render_context.update_size(new_size);
        let new_h = self.render_context.renderer.config.columns - 40;
        let new_w = self.render_context.renderer.config.rows - 10;
        match self.messenger.send_resize(new_h, new_w, 0, 0) {
            Ok(new_window) => {
                // let mut terminal = self.terminal.lock();
                // terminal.resize(true, 25, 30);
                // drop(terminal);
            }
            Err(_) => {}
        }
    }

    // https://docs.rs/winit/latest/winit/dpi/
    #[allow(dead_code)]
    pub fn set_scale(
        &mut self,
        new_scale: f32,
        new_size: winit::dpi::PhysicalSize<u32>,
    ) {
        if self.render_context.renderer.scale() != new_scale {
            self.render_context.update_scale(new_size, new_scale);
        }
    }
}
