// Example demonstrating the unified rich text and quad rendering
// Run with: cargo run --example simple_unified

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use rio_window::application::ApplicationHandler;
use rio_window::event_loop::ControlFlow;
use rio_window::event_loop::{ActiveEventLoop, DeviceEvents};
use rio_window::window::{Window, WindowId};
use rio_window::{
    dpi::LogicalSize, event::WindowEvent, event_loop::EventLoop, window::WindowAttributes,
};
use std::error::Error;
use sugarloaf::{
    layout::RootStyle, Object, Rect, RichText,
    Sugarloaf, SugarloafWindow, SugarloafWindowSize,
};

fn main() {
    let width = 800.0;
    let height = 600.0;
    let window_event_loop = rio_window::event_loop::EventLoop::new().unwrap();
    let mut application = Application::new(&window_event_loop, width, height);
    let _ = application.run(window_event_loop);
}

struct Application {
    sugarloaf: Option<Sugarloaf<'static>>,
    window: Option<Window>,
    height: f32,
    width: f32,
}

impl Application {
    fn new(event_loop: &EventLoop<()>, width: f32, height: f32) -> Self {
        event_loop.listen_device_events(DeviceEvents::Never);

        Application {
            sugarloaf: None,
            window: None,
            width,
            height,
        }
    }

    fn run(&mut self, event_loop: EventLoop<()>) -> Result<(), Box<dyn Error>> {
        let result = event_loop.run_app(self);
        result.map_err(Into::into)
    }
}

impl ApplicationHandler for Application {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window_attributes = WindowAttributes::default()
            .with_title("Sugarloaf - Unified Rendering Demo")
            .with_inner_size(LogicalSize::new(self.width, self.height))
            .with_resizable(true);

        let window = event_loop.create_window(window_attributes).unwrap();

        let sugarloaf_window = SugarloafWindow {
            handle: window.window_handle().unwrap().as_raw(),
            display: window.display_handle().unwrap().as_raw(),
            scale: window.scale_factor() as f32,
            size: SugarloafWindowSize {
                width: self.width,
                height: self.height,
            },
        };

        let font_library = sugarloaf::font::FontLibrary::default();
        let sugarloaf = Sugarloaf::new(
            sugarloaf_window,
            sugarloaf::SugarloafRenderer::default(),
            &font_library,
            RootStyle::default(),
        )
        .expect("Sugarloaf instance should be created");

        self.sugarloaf = Some(sugarloaf);
        self.window = Some(window);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let sugarloaf = self.sugarloaf.as_mut().unwrap();
        let window = self.window.as_mut().unwrap();

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ScaleFactorChanged {
                scale_factor, ..
            } => {
                let scale_factor_f32 = scale_factor as f32;
                let new_inner_size = window.inner_size();
                sugarloaf.rescale(scale_factor_f32);
                sugarloaf.resize(new_inner_size.width, new_inner_size.height);
                window.request_redraw();
            }
            WindowEvent::Resized(new_size) => {
                sugarloaf.resize(new_size.width, new_size.height);
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                // Demonstrate the unified rendering - rectangles and text rendered together
                    // Background
                    Object::Rect(Rect::new(
                        0.0, 0.0,  // x, y
                        self.width, self.height,  // width, height
                        [0.1, 0.1, 0.2, 1.0], // dark blue background
                    )),
                    // Red rectangle
                    Object::Rect(Rect::new(
                        50.0, 50.0,   // x, y
                        200.0, 100.0,  // width, height
                        [1.0, 0.2, 0.2, 0.8], // red color with transparency
                    )),
                    // Green rectangle
                    Object::Rect(Rect::new(
                        300.0, 150.0, // x, y
                        150.0, 80.0,   // width, height
                        [0.2, 1.0, 0.2, 0.9], // green color
                    )),
                    // Blue rectangle
                    Object::Rect(Rect::new(
                        500.0, 300.0, // x, y
                        180.0, 120.0,  // width, height
                        [0.2, 0.2, 1.0, 0.7], // blue color
                    )),
                    // Rich text object would go here
                    // Object::RichText(RichText { ... }),
                ]);
                
                sugarloaf.render();
                event_loop.set_control_flow(ControlFlow::Wait);
            }
            _ => (),
        }
    }
}