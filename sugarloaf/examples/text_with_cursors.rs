extern crate tokio;

use sugarloaf::core::SugarStyle;
use sugarloaf::{
    core::{Sugar, SugarDecoration},
    layout::SugarloafLayout,
    Sugarloaf,
};

use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

#[tokio::main]
async fn main() {
    let event_loop = EventLoop::new();
    let width = 1200.0;
    let height = 800.0;

    let window = WindowBuilder::new()
        .with_title("Text example")
        .with_inner_size(LogicalSize::new(width, height))
        .with_resizable(true)
        .build(&event_loop)
        .unwrap();

    let scale_factor = window.scale_factor();
    let font_size = 90.;

    let sugarloaf_layout = SugarloafLayout::new(
        width as f32,
        height as f32,
        (10.0, 10.0),
        scale_factor as f32,
        font_size,
        1.0,
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

    sugarloaf.calculate_bounds();

    event_loop.run(move |event, _, control_flow| {
        control_flow.set_wait();

        let sugar = vec![
            Sugar {
                content: 'u',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 85.),
                    size: (1.0, 0.050),
                    color: [0.0, 0.0, 0.0, 1.0],
                }),
            },
            Sugar {
                content: 'n',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 85.),
                    size: (1.0, 0.025),
                    color: [0.0, 0.0, 0.0, 1.0],
                }),
            },
            Sugar {
                content: 'd',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 86.),
                    size: (1.0, 0.025),
                    color: [0.0, 0.0, 0.0, 1.0],
                }),
            },
            Sugar {
                content: 'e',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 86.),
                    size: (1.0, 0.025),
                    color: [0.0, 0.0, 0.0, 1.0],
                }),
            },
            Sugar {
                content: 'r',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 86.),
                    size: (1.0, 0.025),
                    color: [0.0, 0.0, 0.0, 1.0],
                }),
            },
            Sugar {
                content: 'l',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 0.0, 1.0, 1.0],
                style: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 86.),
                    size: (1.0, 0.025),
                    color: [0.0, 0.0, 0.0, 1.0],
                }),
            },
            Sugar {
                content: '!',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 0.0, 1.0, 1.0],
                style: None,
                decoration: None,
            },
            Sugar {
                content: 'i',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 86.),
                    size: (1.0, 0.025),
                    color: [0.0, 0.0, 0.0, 1.0],
                }),
            },
            Sugar {
                content: 'n',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 86.),
                    size: (1.0, 0.025),
                    color: [0.0, 0.0, 0.0, 1.0],
                }),
            },
            Sugar {
                content: 'e',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 86.),
                    size: (1.0, 0.025),
                    color: [0.0, 0.0, 0.0, 1.0],
                }),
            },
            Sugar {
                content: ' ',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 86.),
                    size: (1.0, 0.025),
                    color: [0.0, 0.0, 0.0, 1.0],
                }),
            },
            Sugar {
                content: ' ',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 86.),
                    size: (1.0, 0.025),
                    color: [0.0, 0.0, 0.0, 1.0],
                }),
            },
            Sugar {
                content: ' ',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: None,
                decoration: None,
            },
        ];

        let italic_and_bold = vec![
            Sugar {
                content: 'i',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: Some(SugarStyle {
                    is_italic: true,
                    is_bold_italic: false,
                    is_bold: false,
                }),
                decoration: None,
            },
            Sugar {
                content: 't',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: Some(SugarStyle {
                    is_italic: true,
                    is_bold_italic: false,
                    is_bold: false,
                }),
                decoration: None,
            },
            Sugar {
                content: 'a',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: Some(SugarStyle {
                    is_italic: true,
                    is_bold_italic: false,
                    is_bold: false,
                }),
                decoration: None,
            },
            Sugar {
                content: 'l',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: Some(SugarStyle {
                    is_italic: true,
                    is_bold_italic: false,
                    is_bold: false,
                }),
                decoration: None,
            },
            Sugar {
                content: 'i',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: Some(SugarStyle {
                    is_italic: true,
                    is_bold_italic: false,
                    is_bold: false,
                }),
                decoration: None,
            },
            Sugar {
                content: 'c',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: Some(SugarStyle {
                    is_italic: true,
                    is_bold_italic: false,
                    is_bold: false,
                }),
                decoration: None,
            },
            Sugar {
                content: ' ',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.5, 0.5, 1.0, 1.0],
                style: Some(SugarStyle {
                    is_italic: false,
                    is_bold_italic: false,
                    is_bold: true,
                }),
                decoration: None,
            },
            Sugar {
                content: 'b',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 0.3, 1.0],
                style: Some(SugarStyle {
                    is_italic: false,
                    is_bold_italic: false,
                    is_bold: true,
                }),
                decoration: None,
            },
            Sugar {
                content: 'o',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 0.3, 1.0],
                style: Some(SugarStyle {
                    is_italic: false,
                    is_bold_italic: false,
                    is_bold: true,
                }),
                decoration: None,
            },
            Sugar {
                content: 'l',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 0.3, 1.0],
                style: Some(SugarStyle {
                    is_italic: false,
                    is_bold_italic: false,
                    is_bold: true,
                }),
                decoration: None,
            },
            Sugar {
                content: 'd',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 0.3, 1.0],
                style: Some(SugarStyle {
                    is_italic: false,
                    is_bold_italic: false,
                    is_bold: true,
                }),
                decoration: None,
            },
        ];

        let rio = vec![
            Sugar {
                content: 'r',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 0.92),
                    size: (1.0, 0.05),
                    color: [0.0, 0.0, 0.0, 1.0],
                }),
            },
            Sugar {
                content: 'e',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 0.0, 1.0, 1.0],
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
                content: 'u',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 1.0, 1.0],
                style: None,
                decoration: None,
            },
            Sugar {
                content: 'l',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 1.0, 0.0, 1.0],
                style: None,
                decoration: None,
            },
            Sugar {
                content: 'a',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [1.0, 1.0, 0.0, 1.0],
                style: None,
                decoration: None,
            },
            Sugar {
                content: 'r',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 1.0, 0.0, 1.0],
                style: None,
                decoration: None,
            },
        ];

        let strike = vec![
            Sugar {
                content: 's',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 0.5),
                    size: (1.0, 0.025),
                    color: [0.5, 0.5, 0.0, 1.0],
                }),
            },
            Sugar {
                content: 't',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 0.5),
                    size: (1.0, 0.025),
                    color: [0.5, 0.5, 0.0, 1.0],
                }),
            },
            Sugar {
                content: 'r',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 0.5),
                    size: (1.0, 0.025),
                    color: [0.5, 0.5, 0.0, 1.0],
                }),
            },
            Sugar {
                content: 'i',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 85.),
                    size: (1.0, 0.025),
                    color: [0.5, 0.5, 0.0, 1.0],
                }),
            },
            Sugar {
                content: 'k',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 0.5),
                    size: (1.0, 0.025),
                    color: [0.5, 0.5, 0.0, 1.0],
                }),
            },
            Sugar {
                content: 'e',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 0.85),
                    size: (1.0, 0.025),
                    color: [0.5, 0.5, 0.0, 1.0],
                }),
            },
        ];

        let block = Some(SugarDecoration {
            relative_position: (0.0, 0.0),
            size: (1.0, 1.0),
            color: [1.0, 0.4, 1.0, 1.0],
        });

        let underline = Some(SugarDecoration {
            relative_position: (0.0, 85.),
            size: (1.0, 0.05),
            color: [1.0, 0.4, 1.0, 1.0],
        });

        let beam = Some(SugarDecoration {
            relative_position: (0.0, 0.0),
            size: (0.1, 1.0),
            color: [1.0, 0.4, 1.0, 1.0],
        });

        let cursors = vec![
            Sugar {
                content: ' ',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 1.0, 1.0, 1.0],
                style: None,
                decoration: block,
            },
            Sugar {
                content: ' ',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 1.0, 1.0, 1.0],
                style: None,
                decoration: None,
            },
            Sugar {
                content: ' ',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: [0.0, 0.0, 0.0, 1.0],
                style: None,
                decoration: underline,
            },
            Sugar {
                content: ' ',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 1.0, 1.0, 1.0],
                style: None,
                decoration: None,
            },
            Sugar {
                content: ' ',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: [0.0, 1.0, 1.0, 1.0],
                style: None,
                decoration: beam,
            },
        ];

        match event {
            Event::Resumed => {
                sugarloaf
                    .set_background_color(wgpu::Color::RED)
                    .calculate_bounds();
                window.request_redraw();
            }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => control_flow.set_exit(),
                WindowEvent::ScaleFactorChanged {
                    new_inner_size,
                    scale_factor,
                    ..
                } => {
                    log::info!("changed scale_factor: {scale_factor:?}");

                    let scale_factor_f32 = scale_factor as f32;
                    sugarloaf
                        .rescale(scale_factor_f32)
                        .resize(new_inner_size.width, new_inner_size.height)
                        .calculate_bounds();
                    window.request_redraw();
                }
                _ => (),
            },
            Event::RedrawRequested { .. } => {
                sugarloaf.stack(sugar);
                sugarloaf.stack(italic_and_bold);
                sugarloaf.stack(rio);
                sugarloaf.stack(strike);
                sugarloaf.stack(cursors);
                sugarloaf.render();
            }
            _ => {
                *control_flow = ControlFlow::Wait;
            }
        }
    });
}
