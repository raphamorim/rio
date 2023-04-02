use crate::event::{ RioEventType, EventP, EventProxy, RioEvent };
use crate::term::Term;
use std::error::Error;
use std::rc::Rc;
use winit::event::{ Event, MouseScrollDelta, WindowEvent };
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
                Event::UserEvent(EventP { payload, .. }) => {
                    if let RioEventType::Rio(event) = payload {
                        match event {
                            RioEvent::Wakeup => {
                                if self.config.advanced.disable_render_when_unfocused
                                    && is_focused
                                {
                                    return;
                                }
                                term.render(self.config.colors.background.1);
                            }
                            RioEvent::Title(_title) => {
                                // if !self.ctx.preserve_title && self.ctx.config.window.dynamic_title {
                                // self.ctx.window().set_title(title);
                                // }
                            }
                            _ => {}
                        }
                    }
                }
                Event::Resumed => {
                    // Should render once the loop is resumed for first time
                    // Then wait for instructions or user inputs
                }

                Event::WindowEvent {
                    event: winit::event::WindowEvent::CloseRequested,
                    ..
                } => *control_flow = winit::event_loop::ControlFlow::Exit,

                Event::WindowEvent {
                    event: winit::event::WindowEvent::ModifiersChanged(modifiers),
                    ..
                } => term.propagate_modifiers_state(modifiers),

                Event::WindowEvent {
                    event: WindowEvent::MouseWheel { delta, .. },
                    ..
                } => {
                    let mut scroll_y: f64 = 0.0;
                    match delta {
                        MouseScrollDelta::LineDelta(_x, _y) => {
                            // scroll_y = y;
                        }

                        MouseScrollDelta::PixelDelta(mut lpos) => {
                            match delta {
                                // TouchPhase::Started => {
                                //     // Reset offset to zero.
                                //     self.ctx.mouse_mut().accumulated_scroll = Default::default();
                                // },
                                // winit::event::TouchPhase::Moved => {
                                //     // When the angle between (x, 0) and (x, y) is lower than ~25 degrees
                                //     // (cosine is larger that 0.9) we consider this scrolling as horizontal.
                                //     if lpos.x.abs() / lpos.x.hypot(lpos.y) > 0.9 {
                                //         lpos.y = 0.;
                                //     } else {
                                //         lpos.x = 0.;
                                //     }

                                //     self.scroll_terminal(lpos.x, lpos.y);
                                // },
                                _ => (),
                            }
                        },
                    }

                    
                }
                Event::WindowEvent {
                    event: winit::event::WindowEvent::ReceivedCharacter(character),
                    ..
                } => {
                    term.input_char(character);
                }

                Event::WindowEvent {
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

                Event::WindowEvent {
                    event: winit::event::WindowEvent::Focused(focused),
                    ..
                } => {
                    is_focused = focused;
                }

                Event::WindowEvent {
                    event: winit::event::WindowEvent::Resized(new_size),
                    ..
                } => {
                    term.resize(new_size);
                    term.render(self.config.colors.background.1);
                }

                Event::WindowEvent {
                    event:
                        winit::event::WindowEvent::ScaleFactorChanged {
                            new_inner_size,
                            scale_factor,
                        },
                    ..
                } => {
                    let scale_factor_f32 = scale_factor as f32;
                    term.set_scale(scale_factor_f32, *new_inner_size);
                    term.render(self.config.colors.background.1);
                }

                Event::MainEventsCleared { .. } => {}
                Event::RedrawRequested { .. } => {}
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
