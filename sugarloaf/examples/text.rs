extern crate tokio;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use sugarloaf::{
    layout::SugarloafLayout, Sugar, SugarDecoration, SugarStyle, Sugarloaf,
    SugarloafWindow, SugarloafWindowSize,
};
use winit::event_loop::ControlFlow;
use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::EventLoop,
    window::WindowAttributes,
};

#[tokio::main]
async fn main() {
    let mut event_loop = EventLoop::new().unwrap();
    let width = 400.0;
    let height = 300.0;

    let window_attribute = WindowAttributes::default()
        .with_title("Text example")
        .with_inner_size(LogicalSize::new(width, height))
        .with_resizable(true);
    #[allow(deprecated)]
    let window = event_loop.create_window(window_attribute).unwrap();

    let scale_factor = window.scale_factor();
    let font_size = 40.;

    let sugarloaf_layout = SugarloafLayout::new(
        width as f32,
        height as f32,
        (10.0, 10.0, 0.0),
        scale_factor as f32,
        font_size,
        1.0,
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

    let mut sugarloaf = Sugarloaf::new(
        sugarloaf_window,
        sugarloaf::SugarloafRenderer::default(),
        &sugarloaf::font::FontLibrary::default(),
        sugarloaf_layout,
    )
    .await
    .expect("Sugarloaf instance should be created");

    #[allow(deprecated)]
    let _ = event_loop.run_on_demand(move |event, event_loop_window_target| {
        event_loop_window_target.set_control_flow(ControlFlow::Wait);

        match event {
            Event::Resumed => {
                sugarloaf.set_background_color(wgpu::Color::RED);
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
                    window.request_redraw();
                }
                winit::event::WindowEvent::Resized(new_size) => {
                    sugarloaf.resize(new_size.width, new_size.height);
                    window.request_redraw();
                }
                winit::event::WindowEvent::RedrawRequested { .. } => {
                    sugarloaf.start_line();
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: 'S',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([0.0, 0.0, 0.0, 1.0]),
                        style: SugarStyle::Bold,
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: 'u',
                        foreground_color: [0.0, 0.0, 0.0, 1.0],
                        background_color: Some([1.0, 1.0, 1.0, 1.0]),
                        style: SugarStyle::Bold,
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: 'g',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([0.0, 0.0, 0.0, 1.0]),
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: 'a',
                        foreground_color: [0.0, 0.0, 0.0, 1.0],
                        background_color: Some([1.0, 1.0, 1.0, 1.0]),
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: 'r',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([0.0, 0.0, 0.0, 1.0]),
                        ..Sugar::default()
                    });

                    sugarloaf.insert_on_current_line(&Sugar {
                        content: 'l',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([0.0, 0.0, 0.0, 1.0]),
                        decoration: SugarDecoration::Strikethrough,
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: 'o',
                        foreground_color: [0.0, 0.0, 0.0, 1.0],
                        background_color: Some([1.0, 1.0, 1.0, 1.0]),
                        decoration: SugarDecoration::Strikethrough,
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: 'a',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([0.0, 0.0, 0.0, 1.0]),
                        decoration: SugarDecoration::Strikethrough,
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: 'f',
                        foreground_color: [0.0, 0.0, 0.0, 1.0],
                        background_color: Some([0.0, 0.0, 1.0, 1.0]),
                        decoration: SugarDecoration::Strikethrough,
                        ..Sugar::default()
                    });
                    sugarloaf.finish_line();

                    sugarloaf.start_line();
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: 'R',
                        foreground_color: [0.0, 0.0, 0.0, 1.0],
                        background_color: Some([0.0, 0.0, 1.0, 1.0]),
                        decoration: SugarDecoration::Underline,
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: 'i',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([0.0, 0.0, 0.0, 1.0]),
                        decoration: SugarDecoration::Underline,
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: 'o',
                        foreground_color: [0.0, 0.0, 0.0, 1.0],
                        background_color: Some([1.0, 1.0, 1.0, 1.0]),
                        decoration: SugarDecoration::Underline,
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: ' ',
                        foreground_color: [0.0, 0.0, 0.0, 1.0],
                        background_color: Some([0.0, 0.0, 1.0, 1.0]),
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: '¼',
                        foreground_color: [0.0, 0.0, 0.0, 1.0],
                        background_color: Some([1.0, 1.0, 0.0, 1.0]),
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: '㏑',
                        foreground_color: [0.0, 0.0, 0.0, 1.0],
                        background_color: Some([0.0, 1.0, 0.0, 1.0]),
                        ..Sugar::default()
                    });
                    sugarloaf.finish_line();

                    sugarloaf.start_line();
                    // Font Symbol (char width 2)
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: '',
                        foreground_color: [0.0, 0.0, 0.0, 1.0],
                        background_color: Some([1.0, 1.0, 1.0, 1.0]),
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: '!',
                        foreground_color: [0.0, 0.0, 0.0, 1.0],
                        background_color: Some([1.0, 1.0, 1.0, 1.0]),
                        ..Sugar::default()
                    });
                    // Font Symbol (apple symbols font)
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: '⫹',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([0.0, 0.5, 0.0, 1.0]),
                        ..Sugar::default()
                    });
                    // Font Emojis
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: '🥶',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([0.0, 0.0, 0.0, 1.0]),
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: '🐶',
                        foreground_color: [0.0, 0.0, 0.0, 1.0],
                        background_color: Some([0.0, 0.0, 1.0, 1.0]),
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: '!',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([0.0, 0.0, 0.0, 1.0]),
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: '=',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([0.0, 0.0, 0.0, 1.0]),
                        ..Sugar::default()
                    });
                    sugarloaf.finish_line();

                    sugarloaf.start_line();
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: 'o',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([0.0, 0.0, 0.0, 1.0]),
                        decoration: SugarDecoration::DashedUnderline,
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: 'l',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([0.0, 0.0, 0.0, 1.0]),
                        decoration: SugarDecoration::DashedUnderline,
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: 'á',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([0.0, 0.0, 0.0, 1.0]),
                        decoration: SugarDecoration::DashedUnderline,
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: ' ',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([1.0, 0.0, 1.0, 1.0]),
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: '-',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([1.0, 0.0, 1.0, 1.0]),
                        decoration: SugarDecoration::DoubleUnderline,
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: '>',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([1.0, 0.0, 1.0, 1.0]),
                        decoration: SugarDecoration::DoubleUnderline,
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: ' ',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([1.0, 0.0, 1.0, 1.0]),
                        decoration: SugarDecoration::DoubleUnderline,
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: 'i',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([1.0, 0.0, 1.0, 1.0]),
                        decoration: SugarDecoration::DoubleUnderline,
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: ' ',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([0.0, 0.0, 0.0, 1.0]),
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: 'R',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([0.0, 0.5, 0.0, 1.0]),
                        decoration: SugarDecoration::DottedUnderline,
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: 'I',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([0.0, 0.5, 0.0, 1.0]),
                        decoration: SugarDecoration::DottedUnderline,
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: 'O',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([0.0, 0.5, 0.0, 1.0]),
                        decoration: SugarDecoration::DottedUnderline,
                        ..Sugar::default()
                    });
                    sugarloaf.finish_line();

                    sugarloaf.start_line();
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: 'T',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([0.0, 0.5, 0.5, 1.0]),
                        decoration: SugarDecoration::CurlyUnderline,
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: 'e',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([0.0, 0.5, 0.5, 1.0]),
                        decoration: SugarDecoration::CurlyUnderline,
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: 'r',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([0.0, 0.5, 0.5, 1.0]),
                        decoration: SugarDecoration::CurlyUnderline,
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: 'm',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([0.0, 0.5, 0.5, 1.0]),
                        decoration: SugarDecoration::CurlyUnderline,
                        ..Sugar::default()
                    });
                    sugarloaf.insert_on_current_line(&Sugar {
                        content: '!',
                        foreground_color: [1.0, 1.0, 1.0, 1.0],
                        background_color: Some([0.0, 0.5, 0.5, 1.0]),
                        decoration: SugarDecoration::CurlyUnderline,
                        ..Sugar::default()
                    });
                    sugarloaf.finish_line();

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
