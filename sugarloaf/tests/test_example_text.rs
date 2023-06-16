#![cfg(target_arch = "wasm32")]

extern crate wasm_bindgen_test;
use sugarloaf::tools::create_html_canvas;
use wasm_bindgen_test::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use sugarloaf::core::Sugar;
use sugarloaf::core::SugarloafStyle;
use sugarloaf::Sugarloaf;
use wasm_bindgen::prelude::*;

use winit::{
    event::*,
    event_loop::{ControlFlow, EventLoop},
};

wasm_bindgen_test_configure!(run_in_browser);

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use web_sys::HtmlCanvasElement;
#[cfg(target_arch = "wasm32")]
use winit::platform::web::WindowBuilderExtWebSys;

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

async fn run() {
    let event_loop = EventLoop::new();
    let width = 1000.0;
    let height = 800.0;

    #[cfg(target_arch = "wasm32")]
    let canvas_element = {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));

        web_sys::window()
            .and_then(|win| win.document())
            .and_then(|doc| doc.get_element_by_id("sugarloaf-canvas"))
            .and_then(|element| element.dyn_into::<HtmlCanvasElement>().ok())
            .expect("Get canvas element")
    };

    #[cfg(target_arch = "wasm32")]
    let window = winit::window::WindowBuilder::new()
        .with_title("sugarloaf-wasm")
        .with_inner_size(winit::dpi::LogicalSize::new(width, height))
        .with_resizable(false)
        .with_canvas(Some(canvas_element))
        .build(&event_loop)
        .unwrap();

    #[cfg(not(target_arch = "wasm32"))]
    let window = winit::window::Window::new(&event_loop).unwrap();

    let mut sugarloaf = Sugarloaf::new(
        &window,
        wgpu::PowerPreference::HighPerformance,
        sugarloaf::font::constants::DEFAULT_FONT_NAME.to_string(),
    )
    .await
    .expect("Sugarloaf instance should be created");

    let scale_factor = sugarloaf.get_scale();
    let font_size = 180.;
    let mut styles = compute_styles(scale_factor, font_size, width, height);

    event_loop.run(move |event, _, control_flow| {
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
            // Font Regular
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
                sugarloaf.render_with_style(wgpu::Color::RED, styles);
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
                *control_flow = ControlFlow::Exit;
            }
        }
    });
}

#[wasm_bindgen_test]
async fn pass() {
    create_html_canvas();

    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init().expect("could not initialize logger");
    wasm_bindgen_futures::spawn_local(run());

    assert_eq!(1 + 1, 2);
}
