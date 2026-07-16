use librio_sugarloaf::{Renderer, Theme};
use librio_vt::{
    Engine, Key, KeyEvent, RenderState, Surface, SurfaceDelegate, SurfaceDesc,
};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use rio_window::application::ApplicationHandler;
use rio_window::dpi::LogicalSize;
use rio_window::event::WindowEvent;
use rio_window::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use rio_window::keyboard::{Key as WinitKey, NamedKey};
use rio_window::window::{Window, WindowAttributes, WindowId};
use std::sync::{Arc, Mutex};
use sugarloaf::{SugarloafWindow, SugarloafWindowSize};

struct Delegate {
    proxy: Mutex<EventLoopProxy<()>>,
}

impl SurfaceDelegate for Delegate {
    fn wakeup(&self, _surface: usize) {
        let _ = self.proxy.lock().unwrap().send_event(());
    }
}

struct App {
    engine: Engine,
    window: Option<Window>,
    renderer: Option<Renderer>,
    surface: Option<Surface>,
    state: Option<RenderState>,
}

impl App {
    fn sync_grid_size(&mut self) {
        let (Some(window), Some(renderer), Some(surface)) =
            (&self.window, &self.renderer, &self.surface)
        else {
            return;
        };
        let scale = window.scale_factor() as f32;
        let size = window.inner_size();
        let logical_w = size.width as f32 / scale;
        let logical_h = size.height as f32 / scale;
        let (cell_w, cell_h) = renderer.cell_size();
        let pad = renderer.padding();
        let cols = (((logical_w - pad * 2.0) / cell_w).floor() as u16).max(2);
        let rows = (((logical_h - pad * 2.0) / cell_h).floor() as u16).max(2);
        surface.resize(cols, rows, size.width as u16, size.height as u16);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = event_loop
            .create_window(
                WindowAttributes::default()
                    .with_title("librio term")
                    .with_inner_size(LogicalSize::new(900.0, 560.0)),
            )
            .unwrap();

        let scale = window.scale_factor() as f32;
        let size = window.inner_size();
        let sugarloaf_window = SugarloafWindow {
            handle: window.window_handle().unwrap().as_raw(),
            display: window.display_handle().unwrap().as_raw(),
            scale,
            size: SugarloafWindowSize {
                width: size.width as f32,
                height: size.height as f32,
            },
        };

        let renderer =
            Renderer::new(sugarloaf_window, 14.0, Theme::default()).expect("renderer");
        let surface = self
            .engine
            .create_surface(&SurfaceDesc::default())
            .expect("surface");
        let state = RenderState::new(&surface);

        self.window = Some(window);
        self.renderer = Some(renderer);
        self.surface = Some(surface);
        self.state = Some(state);
        self.sync_grid_size();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: ()) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(new_size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(new_size.width, new_size.height);
                }
                self.sync_grid_size();
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.rescale(scale_factor as f32);
                }
                self.sync_grid_size();
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                if !key_event.state.is_pressed() {
                    return;
                }
                let Some(surface) = &self.surface else { return };
                match &key_event.logical_key {
                    WinitKey::Named(named) => {
                        let key = match named {
                            NamedKey::Enter => Some(Key::Enter),
                            NamedKey::Tab => Some(Key::Tab),
                            NamedKey::Backspace => Some(Key::Backspace),
                            NamedKey::Escape => Some(Key::Escape),
                            NamedKey::ArrowUp => Some(Key::Up),
                            NamedKey::ArrowDown => Some(Key::Down),
                            NamedKey::ArrowLeft => Some(Key::Left),
                            NamedKey::ArrowRight => Some(Key::Right),
                            NamedKey::Home => Some(Key::Home),
                            NamedKey::End => Some(Key::End),
                            NamedKey::PageUp => Some(Key::PageUp),
                            NamedKey::PageDown => Some(Key::PageDown),
                            NamedKey::Delete => Some(Key::Delete),
                            NamedKey::Space => {
                                surface.text(" ");
                                None
                            }
                            _ => None,
                        };
                        if let Some(key) = key {
                            surface.key(KeyEvent::new(key));
                        }
                    }
                    _ => {
                        if let Some(text) = &key_event.text {
                            surface.text(text);
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let (Some(renderer), Some(state)) =
                    (&mut self.renderer, &mut self.state)
                {
                    state.update();
                    renderer.draw(state);
                    state.reset_dirty();
                }
            }
            _ => {}
        }
    }
}
fn main() {
    let event_loop = EventLoop::<()>::with_user_event().build().unwrap();
    let proxy = event_loop.create_proxy();
    let engine = Engine::new(Arc::new(Delegate {
        proxy: Mutex::new(proxy),
    }));
    let mut app = App {
        engine,
        window: None,
        renderer: None,
        surface: None,
        state: None,
    };
    event_loop.run_app(&mut app).unwrap();
}
