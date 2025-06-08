use rio_window::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    raw_window_handle::{HasDisplayHandle, HasWindowHandle},
    window::{Window, WindowId},
};
use sugarloaf::{
    RenderBackend, Sugarloaf, SugarloafRenderer, SugarloafWindow, SugarloafWindowSize,
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
                    .with_title("Sugarloaf Metal Backend Demo")
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
        let layout = sugarloaf::layout::RootStyle::default();

        match Sugarloaf::new(sugarloaf_window, renderer, &font_library, layout) {
            Ok(sugarloaf) => {
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
                                    Ok(sugarloaf) => {
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

fn main() {
    println!("🎯 Sugarloaf Metal Backend Demo");
    println!("================================");
    println!("This demo shows Metal backend initialization and context creation.");
    println!();
    println!("Controls:");
    println!("  SPACE - Toggle between Metal and WebGPU backends");
    println!("  ESC   - Exit");
    println!();

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App {
        window: None,
        sugarloaf: None,
        use_metal: true, // Start with Metal backend
    };

    event_loop.run_app(&mut app).unwrap();
}
