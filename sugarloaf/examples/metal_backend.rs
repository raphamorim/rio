use rio_window::application::ApplicationHandler;
use rio_window::event::WindowEvent;
use rio_window::event_loop::ActiveEventLoop;
use rio_window::event_loop::ControlFlow;
use rio_window::event_loop::EventLoop;
use rio_window::raw_window_handle::HasDisplayHandle;
use rio_window::raw_window_handle::HasWindowHandle;
use rio_window::window::Window;
use rio_window::window::WindowId;

use sugarloaf::{
    RenderBackend, Sugarloaf, SugarloafRenderer, SugarloafWindow, SugarloafWindowSize,
};

struct App {
    window: Option<Window>,
    sugarloaf: Option<Sugarloaf<'static>>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = event_loop
            .create_window(
                rio_window::window::WindowAttributes::default()
                    .with_title("Sugarloaf Metal Backend Example")
                    .with_inner_size(rio_window::dpi::LogicalSize::new(800, 600)),
            )
            .unwrap();

        let size = window.inner_size();
        let scale_factor = window.scale_factor() as f32;

        let sugarloaf_window = SugarloafWindow {
            handle: window.window_handle().unwrap().as_raw(),
            display: window.display_handle().unwrap().as_raw(),
            size: SugarloafWindowSize {
                width: size.width as f32,
                height: size.height as f32,
            },
            scale: scale_factor,
        };

        // Configure renderer to use Metal backend
        let renderer = SugarloafRenderer {
            render_backend: {
                #[cfg(target_os = "macos")]
                {
                    RenderBackend::Metal
                }
                #[cfg(not(target_os = "macos"))]
                {
                    RenderBackend::WebGpu
                }
            },
            ..Default::default()
        };

        let font_library = sugarloaf::font::FontLibrary::default();
        let layout = sugarloaf::layout::RootStyle::default();

        match Sugarloaf::new(sugarloaf_window, renderer, &font_library, layout) {
            Ok(mut sugarloaf) => {
                println!("Sugarloaf initialized successfully!");
                println!("Using Metal backend: {}", sugarloaf.is_using_metal());
                println!("Render backend: {:?}", sugarloaf.render_backend());

                self.sugarloaf = Some(sugarloaf);
            }
            Err(e) => {
                eprintln!("Failed to initialize Sugarloaf: {:?}", e);
                event_loop.exit();
                return;
            }
        }

        self.window = Some(window);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                if let Some(ref mut sugarloaf) = self.sugarloaf {
                    sugarloaf.render();
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(ref mut sugarloaf) = self.sugarloaf {
                    sugarloaf.resize(size.width, size.height);
                }
                if let Some(ref window) = self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(ref window) = self.window {
            window.request_redraw();
        }
    }
}

fn main() {
    // Simple logging setup
    println!("Starting Sugarloaf Metal Backend Example");

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App {
        window: None,
        sugarloaf: None,
    };

    event_loop.run_app(&mut app).unwrap();
}
