extern crate png;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use rio_window::event::WindowEvent;
use rio_window::event_loop::ControlFlow;
use rio_window::platform::run_on_demand::EventLoopExtRunOnDemand;
use rio_window::{
    dpi::LogicalSize, event::Event, event_loop::EventLoop, window::WindowAttributes,
};
use sugarloaf::layout::RootStyle;
use sugarloaf::{Object, Quad, Sugarloaf, SugarloafWindow, SugarloafWindowSize};

fn main() {
    let mut event_loop = EventLoop::new().unwrap();
    let width = 400.0;
    let height = 200.0;

    let window_attribute = WindowAttributes::default()
        .with_title("Transparent example")
        .with_inner_size(LogicalSize::new(width, height))
        .with_resizable(true)
        .with_transparent(true);
    #[allow(deprecated)]
    let window = event_loop.create_window(window_attribute).unwrap();

    let scale_factor = window.scale_factor();
    let font_size = 60.;
    let line_height = 1.0;

    let sugarloaf_layout = RootStyle::new(scale_factor as f32, font_size, line_height);

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

    sugarloaf.set_background_color(None);

    #[allow(deprecated)]
    let _ = event_loop.run_on_demand(move |event, event_loop_window_target| {
        event_loop_window_target.set_control_flow(ControlFlow::Wait);

        match event {
            Event::Resumed => {
                window.request_redraw();
            }
            Event::WindowEvent { event, .. } => {
                if let WindowEvent::RedrawRequested = event {
                    let objects = vec![
                        Object::Quad(Quad {
                            position: [10.0, 10.0],
                            color: [1.0, 0.0, 1.0, 0.2],
                            size: [50.0, 50.0],
                            ..Quad::default()
                        }),
                        Object::Quad(Quad {
                            position: [115.0, 10.0],
                            color: [0.0, 1.0, 1.0, 0.5],
                            size: [50.0, 50.0],
                            ..Quad::default()
                        }),
                    ];

                    sugarloaf.set_objects(objects);
                    sugarloaf.render();
                }
            }
            _ => {
                event_loop_window_target.set_control_flow(ControlFlow::Wait);
            }
        }
    });
}
