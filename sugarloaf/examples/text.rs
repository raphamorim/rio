extern crate tokio;

use sugarloaf::{core::Sugar, layout::SugarloafLayout, Sugarloaf};
use winit::platform::run_return::EventLoopExtRunReturn;
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::EventLoop,
    window::WindowBuilder,
};

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

    let scale_factor = window.scale_factor();
    let font_size = 180.;

    let sugarloaf_layout = SugarloafLayout::new(
        width as f32,
        height as f32,
        (10.0, 10.0),
        scale_factor as f32,
        font_size,
        (2, 1),
    );

    let mut sugarloaf = Sugarloaf::new(
        &window,
        wgpu::PowerPreference::HighPerformance,
        sugarloaf::font::constants::DEFAULT_FONT_NAME.to_string(),
        sugarloaf_layout,
    )
    .await
    .expect("Sugarloaf instance should be created");

    event_loop.run_return(move |event, _, control_flow| {
        control_flow.set_wait();

        let sugar = vec![
            Sugar {
                content: 'S',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
                decoration: None,
            },
            Sugar {
                content: 'u',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: None,
                decoration: None,
            },
            Sugar {
                content: 'g',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
                decoration: None,
            },
            Sugar {
                content: 'a',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: None,
                decoration: None,
            },
            Sugar {
                content: 'r',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
                decoration: None,
            },
            Sugar {
                content: 'g',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 0.0, 1.0, 1.0],
                style: None,
                decoration: None,
            },
            Sugar {
                content: '|',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: None,
                decoration: None,
            },
        ];

        let loaf = vec![
            Sugar {
                content: 'l',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
                decoration: None,
            },
            Sugar {
                content: 'o',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: None,
                decoration: None,
            },
            Sugar {
                content: 'a',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
                decoration: None,
            },
            Sugar {
                content: 'f',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 0.0, 1.0, 1.0],
                style: None,
                decoration: None,
            },
            Sugar {
                content: 'g',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 0.0, 1.0, 1.0],
                style: None,
                decoration: None,
            },
            Sugar {
                content: '|',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: None,
                decoration: None,
            },
        ];

        let rio = vec![
            Sugar {
                content: ' ',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
                decoration: None,
            },
            Sugar {
                content: 'r',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 0.0, 1.0, 1.0],
                style: None,
                decoration: None,
            },
            Sugar {
                content: 'i',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
                decoration: None,
            },
            Sugar {
                content: 'o',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: None,
                decoration: None,
            },
            Sugar {
                content: 'g',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 1.0, 0.0, 1.0],
                style: None,
                decoration: None,
            },
            Sugar {
                content: '¼',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 0.0, 1.0],
                style: None,
                decoration: None,
            },
            Sugar {
                content: '¬',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 1.0, 0.0, 1.0],
                style: None,
                decoration: None,
            },
        ];

        let special = vec![
            // Font Unicode (unicode font)
            Sugar {
                content: '㏑',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 1.0, 1.0, 1.0],
                style: None,
                decoration: None,
            },
            // Font Symbol (apple symbols font)
            Sugar {
                content: '⫹',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
                decoration: None,
            },
            // Font Regular (firamono)
            Sugar {
                content: 'λ',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 1.0, 1.0, 1.0],
                style: None,
                decoration: None,
            },
            // Font Emojis
            Sugar {
                content: '🥇',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
                decoration: None,
            },
            Sugar {
                content: '👷',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 0.0, 1.0, 1.0],
                style: None,
                decoration: None,
            },
        ];

        match event {
            Event::Resumed => {
                sugarloaf.config(wgpu::Color::RED);
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
                    sugarloaf
                        .rescale(scale_factor_f32)
                        .resize(new_inner_size.width, new_inner_size.height);
                    window.request_redraw();
                }
                _ => (),
            },
            Event::RedrawRequested { .. } => {
                sugarloaf.stack(sugar);
                sugarloaf.stack(loaf);
                sugarloaf.stack(rio);
                sugarloaf.stack(special);
                sugarloaf.render();
            }
            _ => {
                *control_flow = winit::event_loop::ControlFlow::Wait;
            }
        }
    });
}
