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
    components::quad::Quad, Object, RenderBackend, Sugarloaf, SugarloafRenderer,
    SugarloafWindow, SugarloafWindowSize,
};

struct App {
    window: Option<Window>,
    sugarloaf: Option<Sugarloaf<'static>>,
    use_metal: bool,
    frame_count: u32,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = event_loop
            .create_window(
                rio_window::window::WindowAttributes::default()
                    .with_title("🚀 Sugarloaf Metal Backend - Quad Rendering Demo")
                    .with_inner_size(rio_window::dpi::LogicalSize::new(1000, 700)),
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
        let layout = sugarloaf::layout::RootStyle::default();

        match Sugarloaf::new(sugarloaf_window, renderer, &font_library, layout) {
            Ok(mut sugarloaf) => {
                println!("🎯 Sugarloaf Metal Backend Demo");
                println!("===============================");
                println!("✅ Sugarloaf initialized successfully!");
                println!("🔧 Using Metal backend: {}", sugarloaf.is_using_metal());
                println!("🎨 Render backend: {:?}", sugarloaf.render_backend());

                #[cfg(target_os = "macos")]
                if sugarloaf.is_using_metal() {
                    println!("🚀 Metal context is available and initialized!");
                    println!("📱 Using native Metal backend");
                    println!("🔧 Metal F16 support: {}", sugarloaf.get_context().supports_f16);
                    println!("💾 Metal supports half-precision for better performance!");
                } else {
                    println!("🌐 Using WebGPU backend for rendering");
                }

                println!("\nFeatures demonstrated:");
                println!("• Metal context initialization and device creation");
                println!("• F16 half-precision support detection");
                println!("• Backend switching between Metal and WebGPU");
                println!("• Animated quad rendering with rounded corners");
                println!("• Metal-optimized shaders (when Metal backend is active)");

                println!("\nControls:");
                println!("• SPACE - Switch between Metal and WebGPU backends");
                println!("• ESC   - Exit demo");
                println!();

                // Setup demo content
                self.setup_demo_content(&mut sugarloaf);

                self.sugarloaf = Some(sugarloaf);
            }
            Err(e) => {
                eprintln!("❌ Failed to initialize Sugarloaf: {:?}", e);
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
                    // Update animation
                    self.frame_count += 1;

                    // Update demo content inline to avoid borrowing issues
                    let mut objects = Vec::new();

                    // Animated quads with color cycling and gentle movement
                    for i in 0..10 {
                        for j in 0..6 {
                            let time = self.frame_count as f32 * 0.02;
                            let wave_offset = (i + j) as f32 * 0.3;

                            // Gentle wave motion
                            let wave_x = (time + wave_offset).sin() * 10.0;
                            let wave_y = (time * 0.7 + wave_offset).cos() * 5.0;

                            let x = 50.0 + i as f32 * 90.0 + wave_x;
                            let y = 50.0 + j as f32 * 100.0 + wave_y;

                            // Color cycling
                            let hue_base = (i * 6 + j) as f32 * 0.05;
                            let hue_shift = (time * 0.5).sin() * 0.3;
                            let hue = (hue_base + hue_shift).fract();
                            let color = hsv_to_rgb(hue, 0.7, 0.9);

                            // Size pulsing
                            let size_pulse =
                                (time * 2.0 + wave_offset).sin() * 5.0 + 75.0;

                            let quad = Quad {
                                color: [color.0, color.1, color.2, 0.85],
                                position: [x, y],
                                size: [size_pulse, size_pulse + 10.0],
                                border_color: [1.0, 1.0, 1.0, 0.8],
                                border_radius: [20.0, 20.0, 20.0, 20.0],
                                border_width: 2.5,
                                shadow_color: [0.0, 0.0, 0.0, 0.4],
                                shadow_offset: [4.0, 4.0],
                                shadow_blur_radius: 8.0,
                            };

                            objects.push(Object::Quad(quad));
                        }
                    }

                    sugarloaf.set_objects(objects);
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
                                "\n🔄 Switching to {} backend...",
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
                                let layout = sugarloaf::layout::RootStyle::default();

                                match Sugarloaf::new(
                                    sugarloaf_window,
                                    renderer,
                                    &font_library,
                                    layout,
                                ) {
                                    Ok(mut sugarloaf) => {
                                        println!(
                                            "✅ Successfully switched to {} backend!",
                                            if sugarloaf.is_using_metal() {
                                                "Metal"
                                            } else {
                                                "WebGPU"
                                            }
                                        );

                                        #[cfg(target_os = "macos")]
                                        if sugarloaf.is_using_metal() {
                                            println!("🌐 Metal backend active");
                                        } else {
                                            println!("🌐 WebGPU backend active");
                                        }

                                        self.setup_demo_content(&mut sugarloaf);
                                        self.sugarloaf = Some(sugarloaf);
                                    }
                                    Err(e) => {
                                        eprintln!("❌ Failed to switch backend: {:?}", e);
                                    }
                                }
                            }
                        }
                        rio_window::keyboard::PhysicalKey::Code(
                            rio_window::keyboard::KeyCode::Escape,
                        ) => {
                            event_loop.exit();
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
    fn setup_demo_content(&self, sugarloaf: &mut Sugarloaf) {
        let mut objects = Vec::new();

        // Create a beautiful grid of animated quads
        for i in 0..10 {
            for j in 0..6 {
                let x = 50.0 + i as f32 * 90.0;
                let y = 50.0 + j as f32 * 100.0;

                let hue = (i * 6 + j) as f32 * 0.05;
                let color = hsv_to_rgb(hue, 0.7, 0.9);

                let quad = Quad {
                    color: [color.0, color.1, color.2, 0.85],
                    position: [x, y],
                    size: [75.0, 85.0],
                    border_color: [1.0, 1.0, 1.0, 0.8],
                    border_radius: [20.0, 20.0, 20.0, 20.0],
                    border_width: 2.5,
                    shadow_color: [0.0, 0.0, 0.0, 0.4],
                    shadow_offset: [4.0, 4.0],
                    shadow_blur_radius: 8.0,
                };

                objects.push(Object::Quad(quad));
            }
        }

        sugarloaf.set_objects(objects);
    }

    fn update_demo_content(&mut self, sugarloaf: &mut Sugarloaf) {
        let mut objects = Vec::new();

        // Animated quads with color cycling and gentle movement
        for i in 0..10 {
            for j in 0..6 {
                let time = self.frame_count as f32 * 0.02;
                let wave_offset = (i + j) as f32 * 0.3;

                // Gentle wave motion
                let wave_x = (time + wave_offset).sin() * 10.0;
                let wave_y = (time * 0.7 + wave_offset).cos() * 5.0;

                let x = 50.0 + i as f32 * 90.0 + wave_x;
                let y = 50.0 + j as f32 * 100.0 + wave_y;

                // Color cycling
                let hue_base = (i * 6 + j) as f32 * 0.05;
                let hue_shift = (time * 0.5).sin() * 0.3;
                let hue = (hue_base + hue_shift).fract();
                let color = hsv_to_rgb(hue, 0.7, 0.9);

                // Size pulsing
                let size_pulse = (time * 2.0 + wave_offset).sin() * 5.0 + 75.0;

                let quad = Quad {
                    color: [color.0, color.1, color.2, 0.85],
                    position: [x, y],
                    size: [size_pulse, size_pulse + 10.0],
                    border_color: [1.0, 1.0, 1.0, 0.8],
                    border_radius: [20.0, 20.0, 20.0, 20.0],
                    border_width: 2.5,
                    shadow_color: [0.0, 0.0, 0.0, 0.4],
                    shadow_offset: [4.0, 4.0],
                    shadow_blur_radius: 8.0,
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
    println!("🚀 Sugarloaf Metal Backend Demo");
    println!("===============================");
    println!("This demo showcases Metal backend support with animated quad rendering");
    println!();

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App {
        window: None,
        sugarloaf: None,
        use_metal: true, // Start with Metal backend
        frame_count: 0,
    };

    event_loop.run_app(&mut app).unwrap();
}
