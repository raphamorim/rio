// mod bar;
mod shared;
mod term;
mod window;

use crate::term::Term;
use config::Config;
use std::borrow::Cow;
use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;
use tty::pty;
use winit::{event, event_loop};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let event_loop = event_loop::EventLoop::new();

    let config = Config::load_macos();
    let window_builder =
        window::create_window_builder("Rio", (config.width, config.height));
    let winit_window = window_builder.build(&event_loop).unwrap();

    std::env::set_var("TERM", "xterm-256color");
    let shell: String = match std::env::var("SHELL") {
        Ok(val) => val,
        Err(..) => String::from("bash"),
    };
    let (process, mut w_process, _ptyname, pid) =
        pty(&Cow::Borrowed(&shell), config.columns, config.rows);

    let mut rio: Term = match Term::new(&winit_window, &config, pid).await {
        Ok(term_instance) => term_instance,
        Err(e) => {
            panic!("couldn't create Rio terminal {e}");
        }
    };

    let mut input_stream = window::input::Input::new();
    let output: Arc<Mutex<String>> = Arc::new(Mutex::from(String::from("")));

    let message = Arc::clone(&output);
    tokio::spawn(async move {
        ansi_machine::process(
            process,
            &message,
            config.columns.into(),
            config.rows.into(),
        );
    });

    let mut is_focused = true;
    event_loop.run(move |event, _, control_flow| {
        match event {
            event::Event::WindowEvent {
                event: event::WindowEvent::CloseRequested,
                ..
            } => *control_flow = event_loop::ControlFlow::Exit,

            event::Event::WindowEvent {
                event: event::WindowEvent::ModifiersChanged(modifiers),
                ..
            } => input_stream.set_modifiers(modifiers),

            // event::Event::WindowEvent {
            //     event: event::WindowEvent::MouseWheel { delta, .. },
            //     ..
            // } => {
            //     let mut scroll_y: f64 = 0.0;
            //     match delta {
            //         winit::event::MouseScrollDelta::LineDelta(_x, _y) => {
            //             // scroll_y = y;
            //         }

            //         winit::event::MouseScrollDelta::PixelDelta(pixel_delta) => {
            //             scroll_y = pixel_delta.y;
            //         }
            //     }

            //     // hacky
            //     if scroll_y < 0.0 {
            //         rio.set_text_scroll(-3.0_f32);
            //         // winit_window.request_redraw();
            //     }
            //     if scroll_y > 0.0 {
            //         rio.set_text_scroll(3.0_f32);
            //     }
            // }
            event::Event::WindowEvent {
                event:
                    event::WindowEvent::KeyboardInput {
                        input:
                            winit::event::KeyboardInput {
                                // semantic meaning of the key
                                // virtual_keycode: Some(keycode),
                                // physical key pressed
                                scancode,
                                state,
                                // modifiers,
                                ..
                            },
                        ..
                    },
                ..
            } => match state {
                winit::event::ElementState::Pressed => {
                    // println!("{:?} {:?}", scancode, keycode);
                    input_stream.keydown(scancode, &mut w_process);
                    rio.draw(&output);
                }

                winit::event::ElementState::Released => {
                    rio.draw(&output);
                }
            },

            event::Event::WindowEvent {
                event: event::WindowEvent::Focused(focused),
                ..
            } => {
                is_focused = focused;
            }

            event::Event::WindowEvent {
                event: event::WindowEvent::Resized(new_size),
                ..
            } => {
                rio.set_size(new_size);
                winit_window.request_redraw();
            }

            event::Event::WindowEvent {
                event:
                    event::WindowEvent::ScaleFactorChanged {
                        new_inner_size,
                        scale_factor,
                    },
                ..
            } => {
                let scale_factor_f32 = scale_factor as f32;
                if rio.get_scale() != scale_factor_f32 {
                    rio.set_scale(scale_factor_f32, *new_inner_size);
                }
            }

            event::Event::MainEventsCleared { .. } => {
                winit_window.request_redraw();
            }

            event::Event::RedrawRequested { .. } => {
                if is_focused {
                    rio.draw(&output);
                }
            }
            _ => {
                let next_frame_time =
                    std::time::Instant::now() + std::time::Duration::from_nanos(500_000);
                *control_flow = event_loop::ControlFlow::WaitUntil(next_frame_time);
                // *control_flow = event_loop::ControlFlow::Wait;
            }
        }
    })
}
