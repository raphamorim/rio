mod ansi;
mod clipboard;
mod messenger;
pub mod window;

use crate::crosswords::grid::Scroll;
use crate::crosswords::Crosswords;
use crate::event::sync::FairMutex;
use crate::event::EventProxy;
use crate::layout::Layout;
use crate::performer::Machine;
use crate::renderer::Renderer;
use clipboard::{Clipboard, ClipboardType};
use messenger::Messenger;
use std::borrow::Cow;
use std::error::Error;
use std::rc::Rc;
use std::sync::Arc;
use teletypewriter::create_pty;

struct Context {
    device: wgpu::Device,
    surface: wgpu::Surface,
    queue: wgpu::Queue,
    staging_belt: wgpu::util::StagingBelt,
    renderer: Renderer,
    format: wgpu::TextureFormat,
    alpha_mode: wgpu::CompositeAlphaMode,
}

impl Context {
    pub async fn new(
        _scale: f32,
        adapter: wgpu::Adapter,
        surface: wgpu::Surface,
        config: &Rc<config::Config>,
        size: winit::dpi::PhysicalSize<u32>,
    ) -> Context {
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

        let staging_belt = wgpu::util::StagingBelt::new(2048);

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
                // present_mode: wgpu::PresentMode::Fifo,
            },
        );

        let renderer =
            Renderer::new(device_copy, format, config).expect("Create renderer");
        Context {
            device,
            queue,
            surface,
            staging_belt,
            renderer,
            format,
            alpha_mode,
        }
    }

    pub fn update_size(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        self.surface.configure(
            &self.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: self.format,
                width: size.width,
                height: size.height,
                view_formats: vec![],
                alpha_mode: self.alpha_mode,
                present_mode: wgpu::PresentMode::AutoVsync,
            },
        );
    }
}

pub struct Screen {
    ctx: Context,
    terminal: Arc<FairMutex<Crosswords<EventProxy>>>,
    messenger: Messenger,
    layout: Layout,
    clipboard: Clipboard,
}

impl Screen {
    pub async fn new(
        winit_window: &winit::window::Window,
        config: &Rc<config::Config>,
        event_proxy: EventProxy,
    ) -> Result<Screen, Box<dyn Error>> {
        let shell = std::env::var("SHELL")?;
        let size = winit_window.inner_size();
        let scale = winit_window.scale_factor();

        let mut layout = Layout::new(
            size.width as f32,
            size.height as f32,
            scale as f32,
            config.style.font_size,
        );
        let (columns, rows) = layout.compute();
        let pty = create_pty(&Cow::Borrowed(&shell), columns as u16, rows as u16);

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

        let scale = scale as f32;

        let ctx = Context::new(scale, adapter, surface, config, size).await;

        let event_proxy_clone = event_proxy.clone();
        let terminal: Arc<FairMutex<Crosswords<EventProxy>>> =
            Arc::new(FairMutex::new(Crosswords::new(columns, rows, event_proxy)));

        let machine = Machine::new(Arc::clone(&terminal), pty, event_proxy_clone)?;
        let channel = machine.channel();
        machine.spawn();
        let messenger = Messenger::new(channel);
        let clipboard = Clipboard::new();

        Ok(Screen {
            ctx,
            terminal,
            layout,
            messenger,
            clipboard,
        })
    }

    #[inline]
    pub fn propagate_modifiers_state(&mut self, state: winit::event::ModifiersState) {
        self.messenger.set_modifiers(state);
    }

    #[inline]
    pub fn input_char(&mut self, character: char) {
        if self.ctx.renderer.config.developer.enable_logs {
            println!("input_char: Received character {}", character);
        }

        if self.messenger.is_logo_pressed() && character == 'v' {
            let content = self.clipboard.get(ClipboardType::Clipboard);
            self.messenger.send_bytes(content.as_bytes().to_vec());
        } else {
            self.messenger.send_character(character);
        }
    }

    #[inline]
    pub fn input_keycode(
        &mut self,
        // _scancode: u32,
        virtual_keycode: Option<winit::event::VirtualKeyCode>,
    ) {
        let logs = self.ctx.renderer.config.developer.enable_logs;
        if logs {
            println!("input_keycode: received keycode {:?}", virtual_keycode);
        }

        if let Some(keycode) = virtual_keycode {
            let _ = self.messenger.send_keycode(keycode);
        } else if logs {
            println!("input_keycode: keycode not as Some");
        }
    }

