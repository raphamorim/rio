use crate::term::Term;
use crate::Event;

use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::error::Error;
use std::fmt::Write;
use std::fmt::{self, Debug, Formatter};
use std::fs::File;
use std::io::{self, ErrorKind};
use std::io::{BufReader, Read};
use std::marker::Send;
use std::rc::Rc;
use crate::scheduler::Scheduler;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;
use teletypewriter::Pty;
use winit::event::Event::WindowEvent;
use winit::event_loop::{
    ControlFlow, DeviceEventFilter, EventLoop, EventLoopProxy, EventLoopWindowTarget,
};
use winit::platform::run_return::EventLoopExtRunReturn;
// https://vt100.net/emu/dec_ansi_parser
// use mio::net::UnixStream;
use mio::{self, Events};
use mio_extras::channel::{self, Receiver, Sender};
pub struct Sequencer {
    // term: Term,
    config: Rc<config::Config>,
}

impl Sequencer {
    /// Create a new event processor.
    ///
    /// Takes a writer which is expected to be hooked up to the write end of a PTY.
    pub fn new(config: config::Config) -> Sequencer {
        Sequencer {
            config: Rc::new(config),
        }
    }

    pub async fn run(
        &mut self,
        mut event_loop: EventLoop<Event>,
    ) -> Result<(), Box<dyn Error>> {
        let proxy = event_loop.create_proxy();
        let mut scheduler = Scheduler::new(proxy.clone());
        let window_builder = crate::window::create_window_builder(
            "Rio",
            (self.config.width, self.config.height),
        );
        let winit_window = window_builder.build(&event_loop).unwrap();
        let mut term = Term::new(&winit_window, &self.config).await?;

        event_loop.set_device_event_filter(DeviceEventFilter::Always);
        event_loop.run_return(move |event, _, control_flow| {
            // if Self::skip_event(&event) {
            //     return;
            // }

            match event {
                winit::event::Event::Resumed => {
                    term.configure();
                    // Should render once the loop is resumed for first time
                    // Then wait for instructions or user inputs
                    term.render(self.config.colors.background.1);
                }

                winit::event::Event::WindowEvent {
                    event: winit::event::WindowEvent::CloseRequested,
                    ..
                } => *control_flow = winit::event_loop::ControlFlow::Exit,

                // winit::event::Event::WindowEvent {
                //     event: winit::event::WindowEvent::ModifiersChanged(modifiers),
                //     ..
                // } => input_stream.set_modifiers(modifiers),

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
                winit::event::Event::WindowEvent {
                    event: winit::event::WindowEvent::ReceivedCharacter(character),
                    ..
                } => {
                    // println!("character: {:?}", character);
                    // input_stream.input_character(character, &mut rio.write_process);
                }

                winit::event::Event::WindowEvent {
                    event:
                        winit::event::WindowEvent::KeyboardInput {
                            input:
                                winit::event::KeyboardInput {
                                    // semantic meaning of the key
                                    virtual_keycode,
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
                        // println!("{:?} {:?}", scancode, Some(virtual_keycode));
                        // input_stream.keydown(
                        //     scancode,
                        //     virtual_keycode,
                        //     &mut rio.write_process,
                        // );
                        // winit_window.request_redraw();
                    }

                    winit::event::ElementState::Released => {
                        // winit_window.request_redraw();
                    }
                },

                winit::event::Event::WindowEvent {
                    event: winit::event::WindowEvent::Focused(focused),
                    ..
                } => {
                    // is_focused = focused;
                }

                winit::event::Event::WindowEvent {
                    event: winit::event::WindowEvent::Resized(new_size),
                    ..
                } => {
                    // rio.set_size(new_size);
                    term.resize(new_size);
                    // winit_window.request_redraw();
                }

                winit::event::Event::WindowEvent {
                    event:
                        winit::event::WindowEvent::ScaleFactorChanged {
                            new_inner_size,
                            scale_factor,
                        },
                    ..
                } => {
                    let scale_factor_f32 = scale_factor as f32;
                    // if rio.get_scale() != scale_factor_f32 {
                    // rio.set_scale(scale_factor_f32, *new_inner_size);
                    // }
                }

                winit::event::Event::MainEventsCleared { .. } => {
                    // winit_window.request_redraw();
                }

                winit::event::Event::RedrawRequested { .. } => {
                    // if rio.renderer.config.advanced.disable_render_when_unfocused
                    //     && is_focused
                    // {
                    //     return;
                    // }
                    // term.render(self.config.colors.background.1);
                }
                _ => {
                    // let next_frame_time =
                    // std::time::Instant::now() + std::time::Duration::from_nanos(500_000);
                    // *control_flow = winit::event_loop::ControlFlow::WaitUntil(next_frame_time);
                    // *control_flow = event_loop::ControlFlow::Wait;
                    return;
                }
            }
        });

        // if exit_code == 0 {
        Ok(())
        // } else {
        //     Err(format!("Event loop terminated with code: {}", exit_code).into())
        // }
    }
}

