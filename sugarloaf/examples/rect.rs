extern crate png;
extern crate tokio;

use winit::platform::run_ondemand::EventLoopExtRunOnDemand;
use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::EventLoop,
    window::WindowBuilder,
};

use sugarloaf::components::rect::Rect;
use sugarloaf::layout::SugarloafLayout;
use sugarloaf::Sugarloaf;

#[tokio::main]
async fn main() {
    let mut event_loop = EventLoop::new().unwrap();
    let width = 1200.0;
    let height = 800.0;

    let window = WindowBuilder::new()
        .with_title("Rect example")
        .with_inner_size(LogicalSize::new(width, height))
        .with_resizable(true)
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
    )
    .await
    .expect("Sugarloaf instance should be created");

    let _ = event_loop.run_ondemand(move |event, _, control_flow| {
        control_flow.set_wait();

        match event {
            Event::Resumed => {
                window.request_redraw();
            }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => control_flow.set_exit(),
                WindowEvent::ScaleFactorChanged {
                    inner_size_writer: _,
                    scale_factor,
                    ..
                } => {
                    let new_inner_size = window.inner_size();
                    sugarloaf
                        .rescale(scale_factor as f32)
                        .resize(new_inner_size.width, new_inner_size.height)
                        .pile_rects(vec![
                            Rect {
                                position: [10.0, 10.0],
                                color: [1.0, 1.0, 1.0, 1.0],
                                size: [1.0, 1.0],
                            },
                            Rect {
                                position: [15.0, 10.0],
                                color: [1.0, 1.0, 1.0, 1.0],
                                size: [10.0, 10.0],
                            },
                            Rect {
                                position: [30.0, 20.0],
                                color: [1.0, 1.0, 0.0, 1.0],
                                size: [50.0, 50.0],
                            },
                            Rect {
                                position: [200., 200.0],
                                color: [0.0, 1.0, 0.0, 1.0],
                                size: [100.0, 100.0],
                            },
                            Rect {
                                position: [500.0, 200.0],
                                color: [1.0, 1.0, 0.0, 1.0],
                                size: [200.0, 200.0],
                            },
                        ])
                        .render();
                }
                _ => (),
            },
            Event::RedrawRequested { .. } => {
                sugarloaf
                    .pile_rects(vec![
                        Rect {
                            position: [10.0, 10.0],
                            color: [1.0, 1.0, 1.0, 1.0],
                            size: [1.0, 1.0],
                        },
                        Rect {
                            position: [15.0, 10.0],
                            color: [1.0, 1.0, 1.0, 1.0],
                            size: [10.0, 10.0],
                        },
                        Rect {
                            position: [30.0, 20.0],
                            color: [1.0, 1.0, 0.0, 1.0],
                            size: [50.0, 50.0],
                        },
                        Rect {
                            position: [200., 200.0],
                            color: [0.0, 1.0, 0.0, 1.0],
                            size: [100.0, 100.0],
                        },
                        Rect {
                            position: [500.0, 200.0],
                            color: [1.0, 1.0, 0.0, 1.0],
                            size: [200.0, 200.0],
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
