mod ansi;
mod bar;
mod shared;
mod term;
mod text;
mod window;

use crate::term::Term;
use std::borrow::Cow;
use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;
use tty::{pty, COLS, ROWS};
use winit::{event, event_loop};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let event_loop = event_loop::EventLoop::new();

    let window_builder = window::create_window_builder("Rio");
    let winit_window = window_builder.build(&event_loop).unwrap();

    std::env::set_var("TERM", "xterm-256color");

    // todo: read from config
    let shell: String = match std::env::var("RIO_SHELL") {
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

    let mut input_stream = window::input::Input::new();
    let output: Arc<Mutex<String>> = Arc::new(Mutex::from(String::from("")));

    let message = Arc::clone(&output);
    tokio::spawn(async move {
        crate::ansi::process(process, &message);
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

                // hacky
                if scroll_y < 0.0 {
                    rio.set_text_scroll(0.8_f32);
                    // winit_window.request_redraw();
                }
                if scroll_y > 0.0 {
                    rio.set_text_scroll(-0.8_f32);
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
                    input_stream.keydown(keycode, &mut w_process);
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
