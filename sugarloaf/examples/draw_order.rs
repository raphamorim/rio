/// Draw Order Example
///
/// Demonstrates the draw order system (painter's algorithm).
/// Higher order values render on top of lower values.
///
/// This example shows:
/// - Three overlapping rectangles with different draw orders
/// - Red (order=0) renders first (bottom)
/// - Green (order=1) renders second (middle)
/// - Blue (order=2) renders last (top)
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use rio_window::application::ApplicationHandler;
use rio_window::event_loop::ControlFlow;
use rio_window::event_loop::{ActiveEventLoop, DeviceEvents};
use rio_window::window::{Window, WindowId};
use rio_window::{
    dpi::LogicalSize, event::WindowEvent, event_loop::EventLoop, window::WindowAttributes,
};
use std::error::Error;
use sugarloaf::{layout::RootStyle, Sugarloaf, SugarloafWindow, SugarloafWindowSize};

fn main() {
    let width = 400.0;
    let height = 300.0;
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
    fn resumed(&mut self, active_event_loop: &ActiveEventLoop) {
        let window_attribute = WindowAttributes::default()
            .with_title("Draw Order Example")
            .with_inner_size(LogicalSize::new(self.width, self.height))
            .with_resizable(true);
        let window = active_event_loop.create_window(window_attribute).unwrap();

        let scale_factor = window.scale_factor();
        let font_size = 24.;

        let sugarloaf_layout = RootStyle::new(scale_factor as f32, font_size, 1.0);

        let size = window.inner_size();
        let sugarloaf_window = SugarloafWindow {
            handle: window.window_handle().unwrap().into(),
            display: window.display_handle().unwrap().into(),
            scale: scale_factor as f32,
            size: SugarloafWindowSize {
                width: size.width as f32,
                height: size.height as f32,
            },
        };

        let mut sugarloaf = Sugarloaf::new(
            sugarloaf_window,
            sugarloaf::SugarloafRenderer::default(),
            &sugarloaf::font::FontLibrary::default(),
            sugarloaf_layout,
        )
        .expect("Sugarloaf instance should be created");

        sugarloaf.set_background_color(Some(wgpu::Color {
            r: 0.1,
            g: 0.1,
            b: 0.1,
            a: 1.0,
        }));
        window.request_redraw();

        self.sugarloaf = Some(sugarloaf);
        self.window = Some(window);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if self.sugarloaf.is_none() || self.window.is_none() {
            return;
        }

        let sugarloaf = self.sugarloaf.as_mut().unwrap();
        let window = self.window.as_mut().unwrap();

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
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
                // Draw order demonstration:
                // Even though we draw red first, then green, then blue,
                // the order parameter controls the final layering.
                //
                // order=0 -> bottom (red)
                // order=1 -> middle (green)
                // order=2 -> top (blue)

                let red = [1.0, 0.0, 0.0, 1.0];
                let green = [0.0, 1.0, 0.0, 1.0];
                let blue = [0.0, 0.0, 1.0, 1.0];

                // These rectangles overlap
                // Red: large background rect (order=0, renders first/bottom)
                sugarloaf.rect(None, 50., 50., 200., 150., red, 0.0, 0);

                // Green: medium rect offset (order=1, renders second/middle)
                sugarloaf.rect(None, 100., 80., 200., 150., green, 0.0, 1);

                // Blue: small rect offset more (order=2, renders last/top)
                sugarloaf.rect(None, 150., 110., 200., 150., blue, 0.0, 2);

                // Demonstration of order independence from call order:
                // Even though yellow is drawn AFTER white, yellow has lower order
                // so white will render on top of yellow
                let yellow = [1.0, 1.0, 0.0, 1.0];
                let white = [1.0, 1.0, 1.0, 1.0];

                // Yellow drawn first but with order=10
                sugarloaf.rect(None, 20., 200., 80., 60., yellow, 0.0, 10);
                // White drawn second but with order=20, so white is on top
                sugarloaf.rect(None, 60., 220., 80., 60., white, 0.0, 20);

                sugarloaf.render();
                event_loop.set_control_flow(ControlFlow::Wait);
            }
            _ => (),
        }
    }
}
