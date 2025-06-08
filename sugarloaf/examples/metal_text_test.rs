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
    layout::{FragmentStyle, RootStyle},
    Object, RenderBackend, RichText, Sugarloaf, SugarloafRenderer, SugarloafWindow,
    SugarloafWindowSize,
};

struct App {
    window: Option<Window>,
    sugarloaf: Option<Sugarloaf<'static>>,
    rich_text_id: usize,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = event_loop
            .create_window(
                rio_window::window::WindowAttributes::default()
                    .with_title("Metal Text Rendering Test")
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

        let renderer = SugarloafRenderer {
            render_backend: RenderBackend::Metal,
            ..Default::default()
        };

        let font_library = sugarloaf::font::FontLibrary::default();
        let layout = RootStyle::default();

        match Sugarloaf::new(sugarloaf_window, renderer, &font_library, layout) {
            Ok(mut sugarloaf) => {
                println!("✅ Sugarloaf initialized successfully!");
                println!("🔧 Using Metal backend: {}", sugarloaf.is_using_metal());
                println!(
                    "📱 Backend info: {:?}",
                    sugarloaf.get_context().adapter_info.backend
                );
                println!("🎨 F16 support: {}", sugarloaf.get_context().supports_f16());

                // Create a rich text
                let rich_text_id = sugarloaf.create_rich_text();

                // Set a very obvious background so we can see if the window is working
                sugarloaf.set_background_color(Some(wgpu::Color {
                    r: 1.0, // Bright red background
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                }));

                self.rich_text_id = rich_text_id;
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
                    // Clear and rebuild the text content on each frame
                    sugarloaf.content().sel(self.rich_text_id).clear();
                    sugarloaf
                        .content()
                        .sel(self.rich_text_id)
                        .add_text(
                            "HELLO METAL BACKEND!",
                            FragmentStyle {
                                color: [0.0, 0.0, 0.0, 1.0], // Black text on red background
                                background_color: Some([1.0, 1.0, 1.0, 1.0]), // White background for text
                                ..Default::default()
                            },
                        )
                        .new_line()
                        .add_text(
                            "THIS TEXT SHOULD BE VISIBLE",
                            FragmentStyle {
                                color: [1.0, 1.0, 1.0, 1.0],                  // White text
                                background_color: Some([0.0, 0.0, 0.0, 1.0]), // Black background for text
                                ..Default::default()
                            },
                        )
                        .new_line()
                        .add_text(
                            "METAL TEXT RENDERING TEST",
                            FragmentStyle {
                                color: [1.0, 1.0, 0.0, 1.0],                  // Yellow text
                                background_color: Some([0.0, 0.0, 1.0, 1.0]), // Blue background for text
                                ..Default::default()
                            },
                        )
                        .build();

                    // Create a RichText object with positioning
                    let rich_text = RichText {
                        id: self.rich_text_id,
                        position: [50.0, 50.0], // Position the text
                        lines: None,
                    };

                    // Set the objects to render
                    sugarloaf.set_objects(vec![Object::RichText(rich_text)]);

                    // Render the frame
                    sugarloaf.render();
                }
                if let Some(ref window) = self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App {
        window: None,
        sugarloaf: None,
        rich_text_id: 0,
    };

    println!("🚀 Starting Metal Text Rendering Test...");
    println!("💡 Look for text in the window that opens.");
    println!("🔍 Check console output for backend information.");

    event_loop.run_app(&mut app).unwrap();
}
