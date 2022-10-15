mod bar;
mod shared;
mod term;
mod text;
mod window;

use crate::term::Term;
use std::borrow::Cow;
use std::error::Error;
use std::io::BufReader;
use std::io::Read;
use std::sync::Arc;
use std::sync::Mutex;
use tty::{pty, COLS, ROWS};
use winit::{event, event_loop};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let event_loop = event_loop::EventLoop::new();

    let window_builder = window::create_window_builder("Rio");
    let winit_window = window_builder.build(&event_loop).unwrap();

    // todo: read from config
    let shell: String = match std::env::var("SHELL_RIO") {
        Ok(val) => val,
        Err(..) => String::from("bash"),
    };
    let (process, mut w_process, _pid) =
        pty(&Cow::Borrowed(&shell), COLS as u16, ROWS as u16);
    let mut rio: Term = match Term::new(&winit_window).await {
        Ok(term_instance) => term_instance,
        Err(e) => {
            panic!("couldn't create Rio terminal {}", e);
        }
    };

    // ■ ~ ▲
    let output: Arc<Mutex<String>> = Arc::new(Mutex::from(String::from("")));
    let message = Arc::clone(&output);
    tokio::spawn(async move {
        let reader = BufReader::new(process);

        for input_byte in reader.bytes() {
            let bs = shared::utils::convert_to_utf8_string(input_byte.unwrap());
            let mut a = message.lock().unwrap();
            *a = format!("{}{}", *a, bs);
        }
    });

    let mut w_input = window::input::Input::new();

    event_loop.run(move |event, _, control_flow| {
        match event {
            event::Event::WindowEvent {
                event: event::WindowEvent::CloseRequested,
                ..
            } => *control_flow = event_loop::ControlFlow::Exit,

            event::Event::WindowEvent {
                event: event::WindowEvent::ModifiersChanged(modifiers),
                ..
            } => w_input.set_modifiers(modifiers),

            event::Event::WindowEvent {
                event: event::WindowEvent::MouseWheel { delta, .. },
                ..
            } => {
                let mut scroll_y: f64 = 0.0;
                match delta {
                    winit::event::MouseScrollDelta::LineDelta(_x, _y) => {
                        // scroll_y = y;
                    }

                    winit::event::MouseScrollDelta::PixelDelta(pixel_delta) => {
                        scroll_y = pixel_delta.y;
                    }
                }

                if scroll_y != 0.0 {
                    // winit_window.request_redraw();
                }
            }

            event::Event::WindowEvent {
                event:
                    event::WindowEvent::KeyboardInput {
                        input:
                            winit::event::KeyboardInput {
                                virtual_keycode: Some(keycode),
                                state,
                                ..
                            },
                        ..
                    },
                ..
            } => match state {
                winit::event::ElementState::Pressed => {
                    w_input.keydown(keycode, &mut w_process);
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
                if focused {
                    // TODO: Optmize non-focused rendering perf
                }
            }

            event::Event::WindowEvent {
                event: event::WindowEvent::Resized(new_size),
                ..
            } => {
                rio.set_size(new_size);

                rio.draw(&output);
            }
            event::Event::RedrawRequested { .. } => {
                rio.draw(&output);
            }
            _ => {
                *control_flow = event_loop::ControlFlow::Wait;
            }
        }
    })
}
