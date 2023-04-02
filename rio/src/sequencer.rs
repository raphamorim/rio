use crate::event::EventP;
use crate::event::EventProxy;
use crate::term::Term;
use std::error::Error;
use std::rc::Rc;
use winit::event_loop::{DeviceEventFilter, EventLoop};
use winit::platform::run_return::EventLoopExtRunReturn;

pub struct Sequencer {
    config: Rc<config::Config>,
}

impl Sequencer {
    pub fn new(config: config::Config) -> Sequencer {
        Sequencer {
            config: Rc::new(config),
        }
    }

    pub async fn run(
        &mut self,
        mut event_loop: EventLoop<EventP>,
    ) -> Result<(), Box<dyn Error>> {
        let proxy = event_loop.create_proxy();
        let event_proxy = EventProxy::new(proxy.clone());
        let window_builder = crate::window::create_window_builder(
            "Rio",
            (self.config.width, self.config.height),
        );
        let winit_window = window_builder.build(&event_loop).unwrap();
        let mut term = Term::new(&winit_window, &self.config, event_proxy).await?;
        let mut is_focused = false;
        term.skeleton(self.config.colors.background.1);
        event_loop.set_device_event_filter(DeviceEventFilter::Always);
        event_loop.run_return(move |event, _, control_flow| {
            // if Self::skip_event(&event) {
            //     return;
            // }

            match event {
                winit::event::Event::UserEvent(EventP { payload, .. }) => {
                    if let crate::event::RioEventType::Rio(event) = payload {
                        match event {
                            crate::event::RioEvent::Wakeup => {
                                if self.config.advanced.disable_render_when_unfocused
                                    && is_focused
                                {
                                    return;
                                }
                                term.render(self.config.colors.background.1);
                            }
                            crate::event::RioEvent::Title(_title) => {
                                // if !self.ctx.preserve_title && self.ctx.config.window.dynamic_title {
                                // self.ctx.window().set_title(title);
                                // }
                            }
                            _ => {}
                        }
                    }
                }
                winit::event::Event::Resumed => {
                    // Should render once the loop is resumed for first time
                    // Then wait for instructions or user inputs
                }

                winit::event::Event::WindowEvent {
                    event: winit::event::WindowEvent::CloseRequested,
                    ..
                } => *control_flow = winit::event_loop::ControlFlow::Exit,

                winit::event::Event::WindowEvent {
                    event: winit::event::WindowEvent::ModifiersChanged(modifiers),
                    ..
                } => term.propagate_modifiers_state(modifiers),

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
                    term.input_char(character);
                }

                winit::event::Event::WindowEvent {
                    event:
                        winit::event::WindowEvent::KeyboardInput {
                            input:
                                winit::event::KeyboardInput {
                                    virtual_keycode,
                                    // scancode,
                                    state,
                                    ..
                                },
                            ..
                        },
                    ..
                } => match state {
                    winit::event::ElementState::Pressed => {
                        term.input_keycode(virtual_keycode);
                    }

                    winit::event::ElementState::Released => {
                        // winit_window.request_redraw();
                    }
                },

                winit::event::Event::WindowEvent {
                    event: winit::event::WindowEvent::Focused(focused),
                    ..
                } => {
                    is_focused = focused;
                }

                winit::event::Event::WindowEvent {
                    event: winit::event::WindowEvent::Resized(new_size),
                    ..
                } => {
                    term.resize(new_size);
                    term.render(self.config.colors.background.1);
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
                    term.set_scale(scale_factor_f32, *new_inner_size);
                }

                winit::event::Event::MainEventsCleared { .. } => {}
                winit::event::Event::RedrawRequested { .. } => {}
                _ => {
                    // let next_frame_time =
                    // std::time::Instant::now() + std::time::Duration::from_nanos(500_000);
                    // *control_flow = winit::event_loop::ControlFlow::WaitUntil(next_frame_time);
                    *control_flow = winit::event_loop::ControlFlow::Wait;
                    // return;
                }
            }
        });

        Ok(())
    }
}
