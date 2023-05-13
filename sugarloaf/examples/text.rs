extern crate tokio;

use winit::platform::run_return::EventLoopExtRunReturn;
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::EventLoop,
    window::WindowBuilder,
};

use sugarloaf::core::{Sugar, SugarloafStyle};
use sugarloaf::{RendererTarget, Sugarloaf};

#[tokio::main]
async fn main() {
    let mut event_loop = EventLoop::new();
    let width = 1200.0;
    let height = 800.0;

    let window = WindowBuilder::new()
        .with_title("Text example")
        .with_inner_size(LogicalSize::new(1200.0, 800.0))
        .with_resizable(true)
        .build(&event_loop)
        .unwrap();

    let mut sugarloaf = Sugarloaf::new(
        RendererTarget::Desktop,
        &window,
        wgpu::PowerPreference::HighPerformance,
        "Firamono".to_string(),
    )
    .await
    .expect("Sugarloaf instance should be created");

    let scale_factor = 2.;
    let font_size = 180.;

    let style = SugarloafStyle {
        screen_position: (
            (20. + 10.) * scale_factor,
            (20. + font_size) * scale_factor,
        ),
        text_scale: font_size * scale_factor,
        bounds: (width * scale_factor, height * scale_factor),
    };

    event_loop.run_return(move |event, _, control_flow| {
        control_flow.set_wait();

        match event {
            Event::Resumed => {
                sugarloaf.init(wgpu::Color::RED, style);
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
                        .render(wgpu::Color::BLACK);
                }
                _ => (),
            },
            Event::RedrawRequested { .. } => {
                let sugar = vec![
                    Sugar {
                        content: 'S',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: [0.0, 0.0, 0.0, 1.0],
                    },
                    Sugar {
                        content: 'u',
                        foreground_color: [0.0, 0.0, 0.0, 1.0],
                        background_color: [1.0, 1.0, 1.0, 1.0],
                    },
                    Sugar {
                        content: 'g',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: [0.0, 0.0, 0.0, 1.0],
                    },
                    Sugar {
                        content: 'a',
                        foreground_color: [0.0, 0.0, 0.0, 1.0],
                        background_color: [1.0, 1.0, 1.0, 1.0],
                    },
                    Sugar {
                        content: 'r',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: [0.0, 0.0, 0.0, 1.0],
                    },
                    Sugar {
                        content: 'g',
                        foreground_color: [0.0, 0.0, 0.0, 1.0],
                        background_color: [0.0, 0.0, 1.0, 1.0],
                    },
                    Sugar {
                        content: '|',
                        foreground_color: [0.0, 0.0, 0.0, 1.0],
                        background_color: [1.0, 1.0, 1.0, 1.0],
                    },
                ];

                let loaf = vec![
                    Sugar {
                        content: 'l',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: [0.0, 0.0, 0.0, 1.0],
                    },
                    Sugar {
                        content: 'o',
                        foreground_color: [0.0, 0.0, 0.0, 1.0],
                        background_color: [1.0, 1.0, 1.0, 1.0],
                    },
                    Sugar {
                        content: 'a',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: [0.0, 0.0, 0.0, 1.0],
                    },
                    Sugar {
                        content: 'f',
                        foreground_color: [0.0, 0.0, 0.0, 1.0],
                        background_color: [0.0, 0.0, 1.0, 1.0],
                    },
                    Sugar {
                        content: 'g',
                        foreground_color: [0.0, 0.0, 0.0, 1.0],
                        background_color: [0.0, 0.0, 1.0, 1.0],
                    },
                    Sugar {
                        content: '|',
                        foreground_color: [0.0, 0.0, 0.0, 1.0],
                        background_color: [1.0, 1.0, 1.0, 1.0],
                    },
                ];

                let rio = vec![
                    Sugar {
                        content: ' ',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: [0.0, 0.0, 0.0, 1.0],
                    },
                    Sugar {
                        content: 'r',
                        foreground_color: [0.0, 0.0, 0.0, 1.0],
                        background_color: [0.0, 0.0, 1.0, 1.0],
                    },
                    Sugar {
                        content: 'i',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: [0.0, 0.0, 0.0, 1.0],
                    },
                    Sugar {
                        content: 'o',
                        foreground_color: [0.0, 0.0, 0.0, 1.0],
                        background_color: [1.0, 1.0, 1.0, 1.0],
                    },
                    Sugar {
                        content: 'g',
                        foreground_color: [0.0, 0.0, 0.0, 1.0],
                        background_color: [0.0, 1.0, 0.0, 1.0],
                    },
                ];

                sugarloaf.stack(sugar, style);
                sugarloaf.stack(loaf, style);
                sugarloaf.stack(rio, style);
                sugarloaf.render(wgpu::Color::RED);
            }
            _ => {
                *control_flow = winit::event_loop::ControlFlow::Wait;
            }
        }
    });
}
