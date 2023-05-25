extern crate png;
extern crate tokio;

use std::{io, path::Path};
use winit::platform::run_return::EventLoopExtRunReturn;
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::EventLoop,
    window::WindowBuilder,
};

use sugarloaf::components::rect::Rect;
use sugarloaf::Sugarloaf;

#[allow(unused)]
fn write_png(
    path: impl AsRef<Path>,
    width: u32,
    height: u32,
    data: &[u8],
    compression: png::Compression,
) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let file = io::BufWriter::new(std::fs::File::create(path).unwrap());

        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(compression);
        let mut writer = encoder.write_header().unwrap();

        writer.write_image_data(data).unwrap();
    }
}

#[tokio::main]
async fn main() {
    let mut event_loop = EventLoop::new();
    let width = 1200.0;
    let height = 800.0;

    let window = WindowBuilder::new()
        .with_title("Rect example")
        .with_inner_size(LogicalSize::new(width, height))
        .with_resizable(true)
        .build(&event_loop)
        .unwrap();

    let mut sugarloaf = Sugarloaf::new(
        &window,
        wgpu::PowerPreference::HighPerformance,
        sugarloaf::font::DEFAULT_FONT_NAME.to_string(),
    )
    .await
    .expect("Sugarloaf instance should be created");

    event_loop.run_return(move |event, _, control_flow| {
        control_flow.set_wait();

        match event {
            Event::Resumed => {
                window.request_redraw();
            }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => control_flow.set_exit(),
                WindowEvent::KeyboardInput {
                    input:
                        KeyboardInput {
                            virtual_keycode: Some(VirtualKeyCode::Space),
                            state: ElementState::Released,
                            ..
                        },
                    ..
                } => {
                    //
                }
                WindowEvent::ScaleFactorChanged {
                    new_inner_size,
                    scale_factor,
                    ..
                } => {
                    sugarloaf
                        .rescale(scale_factor as f32)
                        .resize(new_inner_size.width, new_inner_size.height)
                        .pile_rect(vec![
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
                    .pile_rect(vec![
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

                // write_png(
                //     "/tmp/rio-rect.png",
                //     width as u32,
                //     height as u32,
                //     &[1],
                //     png::Compression::Best,
                // );
            }
            _ => {
                *control_flow = winit::event_loop::ControlFlow::Wait;
            }
        }
    });
}
