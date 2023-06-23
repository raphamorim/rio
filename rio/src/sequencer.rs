#[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
use {
    wayland_client::protocol::wl_surface::WlSurface,
    wayland_client::{Display as WaylandDisplay, Proxy},
    winit::platform::wayland::{EventLoopWindowTargetExtWayland, WindowExtWayland},
};

use crate::clipboard::ClipboardType;
use crate::event::{ClickState, EventP, EventProxy, RioEvent, RioEventType};
use crate::ime::Preedit;
use crate::scheduler::{Scheduler, TimerId, Topic};
use crate::screen::{window::create_window_builder, Screen};
use crate::utils::watch::watch;
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
use winit::window::{CursorIcon, ImePurpose};

pub struct Sequencer {
    config: Rc<config::Config>,
    is_window_focused: bool,
    has_render_updates: bool,
    is_occluded: bool,
    #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
    has_wayland_forcefully_reloaded: bool,
}

impl Sequencer {
    pub fn new(config: config::Config) -> Sequencer {
        Sequencer {
            config: Rc::new(config),
            is_window_focused: false,
            has_render_updates: false,
            is_occluded: false,
            #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
            has_wayland_forcefully_reloaded: false,
        }
    }

    pub async fn run(
        &mut self,
        mut event_loop: EventLoop<EventP>,
        command: Vec<String>,
    ) -> Result<(), Box<dyn Error>> {
        #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
        let mut wayland_event_queue = event_loop.wayland_display().map(|display| {
            let display = unsafe { WaylandDisplay::from_external_display(display as _) };
            display.create_event_queue()
        });

        let proxy = event_loop.create_proxy();
        let event_proxy = EventProxy::new(proxy.clone());
        let event_proxy_clone = event_proxy.clone();
        let mut scheduler = Scheduler::new(proxy);
        let window_builder = create_window_builder("Rio");
        let winit_window = window_builder.build(&event_loop).unwrap();

        let current_mouse_cursor = CursorIcon::Text;
        winit_window.set_cursor_icon(current_mouse_cursor);

        // https://docs.rs/winit/latest/winit;/window/enum.ImePurpose.html#variant.Terminal
        winit_window.set_ime_purpose(ImePurpose::Terminal);
        winit_window.set_ime_allowed(true);

        winit_window.set_transparent(self.config.window_opacity < 1.);

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
        let _wayland_surface = if event_loop.is_wayland() {
            // Attach surface to Rio internal wayland queue to handle frame callbacks.
            let surface = winit_window.wayland_surface().unwrap();
            let proxy: Proxy<WlSurface> = unsafe { Proxy::from_c_ptr(surface as _) };
            Some(proxy.attach(wayland_event_queue.as_ref().unwrap().token()))
        } else {
            None
        };

        #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
        let display: Option<*mut c_void> = event_loop.wayland_display();
        #[cfg(any(not(feature = "wayland"), target_os = "macos", windows))]
        let display: Option<*mut c_void> = Option::None;

        let _ = watch(config::config_dir_path(), event_proxy_clone);
        let mut screen =
            Screen::new(&winit_window, &self.config, event_proxy, display, command)
                .await?;

        screen.init(self.config.colors.background.1);
        event_loop.set_device_event_filter(DeviceEventFilter::Always);
        event_loop.run_return(move |event, _, control_flow| {
            match event {
                Event::UserEvent(EventP { payload, .. }) => {
                    if let RioEventType::Rio(event) = payload {
                        match event {
                            RioEvent::Wakeup => {
                                self.has_render_updates = true;
                            }
                            RioEvent::Render => {
                                if self.config.advanced.disable_render_when_unfocused
                                    && self.is_window_focused
                                {
                                    return;
                                }
                                screen.render();
                            }
                            RioEvent::UpdateConfig => {
                                let config = config::Config::load();
                                self.config = config.into();
                                screen.update_config(&self.config);
                                self.has_render_updates = true;
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
                                screen.reset_mouse();
                            }
                            RioEvent::Scroll(scroll) => {
                                let mut terminal = screen.ctx().current().terminal.lock();
                                terminal.scroll_display(scroll);
                                drop(terminal);
                            }
                            RioEvent::ClipboardLoad(clipboard_type, format) => {
                                if self.is_window_focused {
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
                    // Emitted when the application has been resumed.

                    // This is a hack to avoid an odd scenario in wayland window initialization
                    // wayland windows starts with the wrong width/height.
                    // Rio is ignoring wayland new dimension events, so the terminal
                    // start with the wrong width/height (fix the ignore would be the best fix though)
                    //
                    // The code below forcefully reload dimensions in the terminal initialization
                    // to load current width/height.
                    #[cfg(all(
                        feature = "wayland",
                        not(any(target_os = "macos", windows))
                    ))]
                    {
                        if !self.has_wayland_forcefully_reloaded {
                            screen.update_config(&self.config);
                            self.has_render_updates = true;
                            self.has_wayland_forcefully_reloaded = true;
                        }
                    }
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
                        MouseButton::Left => screen.mouse.left_button_state = state,
                        MouseButton::Middle => screen.mouse.middle_button_state = state,
                        MouseButton::Right => screen.mouse.right_button_state = state,
                        _ => (),
                    }

                    match state {
                        ElementState::Pressed => {
                            // Process mouse press before bindings to update the `click_state`.
                            if !screen.modifiers.shift() && screen.mouse_mode() {
                                screen.mouse.click_state = ClickState::None;

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
                                let elapsed = now - screen.mouse.last_click_timestamp;
                                screen.mouse.last_click_timestamp = now;

                                let threshold = Duration::from_millis(300);
                                let mouse = &screen.mouse;
                                screen.mouse.click_state = match mouse.click_state {
                                    // Reset click state if button has changed.
                                    _ if button != mouse.last_click_button => {
                                        screen.mouse.last_click_button = button;
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
                                    let point = screen.mouse_position(display_offset);
                                    screen.on_left_click(point);
                                }

                                self.has_render_updates = true;
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
                        screen.mouse.left_button_state == ElementState::Pressed;
                    let rmb_pressed =
                        screen.mouse.right_button_state == ElementState::Pressed;

                    if !screen.selection_is_empty() && (lmb_pressed || rmb_pressed) {
                        // screen.update_selection_scrolling(y);
                        // self.has_render_updates = true;
                    }

                    let display_offset = screen.display_offset();
                    let old_point = screen.mouse_position(display_offset);

                    let x = x.clamp(0.0, screen.sugarloaf.layout.width.into()) as usize;
                    let y = y.clamp(0.0, screen.sugarloaf.layout.height.into()) as usize;
                    screen.mouse.x = x;
                    screen.mouse.y = y;

                    let point = screen.mouse_position(display_offset);
                    let square_changed = old_point != point;

                    let inside_text_area = screen.contains_point(x, y);
                    let square_side = screen.side_by_pos(x);

                    // If the mouse hasn't changed cells, do nothing.
                    if !square_changed
                        && screen.mouse.square_side == square_side
                        && screen.mouse.inside_text_area == inside_text_area
                    {
                        return;
                    }

                    screen.mouse.inside_text_area = inside_text_area;
                    screen.mouse.square_side = square_side;

                    let cursor_icon = if !screen.modifiers.shift() && screen.mouse_mode()
                    {
                        CursorIcon::Default
                    } else {
                        CursorIcon::Text
                    };
                    winit_window.set_cursor_icon(cursor_icon);

                    if (lmb_pressed || rmb_pressed)
                        && (screen.modifiers.shift() || !screen.mouse_mode())
                    {
                        screen.update_selection(point, square_side);
                        self.has_render_updates = true;
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
                        MouseScrollDelta::LineDelta(columns, lines) => {
                            let new_scroll_px_x =
                                columns * screen.sugarloaf.layout.font_size;
                            let new_scroll_px_y =
                                lines * screen.sugarloaf.layout.font_size;
                            screen.scroll(new_scroll_px_x as f64, new_scroll_px_y as f64);
                        }
                        MouseScrollDelta::PixelDelta(mut lpos) => {
                            match phase {
                                TouchPhase::Started => {
                                    // Reset offset to zero.
                                    screen.mouse.accumulated_scroll = Default::default();
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
                        self.has_render_updates = true;
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
                    winit_window.set_cursor_visible(true);
                    self.is_window_focused = focused;
                }

                Event::WindowEvent {
                    event: winit::event::WindowEvent::Occluded(occluded),
                    ..
                } => {
                    self.is_occluded = occluded;
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
                    self.has_render_updates = true;
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
                    self.has_render_updates = true;
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
                Event::RedrawEventsCleared { .. } => {
                    // Skip render for macos and x11 windows that are fully occluded
                    if self.is_occluded {
                        return;
                    }

                    #[cfg(all(feature = "wayland", not(any(target_os = "macos", target_os = "windows"))))]
                    if let Some(w_event_queue) = wayland_event_queue.as_mut() {
                        w_event_queue
                            .dispatch_pending(&mut (), |_, _, _| {})
                            .expect("failed to dispatch wayland event queue");
                    }

                    if self.has_render_updates {
                        screen.render();
                        self.has_render_updates = false;
                        return;
                    }

                    scheduler.update();
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
