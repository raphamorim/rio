extern crate png;
extern crate tokio;

use winit::platform::run_ondemand::EventLoopExtRunOnDemand;
use winit::{
    dpi::LogicalSize, event::Event, event_loop::EventLoop, window::WindowBuilder,
};

use sugarloaf::components::rect::Rect;
use sugarloaf::layout::SugarloafLayout;
use sugarloaf::Sugarloaf;

#[tokio::main]
async fn main() {
    let mut event_loop = EventLoop::new().unwrap();
    let width = 400.0;
    let height = 200.0;

    let window = WindowBuilder::new()
        .with_title("Rect transparency example")
        .with_inner_size(LogicalSize::new(width, height))
        .with_resizable(true)
        .with_transparent(true)
        .build(&event_loop)
        .unwrap();

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
        (2, 1),
    );

    let mut sugarloaf = Sugarloaf::new(
        &window,
        wgpu::PowerPreference::HighPerformance,
        sugarloaf::font::fonts::SugarloafFonts::default(),
        sugarloaf_layout,
        None,
    )
    .await
    .expect("Sugarloaf instance should be created");

    sugarloaf.set_background_color(wgpu::Color::TRANSPARENT);

    let _ = event_loop.run_ondemand(move |event, _, control_flow| {
        control_flow.set_wait();

        match event {
            Event::Resumed => {
                window.request_redraw();
            }
            Event::RedrawRequested { .. } => {
                sugarloaf
                    .pile_rects(vec![
                        Rect {
                            position: [10.0, 10.0],
                            color: [1.0, 0.0, 1.0, 0.2],
                            size: [100.0, 100.0],
                        },
                        Rect {
                            position: [115.0, 10.0],
                            color: [0.0, 1.0, 1.0, 0.5],
                            size: [100.0, 100.0],
                        },
                    ])
                    .render();
            }
            _ => {
                *control_flow = winit::event_loop::ControlFlow::Wait;
            }
        }
    });
}
