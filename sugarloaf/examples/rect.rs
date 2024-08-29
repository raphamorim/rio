extern crate png;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use sugarloaf::components::rect::Rect;
use sugarloaf::layout::SugarloafLayout;
use sugarloaf::{Object, Sugarloaf, SugarloafWindow, SugarloafWindowSize};
use winit::event_loop::ControlFlow;
use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::EventLoop,
    window::WindowAttributes,
};

fn main() {
    let mut event_loop = EventLoop::new().unwrap();
    let width = 1200.0;
    let height = 800.0;

    let window_attribute = WindowAttributes::default()
        .with_title("Rect example")
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

    let font_library = sugarloaf::font::FontLibrary::default();
    let mut sugarloaf = Sugarloaf::new(
        sugarloaf_window,
        sugarloaf::SugarloafRenderer::default(),
        &font_library,
        sugarloaf_layout,
    )
    .expect("Sugarloaf instance should be created");

    #[allow(deprecated)]
    let _ = event_loop.run_on_demand(move |event, event_loop_window_target| {
        event_loop_window_target.set_control_flow(ControlFlow::Wait);

        let mut objects = Vec::with_capacity(5);
        objects.push(Object::Rect(Rect {
            position: [10.0, 10.0],
            color: [1.0, 1.0, 1.0, 1.0],
            size: [1.0, 1.0],
        }));
        objects.push(Object::Rect(Rect {
            position: [15.0, 10.0],
            color: [1.0, 1.0, 1.0, 1.0],
            size: [10.0, 10.0],
        }));
        objects.push(Object::Rect(Rect {
            position: [30.0, 20.0],
            color: [1.0, 1.0, 0.0, 1.0],
            size: [50.0, 50.0],
        }));
        objects.push(Object::Rect(Rect {
            position: [200., 200.0],
            color: [0.0, 1.0, 0.0, 1.0],
            size: [100.0, 100.0],
        }));
        objects.push(Object::Rect(Rect {
            position: [500.0, 200.0],
            color: [1.0, 1.0, 0.0, 1.0],
            size: [200.0, 200.0],
        }));

        match event {
            Event::Resumed => {
                window.request_redraw();
            }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => event_loop_window_target.exit(),
                WindowEvent::ScaleFactorChanged {
                    inner_size_writer: _,
                    scale_factor,
                    ..
                } => {
                    let new_inner_size = window.inner_size();
                    sugarloaf.rescale(scale_factor as f32);
                    sugarloaf.resize(new_inner_size.width, new_inner_size.height);

                    sugarloaf.set_objects(objects);
                    sugarloaf.render();
                }
                WindowEvent::RedrawRequested { .. } => {
                    sugarloaf.set_objects(objects);
                    sugarloaf.render();
                }
                _ => (),
            },
            _ => {
                event_loop_window_target.set_control_flow(ControlFlow::Wait);
            }
        }
    });
}
