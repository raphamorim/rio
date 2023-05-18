use crate::event::{EventP, EventProxy, RioEvent, RioEventType};
use crate::ime::Preedit;
use crate::screen::{window::create_window_builder, Screen};
use std::error::Error;
use std::rc::Rc;
use winit::event::TouchPhase;
use winit::event::{Event, Ime, MouseScrollDelta, WindowEvent};
use winit::event_loop::{DeviceEventFilter, EventLoop};
use winit::platform::run_return::EventLoopExtRunReturn;
#[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
use winit::platform::wayland::EventLoopWindowTargetExtWayland;
use winit::window::ImePurpose;

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
        let window_builder =
            create_window_builder("Rio", (self.config.width, self.config.height));
        let winit_window = window_builder.build(&event_loop).unwrap();

        // https://docs.rs/winit/latest/winit/window/enum.ImePurpose.html#variant.Terminal
        winit_window.set_ime_purpose(ImePurpose::Terminal);
        winit_window.set_ime_allowed(true);

        // TODO: Update ime position based on cursor
        // winit_window.set_ime_position(winit::dpi::PhysicalPosition::new(500.0, 500.0));

        // This will ignore diacritical marks and accent characters from
        // being processed as received characters. Instead, the input
        // device's raw character will be placed in event queues with the
        // Alt modifier set.
        #[cfg(target_os = "macos")]
        {
            // The left `Option` key is treated as `Alt`.
            // OnlyLeft,

            // The right `Option` key is treated as `Alt`.
            // OnlyRight,

            // Both `Option` keys are treated as `Alt`.
            // Both,

            // No special handling is applied for `Option` key.
            // None,
            use winit::platform::macos::{OptionAsAlt, WindowExtMacOS};

            match self.config.option_as_alt.to_lowercase().as_str() {
                "both" => winit_window.set_option_as_alt(OptionAsAlt::Both),
                "left" => winit_window.set_option_as_alt(OptionAsAlt::OnlyLeft),
                "right" => winit_window.set_option_as_alt(OptionAsAlt::OnlyRight),
                _ => {}
            }
        }

        let mut screen = Screen::new(&winit_window, &self.config, event_proxy).await?;
        let mut is_focused = false;
        screen.skeleton(self.config.colors.background.1);
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
                                screen.render();
                            }
                            RioEvent::Title(_title) => {
                                // if !self.ctx.preserve_title && self.ctx.config.window.dynamic_title {
                                // self.ctx.window().set_title(title);
                                // }
                            }
                            RioEvent::MouseCursorDirty => {
                                screen.layout().reset_mouse();
                                screen.render();
                            }
                            RioEvent::ClipboardLoad(clipboard_type, format) => {
                                if is_focused {
                                    let text = format(
                                        screen.clipboard_get(clipboard_type).as_str(),
                                    );
                                    screen.messenger.send_bytes(text.into_bytes());
                                }
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
                } => screen.propagate_modifiers_state(modifiers),

                Event::WindowEvent {
                    event: WindowEvent::MouseWheel { delta, phase, .. },
                    ..
                } => {
                    match delta {
                        MouseScrollDelta::LineDelta(_x, _y) => {
                            // scroll_y = y;
                        }

                        MouseScrollDelta::PixelDelta(mut lpos) => {
                            match phase {
                                TouchPhase::Started => {
                                    // Reset offset to zero.
                                    // screen.ctx.mouse_mut().accumulated_scroll = Default::default();
                                }
                                TouchPhase::Moved => {
                                    // When the angle between (x, 0) and (x, y) is lower than ~25 degrees
                                    // (cosine is larger that 0.9) we consider this scrolling as horizontal.
                                    if lpos.x.abs() / lpos.x.hypot(lpos.y) > 0.9 {
                                        lpos.y = 0.;
                                    } else {
                                        lpos.x = 0.;
                                    }

                                    screen.scroll(lpos.x, lpos.y);
                                }
                                _ => (),
                            }
                        }
                    }
                }
                Event::WindowEvent {
                    event: winit::event::WindowEvent::ReceivedCharacter(character),
                    ..
                } => {
                    // if self.ctx.terminal().grid().display_offset() != 0 {
                    //     self.ctx.scroll(Scroll::Bottom);
                    // }
                    // self.ctx.clear_selection();

                    screen.input_character(character);
                }

                Event::WindowEvent {
                    event:
                        winit::event::WindowEvent::KeyboardInput {
                            is_synthetic: false,
                            input:
                                winit::event::KeyboardInput {
                                    virtual_keycode,
                                    scancode,
                                    state,
                                    ..
                                },
                            ..
                        },
                    ..
                } => match state {
                    winit::event::ElementState::Pressed => {
                        screen.input_keycode(virtual_keycode, scancode);
                    }

                    winit::event::ElementState::Released => {
                        // winit_window.request_redraw();
                    }
                },

                Event::WindowEvent {
                    event: WindowEvent::Ime(ime),
                    ..
                } => match ime {
                    Ime::Commit(text) => {
                        screen.paste(&text, true);
                    }
                    Ime::Preedit(text, cursor_offset) => {
                        let preedit = if text.is_empty() {
                            None
                        } else {
                            Some(Preedit::new(text, cursor_offset.map(|offset| offset.0)))
                        };

                        if screen.ime.preedit() != preedit.as_ref() {
                            screen.ime.set_preedit(preedit);
                            screen.render();
                        }
                    }
                    Ime::Enabled => {
                        screen.ime.set_enabled(true);
                    }
                    Ime::Disabled => {
                        screen.ime.set_enabled(false);
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
                    if new_size.width == 0 || new_size.height == 0 {
                        return;
                    }

                    screen.resize(new_size).render();
                }

                Event::WindowEvent {
                    event:
                        winit::event::WindowEvent::ScaleFactorChanged {
                            new_inner_size,
                            scale_factor,
                        },
                    ..
                } => {
                    screen
                        .set_scale(scale_factor as f32, *new_inner_size)
                        .render();
                }

                Event::MainEventsCleared { .. } => {}
                Event::RedrawRequested { .. } => {}
                _ => {
                    *control_flow = winit::event_loop::ControlFlow::Wait;
                }
            }
        });

        Ok(())
    }
}
