use crate::clipboard::ClipboardType;
use crate::event::{ClickState, EventP, EventProxy, RioEvent, RioEventType};
use crate::ime::Preedit;
use crate::scheduler::{Scheduler, TimerId, Topic};
use crate::screen::{window::create_window_builder, Screen};
use colors::ColorRgb;
use std::error::Error;
use std::os::raw::c_void;
use std::rc::Rc;
use std::time::{Duration, Instant};
use winit::event::{
    ElementState, Event, Ime, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent,
};
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
        let mut scheduler = Scheduler::new(proxy);
        let window_builder =
            create_window_builder("Rio", (self.config.width, self.config.height));
        let winit_window = window_builder.build(&event_loop).unwrap();

        let current_mouse_cursor = winit::window::CursorIcon::Text;
        winit_window.set_cursor_icon(current_mouse_cursor);

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
            // OnlyLeft - The left `Option` key is treated as `Alt`.
            // OnlyRight - The right `Option` key is treated as `Alt`.
            // Both - Both `Option` keys are treated as `Alt`.
            // None - No special handling is applied for `Option` key.
            use winit::platform::macos::{OptionAsAlt, WindowExtMacOS};

            match self.config.option_as_alt.to_lowercase().as_str() {
                "both" => winit_window.set_option_as_alt(OptionAsAlt::Both),
                "left" => winit_window.set_option_as_alt(OptionAsAlt::OnlyLeft),
                "right" => winit_window.set_option_as_alt(OptionAsAlt::OnlyRight),
                _ => {}
            }
        }

        #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
        let display: Option<*mut c_void> = event_loop.wayland_display();
        #[cfg(any(not(feature = "wayland"), target_os = "macos", windows))]
        let display: Option<*mut c_void> = Option::None;

        let mut screen =
            Screen::new(&winit_window, &self.config, event_proxy, display).await?;
        let mut is_window_focused = false;
        let mut should_render = false;
        screen.init(self.config.colors.background.1);
        event_loop.set_device_event_filter(DeviceEventFilter::Always);
        event_loop.run_return(move |event, _, control_flow| {
            match event {
                Event::UserEvent(EventP { payload, .. }) => {
                    if let RioEventType::Rio(event) = payload {
                        match event {
                            RioEvent::Wakeup => {
                                should_render = true;
                            }
                            RioEvent::Render => {
                                if self.config.advanced.disable_render_when_unfocused
                                    && is_window_focused
                                {
                                    return;
                                }
                                screen.render();
                            }
                            RioEvent::Exit => {
                                if !screen.try_close_existent_tab() {
                                    *control_flow = winit::event_loop::ControlFlow::Exit;
                                }
                            }
                            RioEvent::PrepareRender(millis) => {
                                let timer_id = TimerId::new(Topic::Frame, 0);
                                let event =
                                    EventP::new(RioEventType::Rio(RioEvent::Render));

                                if !scheduler.scheduled(timer_id) {
                                    scheduler.schedule(
                                        event,
                                        Duration::from_millis(millis),
                                        false,
                                        timer_id,
                                    );
                                }
                            }
                            RioEvent::Title(_title) => {
                                // if !self.ctx.preserve_title && self.ctx.config.window.dynamic_title {
                                // self.ctx.window().set_title(title);
                                // }
                            }
                            RioEvent::MouseCursorDirty => {
                                screen.layout_mut().reset_mouse();
                            }
                            RioEvent::ClipboardLoad(clipboard_type, format) => {
                                if is_window_focused {
                                    let text = format(
                                        screen.clipboard_get(clipboard_type).as_str(),
                                    );
                                    screen
                                        .ctx_mut()
                                        .current_mut()
                                        .messenger
                                        .send_bytes(text.into_bytes());
                                }
                            }
                            RioEvent::ColorRequest(index, format) => {
                                // TODO: colors could be coming terminal as well
                                // if colors has been declaratively changed
                                // Rio doesn't cover this case yet.
                                //
                                // In the future should try first get
                                // from Crosswords then state colors
                                // screen.colors()[index] or screen.state.colors[index]
                                let color = screen.state.colors[index];
                                let rgb = ColorRgb::from_color_arr(color);
                                screen
                                    .ctx_mut()
                                    .current_mut()
                                    .messenger
                                    .send_bytes(format(rgb).into_bytes());
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
                } => {
                    *control_flow = winit::event_loop::ControlFlow::Exit;
                }

                Event::WindowEvent {
                    event: winit::event::WindowEvent::ModifiersChanged(modifiers),
                    ..
                } => screen.set_modifiers(modifiers),

                Event::WindowEvent {
                    event: WindowEvent::MouseInput { state, button, .. },
                    ..
                } => {
                    winit_window.set_cursor_visible(true);

                    match button {
                        MouseButton::Left => {
                            screen.layout_mut().mouse_mut().left_button_state = state
                        }
                        MouseButton::Middle => {
                            screen.layout_mut().mouse_mut().middle_button_state = state
                        }
                        MouseButton::Right => {
                            screen.layout_mut().mouse_mut().right_button_state = state
                        }
                        _ => (),
                    }

                    match state {
                        ElementState::Pressed => {
                            // Process mouse press before bindings to update the `click_state`.
                            if !screen.modifiers.shift() && screen.mouse_mode() {
                                screen.layout_mut().mouse_mut().click_state =
                                    ClickState::None;

                                // let code = match button {
                                //     MouseButton::Left => 0,
                                //     MouseButton::Middle => 1,
                                //     MouseButton::Right => 2,
                                //     // Can't properly report more than three buttons..
                                //     MouseButton::Other(_) => return,
                                // };

                                // self.mouse_report(code, ElementState::Pressed);
                            } else {
                                // Calculate time since the last click to handle double/triple clicks.
                                let now = Instant::now();
                                let elapsed =
                                    now - screen.layout().mouse.last_click_timestamp;
                                screen.layout_mut().mouse_mut().last_click_timestamp =
                                    now;

                                let threshold = Duration::from_millis(300);
                                let mouse = &screen.layout().mouse;
                                screen.layout_mut().mouse_mut().click_state = match mouse
                                    .click_state
                                {
                                    // Reset click state if button has changed.
                                    _ if button != mouse.last_click_button => {
                                        screen
                                            .layout_mut()
                                            .mouse_mut()
                                            .last_click_button = button;
                                        ClickState::Click
                                    }
                                    ClickState::Click if elapsed < threshold => {
                                        ClickState::DoubleClick
                                    }
                                    ClickState::DoubleClick if elapsed < threshold => {
                                        ClickState::TripleClick
                                    }
                                    _ => ClickState::Click,
                                };

                                // Load mouse point, treating message bar and padding as the closest square.
                                let display_offset = screen.display_offset();

                                if let MouseButton::Left = button {
                                    let point =
                                        screen.layout().mouse_position(display_offset);
                                    screen.on_left_click(point);
                                }

                                should_render = true;
                            }
                            // screen.process_mouse_bindings(button);
                        }
                        ElementState::Released => {
                            if !screen.modifiers.shift() && screen.mouse_mode() {
                                // let code = match button {
                                //     MouseButton::Left => 0,
                                //     MouseButton::Middle => 1,
                                //     MouseButton::Right => 2,
                                //     // Can't properly report more than three buttons.
                                //     MouseButton::Other(_) => return,
                                // };
                                // self.mouse_report(code, ElementState::Released);
                                return;
                            }

                            if let MouseButton::Left | MouseButton::Right = button {
                                // Copy selection on release, to prevent flooding the display server.
                                screen.copy_selection(ClipboardType::Selection);
                            }
                        }
                    }
                }

                Event::WindowEvent {
                    event: WindowEvent::CursorMoved { position, .. },
                    ..
                } => {
                    winit_window.set_cursor_visible(true);
                    let x = position.x;
                    let y = position.y;

                    let lmb_pressed =
                        screen.layout().mouse.left_button_state == ElementState::Pressed;
                    let rmb_pressed =
                        screen.layout().mouse.right_button_state == ElementState::Pressed;

                    // if !screen.selection_is_empty() && (lmb_pressed || rmb_pressed) {
                    // screen.update_selection_scrolling(y);
                    // }

                    let display_offset = screen.display_offset();
                    let old_point = screen.layout().mouse_position(display_offset);

                    let x = x.clamp(0.0, screen.layout().width.into()) as usize;
                    let y = y.clamp(0.0, screen.layout().height.into()) as usize;
                    screen.layout_mut().mouse_mut().x = x;
                    screen.layout_mut().mouse_mut().y = y;

                    let point = screen.layout().mouse_position(display_offset);
                    let square_changed = old_point != point;

                    // If the mouse hasn't changed cells, do nothing.
                    if !square_changed
                    // && screen.layout().mouse.square_side == square_side
                    // && screen.layout().mouse.inside_text_area == inside_text_area
                    {
                        return;
                    }

                    // screen.layout().mouse_mut().inside_text_area = inside_text_area;
                    // let square_side = self.square_side(x);
                    // let square_side = Side::Left;
                    // screen.layout().mouse_mut().square_side = square_side;

                    // Update mouse state and check for URL change.
                    // let mouse_state = self.cursor_state();
                    // winit_window.set_mouse_cursor(mouse_state);

                    if (lmb_pressed || rmb_pressed)
                        && (screen.modifiers.shift() || !screen.mouse_mode())
                    {
                        screen.update_selection(point);
                        should_render = true;
                    }
                    // else if square_changed
                    //     && screen.terminal().mode().intersects(TermMode::MOUSE_MOTION | TermMode::MOUSE_DRAG)
                    // {
                    //     // if lmb_pressed {
                    //         // self.mouse_report(32, ElementState::Pressed);
                    //     // } else if self.ctx.mouse().middle_button_state == ElementState::Pressed {
                    //         // self.mouse_report(33, ElementState::Pressed);
                    //     // } else if self.ctx.mouse().right_button_state == ElementState::Pressed {
                    //         // self.mouse_report(34, ElementState::Pressed);
                    //     // } else if self.ctx.terminal().mode().contains(TermMode::MOUSE_MOTION) {
                    //         // self.mouse_report(35, ElementState::Pressed);
                    //     // }
                    // }
                }

                Event::WindowEvent {
                    event: WindowEvent::MouseWheel { delta, phase, .. },
                    ..
                } => {
                    winit_window.set_cursor_visible(true);
                    match delta {
                        MouseScrollDelta::LineDelta(_x, _y) => {
                            // scroll_y = y;
                        }

                        MouseScrollDelta::PixelDelta(mut lpos) => {
                            match phase {
                                TouchPhase::Started => {
                                    // Reset offset to zero.
                                    screen.layout_mut().mouse_mut().accumulated_scroll =
                                        Default::default();
                                }
                                TouchPhase::Moved => {
                                    // When the angle between (x, 0) and (x, y) is lower than ~25 degrees
                                    // (cosine is larger that 0.9) we consider this scrolling as horizontal.
                                    if lpos.x.abs() / lpos.x.hypot(lpos.y) > 0.9 {
                                        lpos.y = 0.;
                                    } else {
                                        lpos.x = 0.;
                                    }

                                    // TODO: Scroll shouldn't clear selection
                                    // Should implement update_selection_scrolling later
                                    screen.clear_selection();
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
                    screen.scroll_bottom_when_cursor_not_visible();
                    screen.clear_selection();
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
                    ElementState::Pressed => {
                        winit_window.set_cursor_visible(false);
                        screen.input_keycode(virtual_keycode, scancode);
                    }

                    ElementState::Released => {
                        should_render = true;
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
                    is_window_focused = focused;
                }

                Event::WindowEvent {
                    event: winit::event::WindowEvent::DroppedFile(path),
                    ..
                } => {
                    let path: String = path.to_string_lossy().into();
                    screen.paste(&(path + " "), true);
                }

                Event::WindowEvent {
                    event: winit::event::WindowEvent::Resized(new_size),
                    ..
                } => {
                    if new_size.width == 0 || new_size.height == 0 {
                        return;
                    }

                    screen.resize(new_size);
                    should_render = true;
                }

                Event::WindowEvent {
                    event:
                        winit::event::WindowEvent::ScaleFactorChanged {
                            new_inner_size,
                            scale_factor,
                        },
                    ..
                } => {
                    screen.set_scale(scale_factor as f32, *new_inner_size);
                    should_render = true;
                }

                // Emitted when the event loop is being shut down.
                // This is irreversible - if this event is emitted, it is guaranteed to be the last event that gets emitted.
                // You generally want to treat this as an “do on quit” event.
                Event::LoopDestroyed { .. } => {
                    // TODO: Now we are forcing an exit operation
                    // but it should be revaluated since CloseRequested in MacOs
                    // not necessarily exit the process
                    std::process::exit(0);
                }
                Event::MainEventsCleared { .. } => {
                    if should_render {
                        screen.render();
                        should_render = false;
                        return;
                    }

                    scheduler.update();
                }
                Event::RedrawRequested { .. } => {}
                _ => {
                    *control_flow = winit::event_loop::ControlFlow::Wait;
                }
            }
        });

        Ok(())
    }
}
