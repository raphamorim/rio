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
    components::quad::Quad, layout::RootStyle, Object, RenderBackend, Sugarloaf,
    SugarloafRenderer, SugarloafWindow, SugarloafWindowSize,
};

struct App {
    window: Option<Window>,
    sugarloaf: Option<Sugarloaf<'static>>,
    use_metal: bool,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = event_loop
            .create_window(
                rio_window::window::WindowAttributes::default()
                    .with_title("Sugarloaf Metal + WebGPU Quad Rendering Example")
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

        // Configure renderer to use Metal or WebGPU backend
        let renderer = SugarloafRenderer {
            render_backend: if self.use_metal {
                #[cfg(target_os = "macos")]
                {
                    RenderBackend::Metal
                }
                #[cfg(not(target_os = "macos"))]
                {
                    RenderBackend::WebGpu
                }
            } else {
                RenderBackend::WebGpu
            },
            ..Default::default()
        };

        let font_library = sugarloaf::font::FontLibrary::default();
        let layout = RootStyle::default();

        match Sugarloaf::new(sugarloaf_window, renderer, &font_library, layout) {
            Ok(mut sugarloaf) => {
                println!("Sugarloaf initialized successfully!");
                println!("Using Metal backend: {}", sugarloaf.is_using_metal());
                println!("Render backend: {:?}", sugarloaf.render_backend());

                // Create some colorful quads to demonstrate rendering
                self.setup_demo_quads(&mut sugarloaf);

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
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == rio_window::event::ElementState::Pressed {
                    match event.physical_key {
                        rio_window::keyboard::PhysicalKey::Code(
                            rio_window::keyboard::KeyCode::Space,
                        ) => {
                            // Toggle between Metal and WebGPU
                            self.use_metal = !self.use_metal;
                            println!(
                                "Switching to {} backend",
                                if self.use_metal { "Metal" } else { "WebGPU" }
                            );

                            // Recreate Sugarloaf with new backend
                            if let Some(window) = &self.window {
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

                                let renderer = SugarloafRenderer {
                                    render_backend: if self.use_metal {
                                        #[cfg(target_os = "macos")]
                                        {
                                            RenderBackend::Metal
                                        }
                                        #[cfg(not(target_os = "macos"))]
                                        {
                                            RenderBackend::WebGpu
                                        }
                                    } else {
                                        RenderBackend::WebGpu
                                    },
                                    ..Default::default()
                                };

                                let font_library =
                                    sugarloaf::font::FontLibrary::default();
                                let layout = RootStyle::default();

                                match Sugarloaf::new(
                                    sugarloaf_window,
                                    renderer,
                                    &font_library,
                                    layout,
                                ) {
                                    Ok(mut sugarloaf) => {
                                        println!(
                                            "Switched to {} backend successfully!",
                                            if sugarloaf.is_using_metal() {
                                                "Metal"
                                            } else {
                                                "WebGPU"
                                            }
                                        );
                                        self.setup_demo_quads(&mut sugarloaf);
                                        self.sugarloaf = Some(sugarloaf);
                                    }
                                    Err(e) => {
                                        eprintln!("Failed to switch backend: {:?}", e);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
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

impl App {
    fn setup_demo_quads(&self, sugarloaf: &mut Sugarloaf) {
        let mut objects = Vec::new();

        // Create a grid of colorful quads
        for i in 0..5 {
            for j in 0..3 {
                let x = 50.0 + i as f32 * 120.0;
                let y = 50.0 + j as f32 * 120.0;

                let hue = (i * 3 + j) as f32 * 0.2;
                let color = hsv_to_rgb(hue, 0.8, 0.9);

                let quad = Quad {
                    color: [color.0, color.1, color.2, 1.0],
                    position: [x, y],
                    size: [100.0, 100.0],
                    border_color: [1.0, 1.0, 1.0, 1.0],
                    border_radius: [10.0, 10.0, 10.0, 10.0],
                    border_width: 2.0,
                    shadow_color: [0.0, 0.0, 0.0, 0.3],
                    shadow_offset: [2.0, 2.0],
                    shadow_blur_radius: 4.0,
                };

                objects.push(Object::Quad(quad));
            }
        }

        sugarloaf.set_objects(objects);
    }
}

// Helper function to convert HSV to RGB
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let c = v * s;
    let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r, g, b) = if h < 1.0 / 6.0 {
        (c, x, 0.0)
    } else if h < 2.0 / 6.0 {
        (x, c, 0.0)
    } else if h < 3.0 / 6.0 {
        (0.0, c, x)
    } else if h < 4.0 / 6.0 {
        (0.0, x, c)
    } else if h < 5.0 / 6.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    (r + m, g + m, b + m)
}

fn main() {
    println!("Starting Sugarloaf Metal + WebGPU Quad Rendering Example");
    println!("Press SPACE to toggle between Metal and WebGPU backends");

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App {
        window: None,
        sugarloaf: None,
        use_metal: true, // Start with Metal backend
    };

    event_loop.run_app(&mut app).unwrap();
}
