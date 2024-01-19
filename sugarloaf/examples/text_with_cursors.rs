extern crate tokio;

use sugarloaf::core::SugarStyle;
use sugarloaf::{
    core::{Sugar, SugarDecoration},
    layout::SugarloafLayout,
};

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use sugarloaf::{Sugarloaf, SugarloafWindow, SugarloafWindowSize};
use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

#[tokio::main]
async fn main() {
    let event_loop = EventLoop::new().unwrap();
    let width = 1200.0;
    let height = 800.0;

    let window = WindowBuilder::new()
        .with_title("Text with cursors example")
        .with_inner_size(LogicalSize::new(width, height))
        .with_resizable(true)
        .build(&event_loop)
        .unwrap();

    let scale_factor = window.scale_factor();
    let font_size = 90.;

    let sugarloaf_layout = SugarloafLayout::new(
        width as f32,
        height as f32,
        (10.0, 10.0, 0.0),
        scale_factor as f32,
        font_size,
        1.0,
        (2, 1),
    );

    let size = window.inner_size();
    let sugarloaf_window = SugarloafWindow {
        handle: window.window_handle().unwrap().into(),
        display: window.display_handle().unwrap().into(),
        scale: scale_factor as f32,
        size: SugarloafWindowSize {
            width: size.width,
            height: size.height,
        },
    };

    let mut sugarloaf = Sugarloaf::new(
        sugarloaf_window,
        sugarloaf::SugarloafRenderer::default(),
        sugarloaf::font::fonts::SugarloafFonts::default(),
        sugarloaf_layout,
        None,
    )
    .await
    .expect("Sugarloaf instance should be created");

    sugarloaf.calculate_bounds();

    let _ = event_loop.run(move |event, event_loop_window_target| {
        event_loop_window_target.set_control_flow(ControlFlow::Wait);

        let sugar = vec![
            Sugar {
                content: 'u',
                fg_color: [1.0, 1.0, 1.0, 1.0],
                bg_color: [0.0, 0.0, 0.0, 1.0],
                style: SugarStyle::default(),
                media: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 85.),
                    size: (1.0, 0.050),
                    color: [0.0, 0.0, 0.0, 1.0],
                }),
            },
            Sugar {
                content: 'n',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [1.0, 1.0, 1.0, 1.0],
                style: SugarStyle::default(),
                media: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 85.),
                    size: (1.0, 0.025),
                    color: [0.0, 0.0, 0.0, 1.0],
                }),
            },
            Sugar {
                content: 'd',
                fg_color: [1.0, 1.0, 1.0, 1.0],
                bg_color: [0.0, 0.0, 0.0, 1.0],
                style: SugarStyle::default(),
                media: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 86.),
                    size: (1.0, 0.025),
                    color: [0.0, 0.0, 0.0, 1.0],
                }),
            },
            Sugar {
                content: 'e',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [1.0, 1.0, 1.0, 1.0],
                style: SugarStyle::default(),
                media: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 86.),
                    size: (1.0, 0.025),
                    color: [0.0, 0.0, 0.0, 1.0],
                }),
            },
            Sugar {
                content: 'r',
                fg_color: [1.0, 1.0, 1.0, 1.0],
                bg_color: [0.0, 0.0, 0.0, 1.0],
                style: SugarStyle::default(),
                media: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 86.),
                    size: (1.0, 0.025),
                    color: [0.0, 0.0, 0.0, 1.0],
                }),
            },
            Sugar {
                content: 'l',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [0.0, 0.0, 1.0, 1.0],
                style: SugarStyle::default(),
                media: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 86.),
                    size: (1.0, 0.025),
                    color: [0.0, 0.0, 0.0, 1.0],
                }),
            },
            Sugar {
                content: '!',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [0.0, 0.0, 1.0, 1.0],
                style: SugarStyle::default(),
                decoration: None,
                media: None,
            },
            Sugar {
                content: 'i',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [1.0, 1.0, 1.0, 1.0],
                style: SugarStyle::default(),
                media: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 86.),
                    size: (1.0, 0.025),
                    color: [0.0, 0.0, 0.0, 1.0],
                }),
            },
            Sugar {
                content: 'n',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [1.0, 1.0, 1.0, 1.0],
                style: SugarStyle::default(),
                media: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 86.),
                    size: (1.0, 0.025),
                    color: [0.0, 0.0, 0.0, 1.0],
                }),
            },
            Sugar {
                content: 'e',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [1.0, 1.0, 1.0, 1.0],
                style: SugarStyle::default(),
                media: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 86.),
                    size: (1.0, 0.025),
                    color: [0.0, 0.0, 0.0, 1.0],
                }),
            },
            Sugar {
                content: ' ',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [1.0, 1.0, 1.0, 1.0],
                style: SugarStyle::default(),
                media: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 86.),
                    size: (1.0, 0.025),
                    color: [0.0, 0.0, 0.0, 1.0],
                }),
            },
            Sugar {
                content: ' ',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [1.0, 1.0, 1.0, 1.0],
                style: SugarStyle::default(),
                media: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 86.),
                    size: (1.0, 0.025),
                    color: [0.0, 0.0, 0.0, 1.0],
                }),
            },
            Sugar {
                content: ' ',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [1.0, 1.0, 1.0, 1.0],
                style: SugarStyle::default(),
                decoration: None,
                media: None,
            },
        ];

        let italic_and_bold = vec![
            Sugar {
                content: 'i',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [1.0, 1.0, 1.0, 1.0],
                style: SugarStyle {
                    italic: true,
                    bold: false,
                },
                decoration: None,
                media: None,
            },
            Sugar {
                content: 't',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [1.0, 1.0, 1.0, 1.0],
                style: SugarStyle {
                    italic: true,
                    bold: false,
                },
                decoration: None,
                media: None,
            },
            Sugar {
                content: 'a',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [1.0, 1.0, 1.0, 1.0],
                style: SugarStyle {
                    italic: true,
                    bold: false,
                },
                decoration: None,
                media: None,
            },
            Sugar {
                content: 'l',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [1.0, 1.0, 1.0, 1.0],
                style: SugarStyle {
                    italic: true,
                    bold: false,
                },
                decoration: None,
                media: None,
            },
            Sugar {
                content: 'i',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [1.0, 1.0, 1.0, 1.0],
                style: SugarStyle {
                    italic: true,
                    bold: false,
                },
                decoration: None,
                media: None,
            },
            Sugar {
                content: 'c',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [1.0, 1.0, 1.0, 1.0],
                style: SugarStyle {
                    italic: true,
                    bold: false,
                },
                decoration: None,
                media: None,
            },
            Sugar {
                content: ' ',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [0.5, 0.5, 1.0, 1.0],
                style: SugarStyle {
                    italic: false,
                    bold: true,
                },
                decoration: None,
                media: None,
            },
            Sugar {
                content: 'b',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [1.0, 1.0, 0.3, 1.0],
                style: SugarStyle {
                    italic: false,
                    bold: true,
                },
                decoration: None,
                media: None,
            },
            Sugar {
                content: 'o',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [1.0, 1.0, 0.3, 1.0],
                style: SugarStyle {
                    italic: false,
                    bold: true,
                },
                decoration: None,
                media: None,
            },
            Sugar {
                content: 'l',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [1.0, 1.0, 0.3, 1.0],
                style: SugarStyle {
                    italic: false,
                    bold: true,
                },
                decoration: None,
                media: None,
            },
            Sugar {
                content: 'd',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [1.0, 1.0, 0.3, 1.0],
                style: SugarStyle {
                    italic: false,
                    bold: true,
                },
                decoration: None,
                media: None,
            },
        ];

        let rio = vec![
            Sugar {
                content: 'r',
                fg_color: [1.0, 1.0, 1.0, 1.0],
                bg_color: [0.0, 0.0, 0.0, 1.0],
                style: SugarStyle::default(),
                media: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 0.92),
                    size: (1.0, 0.05),
                    color: [0.0, 0.0, 0.0, 1.0],
                }),
            },
            Sugar {
                content: 'e',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [0.0, 0.0, 1.0, 1.0],
                style: SugarStyle::default(),
                decoration: None,
                media: None,
            },
            Sugar {
                content: 'g',
                fg_color: [1.0, 1.0, 1.0, 1.0],
                bg_color: [0.0, 0.0, 0.0, 1.0],
                style: SugarStyle::default(),
                decoration: None,
                media: None,
            },
            Sugar {
                content: 'u',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [1.0, 1.0, 1.0, 1.0],
                style: SugarStyle::default(),
                decoration: None,
                media: None,
            },
            Sugar {
                content: 'l',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [0.0, 1.0, 0.0, 1.0],
                style: SugarStyle::default(),
                decoration: None,
                media: None,
            },
            Sugar {
                content: 'a',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [1.0, 1.0, 0.0, 1.0],
                style: SugarStyle::default(),
                decoration: None,
                media: None,
            },
            Sugar {
                content: 'r',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [0.0, 1.0, 0.0, 1.0],
                style: SugarStyle::default(),
                decoration: None,
                media: None,
            },
        ];

        let strike = vec![
            Sugar {
                content: 's',
                fg_color: [1.0, 1.0, 1.0, 1.0],
                bg_color: [0.0, 0.0, 0.0, 1.0],
                style: SugarStyle::default(),
                media: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 0.5),
                    size: (1.0, 0.025),
                    color: [0.5, 0.5, 0.0, 1.0],
                }),
            },
            Sugar {
                content: 't',
                fg_color: [1.0, 1.0, 1.0, 1.0],
                bg_color: [0.0, 0.0, 0.0, 1.0],
                style: SugarStyle::default(),
                media: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 0.5),
                    size: (1.0, 0.025),
                    color: [0.5, 0.5, 0.0, 1.0],
                }),
            },
            Sugar {
                content: 'r',
                fg_color: [1.0, 1.0, 1.0, 1.0],
                bg_color: [0.0, 0.0, 0.0, 1.0],
                style: SugarStyle::default(),
                media: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 0.5),
                    size: (1.0, 0.025),
                    color: [0.5, 0.5, 0.0, 1.0],
                }),
            },
            Sugar {
                content: 'i',
                fg_color: [1.0, 1.0, 1.0, 1.0],
                bg_color: [0.0, 0.0, 0.0, 1.0],
                style: SugarStyle::default(),
                media: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 85.),
                    size: (1.0, 0.025),
                    color: [0.5, 0.5, 0.0, 1.0],
                }),
            },
            Sugar {
                content: 'k',
                fg_color: [1.0, 1.0, 1.0, 1.0],
                bg_color: [0.0, 0.0, 0.0, 1.0],
                style: SugarStyle::default(),
                media: None,
                decoration: Some(SugarDecoration {
                    relative_position: (0.0, 0.5),
                    size: (1.0, 0.025),
                    color: [0.5, 0.5, 0.0, 1.0],
                }),
            },
            Sugar {
                content: 'e',
                fg_color: [1.0, 1.0, 1.0, 1.0],
                bg_color: [0.0, 0.0, 0.0, 1.0],
                style: SugarStyle::default(),
                media: None,
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
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [0.0, 1.0, 1.0, 1.0],
                style: SugarStyle::default(),
                media: None,
                decoration: block,
            },
            Sugar {
                content: ' ',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [0.0, 1.0, 1.0, 1.0],
                style: SugarStyle::default(),
                decoration: None,
                media: None,
            },
            Sugar {
                content: ' ',
                fg_color: [1.0, 1.0, 1.0, 1.0],
                bg_color: [0.0, 0.0, 0.0, 1.0],
                style: SugarStyle::default(),
                media: None,
                decoration: underline,
            },
            Sugar {
                content: ' ',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [0.0, 1.0, 1.0, 1.0],
                style: SugarStyle::default(),
                decoration: None,
                media: None,
            },
            Sugar {
                content: ' ',
                fg_color: [0.0, 0.0, 0.0, 1.0],
                bg_color: [0.0, 1.0, 1.0, 1.0],
                style: SugarStyle::default(),
                media: None,
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
                WindowEvent::CloseRequested => event_loop_window_target.exit(),
                WindowEvent::ScaleFactorChanged {
                    // mut inner_size_writer,
                    scale_factor,
                    ..
                } => {
                    let scale_factor_f32 = scale_factor as f32;
                    let new_inner_size = window.inner_size();
                    sugarloaf.rescale(scale_factor_f32);
                    sugarloaf.resize(new_inner_size.width, new_inner_size.height);
                    sugarloaf.calculate_bounds();
                    window.request_redraw();
                }
                winit::event::WindowEvent::Resized(new_size) => {
                    sugarloaf.resize(new_size.width, new_size.height);
                    sugarloaf.calculate_bounds();
                    window.request_redraw();
                }
                winit::event::WindowEvent::RedrawRequested { .. } => {
                    sugarloaf.stack(sugar);
                    sugarloaf.stack(italic_and_bold);
                    sugarloaf.stack(rio);
                    sugarloaf.stack(strike);
                    sugarloaf.stack(cursors);
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