    #[inline]
    pub fn skeleton(&mut self, color: wgpu::Color) {
        // TODO: WGPU caching
        let mut encoder =
            self.ctx
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Skeleton"),
                });
        let frame = self
            .ctx
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
        self.ctx.renderer.draw_queued(
            &self.ctx.device,
            &mut self.ctx.staging_belt,
            &mut encoder,
            view,
            (self.layout.width_u32, self.layout.height_u32),
        );
        self.ctx.staging_belt.finish();
        self.ctx.queue.submit(Some(encoder.finish()));
        frame.present();
        self.ctx.staging_belt.recall();
    }

    #[inline]
    pub fn render(&mut self, color: wgpu::Color) {
        let mut encoder =
            self.ctx
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Redraw"),
                });

        let frame = self
            .ctx
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
        self.ctx
            .renderer
            .term(visible_rows, self.layout.styles.term);

        self.ctx.renderer.draw_queued(
            &self.ctx.device,
            &mut self.ctx.staging_belt,
            &mut encoder,
            view,
            (self.layout.width_u32, self.layout.height_u32),
        );

        self.ctx.staging_belt.finish();
        self.ctx.queue.submit(Some(encoder.finish()));
        frame.present();
        self.ctx.staging_belt.recall();
    }

    #[inline]
    pub fn scroll(&mut self, _new_scroll_x_px: f64, new_scroll_y_px: f64) {
        // let width = self.layout.width as f64;
        // let height = self.layout.height as f64;

        // if self
        //     .ctx
        //     .terminal()
        //     .mode()
        //     .contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL)
        //     && !self.ctx.modifiers().shift()
        // {
        // // let multiplier = f64::from(self.ctx.config().terminal_config.scrolling.multiplier);

        // // self.layout.mouse_mut().accumulated_scroll.x += new_scroll_x_px;//* multiplier;
        // // self.layout.mouse_mut().accumulated_scroll.y += new_scroll_y_px;// * multiplier;

        // // // The chars here are the same as for the respective arrow keys.
        // let line_cmd = if new_scroll_y_px > 0. { b'A' } else { b'B' };
        // let column_cmd = if new_scroll_x_px > 0. { b'D' } else { b'C' };

        // // let lines = (self.layout.cursor.accumulated_scroll.y / self.layout.font_size as f64).abs() as usize;
        // let lines = 1;
        // let columns = (self.layout.cursor.accumulated_scroll.x / width).abs() as usize;

        // let mut content = Vec::with_capacity(3 * (lines + columns));

        // for _ in 0..lines {
        //     content.push(0x1b);
        //     content.push(b'O');
        //     content.push(line_cmd);
        // }

        // for _ in 0..columns {
        //     content.push(0x1b);
        //     content.push(b'O');
        //     content.push(column_cmd);
        // }

        // println!("{:?} {:?} {:?} {:?}", content, lines, columns, self.layout.cursor);
        // if content.len() > 0 {
        //     self.messenger.write_to_pty(content);
        // }
        // }

        self.layout.mouse_mut().accumulated_scroll.y +=
            new_scroll_y_px * self.layout.mouse.multiplier;
        let lines = (self.layout.mouse.accumulated_scroll.y
            / self.layout.font_size as f64) as i32;

        if lines != 0 {
            let mut terminal = self.terminal.lock();
            terminal.scroll_display(Scroll::Delta(lines));
            drop(terminal);
        }
    }

    pub fn layout(&mut self) -> &mut Layout {
        &mut self.layout
    }

    #[inline]
    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.ctx.update_size(new_size);
        self.layout
            .set_size(new_size.width, new_size.height)
            .update();
        let (c, l) = self.layout.compute();

        let mut terminal = self.terminal.lock();
        terminal.resize::<Layout>(self.layout.columns, self.layout.rows);
        drop(terminal);

        let _ = self.messenger.send_resize(
            new_size.width as u16,
            new_size.height as u16,
            c as u16,
            l as u16,
        );
    }

    // https://docs.rs/winit/latest/winit/dpi/
    #[allow(dead_code)]
    pub fn set_scale(&mut self, new_scale: f32, new_size: winit::dpi::PhysicalSize<u32>) {
        self.ctx.update_size(new_size);
        self.layout
            .set_scale(new_scale)
            .set_size(new_size.width, new_size.height)
            .update();
    }
}
