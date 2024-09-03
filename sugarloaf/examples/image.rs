extern crate png;

use rio_window::event::WindowEvent;
use rio_window::event_loop::ControlFlow;
use rio_window::platform::run_on_demand::EventLoopExtRunOnDemand;
use rio_window::{
    dpi::LogicalSize, event::Event, event_loop::EventLoop, window::WindowAttributes,
};

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use sugarloaf::layout::SugarloafLayout;
use sugarloaf::{Sugarloaf, SugarloafWindow, SugarloafWindowSize};

fn main() {
    let mut event_loop = EventLoop::new().unwrap();
    let width = 400.0;
    let height = 200.0;

    let window_attribute = WindowAttributes::default()
        .with_title("Image transparency example")
        .with_inner_size(LogicalSize::new(width, height))
        .with_resizable(true);
    #[allow(deprecated)]
    let window = event_loop.create_window(window_attribute).unwrap();

    let scale_factor = window.scale_factor();
    let font_size = 60.;
    let line_height = 1.0;

    let sugarloaf_layout = SugarloafLayout::new(
        width as f32,
        height as f32,
        (0.0, 0.0, 0.0),
        scale_factor as f32,
        font_size,
        line_height,
    );

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

    sugarloaf.set_background_image(&sugarloaf::ImageProperties {
        path: String::from("resources/rio-colors.png"),
        width: Some(400.),
        height: Some(400.),
        x: 0.,
        y: 0.,
    });

    #[allow(deprecated)]
    let _ = event_loop.run_on_demand(move |event, event_loop_window_target| {
        event_loop_window_target.set_control_flow(ControlFlow::Wait);

        match event {
            Event::Resumed => {
                window.request_redraw();
            }
            Event::WindowEvent { event, .. } => {
                if let WindowEvent::Resized(new_size) = event {
                    sugarloaf.resize(new_size.width, new_size.height);
                    window.request_redraw();
                }

                if let WindowEvent::RedrawRequested { .. } = event {
                    sugarloaf.render();
                }
            }
            _ => {
                event_loop_window_target.set_control_flow(ControlFlow::Wait);
            }
        }
    });
}
