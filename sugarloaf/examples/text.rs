extern crate tokio;

use winit::platform::run_return::EventLoopExtRunReturn;
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::EventLoop,
    window::WindowBuilder,
};

use sugarloaf::core::{Sugar, SugarloafStyle};
use sugarloaf::Sugarloaf;

fn compute_styles(
    scale_factor: f32,
    font_size: f32,
    width: f32,
    height: f32,
) -> SugarloafStyle {
    SugarloafStyle {
        screen_position: ((20. + 10.) * scale_factor, (20. + font_size) * scale_factor),
        text_scale: font_size * scale_factor,
        bounds: (width * scale_factor, height * scale_factor),
    }
}

#[tokio::main]
async fn main() {
    let mut event_loop = EventLoop::new();
    let width = 1200.0;
    let height = 800.0;

    let window = WindowBuilder::new()
        .with_title("Text example")
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

    let scale_factor = sugarloaf.get_scale();
    let font_size = 180.;
    let mut styles = compute_styles(scale_factor, font_size, width, height);
    println!("{:?}", styles);

    event_loop.run_return(move |event, _, control_flow| {
        control_flow.set_wait();

        let sugar = vec![
            Sugar {
                content: 'S',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
            },
            Sugar {
                content: 'u',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: None,
            },
            Sugar {
                content: 'g',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
            },
            Sugar {
                content: 'a',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: None,
            },
            Sugar {
                content: 'r',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
            },
            Sugar {
                content: 'g',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 0.0, 1.0, 1.0],
                style: None,
            },
            Sugar {
                content: '|',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: None,
            },
        ];

        let loaf = vec![
            Sugar {
                content: 'l',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
            },
            Sugar {
                content: 'o',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: None,
            },
            Sugar {
                content: 'a',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
            },
            Sugar {
                content: 'f',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 0.0, 1.0, 1.0],
                style: None,
            },
            Sugar {
                content: 'g',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 0.0, 1.0, 1.0],
                style: None,
            },
            Sugar {
                content: '|',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: None,
            },
        ];

        let rio = vec![
            Sugar {
                content: ' ',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
            },
            Sugar {
                content: 'r',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 0.0, 1.0, 1.0],
                style: None,
            },
            Sugar {
                content: 'i',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
            },
            Sugar {
                content: 'o',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: None,
            },
            Sugar {
                content: 'g',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 1.0, 0.0, 1.0],
                style: None,
            },
            Sugar {
                content: '¼',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 0.0, 1.0],
                style: None,
            },
            Sugar {
                content: '¬',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 1.0, 0.0, 1.0],
                style: None,
            },
        ];

        let special = vec![
            // Font Unicode (unicode font)
            Sugar {
                content: '㏑',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 1.0, 1.0, 1.0],
                style: None,
            },
            // Font Symbol (apple symbols font)
            Sugar {
                content: '⫹',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
            },
            // Font Regular (firamono)
            Sugar {
                content: 'λ',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 1.0, 1.0, 1.0],
                style: None,
            },
            // Font Emojis
            Sugar {
                content: '🥇',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
            },
            Sugar {
                content: '👷',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 0.0, 1.0, 1.0],
                style: None,
            },
        ];

        match event {
            Event::Resumed => {
                sugarloaf.init(wgpu::Color::RED, styles);
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
                    let scale_factor_f32 = scale_factor as f32;
                    styles = compute_styles(
                        scale_factor_f32,
                        font_size,
                        new_inner_size.width as f32,
                        new_inner_size.height as f32,
                    );
                    sugarloaf
                        .rescale(scale_factor_f32)
                        .resize(new_inner_size.width, new_inner_size.height);
                    window.request_redraw();
                }
                _ => (),
            },
            Event::RedrawRequested { .. } => {
                sugarloaf.stack(sugar, styles);
                sugarloaf.stack(loaf, styles);
                sugarloaf.stack(rio, styles);
                sugarloaf.stack(special, styles);
                sugarloaf.render();
            }
            _ => {
                *control_flow = winit::event_loop::ControlFlow::Wait;
            }
        }
    });
}
