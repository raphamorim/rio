extern crate tokio;

// use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};
use sugarloaf::{
    layout::SugarloafLayout, Sugar, SugarCursor, SugarDecoration, SugarStyle,
};
use sugarloaf::{Sugarloaf, SugarloafWindow, SugarloafWindowSize};
use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowAttributes,
};

#[tokio::main]
async fn main() {
    let event_loop = EventLoop::new().unwrap();
    let width = 1200.0;
    let height = 800.0;

    let window_attribute = WindowAttributes::default()
        .with_title("Text with cursors example")
        .with_inner_size(LogicalSize::new(width, height))
        .with_resizable(true);
    #[allow(deprecated)]
    let window = event_loop.create_window(window_attribute).unwrap();

    let scale_factor = window.scale_factor();
    let font_size = 90.;

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
        // handle: window.window_handle().unwrap().into(),
        // display: window.display_handle().unwrap().into(),
        handle: window.raw_window_handle(),
        display: window.raw_display_handle(),
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
    let _ = event_loop.run(move |event, event_loop_window_target| {
        event_loop_window_target.set_control_flow(ControlFlow::Wait);

        let sugar = vec![
            Sugar {
                content: 'u',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: Some([0.0, 0.0, 0.0, 1.0]),
                decoration: SugarDecoration::Underline,
                ..Sugar::default()
            },
            Sugar {
                content: 'n',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([1.0, 1.0, 1.0, 1.0]),
                decoration: SugarDecoration::Underline,
                ..Sugar::default()
            },
            Sugar {
                content: 'd',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: Some([0.0, 0.0, 0.0, 1.0]),
                decoration: SugarDecoration::Underline,
                ..Sugar::default()
            },
            Sugar {
                content: 'e',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([1.0, 1.0, 1.0, 1.0]),
                decoration: SugarDecoration::Underline,
                ..Sugar::default()
            },
            Sugar {
                content: 'r',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: Some([0.0, 0.0, 0.0, 1.0]),
                decoration: SugarDecoration::Underline,
                ..Sugar::default()
            },
            Sugar {
                content: 'l',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([0.0, 0.0, 1.0, 1.0]),
                decoration: SugarDecoration::Underline,
                ..Sugar::default()
            },
            Sugar {
                content: '!',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([0.0, 0.0, 1.0, 1.0]),
                ..Sugar::default()
            },
            Sugar {
                content: 'i',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([1.0, 1.0, 1.0, 1.0]),
                decoration: SugarDecoration::Underline,
                ..Sugar::default()
            },
            Sugar {
                content: 'n',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([1.0, 1.0, 1.0, 1.0]),
                decoration: SugarDecoration::Underline,
                ..Sugar::default()
            },
            Sugar {
                content: 'e',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([1.0, 1.0, 1.0, 1.0]),
                decoration: SugarDecoration::Underline,
                ..Sugar::default()
            },
            Sugar {
                content: ' ',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([1.0, 1.0, 1.0, 1.0]),
                decoration: SugarDecoration::Underline,
                ..Sugar::default()
            },
            Sugar {
                content: ' ',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([1.0, 1.0, 1.0, 1.0]),
                decoration: SugarDecoration::Underline,
                ..Sugar::default()
            },
            Sugar {
                content: ' ',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([1.0, 1.0, 1.0, 1.0]),
                ..Sugar::default()
            },
        ];

        let italic_and_bold = vec![
            Sugar {
                content: 'i',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([1.0, 1.0, 1.0, 1.0]),
                style: SugarStyle::Italic,
                ..Sugar::default()
            },
            Sugar {
                content: 't',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([1.0, 1.0, 1.0, 1.0]),
                style: SugarStyle::Italic,
                ..Sugar::default()
            },
            Sugar {
                content: 'a',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([1.0, 1.0, 1.0, 1.0]),
                style: SugarStyle::Italic,
                ..Sugar::default()
            },
            Sugar {
                content: 'l',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([1.0, 1.0, 1.0, 1.0]),
                style: SugarStyle::Italic,
                ..Sugar::default()
            },
            Sugar {
                content: 'i',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([1.0, 1.0, 1.0, 1.0]),
                style: SugarStyle::Italic,
                ..Sugar::default()
            },
            Sugar {
                content: 'c',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([1.0, 1.0, 1.0, 1.0]),
                style: SugarStyle::Italic,
                ..Sugar::default()
            },
            Sugar {
                content: ' ',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([0.5, 0.5, 1.0, 1.0]),
                style: SugarStyle::Bold,
                ..Sugar::default()
            },
            Sugar {
                content: 'b',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([1.0, 1.0, 0.3, 1.0]),
                style: SugarStyle::Bold,
                ..Sugar::default()
            },
            Sugar {
                content: 'o',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([1.0, 1.0, 0.3, 1.0]),
                style: SugarStyle::Bold,
                ..Sugar::default()
            },
            Sugar {
                content: 'l',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([1.0, 1.0, 0.3, 1.0]),
                style: SugarStyle::Bold,
                ..Sugar::default()
            },
            Sugar {
                content: 'd',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([1.0, 1.0, 0.3, 1.0]),
                style: SugarStyle::Bold,
                ..Sugar::default()
            },
        ];

        let rio = vec![
            Sugar {
                content: 'r',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: Some([0.0, 0.0, 0.0, 1.0]),
                decoration: SugarDecoration::Underline,
                ..Sugar::default()
            },
            Sugar {
                content: 'e',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([0.0, 0.0, 1.0, 1.0]),
                ..Sugar::default()
            },
            Sugar {
                content: 'g',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: Some([0.0, 0.0, 0.0, 1.0]),
                ..Sugar::default()
            },
            Sugar {
                content: 'u',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([1.0, 1.0, 1.0, 1.0]),
                ..Sugar::default()
            },
            Sugar {
                content: 'l',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([0.0, 1.0, 0.0, 1.0]),
                ..Sugar::default()
            },
            Sugar {
                content: 'a',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([1.0, 1.0, 0.0, 1.0]),
                ..Sugar::default()
            },
            Sugar {
                content: 'r',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([0.0, 1.0, 0.0, 1.0]),
                ..Sugar::default()
            },
        ];

        let strike = vec![
            Sugar {
                content: 's',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: Some([0.0, 0.0, 0.0, 1.0]),
                decoration: SugarDecoration::Underline,
                ..Sugar::default()
            },
            Sugar {
                content: 't',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: Some([0.0, 0.0, 0.0, 1.0]),
                decoration: SugarDecoration::Underline,
                ..Sugar::default()
            },
            Sugar {
                content: 'r',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: Some([0.0, 0.0, 0.0, 1.0]),
                decoration: SugarDecoration::Underline,
                ..Sugar::default()
            },
            Sugar {
                content: 'i',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: Some([0.0, 0.0, 0.0, 1.0]),
                decoration: SugarDecoration::Underline,
                ..Sugar::default()
            },
            Sugar {
                content: 'k',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: Some([0.0, 0.0, 0.0, 1.0]),
                decoration: SugarDecoration::Underline,
                ..Sugar::default()
            },
            Sugar {
                content: 'e',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: Some([0.0, 0.0, 0.0, 1.0]),
                decoration: SugarDecoration::Underline,
                ..Sugar::default()
            },
        ];

        let cursors = vec![
            Sugar {
                content: ' ',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([0.0, 1.0, 1.0, 1.0]),
                cursor: SugarCursor::Block([0.0, 0.0, 1.0, 1.0]),
                ..Sugar::default()
            },
            Sugar {
                content: ' ',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([0.0, 1.0, 1.0, 1.0]),
                ..Sugar::default()
            },
            Sugar {
                content: ' ',
                foreground_color: [1.0, 1.0, 1.0, 1.0],
                background_color: Some([0.0, 0.0, 0.0, 1.0]),
                decoration: SugarDecoration::Underline,
                ..Sugar::default()
            },
            Sugar {
                content: ' ',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([0.0, 1.0, 1.0, 1.0]),
                ..Sugar::default()
            },
            Sugar {
                content: ' ',
                foreground_color: [0.0, 0.0, 0.0, 1.0],
                background_color: Some([0.0, 1.0, 1.0, 1.0]),
                cursor: SugarCursor::Caret([0.0, 0.0, 1.0, 1.0]),
                ..Sugar::default()
            },
        ];

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
                    sugarloaf.insert_on_current_line_from_vec_owned(&sugar);
                    sugarloaf.finish_line();

                    sugarloaf.start_line();
                    sugarloaf.insert_on_current_line_from_vec_owned(&italic_and_bold);
                    sugarloaf.finish_line();

                    sugarloaf.start_line();
                    sugarloaf.insert_on_current_line_from_vec_owned(&rio);
                    sugarloaf.finish_line();

                    sugarloaf.start_line();
                    sugarloaf.insert_on_current_line_from_vec_owned(&strike);
                    sugarloaf.finish_line();

                    sugarloaf.start_line();
                    sugarloaf.insert_on_current_line_from_vec_owned(&cursors);
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
