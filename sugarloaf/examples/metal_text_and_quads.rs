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
    components::quad::Quad,
    layout::{FragmentStyle, RootStyle},
    Object, RenderBackend, RichText, Sugarloaf, SugarloafRenderer, SugarloafWindow,
    SugarloafWindowSize,
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
                    .with_title("Sugarloaf Metal Text & Quad Rendering Demo")
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
        let layout = RootStyle::default();

        match Sugarloaf::new(sugarloaf_window, renderer, &font_library, layout) {
            Ok(mut sugarloaf) => {
                println!("🎯 Sugarloaf Metal Text & Quad Demo");
                println!("===================================");
                println!("✅ Sugarloaf initialized successfully!");
                println!("🔧 Using Metal backend: {}", sugarloaf.is_using_metal());
                println!("🎨 Render backend: {:?}", sugarloaf.render_backend());

                #[cfg(target_os = "macos")]
                if sugarloaf.is_using_metal() {
                    println!("🚀 Metal context is available and initialized!");
                    println!("📱 Using native Metal backend");
                    println!(
                        "🔧 Metal F16 support: {}",
                        sugarloaf.get_context().supports_f16
                    );
                    println!("💾 Metal supports half-precision for better performance!");
                } else {
                    println!("🔧 Using WGPU with Metal backend");
                    println!(
                        "📱 Backend info: {:?}",
                        sugarloaf.get_context().adapter_info.backend
                    );
                }

                #[cfg(not(target_os = "macos"))]
                {
                    println!("🔧 Using WGPU with Metal backend");
                    println!(
                        "📱 Backend info: {:?}",
                        sugarloaf.get_context().adapter_info.backend
                    );
                }

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

                    // Update demo content
                    let mut objects = Vec::new();

                    for i in 0..8 {
                        for j in 0..5 {
                            let x = 50.0 + i as f32 * 110.0;
                            let y = 100.0 + j as f32 * 110.0;

                            let time_offset =
                                (self.frame_count as f32 * 0.02) + (i + j) as f32 * 0.1;
                            let hue = (time_offset.sin() * 0.5 + 0.5) * 0.8;
                            let color = hsv_to_rgb(hue, 0.6, 0.8);

                            let quad = Quad {
                                color: [color.0, color.1, color.2, 0.8],
                                position: [x, y],
                                size: [90.0, 90.0],
                                border_color: [1.0, 1.0, 1.0, 0.9],
                                border_radius: [15.0, 15.0, 15.0, 15.0],
                                border_width: 2.0,
                                shadow_color: [0.0, 0.0, 0.0, 0.3],
                                shadow_offset: [3.0, 3.0],
                                shadow_blur_radius: 6.0,
                            };

                            objects.push(Object::Quad(quad));
                        }
                    }

                    // Add the text object back - this was missing!
                    objects.push(Object::RichText(RichText {
                        id: 1, // The text_id from setup_demo_content
                        position: [10., 10.],
                        lines: None,
                    }));

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
                                let layout = RootStyle::default();

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
                                            println!(
                                                "🚀 Switched to native Metal backend!"
                                            );
                                            println!(
                                                "🔧 Metal F16 support: {}",
                                                sugarloaf.get_context().supports_f16
                                            );
                                        } else {
                                            println!("🔧 Using WGPU with Metal backend");
                                            println!(
                                                "📱 Backend info: {:?}",
                                                sugarloaf
                                                    .get_context()
                                                    .adapter_info
                                                    .backend
                                            );
                                        }

                                        #[cfg(not(target_os = "macos"))]
                                        {
                                            println!("🔧 Using WGPU with Metal backend");
                                            println!(
                                                "📱 Backend info: {:?}",
                                                sugarloaf
                                                    .get_context()
                                                    .adapter_info
                                                    .backend
                                            );
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

        // Create colorful background quads
        for i in 0..8 {
            for j in 0..5 {
                let x = 50.0 + i as f32 * 110.0;
                let y = 100.0 + j as f32 * 110.0;

                let hue = (i * 5 + j) as f32 * 0.1;
                let color = hsv_to_rgb(hue, 0.6, 0.8);

                let quad = Quad {
                    color: [color.0, color.1, color.2, 0.8],
                    position: [x, y],
                    size: [90.0, 90.0],
                    border_color: [1.0, 1.0, 1.0, 0.9],
                    border_radius: [15.0, 15.0, 15.0, 15.0],
                    border_width: 2.0,
                    shadow_color: [0.0, 0.0, 0.0, 0.3],
                    shadow_offset: [3.0, 3.0],
                    shadow_blur_radius: 6.0,
                };

                objects.push(Object::Quad(quad));
            }
        }

        // Create text content
        let text_id = sugarloaf.create_rich_text();
        println!("Created rich text with ID: {}", text_id);

        // Title text
        sugarloaf.content().sel(text_id).add_text(
            "🚀 Metal Backend Demo",
            FragmentStyle {
                font_id: 0,
                width: 1.0,
                font_attrs: Default::default(),
                color: [1.0, 1.0, 1.0, 1.0],
                background_color: None,
                font_vars: Default::default(),
                decoration: None,
                decoration_color: None,
                cursor: None,
                media: None,
                drawable_char: None,
            },
        );

        sugarloaf.content().sel(text_id).add_text(
            "\n\nThis demo showcases Sugarloaf's Metal backend support:",
            FragmentStyle {
                font_id: 0,
                width: 1.0,
                font_attrs: Default::default(),
                color: [0.9, 0.9, 0.9, 1.0],
                background_color: None,
                font_vars: Default::default(),
                decoration: None,
                decoration_color: None,
                cursor: None,
                media: None,
                drawable_char: None,
            },
        );

        sugarloaf.content().sel(text_id).add_text(
            "\n• Native Metal context initialization",
            FragmentStyle {
                font_id: 0,
                width: 1.0,
                font_attrs: Default::default(),
                color: [0.5, 1.0, 0.5, 1.0],
                background_color: None,
                font_vars: Default::default(),
                decoration: None,
                decoration_color: None,
                cursor: None,
                media: None,
                drawable_char: None,
            },
        );

        sugarloaf.content().sel(text_id).add_text(
            "\n• F16 half-precision support detection",
            FragmentStyle {
                font_id: 0,
                width: 1.0,
                font_attrs: Default::default(),
                color: [0.5, 1.0, 0.5, 1.0],
                background_color: None,
                font_vars: Default::default(),
                decoration: None,
                decoration_color: None,
                cursor: None,
                media: None,
                drawable_char: None,
            },
        );

        sugarloaf.content().sel(text_id).add_text(
            "\n• Backend switching at runtime",
            FragmentStyle {
                font_id: 0,
                width: 1.0,
                font_attrs: Default::default(),
                color: [0.5, 1.0, 0.5, 1.0],
                background_color: None,
                font_vars: Default::default(),
                decoration: None,
                decoration_color: None,
                cursor: None,
                media: None,
                drawable_char: None,
            },
        );

        sugarloaf.content().sel(text_id).add_text(
            "\n• Quad rendering with rounded corners",
            FragmentStyle {
                font_id: 0,
                width: 1.0,
                font_attrs: Default::default(),
                color: [0.5, 1.0, 0.5, 1.0],
                background_color: None,
                font_vars: Default::default(),
                decoration: None,
                decoration_color: None,
                cursor: None,
                media: None,
                drawable_char: None,
            },
        );

        let backend_name = if sugarloaf.is_using_metal() {
            "Metal"
        } else {
            "WebGPU"
        };
        sugarloaf.content().sel(text_id).add_text(
            &format!("\n\nCurrent Backend: {}", backend_name),
            FragmentStyle {
                font_id: 0,
                width: 1.0,
                font_attrs: Default::default(),
                color: [1.0, 0.8, 0.2, 1.0],
                background_color: None,
                font_vars: Default::default(),
                decoration: None,
                decoration_color: None,
                cursor: None,
                media: None,
                drawable_char: None,
            },
        );

        #[cfg(target_os = "macos")]
        if sugarloaf.is_using_metal() {
            let device_name = "\nMetal Device: Native Metal Backend";
            let f16_support =
                format!("\nF16 Support: {}", sugarloaf.get_context().supports_f16);

            sugarloaf.content().sel(text_id).add_text(
                device_name,
                FragmentStyle {
                    font_id: 0,
                    width: 1.0,
                    font_attrs: Default::default(),
                    color: [0.8, 0.8, 1.0, 1.0],
                    background_color: None,
                    font_vars: Default::default(),
                    decoration: None,
                    decoration_color: None,
                    cursor: None,
                    media: None,
                    drawable_char: None,
                },
            );

            sugarloaf.content().sel(text_id).add_text(
                &f16_support,
                FragmentStyle {
                    font_id: 0,
                    width: 1.0,
                    font_attrs: Default::default(),
                    color: [0.8, 0.8, 1.0, 1.0],
                    background_color: None,
                    font_vars: Default::default(),
                    decoration: None,
                    decoration_color: None,
                    cursor: None,
                    media: None,
                    drawable_char: None,
                },
            );
        } else {
            let backend_info = format!(
                "\nBackend: {:?}",
                sugarloaf.get_context().adapter_info.backend
            );

            sugarloaf.content().sel(text_id).add_text(
                &backend_info,
                FragmentStyle {
                    font_id: 0,
                    width: 1.0,
                    font_attrs: Default::default(),
                    color: [0.8, 0.8, 1.0, 1.0],
                    background_color: None,
                    font_vars: Default::default(),
                    decoration: None,
                    decoration_color: None,
                    cursor: None,
                    media: None,
                    drawable_char: None,
                },
            );
        }

        #[cfg(not(target_os = "macos"))]
        {
            let backend_info = format!(
                "\nBackend: {:?}",
                sugarloaf.get_context().adapter_info.backend
            );

            sugarloaf.content().sel(text_id).add_text(
                &backend_info,
                FragmentStyle {
                    font_id: 0,
                    width: 1.0,
                    font_attrs: Default::default(),
                    color: [0.8, 0.8, 1.0, 1.0],
                    background_color: None,
                    font_vars: Default::default(),
                    decoration: None,
                    decoration_color: None,
                    cursor: None,
                    media: None,
                    drawable_char: None,
                },
            );
        }

        sugarloaf.content().sel(text_id).add_text(
            "\n\nControls:",
            FragmentStyle {
                font_id: 0,
                width: 1.0,
                font_attrs: Default::default(),
                color: [1.0, 1.0, 0.5, 1.0],
                background_color: None,
                font_vars: Default::default(),
                decoration: None,
                decoration_color: None,
                cursor: None,
                media: None,
                drawable_char: None,
            },
        );

        sugarloaf.content().sel(text_id).add_text(
            "\nSPACE - Switch backend",
            FragmentStyle {
                font_id: 0,
                width: 1.0,
                font_attrs: Default::default(),
                color: [0.9, 0.9, 0.9, 1.0],
                background_color: None,
                font_vars: Default::default(),
                decoration: None,
                decoration_color: None,
                cursor: None,
                media: None,
                drawable_char: None,
            },
        );

        sugarloaf.content().sel(text_id).add_text(
            "\nESC   - Exit demo",
            FragmentStyle {
                font_id: 0,
                width: 1.0,
                font_attrs: Default::default(),
                color: [0.9, 0.9, 0.9, 1.0],
                background_color: None,
                font_vars: Default::default(),
                decoration: None,
                decoration_color: None,
                cursor: None,
                media: None,
                drawable_char: None,
            },
        );

        objects.push(Object::RichText(RichText {
            id: text_id,
            position: [10., 10.],
            lines: None,
        }));

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
    println!("🎯 Starting Sugarloaf Metal Text & Quad Demo");
    println!(
        "This demo showcases both text and quad rendering with Metal backend support"
    );
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
